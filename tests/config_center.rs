// Copyright © 2026 Kirky.X. All rights reserved.

//! ConfigStore + ConfigHandle tests (TC-CONFIG-001~006).

mod common;

use common::*;
use trait_kit::prelude::*;

#[test]
fn tc_config_001_set_config_first_time() {
    let kit = Kit::new();
    kit.set_config::<LoggerConfigKey>(LoggerConfig {
        prefix: "[v1] ".to_string(),
    });

    assert!(kit.contains_config::<LoggerConfigKey>());
    assert_eq!(
        kit.config::<LoggerConfigKey>().unwrap().load().prefix,
        "[v1] "
    );
}

#[test]
fn tc_config_002_set_config_updates() {
    let kit = Kit::new();
    kit.set_config::<LoggerConfigKey>(LoggerConfig {
        prefix: "[v1] ".to_string(),
    });
    kit.set_config::<LoggerConfigKey>(LoggerConfig {
        prefix: "[v2] ".to_string(),
    });

    assert_eq!(
        kit.config::<LoggerConfigKey>().unwrap().load().prefix,
        "[v2] "
    );
}

#[test]
fn tc_config_003_multiple_handles_share_same_slot() {
    let kit = Kit::new();
    kit.set_config::<LoggerConfigKey>(LoggerConfig {
        prefix: "[v1] ".to_string(),
    });

    let handle_1 = kit.config::<LoggerConfigKey>().unwrap();
    let handle_2 = kit.config::<LoggerConfigKey>().unwrap();
    let handle_3 = handle_1.clone();

    handle_1.set(LoggerConfig {
        prefix: "[v2] ".to_string(),
    });

    assert_eq!(handle_2.load().prefix, "[v2] ");
    assert_eq!(handle_3.load().prefix, "[v2] ");
}

#[test]
fn tc_config_004_old_snapshot_survives_update() {
    let kit = Kit::new();
    kit.set_config::<LoggerConfigKey>(LoggerConfig {
        prefix: "[v1] ".to_string(),
    });

    let handle = kit.config::<LoggerConfigKey>().unwrap();
    let old = handle.load();

    handle.set(LoggerConfig {
        prefix: "[v2] ".to_string(),
    });

    let new = handle.load();

    assert_eq!(old.prefix, "[v1] ");
    assert_eq!(new.prefix, "[v2] ");
}

#[test]
fn tc_config_005_read_missing_config_fails() {
    let kit = Kit::new();
    let result = kit.config::<LoggerConfigKey>();

    assert!(matches!(
        result,
        Err(KitError::MissingConfig {
            key: "logger_config"
        })
    ));
}

#[test]
fn tc_config_006_contains_missing_returns_false() {
    let kit = Kit::new();
    assert!(!kit.contains_config::<LoggerConfigKey>());

    assert!(matches!(
        kit.config::<LoggerConfigKey>(),
        Err(KitError::MissingConfig {
            key: "logger_config"
        })
    ));
}
