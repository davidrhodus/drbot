//! Proxy pattern utilities for drbot.
//!
//! This crate provides:
//! - Proxy trait for transparent wrapping
//! - Virtual proxy (lazy loading)
//! - Protection proxy (access control)
//! - Remote proxy (stub)

use std::sync::Arc;
use thiserror::Error;

/// Proxy error types.
#[derive(Error, Debug)]
pub enum ProxyError {
    #[error("Access denied: {0}")]
    AccessDenied(String),

    #[error("Remote error: {0}")]
    RemoteError(String),

    #[error("Proxy initialization failed")]
    InitFailed,
}

/// Result type for proxy operations.
pub type Result<T> = std::result::Result<T, ProxyError>;

/// Subject trait that proxy and real subject implement.
pub trait Subject: Send + Sync {
    /// The request type.
    type Request;
    /// The response type.
    type Response;

    /// Handle request.
    fn request(&self, req: Self::Request) -> Result<Self::Response>;
}

/// Virtual proxy for lazy initialization.
pub struct VirtualProxy<S: Subject> {
    subject: std::sync::RwLock<Option<S>>,
    creator: Box<dyn Fn() -> S + Send + Sync>,
}

impl<S: Subject> VirtualProxy<S> {
    /// Create new virtual proxy.
    pub fn new<F>(creator: F) -> Self
    where
        F: Fn() -> S + Send + Sync + 'static,
    {
        Self {
            subject: std::sync::RwLock::new(None),
            creator: Box::new(creator),
        }
    }

    /// Check if subject is initialized.
    pub fn is_initialized(&self) -> bool {
        self.subject.read().unwrap().is_some()
    }

    fn ensure_initialized(&self) {
        {
            let read = self.subject.read().unwrap();
            if read.is_some() {
                return;
            }
        }

        let mut write = self.subject.write().unwrap();
        if write.is_none() {
            *write = Some((self.creator)());
        }
    }
}

impl<S: Subject> Subject for VirtualProxy<S> {
    type Request = S::Request;
    type Response = S::Response;

    fn request(&self, req: Self::Request) -> Result<Self::Response> {
        self.ensure_initialized();
        let read = self.subject.read().unwrap();
        read.as_ref().unwrap().request(req)
    }
}

/// Protection proxy for access control.
pub struct ProtectionProxy<S: Subject, A: Fn(&S::Request) -> bool + Send + Sync> {
    subject: S,
    access_check: A,
}

impl<S: Subject, A: Fn(&S::Request) -> bool + Send + Sync> ProtectionProxy<S, A> {
    /// Create new protection proxy.
    pub fn new(subject: S, access_check: A) -> Self {
        Self {
            subject,
            access_check,
        }
    }
}

impl<S: Subject, A: Fn(&S::Request) -> bool + Send + Sync> Subject for ProtectionProxy<S, A> {
    type Request = S::Request;
    type Response = S::Response;

    fn request(&self, req: Self::Request) -> Result<Self::Response> {
        if (self.access_check)(&req) {
            self.subject.request(req)
        } else {
            Err(ProxyError::AccessDenied("Access check failed".to_string()))
        }
    }
}

/// Logging proxy for request/response logging.
pub struct LoggingProxy<S: Subject, L: Fn(&str) + Send + Sync> {
    subject: S,
    logger: L,
    name: String,
}

impl<S: Subject, L: Fn(&str) + Send + Sync> LoggingProxy<S, L> {
    /// Create new logging proxy.
    pub fn new(subject: S, logger: L, name: impl Into<String>) -> Self {
        Self {
            subject,
            logger,
            name: name.into(),
        }
    }
}

impl<S: Subject, L: Fn(&str) + Send + Sync> Subject for LoggingProxy<S, L> {
    type Request = S::Request;
    type Response = S::Response;

    fn request(&self, req: Self::Request) -> Result<Self::Response> {
        (self.logger)(&format!("[{}] Request received", self.name));
        let result = self.subject.request(req);
        match &result {
            Ok(_) => (self.logger)(&format!("[{}] Request succeeded", self.name)),
            Err(e) => (self.logger)(&format!("[{}] Request failed: {}", self.name, e)),
        }
        result
    }
}

/// Caching proxy.
pub struct CachingProxy<S: Subject>
where
    S::Request: Clone + std::hash::Hash + Eq,
    S::Response: Clone,
{
    subject: S,
    cache: std::sync::RwLock<std::collections::HashMap<S::Request, S::Response>>,
}

impl<S: Subject> CachingProxy<S>
where
    S::Request: Clone + std::hash::Hash + Eq,
    S::Response: Clone,
{
    /// Create new caching proxy.
    pub fn new(subject: S) -> Self {
        Self {
            subject,
            cache: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Clear cache.
    pub fn clear(&self) {
        self.cache.write().unwrap().clear();
    }

    /// Get cache size.
    pub fn cache_size(&self) -> usize {
        self.cache.read().unwrap().len()
    }
}

impl<S: Subject> Subject for CachingProxy<S>
where
    S::Request: Clone + std::hash::Hash + Eq + Send + Sync,
    S::Response: Clone + Send + Sync,
{
    type Request = S::Request;
    type Response = S::Response;

    fn request(&self, req: Self::Request) -> Result<Self::Response> {
        // Check cache
        {
            let cache = self.cache.read().unwrap();
            if let Some(response) = cache.get(&req) {
                return Ok(response.clone());
            }
        }

        // Execute and cache
        let response = self.subject.request(req.clone())?;
        {
            let mut cache = self.cache.write().unwrap();
            cache.insert(req, response.clone());
        }
        Ok(response)
    }
}

/// Smart reference proxy.
pub struct SmartProxy<T: Send + Sync> {
    target: Arc<T>,
    access_count: std::sync::atomic::AtomicUsize,
}

impl<T: Send + Sync> SmartProxy<T> {
    /// Create new smart proxy.
    pub fn new(target: T) -> Self {
        Self {
            target: Arc::new(target),
            access_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Get reference to target.
    pub fn get(&self) -> &T {
        self.access_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        &self.target
    }

    /// Get access count.
    pub fn access_count(&self) -> usize {
        self.access_count.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Get reference count.
    pub fn ref_count(&self) -> usize {
        Arc::strong_count(&self.target)
    }
}

impl<T: Send + Sync> Clone for SmartProxy<T> {
    fn clone(&self) -> Self {
        Self {
            target: self.target.clone(),
            access_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SimpleSubject;

    impl Subject for SimpleSubject {
        type Request = i32;
        type Response = i32;

        fn request(&self, req: Self::Request) -> Result<Self::Response> {
            Ok(req * 2)
        }
    }

    #[test]
    fn test_virtual_proxy() {
        let proxy = VirtualProxy::new(|| SimpleSubject);

        assert!(!proxy.is_initialized());
        assert_eq!(proxy.request(21).unwrap(), 42);
        assert!(proxy.is_initialized());
    }

    #[test]
    fn test_protection_proxy() {
        let proxy = ProtectionProxy::new(SimpleSubject, |req: &i32| *req > 0);

        assert!(proxy.request(10).is_ok());
        assert!(proxy.request(-1).is_err());
    }

    #[test]
    fn test_caching_proxy() {
        let proxy = CachingProxy::new(SimpleSubject);

        assert_eq!(proxy.cache_size(), 0);
        assert_eq!(proxy.request(21).unwrap(), 42);
        assert_eq!(proxy.cache_size(), 1);
        assert_eq!(proxy.request(21).unwrap(), 42); // From cache
        assert_eq!(proxy.cache_size(), 1);
    }

    #[test]
    fn test_smart_proxy() {
        let proxy = SmartProxy::new(vec![1, 2, 3]);

        assert_eq!(proxy.access_count(), 0);
        assert_eq!(proxy.get().len(), 3);
        assert_eq!(proxy.access_count(), 1);
        assert_eq!(proxy.get()[0], 1);
        assert_eq!(proxy.access_count(), 2);
    }
}
