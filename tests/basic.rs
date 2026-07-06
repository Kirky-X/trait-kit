use std::sync::Arc;

use trait_kit::prelude::*;

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
        static DEPS: &[(&str, std::any::TypeId)] = &[("logger", std::any::TypeId::of::<LoggerModule>())];
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
    assert_eq!(
        cycle.to_string(),
        "dependency cycle detected: a → b → a"
    );

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
