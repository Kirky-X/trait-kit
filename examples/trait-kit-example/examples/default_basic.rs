// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Default feature — no confers, no macros.
//!
//! Demonstrates the core typestate flow: `Kit<Unbuilt>` → `Kit<Ready>`.
//! Run: `cargo run -p trait-kit-example --example default_basic`

use std::sync::Arc;
use trait_kit::prelude::*;

struct StdoutLogger;
impl StdoutLogger {
    fn info(&self, msg: &str) {
        println!("[LOG] {msg}");
    }
}

struct LoggerModule;
impl ModuleMeta for LoggerModule {
    const NAME: &'static str = "logger";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        &[]
    }
}

impl AutoBuilder for LoggerModule {
    type Capability = Arc<StdoutLogger>;
    type Error = KitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(StdoutLogger))
    }
}

fn main() {
    let mut kit = Kit::new();
    kit.register::<LoggerModule>()
        .expect("register LoggerModule");
    let kit = kit.build().expect("build should succeed");

    let logger = kit.require::<LoggerModule>().expect("require LoggerModule");
    logger.info("Hello from trait-kit!");
    assert!(
        kit.contains::<LoggerModule>(),
        "contains should report true"
    );
    assert!(
        kit.optional::<LoggerModule>().is_some(),
        "optional should return Some"
    );
    assert!(
        !kit.contains_config::<()>(),
        "contains_config should be false for unset type"
    );

    println!("default_basic: OK");
}
