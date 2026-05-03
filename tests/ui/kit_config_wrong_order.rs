// Copyright © 2026 Kirky.X. All rights reserved.
// TC-COMPILE-002: .kit().config() wrong order should fail to compile

use trait_kit::prelude::*;

struct MyConfig {
    value: i32,
}

struct MyModule;

impl Module for MyModule {
    const NAME: &'static str = "my_module";
    type Config = MyConfig;
    type Requirements = NoRequirements;
    type Capability = std::sync::Arc<dyn std::any::Any + Send + Sync>;
    type Error = std::convert::Infallible;
    type Builder = MyBuilder;
}

struct MyBuilder {
    config: Option<MyConfig>,
}

impl MyBuilder {
    fn new() -> Self {
        MyBuilder { config: None }
    }
}

impl WithConfig<MyModule> for MyBuilder {
    fn config(self, config: MyConfig) -> Self {
        MyBuilder {
            config: Some(config),
        }
    }
}

impl ModuleBuilder<MyModule> for MyBuilder {
    fn build(
        self,
    ) -> Result<std::sync::Arc<dyn std::any::Any + Send + Sync>, std::convert::Infallible> {
        Ok(std::sync::Arc::new(MyConfig { value: 42 }))
    }
}

fn main() {
    let kit = Kit::new();
    // WRONG: .config() after .kit() should not compile
    let _ = MyBuilder::new().kit(&kit).config(MyConfig { value: 1 });
}
