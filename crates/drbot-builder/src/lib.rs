//! Builder pattern utilities for drbot.
//!
//! This crate provides:
//! - Builder trait
//! - Validation builders
//! - Builder macros

use thiserror::Error;

/// Builder error types.
#[derive(Error, Debug)]
pub enum BuilderError {
    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid value for {0}: {1}")]
    InvalidValue(String, String),

    #[error("Build failed: {0}")]
    BuildFailed(String),

    #[error("Validation failed: {0}")]
    ValidationFailed(String),
}

/// Result type for builder operations.
pub type Result<T> = std::result::Result<T, BuilderError>;

/// Builder trait for creating objects.
pub trait Builder: Default {
    /// Target type being built.
    type Target;

    /// Build the target object.
    fn build(self) -> Result<Self::Target>;
}

/// Validating builder trait.
pub trait ValidatingBuilder: Builder {
    /// Validate current state.
    fn validate(&self) -> Result<()>;

    /// Build with validation.
    fn build_validated(self) -> Result<Self::Target>
    where
        Self: Sized,
    {
        self.validate()?;
        self.build()
    }
}

/// Field setter trait for fluent builders.
pub trait FieldSetter<T> {
    /// Set field value.
    fn set(self, value: T) -> Self;
}

/// Optional field wrapper for builders.
#[derive(Debug, Clone, Default)]
pub struct BuilderField<T> {
    value: Option<T>,
    required: bool,
    name: &'static str,
}

impl<T> BuilderField<T> {
    /// Create required field.
    pub fn required(name: &'static str) -> Self {
        Self {
            value: None,
            required: true,
            name,
        }
    }

    /// Create optional field.
    pub fn optional(name: &'static str) -> Self {
        Self {
            value: None,
            required: false,
            name,
        }
    }

    /// Create optional field with default.
    pub fn with_default(name: &'static str, default: T) -> Self {
        Self {
            value: Some(default),
            required: false,
            name,
        }
    }

    /// Set value.
    pub fn set(&mut self, value: T) {
        self.value = Some(value);
    }

    /// Get value (consumes).
    pub fn take(self) -> Result<T> {
        match self.value {
            Some(v) => Ok(v),
            None if self.required => Err(BuilderError::MissingField(self.name.to_string())),
            None => Err(BuilderError::MissingField(self.name.to_string())),
        }
    }

    /// Get optional value.
    pub fn take_optional(self) -> Option<T> {
        self.value
    }

    /// Check if set.
    pub fn is_set(&self) -> bool {
        self.value.is_some()
    }

    /// Validate field.
    pub fn validate(&self) -> Result<()> {
        if self.required && self.value.is_none() {
            Err(BuilderError::MissingField(self.name.to_string()))
        } else {
            Ok(())
        }
    }
}

/// Step builder for multi-step construction.
pub struct StepBuilder<S, T> {
    state: S,
    _marker: std::marker::PhantomData<T>,
}

impl<S, T> StepBuilder<S, T> {
    /// Create new step builder.
    pub fn new(state: S) -> Self {
        Self {
            state,
            _marker: std::marker::PhantomData,
        }
    }

    /// Get current state.
    pub fn state(&self) -> &S {
        &self.state
    }

    /// Transition to next step.
    pub fn next<N>(self, next_state: N) -> StepBuilder<N, T> {
        StepBuilder::new(next_state)
    }
}

/// Macro for generating builder structs.
#[macro_export]
macro_rules! define_builder {
    (
        $builder:ident for $target:ident {
            $(
                $field:ident : $field_type:ty $(= $default:expr)?
            ),* $(,)?
        }
    ) => {
        #[derive(Default)]
        pub struct $builder {
            $(
                $field: Option<$field_type>,
            )*
        }

        impl $builder {
            pub fn new() -> Self {
                Self::default()
            }

            $(
                pub fn $field(mut self, value: $field_type) -> Self {
                    self.$field = Some(value);
                    self
                }
            )*
        }
    };
}

/// Typed builder state markers.
pub mod states {
    /// Initial state.
    pub struct Initial;

    /// Configured state.
    pub struct Configured;

    /// Ready to build state.
    pub struct Ready;
}

/// Generic configurable builder.
#[derive(Debug, Clone)]
pub struct ConfigBuilder<T> {
    config: std::collections::HashMap<String, T>,
}

impl<T> ConfigBuilder<T> {
    /// Create new config builder.
    pub fn new() -> Self {
        Self {
            config: std::collections::HashMap::new(),
        }
    }

    /// Set configuration value.
    pub fn set(mut self, key: impl Into<String>, value: T) -> Self {
        self.config.insert(key.into(), value);
        self
    }

    /// Get configuration value.
    pub fn get(&self, key: &str) -> Option<&T> {
        self.config.get(key)
    }

    /// Get all configuration.
    pub fn into_config(self) -> std::collections::HashMap<String, T> {
        self.config
    }
}

impl<T> Default for ConfigBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Example: Simple struct with builder.
#[derive(Debug, Clone)]
pub struct Person {
    pub name: String,
    pub age: u32,
    pub email: Option<String>,
}

/// PersonBuilder example.
#[derive(Default)]
pub struct PersonBuilder {
    name: BuilderField<String>,
    age: BuilderField<u32>,
    email: BuilderField<String>,
}

impl PersonBuilder {
    /// Create new builder.
    pub fn new() -> Self {
        Self {
            name: BuilderField::required("name"),
            age: BuilderField::required("age"),
            email: BuilderField::optional("email"),
        }
    }

    /// Set name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name.set(name.into());
        self
    }

    /// Set age.
    pub fn age(mut self, age: u32) -> Self {
        self.age.set(age);
        self
    }

    /// Set email.
    pub fn email(mut self, email: impl Into<String>) -> Self {
        self.email.set(email.into());
        self
    }
}

impl Builder for PersonBuilder {
    type Target = Person;

    fn build(self) -> Result<Person> {
        Ok(Person {
            name: self.name.take()?,
            age: self.age.take()?,
            email: self.email.take_optional(),
        })
    }
}

impl ValidatingBuilder for PersonBuilder {
    fn validate(&self) -> Result<()> {
        self.name.validate()?;
        self.age.validate()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_person_builder() {
        let person = PersonBuilder::new()
            .name("Alice")
            .age(30)
            .email("alice@example.com")
            .build()
            .unwrap();

        assert_eq!(person.name, "Alice");
        assert_eq!(person.age, 30);
        assert_eq!(person.email, Some("alice@example.com".to_string()));
    }

    #[test]
    fn test_missing_required() {
        let result = PersonBuilder::new().name("Bob").build();

        assert!(result.is_err());
    }

    #[test]
    fn test_optional_field() {
        let person = PersonBuilder::new()
            .name("Charlie")
            .age(25)
            .build()
            .unwrap();

        assert!(person.email.is_none());
    }

    #[test]
    fn test_config_builder() {
        let config = ConfigBuilder::new()
            .set("host", "localhost")
            .set("port", "8080")
            .into_config();

        assert_eq!(config.get("host"), Some(&"localhost"));
        assert_eq!(config.get("port"), Some(&"8080"));
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // BuilderField Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_builder_field_required_unset() {
        let field: BuilderField<i32> = BuilderField::required("test");

        kani::assert(!field.is_set(), "required field starts unset");
        kani::assert(field.validate().is_err(), "validate fails when unset");
    }

    #[kani::proof]
    fn proof_builder_field_required_set() {
        let mut field: BuilderField<i32> = BuilderField::required("test");
        let value: i8 = kani::any();

        field.set(value as i32);

        kani::assert(field.is_set(), "field is set after set()");
        kani::assert(field.validate().is_ok(), "validate succeeds when set");
    }

    #[kani::proof]
    fn proof_builder_field_optional_unset() {
        let field: BuilderField<i32> = BuilderField::optional("test");

        kani::assert(!field.is_set(), "optional field starts unset");
        kani::assert(field.validate().is_ok(), "validate succeeds even unset");
    }

    #[kani::proof]
    fn proof_builder_field_with_default() {
        let field: BuilderField<i32> = BuilderField::with_default("test", 42);

        kani::assert(field.is_set(), "with_default starts set");
        kani::assert(field.validate().is_ok(), "validate succeeds");
    }

    #[kani::proof]
    fn proof_builder_field_take() {
        let mut field: BuilderField<i32> = BuilderField::required("test");
        let value: i8 = kani::any();

        field.set(value as i32);
        let result = field.take();

        kani::assert(result.is_ok(), "take succeeds when set");
        kani::assert(
            result.unwrap() == value as i32,
            "take returns correct value",
        );
    }

    #[kani::proof]
    fn proof_builder_field_take_unset_required() {
        let field: BuilderField<i32> = BuilderField::required("test");
        let result = field.take();

        kani::assert(result.is_err(), "take fails when required and unset");
    }

    #[kani::proof]
    fn proof_builder_field_take_optional() {
        let mut field: BuilderField<i32> = BuilderField::optional("test");
        let value: i8 = kani::any();

        field.set(value as i32);
        let result = field.take_optional();

        kani::assert(result.is_some(), "take_optional returns Some when set");
        kani::assert(
            result.unwrap() == value as i32,
            "take_optional returns correct value",
        );
    }

    #[kani::proof]
    fn proof_builder_field_take_optional_unset() {
        let field: BuilderField<i32> = BuilderField::optional("test");
        let result = field.take_optional();

        kani::assert(result.is_none(), "take_optional returns None when unset");
    }

    // ========================================================================
    // StepBuilder Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_step_builder_new() {
        let builder: StepBuilder<i32, String> = StepBuilder::new(42);

        kani::assert(*builder.state() == 42, "state returns initial value");
    }

    #[kani::proof]
    fn proof_step_builder_next() {
        let builder: StepBuilder<i32, String> = StepBuilder::new(1);
        let next: StepBuilder<&str, String> = builder.next("hello");

        kani::assert(*next.state() == "hello", "next transitions state");
    }

    #[kani::proof]
    fn proof_step_builder_chain() {
        let builder: StepBuilder<i32, ()> = StepBuilder::new(1);
        let step2: StepBuilder<i32, ()> = builder.next(2);
        let step3: StepBuilder<i32, ()> = step2.next(3);

        kani::assert(*step3.state() == 3, "chained transitions work");
    }

    // ========================================================================
    // ConfigBuilder Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_config_builder_new_empty() {
        let builder: ConfigBuilder<i32> = ConfigBuilder::new();
        let config = builder.into_config();

        kani::assert(config.is_empty(), "new config is empty");
    }

    #[kani::proof]
    fn proof_config_builder_default_empty() {
        let builder: ConfigBuilder<i32> = ConfigBuilder::default();
        let config = builder.into_config();

        kani::assert(config.is_empty(), "default config is empty");
    }

    #[kani::proof]
    fn proof_config_builder_set_get() {
        let builder: ConfigBuilder<i32> = ConfigBuilder::new().set("key", 42);

        kani::assert(builder.get("key") == Some(&42), "get returns set value");
        kani::assert(builder.get("other").is_none(), "get returns None for unset");
    }

    #[kani::proof]
    fn proof_config_builder_set_overwrite() {
        let builder: ConfigBuilder<i32> = ConfigBuilder::new().set("key", 1).set("key", 2);

        kani::assert(builder.get("key") == Some(&2), "later set overwrites");
    }

    #[kani::proof]
    fn proof_config_builder_multiple_keys() {
        let builder: ConfigBuilder<i32> = ConfigBuilder::new().set("a", 1).set("b", 2).set("c", 3);

        kani::assert(builder.get("a") == Some(&1), "first key correct");
        kani::assert(builder.get("b") == Some(&2), "second key correct");
        kani::assert(builder.get("c") == Some(&3), "third key correct");
    }

    #[kani::proof]
    fn proof_config_builder_into_config() {
        let builder: ConfigBuilder<i32> = ConfigBuilder::new().set("key", 42);
        let config = builder.into_config();

        kani::assert(config.len() == 1, "config has one entry");
        kani::assert(config.get("key") == Some(&42), "config has correct value");
    }

    // ========================================================================
    // PersonBuilder Proofs (Example Builder)
    // ========================================================================

    #[kani::proof]
    fn proof_person_builder_new() {
        let builder = PersonBuilder::new();

        // Verify fields are initialized correctly
        kani::assert(!builder.name.is_set(), "name starts unset");
        kani::assert(!builder.age.is_set(), "age starts unset");
        kani::assert(!builder.email.is_set(), "email starts unset");
    }

    #[kani::proof]
    fn proof_person_builder_name() {
        let builder = PersonBuilder::new().name("Alice");

        kani::assert(builder.name.is_set(), "name is set after name()");
    }

    #[kani::proof]
    fn proof_person_builder_age() {
        let age: u8 = kani::any();
        let builder = PersonBuilder::new().age(age as u32);

        kani::assert(builder.age.is_set(), "age is set after age()");
    }

    #[kani::proof]
    fn proof_person_builder_email() {
        let builder = PersonBuilder::new().email("test@example.com");

        kani::assert(builder.email.is_set(), "email is set after email()");
    }

    #[kani::proof]
    fn proof_person_builder_build_success() {
        let builder = PersonBuilder::new().name("Alice").age(30);

        let result = builder.build();
        kani::assert(result.is_ok(), "build succeeds with required fields");

        let person = result.unwrap();
        kani::assert(person.name == "Alice", "name is correct");
        kani::assert(person.age == 30, "age is correct");
        kani::assert(person.email.is_none(), "email is None when not set");
    }

    #[kani::proof]
    fn proof_person_builder_build_with_email() {
        let builder = PersonBuilder::new()
            .name("Bob")
            .age(25)
            .email("bob@example.com");

        let result = builder.build();
        kani::assert(result.is_ok(), "build succeeds");

        let person = result.unwrap();
        kani::assert(
            person.email == Some("bob@example.com".to_string()),
            "email is set",
        );
    }

    #[kani::proof]
    fn proof_person_builder_build_missing_name() {
        let builder = PersonBuilder::new().age(30);

        let result = builder.build();
        kani::assert(result.is_err(), "build fails without name");
    }

    #[kani::proof]
    fn proof_person_builder_build_missing_age() {
        let builder = PersonBuilder::new().name("Alice");

        let result = builder.build();
        kani::assert(result.is_err(), "build fails without age");
    }

    #[kani::proof]
    fn proof_person_builder_validate_success() {
        let builder = PersonBuilder::new().name("Alice").age(30);

        let result = builder.validate();
        kani::assert(result.is_ok(), "validate succeeds with required fields");
    }

    #[kani::proof]
    fn proof_person_builder_validate_missing_name() {
        let builder = PersonBuilder::new().age(30);

        let result = builder.validate();
        kani::assert(result.is_err(), "validate fails without name");
    }

    #[kani::proof]
    fn proof_person_builder_validate_missing_age() {
        let builder = PersonBuilder::new().name("Alice");

        let result = builder.validate();
        kani::assert(result.is_err(), "validate fails without age");
    }

    // ========================================================================
    // State Markers Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_state_initial() {
        let _: states::Initial = states::Initial;
    }

    #[kani::proof]
    fn proof_state_configured() {
        let _: states::Configured = states::Configured;
    }

    #[kani::proof]
    fn proof_state_ready() {
        let _: states::Ready = states::Ready;
    }
}
