// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 掩码引擎错误类型（原 inklog `InklogError` 的掩码子集）。

use std::fmt;

/// 掩码引擎错误：规则构建/注册失败与 secret-scan fail-closed 拒绝。
#[derive(Debug, Clone)]
pub enum MaskError {
    /// 规则定义非法：缺少 `pattern`/`name`、正则编译失败、同名规则重复注册
    /// 或 TOML 规则表解析失败。
    ConfigError(String),

    /// secret-scan fail-closed：单条输入内某脱敏模式的命中数超过门限，
    /// 脱敏输出被拒绝（防疑似密钥风暴落盘）。
    SecretScanLimit {
        /// 命中门限的模式名。
        pattern: String,
        /// 实际命中次数。
        count: u64,
        /// 配置的命中门限。
        limit: usize,
    },

    /// secret-scan fail-closed：单条输入超过掩码上限
    /// （`MAX_MASK_INPUT_BYTES`）——不扫描
    /// 即不可证安全，`mask_checked` 拒绝输出。
    SecretScanOversizedInput { size: usize, limit: usize },
}

impl fmt::Display for MaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConfigError(msg) => write!(f, "Masking config error: {msg}"),
            Self::SecretScanLimit {
                pattern,
                count,
                limit,
            } => write!(
                f,
                "Secret scan limit exceeded: pattern '{pattern}' hit {count} times in one input \
                 (limit {limit}); output withheld"
            ),
            Self::SecretScanOversizedInput { size, limit } => write!(
                f,
                "Secret scan oversized input: {size} bytes exceeds the {limit}-byte masking \
                 limit; output withheld (fail-closed)"
            ),
        }
    }
}

impl std::error::Error for MaskError {}
