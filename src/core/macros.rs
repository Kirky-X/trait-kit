// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Declarative macros for reducing boilerplate in module declarations.
//!
//! These `macro_rules!` macros generate `ModuleMeta` (and optionally
//! `AsyncAutoBuilder`) implementations, replacing repetitive hand-written
//! impl blocks with a single-line invocation.

/// Implements `ModuleMeta` for a module type (no dependencies).
///
/// # Syntax
///
/// ```text
/// impl_module_meta!(Type, "name");
/// impl_module_meta!(Type, "name", deps = [DepA, DepB]);
/// ```
///
/// # Example
///
/// ```
/// use trait_kit::impl_module_meta;
/// use trait_kit::core::ModuleMeta;
///
/// struct MyModule;
/// impl_module_meta!(MyModule, "my-module");
///
/// assert_eq!(MyModule::NAME, "my-module");
/// assert!(MyModule::dependencies().is_empty());
/// ```
#[macro_export]
macro_rules! impl_module_meta {
    ($ty:ty, $name:literal) => {
        impl $crate::core::ModuleMeta for $ty {
            const NAME: &'static str = $name;

            fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
                &[]
            }
        }
    };
    ($ty:ty, $name:literal, deps = [$($dep:ty),* $(,)?]) => {
        impl $crate::core::ModuleMeta for $ty {
            const NAME: &'static str = $name;

            fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
                static DEPS: &[(&str, std::any::TypeId)] = &[
                    $((stringify!($dep), std::any::TypeId::of::<$dep>()),)*
                ];
                DEPS
            }
        }
    };
}

/// Implements `AsyncAutoBuilder` for a module type.
///
/// The body expression must evaluate to
/// `Pin<Box<dyn Future<Output = Result<Capability, Error>> + Send + 'a>>`.
/// The closure parameter `|kit|` binds the `&AsyncKit` argument, matching
/// the hand-written impl pattern.
///
/// # Syntax
///
/// ```text
/// impl_async_auto_builder!(Type, Capability, Error, |kit| <expr>);
/// ```
///
/// # Example
///
/// ```
/// use std::sync::Arc;
/// use trait_kit::impl_module_meta;
/// use trait_kit::impl_async_auto_builder;
/// use trait_kit::core::{AsyncAutoBuilder, ModuleMeta};
/// use trait_kit::kit::AsyncKit;
///
/// # #[derive(Debug, thiserror::Error)]
/// # #[error("mock")]
/// # struct MockErr;
/// # #[derive(Clone)]
/// # struct Cap { v: u32 }
/// struct MyAsyncModule;
/// impl_module_meta!(MyAsyncModule, "my-async");
/// impl_async_auto_builder!(
///     MyAsyncModule,
///     Arc<Cap>,
///     MockErr,
///     |kit| Box::pin(async move {
///         let _ = kit;
///         Ok(Arc::new(Cap { v: 42 }))
///     })
/// );
/// ```
#[cfg(feature = "async")]
#[macro_export]
macro_rules! impl_async_auto_builder {
    ($ty:ty, $cap:ty, $err:ty, |$kit:ident| $body:expr) => {
        impl $crate::core::AsyncAutoBuilder for $ty {
            type Capability = $cap;
            type Error = $err;

            fn build<'a>(
                $kit: &'a $crate::kit::AsyncKit,
            ) -> ::std::pin::Pin<::std::boxed::Box<
                dyn ::std::future::Future<
                    Output = ::std::result::Result<Self::Capability, Self::Error>,
                > + Send + 'a,
            >> {
                $body
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use crate::core::ModuleMeta;

    // === Fixtures ===

    struct MacroModuleNoDeps;
    impl_module_meta!(MacroModuleNoDeps, "macro-no-deps");

    struct Dep1;
    impl_module_meta!(Dep1, "dep1");

    struct Dep2;
    impl_module_meta!(Dep2, "dep2");

    struct MacroModuleWithDeps;
    impl_module_meta!(MacroModuleWithDeps, "macro-with-deps", deps = [Dep1, Dep2]);

    // Hand-written equivalents for comparison

    struct HandWrittenNoDeps;
    impl ModuleMeta for HandWrittenNoDeps {
        const NAME: &'static str = "macro-no-deps";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }

    struct HandWrittenWithDeps;
    impl ModuleMeta for HandWrittenWithDeps {
        const NAME: &'static str = "macro-with-deps";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            static DEPS: &[(&str, std::any::TypeId)] = &[
                ("Dep1", std::any::TypeId::of::<Dep1>()),
                ("Dep2", std::any::TypeId::of::<Dep2>()),
            ];
            DEPS
        }
    }

    // === Tests ===

    #[test]
    fn macro_generates_correct_name_no_deps() {
        assert_eq!(MacroModuleNoDeps::NAME, "macro-no-deps");
    }

    #[test]
    fn macro_generates_empty_dependencies_when_no_deps() {
        assert!(MacroModuleNoDeps::dependencies().is_empty());
    }

    #[test]
    fn macro_generates_correct_name_with_deps() {
        assert_eq!(MacroModuleWithDeps::NAME, "macro-with-deps");
    }

    #[test]
    fn macro_generates_correct_dependency_count() {
        assert_eq!(MacroModuleWithDeps::dependencies().len(), 2);
    }

    #[test]
    fn macro_dependency_names_match_stringified_types() {
        let deps = MacroModuleWithDeps::dependencies();
        assert_eq!(deps[0].0, "Dep1");
        assert_eq!(deps[1].0, "Dep2");
    }

    #[test]
    fn macro_dependency_type_ids_match_hand_written() {
        let macro_deps = MacroModuleWithDeps::dependencies();
        let hand_deps = HandWrittenWithDeps::dependencies();
        assert_eq!(macro_deps.len(), hand_deps.len());
        for (i, (m, h)) in macro_deps.iter().zip(hand_deps.iter()).enumerate() {
            assert_eq!(m.0, h.0, "dep {i}: name mismatch");
            assert_eq!(m.1, h.1, "dep {i}: TypeId mismatch");
        }
    }

    #[test]
    fn macro_name_equals_hand_written_name() {
        assert_eq!(MacroModuleNoDeps::NAME, HandWrittenNoDeps::NAME);
        assert_eq!(MacroModuleWithDeps::NAME, HandWrittenWithDeps::NAME);
    }

    #[test]
    fn macro_dependencies_equal_hand_written_no_deps() {
        let m = MacroModuleNoDeps::dependencies();
        let h = HandWrittenNoDeps::dependencies();
        assert_eq!(m.len(), h.len());
    }
}

#[cfg(all(test, feature = "async"))]
mod async_macro_tests {
    use crate::core::{AsyncAutoBuilder, ModuleMeta};
    use crate::kit::AsyncKit;
    use crate::test_helpers::block_on;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use thiserror::Error;

    // === Fixtures ===

    #[derive(Debug, Error)]
    #[allow(dead_code, reason = "mock error type verifies trait signature only")]
    enum MockErr {
        #[error("mock async build failed: {0}")]
        Failed(String),
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct AsyncCap {
        value: u32,
    }

    // Macro-generated impl
    struct MacroAsyncModule;
    impl_module_meta!(MacroAsyncModule, "macro-async");
    impl_async_auto_builder!(
        MacroAsyncModule,
        Arc<AsyncCap>,
        MockErr,
        |kit| Box::pin(async move {
            let _ = kit;
            Ok(Arc::new(AsyncCap { value: 42 }))
        })
    );

    // Hand-written impl for comparison
    struct HandAsyncModule;
    impl ModuleMeta for HandAsyncModule {
        const NAME: &'static str = "macro-async";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }
    impl AsyncAutoBuilder for HandAsyncModule {
        type Capability = Arc<AsyncCap>;
        type Error = MockErr;
        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>> {
            let _ = kit;
            Box::pin(async move { Ok(Arc::new(AsyncCap { value: 42 })) })
        }
    }

    // Error-propagation fixture
    struct ErrAsyncModule;
    impl_module_meta!(ErrAsyncModule, "err-async");
    impl_async_auto_builder!(
        ErrAsyncModule,
        Arc<AsyncCap>,
        MockErr,
        |kit| Box::pin(async move {
            let _ = kit;
            Err(MockErr::Failed("intentional".to_string()))
        })
    );

    // === Tests ===

    #[test]
    fn macro_async_generates_correct_name() {
        assert_eq!(MacroAsyncModule::NAME, "macro-async");
    }

    #[test]
    fn macro_async_generates_empty_dependencies() {
        assert!(MacroAsyncModule::dependencies().is_empty());
    }

    #[test]
    fn macro_async_capability_type_matches_hand_written() {
        assert_eq!(
            std::any::TypeId::of::<<MacroAsyncModule as AsyncAutoBuilder>::Capability>(),
            std::any::TypeId::of::<<HandAsyncModule as AsyncAutoBuilder>::Capability>(),
        );
    }

    #[test]
    fn macro_async_error_type_matches_hand_written() {
        assert_eq!(
            std::any::TypeId::of::<<MacroAsyncModule as AsyncAutoBuilder>::Error>(),
            std::any::TypeId::of::<<HandAsyncModule as AsyncAutoBuilder>::Error>(),
        );
    }

    #[test]
    fn macro_async_build_returns_expected_capability() {
        let kit = AsyncKit::new();
        let cap = block_on(MacroAsyncModule::build(&kit)).unwrap();
        assert_eq!(cap.value, 42);
    }

    #[test]
    fn macro_async_build_result_matches_hand_written() {
        let kit = AsyncKit::new();
        let macro_cap = block_on(MacroAsyncModule::build(&kit)).unwrap();
        let hand_cap = block_on(HandAsyncModule::build(&kit)).unwrap();
        assert_eq!(macro_cap, hand_cap);
    }

    #[test]
    fn macro_async_build_propagates_errors() {
        let kit = AsyncKit::new();
        let result = block_on(ErrAsyncModule::build(&kit));
        assert!(result.is_err());
    }

    #[test]
    fn macro_async_name_equals_hand_written_name() {
        assert_eq!(MacroAsyncModule::NAME, HandAsyncModule::NAME);
    }
}
