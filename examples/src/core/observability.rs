// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Observability feature — BuildObserver callbacks during build().
//!
//! Demonstrates:
//! - `BuildObserver` trait with `on_module_start` / `on_module_built` / `on_build_error`
//! - `Kit::with_observer()` to register an observer
//! - Observer receives callbacks for every module built during `build()`
//!
//! Run: `cargo run -p trait-kit-example --example observability --features observability`

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use trait_kit::core::BuildObserver;
use trait_kit::prelude::*;

// ─── Observer implementation ───────────────────────────────────────────────

struct LoggingObserver {
    start_count: Arc<AtomicUsize>,
    built_count: Arc<AtomicUsize>,
}

impl BuildObserver for LoggingObserver {
    fn on_module_start(&self, module_name: &'static str) {
        self.start_count.fetch_add(1, Ordering::SeqCst);
        println!("  [observer] building: {module_name}");
    }

    fn on_module_built(&self, module_name: &'static str, elapsed: Duration) {
        self.built_count.fetch_add(1, Ordering::SeqCst);
        println!(
            "  [observer] built: {module_name} in {}µs",
            elapsed.as_micros()
        );
    }

    fn on_build_error(&self, module_name: &'static str, _error: &TraitKitError) {
        println!("  [observer] FAILED: {module_name}");
    }
}

// ─── Module definition ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct LoggerCap;

struct LoggerModule;

impl ModuleMeta for LoggerModule {
    const NAME: &'static str = "logger";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        &[]
    }
}

impl AutoBuilder for LoggerModule {
    type Capability = Arc<LoggerCap>;
    type Error = TraitKitError;

    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(LoggerCap))
    }
}

fn main() {
    let start_count = Arc::new(AtomicUsize::new(0));
    let built_count = Arc::new(AtomicUsize::new(0));

    let mut kit = Kit::new();
    kit.with_observer(Arc::new(LoggingObserver {
        start_count: Arc::clone(&start_count),
        built_count: Arc::clone(&built_count),
    }));
    kit.register::<LoggerModule>()
        .expect("register LoggerModule");

    let kit = kit.build().expect("build should succeed");
    let _ = kit.require::<LoggerModule>().expect("require LoggerModule");

    assert!(
        start_count.load(Ordering::SeqCst) >= 1,
        "observer should have seen at least 1 start"
    );
    assert!(
        built_count.load(Ordering::SeqCst) >= 1,
        "observer should have seen at least 1 built"
    );

    println!("observability: OK");
}
