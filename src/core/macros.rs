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
