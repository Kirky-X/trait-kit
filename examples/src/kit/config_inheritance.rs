// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Config inheritance example: demonstrates the four-layer config inheritance
//! system (merge_json_deep → ConfigInherit → SharedConfig → populate_defaults).
//!
//! Run with:
//! ```bash
//! cargo run --example config_inheritance --features confers
//! ```

use trait_kit::kit::{Kit, ModuleConfig};
use trait_kit_derive::{ConfigInherit, SharedConfig};

// ── Layer 1: ModuleConfig — zero-config defaults ──

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
            database: "app_db".into(),
        }
    }
}

// ── Layer 2: SharedConfig — cross-type field inheritance ──

#[derive(Clone, Debug, PartialEq, SharedConfig)]
#[shared(host, port)]
struct RedisConfig {
    host: String,
    port: u16,
    db_index: u8,
}

// ── Layer 3: AppConfig — the "source of truth" for shared fields ──

#[derive(Clone, Debug, PartialEq, SharedConfig)]
#[shared(host, port)]
struct AppConfig {
    host: String,
    port: u16,
    app_name: String,
}

fn main() {
    let kit = Kit::new();

    // ── Step 1: Populate defaults for DbConfig (Layer 4: populate_defaults) ──
    let populated = kit.populate_defaults::<DbConfig>();
    println!("DbConfig defaults populated: {populated}");

    let db: DbConfig = kit.config().unwrap();
    println!("  DbConfig before inheritance: {db:?}");

    // ── Step 2: Set AppConfig (the shared field source) ──
    kit.set_config(AppConfig {
        host: "prod.example.com".into(),
        port: 8080,
        app_name: "My Production App".into(),
    });

    // ── Step 3: Extract shared fields from AppConfig → overlay ──
    kit.extract_shared::<AppConfig>();
    println!("\nShared fields extracted from AppConfig");

    // ── Step 4: Inject shared fields into DbConfig ──
    kit.inject_shared::<DbConfig>();

    let db_after: DbConfig = kit.config().unwrap();
    println!("  DbConfig after inheritance: {db_after:?}");
    assert_eq!(db_after.host, "prod.example.com");
    assert_eq!(db_after.port, 8080);

    // ── Step 5: Apply compile-time safe override (Layer 2: ConfigInherit) ──
    kit.merge_config::<DbConfig>(DbConfigOverride {
        host: None,       // keep inherited value
        port: Some(5432), // override port
        max_connections: Some(200),
        database: None, // keep default
    });

    let db_final: DbConfig = kit.config().unwrap();
    println!("\n  DbConfig after merge_config: {db_final:?}");
    assert_eq!(db_final.host, "prod.example.com"); // inherited
    assert_eq!(db_final.port, 5432); // overridden
    assert_eq!(db_final.max_connections, 200); // overridden
    assert_eq!(db_final.database, "app_db"); // default

    // ── Step 6: RedisConfig also inherits shared fields ──
    kit.set_config(RedisConfig {
        host: "redis-default".into(),
        port: 6379,
        db_index: 0,
    });
    kit.inject_shared::<RedisConfig>();

    let redis: RedisConfig = kit.config().unwrap();
    println!("\n  RedisConfig after injection: {redis:?}");
    assert_eq!(redis.host, "prod.example.com"); // inherited from AppConfig
    assert_eq!(redis.port, 8080); // inherited from AppConfig

    println!("\n✅ Config inheritance demo completed successfully!");
}
