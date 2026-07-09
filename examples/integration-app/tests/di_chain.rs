//! Integration tests for the `integration-app` — verifies both
//! dependency-injection chains end-to-end through trait-kit's `AsyncKit`.
//!
//! Chain 1: `oxcache` → `dbnexus` → `inklog` (cache → pool → log storage)
//! Chain 2: `limiteron` → `sdforge` (governor → rate-limiter facade)

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
/// rules vec which fails `validate()`.
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
fn make_memory_db_config() -> dbnexus::foundation::config::DbConfig {
    dbnexus::foundation::config::DbConfig {
        url: "sqlite::memory:".to_string(),
        max_connections: 5,
        min_connections: 1,
        ..Default::default()
    }
}

/// DI chain 1: `oxcache` → `dbnexus` → `inklog`.
///
/// The topological sort must build `OxcacheModule` first, then
/// `DbNexusModule` (which calls `kit.require::<OxcacheModule>()` in its
/// `build` callback), then `InklogModule` (which calls
/// `kit.require::<DbNexusModule>()`). The end-to-end check executes a DDL
/// statement via the `LogDbProvider` capability returned by `InklogModule`.
#[tokio::test]
async fn di_chain_oxcache_dbnexus_inklog() {
    let mut kit = AsyncKit::new();
    kit.set_config(OxcacheConfig::default());
    kit.set_config(make_memory_db_config());
    kit.register::<OxcacheModule>()
        .expect("register OxcacheModule");
    kit.register::<DbNexusModule>()
        .expect("register DbNexusModule");
    kit.register::<InklogModule>()
        .expect("register InklogModule");

    let kit = kit.build().await.expect("AsyncKit::build should succeed");

    // Verify each capability type (compile-time check via let _ annotations).
    let _: Arc<dyn oxcache::backend::CacheBackend + Send + Sync> = kit
        .require::<OxcacheModule>()
        .expect("require OxcacheModule");
    let _: Arc<dyn dbnexus::database::pool::ConnectionPool + Send + Sync> = kit
        .require::<DbNexusModule>()
        .expect("require DbNexusModule");
    let provider: Arc<dyn inklog::LogDbProvider + Send + Sync> =
        kit.require::<InklogModule>().expect("require InklogModule");

    // Execute a DDL statement through the LogDbProvider — proves the
    // ConnectionPool from DbNexusModule was successfully injected (which
    // itself depended on the CacheBackend from OxcacheModule).
    provider
        .execute_log("CREATE TABLE IF NOT EXISTS logs (id INTEGER PRIMARY KEY)")
        .await
        .expect("execute_log should succeed on injected ConnectionPool");
}

/// DI chain 2: `limiteron` → `sdforge`.
///
/// `SdforgeModule::build` calls `kit.require::<LimiteronModule>()` and wraps
/// the `Governor` in a `LimiteronForgeAdapter`. The returned
/// `ForgeRateLimiter` capability must accept a request and report `true`
/// (TokenBucket has 100 tokens, so the first request is allowed).
#[tokio::test]
async fn di_chain_limiteron_sdforge() {
    let mut kit = AsyncKit::new();
    kit.set_config(make_minimal_valid_flow_config());
    kit.register::<LimiteronModule>()
        .expect("register LimiteronModule");
    kit.register::<SdforgeModule>()
        .expect("register SdforgeModule");

    let kit = kit.build().await.expect("AsyncKit::build should succeed");

    let _: Arc<limiteron::governor::Governor> = kit
        .require::<LimiteronModule>()
        .expect("require LimiteronModule");
    let limiter: Arc<dyn sdforge::domain::rate_limiter::ForgeRateLimiter + Send + Sync> = kit
        .require::<SdforgeModule>()
        .expect("require SdforgeModule");

    let allowed = limiter
        .check("1.2.3.4")
        .await
        .expect("check should succeed on injected Governor");
    assert!(
        allowed,
        "first request must be allowed (TokenBucket 100 tokens)"
    );
}
