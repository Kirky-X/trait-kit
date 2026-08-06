// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Interface feature — interface/implementation separation via dyn Trait.
//!
//! Demonstrates:
//! - `InterfaceBuilder` trait for type-erased module registration
//! - `Kit::register_as::<M>()` — register behind a `dyn Trait` interface
//! - `Kit::resolve::<I>()` — retrieve by interface type (`Arc<dyn Trait>`)
//!
//! Run: `cargo run -p trait-kit-example --example interface --features interface`

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use trait_kit::core::InterfaceBuilder;
use trait_kit::prelude::*;

// ─── Interface (trait object) ──────────────────────────────────────────────

trait Logger: 'static + Send + Sync {
    fn log(&self, msg: &str);
}

// ─── Concrete implementation ───────────────────────────────────────────────

struct ConsoleLogger {
    counter: AtomicUsize,
}

impl Logger for ConsoleLogger {
    fn log(&self, msg: &str) {
        let n = self.counter.fetch_add(1, Ordering::SeqCst);
        println!("  [log #{n}] {msg}");
    }
}

// ─── Module that provides ConsoleLogger behind dyn Logger ──────────────────

struct ConsoleLoggerModule;

impl ModuleMeta for ConsoleLoggerModule {
    const NAME: &'static str = "console-logger";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        &[]
    }
}

impl InterfaceBuilder for ConsoleLoggerModule {
    type Interface = dyn Logger;
    type Capability = Arc<ConsoleLogger>;
    type Error = TraitKitError;

    fn build(_kit: &Kit) -> Result<Arc<ConsoleLogger>, TraitKitError> {
        Ok(Arc::new(ConsoleLogger {
            counter: AtomicUsize::new(0),
        }))
    }

    fn into_interface(cap: Arc<ConsoleLogger>) -> Arc<dyn Logger> {
        cap
    }
}

fn main() {
    let mut kit = Kit::new();
    kit.register_as::<ConsoleLoggerModule>()
        .expect("register_as ConsoleLoggerModule");
    let kit = kit.build().expect("build should succeed");

    // Retrieve by interface type (not concrete type)
    let logger: Arc<dyn Logger> = kit.resolve::<dyn Logger>().expect("resolve dyn Logger");

    logger.log("Hello from interface-based DI!");
    logger.log("Second message");

    println!("interface: OK");
}
