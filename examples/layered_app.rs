// Copyright © 2026 Kirky.X. All rights reserved.

//! TC-DOC-004: Layered composition — logger → storage → user service → app.

use std::sync::Arc;
use trait_kit::prelude::*;

// === Logger ===

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

// === Storage ===

trait Storage: Send + Sync {
    fn store(&self, key: &str, value: &str);
}

struct MemoryStorage {
    logger: Arc<dyn Logger + Send + Sync>,
}

impl Storage for MemoryStorage {
    fn store(&self, key: &str, value: &str) {
        self.logger.info(&format!("storing {key}={value}"));
    }
}

struct MainStorage;

impl CapabilityKey for MainStorage {
    type Capability = dyn Storage + Send + Sync;
    const NAME: &'static str = "main_storage";
}

struct StorageModule;

impl Module for StorageModule {
    const NAME: &'static str = "storage_module";
    type Config = NoConfig;
    type Requirements = NoRequirements;
    type Capability = Arc<dyn Storage + Send + Sync>;
    type Error = std::convert::Infallible;
    type Builder = StorageBuilder;
}

struct StorageBuilder {
    logger: Option<Arc<dyn Logger + Send + Sync>>,
}

impl StorageBuilder {
    fn new() -> Self {
        StorageBuilder { logger: None }
    }

    fn logger(self, logger: Arc<dyn Logger + Send + Sync>) -> Self {
        StorageBuilder {
            logger: Some(logger),
        }
    }
}

impl ModuleBuilder<StorageModule> for StorageBuilder {
    fn build(self) -> Result<Arc<dyn Storage + Send + Sync>, std::convert::Infallible> {
        Ok(Arc::new(MemoryStorage {
            logger: self.logger.unwrap(),
        }))
    }
}

// === User Service ===

trait UserService: Send + Sync {
    fn create_user(&self, name: &str);
}

struct AppUserService {
    logger: Arc<dyn Logger + Send + Sync>,
    storage: Arc<dyn Storage + Send + Sync>,
}

impl UserService for AppUserService {
    fn create_user(&self, name: &str) {
        self.logger.info(&format!("creating user: {name}"));
        self.storage.store("last_user", name);
    }
}

struct UserServiceKey;

impl CapabilityKey for UserServiceKey {
    type Capability = dyn UserService + Send + Sync;
    const NAME: &'static str = "user_service";
}

struct UserModule;

impl Module for UserModule {
    const NAME: &'static str = "user_module";
    type Config = NoConfig;
    type Requirements = NoRequirements;
    type Capability = Arc<dyn UserService + Send + Sync>;
    type Error = std::convert::Infallible;
    type Builder = UserBuilder;
}

struct UserBuilder {
    logger: Option<Arc<dyn Logger + Send + Sync>>,
    storage: Option<Arc<dyn Storage + Send + Sync>>,
}

impl UserBuilder {
    fn new() -> Self {
        UserBuilder {
            logger: None,
            storage: None,
        }
    }

    fn logger(self, logger: Arc<dyn Logger + Send + Sync>) -> Self {
        UserBuilder {
            logger: Some(logger),
            ..self
        }
    }

    fn storage(self, storage: Arc<dyn Storage + Send + Sync>) -> Self {
        UserBuilder {
            storage: Some(storage),
            ..self
        }
    }
}

impl ModuleBuilder<UserModule> for UserBuilder {
    fn build(self) -> Result<Arc<dyn UserService + Send + Sync>, std::convert::Infallible> {
        Ok(Arc::new(AppUserService {
            logger: self.logger.unwrap(),
            storage: self.storage.unwrap(),
        }))
    }
}

fn main() {
    let kit = Kit::new();

    // Layer 1: Logger
    let logger = LoggerModuleBuilder
        .kit(&kit)
        .provide::<MainLogger>()
        .unwrap();

    // Layer 2: Storage (depends on Logger)
    let storage = StorageBuilder::new()
        .logger(logger.clone())
        .kit(&kit)
        .provide::<MainStorage>()
        .unwrap();

    // Layer 3: User Service (depends on Logger + Storage)
    let user_service = UserBuilder::new()
        .logger(logger)
        .storage(storage)
        .kit(&kit)
        .provide::<UserServiceKey>()
        .unwrap();

    // Use the app
    user_service.create_user("Alice");
}
