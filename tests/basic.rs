// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
use std::sync::Arc;

use static_assertions::assert_not_impl_any;
use trait_kit::prelude::*;

// Compile-time guarantee: Kit is !Sync by design (uses RefCell for interior
// mutability on single-threaded typestate builds).
assert_not_impl_any!(Kit<Unbuilt>: Sync);
assert_not_impl_any!(Kit<Ready>: Sync);

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

#[derive(Clone, Debug)]
struct DbConfig {
    #[allow(dead_code)]
    url: String,
    max_connections: u32,
}

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
        KitError::BuildFailed { context, .. } => assert_eq!(context, "db_pool"),
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

    // Display: NotReady (deprecated — typestate pattern makes it unreachable)
    #[allow(deprecated)]
    let not_ready = KitError::NotReady;
    assert_eq!(
        not_ready.to_string(),
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
        context: "db",
        source,
    };
    assert!(build.to_string().contains("failed to build `db`"));
    assert!(build.to_string().contains("inner failure"));

    // Error::source() for BuildFailed returns Some
    assert!(build.source().is_some());

    // Error::source() for other variants returns None
    #[allow(deprecated)]
    {
        assert!(KitError::NotReady.source().is_none());
    }
    assert!(cycle.source().is_none());
    assert!(dep.source().is_none());
}

#[cfg(feature = "confers")]
mod confers_loader {
    use std::error::Error;
    use trait_kit::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct StubConfig {
        value: u32,
    }

    impl Configurable for StubConfig {
        fn load() -> Result<Self, Box<dyn Error + Send>> {
            Ok(Self { value: 42 })
        }
    }

    #[test]
    fn load_config_stores_value_when_load_succeeds() {
        let kit = Kit::new();
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
        fn load() -> Result<Self, Box<dyn Error + Send>> {
            Err(Box::new(std::io::Error::other("intentional load failure")))
        }
    }

    #[test]
    fn load_config_propagates_error_when_load_fails() {
        let kit = Kit::new();
        let result = kit.load_config::<FailingConfig>();

        match result {
            Err(KitError::BuildFailed { context, source }) => {
                assert_eq!(context, "load_config");
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
        fn load() -> Result<Self, Box<dyn Error + Send>> {
            Ok(Self { value: "loaded" })
        }
    }

    #[test]
    fn load_config_overrides_prior_set_config() {
        let kit = Kit::new();
        kit.set_config(OverridableConfig { value: "initial" });
        kit.load_config::<OverridableConfig>()
            .expect("load should override prior value");
        let kit = kit.build().expect("build should succeed");

        let stored: OverridableConfig = kit.config().expect("config should be retrievable");
        assert_eq!(stored.value, "loaded");
    }
}

#[cfg(feature = "confers")]
mod confers_derive_bridge {
    use serial_test::serial;
    use std::error::Error;
    use trait_kit::prelude::*;

    #[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, confers::Config)]
    #[config(env_prefix = "TRAIT_KIT_T026_")]
    struct DerivedConfig {
        #[config(default = "fallback_value".to_string())]
        field: String,
    }

    impl Configurable for DerivedConfig {
        fn load() -> Result<Self, Box<dyn Error + Send>> {
            match DerivedConfig::load_sync() {
                Ok(c) => Ok(c),
                Err(e) => Err(Box::new(std::io::Error::other(e.to_string()))),
            }
        }
    }

    #[test]
    #[serial]
    fn load_config_bridges_to_confers_derive_load_sync() {
        unsafe { std::env::remove_var("TRAIT_KIT_T026_FIELD") };

        let kit = Kit::new();
        kit.load_config::<DerivedConfig>()
            .expect("load should succeed via confers derive load_sync()");
        let kit = kit.build().expect("build should succeed");

        let config: DerivedConfig = kit.config().expect("config should be retrievable");
        assert_eq!(config.field, "fallback_value");
        drop(kit);

        unsafe { std::env::set_var("TRAIT_KIT_T026_FIELD", "from_env") };
        let kit = Kit::new();
        let result = kit.load_config::<DerivedConfig>();
        unsafe { std::env::remove_var("TRAIT_KIT_T026_FIELD") };

        let kit = match result {
            Ok(()) => kit.build().expect("build should succeed"),
            Err(e) => panic!("load_config failed: {e:?}"),
        };
        let config: DerivedConfig = kit.config().expect("config should be retrievable");
        assert_eq!(config.field, "from_env");
    }
}

#[cfg(feature = "confers-macros")]
mod module_config_trait {
    use trait_kit::kit::config::ModuleConfig;
    use trait_kit::kit::Config;

    #[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Config)]
    struct ModuleStub {
        #[config(default = "stub".to_string())]
        name: String,
    }

    impl ModuleConfig for ModuleStub {
        const PATH: &'static str = "config/module_stub.toml";

        fn default_value() -> Self {
            Self {
                name: "default".to_string(),
            }
        }
    }

    #[test]
    fn module_config_trait_requires_path_and_default() {
        assert_eq!(ModuleStub::PATH, "config/module_stub.toml");
        let default = ModuleStub::default_value();
        assert_eq!(default.name, "default");
    }

    #[test]
    fn derive_config_macro_re_exported() {
        // If this compiles, `use trait_kit::kit::Config;` succeeded.
        let _ = std::marker::PhantomData::<ModuleStub>;
    }
}

#[cfg(feature = "hot-reload")]
mod hot_reload {
    use std::cell::Cell;
    use std::error::Error;
    use std::rc::Rc;
    use trait_kit::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct ReloadableConfig {
        value: u32,
    }

    impl Configurable for ReloadableConfig {
        fn load() -> Result<Self, Box<dyn Error + Send>> {
            Ok(Self { value: 99 })
        }
    }

    #[test]
    fn subscribe_callback_invoked_on_reload() {
        let kit = Kit::new();
        let called = Rc::new(Cell::new(false));
        let called_clone = Rc::clone(&called);
        kit.subscribe::<ReloadableConfig>(move || {
            called_clone.set(true);
        });

        kit.reload_config::<ReloadableConfig>()
            .expect("reload should succeed");
        let kit = kit.build().expect("build should succeed");

        assert!(called.get(), "callback should have been invoked");
        let config: ReloadableConfig = kit.config().expect("config should be retrievable");
        assert_eq!(config.value, 99);
    }

    #[test]
    fn reload_config_updates_stored_value() {
        let kit = Kit::new();
        kit.set_config(ReloadableConfig { value: 1 });
        kit.reload_config::<ReloadableConfig>()
            .expect("reload should override prior value");
        let kit = kit.build().expect("build should succeed");

        let config: ReloadableConfig = kit.config().expect("config should be retrievable");
        assert_eq!(config.value, 99);
    }
}

#[cfg(feature = "encryption")]
mod encryption {
    use trait_kit::kit::config::ModuleConfig;
    use trait_kit::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    struct SecretConfig {
        api_key: String,
        port: u16,
    }

    impl ModuleConfig for SecretConfig {
        const PATH: &'static str = "config/secret.toml";

        fn default_value() -> Self {
            Self {
                api_key: "default_key".to_string(),
                port: 8080,
            }
        }
    }

    // 32-byte master key for XChaCha20-Poly1305 + HKDF.
    // pragma: allowlist secret
    const MASTER_KEY: [u8; 32] = *b"0123456789abcdef0123456789abcdef";

    #[test]
    fn encrypted_config_roundtrip() {
        let kit = Kit::new();
        let original = SecretConfig {
            api_key: "sk-12345".to_string(),
            port: 5432,
        };
        kit.set_encrypted(&original, &MASTER_KEY)
            .expect("encrypt should succeed");
        assert!(kit.contains_encrypted::<SecretConfig>());
        let kit = kit.build().expect("build should succeed");

        let decrypted: SecretConfig = kit
            .get_encrypted(&MASTER_KEY)
            .expect("decrypt should succeed");
        assert_eq!(decrypted, original);
    }

    #[test]
    fn get_encrypted_fails_with_wrong_key() {
        let kit = Kit::new();
        let original = SecretConfig {
            api_key: "sk-12345".to_string(),
            port: 5432,
        };
        kit.set_encrypted(&original, &MASTER_KEY)
            .expect("encrypt should succeed");
        let kit = kit.build().expect("build should succeed");

        let wrong_key = *b"fedcba9876543210fedcba9876543210";
        let result: Result<SecretConfig, _> = kit.get_encrypted(&wrong_key);
        assert!(
            result.is_err(),
            "decryption with wrong key should fail (proves real encryption, not plaintext storage)"
        );
    }

    #[test]
    fn get_encrypted_returns_missing_config_error_when_not_set() {
        let kit = Kit::new();
        let kit = kit.build().expect("build should succeed");

        let result: Result<SecretConfig, _> = kit.get_encrypted(&MASTER_KEY);
        assert!(result.is_err());
        match result.unwrap_err() {
            KitError::MissingConfig { key } => {
                assert!(key.contains("SecretConfig"));
            }
            other => panic!("expected MissingConfig, got: {other:?}"),
        }
    }

    #[test]
    fn set_encrypted_overwrites_prior_value() {
        let kit = Kit::new();
        kit.set_encrypted(
            &SecretConfig {
                api_key: "old_key".to_string(),
                port: 1111,
            },
            &MASTER_KEY,
        )
        .expect("first encrypt should succeed");
        kit.set_encrypted(
            &SecretConfig {
                api_key: "new_key".to_string(),
                port: 2222,
            },
            &MASTER_KEY,
        )
        .expect("overwrite should succeed");
        let kit = kit.build().expect("build should succeed");

        let decrypted: SecretConfig = kit
            .get_encrypted(&MASTER_KEY)
            .expect("decrypt should return latest value");
        assert_eq!(decrypted.api_key, "new_key");
        assert_eq!(decrypted.port, 2222);
    }

    #[test]
    fn encrypted_storage_is_separate_from_plaintext_typemap() {
        // set_encrypted must NOT populate the plaintext TypeMap — the value
        // should only exist in encrypted_configs, retrievable solely via
        // get_encrypted with the correct master key.
        let kit = Kit::new();
        kit.set_encrypted(
            &SecretConfig {
                api_key: "sk-12345".to_string(),
                port: 5432,
            },
            &MASTER_KEY,
        )
        .expect("encrypt should succeed");
        assert!(kit.contains_encrypted::<SecretConfig>());
        let kit = kit.build().expect("build should succeed");
        // Plaintext TypeMap should NOT contain the config.
        assert!(!kit.contains_config::<SecretConfig>());
        assert!(kit.config::<SecretConfig>().is_err());
    }

    #[test]
    fn encrypted_blob_getters_via_roundtrip() {
        // The pub(crate) fields can't be constructed directly from
        // integration tests; verify getters indirectly by confirming
        // set_encrypted + get_encrypted round-trips (which exercises
        // nonce()/ciphertext() inside Kit::get_encrypted).
        let kit = Kit::new();
        kit.set_encrypted(
            &SecretConfig {
                api_key: "sk-12345".to_string(),
                port: 5432,
            },
            &MASTER_KEY,
        )
        .expect("encrypt should succeed");
        let kit = kit.build().expect("build should succeed");
        let ok: Result<SecretConfig, _> = kit.get_encrypted(&MASTER_KEY);
        assert!(ok.is_ok());
    }

    #[test]
    fn set_encrypted_propagates_serialization_error() {
        use trait_kit::kit::config::ModuleConfig;

        #[derive(Clone)]
        struct Unserializable;
        impl serde::Serialize for Unserializable {
            fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                Err(serde::ser::Error::custom("intentional serialize failure"))
            }
        }
        impl ModuleConfig for Unserializable {
            const PATH: &'static str = "config/unserializable.toml";
            fn default_value() -> Self {
                Self
            }
        }

        let kit = Kit::new();
        let result = kit.set_encrypted(&Unserializable, &MASTER_KEY);
        match result {
            Err(KitError::BuildFailed { context, source }) => {
                assert_eq!(context, "set_encrypted");
                assert!(source.to_string().contains("intentional serialize failure"));
            }
            other => panic!("expected BuildFailed, got: {other:?}"),
        }
    }
}

#[cfg(test)]
mod graph_coverage {
    use std::any::TypeId;
    use trait_kit::kit::graph::{DependencyGraph, GraphError, ModuleEntry};

    fn entry(name: &'static str, deps: Vec<(&'static str, TypeId)>) -> ModuleEntry {
        ModuleEntry {
            type_id: TypeId::of::<()>(),
            name,
            dependencies: deps,
        }
    }

    // Use distinct placeholder types so each entry has a unique TypeId.
    struct A;
    struct B;
    struct C;

    #[test]
    fn name_of_returns_registered_name() {
        let mut g = DependencyGraph::new();
        g.add(ModuleEntry {
            type_id: TypeId::of::<A>(),
            name: "a",
            dependencies: vec![],
        })
        .unwrap();
        assert_eq!(g.name_of(TypeId::of::<A>()), Some("a"));
        assert_eq!(g.name_of(TypeId::of::<B>()), None);
    }

    #[test]
    fn dependency_names_returns_registered_deps() {
        let mut g = DependencyGraph::new();
        g.add(ModuleEntry {
            type_id: TypeId::of::<A>(),
            name: "a",
            dependencies: vec![("b", TypeId::of::<B>())],
        })
        .unwrap();
        g.add(ModuleEntry {
            type_id: TypeId::of::<B>(),
            name: "b",
            dependencies: vec![],
        })
        .unwrap();
        assert_eq!(g.dependency_names(TypeId::of::<A>()), vec!["b"]);
        assert_eq!(g.dependency_names(TypeId::of::<B>()), Vec::<&str>::new());
        // Unknown type returns empty.
        assert_eq!(g.dependency_names(TypeId::of::<C>()), Vec::<&str>::new());
    }

    #[test]
    fn entries_returns_registration_order() {
        let mut g = DependencyGraph::new();
        g.add(ModuleEntry {
            type_id: TypeId::of::<A>(),
            name: "a",
            dependencies: vec![],
        })
        .unwrap();
        g.add(ModuleEntry {
            type_id: TypeId::of::<B>(),
            name: "b",
            dependencies: vec![],
        })
        .unwrap();
        let names: Vec<_> = g.entries().iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["a", "b"]);
    }

    #[test]
    fn default_creates_empty_graph() {
        let g = DependencyGraph::default();
        assert!(g.entries().is_empty());
    }

    #[test]
    fn add_rejects_duplicate_type_id() {
        let mut g = DependencyGraph::new();
        g.add(ModuleEntry {
            type_id: TypeId::of::<A>(),
            name: "a",
            dependencies: vec![],
        })
        .unwrap();
        let err = g
            .add(ModuleEntry {
                type_id: TypeId::of::<A>(),
                name: "a_dup",
                dependencies: vec![],
            })
            .unwrap_err();
        assert_eq!(err, "a_dup");
    }

    #[test]
    fn validate_returns_dependency_missing_for_unknown_dep() {
        let mut g = DependencyGraph::new();
        g.add(ModuleEntry {
            type_id: TypeId::of::<A>(),
            name: "a",
            dependencies: vec![("missing", TypeId::of::<B>())],
        })
        .unwrap();
        match g.validate() {
            Err(GraphError::DependencyMissing { module, missing }) => {
                assert_eq!(module, "a");
                assert_eq!(missing, "missing");
            }
            other => panic!("expected DependencyMissing, got: {other:?}"),
        }
    }

    #[test]
    fn find_cycle_traverses_unvisited_branch() {
        // Graph: a → b → c → b (cycle between b and c, a leads in)
        // This exercises the `if visited[dep_idx] == 0 && dfs(...)` true branch.
        let mut g = DependencyGraph::new();
        g.add(ModuleEntry {
            type_id: TypeId::of::<A>(),
            name: "a",
            dependencies: vec![("b", TypeId::of::<B>())],
        })
        .unwrap();
        g.add(ModuleEntry {
            type_id: TypeId::of::<B>(),
            name: "b",
            dependencies: vec![("c", TypeId::of::<C>())],
        })
        .unwrap();
        g.add(ModuleEntry {
            type_id: TypeId::of::<C>(),
            name: "c",
            dependencies: vec![("b", TypeId::of::<B>())], // back-edge to b
        })
        .unwrap();

        match g.validate() {
            Err(GraphError::CycleDetected { cycle }) => {
                assert!(cycle.contains(&"b"));
                assert!(cycle.contains(&"c"));
            }
            other => panic!("expected CycleDetected, got: {other:?}"),
        }
    }

    #[test]
    fn validate_succeeds_for_acyclic_graph() {
        let mut g = DependencyGraph::new();
        g.add(ModuleEntry {
            type_id: TypeId::of::<A>(),
            name: "a",
            dependencies: vec![],
        })
        .unwrap();
        g.add(ModuleEntry {
            type_id: TypeId::of::<B>(),
            name: "b",
            dependencies: vec![("a", TypeId::of::<A>())],
        })
        .unwrap();
        assert!(g.validate().is_ok());
    }

    // Suppress unused warning for the helper.
    #[test]
    fn entry_helper_compiles() {
        let _ = entry("x", vec![]);
    }
}

mod kit_build_coverage {
    use std::sync::Arc;
    use trait_kit::prelude::*;

    struct CycleA;
    impl ModuleMeta for CycleA {
        const NAME: &'static str = "cycle_a";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            static DEPS: &[(&str, std::any::TypeId)] =
                &[("cycle_b", std::any::TypeId::of::<CycleB>())];
            DEPS
        }
    }
    impl AutoBuilder for CycleA {
        type Capability = Arc<()>;
        type Error = KitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(()))
        }
    }

    struct CycleB;
    impl ModuleMeta for CycleB {
        const NAME: &'static str = "cycle_b";
        fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
            static DEPS: &[(&str, std::any::TypeId)] =
                &[("cycle_a", std::any::TypeId::of::<CycleA>())];
            DEPS
        }
    }
    impl AutoBuilder for CycleB {
        type Capability = Arc<()>;
        type Error = KitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(()))
        }
    }

    #[test]
    fn kit_build_returns_cycle_detected_for_mutual_deps() {
        let mut kit = Kit::new();
        kit.register::<CycleA>().unwrap();
        kit.register::<CycleB>().unwrap();
        match kit.build() {
            Err(KitError::CycleDetected { cycle }) => {
                assert!(cycle.contains(&"cycle_a"));
                assert!(cycle.contains(&"cycle_b"));
            }
            other => panic!("expected CycleDetected, got: {other:?}"),
        }
    }

    #[test]
    fn kit_debug_shows_module_and_config_counts() {
        let mut kit = Kit::new();
        kit.set_config(42i32);
        kit.set_config("hello".to_string());
        let s = format!("{:?}", kit);
        assert!(s.contains("Kit<Unbuilt>"));
        assert!(s.contains("modules: 0"));
        assert!(s.contains("configs: 2"));

        kit.register::<CycleA>().unwrap();
        let s2 = format!("{:?}", kit);
        assert!(s2.contains("modules: 1"));
    }

    #[test]
    fn kit_ready_debug_shows_counts() {
        // Use a non-cyclic module so build() succeeds.
        struct Solo;
        impl ModuleMeta for Solo {
            const NAME: &'static str = "solo";
            fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
                &[]
            }
        }
        impl AutoBuilder for Solo {
            type Capability = Arc<()>;
            type Error = KitError;
            fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
                Ok(Arc::new(()))
            }
        }

        let mut kit = Kit::new();
        kit.set_config(99u64);
        kit.register::<Solo>().unwrap();
        let kit = kit.build().unwrap();
        let s = format!("{:?}", kit);
        assert!(s.contains("Kit<Ready>"));
        assert!(s.contains("modules: 1"));
        assert!(s.contains("configs: 1"));
    }

    #[test]
    fn kit_default_equals_new() {
        let kit = Kit::default();
        let s = format!("{:?}", kit);
        assert!(s.contains("modules: 0"));
        assert!(s.contains("configs: 0"));
    }
}

#[cfg(feature = "hot-reload")]
mod reload_config_coverage {
    use std::error::Error;
    use trait_kit::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct FailingReloadConfig;

    impl Configurable for FailingReloadConfig {
        fn load() -> Result<Self, Box<dyn Error + Send>> {
            Err(Box::new(std::io::Error::other(
                "reload intentional failure",
            )))
        }
    }

    #[test]
    fn reload_config_propagates_load_error() {
        let kit = Kit::new();
        let result = kit.reload_config::<FailingReloadConfig>();
        match result {
            Err(KitError::BuildFailed { context, source }) => {
                assert_eq!(context, "reload_config");
                assert!(source.to_string().contains("reload intentional failure"));
            }
            other => panic!("expected BuildFailed, got: {other:?}"),
        }
    }

    #[test]
    fn reload_config_invokes_multiple_subscribers() {
        use std::cell::Cell;
        use std::rc::Rc;

        #[derive(Clone, Debug, PartialEq, Eq)]
        struct MultiSubConfig;
        impl Configurable for MultiSubConfig {
            fn load() -> Result<Self, Box<dyn Error + Send>> {
                Ok(Self)
            }
        }

        let kit = Kit::new();
        let counter = Rc::new(Cell::new(0u32));
        let c1 = Rc::clone(&counter);
        let c2 = Rc::clone(&counter);

        kit.subscribe::<MultiSubConfig>(move || {
            c1.set(c1.get() + 1);
        });
        kit.subscribe::<MultiSubConfig>(move || {
            c2.set(c2.get() + 10);
        });

        kit.reload_config::<MultiSubConfig>()
            .expect("reload should succeed");

        // Both subscribers should have fired: 1 + 10 = 11.
        assert_eq!(counter.get(), 11);
    }

    #[test]
    fn subscribe_and_reload_with_no_subscribers_succeeds() {
        #[derive(Clone, Debug, PartialEq, Eq)]
        struct NoSubConfig;
        impl Configurable for NoSubConfig {
            fn load() -> Result<Self, Box<dyn Error + Send>> {
                Ok(Self)
            }
        }
        let kit = Kit::new();
        // No subscribers registered — reload should still succeed.
        kit.reload_config::<NoSubConfig>()
            .expect("reload should succeed");
        let kit = kit.build().expect("build should succeed");
        let _: NoSubConfig = kit.config().expect("config should be stored");
    }
}

#[cfg(feature = "confers-macros")]
mod load_config_or_default_coverage {
    use std::error::Error;
    use trait_kit::kit::config::ModuleConfig;
    use trait_kit::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, trait_kit::kit::Config)]
    struct WithDefault {
        #[config(default = "loaded".to_string())]
        field: String,
    }

    impl ModuleConfig for WithDefault {
        const PATH: &'static str = "config/with_default.toml";
        fn default_value() -> Self {
            Self {
                field: "fallback".to_string(),
            }
        }
    }

    impl Configurable for WithDefault {
        fn load() -> Result<Self, Box<dyn Error + Send>> {
            // Simulate load failure — should fall back to default_value.
            Err(Box::new(std::io::Error::other(
                "load failed, using default",
            )))
        }
    }

    #[test]
    fn load_config_or_default_uses_default_when_load_fails() {
        let kit = Kit::new();
        kit.load_config_or_default::<WithDefault>()
            .expect("should never error");
        let kit = kit.build().expect("build should succeed");
        let cfg: WithDefault = kit.config().expect("config should be stored");
        assert_eq!(cfg.field, "fallback");
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct LoadOkConfig {
        v: u32,
    }

    impl ModuleConfig for LoadOkConfig {
        const PATH: &'static str = "config/load_ok.toml";
        fn default_value() -> Self {
            Self { v: 0 }
        }
    }

    impl Configurable for LoadOkConfig {
        fn load() -> Result<Self, Box<dyn Error + Send>> {
            Ok(Self { v: 42 })
        }
    }

    #[test]
    fn load_config_or_default_uses_loaded_value_when_load_succeeds() {
        let kit = Kit::new();
        kit.load_config_or_default::<LoadOkConfig>()
            .expect("should never error");
        let kit = kit.build().expect("build should succeed");
        let cfg: LoadOkConfig = kit.config().expect("config should be stored");
        assert_eq!(cfg.v, 42);
    }

    #[test]
    fn load_config_or_default_overrides_prior_set_config() {
        let kit = Kit::new();
        kit.set_config(LoadOkConfig { v: 1 });
        kit.load_config_or_default::<LoadOkConfig>()
            .expect("should override");
        let kit = kit.build().expect("build should succeed");
        let cfg: LoadOkConfig = kit.config().expect("config should be stored");
        assert_eq!(cfg.v, 42);
    }
}
