// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Factory feature — per-call instance creation (non-singleton).
//!
//! Demonstrates:
//! - `Kit<Ready>::factory::<M>()` — returns a closure that invokes `M::build()`
//!   on every call, producing a fresh instance each time
//! - Unlike `require()` (singleton), factory creates new instances on demand
//!
//! Run: `cargo run -p trait-kit-example --example factory`

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use trait_kit::prelude::*;

static INSTANCE_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone)]
struct ConnectionCap {
    id: usize,
}

struct ConnectionModule;

impl ModuleMeta for ConnectionModule {
    const NAME: &'static str = "connection";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        &[]
    }
}

impl AutoBuilder for ConnectionModule {
    type Capability = Arc<ConnectionCap>;
    type Error = TraitKitError;

    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        let id = INSTANCE_COUNTER.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(ConnectionCap { id }))
    }
}

fn main() {
    INSTANCE_COUNTER.store(0, Ordering::SeqCst);

    let mut kit = Kit::new();
    kit.register::<ConnectionModule>()
        .expect("register ConnectionModule");
    let kit = kit.build().expect("build should succeed");

    // Create a factory — each call produces a fresh instance
    let factory = kit.factory::<ConnectionModule>();

    let conn1 = factory().expect("factory call 1");
    let conn2 = factory().expect("factory call 2");
    let conn3 = factory().expect("factory call 3");

    println!(
        "Factory produced: conn1.id={}, conn2.id={}, conn3.id={}",
        conn1.id, conn2.id, conn3.id
    );

    // Each call creates a new instance with a unique id
    assert_ne!(conn1.id, conn2.id);
    assert_ne!(conn2.id, conn3.id);

    // Meanwhile, require() returns the singleton built during build()
    let singleton = kit.require::<ConnectionModule>().expect("require");
    println!("Singleton id: {}", singleton.id);

    println!("factory: OK");
}
