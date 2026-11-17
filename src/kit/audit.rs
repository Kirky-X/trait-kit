// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Security posture audit (`Kit<Ready>::security_posture`).
//!
//! Verifiable report of architecture-level safety capabilities under the
//! current feature set and registration state. Every check is computed
//! locally (`cfg!` constants plus the query APIs from previous changes) —
//! zero IO, zero network.

use crate::kit::Ready;
use crate::kit::kit::Kit;

/// Verdict of one posture check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum PostureStatus {
    /// The invariant holds (positive evidence).
    Pass,
    /// A soft risk the host should decide on (e.g. "all capabilities
    /// Public") — allowed by `is_ok`.
    Warn,
    /// Neutral state (e.g. capability not compiled in) — allowed by
    /// `is_ok`.
    Info,
    /// Reserved for programmatic checks whose declared invariant failed to
    /// verify (no such check ships today; the variant keeps the enum
    /// complete so hosts can branch exhaustively).
    Fail,
}

/// One row of the posture report.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PostureCheck {
    /// Stable check id (e.g. `"unsafe-code"`, `"dependency-policy"`); ids
    /// do not change, thresholds behind them may be tuned.
    pub id: &'static str,
    /// Verdict for this check.
    pub status: PostureStatus,
    /// Human-readable evidence (English; diagnostics, not API contract).
    pub detail: String,
}

/// Serializable report of architecture-level safety capabilities.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SecurityPosture {
    /// One row per check, stable ids.
    pub checks: Vec<PostureCheck>,
}

impl SecurityPosture {
    /// `true` when no check is [`PostureStatus::Fail`] (Warn/Info still
    /// allowed — the host decides whether a Warn posture may go live).
    #[must_use]
    pub fn is_ok(&self) -> bool {
        !self
            .checks
            .iter()
            .any(|check| check.status == PostureStatus::Fail)
    }

    /// Serialize to JSON (`serde_json` ships with the `report` feature,
    /// inherited by `audit`).
    ///
    /// # Errors
    ///
    /// Returns `serde_json::Error` when serialization fails (non-finite
    /// floats or depth limits — neither occurs with this schema).
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }
}

impl Kit<Ready> {
    /// Verifiable posture of architecture-level safety capabilities under
    /// the current feature set and registration state.
    ///
    /// Eight checks with stable ids (see [`SecurityPosture`]), each
    /// computed locally from `cfg!` constants and the query APIs of the
    /// governance features — zero IO, zero network. A host may gate
    /// go-live on `is_ok()` or on individual check statuses.
    ///
    /// Requires the `audit` feature (implies `report`).
    #[must_use]
    pub fn security_posture(&self) -> SecurityPosture {
        let mut checks = vec![
            PostureCheck {
                id: "unsafe-code",
                status: PostureStatus::Pass,
                detail: "#![deny(unsafe_code)] enforced crate-wide at compile time; any \
                         exemption requires an explicit allow with a SAFETY justification"
                    .to_string(),
            },
            PostureCheck {
                id: "typestate",
                status: PostureStatus::Pass,
                detail: "Unbuilt/Ready typestate split enforced at compile time: capability \
                         queries only exist on Kit<Ready>"
                    .to_string(),
            },
        ];
        checks.push(self.dependency_policy_check());
        checks.push(self.capability_visibility_check());
        checks.push(config_encryption_check());
        checks.push(masking_check());
        checks.push(self.sensitive_resources_check());
        checks.push(lifecycle_shutdown_check());
        SecurityPosture { checks }
    }

    /// `dependency-policy`: configured rule count via
    /// [`dependency_policy_summary`](Kit::dependency_policy_summary).
    #[cfg(feature = "policy")]
    fn dependency_policy_check(&self) -> PostureCheck {
        match self.dependency_policy_summary() {
            Some((forbid, required)) if forbid + required > 0 => PostureCheck {
                id: "dependency-policy",
                status: PostureStatus::Pass,
                detail: format!("policy configured: {forbid} forbidden, {required} required edges"),
            },
            Some(_) => PostureCheck {
                id: "dependency-policy",
                status: PostureStatus::Warn,
                detail: "policy feature enabled but no rules configured — violations \
                         undetectable"
                    .to_string(),
            },
            None => PostureCheck {
                id: "dependency-policy",
                status: PostureStatus::Warn,
                detail: "policy feature enabled but no policy set — violations undetectable"
                    .to_string(),
            },
        }
    }

    /// `dependency-policy` fallback when the `policy` feature is off.
    #[cfg(not(feature = "policy"))]
    fn dependency_policy_check(&self) -> PostureCheck {
        PostureCheck {
            id: "dependency-policy",
            status: PostureStatus::Info,
            detail: "policy feature not enabled — architecture violations undetectable".to_string(),
        }
    }

    /// `capability-visibility`: restricted distribution via
    /// [`visibility_summary`](Kit::visibility_summary).
    #[cfg(feature = "visibility")]
    fn capability_visibility_check(&self) -> PostureCheck {
        match self.visibility_summary() {
            Some((total, restricted)) if restricted > 0 => PostureCheck {
                id: "capability-visibility",
                status: PostureStatus::Pass,
                detail: format!("{restricted} of {total} declared capabilities restricted"),
            },
            Some((total, _)) => PostureCheck {
                id: "capability-visibility",
                status: PostureStatus::Warn,
                detail: format!(
                    "all {total} declared capabilities Public — plugins can retrieve everything"
                ),
            },
            None => PostureCheck {
                id: "capability-visibility",
                status: PostureStatus::Warn,
                detail: "no visibility declared — every capability Public".to_string(),
            },
        }
    }

    /// `capability-visibility` fallback when the `visibility` feature is
    /// off.
    #[cfg(not(feature = "visibility"))]
    fn capability_visibility_check(&self) -> PostureCheck {
        PostureCheck {
            id: "capability-visibility",
            status: PostureStatus::Warn,
            detail: "visibility feature not enabled — every capability Public".to_string(),
        }
    }

    /// `sensitive-resources`: marked module count via
    /// [`sensitive_modules`](Kit::sensitive_modules).
    #[cfg(feature = "resource-guard")]
    fn sensitive_resources_check(&self) -> PostureCheck {
        let marked = self.sensitive_modules().len();
        if marked > 0 {
            PostureCheck {
                id: "sensitive-resources",
                status: PostureStatus::Pass,
                detail: format!("{marked} sensitive modules marked and auditable"),
            }
        } else {
            PostureCheck {
                id: "sensitive-resources",
                status: PostureStatus::Info,
                detail: "resource-guard enabled; no sensitive modules marked".to_string(),
            }
        }
    }

    /// `sensitive-resources` fallback when `resource-guard` is off.
    #[cfg(not(feature = "resource-guard"))]
    fn sensitive_resources_check(&self) -> PostureCheck {
        PostureCheck {
            id: "sensitive-resources",
            status: PostureStatus::Info,
            detail: "resource-guard feature not enabled — sensitive cleanup unverifiable"
                .to_string(),
        }
    }
}

/// `config-encryption`: static feature-chain state. `encryption` implies
/// `reload` implies `confers`, so an enabled encryption also proves the
/// lower layers; there is no "encrypted but no hot reload" state.
fn config_encryption_check() -> PostureCheck {
    if cfg!(feature = "encryption") {
        PostureCheck {
            id: "config-encryption",
            status: PostureStatus::Pass,
            detail: "config encryption enabled (feature chain: encryption → reload → confers)"
                .to_string(),
        }
    } else if cfg!(feature = "reload") {
        PostureCheck {
            id: "config-encryption",
            status: PostureStatus::Info,
            detail: "hot reload enabled without encryption (chain: reload → confers)".to_string(),
        }
    } else if cfg!(feature = "confers") {
        PostureCheck {
            id: "config-encryption",
            status: PostureStatus::Info,
            detail: "config integration enabled without encryption/reload".to_string(),
        }
    } else {
        PostureCheck {
            id: "config-encryption",
            status: PostureStatus::Info,
            detail: "no config persistence feature (confers/reload/encryption) enabled".to_string(),
        }
    }
}

/// `masking`: PII/credential redaction availability.
fn masking_check() -> PostureCheck {
    if cfg!(feature = "masking") {
        PostureCheck {
            id: "masking",
            status: PostureStatus::Pass,
            detail: "PII/credential masking engine enabled".to_string(),
        }
    } else {
        PostureCheck {
            id: "masking",
            status: PostureStatus::Info,
            detail: "masking feature not enabled — secrets pass through unredacted".to_string(),
        }
    }
}

/// `lifecycle-shutdown`: verifiable cleanup capability. Both `lifecycle`
/// hooks and the `shutdown` coordinator together give a full shutdown
/// chain; any single one is a partial (Info) state.
fn lifecycle_shutdown_check() -> PostureCheck {
    match (cfg!(feature = "lifecycle"), cfg!(feature = "shutdown")) {
        (true, true) => PostureCheck {
            id: "lifecycle-shutdown",
            status: PostureStatus::Pass,
            detail: "lifecycle hooks and graceful shutdown coordinator both enabled".to_string(),
        },
        (true, false) => PostureCheck {
            id: "lifecycle-shutdown",
            status: PostureStatus::Info,
            detail: "lifecycle hooks enabled; graceful shutdown coordinator not enabled"
                .to_string(),
        },
        (false, true) => PostureCheck {
            id: "lifecycle-shutdown",
            status: PostureStatus::Info,
            detail: "shutdown coordinator enabled; lifecycle hooks not enabled".to_string(),
        },
        (false, false) => PostureCheck {
            id: "lifecycle-shutdown",
            status: PostureStatus::Info,
            detail: "neither lifecycle hooks nor graceful shutdown enabled".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::kit::Kit;

    // ---- security_posture()（T007, R-aud-001）----

    /// 8 个稳定 id 全部存在、顺序稳定，detail 携带证据，无 Fail（R-aud-001）。
    #[test]
    fn posture_covers_all_eight_stable_ids() {
        let ready = Kit::new().build().expect("build empty kit");
        let posture = ready.security_posture();
        let ids: Vec<&str> = posture.checks.iter().map(|check| check.id).collect();
        assert_eq!(
            ids,
            [
                "unsafe-code",
                "typestate",
                "dependency-policy",
                "capability-visibility",
                "config-encryption",
                "masking",
                "sensitive-resources",
                "lifecycle-shutdown",
            ]
        );
        for check in &posture.checks {
            assert!(
                !check.detail.is_empty(),
                "check '{}' must carry evidence",
                check.id
            );
        }
        assert!(posture.is_ok(), "empty kit must not fail any check");
    }

    /// 状态规则与编译组合一致：`cargo test --features audit` 与
    /// `cargo test --features audit,policy,visibility,resource-guard`
    /// 各跑一遍，`cfg!` 分支覆盖两种组合（R-aud-001）。
    #[test]
    fn posture_status_rules_follow_feature_set() {
        let ready = Kit::new().build().expect("build empty kit");
        let posture = ready.security_posture();
        let status_of = |id: &str| {
            posture
                .checks
                .iter()
                .find(|check| check.id == id)
                .map(|check| check.status)
                .expect("stable check id")
        };

        // 编译期常量检查恒为 Pass。
        assert_eq!(status_of("unsafe-code"), PostureStatus::Pass);
        assert_eq!(status_of("typestate"), PostureStatus::Pass);

        // dependency-policy：feature 未启用 → Info；启用未配置 → Warn。
        assert_eq!(
            status_of("dependency-policy"),
            if cfg!(feature = "policy") {
                PostureStatus::Warn
            } else {
                PostureStatus::Info
            }
        );
        // capability-visibility：无声明（含 feature 未启用）→ 全 Public → Warn。
        assert_eq!(status_of("capability-visibility"), PostureStatus::Warn);
        // config-encryption：encryption 启用 → Pass（蕴含 reload），否则 Info。
        assert_eq!(
            status_of("config-encryption"),
            if cfg!(feature = "encryption") {
                PostureStatus::Pass
            } else {
                PostureStatus::Info
            }
        );
        // masking：启用 → Pass；未启用 → Info。
        assert_eq!(
            status_of("masking"),
            if cfg!(feature = "masking") {
                PostureStatus::Pass
            } else {
                PostureStatus::Info
            }
        );
        // sensitive-resources：空 Kit 无标记（含 feature 未启用）→ Info。
        assert_eq!(status_of("sensitive-resources"), PostureStatus::Info);
        // lifecycle-shutdown：lifecycle + shutdown 全启用 → Pass；否则 Info。
        assert_eq!(
            status_of("lifecycle-shutdown"),
            if cfg!(feature = "lifecycle") && cfg!(feature = "shutdown") {
                PostureStatus::Pass
            } else {
                PostureStatus::Info
            }
        );
    }

    /// 组合 `--features audit,policy,visibility,resource-guard` 下：配置
    /// forbid 策略、Restricted 可见性与敏感标记 → 三项转 Pass（R-aud-001）。
    #[cfg(all(feature = "policy", feature = "visibility", feature = "resource-guard"))]
    #[test]
    fn posture_passes_with_configured_capabilities() {
        use crate::core::{AutoBuilder, ModuleMeta};
        use crate::kit::policy::{DependencyPolicy, ForbiddenEdge, ModulePattern};
        use crate::kit::visibility::CapabilityVisibility;
        use std::sync::Arc;

        struct AuditSvc;
        impl ModuleMeta for AuditSvc {
            const NAME: &'static str = "aud::svc";
        }
        impl AutoBuilder for AuditSvc {
            type Capability = Arc<u32>;
            type Error = std::io::Error;
            fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
                Ok(Arc::new(1))
            }
        }

        let mut kit = Kit::new();
        kit.register::<AuditSvc>().expect("register audit svc");
        kit.dependency_policy(DependencyPolicy {
            forbid: vec![ForbiddenEdge {
                dependent: ModulePattern::Any,
                dependency: ModulePattern::Exact("aud::svc"),
            }],
            required: vec![],
        });
        kit.set_visibility::<AuditSvc>(CapabilityVisibility::Restricted(&["aud::caller"]));
        kit.mark_sensitive::<AuditSvc>();
        let ready = kit.build().expect("build");

        let posture = ready.security_posture();
        let status_of = |id: &str| {
            posture
                .checks
                .iter()
                .find(|check| check.id == id)
                .map(|check| check.status)
                .expect("stable check id")
        };
        assert_eq!(status_of("dependency-policy"), PostureStatus::Pass);
        assert_eq!(status_of("capability-visibility"), PostureStatus::Pass);
        assert_eq!(status_of("sensitive-resources"), PostureStatus::Pass);
    }

    /// `is_ok()`：无 Fail 即 true —— Pass/Warn/Info 混合仍算 ok（R-aud-001）。
    #[test]
    fn is_ok_true_without_fail() {
        let posture = SecurityPosture {
            checks: vec![
                PostureCheck {
                    id: "unsafe-code",
                    status: PostureStatus::Pass,
                    detail: "deny(unsafe_code) at compile time".to_string(),
                },
                PostureCheck {
                    id: "masking",
                    status: PostureStatus::Info,
                    detail: "feature not enabled".to_string(),
                },
                PostureCheck {
                    id: "capability-visibility",
                    status: PostureStatus::Warn,
                    detail: "all capabilities Public".to_string(),
                },
            ],
        };
        assert!(posture.is_ok());
    }

    /// `is_ok()`：含 Fail 即 false（R-aud-001）。
    #[test]
    fn is_ok_false_with_fail() {
        let posture = SecurityPosture {
            checks: vec![PostureCheck {
                id: "programmatic-check",
                status: PostureStatus::Fail,
                detail: "declared invariant did not verify".to_string(),
            }],
        };
        assert!(!posture.is_ok());
    }

    /// `to_json()` round-trip：checks 数量与首项 id/status 不变（R-aud-001）。
    #[test]
    fn posture_to_json_round_trips() {
        let posture = SecurityPosture {
            checks: vec![PostureCheck {
                id: "unsafe-code",
                status: PostureStatus::Pass,
                detail: "deny(unsafe_code) at compile time".to_string(),
            }],
        };
        let json = posture.to_json().expect("serialize posture");
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse posture json");
        assert_eq!(value["checks"].as_array().map(Vec::len), Some(1));
        assert_eq!(value["checks"][0]["id"], "unsafe-code");
        assert_eq!(value["checks"][0]["status"], "Pass");
    }
}
