// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Health feature — HealthCheck + HealthStatus + health_report.
//!
//! Demonstrates:
//! - `HealthCheck` trait with `check()` returning `HealthStatus`
//! - `Kit::register_health_check::<M>()` to register a checker
//! - `Kit<Ready>::health_check::<M>()` for per-module query
//! - `Kit<Ready>::health_report()` for aggregate report
//! - Config-driven thresholds: the healthy threshold and the cache's initial
//!   hit count are read from the Kit's typed config, so both the `Healthy`
//!   and `Degraded` branches are exercised.
//!
//! Run: `cargo run -p trait-kit-examples --example health_check --features health`

use std::sync::Arc;
use trait_kit::prelude::*;

/// Config controlling the cache's initial hit count and its healthy threshold.
#[derive(Debug, Clone)]
struct CacheConfig {
    initial_hits: u64,
    healthy_threshold: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            initial_hits: 42,
            healthy_threshold: 1,
        }
    }
}

#[derive(Debug, Clone)]
struct CacheCap {
    hit_count: u64,
    healthy_threshold: u64,
}

struct CacheModule;

impl ModuleMeta for CacheModule {
    const NAME: &'static str = "cache";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        &[]
    }
}

impl AutoBuilder for CacheModule {
    type Capability = Arc<CacheCap>;
    type Error = TraitKitError;

    fn build(kit: &Kit) -> Result<Self::Capability, Self::Error> {
        // Read threshold/values from the kit config (falls back to defaults
        // when no CacheConfig was set).
        let cfg = kit.config::<CacheConfig>().unwrap_or_default();
        Ok(Arc::new(CacheCap {
            hit_count: cfg.initial_hits,
            healthy_threshold: cfg.healthy_threshold,
        }))
    }
}

impl HealthCheck for CacheModule {
    fn check(cap: &Arc<CacheCap>) -> HealthStatus {
        if cap.hit_count >= cap.healthy_threshold {
            HealthStatus::Healthy
        } else {
            HealthStatus::Degraded {
                detail: format!(
                    "hit_count {} below healthy threshold {}",
                    cap.hit_count, cap.healthy_threshold
                ),
            }
        }
    }
}

fn main() {
    // ── Scenario 1: enough hits → Healthy ───────────────────────────────
    let mut kit = Kit::new();
    kit.set_config(CacheConfig {
        initial_hits: 42,
        healthy_threshold: 1,
    });
    kit.register::<CacheModule>().expect("register CacheModule");
    kit.register_health_check::<CacheModule>();

    let kit = kit.build().expect("build should succeed");

    // Per-module health check
    let status = kit
        .health_check::<CacheModule>()
        .expect("health_check should succeed");
    println!("Cache health: {:?}", status);
    assert!(status.is_healthy(), "expected Healthy, got {status:?}");

    // Aggregate health report
    let report = kit.health_report();
    println!("Health report ({} modules):", report.len());
    for (name, s) in &report {
        println!("  {name}: {s:?}");
    }
    assert!(!report.is_empty());

    println!("health_check scenario 1: OK (healthy)");

    // ── Scenario 2: hits below threshold → Degraded ─────────────────────
    let mut kit2 = Kit::new();
    kit2.set_config(CacheConfig {
        initial_hits: 0,
        healthy_threshold: 1,
    });
    kit2.register::<CacheModule>()
        .expect("register CacheModule");
    kit2.register_health_check::<CacheModule>();

    let kit2 = kit2.build().expect("build should succeed");

    let status2 = kit2
        .health_check::<CacheModule>()
        .expect("health_check should succeed");
    println!("Cache health (scenario 2): {status2:?}");
    assert!(!status2.is_healthy(), "expected Degraded, got {status2:?}");
    match &status2 {
        HealthStatus::Degraded { detail } => {
            assert!(
                detail.contains("below healthy threshold"),
                "degraded detail should explain the cause: got '{detail}'"
            );
        }
        other => panic!("expected Degraded, got {other:?}"),
    }

    println!("health_check: OK (scenario 1 healthy, scenario 2 degraded)");
}
