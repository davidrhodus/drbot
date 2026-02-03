//! Repository pattern utilities for drbot.
//!
//! This crate provides:
//! - Repository trait
//! - In-memory repository
//! - Query specifications

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::RwLock;
use thiserror::Error;

/// Repository error types.
#[derive(Error, Debug)]
pub enum RepositoryError {
    #[error("Entity not found: {0}")]
    NotFound(String),

    #[error("Duplicate entity: {0}")]
    Duplicate(String),

    #[error("Repository error: {0}")]
    Error(String),
}

/// Result type for repository operations.
pub type Result<T> = std::result::Result<T, RepositoryError>;

/// Entity trait for repository items.
pub trait Entity {
    /// ID type.
    type Id: Clone + Eq + Hash + Send + Sync;

    /// Get entity ID.
    fn id(&self) -> &Self::Id;
}

/// Repository trait.
pub trait Repository<T: Entity>: Send + Sync {
    /// Find by ID.
    fn find(&self, id: &T::Id) -> Result<Option<T>>;

    /// Find all.
    fn find_all(&self) -> Result<Vec<T>>;

    /// Save entity.
    fn save(&self, entity: T) -> Result<()>;

    /// Delete by ID.
    fn delete(&self, id: &T::Id) -> Result<bool>;

    /// Check if exists.
    fn exists(&self, id: &T::Id) -> Result<bool> {
        Ok(self.find(id)?.is_some())
    }

    /// Count entities.
    fn count(&self) -> Result<usize> {
        Ok(self.find_all()?.len())
    }
}

/// In-memory repository.
pub struct InMemoryRepository<T: Entity + Clone> {
    data: RwLock<HashMap<T::Id, T>>,
}

impl<T: Entity + Clone> InMemoryRepository<T> {
    /// Create new in-memory repository.
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }

    /// Find by predicate.
    pub fn find_by<F: Fn(&T) -> bool>(&self, predicate: F) -> Result<Vec<T>> {
        let data = self.data.read().unwrap();
        Ok(data.values().filter(|e| predicate(e)).cloned().collect())
    }

    /// Find one by predicate.
    pub fn find_one_by<F: Fn(&T) -> bool>(&self, predicate: F) -> Result<Option<T>> {
        let data = self.data.read().unwrap();
        Ok(data.values().find(|e| predicate(e)).cloned())
    }

    /// Clear all entities.
    pub fn clear(&self) {
        self.data.write().unwrap().clear();
    }
}

impl<T: Entity + Clone> Default for InMemoryRepository<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Entity + Clone + Send + Sync> Repository<T> for InMemoryRepository<T> {
    fn find(&self, id: &T::Id) -> Result<Option<T>> {
        let data = self.data.read().unwrap();
        Ok(data.get(id).cloned())
    }

    fn find_all(&self) -> Result<Vec<T>> {
        let data = self.data.read().unwrap();
        Ok(data.values().cloned().collect())
    }

    fn save(&self, entity: T) -> Result<()> {
        let mut data = self.data.write().unwrap();
        data.insert(entity.id().clone(), entity);
        Ok(())
    }

    fn delete(&self, id: &T::Id) -> Result<bool> {
        let mut data = self.data.write().unwrap();
        Ok(data.remove(id).is_some())
    }
}

/// Query specification for repositories.
pub trait QuerySpec<T>: Send + Sync {
    /// Check if entity matches.
    fn matches(&self, entity: &T) -> bool;
}

/// Function-based query spec.
pub struct FnQuerySpec<T, F: Fn(&T) -> bool + Send + Sync> {
    func: F,
    _marker: std::marker::PhantomData<T>,
}

impl<T, F: Fn(&T) -> bool + Send + Sync> FnQuerySpec<T, F> {
    /// Create new function query spec.
    pub fn new(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T: Send + Sync, F: Fn(&T) -> bool + Send + Sync> QuerySpec<T> for FnQuerySpec<T, F> {
    fn matches(&self, entity: &T) -> bool {
        (self.func)(entity)
    }
}

/// Queryable repository extension.
pub trait QueryableRepository<T: Entity>: Repository<T> {
    /// Find by query specification.
    fn query<Q: QuerySpec<T>>(&self, spec: &Q) -> Result<Vec<T>>;
}

impl<T: Entity + Clone + Send + Sync> QueryableRepository<T> for InMemoryRepository<T> {
    fn query<Q: QuerySpec<T>>(&self, spec: &Q) -> Result<Vec<T>> {
        let data = self.data.read().unwrap();
        Ok(data.values().filter(|e| spec.matches(e)).cloned().collect())
    }
}

/// Cached repository wrapper.
pub struct CachedRepository<T: Entity + Clone, R: Repository<T>> {
    inner: R,
    cache: RwLock<HashMap<T::Id, T>>,
}

impl<T: Entity + Clone, R: Repository<T>> CachedRepository<T, R> {
    /// Create new cached repository.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Invalidate cache entry.
    pub fn invalidate(&self, id: &T::Id) {
        self.cache.write().unwrap().remove(id);
    }

    /// Clear cache.
    pub fn clear_cache(&self) {
        self.cache.write().unwrap().clear();
    }
}

impl<T: Entity + Clone + Send + Sync, R: Repository<T>> Repository<T> for CachedRepository<T, R> {
    fn find(&self, id: &T::Id) -> Result<Option<T>> {
        // Check cache first
        {
            let cache = self.cache.read().unwrap();
            if let Some(entity) = cache.get(id) {
                return Ok(Some(entity.clone()));
            }
        }

        // Fetch from inner and cache
        if let Some(entity) = self.inner.find(id)? {
            let mut cache = self.cache.write().unwrap();
            cache.insert(id.clone(), entity.clone());
            Ok(Some(entity))
        } else {
            Ok(None)
        }
    }

    fn find_all(&self) -> Result<Vec<T>> {
        self.inner.find_all()
    }

    fn save(&self, entity: T) -> Result<()> {
        let id = entity.id().clone();
        self.inner.save(entity.clone())?;
        self.cache.write().unwrap().insert(id, entity);
        Ok(())
    }

    fn delete(&self, id: &T::Id) -> Result<bool> {
        self.cache.write().unwrap().remove(id);
        self.inner.delete(id)
    }
}

/// Unit of work for transactional operations.
pub struct UnitOfWork<T: Entity + Clone> {
    to_save: RwLock<Vec<T>>,
    to_delete: RwLock<Vec<T::Id>>,
}

impl<T: Entity + Clone> UnitOfWork<T> {
    /// Create new unit of work.
    pub fn new() -> Self {
        Self {
            to_save: RwLock::new(Vec::new()),
            to_delete: RwLock::new(Vec::new()),
        }
    }

    /// Register entity to save.
    pub fn register_save(&self, entity: T) {
        self.to_save.write().unwrap().push(entity);
    }

    /// Register entity to delete.
    pub fn register_delete(&self, id: T::Id) {
        self.to_delete.write().unwrap().push(id);
    }

    /// Commit changes to repository.
    pub fn commit<R: Repository<T>>(&self, repo: &R) -> Result<()> {
        for entity in self.to_save.write().unwrap().drain(..) {
            repo.save(entity)?;
        }
        for id in self.to_delete.write().unwrap().drain(..) {
            repo.delete(&id)?;
        }
        Ok(())
    }

    /// Rollback (clear pending operations).
    pub fn rollback(&self) {
        self.to_save.write().unwrap().clear();
        self.to_delete.write().unwrap().clear();
    }
}

impl<T: Entity + Clone> Default for UnitOfWork<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct User {
        id: i32,
        name: String,
    }

    impl Entity for User {
        type Id = i32;

        fn id(&self) -> &Self::Id {
            &self.id
        }
    }

    #[test]
    fn test_in_memory_repository() {
        let repo = InMemoryRepository::new();

        let user = User {
            id: 1,
            name: "Alice".to_string(),
        };

        repo.save(user.clone()).unwrap();

        let found = repo.find(&1).unwrap();
        assert_eq!(found, Some(user));

        assert!(repo.delete(&1).unwrap());
        assert!(repo.find(&1).unwrap().is_none());
    }

    #[test]
    fn test_find_by() {
        let repo = InMemoryRepository::new();

        repo.save(User {
            id: 1,
            name: "Alice".to_string(),
        })
        .unwrap();
        repo.save(User {
            id: 2,
            name: "Bob".to_string(),
        })
        .unwrap();
        repo.save(User {
            id: 3,
            name: "Charlie".to_string(),
        })
        .unwrap();

        let results = repo.find_by(|u| u.name.starts_with('A')).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Alice");
    }

    #[test]
    fn test_unit_of_work() {
        let repo = InMemoryRepository::new();
        let uow = UnitOfWork::new();

        uow.register_save(User {
            id: 1,
            name: "Alice".to_string(),
        });
        uow.register_save(User {
            id: 2,
            name: "Bob".to_string(),
        });

        // Not committed yet
        assert_eq!(repo.count().unwrap(), 0);

        uow.commit(&repo).unwrap();

        assert_eq!(repo.count().unwrap(), 2);
    }
}
