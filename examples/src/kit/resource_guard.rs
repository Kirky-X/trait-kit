// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Resource Guard — 敏感资源标记与 shutdown 可核查报告。
//!
//! Demonstrates:
//! - `Kit::mark_sensitive::<M>()` — 标记持有敏感资源（连接池、密钥、会话存储）的模块
//! - `Kit<Ready>::sensitive_modules()` — 拓扑序列出敏感模块
//! - `Kit<Ready>::shutdown_with_report()` — 逐模块报告 on_shutdown 是否确认执行
//!
//! Run: `cargo run -p trait-kit-examples --example resource_guard --features resource-guard`

use trait_kit::prelude::*;

#[derive(Clone, Debug)]
struct Cap;

#[derive(Debug)]
struct BuildErr;
impl std::fmt::Display for BuildErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "resource-guard example error")
    }
}
impl std::error::Error for BuildErr {}

/// 连接池模块：典型的敏感资源持有者（清理必须可核查）。
struct ConnectionPool;
impl ModuleMeta for ConnectionPool {
    const NAME: &'static str = "ex::connection-pool";
}
impl AutoBuilder for ConnectionPool {
    type Capability = Cap;
    type Error = BuildErr;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Cap)
    }
}
impl Lifecycle for ConnectionPool {
    fn on_shutdown(_cap: &Self::Capability) {
        println!("[pool] connections closed");
    }
}

/// 会话存储模块：依赖连接池（拓扑序在后），同样敏感但未注册生命周期
/// 回调——演示 skipped 分支。
struct SessionStore;
impl ModuleMeta for SessionStore {
    const NAME: &'static str = "ex::session-store";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        static DEPS: &[(&str, std::any::TypeId)] = &[(
            <ConnectionPool as ModuleMeta>::NAME,
            std::any::TypeId::of::<ConnectionPool>(),
        )];
        DEPS
    }
}
impl AutoBuilder for SessionStore {
    type Capability = Cap;
    type Error = BuildErr;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Cap)
    }
}

fn main() {
    let mut kit = Kit::new();
    kit.register::<ConnectionPool>().expect("register pool");
    kit.register_lifecycle::<ConnectionPool>();
    kit.register::<SessionStore>().expect("register store");
    // 幂等：重复标记只保留一个条目。
    kit.mark_sensitive::<ConnectionPool>();
    kit.mark_sensitive::<ConnectionPool>();
    kit.mark_sensitive::<SessionStore>();

    let ready = kit.build().expect("build");

    // ── Sensitive inventory: 拓扑序敏感模块清单 ─────────────────────────
    println!(
        "[inventory] sensitive modules (topological): {:?}",
        ready.sensitive_modules()
    );

    // ── Verified shutdown: 逐模块报告清理结果 ──────────────────────────
    // `ex::connection-pool` 注册了 on_shutdown → verified；
    // `ex::session-store` 被标记但无生命周期回调 → skipped（可核查的缺口）。
    let report = ready.shutdown_with_report();
    println!("[report] verified: {:?}", report.verified);
    println!("[report] skipped: {:?}", report.skipped);
}
