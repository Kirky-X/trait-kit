// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Conditional feature — runtime predicate-gated module registration.
//!
//! Demonstrates:
//! - `Kit::register_if::<M>(predicate)` — register only when predicate returns true
//! - Predicate receives `&Kit` for inspecting configs or other state
//! - Returns `Ok(true)` if registered, `Ok(false)` if skipped
//!
//! Run: `cargo run -p trait-kit-example --example conditional`

use std::sync::Arc;
use trait_kit::prelude::*;

#[derive(Debug, Clone)]
struct MetricsCap {
    enabled: bool,
}

struct MetricsModule;

impl ModuleMeta for MetricsModule {
    const NAME: &'static str = "metrics";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        &[]
    }
}

impl AutoBuilder for MetricsModule {
    type Capability = Arc<MetricsCap>;
    type Error = TraitKitError;

    fn build(kit: &Kit) -> Result<Self::Capability, Self::Error> {
        let enabled = kit
            .config::<FeatureFlags>()
            .map(|f| f.enable_metrics)
            .unwrap_or(false);
        Ok(Arc::new(MetricsCap { enabled }))
    }
}

/// Config that controls whether metrics should be enabled.
#[derive(Clone)]
struct FeatureFlags {
    enable_metrics: bool,
}

fn main() {
    // ── Scenario 1: metrics enabled ─────────────────────────────────────
    let mut kit = Kit::new();
    kit.set_config(FeatureFlags {
        enable_metrics: true,
    });
    let registered = kit
        .register_if::<MetricsModule>(|k| {
            k.config::<FeatureFlags>()
                .map(|f| f.enable_metrics)
                .unwrap_or(false)
        })
        .expect("register_if should succeed");
    assert!(registered, "module should be registered when flag is true");

    let kit = kit.build().expect("build should succeed");
    assert!(kit.contains::<MetricsModule>());
    let cap = kit
        .require::<MetricsModule>()
        .expect("require MetricsModule");
    assert!(cap.enabled, "capability should reflect enabled flag");
    println!("Scenario 1 (enabled): metrics registered = true, capability.enabled = true");

    // ── Scenario 2: metrics disabled ────────────────────────────────────
    let mut kit2 = Kit::new();
    kit2.set_config(FeatureFlags {
        enable_metrics: false,
    });
    let registered2 = kit2
        .register_if::<MetricsModule>(|k| {
            k.config::<FeatureFlags>()
                .map(|f| f.enable_metrics)
                .unwrap_or(false)
        })
        .expect("register_if should succeed");
    assert!(
        !registered2,
        "module should NOT be registered when flag is false"
    );

    let kit2 = kit2.build().expect("build should succeed");
    assert!(!kit2.contains::<MetricsModule>());
    println!("Scenario 2 (disabled): metrics registered = false");

    println!("conditional: OK");
}
