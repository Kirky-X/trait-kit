// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Level 1: `confers` feature — Configurable + load_config.
//!
//! Bridges trait-kit's `Configurable` trait to confers' `#[derive(Config)]`
//! generated `load_sync()`. Demonstrates env-var override + default fallback.
//! Run: `cargo run -p trait-kit-example --example confers_loader --features confers`

use std::error::Error;
use trait_kit::prelude::*;

// At Level 1 (`confers` feature), the derive macro is accessed via the
// `confers` crate directly. The `trait_kit::kit::Config` re-export is gated
// behind `confers-macros` (Level 2).
#[derive(Debug, Clone, PartialEq, serde::Deserialize, confers::Config)]
#[config(env_prefix = "TRAIT_KIT_EXAMPLE_")]
struct AppConfig {
    #[config(default = "localhost".to_string())]
    host: String,
    #[config(default = 8080)]
    port: u16,
}

impl Configurable for AppConfig {
    fn load() -> Result<Self, Box<dyn Error + Send>> {
        // ConfigError from confers is not Send, so we can't use `?` directly
        // with Box<dyn Error + Send>. Bridge via io::Error which is Send.
        AppConfig::load_sync().map_err(|e| -> Box<dyn Error + Send> {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })
    }
}

fn main() {
    unsafe { std::env::remove_var("TRAIT_KIT_EXAMPLE_HOST") };

    // 1. No env override — load_sync() falls back to #[config(default)].
    let kit = Kit::new();
    kit.load_config::<AppConfig>()
        .expect("load_config should fall back to defaults");
    let kit = kit.build().expect("build should succeed");
    let config: AppConfig = kit.config().expect("config should be retrievable");
    println!(
        "Loaded (defaults): host={}, port={}",
        config.host, config.port
    );
    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, 8080);

    // 2. Env override — confers' load_sync() picks up APP_HOST from the environment.
    //    (Numeric fields aren't auto-parsed from env strings; only String fields
    //    demonstrate env override in this example.)
    unsafe { std::env::set_var("TRAIT_KIT_EXAMPLE_HOST", "10.0.0.1") };
    let kit2 = Kit::new();
    kit2.load_config::<AppConfig>()
        .expect("load_config should pick up env override");
    let kit2 = kit2.build().expect("build should succeed");
    let config2: AppConfig = kit2.config().expect("config should be retrievable");
    println!(
        "Loaded (env override): host={}, port={}",
        config2.host, config2.port
    );
    assert_eq!(config2.host, "10.0.0.1");
    assert_eq!(config2.port, 8080);

    unsafe { std::env::remove_var("TRAIT_KIT_EXAMPLE_HOST") };
    println!("confers_loader: OK");
}
