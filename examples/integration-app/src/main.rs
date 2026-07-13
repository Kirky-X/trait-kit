// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Integration app — demonstrates trait-kit `AsyncKit` assembling all 5
//! component modules (`oxcache` / `dbnexus` / `inklog` / `limiteron` /
//! `sdforge`) with the full dependency-injection chain.
//!
//! Architecture layers (see `specmark/changes/trait-kit-arch-di-app/`):
//!
//! - **Basic component layer**: `oxcache` (cache backend for `dbnexus`)
//! - **Functional component layer**: `limiteron`, `sdforge`, `inklog`, `dbnexus`
//!
//! Dependency-injection chains:
//!
//! 1. `oxcache` → `dbnexus` → `inklog` (cache → pool → log storage)
//! 2. `limiteron` → `sdforge` (governor → rate-limiter facade)
//!
//! The app registers all 5 modules, builds the `AsyncKit`, then `require`s
//! each `Capability` and prints a verification line so callers can confirm
//! the trait-based dispatch works end-to-end.

// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT (see ../../LICENSE)

use std::sync::Arc;

use trait_kit::prelude::*;

use dbnexus::integrations::kit::DbNexusModule;
use inklog::integrations::kit::InklogModule;
use limiteron::integrations::kit::LimiteronModule;
use oxcache::integrations::kit::{OxcacheConfig, OxcacheModule};
use sdforge::integrations::kit::SdforgeModule;

/// Build a minimal valid `FlowControlConfig` (1 rule matching all IPs with a
/// permissive token bucket). `FlowControlConfig::default()` has an empty
/// rules vec which fails `validate()`. Mirrors the helper in
/// `sdforge/src/integrations/kit/module.rs::make_minimal_valid_config` so the
/// `SdforgeModule` chain's `limiter.check("1.2.3.4")` matches a rule.
fn make_minimal_valid_flow_config() -> limiteron::config::FlowControlConfig {
    use limiteron::config::{
        Action, ActionConfig, FlowControlConfig, LimiterConfig, Matcher, Rule,
    };
    let mut config = FlowControlConfig::default();
    config.rules.push(Rule {
        id: "default".to_string(),
        name: "default rule".to_string(),
        priority: 0,
        matchers: vec![Matcher::Ip {
            ip_ranges: vec!["0.0.0.0/0".to_string()],
        }],
        limiters: vec![LimiterConfig::TokenBucket {
            capacity: 100,
            refill_rate: 10,
        }],
        action: ActionConfig {
            on_exceed: Action::Reject,
            ban: None,
        },
    });
    config
}

/// Construct a minimal in-memory `DbConfig` (sqlite::memory:, 1..=5 conns).
/// Mirrors the helper pattern in `dbnexus/src/integrations/kit/module.rs`.
fn make_memory_db_config() -> dbnexus::foundation::config::DbConfig {
    dbnexus::foundation::config::DbConfig {
        url: "sqlite::memory:".to_string(),
        max_connections: 5,
        min_connections: 1,
        ..Default::default()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut kit = AsyncKit::new();

    // Three configs cover the three leaves that need configuration.
    kit.set_config(OxcacheConfig::default());
    kit.set_config(make_minimal_valid_flow_config());
    kit.set_config(make_memory_db_config());

    // Register all 5 modules in arbitrary order — topological sort will
    // produce oxcache → dbnexus → inklog and limiteron → sdforge.
    kit.register::<OxcacheModule>()?;
    kit.register::<LimiteronModule>()?;
    kit.register::<DbNexusModule>()?;
    kit.register::<SdforgeModule>()?;
    kit.register::<InklogModule>()?;

    println!("[integration-app] building AsyncKit with 5 modules …");
    let kit = kit.build().await.expect("AsyncKit::build should succeed");
    println!("[integration-app] build OK; all 5 modules resolved by topological sort");

    // --- DI chain 1: oxcache → dbnexus → inklog ---------------------------
    let cache: Arc<dyn oxcache::backend::CacheBackend + Send + Sync> = kit
        .require::<OxcacheModule>()
        .expect("require OxcacheModule");
    println!(
        "[integration-app] OxcacheModule capability: CacheBackend@{:p}",
        Arc::as_ptr(&cache)
    );

    let pool: Arc<dyn dbnexus::database::pool::ConnectionPool + Send + Sync> = kit
        .require::<DbNexusModule>()
        .expect("require DbNexusModule");
    println!(
        "[integration-app] DbNexusModule capability: ConnectionPool@{:p}",
        Arc::as_ptr(&pool)
    );

    let provider: Arc<dyn inklog::LogDbProvider + Send + Sync> =
        kit.require::<InklogModule>().expect("require InklogModule");
    println!(
        "[integration-app] InklogModule capability: LogDbProvider@{:p}",
        Arc::as_ptr(&provider)
    );

    // Execute a DDL statement through LogDbProvider — proves ConnectionPool
    // from DbNexusModule was successfully injected (which itself depended on
    // the CacheBackend from OxcacheModule).
    provider
        .execute_log("CREATE TABLE IF NOT EXISTS logs (id INTEGER PRIMARY KEY)")
        .await
        .expect("execute_log should succeed on injected ConnectionPool");
    println!("[integration-app] DI chain 1 OK: oxcache → dbnexus → inklog (CREATE TABLE logs)");

    // --- DI chain 2: limiteron → sdforge ----------------------------------
    let governor: Arc<limiteron::Governor> = kit
        .require::<LimiteronModule>()
        .expect("require LimiteronModule");
    println!(
        "[integration-app] LimiteronModule capability: Governor@{:p}",
        Arc::as_ptr(&governor)
    );

    let limiter: Arc<dyn sdforge::domain::rate_limiter::ForgeRateLimiter + Send + Sync> = kit
        .require::<SdforgeModule>()
        .expect("require SdforgeModule");
    println!(
        "[integration-app] SdforgeModule capability: ForgeRateLimiter@{:p}",
        Arc::as_ptr(&limiter)
    );

    let allowed = limiter
        .check("1.2.3.4")
        .await
        .expect("check should succeed on injected Governor");
    assert!(
        allowed,
        "first request must be allowed (TokenBucket 100 tokens)"
    );
    println!(
        "[integration-app] DI chain 2 OK: limiteron → sdforge (check(\"1.2.3.4\") = {allowed})"
    );

    // --- contains / contains_config probes --------------------------------
    assert!(kit.contains::<OxcacheModule>(), "contains OxcacheModule");
    assert!(
        kit.contains::<LimiteronModule>(),
        "contains LimiteronModule"
    );
    assert!(kit.contains::<DbNexusModule>(), "contains DbNexusModule");
    assert!(kit.contains::<SdforgeModule>(), "contains SdforgeModule");
    assert!(kit.contains::<InklogModule>(), "contains InklogModule");
    assert!(
        kit.contains_config::<OxcacheConfig>(),
        "contains_config OxcacheConfig"
    );
    println!("[integration-app] contains() / contains_config() probes OK");

    println!("[integration-app] ✅ all 5 modules assembled, both DI chains verified");
    Ok(())
}
