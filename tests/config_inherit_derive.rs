//! Integration tests for `#[derive(ConfigInherit)]`.

#![cfg(feature = "confers")]

use trait_kit::kit::{ConfigInherit, Kit};
use trait_kit_derive::ConfigInherit;

// ── Basic derive ──

#[derive(Clone, Debug, PartialEq, ConfigInherit)]
struct BasicDbConfig {
    host: String,
    port: u16,
    max_connections: u32,
}

#[test]
fn derive_config_inherit_generates_override_type() {
    // The generated Override type should be BasicDbConfigOverride
    let ovr = BasicDbConfigOverride {
        host: Some("new-host".into()),
        port: None,
        max_connections: Some(200),
    };

    let mut cfg = BasicDbConfig {
        host: "old-host".into(),
        port: 3306,
        max_connections: 10,
    };

    cfg.apply_override(&ovr);

    assert_eq!(cfg.host, "new-host");
    assert_eq!(cfg.port, 3306); // unchanged
    assert_eq!(cfg.max_connections, 200);
}

#[test]
fn derive_config_inherit_default_override_is_all_none() {
    let ovr = BasicDbConfigOverride::default();
    let mut cfg = BasicDbConfig {
        host: "keep".into(),
        port: 5432,
        max_connections: 50,
    };
    cfg.apply_override(&ovr);

    assert_eq!(cfg.host, "keep");
    assert_eq!(cfg.port, 5432);
    assert_eq!(cfg.max_connections, 50);
}

#[test]
fn derive_config_inherit_works_with_kit_merge_config() {
    let kit = Kit::new();
    kit.set_config(BasicDbConfig {
        host: "original".into(),
        port: 3306,
        max_connections: 10,
    });

    kit.merge_config::<BasicDbConfig>(BasicDbConfigOverride {
        host: Some("merged".into()),
        port: Some(9999),
        max_connections: None,
    });

    let cfg: BasicDbConfig = kit.config().unwrap();
    assert_eq!(cfg.host, "merged");
    assert_eq!(cfg.port, 9999);
    assert_eq!(cfg.max_connections, 10); // unchanged
}

// ── Custom override name ──

#[derive(Clone, Debug, PartialEq, ConfigInherit)]
#[config_inherit(name = "MyCustomOverride")]
struct CustomNameConfig {
    value: String,
}

#[test]
fn derive_config_inherit_custom_override_name() {
    let ovr = MyCustomOverride {
        value: Some("custom".into()),
    };
    let mut cfg = CustomNameConfig {
        value: "original".into(),
    };
    cfg.apply_override(&ovr);
    assert_eq!(cfg.value, "custom");
}

// ── Nested ConfigInherit ──

#[derive(Clone, Debug, PartialEq, ConfigInherit)]
struct PoolConfig {
    min_size: u32,
    max_size: u32,
}

#[derive(Clone, Debug, PartialEq, ConfigInherit)]
struct NestedDbConfig {
    host: String,
    #[config_inherit(nested)]
    pool: PoolConfig,
}

#[test]
fn derive_config_inherit_nested_delegates() {
    let ovr = NestedDbConfigOverride {
        host: Some("nested-host".into()),
        pool: Some(PoolConfigOverride {
            min_size: None,
            max_size: Some(50),
        }),
    };

    let mut cfg = NestedDbConfig {
        host: "old".into(),
        pool: PoolConfig {
            min_size: 5,
            max_size: 20,
        },
    };

    cfg.apply_override(&ovr);

    assert_eq!(cfg.host, "nested-host");
    assert_eq!(cfg.pool.min_size, 5); // unchanged via nested
    assert_eq!(cfg.pool.max_size, 50); // overridden via nested
}

// ── Visibility test ──

#[derive(Clone, Debug, PartialEq, ConfigInherit)]
pub struct PubVisibilityConfig {
    pub host: String,
    pub port: u16,
}

#[test]
fn derive_config_inherit_pub_struct_override_is_accessible() {
    // The generated Override type for a pub struct should be usable
    let ovr = PubVisibilityConfigOverride {
        host: Some("visible".into()),
        port: None,
    };
    let mut cfg = PubVisibilityConfig {
        host: "original".into(),
        port: 80,
    };
    cfg.apply_override(&ovr);
    assert_eq!(cfg.host, "visible");
}
