// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Core traits for module declaration and dependency management.

#[cfg(feature = "async")]
use std::future::Future;
#[cfg(feature = "async")]
use std::pin::Pin;

/// Metadata trait for module registration.
pub trait ModuleMeta: 'static {
    /// The diagnostic name of this module.
    const NAME: &'static str;

    /// Returns (name, `TypeId`) pairs for modules this module depends on.
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)];
}

/// Builder trait for module construction.
///
/// Implemented by the user for each module.
pub trait AutoBuilder: ModuleMeta {
    /// The capability type this module provides. Must be Clone.
    type Capability: Clone + 'static;

    /// The error type returned on build failure.
    type Error: std::error::Error + Send + 'static;

    /// Build the module's capability using the provided Kit.
    ///
    /// # Errors
    ///
    /// Returns `Self::Error` if the module fails to build.
    fn build(kit: &crate::kit::Kit) -> Result<Self::Capability, Self::Error>;
}

/// Marker trait for interface/implementation separation.
///
/// Automatically implemented for all `'static` types (including `?Sized`
/// trait objects like `dyn MyTrait`). Used by the `interface` feature to
/// enable `register_as<M, I>()` and `resolve<I>()` for type-erased
/// dependency injection behind a `dyn Trait` interface.
#[cfg(feature = "interface")]
pub trait Interface: 'static {}

#[cfg(feature = "interface")]
impl<T: ?Sized + 'static> Interface for T {}

/// Async builder trait for module construction in async context.
///
/// Async counterpart of [`AutoBuilder`]. Implement this for modules requiring
/// async initialization (database pools, HTTP clients, cache backends).
///
/// The `build` method returns a `Pin<Box<dyn Future + Send>>` rather than using
/// native `async fn` in trait so that the trait can be type-erased through the
/// `AsyncBuildFn` stored in `AsyncKit`'s dependency graph (Phase 1b). Rust
/// 1.91 supports `async fn` in trait (stable since 1.75), but `dyn`-compatible
/// dispatch still requires the explicit `Pin<Box>` indirection.
///
/// Compared to [`AutoBuilder`], the associated types tighten bounds:
/// - `Capability: Clone + Send + Sync + 'static` (cross-thread sharing).
/// - `Error: std::error::Error + Send + 'static` (cross-thread error propagation).
///
/// Requires the `async` feature on the crate.
#[cfg(feature = "async")]
pub trait AsyncAutoBuilder: ModuleMeta {
    /// The capability type this module provides. Must be `Clone + Send + Sync`.
    type Capability: Clone + Send + Sync + 'static;

    /// The error type returned on build failure. Must be `Send + 'static`.
    type Error: std::error::Error + Send + 'static;

    /// Build the module's capability using the provided `AsyncKit`.
    ///
    /// The returned future borrows the kit for lifetime `'a`, allowing
    /// the build callback to read configs / require dependencies from the kit
    /// during async construction.
    ///
    /// # Errors
    ///
    /// Returns `Self::Error` if the module fails to build.
    #[allow(
        clippy::type_complexity,
        reason = "Pin<Box<dyn Future + Send>> is the canonical dyn-compatible async trait dispatch type"
    )]
    fn build<'a>(
        kit: &'a crate::kit::AsyncKit,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>;
}

/// Type-erased build function stored in the dependency graph.
///
/// Takes `&Kit<Unbuilt>` (same memory layout as `&Kit<Ready>`)
/// because during the build phase we only have the unbuilt Kit.
pub(crate) type BuildFn = Box<
    dyn FnOnce(
        &crate::kit::Kit,
    ) -> Result<Box<dyn std::any::Any>, Box<dyn std::error::Error + Send + 'static>>,
>;

#[cfg(all(test, feature = "async"))]
mod async_tests {
    use super::*;
    use crate::kit::AsyncKit;
    use crate::test_helpers::{MockError, block_on};
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;

    #[derive(Debug, Clone, PartialEq)]
    struct LoggerCapability {
        name: String,
    }

    struct MockLoggerModule;

    impl ModuleMeta for MockLoggerModule {
        const NAME: &'static str = "mock-logger";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }

    impl AsyncAutoBuilder for MockLoggerModule {
        type Capability = Arc<LoggerCapability>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
            let _ = kit;
            Box::pin(async move {
                Ok(Arc::new(LoggerCapability {
                    name: "mock".to_string(),
                }))
            })
        }
    }

    #[test]
    fn async_auto_builder_returns_pin_box_future() {
        let kit = AsyncKit::new();
        let fut = MockLoggerModule::build(&kit);
        let result = block_on(fut);
        assert!(result.is_ok());
        let cap = result.expect("build future returned Ok");
        assert_eq!(cap.name, "mock");
    }

    #[test]
    fn async_auto_builder_capability_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<LoggerCapability>();
        assert_send_sync::<Arc<LoggerCapability>>();
    }

    #[test]
    fn async_auto_builder_error_is_send_static() {
        fn assert_send_static<T: Send + 'static>() {}
        assert_send_static::<MockError>();
    }
}

#[cfg(all(test, feature = "interface"))]
mod interface_tests {
    use super::*;

    #[test]
    fn interface_auto_implemented_for_primitive_types() {
        fn assert_interface<T: Interface>() {}
        assert_interface::<i32>();
        assert_interface::<u64>();
        assert_interface::<String>();
        assert_interface::<Vec<u8>>();
        assert_interface::<bool>();
    }

    #[test]
    fn interface_auto_implemented_for_custom_types() {
        struct MyType;
        #[allow(dead_code)]
        enum MyEnum {
            A,
            B,
        }

        fn assert_interface<T: Interface>() {}
        assert_interface::<MyType>();
        assert_interface::<MyEnum>();
    }

    #[test]
    fn interface_auto_implemented_for_reference_types() {
        trait MyTrait {}
        fn assert_interface<T: Interface + ?Sized>() {}
        assert_interface::<dyn MyTrait>();
    }
}
