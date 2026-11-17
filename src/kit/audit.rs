// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Security posture audit (`Kit<Ready>::security_posture`).
//!
//! Verifiable report of architecture-level safety capabilities under the
//! current feature set and registration state. Every check is computed
//! locally (`cfg!` constants plus the query APIs from previous changes) —
//! zero IO, zero network.

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

#[cfg(test)]
mod tests {
    use super::*;

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
