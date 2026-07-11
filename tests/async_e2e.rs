// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! End-to-end integration test for trait-kit 0.2.2 `AsyncKit`.
//!
//! Phase 7 (T046) — registers all 5 component modules (`OxcacheModule`,
//! `LimiteronModule`, `DbNexusModule`, `SdforgeModule`, `InklogModule`),
//! builds the `AsyncKit`, and verifies `require::<EachModule>()` returns
//! the correct `Capability` type.
//!
//! # Capability type reference (verified against source — Rule 8)
//!
//! | Module           | Capability                                            |
//! |------------------|-------------------------------------------------------|
//! | `OxcacheModule`  | `Arc<dyn oxcache::backend::CacheBackend + Send + Sync>` |
//! | `LimiteronModule`| `Arc<limiteron::governor::Governor>` (concrete)       |
//! | `DbNexusModule`  | `Arc<dyn dbnexus::database::pool::ConnectionPool + Send + Sync>` |
//! | `SdforgeModule`  | `Arc<dyn sdforge::domain::rate_limiter::ForgeRateLimiter + Send + Sync>` |
//! | `InklogModule`   | `Arc<dyn inklog::LogDbProvider + Send + Sync>`        |

#![cfg(feature = "async")]

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
/// `SdforgeModule` chain test's `limiter.check("1.2.3.4")` matches a rule.
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

/// R-t046-001: Register all 5 modules, build the AsyncKit, verify
/// `require::<EachModule>()` returns the correct `Capability` type, and
/// `contains::<EachModule>()` returns `true` for every module. Also verifies
/// `contains_config::<OxcacheConfig>()` returns `true`.
#[tokio::test]
async fn e2e_full_async_kit_with_all_5_modules() {
    let mut kit = AsyncKit::new();
    // Three configs cover the three leaves that need configuration.
    kit.set_config(OxcacheConfig::default());
    kit.set_config(make_minimal_valid_flow_config());
    kit.set_config(make_memory_db_config());

    // Register all 5 modules in arbitrary order — topological sort will
    // produce oxcache → dbnexus → inklog and limiteron → sdforge.
    kit.register::<OxcacheModule>()
        .expect("register OxcacheModule");
    kit.register::<LimiteronModule>()
        .expect("register LimiteronModule");
    kit.register::<DbNexusModule>()
        .expect("register DbNexusModule");
    kit.register::<SdforgeModule>()
        .expect("register SdforgeModule");
    kit.register::<InklogModule>()
        .expect("register InklogModule");

    let kit = kit.build().await.expect("AsyncKit::build should succeed");

    // Verify each `require::<M>()` returns the correct Capability type
    // (compile-time check via explicit `let _: <Type>` annotations).
    let _: Arc<dyn oxcache::backend::CacheBackend + Send + Sync> = kit
        .require::<OxcacheModule>()
        .expect("require OxcacheModule");
    let _: Arc<limiteron::governor::Governor> = kit
        .require::<LimiteronModule>()
        .expect("require LimiteronModule");
    let _: Arc<dyn dbnexus::database::pool::ConnectionPool + Send + Sync> = kit
        .require::<DbNexusModule>()
        .expect("require DbNexusModule");
    let _: Arc<dyn sdforge::domain::rate_limiter::ForgeRateLimiter + Send + Sync> = kit
        .require::<SdforgeModule>()
        .expect("require SdforgeModule");
    let _: Arc<dyn inklog::LogDbProvider + Send + Sync> =
        kit.require::<InklogModule>().expect("require InklogModule");

    // `contains::<M>()` must report `true` for every registered module.
    assert!(kit.contains::<OxcacheModule>(), "contains OxcacheModule");
    assert!(
        kit.contains::<LimiteronModule>(),
        "contains LimiteronModule"
    );
    assert!(kit.contains::<DbNexusModule>(), "contains DbNexusModule");
    assert!(kit.contains::<SdforgeModule>(), "contains SdforgeModule");
    assert!(kit.contains::<InklogModule>(), "contains InklogModule");

    // `contains_config::<C>()` must report `true` for at least one set config.
    assert!(
        kit.contains_config::<OxcacheConfig>(),
        "contains_config OxcacheConfig"
    );
}

/// R-t046-002: Verify the oxcache ← dbnexus ← inklog dependency injection
/// chain. The topological sort must build `OxcacheModule` first, then
/// `DbNexusModule` (which calls `kit.require::<OxcacheModule>()` in its
/// `build` callback), then `InklogModule` (which calls
/// `kit.require::<DbNexusModule>()`). The end-to-end check executes a DDL
/// statement via the `LogDbProvider` capability returned by `InklogModule`.
#[tokio::test]
async fn e2e_dependency_injection_chain_oxcache_dbnexus_inklog() {
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

    let provider: Arc<dyn inklog::LogDbProvider + Send + Sync> =
        kit.require::<InklogModule>().expect("require InklogModule");

    // Execute a DDL statement through the LogDbProvider — proves the
    // ConnectionPool from DbNexusModule was successfully injected.
    provider
        .execute_log("CREATE TABLE IF NOT EXISTS logs (id INTEGER PRIMARY KEY)")
        .await
        .expect("execute_log should succeed on injected ConnectionPool");
}

/// R-t046-003: Verify the limiteron ← sdforge dependency injection chain.
/// `SdforgeModule::build` calls `kit.require::<LimiteronModule>()` and wraps
/// the `Governor` in a `LimiteronForgeAdapter`. The returned
/// `ForgeRateLimiter` capability must accept a request and report `true`
/// (TokenBucket has 100 tokens, so the first request is allowed).
#[tokio::test]
async fn e2e_dependency_injection_chain_limiteron_sdforge() {
    let mut kit = AsyncKit::new();
    kit.set_config(make_minimal_valid_flow_config());
    kit.register::<LimiteronModule>()
        .expect("register LimiteronModule");
    kit.register::<SdforgeModule>()
        .expect("register SdforgeModule");

    let kit = kit.build().await.expect("AsyncKit::build should succeed");

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

/// R-t046-004: Compile-time verification — all 5 module types satisfy the
/// `AsyncAutoBuilder` trait bound required by `AsyncKit::register` / `require`.
/// If any module's `Capability` / `Error` fail to meet the bound, this test
/// fails to compile.
#[test]
fn e2e_all_modules_satisfy_async_auto_builder_bounds() {
    fn assert_bounds<M: AsyncAutoBuilder>() {}
    assert_bounds::<OxcacheModule>();
    assert_bounds::<LimiteronModule>();
    assert_bounds::<DbNexusModule>();
    assert_bounds::<SdforgeModule>();
    assert_bounds::<InklogModule>();
}
