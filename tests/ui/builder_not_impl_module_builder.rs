// Copyright © 2026 Kirky.X. All rights reserved.
// TC-COMPILE-005: Module::Builder not implementing ModuleBuilder should fail at usage

use std::sync::Arc;
use trait_kit::prelude::*;

// A builder that does NOT implement ModuleBuilder
struct BadBuilder;

struct MyModule;

impl Module for MyModule {
    const NAME: &'static str = "my_module";
    type Config = NoConfig;
    type Requirements = NoRequirements;
    type Capability = Arc<dyn std::any::Any + Send + Sync>;
    type Error = std::convert::Infallible;
    type Builder = BadBuilder;
}

fn main() {
    let kit = Kit::new();
    // WRONG: BadBuilder doesn't implement ModuleBuilder<MyModule>,
    // so .kit() is not available (IntoKitModuleBuilder requires ModuleBuilder)
    let _ = BadBuilder.kit(&kit);
}
