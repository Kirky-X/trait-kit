use std::sync::Arc;

use static_assertions::assert_not_impl_any;
use trait_kit::prelude::*;

// Compile-time guarantee: Kit is !Sync by design (uses RefCell for interior
// mutability on single-threaded typestate builds).
assert_not_impl_any!(Kit<Unbuilt>: Sync);
assert_not_impl_any!(Kit<Ready>: Sync);

// === Logger module (no dependencies) ===

struct StdoutLogger;
impl StdoutLogger {
    fn info(&self, msg: &str) {
        println!("[LOG] {msg}");
    }
}

struct LoggerModule;
impl ModuleMeta for LoggerModule {
    const NAME: &'static str = "logger";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        &[]
    }
}
impl AutoBuilder for LoggerModule {
    type Capability = Arc<StdoutLogger>;
    type Error = KitError;

    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(StdoutLogger))
    }
}

// === Config ===

#[derive(Clone, Debug)]
struct DbConfig {
    url: String,
    max_connections: u32,
}

// === DbPool module (depends on Logger) ===

struct DbPool {
    _logger: Arc<StdoutLogger>,
    config: DbConfig,
}

struct DbPoolModule;
impl ModuleMeta for DbPoolModule {
    const NAME: &'static str = "db_pool";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        static DEPS: &[(&str, std::any::TypeId)] =
            &[("logger", std::any::TypeId::of::<LoggerModule>())];
        DEPS
    }
}
impl AutoBuilder for DbPoolModule {
    type Capability = Arc<DbPool>;
    type Error = KitError;

    fn build(kit: &Kit) -> Result<Self::Capability, Self::Error> {
        let logger = kit.require::<LoggerModule>()?;
        let config: DbConfig = kit.config()?;
        Ok(Arc::new(DbPool {
            _logger: logger,
            config,
        }))
    }
}

#[test]
fn test_basic_build_and_require() {
    let mut kit = Kit::new();
    kit.register::<LoggerModule>().unwrap();
    let kit = kit.build().unwrap();

    let logger = kit.require::<LoggerModule>().unwrap();
    logger.info("hello from test");
    assert!(kit.contains::<LoggerModule>());
}

#[test]
fn test_dependency_resolution() {
    let mut kit = Kit::new();
    kit.set_config(DbConfig {
        url: "postgres://localhost".into(),
        max_connections: 10,
    });
    kit.register::<LoggerModule>().unwrap();
    kit.register::<DbPoolModule>().unwrap();
    let kit = kit.build().unwrap();

    let pool = kit.require::<DbPoolModule>().unwrap();
    assert_eq!(pool.config.max_connections, 10);
}

#[test]
fn test_missing_config_error() {
    let mut kit = Kit::new();
    kit.register::<LoggerModule>().unwrap();
    kit.register::<DbPoolModule>().unwrap();
    let result = kit.build();

    assert!(result.is_err());
    match result.unwrap_err() {
        KitError::BuildFailed { module, .. } => assert_eq!(module, "db_pool"),
        other => panic!("expected BuildFailed, got: {other}"),
    }
}

#[test]
fn test_missing_dependency_error() {
    let mut kit = Kit::new();
    kit.register::<DbPoolModule>().unwrap();
    let result = kit.build();

    assert!(result.is_err());
    match result.unwrap_err() {
        KitError::DependencyMissing { module, missing } => {
            assert_eq!(module, "db_pool");
            assert_eq!(missing, "logger");
        }
        other => panic!("expected DependencyMissing, got: {other}"),
    }
}

#[test]
fn test_duplicate_registration_error() {
    let mut kit = Kit::new();
    kit.register::<LoggerModule>().unwrap();
    let result = kit.register::<LoggerModule>();

    assert!(result.is_err());
    match result.unwrap_err() {
        KitError::AlreadyRegistered { module } => assert_eq!(module, "logger"),
        other => panic!("expected AlreadyRegistered, got: {other}"),
    }
}

#[test]
fn test_config_retrieval() {
    let mut kit = Kit::new();
    kit.set_config(DbConfig {
        url: "postgres://localhost".into(),
        max_connections: 5,
    });
    kit.register::<LoggerModule>().unwrap();
    let kit = kit.build().unwrap();

    let config: DbConfig = kit.config().unwrap();
    assert_eq!(config.max_connections, 5);
}

#[test]
fn test_missing_config_retrieval() {
    let mut kit = Kit::new();
    kit.register::<LoggerModule>().unwrap();
    let kit = kit.build().unwrap();

    let result = kit.config::<DbConfig>();
    assert!(result.is_err());
}

#[test]
fn test_optional_missing() {
    let mut kit = Kit::new();
    kit.register::<LoggerModule>().unwrap();
    let kit = kit.build().unwrap();

    let result = kit.optional::<DbPoolModule>();
    assert!(result.is_none());
}

#[test]
fn test_cycle_detection() {
    // This test needs two modules that depend on each other.
    // We can't easily create a cycle with ModuleMeta since dependencies
    // are static. Instead, test that the graph validator catches cycles.
    use trait_kit::kit::graph::DependencyGraph;
    use trait_kit::kit::graph::ModuleEntry;

    let mut graph = DependencyGraph::new();
    graph
        .add(ModuleEntry {
            type_id: std::any::TypeId::of::<LoggerModule>(),
            name: "a",
            dependencies: vec![("b", std::any::TypeId::of::<DbPoolModule>())],
        })
        .unwrap();
    graph
        .add(ModuleEntry {
            type_id: std::any::TypeId::of::<DbPoolModule>(),
            name: "b",
            dependencies: vec![("a", std::any::TypeId::of::<LoggerModule>())],
        })
        .unwrap();

    let result = graph.validate();
    assert!(result.is_err());
}

#[test]
fn kit_error_display_and_source_behavior() {
    use std::error::Error;

    // Display: NotReady
    assert_eq!(
        KitError::NotReady.to_string(),
        "kit is not ready; call build() first"
    );

    // Display: CycleDetected
    let cycle = KitError::CycleDetected {
        cycle: vec!["a", "b", "a"],
    };
    assert_eq!(cycle.to_string(), "dependency cycle detected: a → b → a");

    // Display: DependencyMissing
    let dep = KitError::DependencyMissing {
        module: "db",
        missing: "logger",
    };
    assert_eq!(
        dep.to_string(),
        "module `db` depends on `logger` which is not registered"
    );

    // Display: AlreadyRegistered
    let dup = KitError::AlreadyRegistered { module: "logger" };
    assert_eq!(dup.to_string(), "module `logger` is already registered");

    // Display: MissingCapability
    let cap = KitError::MissingCapability { key: "logger" };
    assert_eq!(cap.to_string(), "missing capability `logger`");

    // Display: MissingConfig
    let cfg = KitError::MissingConfig { key: "db_url" };
    assert_eq!(cfg.to_string(), "missing config `db_url`");

    // Display: BuildFailed (contains source message)
    let source: Box<dyn Error + Send + Sync> = "inner failure".into();
    let build = KitError::BuildFailed {
        module: "db",
        source,
    };
    assert!(build.to_string().contains("failed to build module `db`"));
    assert!(build.to_string().contains("inner failure"));

    // Error::source() for BuildFailed returns Some
    assert!(build.source().is_some());

    // Error::source() for other variants returns None
    assert!(KitError::NotReady.source().is_none());
    assert!(cycle.source().is_none());
    assert!(dep.source().is_none());
}

// === Configurable trait + load_config (confers feature) ===

#[cfg(feature = "confers")]
mod confers_loader {
    use std::error::Error;
    use trait_kit::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct StubConfig {
        value: u32,
    }

    impl Configurable for StubConfig {
        fn load() -> Result<Self, Box<dyn Error>> {
            Ok(Self { value: 42 })
        }
    }

    #[test]
    fn load_config_stores_value_when_load_succeeds() {
        let mut kit = Kit::new();
        kit.load_config::<StubConfig>()
            .expect("load should succeed");
        let kit = kit.build().expect("build should succeed");

        assert!(kit.contains_config::<StubConfig>());
        let stored: StubConfig = kit.config().expect("config should be retrievable");
        assert_eq!(stored.value, 42);
    }

    #[derive(Clone, Debug)]
    struct FailingConfig;

    impl Configurable for FailingConfig {
        fn load() -> Result<Self, Box<dyn Error>> {
            Err("intentional load failure".into())
        }
    }

    #[test]
    fn load_config_propagates_error_when_load_fails() {
        let mut kit = Kit::new();
        let result = kit.load_config::<FailingConfig>();

        match result {
            Err(KitError::BuildFailed { module, source }) => {
                assert_eq!(module, "load_config");
                assert!(source.to_string().contains("intentional load failure"));
            }
            other => panic!("expected BuildFailed, got: {other:?}"),
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct OverridableConfig {
        value: &'static str,
    }

    impl Configurable for OverridableConfig {
        fn load() -> Result<Self, Box<dyn Error>> {
            Ok(Self { value: "loaded" })
        }
    }

    #[test]
    fn load_config_overrides_prior_set_config() {
        let mut kit = Kit::new();
        kit.set_config(OverridableConfig { value: "initial" });
        kit.load_config::<OverridableConfig>()
            .expect("load should override prior value");
        let kit = kit.build().expect("build should succeed");

        let stored: OverridableConfig = kit.config().expect("config should be retrievable");
        assert_eq!(stored.value, "loaded");
    }
}
