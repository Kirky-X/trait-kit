// Copyright © 2026 Kirky.X. All rights reserved.
// TC-COMPILE-003: .kit().requirements() wrong order should fail to compile

use std::sync::Arc;
use trait_kit::prelude::*;

struct MyRequirements {
    data: Arc<dyn std::any::Any + Send + Sync>,
}

struct MyModule;

impl Module for MyModule {
    const NAME: &'static str = "my_module";
    type Config = NoConfig;
    type Requirements = MyRequirements;
    type Capability = Arc<dyn std::any::Any + Send + Sync>;
    type Error = std::convert::Infallible;
    type Builder = MyBuilder;
}

struct MyBuilder {
    requirements: Option<MyRequirements>,
}

impl MyBuilder {
    fn new() -> Self {
        MyBuilder { requirements: None }
    }
}

impl WithRequirements<MyModule> for MyBuilder {
    fn requirements(self, requirements: MyRequirements) -> Self {
        MyBuilder {
            requirements: Some(requirements),
        }
    }
}

impl ModuleBuilder<MyModule> for MyBuilder {
    fn build(self) -> Result<Arc<dyn std::any::Any + Send + Sync>, std::convert::Infallible> {
        Ok(Arc::new(42i32))
    }
}

fn main() {
    let kit = Kit::new();
    // WRONG: .requirements() after .kit() should not compile
    let _ = MyBuilder::new().kit(&kit).requirements(MyRequirements {
        data: Arc::new(42i32),
    });
}
