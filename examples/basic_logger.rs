// Copyright © 2026 Kirky.X. All rights reserved.

//! TC-DOC-001: Build a logger and register it to Kit.

use std::sync::Arc;
use trait_kit::prelude::*;

trait Logger: Send + Sync {
    fn info(&self, msg: &str);
}

struct ConsoleLogger;

impl Logger for ConsoleLogger {
    fn info(&self, msg: &str) {
        println!("[INFO] {msg}");
    }
}

struct MainLogger;

impl CapabilityKey for MainLogger {
    type Capability = dyn Logger + Send + Sync;
    const NAME: &'static str = "main_logger";
}

struct LoggerModule;

impl Module for LoggerModule {
    const NAME: &'static str = "logger_module";
    type Config = NoConfig;
    type Requirements = NoRequirements;
    type Capability = Arc<dyn Logger + Send + Sync>;
    type Error = std::convert::Infallible;
    type Builder = LoggerModuleBuilder;
}

struct LoggerModuleBuilder;

impl ModuleBuilder<LoggerModule> for LoggerModuleBuilder {
    fn build(self) -> Result<Arc<dyn Logger + Send + Sync>, std::convert::Infallible> {
        Ok(Arc::new(ConsoleLogger))
    }
}

fn main() {
    let kit = Kit::new();

    let logger = LoggerModuleBuilder
        .kit(&kit)
        .provide::<MainLogger>()
        .unwrap();

    logger.info("Logger registered and ready");

    let from_kit = kit.require::<MainLogger>().unwrap();
    from_kit.info("Retrieved from Kit");
}
