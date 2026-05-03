// Copyright © 2026 Kirky.X. All rights reserved.
// TC-COMPILE-001: capability type mismatch should fail to compile

use std::sync::Arc;
use trait_kit::prelude::*;

// Logger trait
trait Logger: Send + Sync {
    fn log(&self, msg: &str);
}

// Concrete logger (not trait object)
struct ConcreteLogger;

impl Logger for ConcreteLogger {
    fn log(&self, _msg: &str) {}
}

// Capability key expects trait object
struct MainLogger;

impl CapabilityKey for MainLogger {
    type Capability = dyn Logger + Send + Sync; // trait object
    const NAME: &'static str = "main_logger";
}

// Module with wrong capability type (Arc<ConcreteLogger> instead of Arc<dyn Logger>)
struct BadLoggerModule;

impl Module for BadLoggerModule {
    const NAME: &'static str = "bad_logger_module";
    type Config = NoConfig;
    type Requirements = NoRequirements;
    type Capability = Arc<ConcreteLogger>; // WRONG: should be Arc<dyn Logger + Send + Sync>
    type Error = std::convert::Infallible;
    type Builder = BadLoggerBuilder;
}

struct BadLoggerBuilder;

impl BadLoggerBuilder {
    fn new() -> Self {
        BadLoggerBuilder
    }
}

impl ModuleBuilder<BadLoggerModule> for BadLoggerBuilder {
    fn build(self) -> Result<Arc<ConcreteLogger>, std::convert::Infallible> {
        Ok(Arc::new(ConcreteLogger))
    }
}

fn main() {
    let kit = Kit::new();
    let _ = BadLoggerBuilder::new().kit(&kit).provide::<MainLogger>(); // should fail: Arc<ConcreteLogger> != Arc<dyn Logger>
}
