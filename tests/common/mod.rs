// Copyright © 2026 Kirky.X. All rights reserved.

//! Shared test fixtures for trait-kit integration tests.

#![allow(dead_code)]

use std::sync::Arc;

use trait_kit::prelude::*;

// === Logger capability ===

pub trait Logger: Send + Sync {
    fn info(&self, message: &str);
}

pub struct TestLogger {
    pub prefix: String,
}

impl Logger for TestLogger {
    fn info(&self, message: &str) {
        let _ = format!("{}{}", self.prefix, message);
    }
}

// === Logger config ===

#[derive(Debug, Clone, PartialEq)]
pub struct LoggerConfig {
    pub prefix: String,
}

pub struct LoggerConfigKey;

impl ConfigKey for LoggerConfigKey {
    type Config = LoggerConfig;
    const NAME: &'static str = "logger_config";
}

// === Logger build error ===

#[derive(Debug, Clone)]
pub enum LoggerBuildError {
    EmptyPrefix,
}

impl std::fmt::Display for LoggerBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoggerBuildError::EmptyPrefix => write!(f, "prefix must not be empty"),
        }
    }
}

impl std::error::Error for LoggerBuildError {}

// === MainLogger capability key ===

pub struct MainLogger;

impl CapabilityKey for MainLogger {
    type Capability = dyn Logger + Send + Sync;
    const NAME: &'static str = "main_logger";
}

// === LoggerModule ===

pub struct LoggerModule;

impl Module for LoggerModule {
    const NAME: &'static str = "logger_module";
    type Config = LoggerConfig;
    type Requirements = NoRequirements;
    type Capability = Arc<dyn Logger + Send + Sync>;
    type Error = LoggerBuildError;
    type Builder = LoggerBuilder;
}

// === LoggerBuilder ===

pub struct LoggerBuilder {
    config: Option<LoggerConfig>,
}

impl LoggerBuilder {
    pub fn new() -> Self {
        LoggerBuilder { config: None }
    }
}

impl WithConfig<LoggerModule> for LoggerBuilder {
    fn config(self, config: LoggerConfig) -> Self {
        LoggerBuilder {
            config: Some(config),
        }
    }
}

impl ModuleBuilder<LoggerModule> for LoggerBuilder {
    fn build(self) -> Result<Arc<dyn Logger + Send + Sync>, LoggerBuildError> {
        match self.config {
            Some(c) if !c.prefix.is_empty() => Ok(Arc::new(TestLogger { prefix: c.prefix })),
            Some(_) => Err(LoggerBuildError::EmptyPrefix),
            None => Err(LoggerBuildError::EmptyPrefix),
        }
    }
}

// === UserService capability ===

pub trait UserService: Send + Sync {
    fn greet(&self, name: &str) -> String;
}

// === UserRequirements ===

pub struct UserRequirements {
    pub logger: Arc<dyn Logger + Send + Sync>,
}

// === UserServiceKey ===

pub struct UserServiceKey;

impl CapabilityKey for UserServiceKey {
    type Capability = dyn UserService + Send + Sync;
    const NAME: &'static str = "user_service";
}

// === UserModule ===

pub struct UserModule;

impl Module for UserModule {
    const NAME: &'static str = "user_module";
    type Config = NoConfig;
    type Requirements = UserRequirements;
    type Capability = Arc<dyn UserService + Send + Sync>;
    type Error = BuildError;
    type Builder = UserBuilder;
}

// === UserBuilder ===

pub struct UserBuilder {
    requirements: Option<UserRequirements>,
}

impl UserBuilder {
    pub fn new() -> Self {
        UserBuilder { requirements: None }
    }
}

impl WithRequirements<UserModule> for UserBuilder {
    fn requirements(self, requirements: UserRequirements) -> Self {
        UserBuilder {
            requirements: Some(requirements),
        }
    }
}

impl ModuleBuilder<UserModule> for UserBuilder {
    fn build(self) -> Result<Arc<dyn UserService + Send + Sync>, BuildError> {
        match self.requirements {
            Some(reqs) => {
                struct SimpleUserService {
                    logger: Arc<dyn Logger + Send + Sync>,
                }
                impl UserService for SimpleUserService {
                    fn greet(&self, name: &str) -> String {
                        let msg = format!("Hello, {name}!");
                        self.logger.info(&msg);
                        msg
                    }
                }
                Ok(Arc::new(SimpleUserService {
                    logger: reqs.logger,
                }))
            }
            None => Err(BuildError::MissingRequirements {
                module: UserModule::NAME,
            }),
        }
    }
}
