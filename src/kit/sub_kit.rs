// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Sub-Kit composition: register a whole child `Kit` as a single
//! module in a parent `Kit`.
//!
//! This gives large applications an explicit vertical-slice boundary: a child
//! Kit owns its own modules and dependency graph (validated independently at
//! `build()`), and the parent only sees the [`SubKitHandle`] capability —
//! capabilities stay namespaced inside the child instead of polluting the
//! parent's flat TypeMap.
//!
//! Usage:
//!
//! 1. Implement [`SubKitSpec`] describing how to compose the child Kit;
//! 2. `parent.register::<SubKitModule<MySpec>>()`.
//!
//! Requires the `compose` feature.

use std::any::TypeId;
use std::marker::PhantomData;
use std::rc::Rc;

use crate::core::{AutoBuilder, ModuleMeta};
use crate::error::TraitKitError;
use crate::kit::{Kit, Ready};

/// Declarative description of a child Kit.
pub trait SubKitSpec: 'static {
    /// Diagnostic name of the child module inside the parent graph.
    const NAME: &'static str;

    /// Register the child's modules into its (fresh) Kit. Invoked once per
    /// parent build.
    fn compose(kit: &mut Kit);

    /// Optional parent-level dependencies: declared like `ModuleMeta::
    /// dependencies` so the parent's graph validates the cross-Kit edges
    /// (`DependencyMissing` is reported at parent `build()`).
    fn dependencies() -> &'static [(&'static str, TypeId)] {
        &[]
    }
}

/// Cloneable handle to a built child Kit.
///
/// The parent-facing capability of a [`SubKitModule`]: resolves capabilities
/// from the child Kit's namespaced store without merging them into the parent.
#[derive(Clone)]
pub struct SubKitHandle {
    kit: Rc<Kit<Ready>>,
}

impl SubKitHandle {
    /// Resolve a capability from the child Kit (read-only view).
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::MissingCapability` if the child has no such
    /// capability (namespace isolation: parent modules are invisible here).
    pub fn require<M: AutoBuilder>(&self) -> Result<M::Capability, TraitKitError> {
        self.kit.require::<M>()
    }

    /// Whether the child Kit has a capability for `M`.
    #[must_use]
    pub fn contains<M: AutoBuilder>(&self) -> bool {
        self.kit.contains::<M>()
    }

    /// Number of modules registered in the child graph.
    #[must_use]
    pub fn module_count(&self) -> usize {
        self.kit.module_count()
    }
}

impl std::fmt::Debug for SubKitHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SubKitHandle({} modules)", self.module_count())
    }
}

/// The parent-side module wrapping a child Kit.
///
/// `build()` composes and builds the child Kit (validating the child's own
/// dependency graph), then hands out the [`SubKitHandle`]. A child build
/// failure surfaces as `TraitKitError::BuildFailed` on the parent build,
/// context-tagged with the child's name.
pub struct SubKitModule<S: SubKitSpec> {
    _marker: PhantomData<S>,
}

impl<S: SubKitSpec> ModuleMeta for SubKitModule<S> {
    const NAME: &'static str = S::NAME;

    fn dependencies() -> &'static [(&'static str, TypeId)] {
        S::dependencies()
    }
}

impl<S: SubKitSpec> AutoBuilder for SubKitModule<S> {
    type Capability = SubKitHandle;
    type Error = TraitKitError;

    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        let mut child = Kit::new();
        S::compose(&mut child);
        let ready = child.build()?;
        Ok(SubKitHandle { kit: Rc::new(ready) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    struct ChildCap(u32);

    #[derive(Debug)]
    struct ChildError;

    impl std::fmt::Display for ChildError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "child error")
        }
    }
    impl std::error::Error for ChildError {}

    struct ChildLeaf;
    impl ModuleMeta for ChildLeaf {
        const NAME: &'static str = "child-leaf";
    }
    impl AutoBuilder for ChildLeaf {
        type Capability = Arc<ChildCap>;
        type Error = ChildError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(ChildCap(1)))
        }
    }

    struct ChildTop;
    impl ModuleMeta for ChildTop {
        const NAME: &'static str = "child-top";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            static DEPS: &[(&str, TypeId)] =
                &[(<ChildLeaf as ModuleMeta>::NAME, TypeId::of::<ChildLeaf>())];
            DEPS
        }
    }
    impl AutoBuilder for ChildTop {
        type Capability = Arc<ChildCap>;
        type Error = ChildError;
        fn build(kit: &Kit) -> Result<Self::Capability, Self::Error> {
            let leaf = kit.require::<ChildLeaf>().map_err(|_| ChildError)?;
            Ok(Arc::new(ChildCap(leaf.0 + 10)))
        }
    }

    /// Spec composing leaf → top inside the child namespace.
    struct DataSliceSpec;
    impl SubKitSpec for DataSliceSpec {
        const NAME: &'static str = "data-slice";
        fn compose(kit: &mut Kit) {
            kit.register::<ChildLeaf>().expect("child leaf");
            kit.register::<ChildTop>().expect("child top");
        }
    }

    #[test]
    fn sub_kit_builds_and_resolves_namespaced_capabilities() {
        let mut parent = Kit::new();
        parent.register::<SubKitModule<DataSliceSpec>>().expect("register sub-kit");
        let ready = parent.build().expect("parent build ok");

        let handle = ready.require::<SubKitModule<DataSliceSpec>>().expect("handle");
        // Child internal DI resolved inside the namespace.
        let top = handle.require::<ChildTop>().expect("child top");
        assert_eq!(top.0, 11);
        assert!(handle.contains::<ChildLeaf>());
        assert_eq!(handle.module_count(), 2);
    }

    #[test]
    fn capabilities_stay_namespaced_inside_child() {
        let mut parent = Kit::new();
        parent.register::<SubKitModule<DataSliceSpec>>().expect("register");
        let ready = parent.build().expect("build ok");

        // Parent has no direct visibility into child capabilities...
        assert!(
            !ready.contains::<ChildLeaf>(),
            "child capabilities must not leak into the parent namespace"
        );
        // ...and the handle's require cannot see parent modules either.
        let handle = ready.require::<SubKitModule<DataSliceSpec>>().expect("handle");
        assert!(!handle.contains::<SubKitModule<DataSliceSpec>>());
    }

    #[test]
    fn child_cycle_fails_parent_build_with_context() {
        struct CycA;
        impl ModuleMeta for CycA {
            const NAME: &'static str = "cyc-a";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                static DEPS: &[(&str, TypeId)] =
                    &[("cyc-b", TypeId::of::<CycB>())];
                DEPS
            }
        }
        impl AutoBuilder for CycA {
            type Capability = Arc<ChildCap>;
            type Error = ChildError;
            fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
                Ok(Arc::new(ChildCap(0)))
            }
        }
        struct CycB;
        impl ModuleMeta for CycB {
            const NAME: &'static str = "cyc-b";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                static DEPS: &[(&str, TypeId)] =
                    &[("cyc-a", TypeId::of::<CycA>())];
                DEPS
            }
        }
        impl AutoBuilder for CycB {
            type Capability = Arc<ChildCap>;
            type Error = ChildError;
            fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
                Ok(Arc::new(ChildCap(0)))
            }
        }

        struct CyclicSpec;
        impl SubKitSpec for CyclicSpec {
            const NAME: &'static str = "cyclic-slice";
            fn compose(kit: &mut Kit) {
                kit.register::<CycA>().expect("a");
                kit.register::<CycB>().expect("b");
            }
        }

        let mut parent = Kit::new();
        parent.register::<SubKitModule<CyclicSpec>>().expect("register");
        let err = parent.build().expect_err("child cycle must fail parent build");
        let msg = err.to_string();
        assert!(
            msg.contains("cyclic-slice"),
            "failure tagged with child module name: {msg}"
        );
    }

    #[test]
    fn declared_cross_kit_dependency_validated_in_parent_graph() {
        struct ParentDepProvider;
        impl ModuleMeta for ParentDepProvider {
            const NAME: &'static str = "parent-dep";
        }
        impl AutoBuilder for ParentDepProvider {
            type Capability = Arc<ChildCap>;
            type Error = ChildError;
            fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
                Ok(Arc::new(ChildCap(0)))
            }
        }

        struct DependentSpec;
        impl SubKitSpec for DependentSpec {
            const NAME: &'static str = "dependent-slice";
            fn compose(_kit: &mut Kit) {}
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                static DEPS: &[(&str, TypeId)] =
                    &[(<ParentDepProvider as ModuleMeta>::NAME, TypeId::of::<ParentDepProvider>())];
                DEPS
            }
        }

        // Missing parent dep → DependencyMissing at parent build.
        let mut parent = Kit::new();
        parent.register::<SubKitModule<DependentSpec>>().expect("register");
        let err = parent.build().expect_err("missing cross-kit dep");
        assert!(
            matches!(err, TraitKitError::DependencyMissing { module: "dependent-slice", .. }),
            "cross-Kit dependency validated in the parent graph: {err:?}"
        );

        // Registered parent dep → build succeeds.
        let mut parent = Kit::new();
        parent.register::<ParentDepProvider>().expect("register dep");
        parent.register::<SubKitModule<DependentSpec>>().expect("register");
        parent.build().expect("build ok with dep present");
    }
}
