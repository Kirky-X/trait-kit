// Copyright © 2026 Kirky.X. All rights reserved.

//! Core Module trait tests (TC-CORE-001~003).

mod common;

use std::sync::Arc;

use trait_kit::prelude::*;

use common::*;

#[test]
fn tc_core_001_no_config_no_requirements_builds() {
    struct NoopCapability;
    struct NoopModule;

    impl Module for NoopModule {
        const NAME: &'static str = "noop";
        type Config = NoConfig;
        type Requirements = NoRequirements;
        type Capability = Arc<NoopCapability>;
        type Error = std::convert::Infallible;
        type Builder = NoopBuilder;
    }

    struct NoopBuilder;

    impl ModuleBuilder<NoopModule> for NoopBuilder {
        fn build(self) -> Result<Arc<NoopCapability>, std::convert::Infallible> {
            Ok(Arc::new(NoopCapability))
        }
    }

    let result = NoopBuilder.build();
    assert!(result.is_ok());
}

#[test]
fn tc_core_002_with_config_builds_successfully() {
    let result = LoggerBuilder::new()
        .config(LoggerConfig {
            prefix: "[test] ".to_string(),
        })
        .build();

    assert!(result.is_ok());
}

#[test]
fn tc_core_002_invalid_config_returns_module_error() {
    let result = LoggerBuilder::new()
        .config(LoggerConfig {
            prefix: "".to_string(),
        })
        .build();

    assert!(result.is_err());
    assert!(matches!(result, Err(LoggerBuildError::EmptyPrefix)));
}

#[test]
fn tc_core_003_with_requirements_builds_successfully() {
    let logger: Arc<dyn Logger + Send + Sync> = Arc::new(TestLogger {
        prefix: "[test] ".to_string(),
    });

    let result = UserBuilder::new()
        .requirements(UserRequirements {
            logger: logger.clone(),
        })
        .build();

    assert!(result.is_ok());
}

#[test]
fn tc_core_003_missing_requirements_returns_error() {
    let result = UserBuilder::new().build();

    assert!(result.is_err());
    assert!(matches!(
        result,
        Err(BuildError::MissingRequirements {
            module: "user_module"
        })
    ));
}
