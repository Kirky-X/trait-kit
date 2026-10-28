// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 掩码引擎（feature `masking`）：PII/凭据检测与替换的单一来源。
//!
//! 自 inklog 下沉（变更 `masking-port-and-rate-limit-ownership`）：引擎本体
//! [`DataMasker`]、规则 [`MaskRule`]、注册中心 [`MaskRuleRegistry`] 与
//! secret-scan 出站门（[`SecretScanGate`] 族）同住本模块，inklog 与
//! limiteron 共用同一实现，规则语义与内置规则集顺序保持不变。
//!
//! 与 inklog 实现的两点差异：错误类型由 `InklogError` 收敛为本模块的
//! [`MaskError`]；诊断与 panic 文案为英文字面量（inklog 侧 i18n 目录不随
//! 引擎迁移）。

pub mod ac;
mod engine;
pub mod error;
mod registry;
pub mod secret_entropy;
pub mod secret_patterns;
pub mod secret_scan;

pub use ac::AcMasker;
pub use engine::{
    DataMasker, DataMaskerBuilder, MaskMatch, MaskRule, MaskRuleBuilder, mask_email, mask_phone,
    secret_scan_masker,
};
pub use error::MaskError;
pub use registry::MaskRuleRegistry;
pub use secret_entropy::EntropyScanner;
pub use secret_patterns::{SecretMatch, SecretPattern, SecretPatternRegistry};
pub use secret_scan::SecretScanGate;
