// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Integration tests for the security posture audit (feature `audit`).

#![cfg(all(feature = "report", feature = "audit"))]

use std::any::TypeId;
use std::sync::Arc;

use trait_kit::core::{AutoBuilder, ModuleMeta};
use trait_kit::kit::Kit;
// R-aud-002：prelude 直接引用（audit 门控 re-export）。
use trait_kit::prelude::SecurityPosture;

const EIGHT_IDS: [&str; 8] = [
    "unsafe-code",
    "typestate",
    "dependency-policy",
    "capability-visibility",
    "config-encryption",
    "masking",
    "sensitive-resources",
    "lifecycle-shutdown",
];

/// 8 个检查 id 全部存在、detail 携带证据、无 Fail；`to_json()` round-trip
/// 后 checks 数量不变（R-aud-001 / R-aud-002）。
#[test]
fn posture_reports_all_eight_ids_and_round_trips() {
    let ready = Kit::new().build().expect("build empty kit");
    let posture: SecurityPosture = ready.security_posture();

    let ids: Vec<&str> = posture.checks.iter().map(|check| check.id).collect();
    assert_eq!(ids, EIGHT_IDS, "stable check ids, in order");
    for check in &posture.checks {
        assert!(
            !check.detail.is_empty(),
            "check '{}' lacks evidence",
            check.id
        );
        assert!(
            matches!(
                check.status,
                trait_kit::kit::PostureStatus::Pass
                    | trait_kit::kit::PostureStatus::Warn
                    | trait_kit::kit::PostureStatus::Info
                    | trait_kit::kit::PostureStatus::Fail
            ),
            "check '{}' has a legal status",
            check.id
        );
    }
    assert!(posture.is_ok(), "empty kit must not fail any check");

    let json = posture.to_json().expect("serialize posture");
    let value: serde_json::Value = serde_json::from_str(&json).expect("parse posture json");
    assert_eq!(
        value["checks"].as_array().map(Vec::len),
        Some(8),
        "round-trip keeps the check count"
    );
}

/// 配置 policy + Restricted 可见性的 Kit：两项治理检查转 Pass
/// （R-aud-001 组合 2，需 `--features audit,policy,visibility`）。
#[cfg(all(feature = "policy", feature = "visibility"))]
#[test]
fn posture_passes_for_policy_and_visibility() {
    use trait_kit::kit::PostureStatus;
    use trait_kit::kit::policy::{DependencyPolicy, ForbiddenEdge, ModulePattern};
    use trait_kit::kit::visibility::CapabilityVisibility;

    struct Governed;
    impl ModuleMeta for Governed {
        const NAME: &'static str = "posture::governed";
    }
    impl AutoBuilder for Governed {
        type Capability = Arc<u32>;
        type Error = std::io::Error;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            Ok(Arc::new(1))
        }
    }

    struct Unrelated;
    impl ModuleMeta for Unrelated {
        const NAME: &'static str = "posture::other";
    }
    impl AutoBuilder for Unrelated {
        type Capability = Arc<u64>;
        type Error = std::io::Error;
        fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
            let _ = TypeId::of::<Governed>();
            Ok(Arc::new(2_u64))
        }
    }

    let mut kit = Kit::new();
    kit.register::<Governed>().expect("register governed");
    kit.register::<Unrelated>().expect("register unrelated");
    kit.dependency_policy(DependencyPolicy {
        forbid: vec![ForbiddenEdge {
            dependent: ModulePattern::Any,
            dependency: ModulePattern::Exact("posture::governed"),
        }],
        required: vec![],
    });
    kit.set_visibility::<Governed>(CapabilityVisibility::Restricted(&["posture::caller"]));
    let ready = kit.build().expect("build");

    let posture = ready.security_posture();
    let status_of = |id: &str| {
        posture
            .checks
            .iter()
            .find(|check| check.id == id)
            .map(|check| check.status)
            .unwrap_or_else(|| panic!("missing check: {id}"))
    };
    assert_eq!(status_of("dependency-policy"), PostureStatus::Pass);
    assert_eq!(status_of("capability-visibility"), PostureStatus::Pass);

    // JSON round-trip after configuration keeps all 8 checks.
    let json = posture.to_json().expect("serialize posture");
    let value: serde_json::Value = serde_json::from_str(&json).expect("parse posture json");
    assert_eq!(
        value["checks"].as_array().map(Vec::len),
        Some(8),
        "round-trip keeps the check count"
    );
}
