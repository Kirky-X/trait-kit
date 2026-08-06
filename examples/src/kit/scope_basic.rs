// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Scope feature — per-request instance isolation.
//!
//! Demonstrates:
//! - `Scope::new()` / `register::<M>()` / `require::<M>()` / `contains::<M>()`
//! - Each scope creates independent instances (not shared with main Kit)
//! - Lazy construction: first `require()` triggers build, result is cached
//!
//! Run: `cargo run -p trait-kit-example --example scope_basic --features scope`

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use trait_kit::prelude::*;

static BUILD_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone)]
struct RequestCap {
    id: usize,
}

struct RequestModule;

impl ModuleMeta for RequestModule {
    const NAME: &'static str = "request";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        &[]
    }
}

impl AutoBuilder for RequestModule {
    type Capability = Arc<RequestCap>;
    type Error = TraitKitError;

    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        let id = BUILD_COUNTER.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(RequestCap { id }))
    }
}

fn main() {
    BUILD_COUNTER.store(0, Ordering::SeqCst);

    // Scope 1 — simulates request 1
    let mut scope1 = Scope::new();
    scope1
        .register::<RequestModule>()
        .expect("register in scope1");
    assert!(scope1.contains::<RequestModule>());

    let cap1 = scope1
        .require::<RequestModule>()
        .expect("require from scope1");
    println!("Scope 1 request id: {}", cap1.id);

    // Scope 2 — simulates request 2 (gets a fresh instance)
    let mut scope2 = Scope::new();
    scope2
        .register::<RequestModule>()
        .expect("register in scope2");

    let cap2 = scope2
        .require::<RequestModule>()
        .expect("require from scope2");
    println!("Scope 2 request id: {}", cap2.id);

    // Each scope has its own independent instance
    assert_ne!(
        cap1.id, cap2.id,
        "different scopes should produce different instances"
    );

    // Require within the same scope returns the cached value
    let cap1_again = scope1.require::<RequestModule>().expect("require again");
    assert_eq!(
        cap1.id, cap1_again.id,
        "same scope should cache the instance"
    );

    println!("scope_basic: OK");
}
