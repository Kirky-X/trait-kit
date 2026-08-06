// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Level 2: `confers-macros` feature — ModuleConfig + Config derive re-export.
//!
//! Demonstrates binding a config type to its module via `ModuleConfig::PATH` and
//! `default_value()`, then having a module's `build()` retrieve that config via
//! `kit.config::<C>()`.
//!
//! Run: `cargo run -p trait-kit-example --example confers_macros --features confers-macros`

use std::sync::Arc;
use trait_kit::kit::config::ModuleConfig;
// Level 2 re-export: `trait_kit::kit::Config` is gated behind `confers-macros`.
use trait_kit::kit::Config;
use trait_kit::prelude::*;

// Demonstrates the Level 2 re-export: derive `Config` via `trait_kit::kit::Config`
// instead of `confers::Config`.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, Config)]
struct DbConfig {
    #[config(default = "postgres://localhost".to_string())]
    url: String,
    #[config(default = 10)]
    max_connections: u32,
}

impl ModuleConfig for DbConfig {
    const PATH: &'static str = "config/db.toml";
    fn default_value() -> Self {
        Self {
            url: "sqlite://default".to_string(),
            max_connections: 5,
        }
    }
}

struct DbPool {
    config: DbConfig,
}

struct DbPoolModule;
impl ModuleMeta for DbPoolModule {
    const NAME: &'static str = "db_pool";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        &[]
    }
}

impl AutoBuilder for DbPoolModule {
    type Capability = Arc<DbPool>;
    type Error = TraitKitError;
    fn build(kit: &Kit) -> Result<Self::Capability, Self::Error> {
        let config: DbConfig = kit.config()?;
        Ok(Arc::new(DbPool { config }))
    }
}

fn main() {
    assert_eq!(
        DbConfig::PATH,
        "config/db.toml",
        "ModuleConfig::PATH should be accessible"
    );

    let mut kit = Kit::new();
    kit.set_config(DbConfig::default_value());
    kit.register::<DbPoolModule>()
        .expect("register DbPoolModule");
    let kit = kit.build().expect("build should succeed");

    let pool = kit.require::<DbPoolModule>().expect("require DbPoolModule");
    println!(
        "DbPool: url={}, max_connections={}",
        pool.config.url, pool.config.max_connections
    );
    assert_eq!(pool.config.url, "sqlite://default");
    assert_eq!(pool.config.max_connections, 5);
    assert!(
        kit.contains_config::<DbConfig>(),
        "contains_config should be true"
    );

    println!("confers_macros: OK");
}
