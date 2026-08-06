// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Feature Toggle — 运行时模块启隐控制。
//!
//! Demonstrates:
//! - `Kit::enable_toggle(key, bool)` — 设置 feature flag
//! - `Kit::is_toggle_enabled(key)` — 查询 flag 状态
//! - `Kit::register_if_toggle::<M>(key)` — 按 flag 条件注册模块
//!
//! Run: `cargo run -p trait-kit-example --example toggle_basic --features toggle`

use std::sync::Arc;
use trait_kit::prelude::*;

/// 缓存能力。
#[derive(Debug)]
struct CacheCap {
    backend: String,
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

    fn build(_kit: &Kit) -> Result<Arc<CacheCap>, TraitKitError> {
        Ok(Arc::new(CacheCap {
            backend: "redis".into(),
        }))
    }
}

fn main() {
    // ── Scenario 1: toggle enabled → module registered ──────────────────
    let mut kit = Kit::new();
    kit.enable_toggle("cache", true);
    assert!(kit.is_toggle_enabled("cache"));

    let registered = kit
        .register_if_toggle::<CacheModule>("cache")
        .expect("registration should succeed");
    assert!(registered, "module should be registered when toggle is on");

    let kit = kit.build().expect("build should succeed");
    let cap = kit.require::<CacheModule>().expect("cache should exist");
    assert_eq!(cap.backend, "redis");
    println!("Scenario 1 (enabled): cache module registered, backend=redis");

    // ── Scenario 2: toggle disabled → module skipped ────────────────────
    let mut kit2 = Kit::new();
    kit2.enable_toggle("cache", false);
    assert!(!kit2.is_toggle_enabled("cache"));

    let registered2 = kit2
        .register_if_toggle::<CacheModule>("cache")
        .expect("registration should succeed");
    assert!(
        !registered2,
        "module should NOT be registered when toggle is off"
    );

    let kit2 = kit2.build().expect("build should succeed");
    assert!(!kit2.contains::<CacheModule>());
    println!("Scenario 2 (disabled): cache module skipped");

    // ── Scenario 3: unknown toggle defaults to false ────────────────────
    let kit3 = Kit::new();
    assert!(!kit3.is_toggle_enabled("nonexistent"));
    println!("Scenario 3: unknown toggle defaults to false");

    // ── Scenario 4: runtime toggle on Ready state ───────────────────────
    let kit4 = Kit::new();
    kit4.enable_toggle("feature-x", true);
    let kit4 = kit4.build().expect("build should succeed");
    assert!(kit4.is_toggle_enabled("feature-x"));
    kit4.enable_toggle("feature-x", false);
    assert!(!kit4.is_toggle_enabled("feature-x"));
    println!("Scenario 4: toggle can be changed on Ready state");

    println!("toggle_basic: OK");
}
