// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Integration tests for capability visibility (feature `visibility`):
//! Data/Audit/Plugin 三方场景 + `compose` 子 Kit 命名空间互不影响。
#![cfg(feature = "visibility")]

use std::sync::Arc;

// 仅经 prelude 引用 CapabilityVisibility：prelude 未导出即编译失败，
// 钉住 T008 的 re-export 验收。
use trait_kit::prelude::CapabilityVisibility;
use trait_kit::prelude::*;

#[derive(Clone, Debug)]
struct DbPool {
    max_connections: u32,
}

struct Database;
impl ModuleMeta for Database {
    const NAME: &'static str = "data::Database";
}
impl AutoBuilder for Database {
    type Capability = Arc<DbPool>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(DbPool {
            max_connections: 16,
        }))
    }
}

struct AuditTrail;
impl ModuleMeta for AuditTrail {
    const NAME: &'static str = "audit::AuditModule";
}
impl AutoBuilder for AuditTrail {
    type Capability = Arc<DbPool>;
    type Error = TraitKitError;
    fn build(kit: &Kit) -> Result<Self::Capability, Self::Error> {
        // 构建回调内是容器级持有者（require_for / require_as 是
        // `Kit<Ready>` 检索路径 API），无 caller 的 require 不受
        // Restricted 约束。
        let _pool = kit.require::<Database>()?;
        Ok(Arc::new(DbPool { max_connections: 1 }))
    }
}

struct Plugin;
impl ModuleMeta for Plugin {
    const NAME: &'static str = "plugin::PluginModule";
}

fn ready_kit() -> Kit<Ready> {
    let mut kit = Kit::new();
    kit.register_with_visibility::<Database>(CapabilityVisibility::Restricted(&["audit::*"]))
        .expect("register Database with visibility");
    kit.register::<AuditTrail>().expect("register AuditTrail");
    kit.build().expect("build succeeds")
}

#[test]
fn restricted_capability_denies_untrusted_plugin_caller() {
    let ready = ready_kit();
    let err = ready
        .require_as::<Database, Plugin>()
        .expect_err("plugin caller is outside the audit::* allow-list");
    assert!(
        matches!(
            err,
            TraitKitError::AccessDenied {
                module: "data::Database",
                caller: "plugin::PluginModule",
            }
        ),
        "wrong error: {err:?}"
    );
    let msg = format!("{err}");
    assert!(msg.contains("data::Database"), "module named: '{msg}'");
    assert!(
        msg.contains("plugin::PluginModule"),
        "caller named: '{msg}'"
    );
}

#[test]
fn restricted_capability_allows_declared_audit_caller() {
    let ready = ready_kit();
    let pool = ready
        .require_as::<Database, AuditTrail>()
        .expect("audit caller matches the audit::* pattern");
    assert_eq!(pool.max_connections, 16);
}

#[test]
fn require_without_caller_keeps_container_holder_semantics() {
    let ready = ready_kit();
    let _pool = ready
        .require::<Database>()
        .expect("caller-less require acts as the container-level holder");
}

#[test]
fn module_builder_retrieval_keeps_working_alongside_visibility() {
    // AuditTrail::build 内部以容器级 require 检索 Database：可见性
    // 存在时不影响既有构建路径。
    let ready = ready_kit();
    let _trail = ready.require::<AuditTrail>().expect("audit builds");
}

/// `compose` 子 Kit 命名空间是 Kit 级边界，与能力级可见性互补互不影响：
/// 父 Kit 的 Restricted 模块不改变子 Kit 的组合行为。
#[cfg(feature = "compose")]
mod compose_unaffected {
    use super::*;
    use trait_kit::kit::{SubKitModule, SubKitSpec};

    struct ChildCapability;
    struct ChildModule;
    impl ModuleMeta for ChildModule {
        const NAME: &'static str = "child::Leaf";
    }
    impl AutoBuilder for ChildModule {
        type Capability = Arc<ChildCapability>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(ChildCapability))
        }
    }

    struct ChildSpec;
    impl SubKitSpec for ChildSpec {
        const NAME: &'static str = "child-kit";
        fn compose(kit: &mut Kit) {
            kit.register::<ChildModule>().expect("child register");
        }
    }

    #[test]
    fn sub_kit_composition_coexists_with_restricted_parent_module() {
        let mut kit = Kit::new();
        kit.register_with_visibility::<Database>(CapabilityVisibility::Restricted(&["audit::*"]))
            .expect("register restricted Database");
        kit.register::<SubKitModule<ChildSpec>>()
            .expect("register sub kit");
        let ready = kit.build().expect("build succeeds");

        // 父 Kit 的可见性约束照常生效（拒绝插件调用方）。
        assert!(ready.require_as::<Database, Plugin>().is_err());
        // 子 Kit 命名空间照常供容器级持有者检索。
        let _child = ready
            .require::<SubKitModule<ChildSpec>>()
            .expect("sub kit capability is unaffected by parent visibility");
    }
}
