// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Lifecycle feature — on_ready + on_shutdown hooks.
//!
//! Demonstrates:
//! - `Lifecycle` trait with `on_ready` (post-build) and `on_shutdown` (cleanup)
//! - `Kit::register_lifecycle::<M>()` to register hooks
//! - `Kit<Ready>::shutdown()` to invoke shutdown in reverse order
//!
//! Run: `cargo run -p trait-kit-example --example lifecycle --features lifecycle`

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use trait_kit::prelude::*;

static SHUTDOWN_CALLED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone)]
struct DatabaseCap {
    connection_string: String,
}

struct DatabaseModule;

impl ModuleMeta for DatabaseModule {
    const NAME: &'static str = "database";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        &[]
    }
}

impl AutoBuilder for DatabaseModule {
    type Capability = Arc<DatabaseCap>;
    type Error = TraitKitError;

    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(DatabaseCap {
            connection_string: "postgres://localhost/mydb".to_string(),
        }))
    }
}

impl Lifecycle for DatabaseModule {
    fn on_ready(_kit: &Kit<Ready>) -> Result<(), Self::Error> {
        println!("  [lifecycle] database on_ready: all modules built");
        Ok(())
    }

    fn on_shutdown(_cap: &Arc<DatabaseCap>) {
        SHUTDOWN_CALLED.store(true, Ordering::SeqCst);
        println!("  [lifecycle] database on_shutdown: closing connection");
    }
}

fn main() {
    let mut kit = Kit::new();
    kit.register::<DatabaseModule>()
        .expect("register DatabaseModule");
    kit.register_lifecycle::<DatabaseModule>();

    let kit = kit.build().expect("build should succeed");
    // on_ready has been called during build()

    let db = kit
        .require::<DatabaseModule>()
        .expect("require DatabaseModule");
    println!("Database: {}", db.connection_string);

    // Shutdown in reverse topological order
    kit.shutdown();
    assert!(
        SHUTDOWN_CALLED.load(Ordering::SeqCst),
        "on_shutdown should have been called"
    );

    println!("lifecycle: OK");
}
