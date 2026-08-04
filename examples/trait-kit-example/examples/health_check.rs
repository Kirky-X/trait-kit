// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Health feature — HealthCheck + HealthStatus + health_report.
//!
//! Demonstrates:
//! - `HealthCheck` trait with `check()` returning `HealthStatus`
//! - `Kit::register_health_check::<M>()` to register a checker
//! - `Kit<Ready>::health_check::<M>()` for per-module query
//! - `Kit<Ready>::health_report()` for aggregate report
//!
//! Run: `cargo run -p trait-kit-example --example health_check --features health`

use std::sync::Arc;
use trait_kit::prelude::*;

#[derive(Debug, Clone)]
struct CacheCap {
    hit_count: u64,
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

    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(CacheCap { hit_count: 42 }))
    }
}

impl HealthCheck for CacheModule {
    fn check(cap: &Arc<CacheCap>) -> HealthStatus {
        if cap.hit_count > 0 {
            HealthStatus::Healthy
        } else {
            HealthStatus::Degraded {
                detail: "no cache hits".to_string(),
            }
        }
    }
}

fn main() {
    let mut kit = Kit::new();
    kit.register::<CacheModule>().expect("register CacheModule");
    kit.register_health_check::<CacheModule>();

    let kit = kit.build().expect("build should succeed");

    // Per-module health check
    let status = kit
        .health_check::<CacheModule>()
        .expect("health_check should succeed");
    println!("Cache health: {:?}", status);
    assert!(status.is_healthy());

    // Aggregate health report
    let report = kit.health_report();
    println!("Health report ({} modules):", report.len());
    for (name, s) in &report {
        println!("  {name}: {s:?}");
    }
    assert!(!report.is_empty());

    println!("health_check: OK");
}
