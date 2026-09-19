// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Snapshot & Restore — 配置快照与回滚。
//!
//! Demonstrates:
//! - `Kit::snapshot_config::<C>()` — 快照当前配置
//! - `Kit::set_config()` — 覆盖配置
//! - `Kit::restore_config::<C>()` — 回滚到快照
//! - `Kit::has_snapshot::<C>()` — 检查快照是否存在
//!
//! Run: `cargo run -p trait-kit-example --example snapshot_restore --features confers`

use trait_kit::prelude::*;

#[derive(Clone, Debug, PartialEq)]
struct TraitKitConfig {
    debug: bool,
    log_level: String,
}

fn main() {
    let kit = Kit::new();

    // ── Step 1: set initial config ──────────────────────────────────────
    kit.set_config(TraitKitConfig {
        debug: false,
        log_level: "info".into(),
    });
    let current: TraitKitConfig = kit.config().expect("config should exist");
    assert!(!current.debug);
    assert_eq!(current.log_level, "info");
    println!("Step 1: initial config — debug=false, log_level=info");

    // ── Step 2: snapshot the current config ─────────────────────────────
    let snapped = kit.snapshot_config::<TraitKitConfig>();
    assert!(snapped, "snapshot should succeed when config exists");
    assert!(kit.has_snapshot::<TraitKitConfig>());
    println!("Step 2: snapshot created");

    // ── Step 3: override config (e.g. hot-reload scenario) ──────────────
    kit.set_config(TraitKitConfig {
        debug: true,
        log_level: "trace".into(),
    });
    let updated: TraitKitConfig = kit.config().expect("config should exist");
    assert!(updated.debug);
    assert_eq!(updated.log_level, "trace");
    println!("Step 3: config overridden — debug=true, log_level=trace");

    // ── Step 4: restore to snapshot ─────────────────────────────────────
    kit.restore_config::<TraitKitConfig>()
        .expect("restore should succeed");
    let restored: TraitKitConfig = kit.config().expect("config should exist after restore");
    assert!(!restored.debug);
    assert_eq!(restored.log_level, "info");
    println!("Step 4: restored — debug=false, log_level=info");

    // ── Step 5: snapshot for non-existent type returns false ────────────
    assert!(!kit.has_snapshot::<String>());
    let snapped_missing = kit.snapshot_config::<String>();
    assert!(!snapped_missing, "snapshot should fail when config missing");
    println!("Step 5: snapshot for missing config returns false");

    println!("snapshot_restore: OK");
}
