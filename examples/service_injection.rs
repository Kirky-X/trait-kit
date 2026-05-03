// Copyright © 2026 Kirky.X. All rights reserved.

//! TC-DOC-002: Inject logger into user service via Kit.

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

trait UserService: Send + Sync {
    fn greet(&self, name: &str);
}

struct SimpleUserService {
    logger: Arc<dyn Logger + Send + Sync>,
}

impl UserService for SimpleUserService {
    fn greet(&self, name: &str) {
        self.logger.info(&format!("Hello, {name}!"));
    }
}

struct UserServiceKey;

impl CapabilityKey for UserServiceKey {
    type Capability = dyn UserService + Send + Sync;
    const NAME: &'static str = "user_service";
}

struct UserServiceModule;

impl Module for UserServiceModule {
    const NAME: &'static str = "user_service_module";
    type Config = NoConfig;
    type Requirements = NoRequirements;
    type Capability = Arc<dyn UserService + Send + Sync>;
    type Error = std::convert::Infallible;
    type Builder = UserServiceBuilder;
}

struct UserServiceBuilder {
    logger: Option<Arc<dyn Logger + Send + Sync>>,
}

impl UserServiceBuilder {
    fn new() -> Self {
        UserServiceBuilder { logger: None }
    }

    fn logger(self, logger: Arc<dyn Logger + Send + Sync>) -> Self {
        UserServiceBuilder {
            logger: Some(logger),
        }
    }
}

impl ModuleBuilder<UserServiceModule> for UserServiceBuilder {
    fn build(self) -> Result<Arc<dyn UserService + Send + Sync>, std::convert::Infallible> {
        let logger = self.logger.unwrap();
        Ok(Arc::new(SimpleUserService { logger }))
    }
}

fn main() {
    let kit = Kit::new();

    let logger = LoggerModuleBuilder
        .kit(&kit)
        .provide::<MainLogger>()
        .unwrap();

    let user_service = UserServiceBuilder::new()
        .logger(logger)
        .kit(&kit)
        .provide::<UserServiceKey>()
        .unwrap();

    user_service.greet("Alice");
}
