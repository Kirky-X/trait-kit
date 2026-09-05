//! End-to-end integration test: simulates project A using project B,
//! where B's config inherits shared fields from A's config.

#![cfg(feature = "confers")]

use trait_kit::kit::{Kit, ModuleConfig};
use trait_kit_derive::{ConfigInherit, SharedConfig};

// ── Project B's database config ──

#[derive(Clone, Debug, PartialEq, ConfigInherit, SharedConfig)]
#[shared(host, port)]
struct DbConfig {
    host: String,
    port: u16,
    max_connections: u32,
    database: String,
}

impl ModuleConfig for DbConfig {
    const PATH: &'static str = "config/db.toml";
    fn default_value() -> Self {
        Self {
            host: "localhost".into(),
            port: 3306,
            max_connections: 10,
            database: "mydb".into(),
        }
    }
}

// ── Project A's application config ──

#[derive(Clone, Debug, PartialEq, SharedConfig)]
#[shared(host, port)]
struct AppConfig {
    host: String,
    port: u16,
    app_name: String,
    log_level: String,
}

// ── Project A's cache config (also shares host/port) ──

#[derive(Clone, Debug, PartialEq, SharedConfig)]
#[shared(host, port)]
struct CacheConfig {
    host: String,
    port: u16,
    ttl_seconds: u64,
}

/// Simulates the full config inheritance flow:
/// 1. Project A loads its AppConfig with production values
/// 2. Project A loads its CacheConfig
/// 3. Project B's DbConfig starts with defaults (populate_defaults)
/// 4. A extracts shared fields → overlay
/// 5. B's DbConfig inherits shared fields from overlay
/// 6. Verify B got A's host/port while keeping its own defaults
#[test]
fn e2e_config_inheritance_a_b_project_scenario() {
    let kit = Kit::new();

    // Step 1: Project A sets its application config
    kit.set_config(AppConfig {
        host: "prod.example.com".into(),
        port: 8080,
        app_name: "production-app".into(),
        log_level: "info".into(),
    });

    // Step 2: Project A sets its cache config
    kit.set_config(CacheConfig {
        host: "cache.example.com".into(),
        port: 6379,
        ttl_seconds: 300,
    });

    // Step 3: Project B's DbConfig gets default values
    assert!(kit.populate_defaults::<DbConfig>());
    // Second call should not override
    assert!(!kit.populate_defaults::<DbConfig>());

    // Verify defaults were populated
    let db_before: DbConfig = kit.config().unwrap();
    assert_eq!(db_before.host, "localhost");
    assert_eq!(db_before.port, 3306);

    // Step 4: Extract shared fields from AppConfig (last extractor wins)
    kit.extract_shared::<AppConfig>();

    // Also extract from CacheConfig — this overrides host/port in the overlay
    kit.extract_shared::<CacheConfig>();

    // Step 5: Inject shared fields into DbConfig
    kit.inject_shared::<DbConfig>();

    // Step 6: Verify
    let db_after: DbConfig = kit.config().unwrap();
    // CacheConfig was extracted last, so its values win
    assert_eq!(db_after.host, "cache.example.com");
    assert_eq!(db_after.port, 6379);
    // DbConfig-specific fields unchanged
    assert_eq!(db_after.max_connections, 10);
    assert_eq!(db_after.database, "mydb");

    // Verify AppConfig is unchanged
    let app: AppConfig = kit.config().unwrap();
    assert_eq!(app.host, "prod.example.com");
    assert_eq!(app.app_name, "production-app");

    // Verify CacheConfig is unchanged
    let cache: CacheConfig = kit.config().unwrap();
    assert_eq!(cache.host, "cache.example.com");
    assert_eq!(cache.ttl_seconds, 300);
}

/// Tests that merge_config works alongside shared inheritance
#[test]
fn e2e_merge_config_then_shared_inheritance() {
    let kit = Kit::new();

    // Set AppConfig
    kit.set_config(AppConfig {
        host: "shared-host".into(),
        port: 9090,
        app_name: "test-app".into(),
        log_level: "debug".into(),
    });

    // Set DbConfig with defaults
    kit.populate_defaults::<DbConfig>();

    // Apply a compile-time safe override to DbConfig
    kit.merge_config::<DbConfig>(DbConfigOverride {
        host: None, // keep default
        port: None, // keep default
        max_connections: Some(100),
        database: Some("production_db".into()),
    });

    // Verify merge_config worked
    let db: DbConfig = kit.config().unwrap();
    assert_eq!(db.host, "localhost"); // default kept
    assert_eq!(db.max_connections, 100); // overridden
    assert_eq!(db.database, "production_db"); // overridden

    // Now extract shared from AppConfig and inject into DbConfig
    kit.extract_shared::<AppConfig>();
    kit.inject_shared::<DbConfig>();

    // DbConfig should now have AppConfig's host/port
    let db_final: DbConfig = kit.config().unwrap();
    assert_eq!(db_final.host, "shared-host"); // from shared
    assert_eq!(db_final.port, 9090); // from shared
    assert_eq!(db_final.max_connections, 100); // preserved from merge_config
    assert_eq!(db_final.database, "production_db"); // preserved from merge_config
}
