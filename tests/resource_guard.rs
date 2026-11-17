// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Integration tests for the `resource-guard` feature: sensitive-resource
//! marking, shutdown verification, build-failure rollback, and coordinator
//! timeout attribution.

#![cfg(feature = "resource-guard")]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use trait_kit::TraitKitError;
use trait_kit::core::{AutoBuilder, Lifecycle, ModuleMeta};
use trait_kit::kit::Kit;

/// Connection-pool-like sensitive module whose cleanup flips a shared flag.
static POOL_CLEANED: AtomicBool = AtomicBool::new(false);

struct ConnectionPoolModule;
impl ModuleMeta for ConnectionPoolModule {
    const NAME: &'static str = "connection-pool";
}
impl AutoBuilder for ConnectionPoolModule {
    type Capability = Arc<u32>;
    type Error = std::io::Error;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(1))
    }
}
impl Lifecycle for ConnectionPoolModule {
    fn on_shutdown(_cap: &Self::Capability) {
        POOL_CLEANED.store(true, Ordering::SeqCst);
    }
}

/// A plain (unmarked) module so topological chains have non-sensitive
/// members to skip over.
struct PlainCacheModule;
impl ModuleMeta for PlainCacheModule {
    const NAME: &'static str = "plain-cache";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        static DEPS: &[(&str, std::any::TypeId)] = &[(
            "connection-pool",
            std::any::TypeId::of::<ConnectionPoolModule>(),
        )];
        DEPS
    }
}
impl AutoBuilder for PlainCacheModule {
    type Capability = Arc<u32>;
    type Error = std::io::Error;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(2))
    }
}

/// A sensitive module that depends on the cache: topologically last.
struct SessionStoreModule;
impl ModuleMeta for SessionStoreModule {
    const NAME: &'static str = "session-store";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        static DEPS: &[(&str, std::any::TypeId)] =
            &[("plain-cache", std::any::TypeId::of::<PlainCacheModule>())];
        DEPS
    }
}
impl AutoBuilder for SessionStoreModule {
    type Capability = Arc<u32>;
    type Error = std::io::Error;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(3))
    }
}
impl Lifecycle for SessionStoreModule {}

/// A module whose build always fails (rollback scenario).
static FAILING_CLEANED: AtomicBool = AtomicBool::new(false);

struct OkThenFailingModule;
impl ModuleMeta for OkThenFailingModule {
    const NAME: &'static str = "ok-then-cleaned";
}
impl AutoBuilder for OkThenFailingModule {
    type Capability = Arc<u32>;
    type Error = std::io::Error;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(1))
    }
}
impl Lifecycle for OkThenFailingModule {
    fn on_shutdown(_cap: &Self::Capability) {
        FAILING_CLEANED.store(true, Ordering::SeqCst);
    }
}

struct AlwaysFailingModule;
impl ModuleMeta for AlwaysFailingModule {
    const NAME: &'static str = "always-failing";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        static DEPS: &[(&str, std::any::TypeId)] = &[(
            "ok-then-cleaned",
            std::any::TypeId::of::<OkThenFailingModule>(),
        )];
        DEPS
    }
}
impl AutoBuilder for AlwaysFailingModule {
    type Capability = Arc<u32>;
    type Error = std::io::Error;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Err(std::io::Error::other(
            "integration: simulated build failure",
        ))
    }
}

/// 场景 1：连接池模块 `mark_sensitive` 后 `shutdown_with_report()` 的
/// verified 含其名（R-res-003）。
#[test]
fn marked_pool_module_is_verified_after_shutdown() {
    POOL_CLEANED.store(false, Ordering::SeqCst);
    let mut kit = Kit::new();
    kit.register::<ConnectionPoolModule>().expect("register");
    kit.register_lifecycle::<ConnectionPoolModule>();
    kit.mark_sensitive::<ConnectionPoolModule>();
    let ready = kit.build().expect("build");
    let report = ready.shutdown_with_report();
    assert_eq!(report.verified, vec!["connection-pool"]);
    assert!(report.skipped.is_empty());
    assert!(POOL_CLEANED.load(Ordering::SeqCst));
}

/// 场景 2：build 失败回滚触发已构建模块的侧效清理（R-res-004）。
#[test]
fn build_failure_rollback_runs_side_effect_cleanup() {
    FAILING_CLEANED.store(false, Ordering::SeqCst);
    let mut kit = Kit::new();
    kit.register::<OkThenFailingModule>().expect("register");
    kit.register_lifecycle::<OkThenFailingModule>();
    kit.register::<AlwaysFailingModule>().expect("register");
    let err = kit.build().expect_err("build must fail");
    assert!(
        matches!(err, TraitKitError::BuildFailed { .. }),
        "expected BuildFailed, got {err:?}"
    );
    assert!(
        FAILING_CLEANED.load(Ordering::SeqCst),
        "rollback must run on_shutdown of the already-built module"
    );
}

/// 场景 3：`sensitive_modules()` 输出拓扑序（跳过未标记模块）（R-res-002）。
#[test]
fn sensitive_modules_follow_topological_order() {
    let mut kit = Kit::new();
    kit.register::<ConnectionPoolModule>().expect("register");
    kit.register::<PlainCacheModule>().expect("register");
    kit.register::<SessionStoreModule>().expect("register");
    kit.mark_sensitive::<ConnectionPoolModule>();
    kit.mark_sensitive::<SessionStoreModule>();
    let ready = kit.build().expect("build");
    assert_eq!(
        ready.sensitive_modules(),
        vec!["connection-pool", "session-store"]
    );
}

/// 场景 4（cfg shutdown）：timed_out 的 `CloseConnections` 结果经
/// `sensitive_at_risk` 点名全部敏感模块（R-res-006）。
#[cfg(feature = "shutdown")]
#[test]
fn coordinator_timeout_names_sensitive_modules_at_risk() {
    use std::time::Duration;

    use trait_kit::kit::shutdown::{ShutdownPhase, ShutdownPhaseResult, ShutdownResult};

    let mut kit = Kit::new();
    kit.register::<ConnectionPoolModule>().expect("register");
    kit.mark_sensitive::<ConnectionPoolModule>();
    let ready = kit.build().expect("build");

    let timed_out = ShutdownResult {
        phases: vec![ShutdownPhaseResult {
            phase: ShutdownPhase::CloseConnections,
            timed_out: true,
            elapsed: Duration::from_secs(1),
            hook_failures: 0,
        }],
    };
    assert_eq!(ready.sensitive_at_risk(&timed_out), vec!["connection-pool"]);
}
