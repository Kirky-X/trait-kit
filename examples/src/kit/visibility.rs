// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Capability Visibility — 能力可见性控制。
//!
//! Demonstrates:
//! - `Kit::register_with_visibility::<M>(CapabilityVisibility::Restricted(..))`
//! - `Kit::require_as::<M, Caller>()` — 类型驱动调用方检索
//! - `Kit::require_for::<M>(caller)` — 字符串调用方检索
//! - `TraitKitError::AccessDenied` — 越权检索被拒绝
//!
//! Run: `cargo run -p trait-kit-examples --example visibility --features visibility`

use std::sync::Arc;

use trait_kit::prelude::*;

/// 数据库连接池能力。
#[derive(Debug)]
struct DbPool {
    max_connections: u32,
}

/// 数据模块：能力仅对 audit::* 调用方可见。
struct DatabaseModule;
impl ModuleMeta for DatabaseModule {
    const NAME: &'static str = "data::Database";
}
impl AutoBuilder for DatabaseModule {
    type Capability = Arc<DbPool>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(DbPool {
            max_connections: 16,
        }))
    }
}

/// 审计模块：在 data::* 允许列表内。
struct AuditModule;
impl ModuleMeta for AuditModule {
    const NAME: &'static str = "audit::Audit";
}

/// 第三方插件模块：不在允许列表内。
struct PluginModule;
impl ModuleMeta for PluginModule {
    const NAME: &'static str = "plugin::ThirdParty";
}

fn main() {
    let mut kit = Kit::new();
    kit.register_with_visibility::<DatabaseModule>(CapabilityVisibility::Restricted(&["audit::*"]))
        .expect("register with visibility");

    let ready = kit.build().expect("build succeeds");

    // ── Scenario 1: 允许列表内的调用方检索成功 ─────────────────────────
    let pool = ready
        .require_as::<DatabaseModule, AuditModule>()
        .expect("audit caller matches the audit::* pattern");
    println!(
        "[audit] require_as ok: max_connections = {}",
        pool.max_connections
    );

    // ── Scenario 2: 允许列表外的调用方被拒绝 ───────────────────────────
    match ready.require_as::<DatabaseModule, PluginModule>() {
        Err(TraitKitError::AccessDenied { module, caller }) => {
            println!("[plugin] AccessDenied: module = {module}, caller = {caller}");
        }
        other => panic!("plugin caller must be denied, got {other:?}"),
    }

    // ── Scenario 3: 字符串调用方同一边界 ───────────────────────────────
    assert!(ready.require_for::<DatabaseModule>("audit::Audit").is_ok());
    assert!(
        ready
            .require_for::<DatabaseModule>("plugin::ThirdParty")
            .is_err()
    );

    // ── Scenario 4: 无 caller 的 require 是容器级持有者，不受约束 ──────
    let _container_pool = ready.require::<DatabaseModule>().expect("container holder");
    println!("[container] caller-less require keeps current semantics");

    // ── Scenario 5: 只读查询可见性声明 ─────────────────────────────────
    println!(
        "[query] visibility_of(Database) = {:?}",
        ready.visibility_of::<DatabaseModule>()
    );

    println!("visibility example finished");
}
