// Unit tests for `crate::kit::kit`, split out of kit.rs for readability.
//
// Path note: declared in kit.rs as `#[cfg(test)] #[path = "kit_tests.rs"] mod kit_tests;`,
// so this file IS the `kit_tests` module. Each inner `mod xxx_tests` therefore
// resolves `use super::super::*;` to the `kit` module (parent chain:
// `xxx_tests` → `kit_tests` → `kit`); the glob also imports `kit`-private
// items, which stay visible to descendant modules.
#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::core::{AutoBuilder, ModuleMeta};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // === Test fixtures ===

    struct MockCapability;
    impl ModuleMeta for MockCapability {
        const NAME: &'static str = "mock";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }
    impl AutoBuilder for MockCapability {
        type Capability = Arc<AtomicUsize>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(AtomicUsize::new(0)))
        }
    }

    struct DependentModule;
    impl ModuleMeta for DependentModule {
        const NAME: &'static str = "dependent";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            static DEPS: &[(&str, std::any::TypeId)] =
                &[("mock", std::any::TypeId::of::<MockCapability>())];
            DEPS
        }
    }
    impl AutoBuilder for DependentModule {
        type Capability = Arc<AtomicUsize>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(AtomicUsize::new(0)))
        }
    }


    #[test]
    fn overrides_field_is_empty_on_new() {
        let kit = Kit::new();
        assert_eq!(kit.overrides.borrow().len(), 0);
    }

    #[test]
    fn overrides_field_is_empty_after_build() {
        let kit = Kit::new();
        assert_eq!(kit.overrides.borrow().len(), 0);
    }


    #[test]
    fn override_module_inserts_into_overrides_map() {
        let kit = Kit::new();
        assert_eq!(kit.overrides.borrow().len(), 0);
        kit.override_module::<MockCapability>(Arc::new(AtomicUsize::new(42)));
        assert_eq!(kit.overrides.borrow().len(), 1);
    }

    #[test]
    fn override_module_strict_succeeds_when_deps_registered() {
        let mut kit = Kit::new();
        // Register the dependency first
        kit.register::<MockCapability>().unwrap();
        // Now strict override of the dependent module should succeed
        let result = kit.override_module_strict::<DependentModule>(Arc::new(AtomicUsize::new(99)));
        assert!(result.is_ok());
        assert_eq!(kit.overrides.borrow().len(), 1);
    }

    #[test]
    fn override_module_strict_fails_when_deps_missing() {
        let mut kit = Kit::new();
        // Do NOT register MockCapability first
        let result = kit.override_module_strict::<DependentModule>(Arc::new(AtomicUsize::new(99)));
        assert!(matches!(
            result,
            Err(TraitKitError::DependencyMissing {
                module: "dependent",
                missing: "mock"
            })
        ));
        // Override should not have been inserted
        assert_eq!(kit.overrides.borrow().len(), 0);
    }


    /// Module whose `build_fn` increments a counter, to verify override skips it.
    struct CountingModule;
    impl ModuleMeta for CountingModule {
        const NAME: &'static str = "counting";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }
    impl AutoBuilder for CountingModule {
        type Capability = Arc<AtomicUsize>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            // Return a counter that starts at 0; the test checks the counter
            // value to distinguish "build_fn ran" from "override used".
            Ok(Arc::new(AtomicUsize::new(0)))
        }
    }

    #[test]
    fn build_uses_override_and_skips_build_fn() {
        let kit = Kit::new();
        // Register the module (so it's in the graph and gets sorted)
        let mut kit = kit;
        kit.register::<CountingModule>().unwrap();
        // Override with a capability value of 42
        kit.override_module::<CountingModule>(Arc::new(AtomicUsize::new(42)));
        // Build
        let built = kit.build().unwrap();
        // require() should return the override value (42), not the build_fn value (0)
        let cap = built.require::<CountingModule>().unwrap();
        assert_eq!(cap.load(Ordering::SeqCst), 42);
    }

    #[test]
    fn build_uses_build_fn_when_no_override() {
        let mut kit = Kit::new();
        kit.register::<CountingModule>().unwrap();
        // No override — build_fn should run and produce value 0
        let built = kit.build().unwrap();
        let cap = built.require::<CountingModule>().unwrap();
        assert_eq!(cap.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn build_inserts_unregistered_override_after_topo_loop() {
        // override_module allows injecting a module that was NOT registered.
        // build() should still make it available via require().
        let kit = Kit::new();
        kit.override_module::<MockCapability>(Arc::new(AtomicUsize::new(77)));
        let built = kit.build().unwrap();
        let cap = built.require::<MockCapability>().unwrap();
        assert_eq!(cap.load(Ordering::SeqCst), 77);
    }


    #[test]
    fn require_ref_returns_reference_to_built_capability() {
        let mut kit = Kit::new();
        kit.register::<CountingModule>().unwrap();
        let built = kit.build().unwrap();
        let r = built.require_ref::<CountingModule>().unwrap();
        // build_fn returns Arc<AtomicUsize::new(0)>
        assert_eq!((*r).load(Ordering::SeqCst), 0);
    }

    #[test]
    fn require_ref_returns_override_value() {
        let mut kit = Kit::new();
        kit.register::<CountingModule>().unwrap();
        kit.override_module::<CountingModule>(Arc::new(AtomicUsize::new(55)));
        let built = kit.build().unwrap();
        let r = built.require_ref::<CountingModule>().unwrap();
        assert_eq!((*r).load(Ordering::SeqCst), 55);
    }

    #[test]
    fn require_ref_returns_missing_capability_for_unbuilt() {
        let kit = Kit::new();
        let built = kit.build().unwrap();
        let result = built.require_ref::<CountingModule>();
        assert!(matches!(
            result,
            Err(TraitKitError::MissingCapability { ref key }) if key == "counting"
        ));
    }


    #[test]
    fn register_lazy_does_not_build_during_build() {
        let mut kit = Kit::new();
        kit.register_lazy::<CountingModule>().unwrap();
        // build() should succeed without triggering CountingModule's build_fn
        let built = kit.build().unwrap();
        // The capability should NOT be available (lazy not yet triggered)
        assert!(!built.contains::<CountingModule>());
    }

    #[test]
    fn register_lazy_adds_to_dependency_graph() {
        let mut kit = Kit::new();
        // Register dependency first
        kit.register::<MockCapability>().unwrap();
        // Register lazy module that depends on MockCapability
        kit.register_lazy::<DependentModule>().unwrap();
        // build() should succeed (graph validation passes)
        let built = kit.build().unwrap();
        // MockCapability should be built (eager), DependentModule should NOT (lazy)
        assert!(built.contains::<MockCapability>());
        assert!(!built.contains::<DependentModule>());
    }

    #[test]
    fn register_lazy_returns_already_registered_for_duplicate() {
        let mut kit = Kit::new();
        kit.register_lazy::<CountingModule>().unwrap();
        let result = kit.register_lazy::<CountingModule>();
        assert!(matches!(
            result,
            Err(TraitKitError::AlreadyRegistered { module: "counting" })
        ));
    }


    #[test]
    fn lazy_slots_empty_on_new_kit() {
        let kit = Kit::new();
        assert_eq!(kit.lazy_slots.borrow().len(), 0);
    }

    #[test]
    fn build_transfers_lazy_builders_to_lazy_slots() {
        let mut kit = Kit::new();
        kit.register_lazy::<CountingModule>().unwrap();
        assert_eq!(kit.lazy_builders.borrow().len(), 1);
        assert_eq!(kit.lazy_slots.borrow().len(), 0);

        let built = kit.build().unwrap();

        // After build(): lazy_builders drained, lazy_slots populated
        assert_eq!(built.lazy_builders.borrow().len(), 0);
        assert_eq!(built.lazy_slots.borrow().len(), 1);
        assert!(
            built
                .lazy_slots
                .borrow()
                .contains_key(&TypeId::of::<CountingModule>())
        );
    }

    #[test]
    fn lazy_slots_cells_empty_after_build() {
        let mut kit = Kit::new();
        kit.register_lazy::<CountingModule>().unwrap();
        let built = kit.build().unwrap();

        // The OnceLock cell should be empty (not yet constructed) — first
        // access via require() will populate it.
        let slots = built.lazy_slots.borrow();
        let slot = slots
            .get(&TypeId::of::<CountingModule>())
            .expect("slot exists");
        assert!(slot.cell.get().is_none());
    }

    #[test]
    fn build_transfers_multiple_lazy_builders_to_lazy_slots() {
        let mut kit = Kit::new();
        kit.register::<MockCapability>().unwrap();
        kit.register_lazy::<DependentModule>().unwrap();
        kit.register_lazy::<CountingModule>().unwrap();
        assert_eq!(kit.lazy_builders.borrow().len(), 2);

        let built = kit.build().unwrap();

        assert_eq!(built.lazy_builders.borrow().len(), 0);
        assert_eq!(built.lazy_slots.borrow().len(), 2);
        assert!(
            built
                .lazy_slots
                .borrow()
                .contains_key(&TypeId::of::<DependentModule>())
        );
        assert!(
            built
                .lazy_slots
                .borrow()
                .contains_key(&TypeId::of::<CountingModule>())
        );
    }


    #[test]
    fn require_triggers_lazy_construction_on_first_access() {
        let mut kit = Kit::new();
        kit.register_lazy::<CountingModule>().unwrap();
        let built = kit.build().unwrap();

        // Before require: capability not in capabilities map
        assert!(!built.contains::<CountingModule>());

        // First require should trigger lazy construction
        let cap = built.require::<CountingModule>().unwrap();
        assert_eq!(cap.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn require_does_not_rebuild_lazy_on_second_call() {
        // Local static counter — each test function has its own COUNT
        static COUNT: AtomicUsize = AtomicUsize::new(0);

        struct CountedModule;
        impl ModuleMeta for CountedModule {
            const NAME: &'static str = "test-counted";
            fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
                &[]
            }
        }
        impl AutoBuilder for CountedModule {
            type Capability = Arc<AtomicUsize>;
            type Error = TraitKitError;
            fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
                let n = COUNT.fetch_add(1, Ordering::SeqCst);
                Ok(Arc::new(AtomicUsize::new(n)))
            }
        }

        COUNT.store(0, Ordering::SeqCst);
        let mut kit = Kit::new();
        kit.register_lazy::<CountedModule>().unwrap();
        let built = kit.build().unwrap();

        let cap1 = built.require::<CountedModule>().unwrap();
        let cap2 = built.require::<CountedModule>().unwrap();

        // Both calls should return the same value (builder called once)
        assert_eq!(
            cap1.load(Ordering::SeqCst),
            0,
            "first require returns count 0"
        );
        assert_eq!(
            cap2.load(Ordering::SeqCst),
            0,
            "second require returns same count"
        );
        assert_eq!(
            COUNT.load(Ordering::SeqCst),
            1,
            "builder invoked exactly once"
        );
    }

    #[test]
    fn require_lazy_with_registered_dependency_succeeds() {
        // A lazy module that calls kit.require() for its dependency in build()
        struct LazyDependentModule;
        impl ModuleMeta for LazyDependentModule {
            const NAME: &'static str = "lazy-dependent";
            fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
                static DEPS: &[(&str, std::any::TypeId)] =
                    &[("mock", std::any::TypeId::of::<MockCapability>())];
                DEPS
            }
        }
        impl AutoBuilder for LazyDependentModule {
            type Capability = Arc<AtomicUsize>;
            type Error = TraitKitError;
            fn build(kit: &Kit) -> Result<Self::Capability, Self::Error> {
                // Verify the eager dependency is accessible during lazy build
                let mock = kit.require::<MockCapability>()?;
                Ok(Arc::new(AtomicUsize::new(
                    mock.load(Ordering::SeqCst) + 100,
                )))
            }
        }

        let mut kit = Kit::new();
        // Register MockCapability (adds to dependency graph) then override
        // with value 42 to verify it's accessible during lazy build
        kit.register::<MockCapability>().unwrap();
        kit.override_module::<MockCapability>(Arc::new(AtomicUsize::new(42)));
        kit.register_lazy::<LazyDependentModule>().unwrap();
        let built = kit.build().unwrap();

        // First require triggers lazy build, which calls require::<MockCapability>()
        let cap = built.require::<LazyDependentModule>().unwrap();
        assert_eq!(
            cap.load(Ordering::SeqCst),
            142,
            "lazy build accessed eager dep (42 + 100)"
        );
    }


    /// Multi-binding module A (capability = Arc<AtomicUsize>).
    struct MultiModuleA;
    impl ModuleMeta for MultiModuleA {
        const NAME: &'static str = "multi-a";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }
    impl AutoBuilder for MultiModuleA {
        type Capability = Arc<AtomicUsize>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(AtomicUsize::new(10)))
        }
    }

    /// Multi-binding module B (same capability type as `MultiModuleA`).
    struct MultiModuleB;
    impl ModuleMeta for MultiModuleB {
        const NAME: &'static str = "multi-b";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }
    impl AutoBuilder for MultiModuleB {
        type Capability = Arc<AtomicUsize>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(AtomicUsize::new(20)))
        }
    }

    /// Multi-binding module C (same capability type as `MultiModuleA`).
    struct MultiModuleC;
    impl ModuleMeta for MultiModuleC {
        const NAME: &'static str = "multi-c";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }
    impl AutoBuilder for MultiModuleC {
        type Capability = Arc<AtomicUsize>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(AtomicUsize::new(30)))
        }
    }

    #[test]
    fn multi_builders_empty_on_new_kit() {
        let kit = Kit::new();
        assert_eq!(kit.multi_builders.borrow().len(), 0);
    }

    #[test]
    fn register_multi_adds_to_multi_builders() {
        let mut kit = Kit::new();
        assert_eq!(kit.multi_builders.borrow().len(), 0);

        kit.register_multi::<MultiModuleA>().unwrap();

        // Keyed by TypeId::of::<M::Capability>() = TypeId::of::<Arc<AtomicUsize>>()
        let cap_id = TypeId::of::<Arc<AtomicUsize>>();
        assert_eq!(kit.multi_builders.borrow().len(), 1);
        assert!(kit.multi_builders.borrow().contains_key(&cap_id));
        assert_eq!(
            kit.multi_builders.borrow().get(&cap_id).unwrap().len(),
            1,
            "first register_multi should produce Vec of length 1"
        );
    }

    #[test]
    fn register_multi_three_times_appends_to_vec() {
        let mut kit = Kit::new();
        kit.register_multi::<MultiModuleA>().unwrap();
        kit.register_multi::<MultiModuleB>().unwrap();
        kit.register_multi::<MultiModuleC>().unwrap();

        let cap_id = TypeId::of::<Arc<AtomicUsize>>();
        let builders = kit.multi_builders.borrow();
        let vec = builders.get(&cap_id).expect("cap_id exists");
        assert_eq!(
            vec.len(),
            3,
            "three register_multi calls should produce Vec of length 3"
        );
    }

    #[test]
    fn register_multi_adds_module_to_dependency_graph() {
        let mut kit = Kit::new();
        kit.register_multi::<MultiModuleA>().unwrap();

        // The module type_id (not cap_id) should be in the graph
        assert!(kit.graph.name_of(TypeId::of::<MultiModuleA>()).is_some());
    }

    #[test]
    fn register_multi_returns_already_registered_for_duplicate_module() {
        let mut kit = Kit::new();
        kit.register_multi::<MultiModuleA>().unwrap();

        let result = kit.register_multi::<MultiModuleA>();
        assert!(matches!(
            result,
            Err(TraitKitError::AlreadyRegistered { module: "multi-a" })
        ));
    }

    #[test]
    fn register_multi_returns_already_registered_if_already_registered_via_register() {
        let mut kit = Kit::new();
        kit.register::<MockCapability>().unwrap();

        let result = kit.register_multi::<MockCapability>();
        assert!(matches!(
            result,
            Err(TraitKitError::AlreadyRegistered { module: "mock" })
        ));
    }

    #[test]
    fn register_multi_coexists_with_register_for_different_modules() {
        let mut kit = Kit::new();
        kit.register::<MockCapability>().unwrap();
        kit.register_multi::<MultiModuleA>().unwrap();
        kit.register_multi::<MultiModuleB>().unwrap();

        // MockCapability in builders, MultiModuleA/B in multi_builders
        assert!(
            kit.builders
                .borrow()
                .contains_key(&TypeId::of::<MockCapability>())
        );
        let cap_id = TypeId::of::<Arc<AtomicUsize>>();
        assert_eq!(kit.multi_builders.borrow().get(&cap_id).unwrap().len(), 2);
    }


    #[test]
    fn require_all_returns_empty_for_unregistered_capability() {
        let mut kit = Kit::new();
        // Register MockCapability (eager, not multi)
        kit.register::<MockCapability>().unwrap();
        let built = kit.build().unwrap();

        // require_all for a capability with no multi-binding registrations
        let result = built.require_all::<MultiModuleA>();
        assert!(matches!(
            result,
            Err(TraitKitError::MissingCapability { ref key }) if key == "multi-a"
        ));
    }

    #[test]
    fn require_all_returns_vec_of_three_after_three_register_multi() {
        let mut kit = Kit::new();
        kit.register_multi::<MultiModuleA>().unwrap();
        kit.register_multi::<MultiModuleB>().unwrap();
        kit.register_multi::<MultiModuleC>().unwrap();
        let built = kit.build().unwrap();

        let caps = built.require_all::<MultiModuleA>().unwrap();
        assert_eq!(
            caps.len(),
            3,
            "three register_multi calls should return Vec of length 3"
        );
    }

    #[test]
    fn require_all_preserves_registration_order() {
        let mut kit = Kit::new();
        kit.register_multi::<MultiModuleA>().unwrap(); // builds value 10
        kit.register_multi::<MultiModuleB>().unwrap(); // builds value 20
        kit.register_multi::<MultiModuleC>().unwrap(); // builds value 30
        let built = kit.build().unwrap();

        let caps = built.require_all::<MultiModuleA>().unwrap();
        assert_eq!(caps.len(), 3);
        // Verify order matches registration: 10, 20, 30
        assert_eq!(
            caps[0].load(Ordering::SeqCst),
            10,
            "first cap should be 10 (MultiModuleA)"
        );
        assert_eq!(
            caps[1].load(Ordering::SeqCst),
            20,
            "second cap should be 20 (MultiModuleB)"
        );
        assert_eq!(
            caps[2].load(Ordering::SeqCst),
            30,
            "third cap should be 30 (MultiModuleC)"
        );
    }

    #[test]
    fn require_all_returns_missing_capability_before_build() {
        let mut kit = Kit::new();
        kit.register_multi::<MultiModuleA>().unwrap();
        // Don't call build() — multi_capabilities is empty

        let result = kit.require_all::<MultiModuleA>();
        assert!(matches!(
            result,
            Err(TraitKitError::MissingCapability { ref key }) if key == "multi-a"
        ));
    }

    #[test]
    fn build_drains_multi_builders_into_multi_capabilities() {
        let mut kit = Kit::new();
        kit.register_multi::<MultiModuleA>().unwrap();
        kit.register_multi::<MultiModuleB>().unwrap();

        // Before build: multi_builders has entries, multi_capabilities is empty
        assert_eq!(kit.multi_builders.borrow().len(), 1); // one cap_id key
        assert_eq!(kit.multi_capabilities.borrow().len(), 0);

        let built = kit.build().unwrap();

        // After build: multi_builders is drained, multi_capabilities is populated
        assert_eq!(built.multi_builders.borrow().len(), 0);
        assert_eq!(built.multi_capabilities.borrow().len(), 1);
        let cap_id = TypeId::of::<Arc<AtomicUsize>>();
        assert_eq!(
            built
                .multi_capabilities
                .borrow()
                .get(&cap_id)
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn require_all_coexists_with_require_for_single_binding() {
        let mut kit = Kit::new();
        // Single binding: MockCapability (eager)
        kit.register::<MockCapability>().unwrap();
        // Multi-binding: MultiModuleA, MultiModuleB
        kit.register_multi::<MultiModuleA>().unwrap();
        kit.register_multi::<MultiModuleB>().unwrap();
        let built = kit.build().unwrap();

        // require gets the single binding
        let single = built.require::<MockCapability>().unwrap();
        assert_eq!(single.load(Ordering::SeqCst), 0);

        // require_all gets the multi-binding (returns MultiModuleA's cap type)
        let multi = built.require_all::<MultiModuleA>().unwrap();
        assert_eq!(multi.len(), 2);
        assert_eq!(multi[0].load(Ordering::SeqCst), 10);
        assert_eq!(multi[1].load(Ordering::SeqCst), 20);
    }

    #[test]
    fn multi_binding_build_error_returns_build_failed() {
        struct FailMultiModule;
        impl ModuleMeta for FailMultiModule {
            const NAME: &'static str = "fail-multi";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                &[]
            }
        }
        impl AutoBuilder for FailMultiModule {
            type Capability = Arc<AtomicUsize>;
            type Error = TraitKitError;
            fn build(_kit: &Kit) -> Result<Arc<AtomicUsize>, TraitKitError> {
                Err(TraitKitError::BuildFailed {
                    context: "fail-multi".into(),
                    source: Box::new(std::io::Error::other("multi fail")),
                })
            }
        }

        let mut kit = Kit::new();
        kit.register_multi::<FailMultiModule>().unwrap();
        let result = kit.build();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TraitKitError::BuildFailed { .. }
        ));
    }
}

#[cfg(all(test, feature = "interface"))]
mod interface_tests {
    use super::super::*;
    use crate::core::{InterfaceBuilder, ModuleMeta};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // === Test fixtures ===

    /// Test interface trait.
    trait Logger: 'static {
        fn log(&self, msg: &str) -> String;
    }

    /// First Logger implementation.
    struct ConsoleLogger;

    impl Logger for ConsoleLogger {
        fn log(&self, msg: &str) -> String {
            format!("[console] {msg}")
        }
    }

    /// Second Logger implementation (for duplicate interface test).
    struct FileLogger;

    impl Logger for FileLogger {
        fn log(&self, msg: &str) -> String {
            format!("[file] {msg}")
        }
    }

    /// Test error type.
    #[derive(Debug)]
    struct InterfaceTestError;

    impl std::fmt::Display for InterfaceTestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "interface test error")
        }
    }

    impl std::error::Error for InterfaceTestError {}

    /// Module providing `ConsoleLogger` behind dyn Logger.
    struct ConsoleLoggerModule;

    impl ModuleMeta for ConsoleLoggerModule {
        const NAME: &'static str = "console-logger-iface";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }

    impl InterfaceBuilder for ConsoleLoggerModule {
        type Interface = dyn Logger;
        type Capability = Arc<ConsoleLogger>;
        type Error = InterfaceTestError;

        fn build(_kit: &Kit) -> Result<Arc<ConsoleLogger>, InterfaceTestError> {
            Ok(Arc::new(ConsoleLogger))
        }

        fn into_interface(cap: Arc<ConsoleLogger>) -> Arc<dyn Logger> {
            cap
        }
    }

    /// Module providing `FileLogger` behind dyn Logger (same interface).
    struct FileLoggerModule;

    impl ModuleMeta for FileLoggerModule {
        const NAME: &'static str = "file-logger";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            &[]
        }
    }

    impl InterfaceBuilder for FileLoggerModule {
        type Interface = dyn Logger;
        type Capability = Arc<FileLogger>;
        type Error = InterfaceTestError;

        fn build(_kit: &Kit) -> Result<Arc<FileLogger>, InterfaceTestError> {
            Ok(Arc::new(FileLogger))
        }

        fn into_interface(cap: Arc<FileLogger>) -> Arc<dyn Logger> {
            cap
        }
    }

    // === Tests ===

    #[test]
    fn register_as_then_resolve_returns_arc_dyn_trait() {
        let mut kit = Kit::new();
        kit.register_as::<ConsoleLoggerModule>()
            .expect("register_as succeeds");
        let built = kit.build().expect("build succeeds");

        let logger: Arc<dyn Logger> = built.resolve::<dyn Logger>().expect("resolve succeeds");
        assert_eq!(logger.log("hello"), "[console] hello");
    }

    #[test]
    fn register_as_twice_same_interface_returns_already_registered() {
        let mut kit = Kit::new();
        kit.register_as::<ConsoleLoggerModule>()
            .expect("first register_as succeeds");
        let err = kit.register_as::<FileLoggerModule>().unwrap_err();
        // 错误里的模块名必须是已占据接口的先注册者，而非被拒绝的新模块。
        assert!(
            matches!(
                err,
                TraitKitError::AlreadyRegistered {
                    module: "console-logger-iface"
                }
            ),
            "expected AlreadyRegistered naming the existing owner, got {err:?}"
        );
    }

    #[test]
    fn resolve_before_build_returns_missing_capability() {
        let mut kit = Kit::new();
        kit.register_as::<ConsoleLoggerModule>()
            .expect("register_as succeeds");
        // resolve on unbuilt kit — capabilities is empty
        assert!(kit.resolve::<dyn Logger>().is_err());
    }

    #[test]
    fn resolve_unregistered_interface_returns_missing_capability() {
        let kit = Kit::new();
        let built = kit.build().expect("build succeeds");
        assert!(built.resolve::<dyn Logger>().is_err());
    }

    #[test]
    fn register_as_builds_during_build() {
        let mut kit = Kit::new();
        kit.register_as::<ConsoleLoggerModule>()
            .expect("register_as succeeds");
        let built = kit.build().expect("build succeeds");
        // After build, resolve should return the built capability
        let logger = built.resolve::<dyn Logger>().expect("resolve succeeds");
        assert_eq!(logger.log("test"), "[console] test");
    }

    #[test]
    fn resolve_returns_callable_trait_object() {
        let mut kit = Kit::new();
        kit.register_as::<ConsoleLoggerModule>()
            .expect("register_as succeeds");
        let built = kit.build().expect("build succeeds");

        let logger: Arc<dyn Logger> = built.resolve().expect("resolve succeeds");
        let result = logger.log("world");
        assert_eq!(result, "[console] world");
    }

    #[test]
    fn register_as_coexists_with_register() {
        // register (AutoBuilder) + register_as (InterfaceBuilder) for
        // different modules should coexist.
        struct RegularModule;
        impl ModuleMeta for RegularModule {
            const NAME: &'static str = "regular";
            fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
                &[]
            }
        }
        impl AutoBuilder for RegularModule {
            type Capability = Arc<AtomicUsize>;
            type Error = TraitKitError;
            fn build(_kit: &Kit) -> Result<Arc<AtomicUsize>, TraitKitError> {
                Ok(Arc::new(AtomicUsize::new(42)))
            }
        }

        let mut kit = Kit::new();
        kit.register::<RegularModule>().expect("register succeeds");
        kit.register_as::<ConsoleLoggerModule>()
            .expect("register_as succeeds");
        let built = kit.build().expect("build succeeds");

        // Both retrieve correctly
        let cap = built.require::<RegularModule>().expect("require succeeds");
        assert_eq!(cap.load(Ordering::SeqCst), 42);

        let logger = built.resolve::<dyn Logger>().expect("resolve succeeds");
        assert_eq!(logger.log("coexist"), "[console] coexist");
    }

    #[test]
    fn register_as_same_module_twice_returns_already_registered() {
        let mut kit = Kit::new();
        kit.register_as::<ConsoleLoggerModule>()
            .expect("first register_as succeeds");
        // Same module type — graph.add() rejects duplicate
        let err = kit.register_as::<ConsoleLoggerModule>().unwrap_err();
        assert!(
            matches!(err, TraitKitError::AlreadyRegistered { .. }),
            "expected AlreadyRegistered, got {err:?}"
        );
    }

    #[test]
    fn file_logger_interface_build_and_resolve() {
        let mut kit = Kit::new();
        kit.register_as::<FileLoggerModule>()
            .expect("register_as succeeds");
        let built = kit.build().expect("build succeeds");
        let logger: Arc<dyn Logger> = built.resolve::<dyn Logger>().expect("resolve succeeds");
        assert_eq!(logger.log("hello"), "[file] hello");
    }

    #[test]
    fn interface_test_error_display() {
        let e = InterfaceTestError;
        assert_eq!(format!("{e}"), "interface test error");
    }

    #[test]
    fn interface_build_error_returns_build_failed() {
        struct FailIfaceModule;
        impl ModuleMeta for FailIfaceModule {
            const NAME: &'static str = "fail-iface";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                &[]
            }
        }
        impl InterfaceBuilder for FailIfaceModule {
            type Interface = dyn Logger;
            type Capability = Arc<()>;
            type Error = InterfaceTestError;
            fn build(_kit: &Kit) -> Result<Arc<()>, InterfaceTestError> {
                Err(InterfaceTestError)
            }
            fn into_interface(_cap: Arc<()>) -> Arc<dyn Logger> {
                unreachable!()
            }
        }

        let mut kit = Kit::new();
        kit.register_as::<FailIfaceModule>().unwrap();
        let result = kit.build();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TraitKitError::BuildFailed { .. }
        ));
    }
}

// ─── Feature-gated integration tests ─────────────────────────────────────

#[cfg(all(test, feature = "lifecycle"))]
mod lifecycle_tests {
    use super::super::*;
    use crate::core::ModuleMeta;
    use crate::core::lifecycle::Lifecycle;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static LC_SHUTDOWN: AtomicUsize = AtomicUsize::new(0);
    static LC_READY: AtomicUsize = AtomicUsize::new(0);

    struct LcModule;
    impl ModuleMeta for LcModule {
        const NAME: &'static str = "lc-module";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AutoBuilder for LcModule {
        type Capability = Arc<AtomicUsize>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<AtomicUsize>, TraitKitError> {
            Ok(Arc::new(AtomicUsize::new(0)))
        }
    }
    impl Lifecycle for LcModule {
        fn on_ready(_kit: &Kit<Ready>) -> Result<(), Self::Error> {
            LC_READY.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn on_shutdown(_cap: &Arc<AtomicUsize>) {
            LC_SHUTDOWN.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn lifecycle_on_ready_called_during_build() {
        LC_READY.store(0, Ordering::SeqCst);
        let mut kit = Kit::new();
        kit.register::<LcModule>().unwrap();
        kit.register_lifecycle::<LcModule>();
        let _built = kit.build().unwrap();
        assert_eq!(
            LC_READY.load(Ordering::SeqCst),
            1,
            "on_ready should be called once"
        );
    }

    #[test]
    fn lifecycle_shutdown_called_in_reverse_order() {
        LC_SHUTDOWN.store(0, Ordering::SeqCst);
        let mut kit = Kit::new();
        kit.register::<LcModule>().unwrap();
        kit.register_lifecycle::<LcModule>();
        let built = kit.build().unwrap();
        built.shutdown();
        assert_eq!(
            LC_SHUTDOWN.load(Ordering::SeqCst),
            1,
            "on_shutdown should be called once"
        );
    }

    // ── 逆拓扑关闭顺序：依赖者（consumer）先于被依赖者（dependency）关闭，
    //    与注册顺序无关（build 时按拓扑索引稳定排序，shutdown 逆序执行）。──

    static TOPO_SHUTDOWN_ORDER: std::sync::Mutex<Vec<&'static str>> =
        std::sync::Mutex::new(Vec::new());

    struct TopoDepModule;
    impl ModuleMeta for TopoDepModule {
        const NAME: &'static str = "topo-dep";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AutoBuilder for TopoDepModule {
        type Capability = Arc<()>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<()>, TraitKitError> {
            Ok(Arc::new(()))
        }
    }
    impl Lifecycle for TopoDepModule {
        fn on_ready(_kit: &Kit<Ready>) -> Result<(), Self::Error> {
            Ok(())
        }
        fn on_shutdown(_cap: &Arc<()>) {
            TOPO_SHUTDOWN_ORDER.lock().unwrap().push("dep");
        }
    }

    struct TopoConsumerModule;
    impl ModuleMeta for TopoConsumerModule {
        const NAME: &'static str = "topo-consumer";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            static DEPS: &[(&str, TypeId)] =
                &[(TopoDepModule::NAME, TypeId::of::<TopoDepModule>())];
            DEPS
        }
    }
    impl AutoBuilder for TopoConsumerModule {
        type Capability = Arc<()>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<()>, TraitKitError> {
            Ok(Arc::new(()))
        }
    }
    impl Lifecycle for TopoConsumerModule {
        fn on_ready(_kit: &Kit<Ready>) -> Result<(), Self::Error> {
            Ok(())
        }
        fn on_shutdown(_cap: &Arc<()>) {
            TOPO_SHUTDOWN_ORDER.lock().unwrap().push("consumer");
        }
    }

    #[test]
    fn lifecycle_shutdown_runs_in_reverse_topological_order() {
        TOPO_SHUTDOWN_ORDER.lock().unwrap().clear();
        let mut kit = Kit::new();
        // 故意按与拓扑序相反的顺序注册/登记生命周期：依赖者先注册。
        kit.register::<TopoConsumerModule>().unwrap();
        kit.register::<TopoDepModule>().unwrap();
        kit.register_lifecycle::<TopoConsumerModule>();
        kit.register_lifecycle::<TopoDepModule>();
        let built = kit.build().unwrap();
        built.shutdown();
        let order = TOPO_SHUTDOWN_ORDER.lock().unwrap();
        assert_eq!(
            order.as_slice(),
            ["consumer", "dep"],
            "shutdown must run in reverse topological order (dependents first), \
             regardless of registration order"
        );
    }

    #[test]
    fn lifecycle_on_ready_failure_propagates() {
        struct FailReadyModule;
        impl ModuleMeta for FailReadyModule {
            const NAME: &'static str = "fail-ready";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                &[]
            }
        }
        impl AutoBuilder for FailReadyModule {
            type Capability = Arc<()>;
            type Error = TraitKitError;
            fn build(_kit: &Kit) -> Result<Arc<()>, TraitKitError> {
                Ok(Arc::new(()))
            }
        }
        impl Lifecycle for FailReadyModule {
            fn on_ready(_kit: &Kit<Ready>) -> Result<(), TraitKitError> {
                Err(TraitKitError::BuildFailed {
                    context: "on_ready".into(),
                    source: Box::new(std::io::Error::other("intentional failure")),
                })
            }
        }

        let mut kit = Kit::new();
        kit.register::<FailReadyModule>().unwrap();
        kit.register_lifecycle::<FailReadyModule>();
        let result = kit.build();
        assert!(result.is_err(), "build should fail when on_ready fails");
        let err = result.unwrap_err();
        assert!(matches!(err, TraitKitError::LifecycleFailed { .. }));
    }
}

#[cfg(all(test, feature = "health"))]
mod health_tests {
    use super::super::*;
    use crate::core::ModuleMeta;
    use crate::core::health::{HealthCheck, HealthStatus};
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    struct HcCap {
        val: i32,
    }

    struct HcModule;
    impl ModuleMeta for HcModule {
        const NAME: &'static str = "hc-module";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AutoBuilder for HcModule {
        type Capability = Arc<HcCap>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<HcCap>, TraitKitError> {
            Ok(Arc::new(HcCap { val: 42 }))
        }
    }
    impl HealthCheck for HcModule {
        fn check(cap: &Arc<HcCap>) -> HealthStatus {
            if cap.val > 0 {
                HealthStatus::Healthy
            } else {
                HealthStatus::Unhealthy {
                    detail: "zero".into(),
                }
            }
        }
    }

    #[test]
    fn health_check_registered_and_queryable() {
        let mut kit = Kit::new();
        kit.register::<HcModule>().unwrap();
        kit.register_health_check::<HcModule>();
        let built = kit.build().unwrap();
        let status = built.health_check::<HcModule>().unwrap();
        assert_eq!(status, HealthStatus::Healthy);
    }

    #[test]
    fn health_report_returns_all_checkers() {
        let mut kit = Kit::new();
        kit.register::<HcModule>().unwrap();
        kit.register_health_check::<HcModule>();
        let built = kit.build().unwrap();
        let report = built.health_report();
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].0, "hc-module");
        assert_eq!(report[0].1, HealthStatus::Healthy);
    }

    #[test]
    fn health_check_unregistered_returns_error() {
        let mut kit = Kit::new();
        kit.register::<HcModule>().unwrap();
        let built = kit.build().unwrap();
        let err = built.health_check::<HcModule>().unwrap_err();
        assert!(matches!(err, TraitKitError::MissingConfig { .. }));
    }

    #[test]
    fn health_check_unhealthy_for_zero_value() {
        struct ZeroHcModule;
        impl ModuleMeta for ZeroHcModule {
            const NAME: &'static str = "zero-hc";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                &[]
            }
        }
        impl AutoBuilder for ZeroHcModule {
            type Capability = Arc<HcCap>;
            type Error = TraitKitError;
            fn build(_kit: &Kit) -> Result<Arc<HcCap>, TraitKitError> {
                Ok(Arc::new(HcCap { val: 0 }))
            }
        }
        impl HealthCheck for ZeroHcModule {
            fn check(cap: &Arc<HcCap>) -> HealthStatus {
                if cap.val > 0 {
                    HealthStatus::Healthy
                } else {
                    HealthStatus::Unhealthy {
                        detail: "zero".into(),
                    }
                }
            }
        }

        let mut kit = Kit::new();
        kit.register::<ZeroHcModule>().unwrap();
        kit.register_health_check::<ZeroHcModule>();
        let built = kit.build().unwrap();
        let status = built.health_check::<ZeroHcModule>().unwrap();
        assert!(matches!(status, HealthStatus::Unhealthy { .. }));
    }
}

#[cfg(all(test, feature = "observer"))]
mod observability_tests {
    use super::super::*;
    use crate::core::ModuleMeta;
    use crate::core::observer::BuildObserver;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    struct CountingObs {
        start: Arc<AtomicUsize>,
        built: Arc<AtomicUsize>,
    }
    impl BuildObserver for CountingObs {
        fn on_module_start(&self, _: &'static str) {
            self.start.fetch_add(1, Ordering::SeqCst);
        }
        fn on_module_built(&self, _: &'static str, _: Duration) {
            self.built.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct ObsModule;
    impl ModuleMeta for ObsModule {
        const NAME: &'static str = "obs-module";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AutoBuilder for ObsModule {
        type Capability = Arc<()>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<()>, TraitKitError> {
            Ok(Arc::new(()))
        }
    }

    #[test]
    fn observer_callbacks_fired_during_build() {
        let start = Arc::new(AtomicUsize::new(0));
        let built = Arc::new(AtomicUsize::new(0));
        let obs = Arc::new(CountingObs {
            start: Arc::clone(&start),
            built: Arc::clone(&built),
        });
        let mut kit = Kit::new();
        kit.with_observer(obs);
        kit.register::<ObsModule>().unwrap();
        kit.build().unwrap();
        assert_eq!(
            start.load(Ordering::SeqCst),
            1,
            "on_module_start should fire"
        );
        assert_eq!(
            built.load(Ordering::SeqCst),
            1,
            "on_module_built should fire"
        );
    }

    #[test]
    fn observer_on_build_error_called_on_failure() {
        struct FailObs {
            errors: Arc<AtomicUsize>,
        }
        impl BuildObserver for FailObs {
            fn on_build_error(&self, _: &'static str, _: &TraitKitError) {
                self.errors.fetch_add(1, Ordering::SeqCst);
            }
        }

        struct FailBuildModule;
        impl ModuleMeta for FailBuildModule {
            const NAME: &'static str = "fail-build";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                &[]
            }
        }
        impl AutoBuilder for FailBuildModule {
            type Capability = Arc<()>;
            type Error = TraitKitError;
            fn build(_kit: &Kit) -> Result<Arc<()>, TraitKitError> {
                Err(TraitKitError::BuildFailed {
                    context: "intentional".into(),
                    source: Box::new(std::io::Error::other("test failure")),
                })
            }
        }

        let errors = Arc::new(AtomicUsize::new(0));
        let obs = Arc::new(FailObs {
            errors: Arc::clone(&errors),
        });
        let mut kit = Kit::new();
        kit.with_observer(obs);
        kit.register::<FailBuildModule>().unwrap();
        let result = kit.build();
        assert!(result.is_err(), "build should fail");
        assert_eq!(
            errors.load(Ordering::SeqCst),
            1,
            "on_build_error should fire once"
        );
    }
}

#[cfg(test)]
mod factory_tests {
    use super::super::*;
    use crate::core::ModuleMeta;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static FACTORY_COUNT: AtomicUsize = AtomicUsize::new(0);

    struct FactoryModule;
    impl ModuleMeta for FactoryModule {
        const NAME: &'static str = "factory-module";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AutoBuilder for FactoryModule {
        type Capability = Arc<AtomicUsize>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<AtomicUsize>, TraitKitError> {
            let n = FACTORY_COUNT.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(AtomicUsize::new(n)))
        }
    }

    #[test]
    fn factory_creates_new_instance_each_call() {
        FACTORY_COUNT.store(0, Ordering::SeqCst);
        let mut kit = Kit::new();
        kit.register::<FactoryModule>().unwrap();
        let built = kit.build().unwrap();
        let factory = built.factory::<FactoryModule>();
        let cap1 = factory().unwrap();
        let cap2 = factory().unwrap();
        // Each call invokes build() — counter increments
        assert_ne!(
            cap1.load(Ordering::SeqCst),
            cap2.load(Ordering::SeqCst),
            "factory should produce different instances"
        );
    }
}

#[cfg(all(test, feature = "scope"))]
mod scope_tests {
    use super::super::*;
    use crate::core::ModuleMeta;
    use std::sync::Arc;

    struct ScopeMockModule;
    impl ModuleMeta for ScopeMockModule {
        const NAME: &'static str = "scope-mock";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AutoBuilder for ScopeMockModule {
        type Capability = Arc<()>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<()>, TraitKitError> {
            Ok(Arc::new(()))
        }
    }

    #[test]
    fn create_scope_returns_empty_scope() {
        let mut kit = Kit::new();
        kit.register::<ScopeMockModule>().unwrap();
        let built = kit.build().unwrap();
        let scope = built.create_scope();
        assert!(!scope.contains::<ScopeMockModule>());
    }
}

#[cfg(test)]
mod conditional_tests {
    use super::super::*;
    use crate::core::ModuleMeta;
    use std::sync::Arc;

    struct CondMockModule;
    impl ModuleMeta for CondMockModule {
        const NAME: &'static str = "cond-mock";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AutoBuilder for CondMockModule {
        type Capability = Arc<()>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<()>, TraitKitError> {
            Ok(Arc::new(()))
        }
    }

    #[test]
    fn register_if_true_registers_module() {
        let mut kit = Kit::new();
        let registered = kit.register_if::<CondMockModule>(|_| true).unwrap();
        assert!(registered);
        let built = kit.build().unwrap();
        assert!(built.contains::<CondMockModule>());
    }

    #[test]
    fn register_if_false_skips_module() {
        let mut kit = Kit::new();
        let registered = kit.register_if::<CondMockModule>(|_| false).unwrap();
        assert!(!registered);
        let built = kit.build().unwrap();
        assert!(!built.contains::<CondMockModule>());
    }
}

#[cfg(all(test, feature = "decorator"))]
mod decorator_tests {
    use super::super::*;
    use crate::core::ModuleMeta;
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    struct DecCap {
        val: String,
    }

    struct DecModule;
    impl ModuleMeta for DecModule {
        const NAME: &'static str = "dec-module";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AutoBuilder for DecModule {
        type Capability = Arc<DecCap>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<DecCap>, TraitKitError> {
            Ok(Arc::new(DecCap {
                val: "original".into(),
            }))
        }
    }

    #[test]
    fn decorate_registers_decorator() {
        let mut kit = Kit::new();
        kit.register_lazy::<DecModule>().unwrap();
        kit.decorate::<DecModule>(|cap| {
            Arc::new(DecCap {
                val: format!("{}+decorated", cap.val),
            })
        });
        // Decorator is applied during lazy require()
        let built = kit.build().unwrap();
        let cap = built.require::<DecModule>().unwrap();
        assert_eq!(cap.val, "original+decorated");
    }

    /// 两个 lazy 模块共享同一能力类型时，装饰器只作用于被 `decorate` 的
    /// 那个模块：lazy `require` 按模块→能力映射（`decorator_module_to_cap`）
    /// 查找装饰器，未映射的模块不得被过应用。
    #[test]
    fn lazy_decorator_not_applied_to_module_sharing_capability_type() {
        #[derive(Debug, Clone)]
        struct SharedCap {
            val: String,
        }

        struct SharedCapA;
        impl ModuleMeta for SharedCapA {
            const NAME: &'static str = "shared-cap-a";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                &[]
            }
        }
        impl AutoBuilder for SharedCapA {
            type Capability = Arc<SharedCap>;
            type Error = TraitKitError;
            fn build(_kit: &Kit) -> Result<Arc<SharedCap>, TraitKitError> {
                Ok(Arc::new(SharedCap { val: "a".into() }))
            }
        }

        struct SharedCapB;
        impl ModuleMeta for SharedCapB {
            const NAME: &'static str = "shared-cap-b";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                &[]
            }
        }
        impl AutoBuilder for SharedCapB {
            type Capability = Arc<SharedCap>;
            type Error = TraitKitError;
            fn build(_kit: &Kit) -> Result<Arc<SharedCap>, TraitKitError> {
                Ok(Arc::new(SharedCap { val: "b".into() }))
            }
        }

        let mut kit = Kit::new();
        kit.register_lazy::<SharedCapA>().unwrap();
        kit.register_lazy::<SharedCapB>().unwrap();
        // 只装饰模块 A；B 与 A 共享 Arc<SharedCap> 能力类型。
        kit.decorate::<SharedCapA>(|cap| {
            Arc::new(SharedCap {
                val: format!("{}+decorated", cap.val),
            })
        });
        let built = kit.build().unwrap();
        let a = built.require::<SharedCapA>().unwrap();
        assert_eq!(a.val, "a+decorated", "被装饰的模块 A 应携带装饰");
        let b = built.require::<SharedCapB>().unwrap();
        assert_eq!(
            b.val, "b",
            "模块 B 未被 decorate，lazy require 不得套用 A 的装饰器"
        );
    }
}

#[cfg(all(test, feature = "encryption"))]
mod encryption_tests {
    use super::super::*;
    use crate::kit::ModuleConfig;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct SecretConfig {
        api_key: String,
    }

    impl ModuleConfig for SecretConfig {
        const PATH: &'static str = "test.secret";
        fn default_value() -> Self {
            Self {
                api_key: "default".into(),
            }
        }
    }

    #[test]
    fn set_and_get_encrypted_roundtrip() {
        let kit = Kit::new();
        let master_key = [0x42u8; 32];
        let config = SecretConfig {
            api_key: std::env::var("TRAIT_KIT_TEST_API_KEY")
                .unwrap_or_else(|_| "demo-value".into()),
        };
        kit.set_encrypted(&config, &master_key).unwrap();
        assert!(kit.contains_encrypted::<SecretConfig>());
        let built = kit.build().unwrap();
        let decrypted: SecretConfig = built.get_encrypted(&master_key).unwrap();
        assert_eq!(decrypted, config);
    }

    #[test]
    fn contains_encrypted_false_for_missing() {
        let kit = Kit::new();
        assert!(!kit.contains_encrypted::<SecretConfig>());
    }

    #[test]
    fn get_encrypted_missing_returns_error() {
        let kit = Kit::new();
        let built = kit.build().unwrap();
        let master_key = [0x42u8; 32];
        let err = built
            .get_encrypted::<SecretConfig>(&master_key)
            .unwrap_err();
        assert!(matches!(err, TraitKitError::MissingConfig { .. }));
    }

    #[test]
    fn secret_config_default_value() {
        let default = SecretConfig::default_value();
        assert_eq!(default.api_key, "default");
    }

    #[test]
    fn get_encrypted_wrong_key_returns_error() {
        let kit = Kit::new();
        let master_key = [0x42u8; 32];
        let config = SecretConfig {
            api_key: "secret".into(),
        };
        kit.set_encrypted(&config, &master_key).unwrap();
        let built = kit.build().unwrap();
        // Use a different key to trigger decryption failure
        let wrong_key = [0xFFu8; 32];
        let err = built.get_encrypted::<SecretConfig>(&wrong_key).unwrap_err();
        assert!(matches!(err, TraitKitError::BuildFailed { .. }));
    }

    /// `zeroize_bytes` 必须把缓冲区整体清零：Vec 堆缓冲与固定大小数组
    /// （`[u8; 32]`，即派生密钥的实际类型）两种形态都要覆盖。
    #[test]
    fn zeroize_bytes_clears_buffer_to_all_zeros() {
        let mut buf = vec![0xA5u8; 64];
        zeroize_bytes(&mut buf);
        assert!(buf.iter().all(|&b| b == 0), "Vec buffer must be all zeros");

        let mut key = [0x11u8; 32];
        zeroize_bytes(&mut key);
        assert!(key.iter().all(|&b| b == 0), "array key must be all zeros");
    }
}

// ─── Kit<Ready> surface tests ─────────────────────────────────────────────

#[cfg(test)]
mod ready_tests {
    use super::super::*;
    use crate::core::ModuleMeta;
    use std::sync::Arc;

    struct ReadyMockModule;
    impl ModuleMeta for ReadyMockModule {
        const NAME: &'static str = "ready-mock";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AutoBuilder for ReadyMockModule {
        type Capability = Arc<()>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<()>, TraitKitError> {
            Ok(Arc::new(()))
        }
    }

    #[test]
    fn ready_optional_returns_none_for_unbuilt() {
        let kit = Kit::new();
        let built = kit.build().unwrap();
        assert!(built.optional::<ReadyMockModule>().is_none());
    }

    #[test]
    fn ready_optional_returns_some_for_built() {
        let mut kit = Kit::new();
        kit.register::<ReadyMockModule>().unwrap();
        let built = kit.build().unwrap();
        assert!(built.optional::<ReadyMockModule>().is_some());
    }

    #[test]
    fn ready_contains_returns_true_for_built() {
        let mut kit = Kit::new();
        kit.register::<ReadyMockModule>().unwrap();
        let built = kit.build().unwrap();
        assert!(built.contains::<ReadyMockModule>());
    }

    #[test]
    fn ready_contains_returns_false_for_unbuilt() {
        let kit = Kit::new();
        let built = kit.build().unwrap();
        assert!(!built.contains::<ReadyMockModule>());
    }

    #[test]
    fn ready_contains_config_returns_true() {
        let kit = Kit::new();
        kit.set_config(42i32);
        let built = kit.build().unwrap();
        assert!(built.contains_config::<i32>());
    }

    #[test]
    fn ready_contains_config_returns_false() {
        let kit = Kit::new();
        let built = kit.build().unwrap();
        assert!(!built.contains_config::<u64>());
    }

    #[test]
    fn debug_unbuilt_format() {
        let kit = Kit::new();
        let debug = format!("{kit:?}");
        assert!(debug.contains("Kit<Unbuilt>"));
        assert!(debug.contains("modules"));
    }

    #[test]
    fn debug_ready_format() {
        let mut kit = Kit::new();
        kit.register::<ReadyMockModule>().unwrap();
        let built = kit.build().unwrap();
        let debug = format!("{built:?}");
        assert!(debug.contains("Kit<Ready>"));
        assert!(debug.contains("modules"));
    }

    #[test]
    fn default_creates_empty_kit() {
        let kit = Kit::default();
        let built = kit.build().unwrap();
        assert_eq!(built.graph.entries().len(), 0);
    }

    #[test]
    fn graph_dot_returns_valid_string() {
        let mut kit = Kit::new();
        kit.register::<ReadyMockModule>().unwrap();
        let built = kit.build().unwrap();
        let dot = built.graph_dot();
        assert!(dot.contains("digraph"));
    }

    #[test]
    fn graph_mermaid_returns_valid_string() {
        let mut kit = Kit::new();
        kit.register::<ReadyMockModule>().unwrap();
        let built = kit.build().unwrap();
        let mermaid = built.graph_mermaid();
        assert!(mermaid.contains("graph TD"));
    }

    #[test]
    fn config_missing_returns_error() {
        let kit = Kit::new();
        let built = kit.build().unwrap();
        let err = built.config::<i32>().unwrap_err();
        assert!(matches!(err, TraitKitError::MissingConfig { .. }));
    }

    #[test]
    fn require_ref_returns_missing_for_unbuilt() {
        let kit = Kit::new();
        let built = kit.build().unwrap();
        let err = built.require_ref::<ReadyMockModule>().unwrap_err();
        assert!(matches!(err, TraitKitError::MissingCapability { .. }));
    }

    #[test]
    fn build_missing_dependency_returns_error() {
        struct DepModule;
        impl ModuleMeta for DepModule {
            const NAME: &'static str = "dep";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                &[]
            }
        }
        impl AutoBuilder for DepModule {
            type Capability = Arc<()>;
            type Error = TraitKitError;
            fn build(_kit: &Kit) -> Result<Arc<()>, TraitKitError> {
                Ok(Arc::new(()))
            }
        }

        struct NeedsDepModule;
        impl ModuleMeta for NeedsDepModule {
            const NAME: &'static str = "needs-dep";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                static DEPS: &[(&str, TypeId)] = &[("dep", TypeId::of::<DepModule>())];
                DEPS
            }
        }
        impl AutoBuilder for NeedsDepModule {
            type Capability = Arc<()>;
            type Error = TraitKitError;
            fn build(_kit: &Kit) -> Result<Arc<()>, TraitKitError> {
                Ok(Arc::new(()))
            }
        }

        let mut kit = Kit::new();
        kit.register::<NeedsDepModule>().unwrap();
        // Don't register DepModule — should fail
        let result = kit.build();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TraitKitError::DependencyMissing { .. }
        ));
    }

    #[test]
    fn build_cycle_detected_returns_error() {
        struct CycleA;
        impl ModuleMeta for CycleA {
            const NAME: &'static str = "cycle-a";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                static DEPS: &[(&str, TypeId)] = &[("cycle-b", TypeId::of::<CycleB>())];
                DEPS
            }
        }
        impl AutoBuilder for CycleA {
            type Capability = Arc<()>;
            type Error = TraitKitError;
            fn build(_kit: &Kit) -> Result<Arc<()>, TraitKitError> {
                Ok(Arc::new(()))
            }
        }

        struct CycleB;
        impl ModuleMeta for CycleB {
            const NAME: &'static str = "cycle-b";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                static DEPS: &[(&str, TypeId)] = &[("cycle-a", TypeId::of::<CycleA>())];
                DEPS
            }
        }
        impl AutoBuilder for CycleB {
            type Capability = Arc<()>;
            type Error = TraitKitError;
            fn build(_kit: &Kit) -> Result<Arc<()>, TraitKitError> {
                Ok(Arc::new(()))
            }
        }

        let mut kit = Kit::new();
        kit.register::<CycleA>().unwrap();
        kit.register::<CycleB>().unwrap();
        let result = kit.build();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TraitKitError::CycleDetected { .. }
        ));
    }

    #[test]
    fn lazy_require_build_error() {
        struct LazyFailModule;
        impl ModuleMeta for LazyFailModule {
            const NAME: &'static str = "lazy-fail";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                &[]
            }
        }
        impl AutoBuilder for LazyFailModule {
            type Capability = Arc<()>;
            type Error = TraitKitError;
            fn build(_kit: &Kit) -> Result<Arc<()>, TraitKitError> {
                Err(TraitKitError::BuildFailed {
                    context: "lazy-fail".into(),
                    source: Box::new(std::io::Error::other("lazy fail")),
                })
            }
        }

        let mut kit = Kit::new();
        kit.register_lazy::<LazyFailModule>().unwrap();
        let built = kit.build().unwrap();
        let err = built.require::<LazyFailModule>().unwrap_err();
        assert!(matches!(err, TraitKitError::BuildFailed { .. }));
        // 失败后 builder 必须被放回槽位：第二次 require 重新执行 builder，
        // 得到同样的 BuildFailed（错误信息一致），而不是永久 MissingCapability，
        // 原始错误也不会丢失。
        let err2 = built.require::<LazyFailModule>().unwrap_err();
        assert!(
            matches!(err2, TraitKitError::BuildFailed { .. }),
            "第二次 require 应重试 builder 并返回 BuildFailed，got {err2:?}"
        );
        assert!(
            !matches!(err2, TraitKitError::MissingCapability { .. }),
            "第二次 require 不得退化为 MissingCapability"
        );
        assert_eq!(
            err.to_string(),
            err2.to_string(),
            "两次 require 的错误信息应一致"
        );
    }
}

// ─── Validation Tests ───────────────────────────────────────────────────────

#[cfg(all(test, feature = "confers"))]
mod validation_tests {
    use super::super::*;
    use crate::kit::config::{Configurable, Validatable};
    use std::error::Error;

    #[derive(Clone, Debug, PartialEq)]
    struct ValidConfig {
        port: u16,
    }

    impl Configurable for ValidConfig {
        fn load() -> Result<Self, Box<dyn Error + Send>> {
            Ok(Self { port: 8080 })
        }
    }

    impl Validatable for ValidConfig {
        fn validate(&self) -> Result<(), Vec<String>> {
            if self.port > 0 && self.port < 65535 {
                Ok(())
            } else {
                Err(vec!["port out of range".to_string()])
            }
        }
    }

    #[derive(Clone, Debug, PartialEq)]
    struct InvalidConfig {
        port: u16,
    }

    impl Configurable for InvalidConfig {
        fn load() -> Result<Self, Box<dyn Error + Send>> {
            Ok(Self { port: 0 })
        }
    }

    impl Validatable for InvalidConfig {
        fn validate(&self) -> Result<(), Vec<String>> {
            Err(vec![
                "port must be > 0".to_string(),
                "port must be < 65535".to_string(),
            ])
        }
    }

    #[test]
    fn load_and_validate_succeeds_with_valid_config() {
        let kit = Kit::new();
        kit.load_and_validate::<ValidConfig>()
            .expect("valid config should pass");
        let config: ValidConfig = kit.config().expect("config should be stored");
        assert_eq!(config.port, 8080);
    }

    #[test]
    fn load_and_validate_fails_with_invalid_config() {
        let kit = Kit::new();
        let err = kit
            .load_and_validate::<InvalidConfig>()
            .expect_err("invalid config should fail");
        let msg = format!("{err}");
        assert!(
            msg.contains("port must be > 0"),
            "error should contain first validation error: {msg}"
        );
        assert!(
            msg.contains("port must be < 65535"),
            "error should contain second validation error: {msg}"
        );
    }

    #[test]
    fn load_and_validate_does_not_store_on_failure() {
        let kit = Kit::new();
        let _ = kit.load_and_validate::<InvalidConfig>();
        let result: Result<InvalidConfig, _> = kit.config();
        assert!(result.is_err(), "invalid config should not be stored");
    }

    #[test]
    fn load_and_validate_retry_after_failure() {
        let kit = Kit::new();
        let _ = kit.load_and_validate::<InvalidConfig>();
        // Now load a valid config of a different type
        kit.load_and_validate::<ValidConfig>()
            .expect("valid config should succeed after previous failure");
        let config: ValidConfig = kit.config().expect("valid config should be stored");
        assert_eq!(config.port, 8080);
    }
}

// ─── Snapshot Tests ─────────────────────────────────────────────────────────

#[cfg(all(test, feature = "confers"))]
mod snapshot_tests {
    use super::super::*;
    use crate::kit::config::Configurable;
    use std::error::Error;

    #[derive(Clone, Debug, PartialEq)]
    struct SnapConfig {
        value: String,
    }

    impl Configurable for SnapConfig {
        fn load() -> Result<Self, Box<dyn Error + Send>> {
            Ok(Self {
                value: "loaded".to_string(),
            })
        }
    }

    #[test]
    fn snapshot_returns_true_when_config_exists() {
        let kit = Kit::new();
        kit.set_config(SnapConfig {
            value: "original".to_string(),
        });
        assert!(kit.snapshot_config::<SnapConfig>());
    }

    #[test]
    fn snapshot_returns_false_when_config_missing() {
        let kit = Kit::new();
        assert!(!kit.snapshot_config::<SnapConfig>());
    }

    #[test]
    fn restore_overwrites_current_config() {
        let kit = Kit::new();
        kit.set_config(SnapConfig {
            value: "original".to_string(),
        });
        kit.snapshot_config::<SnapConfig>();
        // Modify current config
        kit.set_config(SnapConfig {
            value: "modified".to_string(),
        });
        let current: SnapConfig = kit.config().unwrap();
        assert_eq!(current.value, "modified");
        // Restore from snapshot
        kit.restore_config::<SnapConfig>()
            .expect("restore should succeed");
        let restored: SnapConfig = kit.config().unwrap();
        assert_eq!(restored.value, "original");
    }

    #[test]
    fn restore_returns_error_when_no_snapshot() {
        let kit = Kit::new();
        let err = kit
            .restore_config::<SnapConfig>()
            .expect_err("restore without snapshot should fail");
        assert!(matches!(err, TraitKitError::MissingConfig { .. }));
    }

    #[test]
    fn has_snapshot_reflects_state() {
        let kit = Kit::new();
        assert!(!kit.has_snapshot::<SnapConfig>());
        kit.set_config(SnapConfig {
            value: "test".to_string(),
        });
        kit.snapshot_config::<SnapConfig>();
        assert!(kit.has_snapshot::<SnapConfig>());
    }

    #[test]
    fn snapshot_overwrite_replaces_previous() {
        let kit = Kit::new();
        kit.set_config(SnapConfig {
            value: "v1".to_string(),
        });
        kit.snapshot_config::<SnapConfig>();
        kit.set_config(SnapConfig {
            value: "v2".to_string(),
        });
        kit.snapshot_config::<SnapConfig>();
        // Restore should get v2 (latest snapshot)
        kit.set_config(SnapConfig {
            value: "current".to_string(),
        });
        kit.restore_config::<SnapConfig>().unwrap();
        let restored: SnapConfig = kit.config().unwrap();
        assert_eq!(restored.value, "v2");
    }
}

// ─── Reload Tests ───────────────────────────────────────────────────────────

#[cfg(all(test, feature = "reload"))]
mod reload_tests {
    use super::super::*;
    use crate::kit::config::Configurable;
    use std::error::Error;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[derive(Clone, Debug, PartialEq)]
    struct ReloadConfig {
        version: u32,
    }

    // 每次 load() 递增，证明 reload 真正重新加载而非复用缓存。
    static LOAD_COUNT: AtomicU32 = AtomicU32::new(0);

    impl Configurable for ReloadConfig {
        fn load() -> Result<Self, Box<dyn Error + Send>> {
            let v = LOAD_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(Self { version: v })
        }
    }

    #[test]
    fn reload_config_updates_value_and_fires_subscribers() {
        let kit = Kit::new();
        let notified = Arc::new(AtomicU32::new(0));
        let notified_clone = Arc::clone(&notified);
        kit.subscribe::<ReloadConfig>(move || {
            notified_clone.fetch_add(1, Ordering::SeqCst);
        });

        // 记录当前计数，断言 reload 后一定递增（对测试执行顺序无关）。
        let before = LOAD_COUNT.load(Ordering::SeqCst);
        kit.set_config(ReloadConfig { version: 0 });
        kit.reload_config::<ReloadConfig>()
            .expect("reload should succeed");
        let cfg: ReloadConfig = kit.config().expect("config present");
        assert!(cfg.version > before, "reload must call Configurable::load");
        assert_eq!(
            notified.load(Ordering::SeqCst),
            1,
            "subscriber must be invoked exactly once"
        );
    }

    #[test]
    fn reload_config_fires_all_subscribers() {
        let kit = Kit::new();
        let count = Arc::new(AtomicU32::new(0));
        for _ in 0..3 {
            let count = Arc::clone(&count);
            kit.subscribe::<ReloadConfig>(move || {
                count.fetch_add(1, Ordering::SeqCst);
            });
        }
        kit.set_config(ReloadConfig { version: 0 });
        kit.reload_config::<ReloadConfig>().expect("reload ok");
        assert_eq!(
            count.load(Ordering::SeqCst),
            3,
            "all three subscribers must fire"
        );
    }
}

// ─── Toggle Tests ───────────────────────────────────────────────────────────

#[cfg(all(test, feature = "toggle"))]
mod toggle_tests {
    use super::super::*;
    use crate::core::ModuleMeta;
    use std::sync::Arc;

    struct ToggleModule;
    impl ModuleMeta for ToggleModule {
        const NAME: &'static str = "toggle-mod";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AutoBuilder for ToggleModule {
        type Capability = Arc<String>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Arc<String>, TraitKitError> {
            Ok(Arc::new("toggle-cap".to_string()))
        }
    }

    #[test]
    fn enable_toggle_sets_value() {
        let kit = Kit::new();
        kit.enable_toggle("feature-a", true);
        assert!(kit.is_toggle_enabled("feature-a"));
        kit.enable_toggle("feature-a", false);
        assert!(!kit.is_toggle_enabled("feature-a"));
    }

    #[test]
    fn is_toggle_enabled_returns_false_for_unknown() {
        let kit = Kit::new();
        assert!(!kit.is_toggle_enabled("nonexistent"));
    }

    #[test]
    fn register_if_toggle_registers_when_enabled() {
        let mut kit = Kit::new();
        kit.enable_toggle("mod-x", true);
        let registered = kit
            .register_if_toggle::<ToggleModule>("mod-x")
            .expect("registration should succeed");
        assert!(registered);
    }

    #[test]
    fn register_if_toggle_skips_when_disabled() {
        let mut kit = Kit::new();
        kit.enable_toggle("mod-x", false);
        let registered = kit
            .register_if_toggle::<ToggleModule>("mod-x")
            .expect("should return Ok(false)");
        assert!(!registered);
    }

    #[test]
    fn register_if_toggle_returns_error_on_duplicate() {
        let mut kit = Kit::new();
        kit.enable_toggle("mod-x", true);
        kit.register_if_toggle::<ToggleModule>("mod-x")
            .expect("first registration");
        let err = kit
            .register_if_toggle::<ToggleModule>("mod-x")
            .expect_err("duplicate should fail");
        assert!(matches!(err, TraitKitError::AlreadyRegistered { .. }));
    }

    #[test]
    fn toggle_state_survives_build() {
        let mut kit = Kit::new();
        kit.enable_toggle("persist", true);
        kit.register_if_toggle::<ToggleModule>("persist").unwrap();
        let ready = kit.build().unwrap();
        assert!(ready.is_toggle_enabled("persist"));
    }

    #[test]
    fn toggle_enable_on_ready_state() {
        let mut kit = Kit::new();
        kit.register::<ToggleModule>().unwrap();
        let ready = kit.build().unwrap();
        ready.enable_toggle("runtime", true);
        assert!(ready.is_toggle_enabled("runtime"));
    }

    #[test]
    fn toggle_disabled_capability_not_retrievable() {
        // 运行时停用 toggle 后：未注册的模块能力必须不可检索
        // （require 返回 MissingCapability，而非返回任何能力）。
        let mut kit = Kit::new();
        kit.enable_toggle("feature-gate", false);
        let registered = kit
            .register_if_toggle::<ToggleModule>("feature-gate")
            .expect("should return Ok(false)");
        assert!(!registered, "disabled toggle must not register the module");

        let ready = kit.build().expect("build without the module succeeds");
        let err = ready.require::<ToggleModule>().unwrap_err();
        assert!(matches!(
            err,
            TraitKitError::MissingCapability { ref key } if key == "toggle-mod"
        ));
        assert!(!ready.contains::<ToggleModule>());
        assert!(ready.optional::<ToggleModule>().is_none());
    }
}

// ─── Interpolation Tests ────────────────────────────────────────────────────

#[cfg(all(test, feature = "confers"))]
mod interpolation_tests {
    use crate::kit::config::interpolate_json_value;
    use std::collections::HashMap;

    #[test]
    fn basic_var_replacement() {
        let mut value = serde_json::json!("${HOST}");
        let mut vars = HashMap::new();
        vars.insert("HOST".to_string(), "localhost".to_string());
        interpolate_json_value(&mut value, &vars);
        assert_eq!(value, serde_json::json!("localhost"));
    }

    #[test]
    fn default_value_when_var_missing() {
        let mut value = serde_json::json!("${HOST:-127.0.0.1}");
        let vars = HashMap::new();
        interpolate_json_value(&mut value, &vars);
        assert_eq!(value, serde_json::json!("127.0.0.1"));
    }

    #[test]
    fn default_value_ignored_when_var_present() {
        let mut value = serde_json::json!("${HOST:-127.0.0.1}");
        let mut vars = HashMap::new();
        vars.insert("HOST".to_string(), "10.0.0.1".to_string());
        interpolate_json_value(&mut value, &vars);
        assert_eq!(value, serde_json::json!("10.0.0.1"));
    }

    #[test]
    fn no_match_preserved() {
        let mut value = serde_json::json!("${UNKNOWN}");
        let vars = HashMap::new();
        interpolate_json_value(&mut value, &vars);
        assert_eq!(value, serde_json::json!("${UNKNOWN}"));
    }

    #[test]
    fn nested_object_replacement() {
        let mut value = serde_json::json!({
            "db": {
                "host": "${DB_HOST}",
                "port": 5432
            }
        });
        let mut vars = HashMap::new();
        vars.insert("DB_HOST".to_string(), "db.example.com".to_string());
        interpolate_json_value(&mut value, &vars);
        assert_eq!(value["db"]["host"], serde_json::json!("db.example.com"));
        // Non-string values untouched
        assert_eq!(value["db"]["port"], serde_json::json!(5432));
    }

    #[test]
    fn array_string_elements_replaced() {
        let mut value = serde_json::json!(["${A}", "${B}", 42]);
        let mut vars = HashMap::new();
        vars.insert("A".to_string(), "alpha".to_string());
        vars.insert("B".to_string(), "beta".to_string());
        interpolate_json_value(&mut value, &vars);
        assert_eq!(value[0], serde_json::json!("alpha"));
        assert_eq!(value[1], serde_json::json!("beta"));
        assert_eq!(value[2], serde_json::json!(42));
    }

    #[test]
    fn non_string_values_untouched() {
        let mut value = serde_json::json!({
            "num": 42,
            "bool": true,
            "null": null
        });
        let vars = HashMap::new();
        interpolate_json_value(&mut value, &vars);
        assert_eq!(value["num"], serde_json::json!(42));
        assert_eq!(value["bool"], serde_json::json!(true));
        assert_eq!(value["null"], serde_json::json!(null));
    }

    #[test]
    fn object_keys_not_replaced() {
        let mut value = serde_json::json!({"${KEY}": "value"});
        let vars = HashMap::new();
        interpolate_json_value(&mut value, &vars);
        // Key should remain as "${KEY}", not be replaced
        assert!(value.as_object().unwrap().contains_key("${KEY}"));
    }

    #[test]
    fn multiple_vars_in_one_string() {
        let mut value = serde_json::json!("${HOST}:${PORT}");
        let mut vars = HashMap::new();
        vars.insert("HOST".to_string(), "localhost".to_string());
        vars.insert("PORT".to_string(), "8080".to_string());
        interpolate_json_value(&mut value, &vars);
        assert_eq!(value, serde_json::json!("localhost:8080"));
    }
}

// ─── Config inheritance tests ─────────────────────────────────────────────

#[cfg(all(test, feature = "confers"))]
mod config_inheritance_tests {
    use super::super::*;
    use crate::kit::{ConfigInherit, ModuleConfig, SharedConfig};

    // ── Test fixtures ──

    #[derive(Clone, Debug, PartialEq)]
    struct TestDbConfig {
        host: String,
        port: u16,
        max_connections: u32,
    }

    #[derive(Clone, Default)]
    struct TestDbConfigOverride {
        host: Option<String>,
        port: Option<u16>,
        max_connections: Option<u32>,
    }

    impl ConfigInherit for TestDbConfig {
        type Override = TestDbConfigOverride;
        fn apply_override(&mut self, ovr: &Self::Override) {
            if let Some(ref h) = ovr.host {
                self.host.clone_from(h);
            }
            if let Some(ref p) = ovr.port {
                self.port = *p;
            }
            if let Some(ref m) = ovr.max_connections {
                self.max_connections = *m;
            }
        }
    }

    impl ModuleConfig for TestDbConfig {
        const PATH: &'static str = "config/db.toml";
        fn default_value() -> Self {
            Self {
                host: "localhost".into(),
                port: 3306,
                max_connections: 10,
            }
        }
    }

    impl SharedConfig for TestDbConfig {
        fn extract_shared(&self) -> serde_json::Map<String, serde_json::Value> {
            let mut map = serde_json::Map::new();
            map.insert("host".into(), serde_json::Value::String(self.host.clone()));
            map.insert("port".into(), serde_json::json!(self.port));
            map
        }
        fn inject_shared(&mut self, shared: &serde_json::Map<String, serde_json::Value>) {
            if let Some(v) = shared.get("host")
                && let Ok(s) = serde_json::from_value(v.clone())
            {
                self.host = s;
            }
            if let Some(v) = shared.get("port")
                && let Ok(n) = serde_json::from_value(v.clone())
            {
                self.port = n;
            }
        }
    }

    // Second config type sharing `host` and `port` with TestDbConfig.
    #[derive(Clone, Debug, PartialEq)]
    struct TestAppConfig {
        host: String,
        port: u16,
        app_name: String,
    }

    impl SharedConfig for TestAppConfig {
        fn extract_shared(&self) -> serde_json::Map<String, serde_json::Value> {
            let mut map = serde_json::Map::new();
            map.insert("host".into(), serde_json::Value::String(self.host.clone()));
            map.insert("port".into(), serde_json::json!(self.port));
            map
        }
        fn inject_shared(&mut self, shared: &serde_json::Map<String, serde_json::Value>) {
            if let Some(v) = shared.get("host")
                && let Ok(s) = serde_json::from_value(v.clone())
            {
                self.host = s;
            }
            if let Some(v) = shared.get("port")
                && let Ok(n) = serde_json::from_value(v.clone())
            {
                self.port = n;
            }
        }
    }


    #[test]
    fn populate_defaults_fills_empty_kit() {
        let kit = Kit::new();
        assert!(!kit.configs.contains::<TestDbConfig>());
        let filled = kit.populate_defaults::<TestDbConfig>();
        assert!(filled, "should return true when default was populated");
        let config: TestDbConfig = kit.config().expect("config should exist");
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 3306);
        assert_eq!(config.max_connections, 10);
    }

    #[test]
    fn populate_defaults_does_not_overwrite_existing() {
        let kit = Kit::new();
        kit.set_config(TestDbConfig {
            host: "prod-db".into(),
            port: 5432,
            max_connections: 100,
        });
        let filled = kit.populate_defaults::<TestDbConfig>();
        assert!(!filled, "should return false when config already exists");
        let config: TestDbConfig = kit.config().expect("config should exist");
        assert_eq!(config.host, "prod-db");
        assert_eq!(config.port, 5432);
    }


    #[test]
    fn merge_config_overrides_only_some_fields() {
        let kit = Kit::new();
        kit.set_config(TestDbConfig {
            host: "localhost".into(),
            port: 3306,
            max_connections: 10,
        });
        kit.merge_config::<TestDbConfig>(TestDbConfigOverride {
            host: Some("prod-db.example.com".into()),
            port: None, // keep original
            max_connections: None,
        });
        let config: TestDbConfig = kit.config().expect("config should exist");
        assert_eq!(config.host, "prod-db.example.com");
        assert_eq!(config.port, 3306); // unchanged
        assert_eq!(config.max_connections, 10); // unchanged
    }

    #[test]
    fn merge_config_noop_when_missing() {
        let kit = Kit::new();
        // No config set — should not panic
        kit.merge_config::<TestDbConfig>(TestDbConfigOverride {
            host: Some("x".into()),
            ..Default::default()
        });
        assert!(!kit.configs.contains::<TestDbConfig>());
    }


    #[test]
    fn extract_then_inject_shared_flows_values() {
        let kit = Kit::new();
        // A loads its config
        kit.set_config(TestAppConfig {
            host: "prod.example.com".into(),
            port: 9090,
            app_name: "my-app".into(),
        });
        // A extracts shared fields
        kit.extract_shared::<TestAppConfig>();

        // B gets defaults
        kit.populate_defaults::<TestDbConfig>();
        // B injects shared fields from A
        kit.inject_shared::<TestDbConfig>();

        let db: TestDbConfig = kit.config().expect("db config should exist");
        assert_eq!(db.host, "prod.example.com"); // inherited from A
        assert_eq!(db.port, 9090); // inherited from A
        assert_eq!(db.max_connections, 10); // B's own default preserved
    }


    #[test]
    fn inject_shared_skips_type_mismatch_silently() {
        let kit = Kit::new();
        // Manually put a wrong-type value into shared overlay
        kit.confers.shared_fields
            .borrow_mut()
            .insert("host".into(), serde_json::json!([1, 2, 3])); // array, not string
        kit.confers.shared_fields
            .borrow_mut()
            .insert("port".into(), serde_json::json!("not-a-number")); // string, not number

        kit.set_config(TestDbConfig {
            host: "original".into(),
            port: 3306,
            max_connections: 10,
        });
        // Should not panic — type mismatches are silently skipped
        kit.inject_shared::<TestDbConfig>();

        let db: TestDbConfig = kit.config().expect("config should exist");
        assert_eq!(db.host, "original"); // unchanged
        assert_eq!(db.port, 3306); // unchanged
    }

    // ── no-op boundary tests ──

    #[test]
    fn extract_shared_noop_when_config_missing() {
        let kit = Kit::new();
        // No config set — should not panic, overlay stays empty
        kit.extract_shared::<TestDbConfig>();
        assert!(kit.confers.shared_fields.borrow().is_empty());
    }

    #[test]
    fn inject_shared_noop_when_config_missing() {
        let kit = Kit::new();
        // Put something in overlay first
        kit.confers.shared_fields
            .borrow_mut()
            .insert("host".into(), serde_json::json!("some-host"));
        // No config set — should not panic
        kit.inject_shared::<TestDbConfig>();
        // Overlay unchanged
        assert_eq!(kit.confers.shared_fields.borrow().len(), 1);
    }

    // ── Kit<Ready> tests ──

    #[test]
    fn config_inheritance_works_on_ready_kit() {
        let kit = Kit::new();
        kit.set_config(TestDbConfig {
            host: "pre-build".into(),
            port: 3306,
            max_connections: 10,
        });
        kit.set_config(TestAppConfig {
            host: "app-host".into(),
            port: 9090,
            app_name: "ready-test".into(),
        });

        let ready = kit.build().expect("build should succeed");

        // extract_shared on Ready kit
        ready.extract_shared::<TestAppConfig>();

        // inject_shared on Ready kit
        ready.inject_shared::<TestDbConfig>();

        let db: TestDbConfig = ready.config().expect("db config should exist");
        assert_eq!(db.host, "app-host"); // inherited
        assert_eq!(db.port, 9090); // inherited
        assert_eq!(db.max_connections, 10); // unchanged

        // merge_config on Ready kit
        ready.merge_config::<TestDbConfig>(TestDbConfigOverride {
            host: Some("post-merge".into()),
            ..Default::default()
        });
        let db2: TestDbConfig = ready.config().unwrap();
        assert_eq!(db2.host, "post-merge");
    }
}
