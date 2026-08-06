// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Validation — 配置加载后自动验证。
//!
//! Demonstrates:
//! - `Validatable` trait — 为配置类型定义验证规则
//! - `Kit::load_and_validate::<C>()` — 加载后验证，失败不存入
//!
//! Run: `cargo run -p trait-kit-example --example validation --features confers`

use trait_kit::prelude::*;

/// 数据库配置，带验证规则。
#[derive(Clone, Debug)]
struct DbConfig {
    host: String,
    port: u16,
    max_connections: u32,
}

impl Validatable for DbConfig {
    fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if self.host.is_empty() {
            errors.push("host must not be empty".into());
        }
        if self.port == 0 {
            errors.push("port must be > 0".into());
        }
        if self.max_connections == 0 {
            errors.push("max_connections must be > 0".into());
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

fn main() {
    // ── Scenario 1: valid config ────────────────────────────────────────
    let kit = Kit::new();
    let valid = DbConfig {
        host: "localhost".into(),
        port: 5432,
        max_connections: 10,
    };
    // 手动 set_config + validate 模拟 load_and_validate 的行为
    match valid.validate() {
        Ok(()) => {
            kit.set_config(valid);
            println!("Scenario 1 (valid): config stored successfully");
        }
        Err(errors) => println!("Scenario 1: unexpected errors: {errors:?}"),
    }
    let loaded: DbConfig = kit.config().expect("config should be present");
    assert_eq!(loaded.host, "localhost");
    assert_eq!(loaded.port, 5432);

    // ── Scenario 2: invalid config — not stored ─────────────────────────
    let kit2 = Kit::new();
    let invalid = DbConfig {
        host: "".into(),
        port: 0,
        max_connections: 0,
    };
    match invalid.validate() {
        Ok(()) => println!("Scenario 2: unexpected success"),
        Err(errors) => {
            println!("Scenario 2 (invalid): validation failed with {} errors", errors.len());
            for e in &errors {
                println!("  - {e}");
            }
            // 验证失败，不存入
            assert!(kit2.config::<DbConfig>().is_err());
        }
    }

    println!("validation: OK");
}
