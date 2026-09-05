//! Integration tests for `#[derive(SharedConfig)]`.

#![cfg(feature = "confers")]

use trait_kit::kit::{Kit, SharedConfig};
use trait_kit_derive::SharedConfig;

// ── Basic derive ──

#[derive(Clone, Debug, PartialEq, SharedConfig)]
#[shared(host, port)]
struct DerivedDbConfig {
    host: String,
    port: u16,
    max_connections: u32,
}

#[test]
fn derive_shared_config_extract_returns_correct_json() {
    let cfg = DerivedDbConfig {
        host: "db.example.com".into(),
        port: 5432,
        max_connections: 100,
    };

    let shared = cfg.extract_shared();

    assert_eq!(shared.len(), 2);
    assert_eq!(
        shared.get("host"),
        Some(&serde_json::Value::String("db.example.com".into()))
    );
    assert_eq!(shared.get("port"), Some(&serde_json::json!(5432)));
    // max_connections is NOT shared
    assert_eq!(shared.get("max_connections"), None);
}

#[test]
fn derive_shared_config_inject_overrides_matching_fields() {
    let mut cfg = DerivedDbConfig {
        host: "old-host".into(),
        port: 3306,
        max_connections: 10,
    };

    let mut overlay = serde_json::Map::new();
    overlay.insert("host".into(), serde_json::json!("new-host"));
    overlay.insert("port".into(), serde_json::json!(9999));

    cfg.inject_shared(&overlay);

    assert_eq!(cfg.host, "new-host");
    assert_eq!(cfg.port, 9999);
    assert_eq!(cfg.max_connections, 10); // not affected
}

#[test]
fn derive_shared_config_inject_skips_type_mismatch() {
    let mut cfg = DerivedDbConfig {
        host: "keep-host".into(),
        port: 3306,
        max_connections: 10,
    };

    let mut overlay = serde_json::Map::new();
    // port expects u16, but we give a string — should be silently skipped
    overlay.insert("port".into(), serde_json::json!("not-a-number"));

    cfg.inject_shared(&overlay);

    assert_eq!(cfg.port, 3306); // unchanged
}

#[test]
fn derive_shared_config_inject_skips_missing_keys() {
    let mut cfg = DerivedDbConfig {
        host: "keep".into(),
        port: 3306,
        max_connections: 10,
    };

    let overlay = serde_json::Map::new(); // empty overlay
    cfg.inject_shared(&overlay);

    assert_eq!(cfg.host, "keep");
    assert_eq!(cfg.port, 3306);
}

// ── Kit integration ──

#[derive(Clone, Debug, PartialEq, SharedConfig)]
#[shared(host, port)]
struct DerivedAppConfig {
    host: String,
    port: u16,
    app_name: String,
}

#[test]
fn derive_shared_config_kit_extract_inject_flow() {
    let kit = Kit::new();

    kit.set_config(DerivedDbConfig {
        host: "shared-host".into(),
        port: 8080,
        max_connections: 50,
    });

    kit.set_config(DerivedAppConfig {
        host: "app-host".into(),
        port: 3000,
        app_name: "my-app".into(),
    });

    // Extract from DbConfig
    kit.extract_shared::<DerivedDbConfig>();

    // Inject into AppConfig
    kit.inject_shared::<DerivedAppConfig>();

    let app_cfg: DerivedAppConfig = kit.config().unwrap();
    assert_eq!(app_cfg.host, "shared-host");
    assert_eq!(app_cfg.port, 8080);
    assert_eq!(app_cfg.app_name, "my-app"); // unchanged
}
