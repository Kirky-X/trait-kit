// Copyright © 2026 Kirky.X. All rights reserved.

//! KitModuleBuilder — build and register module capabilities.

use std::marker::PhantomData;
use std::sync::Arc;

use crate::core::builder::ModuleBuilder;
use crate::core::capability::CapabilityKey;
use crate::core::module::Module;

use crate::kit::error::KitError;
use crate::kit::kit::Kit;

/// Trait for converting a builder into a KitModuleBuilder.
///
/// Implemented automatically for all types that implement `ModuleBuilder<M>`.
pub trait IntoKitModuleBuilder<M: Module> {
    /// The builder type that gets wrapped.
    type Builder: ModuleBuilder<M>;

    /// Attach a Kit reference to the builder.
    ///
    /// Returns a `KitModuleBuilder` that can call `.provide::<K>()`.
    fn kit(self, kit: &Kit) -> KitModuleBuilder<M, Self::Builder>;
}

/// Blanket implementation for all ModuleBuilder types.
impl<M, B> IntoKitModuleBuilder<M> for B
where
    M: Module<Builder = B>,
    B: ModuleBuilder<M>,
{
    type Builder = B;

    fn kit(self, kit: &Kit) -> KitModuleBuilder<M, Self::Builder> {
        KitModuleBuilder::new(self, kit.clone())
    }
}

/// A builder wrapper that can build and register a module's capability.
///
/// Created by calling `.kit(&kit)` on any standard builder.
///
/// Does NOT implement `WithConfig` or `WithRequirements` — those must be
/// called before `.kit()`.
pub struct KitModuleBuilder<M: Module, B> {
    builder: B,
    kit: Kit,
    _phantom: PhantomData<M>,
}

impl<M: Module, B: ModuleBuilder<M>> KitModuleBuilder<M, B> {
    /// Create a new KitModuleBuilder.
    pub fn new(builder: B, kit: Kit) -> Self {
        KitModuleBuilder {
            builder,
            kit,
            _phantom: PhantomData,
        }
    }

    /// Build the module and register its capability.
    ///
    /// Returns the built capability on success.
    /// On failure, returns `KitError::BuildFailed` or `KitError::DuplicateCapability`.
    ///
    /// # Type Constraint
    ///
    /// `M::Capability` must be convertible to `Arc<K::Capability>` for the provided key `K`.
    /// Typically, `M::Capability = Arc<dyn Trait + Send + Sync>` and `K::Capability = dyn Trait`.
    ///
    /// # Failure Behavior
    ///
    /// If build succeeds but registration fails (key already exists),
    /// the built capability is discarded and `DuplicateCapability` is returned.
    pub fn provide<K>(self) -> Result<Arc<K::Capability>, KitError>
    where
        K: CapabilityKey,
        M: Module<Capability = Arc<K::Capability>>,
    {
        let module_name = M::NAME;

        // Build the capability
        let capability: Arc<K::Capability> =
            self.builder.build().map_err(|e| KitError::BuildFailed {
                module: module_name,
                source: Box::new(e),
            })?;

        // Register in Kit
        self.kit.provide::<K>(Arc::clone(&capability))?;

        // Return the registered capability
        Ok(capability)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::builder::ModuleBuilder;
    use crate::core::capability::CapabilityKey;
    use crate::core::module::Module;
    use std::fmt;

    #[derive(Debug)]
    struct TestError(&'static str);

    impl fmt::Display for TestError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl std::error::Error for TestError {}

    struct TestCapKey;
    impl CapabilityKey for TestCapKey {
        type Capability = dyn Send + Sync;
        const NAME: &'static str = "test_cap";
    }

    struct TestModule;
    impl Module for TestModule {
        const NAME: &'static str = "test_module";
        type Config = ();
        type Requirements = ();
        type Capability = Arc<dyn Send + Sync>;
        type Error = TestError;
        type Builder = TestBuilder;
    }

    struct TestBuilder;
    impl ModuleBuilder<TestModule> for TestBuilder {
        fn build(self) -> Result<Arc<dyn Send + Sync>, TestError> {
            Ok(Arc::new(42i32) as Arc<dyn Send + Sync>)
        }
    }

    struct FailingModule;
    impl Module for FailingModule {
        const NAME: &'static str = "failing_module";
        type Config = ();
        type Requirements = ();
        type Capability = Arc<dyn Send + Sync>;
        type Error = TestError;
        type Builder = FailingBuilder;
    }

    struct FailingBuilder;
    impl ModuleBuilder<FailingModule> for FailingBuilder {
        fn build(self) -> Result<Arc<dyn Send + Sync>, TestError> {
            Err(TestError("build failed"))
        }
    }

    #[test]
    fn kit_creates_kit_module_builder() {
        let kit = Kit::new();
        let kmb = TestBuilder.kit(&kit);
        // Verify it is a KitModuleBuilder by calling provide
        let result = kmb.provide::<TestCapKey>();
        assert!(result.is_ok());
        assert!(kit.contains::<TestCapKey>());
    }

    #[test]
    fn provide_build_failure() {
        let kit = Kit::new();
        let result = FailingBuilder.kit(&kit).provide::<TestCapKey>();
        assert!(result.is_err());
        if let Err(e) = &result {
            assert!(e.to_string().contains("failed to build module"));
            assert!(e.to_string().contains("failing_module"));
        } else {
            panic!("expected build failure error");
        }
    }

    #[test]
    fn provide_duplicate_failure() {
        let kit = Kit::new();
        TestBuilder.kit(&kit).provide::<TestCapKey>().unwrap();
        let result = TestBuilder.kit(&kit).provide::<TestCapKey>();
        assert!(result.is_err());
        if let Err(e) = &result {
            assert!(e.to_string().contains("already exists"));
            assert!(e.to_string().contains("test_cap"));
        } else {
            panic!("expected duplicate capability error");
        }
    }

    #[test]
    fn test_error_display() {
        let err = TestError("oops");
        assert_eq!(err.to_string(), "oops");
    }
}
