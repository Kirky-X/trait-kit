// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Capability Guard — 运行时策略钩子。
//!
//! Demonstrates:
//! - `CapabilityGuard` trait — 生态侧实现的运行时策略注入点
//! - `Kit::require_guarded::<M>(caller, &guard)` — visibility + guard 双关卡
//!
//! Run: `cargo run -p trait-kit-examples --example guard --features guard`

use std::sync::atomic::{AtomicU32, Ordering};

use trait_kit::prelude::*;

/// 数据库能力。
#[derive(Debug)]
struct DbPool {
    max_connections: u32,
}

struct DatabaseModule;
impl ModuleMeta for DatabaseModule {
    const NAME: &'static str = "data::Database";
}
impl AutoBuilder for DatabaseModule {
    type Capability = std::sync::Arc<DbPool>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(std::sync::Arc::new(DbPool {
            max_connections: 16,
        }))
    }
}

struct AuditModule;
impl ModuleMeta for AuditModule {
    const NAME: &'static str = "audit::Audit";
}

/// 限流 mock：每调用方最多放行 N 次（真实实现归 limiteron 等生态 crate）。
struct RateLimitGuard {
    limit: u32,
    used: AtomicU32,
}

impl CapabilityGuard for RateLimitGuard {
    fn allow(&self, _caller: &'static str, _target: &'static str) -> bool {
        let used = self.used.fetch_add(1, Ordering::SeqCst);
        used < self.limit
    }
}

fn main() {
    let mut kit = Kit::new();
    kit.register::<DatabaseModule>().expect("register database");
    let ready = kit.build().expect("build succeeds");

    let guard = RateLimitGuard {
        limit: 2,
        used: AtomicU32::new(0),
    };

    // ── Scenario 1: 配额内两次放行 ─────────────────────────────────────
    let p1 = ready
        .require_guarded::<DatabaseModule>("audit::Audit", &guard)
        .expect("first call within quota");
    println!("[1st] allowed (max_connections = {})", p1.max_connections);
    let _p2 = ready
        .require_guarded::<DatabaseModule>("audit::Audit", &guard)
        .expect("second call within quota");
    println!("[2nd] allowed");

    // ── Scenario 2: 超出配额被拒绝（AccessDenied） ──────────────────────
    match ready.require_guarded::<DatabaseModule>("audit::Audit", &guard) {
        Err(TraitKitError::AccessDenied { module, caller }) => {
            println!("[3rd] AccessDenied: module = {module}, caller = {caller}");
        }
        other => panic!("quota exhausted, must be denied, got {other:?}"),
    }

    // ── Scenario 3: visibility 与 guard 叠加（静态边界优先） ────────────
    let mut strict_kit = Kit::new();
    strict_kit
        .register_with_visibility::<DatabaseModule>(CapabilityVisibility::Restricted(&["audit::*"]))
        .expect("register restricted");
    let strict = strict_kit.build().expect("build succeeds");
    let fresh_guard = RateLimitGuard {
        limit: 10,
        used: AtomicU32::new(0),
    };
    // guard 放行，但 caller 不在 audit::* 允许列表 → 拒绝。
    match strict.require_guarded::<DatabaseModule>("plugin::ThirdParty", &fresh_guard) {
        Err(TraitKitError::AccessDenied { .. }) => {
            println!("[plugin] blocked by static visibility boundary");
        }
        other => panic!("plugin caller must be denied, got {other:?}"),
    }
    // 允许列表内的类型驱动调用方照常放行。
    let _audited = strict
        .require_as::<DatabaseModule, AuditModule>()
        .expect("audit caller is inside the allow-list");

    println!("guard example finished");
}
