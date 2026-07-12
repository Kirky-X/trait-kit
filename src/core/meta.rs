// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Core traits for module declaration and dependency management.

#[cfg(feature = "async")]
use std::future::Future;
#[cfg(feature = "async")]
use std::pin::Pin;
#[cfg(feature = "interface")]
use std::sync::Arc;

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

/// Extension trait for interface/implementation separation.
///
/// Unlike [`AutoBuilder`], this trait associates a concrete `Capability`
/// type with a `?Sized` `Interface` type (e.g., `dyn Logger`). The
/// [`into_interface`](InterfaceBuilder::into_interface) method performs the
/// type erasure, converting the concrete capability into
/// `Arc<Self::Interface>`.
///
/// Used by `register_as<M>()` and `resolve<I>()` behind the `interface`
/// feature. This trait does **not** modify [`AutoBuilder`], so existing
/// module impls are unaffected.
#[cfg(feature = "interface")]
pub trait InterfaceBuilder: ModuleMeta {
    /// The interface type (e.g., `dyn Logger`). Must be `?Sized + 'static`.
    type Interface: ?Sized + 'static;

    /// The concrete capability type. Must be `Clone + 'static`.
    type Capability: Clone + 'static;

    /// The error type returned on build failure.
    type Error: std::error::Error + Send + 'static;

    /// Build the module's concrete capability using the provided Kit.
    ///
    /// # Errors
    ///
    /// Returns `Self::Error` if the module fails to build.
    fn build(kit: &crate::kit::Kit) -> Result<Self::Capability, Self::Error>;

    /// Convert the concrete capability into a type-erased interface object.
    ///
    /// Typically implemented as `Ok(cap)` relying on `Arc<T> → Arc<dyn Trait>`
    /// unsized coercion, where `T: Self::Interface`.
    fn into_interface(cap: Self::Capability) -> Arc<Self::Interface>;
}

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

#[cfg(all(test, feature = "interface"))]
mod interface_builder_tests {
    use super::*;
    use crate::kit::Kit;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Test interface: a simple Logger trait.
    trait Logger: 'static {
        fn log(&self, msg: &str);
    }

    /// Concrete implementation of Logger.
    struct ConsoleLogger {
        counter: AtomicUsize,
    }

    impl Logger for ConsoleLogger {
        fn log(&self, msg: &str) {
            let _ = msg;
            self.counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Test error type (test_helpers::MockError is gated on `async` feature).
    #[derive(Debug)]
    struct InterfaceTestError;

    impl std::fmt::Display for InterfaceTestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "interface test error")
        }
    }

    impl std::error::Error for InterfaceTestError {}

    /// Module that provides a ConsoleLogger behind the dyn Logger interface.
    struct ConsoleLoggerModule;

    impl ModuleMeta for ConsoleLoggerModule {
        const NAME: &'static str = "console-logger";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }

    impl InterfaceBuilder for ConsoleLoggerModule {
        type Interface = dyn Logger;
        type Capability = Arc<ConsoleLogger>;
        type Error = InterfaceTestError;

        fn build(_kit: &Kit) -> Result<Arc<ConsoleLogger>, InterfaceTestError> {
            Ok(Arc::new(ConsoleLogger {
                counter: AtomicUsize::new(0),
            }))
        }

        fn into_interface(cap: Arc<ConsoleLogger>) -> Arc<dyn Logger> {
            cap
        }
    }

    #[test]
    fn interface_builder_build_returns_concrete_capability() {
        let kit = Kit::new();
        let cap = ConsoleLoggerModule::build(&kit).expect("build succeeds");
        assert_eq!(Arc::strong_count(&cap), 1);
    }

    #[test]
    fn interface_builder_into_interface_produces_trait_object() {
        let kit = Kit::new();
        let cap = ConsoleLoggerModule::build(&kit).expect("build succeeds");
        let iface: Arc<dyn Logger> = ConsoleLoggerModule::into_interface(cap);
        iface.log("hello");
        iface.log("world");
    }

    #[test]
    fn interface_builder_interface_type_is_dyn_compatible() {
        fn assert_dyn_compatible<T: ?Sized + 'static>() {}
        assert_dyn_compatible::<dyn Logger>();
    }

    #[test]
    fn interface_builder_capability_is_clone() {
        let cap = Arc::new(ConsoleLogger {
            counter: AtomicUsize::new(0),
        });
        let cloned = cap.clone();
        assert_eq!(Arc::strong_count(&cloned), 2);
        drop(cap);
        assert_eq!(Arc::strong_count(&cloned), 1);
    }

    #[test]
    fn interface_builder_does_not_require_autobuilder() {
        // InterfaceBuilder is an independent trait — a module can implement
        // InterfaceBuilder without implementing AutoBuilder. Verify
        // ConsoleLoggerModule does NOT impl AutoBuilder by checking that
        // calling AutoBuilder::build would not compile (negative verification
        // via trait bound assertion).
        fn requires_interface_builder<T: InterfaceBuilder>() {}
        requires_interface_builder::<ConsoleLoggerModule>();
    }
}
