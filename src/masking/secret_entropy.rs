// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 值熵扫描器（feature `secret-scan`）：对日志内容做 Shannon 熵探测。
//!
//! 与 `encryption::validate_key_entropy` 刻意分域：
//! - 那边校验**加密密钥原始字节**（bits/byte，阈值 4.0，拒绝全零等弱密钥），
//!   调用点在加密链路构造期；
//! - 本模块扫描**日志文本 token**（bits/char，阈值可配，默认 4.5），在内容
//!   探测链路找出「无已知前缀但形似随机串」的疑似 secret。
//!
//! 单位、对象、阈值、调用点均不同，两者互不复用、互不感知。
//!
//! 误报控制：token 字符集只取 base64 核心字母 `[A-Za-z0-9+/]`——hex 全集熵
//! 上限 4.0（哈希/UUID/时间戳族在默认阈值下不误伤），`-`/`_` 连字符排除后
//! 脱敏标记片段（`REDACTED` 等）长度不足最短 token，不会被再命中。

use std::collections::HashMap;

use regex::Regex;

/// 默认熵阈值（bits/char）：hex 全集上限 4.0 不误伤，4.5 起覆盖 base64 形态。
pub const DEFAULT_ENTROPY_THRESHOLD: f64 = 4.5;

/// 默认参与熵判定的 token 最短字符数（base64 密钥常见 24 字节起）。
pub const DEFAULT_MIN_TOKEN_CHARS: usize = 20;

/// 熵命中在计数器与命中归因中的模式名。
pub const HIGH_ENTROPY_PATTERN: &str = "high_entropy";

/// 熵命中替换标记。
pub const HIGH_ENTROPY_REPLACEMENT: &str = "***HIGH_ENTROPY***";

/// token 提取正则模板（最短长度按配置在构造期展开）。
const TOKEN_PATTERN_TEMPLATE: &str = r"[A-Za-z0-9+/]{@@MIN@@,}";

/// 值熵扫描器：定位文本中熵不低于阈值的 token 字节区间。
///
/// `Clone` 语义为配置复制（无共享状态）；扫描是纯函数。
#[derive(Debug, Clone)]
pub struct EntropyScanner {
    threshold_bits: f64,
    min_token_chars: usize,
    token_regex: Regex,
}

impl Default for EntropyScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl EntropyScanner {
    /// 默认配置的扫描器（阈值 [`DEFAULT_ENTROPY_THRESHOLD`]、
    /// 最短 [`DEFAULT_MIN_TOKEN_CHARS`]）。
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(DEFAULT_ENTROPY_THRESHOLD, DEFAULT_MIN_TOKEN_CHARS)
    }

    /// 自定义阈值与最短 token 长度。
    ///
    /// # Panics
    /// `threshold_bits` 非有限（NaN/±inf）时 panic：NaN 参与任何 `>=`
    /// 比较恒为 false，会无痕关闭整个熵检测层——配置错误在构造期显性
    /// 失败（与容量调光 0 门限同取舍）。
    ///
    /// `min_token_chars` 小于 1 时按 1 处理；正则按该长度静态展开，
    /// 构造失败属实现缺陷，panic 暴露（与内置掩码正则同取舍）。
    #[must_use]
    pub fn with_config(threshold_bits: f64, min_token_chars: usize) -> Self {
        assert!(
            threshold_bits.is_finite(),
            "{}",
            "EntropyScanner: threshold_bits must be finite; NaN/infinity would silently disable entropy detection"
        );
        let min_token_chars = min_token_chars.max(1);
        let pattern = TOKEN_PATTERN_TEMPLATE.replace("@@MIN@@", &min_token_chars.to_string());
        let token_regex =
            Regex::new(&pattern).unwrap_or_else(|_| panic!("{}", "token regex must compile"));
        Self {
            threshold_bits,
            min_token_chars,
            token_regex,
        }
    }

    /// 熵判定阈值（bits/char，含）。
    #[must_use]
    pub fn threshold_bits(&self) -> f64 {
        self.threshold_bits
    }

    /// 参与熵判定的最短 token 字符数。
    #[must_use]
    pub fn min_token_chars(&self) -> usize {
        self.min_token_chars
    }

    /// Shannon 熵（bits/char）：按字符出现频率对字符集求和。
    ///
    /// 空串熵为 0。ASCII 输入走固定计数数组（零堆分配——熵扫描的 token
    /// 字符集 `[A-Za-z0-9+/]` 有界，热路径全部落在快路径）；非 ASCII 回退
    /// 通用字符频率表。
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // 熵计算对 2^52 以上的字符计数精度不敏感
    pub fn shannon_entropy(input: &str) -> f64 {
        if input.is_empty() {
            return 0.0;
        }
        if input.is_ascii() {
            let mut freq = [0u32; 256];
            for &byte in input.as_bytes() {
                freq[byte as usize] += 1;
            }
            return Self::entropy_from_counts(&freq, input.len());
        }
        let mut freq: HashMap<char, usize> = HashMap::new();
        let mut total = 0usize;
        for ch in input.chars() {
            *freq.entry(ch).or_insert(0) += 1;
            total += 1;
        }
        let total = total as f64;
        freq.values()
            .map(|&count| {
                let p = count as f64 / total;
                -p * p.log2()
            })
            .sum()
    }

    /// 等长 ASCII 输入的熵：字节计数数组 → bits/char。
    #[allow(clippy::cast_precision_loss)]
    fn entropy_from_counts(freq: &[u32; 256], total: usize) -> f64 {
        let total = total as f64;
        freq.iter()
            .filter(|&&count| count > 0)
            .map(|&count| {
                let p = f64::from(count) / total;
                -p * p.log2()
            })
            .sum()
    }

    /// 返回熵 >= 阈值的 token 字节区间（按出现顺序，互不重叠）。
    #[must_use]
    pub fn scan(&self, text: &str) -> Vec<(usize, usize)> {
        self.token_regex
            .find_iter(text)
            .filter(|m| Self::shannon_entropy(m.as_str()) >= self.threshold_bits)
            .map(|m| (m.start(), m.end()))
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // 断言对象是精确可表示值（0/3/4 bits）
mod tests {
    use super::*;

    /// 30 个互异字符：熵 = log2(30) ≈ 4.91 bits/char。
    const HIGH_ENTROPY_TOKEN: &str = "aB3xK9mP2qR7sT5uV1wX8yZ4nC6jM0";
    /// hex 全集等频：熵恰为 4.0 bits/char。
    const HEX_TOKEN: &str = "abcdef0123456789abcdef0123456789";

    #[test]
    fn test_shannon_entropy_empty_and_single_char() {
        assert_eq!(EntropyScanner::shannon_entropy(""), 0.0);
        assert_eq!(EntropyScanner::shannon_entropy("aaaaaaaaaa"), 0.0);
    }

    #[test]
    fn test_shannon_entropy_hex_exact_four_bits() {
        let h = EntropyScanner::shannon_entropy(HEX_TOKEN);
        assert!(
            (h - 4.0).abs() < 1e-9,
            "uniform hex must be exactly 4.0 bits/char, got {h}"
        );
    }

    #[test]
    fn test_shannon_entropy_high_token_above_default_threshold() {
        let h = EntropyScanner::shannon_entropy(HIGH_ENTROPY_TOKEN);
        assert!(
            h > DEFAULT_ENTROPY_THRESHOLD,
            "30 distinct chars must exceed 4.5, got {h}"
        );
    }

    #[test]
    fn test_scan_flags_high_entropy_token_with_spans() {
        let scanner = EntropyScanner::new();
        let text = format!("token {HIGH_ENTROPY_TOKEN} end");
        let spans = scanner.scan(&text);
        assert_eq!(spans.len(), 1);
        assert_eq!(&text[spans[0].0..spans[0].1], HIGH_ENTROPY_TOKEN);
    }

    #[test]
    fn test_scan_skips_low_entropy_and_hex_at_default_threshold() {
        let scanner = EntropyScanner::new();
        for text in [
            format!("blob {}", "a".repeat(40)),
            format!("blob {}", "passwordpasswordpasswordpassword"),
            format!("hex {HEX_TOKEN}"),
            // UUID：连字符拆散 + hex 熵低，默认阈值不误伤
            "uuid 550e8400e29b41d4a716446655440000".to_string(),
        ] {
            assert!(
                scanner.scan(&text).is_empty(),
                "low-entropy input must not be flagged: {text}"
            );
        }
    }

    #[test]
    fn test_scan_skips_short_tokens() {
        let scanner = EntropyScanner::new();
        // 18 字符高熵串：低于默认最短 20
        let text = "blob aB3xK9mP2qR7sT5uVw end";
        assert!(scanner.scan(text).is_empty(), "short token must be skipped");
    }

    #[test]
    fn test_threshold_boundary_is_inclusive() {
        // 阈值恰为 4.0：hex（恰 4.0）被命中——边界按 >= 语义
        let scanner = EntropyScanner::with_config(4.0, 20);
        let text = format!("hex {HEX_TOKEN}");
        assert_eq!(scanner.scan(&text).len(), 1);
        // 阈值 4.5：同一 hex 不命中
        let scanner = EntropyScanner::new();
        assert!(scanner.scan(&format!("hex {HEX_TOKEN}")).is_empty());
    }

    #[test]
    fn test_min_token_chars_configurable() {
        // 12 字符互异串（熵 log2(12) ≈ 3.58）：阈值 3.0 下只由长度门槛取舍
        let strict = EntropyScanner::with_config(3.0, 20);
        assert!(
            strict.scan("blob aB3xK9mP2qR end").is_empty(),
            "12-char token must be skipped under min 20"
        );
        let lax = EntropyScanner::with_config(3.0, 8);
        assert_eq!(
            lax.scan("blob aB3xK9mP2qR end").len(),
            1,
            "min 8 lets the 12-char token in"
        );
        assert_eq!(lax.min_token_chars(), 8);
        assert_eq!(lax.threshold_bits(), 3.0);
    }

    #[test]
    fn test_marker_fragments_never_flagged() {
        // 脱敏标记片段不含 +/_ 连字符时长度不足最短 token；
        // 即使阈值降到 3.5 也不得把门自身的替换标记再命中
        let scanner = EntropyScanner::with_config(3.5, 20);
        for text in [
            "***REDACTED_API_KEY***",
            "x ***REDACTED_PRIVATE_KEY*** y",
            HIGH_ENTROPY_REPLACEMENT,
        ] {
            assert!(
                scanner.scan(text).is_empty(),
                "marker fragment must not be flagged: {text}"
            );
        }
    }

    #[test]
    fn test_scan_empty_text() {
        assert!(EntropyScanner::new().scan("").is_empty());
    }

    #[test]
    fn test_zero_min_token_chars_clamped_to_one() {
        let scanner = EntropyScanner::with_config(0.5, 0);
        assert_eq!(scanner.min_token_chars(), 1);
    }

    #[test]
    #[should_panic(expected = "threshold_bits must be finite")]
    fn test_nan_threshold_rejected_loudly() {
        // NaN 参与任何 >= 比较恒为 false，会无痕关闭整个熵检测层——
        // 配置错误在构造期显性失败（与 CapacityDial 的 0 门限同取舍）。
        let _ = EntropyScanner::with_config(f64::NAN, 20);
    }

    #[test]
    #[should_panic(expected = "threshold_bits must be finite")]
    fn test_infinite_threshold_rejected_loudly() {
        let _ = EntropyScanner::with_config(f64::INFINITY, 20);
    }
}
