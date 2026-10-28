// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 出站脱敏门（feature `secret-scan`）。
//!
//! [`SecretScanGate`] 组合 [`SecretPatternRegistry`](super::secret_patterns::SecretPatternRegistry)
//! 与值熵扫描（[`EntropyScanner`](super::secret_entropy::EntropyScanner)），
//! 提供逐模式命中计数与超限 fail-closed 出口：
//!
//! - `mask`：不返回错误——替换全部命中并累加计数；
//! - `mask_checked`：同语义，但单条输入内任一模式命中数超过门限
//!   （`SecretScanLimit`）或输入超过掩码上限（`SecretScanOversizedInput`，
//!   不扫描即不可证安全）时返回 `Err`，脱敏输出不可用（防疑似密钥风暴
//!   落盘）。两个出口共用计数器，Err 时计数已更新可查。
//!
//! 层位：经 `DataMasker::builder().with_secret_scan()` 挂到 sink 写出前的
//! 掩码点（与 encryption / `ring_buffered_file` 同层）。两个出口都是
//! fail-closed 的：命中数超门限时 `mask_checked` 拒绝输出；超大输入在
//! `mask` 以整体遮蔽收口（不扫描即不可证安全），在 `mask_checked` 拒绝。

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::engine::MAX_MASK_INPUT_BYTES;
use super::error::MaskError;
use super::secret_entropy::{EntropyScanner, HIGH_ENTROPY_PATTERN, HIGH_ENTROPY_REPLACEMENT};
use super::secret_patterns::{SecretMatch, SecretPattern, SecretPatternRegistry};

/// 默认单条输入内单模式命中上限。
pub const DEFAULT_MATCH_LIMIT: usize = 64;

#[derive(Debug)]
struct GateCore {
    /// 冻结的模式表（注册顺序 = 替换顺序）。
    patterns: Vec<SecretPattern>,
    /// 与 patterns 平行的逐模式累计计数器。
    counters: Box<[AtomicU64]>,
    entropy_hits: AtomicU64,
}

/// Secret 出站脱敏门：值形态替换 + 熵替换 + 逐模式计数 + 超限 fail-closed。
///
/// `Clone` 共享同一计数器（克隆体与本体聚合统计，sink 各自持有克隆时
/// 全局可见）；熵配置与门限是纯配置值随克隆复制。门构造后不可变
/// （模式表在 `new` 时冻结）。
#[derive(Clone, Debug)]
pub struct SecretScanGate {
    core: Arc<GateCore>,
    /// 值熵扫描（None = 关闭；纯配置，无共享状态）。
    entropy: Option<EntropyScanner>,
    /// fail-closed 门限：单条输入内任一模式命中数上限。
    match_limit: usize,
}

impl SecretScanGate {
    /// 由模式注册表构建门（表在构建时冻结；熵扫描默认关闭，命中上限默认
    /// [`DEFAULT_MATCH_LIMIT`]）。
    #[must_use]
    pub fn new(registry: &SecretPatternRegistry) -> Self {
        let patterns = registry.patterns().to_vec();
        let counters = patterns.iter().map(|_| AtomicU64::new(0)).collect();
        Self {
            core: Arc::new(GateCore {
                patterns,
                counters,
                entropy_hits: AtomicU64::new(0),
            }),
            entropy: None,
            match_limit: DEFAULT_MATCH_LIMIT,
        }
    }

    /// 启用值熵扫描（高熵 token 以 [`HIGH_ENTROPY_REPLACEMENT`] 替换，
    /// 计入 [`HIGH_ENTROPY_PATTERN`] 名下）。
    #[must_use]
    pub fn with_entropy(mut self, scanner: EntropyScanner) -> Self {
        self.entropy = Some(scanner);
        self
    }

    /// 设置 fail-closed 门限（单条输入内任一模式命中数上限）。
    #[must_use]
    pub fn with_match_limit(mut self, limit: usize) -> Self {
        self.match_limit = limit;
        self
    }

    /// 当前 fail-closed 门限。
    #[must_use]
    pub fn match_limit(&self) -> usize {
        self.match_limit
    }

    /// 熵扫描配置（未启用返回 `None`）。
    #[must_use]
    pub fn entropy(&self) -> Option<&EntropyScanner> {
        self.entropy.as_ref()
    }

    /// 不返回错误的出口：替换全部命中并计数。
    ///
    /// 与 `DataMasker::mask` 同契约：超 1 MiB 输入跳过扫描并以整体遮蔽
    /// 收口（不扫描即不可证安全）；脱敏标记文本不被特殊处理（标记不被任何
    /// 内置模式命中，幂等自然成立），同条消息中的裸 secret 照常替换与计数。
    #[must_use]
    pub fn mask(&self, text: &str) -> String {
        self.mask_inner(text).0
    }

    /// fail-closed 出口：替换语义同 [`Self::mask`]，但任一模式命中数超过
    /// 门限、或输入超过 1 MiB 掩码上限（不扫描即不可证安全）时返回
    /// `Err`——输出不可用，命中类错误的计数器已更新（诊断可查）。
    ///
    /// 门限 0 语义为「任一命中即拒绝」。
    ///
    /// # Errors
    /// - `MaskError::SecretScanLimit`：任一模式（含熵命中）在单条输入内
    ///   命中数 > 门限，携带首个越限模式名与计数。
    /// - `MaskError::SecretScanOversizedInput`：输入超过掩码上限，未做
    ///   任何扫描（`mask` 对同输入以整体遮蔽收口，两出口在此分叉）。
    pub fn mask_checked(&self, text: &str) -> Result<String, MaskError> {
        // fail-closed 不豁免超大输入：跳过扫描的原样放行只属于 fail-open
        // 的 mask；这里拒绝输出，把「内容是否含 secret」的不确定性显性化。
        if text.len() > MAX_MASK_INPUT_BYTES {
            tracing::warn!(
                size = text.len(),
                limit = MAX_MASK_INPUT_BYTES,
                "{}",
                "oversized input rejected by the fail-closed secret scan exit"
            );
            return Err(MaskError::SecretScanOversizedInput {
                size: text.len(),
                limit: MAX_MASK_INPUT_BYTES,
            });
        }
        let (output, counts) = self.mask_inner(text);
        let limit = self.match_limit;
        if let Some((pattern, count)) = counts.iter().find(|(_, count)| *count > limit as u64) {
            tracing::warn!(
                pattern = pattern.as_str(),
                count = count,
                limit,
                "{}",
                "secret scan limit exceeded; output withheld (fail-closed)"
            );
            return Err(MaskError::SecretScanLimit {
                pattern: pattern.clone(),
                count: *count,
                limit,
            });
        }
        Ok(output)
    }

    /// 检测面：报告值形态与熵命中的模式名 + 原文字节区间。
    ///
    /// 纯读取（不计数、不改写），不做标记短路——已脱敏文本中的残余
    /// secret 仍要可见，语义对齐 `DataMasker::detect`。
    #[must_use]
    pub fn scan(&self, text: &str) -> Vec<SecretMatch> {
        let mut matches = Vec::new();
        for pattern in &self.core.patterns {
            for span in pattern.find_spans_iter(text) {
                // 引擎错误：fancy-regex 在首个错误处终止迭代，其后命中不可信
                let Ok((start, end)) = span else { break };
                matches.push(SecretMatch {
                    pattern: pattern.name().to_string(),
                    start,
                    end,
                });
            }
        }
        if let Some(scanner) = &self.entropy {
            for (start, end) in scanner.scan(text) {
                matches.push(SecretMatch {
                    pattern: HIGH_ENTROPY_PATTERN.to_string(),
                    start,
                    end,
                });
            }
        }
        matches
    }

    /// 逐模式累计命中数快照（含 0 计数的模式；熵命中排在末位，未启用时
    /// 不出现）。顺序与模式注册顺序一致。
    #[must_use]
    pub fn hit_counts(&self) -> Vec<(String, u64)> {
        let mut counts: Vec<(String, u64)> = self
            .core
            .patterns
            .iter()
            .zip(self.core.counters.iter())
            .map(|(pattern, counter)| (pattern.name().to_string(), counter.load(Ordering::Relaxed)))
            .collect();
        if self.entropy.is_some() {
            counts.push((
                HIGH_ENTROPY_PATTERN.to_string(),
                self.core.entropy_hits.load(Ordering::Relaxed),
            ));
        }
        counts
    }

    /// 单模式累计命中数；未知模式名返回 `None`。
    #[must_use]
    pub fn hit_count(&self, pattern: &str) -> Option<u64> {
        if pattern == HIGH_ENTROPY_PATTERN && self.entropy.is_some() {
            return Some(self.core.entropy_hits.load(Ordering::Relaxed));
        }
        self.core
            .patterns
            .iter()
            .position(|p| p.name() == pattern)
            .map(|idx| self.core.counters[idx].load(Ordering::Relaxed))
    }

    /// 累计命中总数（全模式 + 熵）。
    #[must_use]
    pub fn total_hits(&self) -> u64 {
        let patterns: u64 = self
            .core
            .counters
            .iter()
            .map(|counter| counter.load(Ordering::Relaxed))
            .sum();
        patterns + self.core.entropy_hits.load(Ordering::Relaxed)
    }

    /// 统一替换路径：返回（改写结果，本次命中 >0 的各模式计数）。
    ///
    /// 值形态按注册顺序串行替换，熵扫描最后在替换产物上进行——替换标记
    /// 片段（`REDACTED` 等）长度不足最短 token，不会被熵扫描再命中。
    /// 干净文本零中间分配：结果 Option 惰性拷贝、counts 只收非零命中、
    /// 无命中的模式不重组字符串。
    fn mask_inner(&self, text: &str) -> (String, Vec<(String, u64)>) {
        // 与 DataMasker::mask 同契约：超大输入跳过扫描（日志热路径不做 CPU
        // 放大）并以整体遮蔽收口——不扫描即不可证安全，原样放行会让其中的
        // 裸 secret 明文落盘；fail-closed 出口在 mask_checked 中显式拒绝。
        if text.len() > MAX_MASK_INPUT_BYTES {
            tracing::warn!(
                size = text.len(),
                limit = MAX_MASK_INPUT_BYTES,
                "{}",
                "Masking skipped: input exceeds the 1 MiB limit; content fully redacted (not scanned means not provably safe)"
            );
            return (
                super::engine::OVERSIZED_INPUT_REDACTION.to_string(),
                Vec::new(),
            );
        }

        // 标记不做短路：脱敏标记不被任何内置模式命中（幂等自然成立），
        // 而子串短路会让同条消息中的裸 secret 免于检测与计数。
        let mut result: Option<String> = None;
        let mut counts: Vec<(String, u64)> = Vec::new();
        for (idx, pattern) in self.core.patterns.iter().enumerate() {
            let base = result.as_deref().unwrap_or(text);
            let Ok((next, hits)) = replace_pattern(base, pattern) else {
                // 引擎错误：该模式的命中集合不可信——整条文本保守遮蔽
                // （fail-closed），裸 secret 不得留在文本里
                result = Some(super::engine::ENGINE_ERROR_REDACTION.to_string());
                break;
            };
            if hits > 0 {
                counts.push((pattern.name().to_string(), hits));
                result = Some(next.into_owned());
                self.core.counters[idx].fetch_add(hits, Ordering::Relaxed);
            }
        }
        if let Some(scanner) = &self.entropy {
            let base = result.as_deref().unwrap_or(text);
            let spans = scanner.scan(base);
            let hits = spans.len() as u64;
            if hits > 0 {
                counts.push((HIGH_ENTROPY_PATTERN.to_string(), hits));
                result = Some(splice(base, &spans, HIGH_ENTROPY_REPLACEMENT));
                self.core.entropy_hits.fetch_add(hits, Ordering::Relaxed);
            }
        }
        (result.unwrap_or_else(|| text.to_string()), counts)
    }
}

/// 单模式单趟扫描替换：命中区间替换为模式标记，返回（新文本，命中数）。
///
/// 单趟定位 + 区间重组，避免先 find 再 replace 的双重扫描；无命中返回
/// 借用原文（热路径主导场景零分配）；引擎级匹配错误上抛（`Err`）——
/// 命中集合不再可信，调用方须 fail-closed，不得把未遮蔽的裸 secret 留在
/// 文本里（`SecretPattern::find_spans_iter` 内已留痕）。
fn replace_pattern<'a>(
    text: &'a str,
    pattern: &SecretPattern,
) -> Result<(std::borrow::Cow<'a, str>, u64), ()> {
    let mut spans = pattern.find_spans_iter(text).peekable();
    match spans.peek() {
        None => return Ok((std::borrow::Cow::Borrowed(text), 0)),
        Some(Err(_)) => return Err(()),
        Some(Ok(_)) => {}
    }
    let mut out = String::with_capacity(text.len());
    let mut last = 0usize;
    let mut hits: u64 = 0;
    for span in spans {
        let (start, end) = span.map_err(|_| ())?;
        out.push_str(&text[last..start]);
        out.push_str(pattern.replacement());
        last = end;
        hits += 1;
    }
    out.push_str(&text[last..]);
    Ok((std::borrow::Cow::Owned(out), hits))
}

/// 按字节区间集合重组文本（区间互不重叠且有序）。
fn splice(text: &str, spans: &[(usize, usize)], replacement: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0usize;
    for (start, end) in spans {
        out.push_str(&text[last..*start]);
        out.push_str(replacement);
        last = *end;
    }
    out.push_str(&text[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SK_KEY: &str = "sk-proj-abcdefghijklmnopqrstuvwxyz123456";
    const JWT: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
    /// 30 个互异字符：熵 ≈ 4.91 bits/char，默认阈值之上。
    const HIGH_ENTROPY_TOKEN: &str = "aB3xK9mP2qR7sT5uV1wX8yZ4nC6jM0";

    fn gate() -> SecretScanGate {
        SecretScanGate::new(&SecretPatternRegistry::with_builtins())
    }

    fn gate_with_entropy() -> SecretScanGate {
        gate().with_entropy(EntropyScanner::new())
    }

    // mask：替换 + 计数（fail-closed：超限整体遮蔽）

    #[test]
    fn test_mask_replaces_and_counts_per_pattern() {
        let g = gate();
        let text = format!("a {SK_KEY} b {JWT}");
        let out = g.mask(&text);
        assert!(!out.contains(SK_KEY), "sk key must be replaced: {out}");
        assert!(out.contains("***REDACTED_API_KEY***"), "{out}");
        assert!(out.contains("***REDACTED_JWT***"), "{out}");
        assert_eq!(g.hit_count("openai_sk"), Some(1));
        assert_eq!(g.hit_count("jwt"), Some(1));
        assert_eq!(g.total_hits(), 2);
    }

    #[test]
    fn test_mask_clean_text_untouched() {
        let g = gate();
        let text = "nothing sensitive here";
        assert_eq!(g.mask(text), text);
        assert_eq!(g.total_hits(), 0);
    }

    #[test]
    fn test_mask_counters_cumulative_across_calls() {
        let g = gate();
        let _ = g.mask(&format!("one {SK_KEY}"));
        let _ = g.mask(&format!("two {SK_KEY}"));
        assert_eq!(g.hit_count("openai_sk"), Some(2));
    }

    #[test]
    fn test_mask_fail_open_over_limit() {
        // mask 不做 fail-closed：超限也返回改写结果（向后兼容语义）
        let g = gate().with_match_limit(1);
        let text = format!("x {SK_KEY} y {} z", SK_KEY.replace("proj", "turb"));
        let out = g.mask(&text);
        assert!(!out.contains("sk-proj-"), "still replaced: {out}");
        assert_eq!(g.hit_count("openai_sk"), Some(2));
    }

    #[test]
    fn test_mask_marker_idempotency_skips_gate() {
        // 标记文本本身不被任何内置模式命中，mask 幂等；但标记不再关闭
        // 检测面（见 test_mask_scans_through_redaction_markers）。
        let g = gate();
        let marked = "token=***REDACTED*** done";
        assert_eq!(g.mask(marked), marked);
        assert_eq!(g.total_hits(), 0, "marker text alone must not be counted");
    }

    #[test]
    fn test_mask_scans_through_redaction_markers() {
        // 假标记不关闭检测面：标记文本保持原样，同条消息中的裸 secret
        // 照常替换与计数——攻击者放置一个标记不能豁免整条记录。
        let g = gate();
        let text = format!("***REDACTED*** token={SK_KEY} done");
        let out = g.mask(&text);
        assert!(
            out.contains("***REDACTED***"),
            "marker text must be preserved: {out}"
        );
        assert!(
            !out.contains(SK_KEY),
            "secret beside a marker must still be replaced: {out}"
        );
        assert_eq!(g.hit_count("openai_sk"), Some(1));
    }

    #[test]
    fn test_mask_checked_scans_through_redaction_markers() {
        let g = gate();
        let text = format!("***MASKED*** token={SK_KEY}");
        let out = g.mask_checked(&text).expect("single hit is under limit");
        assert!(
            out.contains("***MASKED***"),
            "marker text must be preserved: {out}"
        );
        assert!(!out.contains(SK_KEY));
    }

    #[test]
    fn test_mask_checked_oversized_input_fails_closed() {
        // fail-closed 出口不豁免超大输入：不扫描即不可证安全，拒绝输出
        // 而非原样放行（mask 同样 fail-closed 整体遮蔽，见 oversized redacted 测试）。
        let g = gate();
        let big = format!("blob {SK_KEY} {}", "x".repeat(1024 * 1024 + 1));
        match g.mask_checked(&big) {
            Err(MaskError::SecretScanOversizedInput { size, limit }) => {
                assert!(size > limit, "size {size} must exceed limit {limit}");
            }
            other => panic!("oversized input must fail closed, got: {other:?}"),
        }
        assert_eq!(g.total_hits(), 0, "rejected input must not be counted");
    }

    #[test]
    fn test_mask_oversized_input_is_redacted() {
        let g = gate();
        let big = format!("blob {SK_KEY} {}", "x".repeat(1024 * 1024 + 1));
        let out = g.mask(&big);
        // 超限输入不扫描即不可证安全：整体遮蔽而非原样放行
        assert!(
            !out.contains(SK_KEY),
            "oversized input must not pass through raw"
        );
        assert_eq!(out, super::super::engine::OVERSIZED_INPUT_REDACTION);
        assert_eq!(g.total_hits(), 0);
    }

    // 熵扫描接入

    #[test]
    fn test_entropy_replacement_and_counter() {
        let g = gate_with_entropy();
        let text = format!("blob {HIGH_ENTROPY_TOKEN} hex 550e8400e29b41d4a716446655440000");
        let out = g.mask(&text);
        assert!(out.contains(HIGH_ENTROPY_REPLACEMENT), "{out}");
        assert!(!out.contains(HIGH_ENTROPY_TOKEN), "{out}");
        // hex UUID 串（熵 ≤ 4.0）不误伤
        assert!(out.contains("550e8400e29b41d4a716446655440000"), "{out}");
        assert_eq!(g.hit_count(HIGH_ENTROPY_PATTERN), Some(1));
    }

    #[test]
    fn test_entropy_disabled_by_default() {
        let g = gate();
        assert!(g.entropy().is_none());
        let text = format!("blob {HIGH_ENTROPY_TOKEN}");
        assert_eq!(g.mask(&text), text);
        assert!(g.hit_count(HIGH_ENTROPY_PATTERN).is_none());
    }

    // mask_checked：fail-closed

    #[test]
    fn test_mask_checked_ok_under_limit() {
        let g = gate().with_match_limit(2);
        let text = format!("x {SK_KEY} y {} z", SK_KEY.replace("proj", "turb"));
        let out = g.mask_checked(&text).expect("under limit must be Ok");
        assert!(!out.contains("sk-proj-"));
        assert!(!out.contains("sk-turb-"));
        assert_eq!(g.hit_count("openai_sk"), Some(2));
    }

    #[test]
    fn test_mask_checked_fails_closed_over_limit() {
        let g = gate().with_match_limit(2);
        let text = format!(
            "x {SK_KEY} y {} z {}",
            SK_KEY.replace("proj", "turb"),
            SK_KEY.replace("proj", "nova")
        );
        match g.mask_checked(&text) {
            Err(MaskError::SecretScanLimit {
                pattern,
                count,
                limit,
            }) => {
                assert_eq!(pattern, "openai_sk");
                assert_eq!(count, 3);
                assert_eq!(limit, 2);
            }
            other => panic!("over-limit must fail closed, got: {other:?}"),
        }
        // fail-closed 不吞计数：越限事实已入账
        assert_eq!(g.hit_count("openai_sk"), Some(3));
    }

    #[test]
    fn test_mask_checked_zero_limit_rejects_any_hit() {
        let g = gate().with_match_limit(0);
        let text = format!("one {SK_KEY}");
        assert!(g.mask_checked(&text).is_err());
    }

    #[test]
    fn test_mask_checked_clean_text_ok() {
        let g = gate().with_match_limit(0);
        assert_eq!(g.mask_checked("plain text").unwrap(), "plain text");
    }

    #[test]
    fn test_mask_checked_entropy_over_limit() {
        let g = gate_with_entropy().with_match_limit(1);
        let text = format!(
            "{HIGH_ENTROPY_TOKEN} {} tail",
            HIGH_ENTROPY_TOKEN.replace('a', "z")
        );
        match g.mask_checked(&text) {
            Err(MaskError::SecretScanLimit { pattern, count, .. }) => {
                assert_eq!(pattern, HIGH_ENTROPY_PATTERN);
                assert_eq!(count, 2);
            }
            other => panic!("entropy over-limit must fail closed, got: {other:?}"),
        }
    }

    // scan 检测面

    #[test]
    fn test_scan_reports_without_counting() {
        let g = gate_with_entropy();
        let text = format!("a {SK_KEY} b {HIGH_ENTROPY_TOKEN}");
        let matches = g.scan(&text);
        let names: Vec<&str> = matches.iter().map(|m| m.pattern.as_str()).collect();
        assert!(names.contains(&"openai_sk"));
        assert!(names.contains(&HIGH_ENTROPY_PATTERN));
        assert_eq!(g.total_hits(), 0, "scan is pure detection");
    }

    #[test]
    fn test_scan_sees_through_redaction_markers() {
        // 检测面不做标记短路：已脱敏文本旁的裸 secret 仍要可见
        let g = gate();
        let text = format!("***REDACTED*** {SK_KEY}");
        assert_eq!(g.scan(&text).len(), 1);
    }

    // 计数器查询面

    #[test]
    fn test_hit_counts_includes_zero_count_patterns() {
        let g = gate_with_entropy();
        let counts = g.hit_counts();
        assert_eq!(counts.len(), 10, "9 builtin patterns + entropy entry");
        assert!(counts.iter().all(|(_, c)| *c == 0));
        assert_eq!(counts.last().unwrap().0, HIGH_ENTROPY_PATTERN);
    }

    #[test]
    fn test_hit_count_unknown_pattern_is_none() {
        let g = gate();
        assert!(g.hit_count("no_such_pattern").is_none());
    }

    #[test]
    fn test_clone_shares_counters() {
        let g = gate();
        let clone = g.clone();
        let _ = clone.mask(&format!("one {SK_KEY}"));
        assert_eq!(
            g.hit_count("openai_sk"),
            Some(1),
            "clone must share counters"
        );
    }

    // 配置面

    #[test]
    fn test_match_limit_accessor() {
        assert_eq!(gate().match_limit(), DEFAULT_MATCH_LIMIT);
        assert_eq!(gate().with_match_limit(7).match_limit(), 7);
    }

    #[test]
    fn test_custom_pattern_in_gate() {
        let mut registry = SecretPatternRegistry::default();
        registry
            .register(
                "corp_token",
                r"\bCORP-[A-Z0-9]{32}\b",
                "***REDACTED_CORP***",
            )
            .unwrap();
        let g = SecretScanGate::new(&registry);
        let out = g.mask("id CORP-01234567890123456789012345678901 ok");
        assert!(out.contains("***REDACTED_CORP***"), "{out}");
        assert_eq!(g.hit_count("corp_token"), Some(1));
        assert_eq!(g.hit_counts().len(), 1, "only registered patterns counted");
    }
}
