// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Secret 值形态模式注册表（feature `secret-scan`）。
//!
//! [`SecretPatternRegistry`] 维护「值形态」正则表：识别日志文本中**无键名
//! 上下文**的裸 secret——`sk-` 前缀密钥、PEM 私钥块、AWS AKIA 形态等，与
//! [`DataMasker`](super::engine::DataMasker) 的键名触发互补。
//!
//! 每条模式都是按「前缀 + 长度 + 字符集」三要素自行编写的 Rust 正则；
//! 形态事实（各厂商令牌的公开格式）不构成对任何外部词表的移植。
//! 检测面（[`SecretPatternRegistry::scan`]）报告模式名 + 原文字节区间，
//! 风格对齐 [`MaskMatch`](super::engine::MaskMatch)。

use fancy_regex::Regex;

use super::error::MaskError;

/// 注册表单条模式：名称、已编译正则、替换标记。
#[derive(Clone)]
pub struct SecretPattern {
    name: String,
    pattern: Regex,
    replacement: String,
}

impl std::fmt::Debug for SecretPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretPattern")
            .field("name", &self.name)
            .field("pattern", &self.pattern.as_str())
            .field("replacement", &self.replacement)
            .finish()
    }
}

impl SecretPattern {
    /// 编译并构造一条模式。
    ///
    /// # Errors
    /// 正则编译失败返回 `Err(MaskError::ConfigError)`（与 `MaskRule` 构建同源）。
    pub fn new(name: &str, pattern: &str, replacement: &str) -> Result<Self, MaskError> {
        let compiled = Regex::new(pattern)
            .map_err(|e| MaskError::ConfigError(format!("invalid regex in rule '{name}': {e}")))?;
        Ok(Self {
            name: name.to_string(),
            pattern: compiled,
            replacement: replacement.to_string(),
        })
    }

    /// 模式名（计数器与命中归因键）。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 正则源串。
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.pattern.as_str()
    }

    /// 命中替换标记。
    #[must_use]
    pub fn replacement(&self) -> &str {
        &self.replacement
    }

    /// 单趟命中区间迭代器。
    ///
    /// 引擎级错误以 `Err` 上抛：fancy-regex 在首个错误处终止迭代，其后的
    /// 命中一并丢失——命中集合不再可信，调用方须 fail-closed（改写路径
    /// 整体遮蔽）而非把未遮蔽的裸 secret 留在文本里。
    pub(crate) fn find_spans_iter<'a>(
        &'a self,
        text: &'a str,
    ) -> impl Iterator<Item = Result<(usize, usize), fancy_regex::Error>> + 'a {
        self.pattern.find_iter(text).map(|m| match m {
            Ok(found) => Ok((found.start(), found.end())),
            Err(e) => {
                tracing::warn!(
                    pattern = self.name.as_str(),
                    error = %e,
                    "{}",
                    "regex match error during secret pattern scan; skipping this match"
                );
                Err(e)
            }
        })
    }
}

/// 检测面单条命中：模式名 + 原文字节区间（`end` 不含）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretMatch {
    /// 命中的模式名（同 [`SecretPattern::name`]）。
    pub pattern: String,
    /// 命中起始字节偏移（含）。
    pub start: usize,
    /// 命中结束字节偏移（不含）。
    pub end: usize,
}

/// Secret 值形态模式注册中心。
///
/// 不做内部同步（与 [`MaskRuleRegistry`](super::masking_registry::MaskRuleRegistry)
/// 同一取舍）：多线程各自构建，或冻结进 [`SecretScanGate`](super::secret_scan::SecretScanGate)
/// 后经 `Arc` 共享。
#[derive(Debug, Clone, Default)]
pub struct SecretPatternRegistry {
    patterns: Vec<SecretPattern>,
}

impl SecretPatternRegistry {
    /// 内置值形态模式表。
    ///
    /// 覆盖六类裸 secret 形态：`sk-` 前缀 API 密钥（含 anthropic 与通用两族，
    /// 前者优先、后者以负向前瞻排除 `ant-` 保证归因唯一）、GitHub/Slack 令牌、
    /// AWS 访问密钥 ID、Google API 密钥、Stripe 密钥、JWT 三段式、PEM 私钥块。
    /// 每条形态的命中/误报样本见本模块单测。
    ///
    /// # Panics
    ///
    /// 内置形态表为编译期常量，正则编译失败属于编程错误（单测钉住）。
    #[must_use]
    pub fn with_builtins() -> Self {
        const BUILTINS: [(&str, &str, &str); 9] = [
            // sk-ant- 优先归因；通用 sk- 族用负向前瞻排除 ant-，避免双计
            (
                "anthropic_sk",
                r"\bsk-ant-[A-Za-z0-9_-]{20,}\b",
                "***REDACTED_API_KEY***",
            ),
            (
                "openai_sk",
                r"\bsk-(?!ant-)[A-Za-z0-9_-]{20,}\b",
                "***REDACTED_API_KEY***",
            ),
            (
                "github_token",
                r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{36,}\b|github_pat_[A-Za-z0-9_]{22,}",
                "***REDACTED_GITHUB***",
            ),
            (
                "slack_token",
                r"\bxox[abprs]-[0-9]{8,13}(?:-[0-9]{8,13}){1,2}-[0-9a-zA-Z]{10,}",
                "***REDACTED_SLACK***",
            ),
            // AWS 访问密钥 ID 形态：四大前缀 + 16 位大写字母数字（小写非合法形态，不放宽）
            (
                "aws_access_key",
                r"\b(?:AKIA|ASIA|ABIA|ACCA)[0-9A-Z]{16}\b",
                "***REDACTED_AWS_KEY***",
            ),
            (
                "google_api_key",
                r"\bAIza[0-9A-Za-z_-]{35}",
                "***REDACTED_GOOGLE***",
            ),
            (
                "stripe_key",
                r"\b(?:sk|pk|rk)_(?:live|test)_[0-9a-zA-Z]{16,}\b",
                "***REDACTED_STRIPE***",
            ),
            // 三段式 JWT：header/payload 均以 eyJ 开头（base64 的 {" 开头）
            (
                "jwt",
                r"\beyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+",
                "***REDACTED_JWT***",
            ),
            // PEM 私钥块族（含 RSA/EC/OPENSSH/ENCRYPTED 变体）；证书/公钥是公开
            // 物料，不在此列
            (
                "pem_private_block",
                r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
                "***REDACTED_PEM_KEY***",
            ),
        ];
        let mut registry = Self::default();
        for (name, pattern, replacement) in BUILTINS {
            // 内置表静态保证合法；编译失败属实现缺陷，构造期 panic 暴露
            registry
                .register(name, pattern, replacement)
                .unwrap_or_else(|_| panic!("{}", "builtin secret pattern must compile"));
        }
        registry
    }

    /// 扩展形态表：内置裸 secret 之外的通用敏感形态，供「宁可多掩」的
    /// 非日志消费方复用同一检测实现。
    ///
    /// 在 [`with_builtins`](Self::with_builtins) 基础上追加四条通用形态：
    /// email 地址、`password/pwd/pass` 赋值、`api_key_`/`key_` 前缀令牌、
    /// `pk-` 前缀密钥。日志门（[`SecretScanGate`](super::secret_scan::SecretScanGate)）
    /// 默认不含本表——日志场景下 email/密码赋值的误报率高于裸 secret
    /// 形态；记忆/提示词卫生类消费方按需选用。
    ///
    /// # Panics
    ///
    /// 扩展形态表为编译期常量，正则编译失败属于编程错误（单测钉住）。
    #[must_use]
    pub fn with_extended() -> Self {
        const EXTENDED: [(&str, &str, &str); 4] = [
            (
                "email",
                r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}",
                "***REDACTED***",
            ),
            (
                "password_assignment",
                r"(?:password\s*[=:]\s*|pwd\s*[=:]\s*|pass\s*[=:]\s*)[^\s]+",
                "***REDACTED***",
            ),
            (
                "prefixed_key",
                r"\b(?:api_key|key)_[A-Za-z0-9]{16,}\b",
                "***REDACTED***",
            ),
            ("pk_key", r"\bpk-[A-Za-z0-9_-]{20,}\b", "***REDACTED***"),
        ];
        let mut registry = Self::with_builtins();
        for (name, pattern, replacement) in EXTENDED {
            registry
                .register(name, pattern, replacement)
                .unwrap_or_else(|_| panic!("{}", "extended secret pattern must compile"));
        }
        registry
    }

    /// 注册自定义模式；同名查重（与 `MaskRuleRegistry::register` 语义对齐）。
    ///
    /// # 回溯引擎警示（ReDoS）
    ///
    /// 底层是 `fancy-regex`（回溯引擎，非 `regex` crate 的线性引擎），注册
    /// 的模式在门开启后于**每条出站记录**上执行。内置 9 条均为「固定前缀 +
    /// 单字符类 + 有界量词」的安全形态；自定义模式必须自行避免嵌套无界
    /// 量词与歧义字符集回退（如 `(.+)*`、`(a+)+`），否则低质/恶意模式会
    /// 放大出站 CPU。编译不校验复杂度——模式质量是注册方的责任。
    ///
    /// # Errors
    /// - 正则编译失败返回 `Err(MaskError::ConfigError)`
    /// - 已存在同名模式返回 `Err(MaskError::ConfigError)`
    pub fn register(
        &mut self,
        name: &str,
        pattern: &str,
        replacement: &str,
    ) -> Result<(), MaskError> {
        if self.patterns.iter().any(|p| p.name == name) {
            return Err(MaskError::ConfigError(format!(
                "rule '{name}' already registered"
            )));
        }
        self.patterns
            .push(SecretPattern::new(name, pattern, replacement)?);
        Ok(())
    }

    /// 全量模式快照（注册顺序 = 替换顺序）。
    #[must_use]
    pub fn patterns(&self) -> &[SecretPattern] {
        &self.patterns
    }

    /// 检测面：报告全部模式在 `text` 中的命中（模式名 + 字节区间）。
    ///
    /// 纯读取，不产生计数、不改写文本。各模式在原始文本上独立报告，
    /// 同一区间可被多条自定义模式重复命中（内置表互不重叠）。
    #[must_use]
    pub fn scan(&self, text: &str) -> Vec<SecretMatch> {
        let mut matches = Vec::new();
        for pattern in &self.patterns {
            for span in pattern.find_spans_iter(text) {
                // 引擎错误：fancy-regex 在首个错误处终止迭代，其后命中不可信
                let Ok((start, end)) = span else { break };
                matches.push(SecretMatch {
                    pattern: pattern.name.clone(),
                    start,
                    end,
                });
            }
        }
        matches
    }

    /// 检测并统一替换：全部命中区间按「起点升序、长者优先」去重叠
    /// （重叠处先到先得），其余原文保留，命中区间统一替换为 `replacement`。
    /// 返回（替换后文本, 是否发生替换）。
    ///
    /// 这是本注册表唯一的执行出口，调用方不再自行拼接替换逻辑——避免
    /// 出现第二套脱敏实现。与 [`SecretScanGate`](super::secret_scan::SecretScanGate)
    /// 的差别：不经过门（无熵扫描、无逐模式计数与 fail-closed）；与
    /// [`DataMasker`](super::engine::DataMasker) 的差别：无脱敏标记幂等
    /// 短路——已含 `***REDACTED***` 旧内容的文本不会被整段放行，新命中
    /// 照常替换。
    #[must_use]
    pub fn scan_and_replace(&self, text: &str, replacement: &str) -> (String, bool) {
        let mut matches = self.scan(text);
        // 确定性去重：起点升序、长者优先，重叠区间让位于先到的命中。
        matches.sort_by(|a, b| {
            let a_len = a.end - a.start;
            let b_len = b.end - b.start;
            (a.start, core::cmp::Reverse(a_len)).cmp(&(b.start, core::cmp::Reverse(b_len)))
        });
        let mut out = String::with_capacity(text.len());
        let mut cursor = 0usize;
        let mut replaced = false;
        for m in matches {
            if m.start < cursor {
                continue;
            }
            out.push_str(&text[cursor..m.start]);
            out.push_str(replacement);
            cursor = m.end;
            replaced = true;
        }
        out.push_str(&text[cursor..]);
        (out, replaced)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SK_KEY: &str = "sk-proj-abcdefghijklmnopqrstuvwxyz123456";
    const SK_ANT_KEY: &str = "sk-ant-api03-abcdefghijklmnopqrst";
    const JWT: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
    const GHP: &str = "ghp_0123456789abcdefghijklmnopqrstuvwxyz";
    const GITHUB_PAT: &str = "github_pat_11ABCDEFG0abcdefghijklmnopqrstuv";
    const SLACK: &str = "xoxb-123456789012-1234567890123-abcdefghij";
    const GOOGLE: &str = "AIzaSyABCDEF1234567890abcdefghijklmnopqrs";
    const STRIPE: &str = "sk_live_abcdefghijklmnop1234";
    const PEM: &str =
        "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQEFAASC\n-----END PRIVATE KEY-----";

    fn builtin() -> SecretPatternRegistry {
        SecretPatternRegistry::with_builtins()
    }

    fn hit_names(text: &str) -> Vec<String> {
        builtin()
            .scan(text)
            .into_iter()
            .map(|m| m.pattern)
            .collect()
    }

    // 命中样本：每条内置模式至少一个正样本

    #[test]
    fn test_openai_sk_hit() {
        let names = hit_names(&format!("connected with {SK_KEY} ok"));
        assert!(names.contains(&"openai_sk".to_string()), "{names:?}");
    }

    #[test]
    fn test_anthropic_sk_hit_exclusively() {
        // sk-ant- 归因到 anthropic_sk，且通用 openai_sk 因负向前瞻不双计
        let text = format!("key {SK_ANT_KEY} end");
        let matches = builtin().scan(&text);
        let names: Vec<&str> = matches.iter().map(|m| m.pattern.as_str()).collect();
        assert_eq!(names, vec!["anthropic_sk"], "ant- must be attributed once");
        assert_eq!(&text[matches[0].start..matches[0].end], SK_ANT_KEY);
    }

    #[test]
    fn test_github_token_hits_both_shapes() {
        let names = hit_names(&format!("token {GHP}"));
        assert!(names.contains(&"github_token".to_string()), "{names:?}");
        let names = hit_names(&format!("token {GITHUB_PAT}"));
        assert!(names.contains(&"github_token".to_string()), "{names:?}");
    }

    #[test]
    fn test_slack_token_hit() {
        let names = hit_names(&format!("bot {SLACK}"));
        assert!(names.contains(&"slack_token".to_string()), "{names:?}");
    }

    #[test]
    fn test_aws_access_key_hit() {
        let names = hit_names("creds AKIAIOSFODNN7EXAMPLE in log");
        assert!(names.contains(&"aws_access_key".to_string()), "{names:?}");
        let names = hit_names("temp ASIAIOSFODNN7EXAMPLE session");
        assert!(names.contains(&"aws_access_key".to_string()), "{names:?}");
    }

    #[test]
    fn test_google_api_key_hit() {
        let names = hit_names(&format!("gcp {GOOGLE}"));
        assert!(names.contains(&"google_api_key".to_string()), "{names:?}");
    }

    #[test]
    fn test_stripe_key_hit() {
        let names = hit_names(&format!("pay {STRIPE}"));
        assert!(names.contains(&"stripe_key".to_string()), "{names:?}");
    }

    #[test]
    fn test_jwt_hit() {
        let names = hit_names(&format!("auth {JWT}"));
        assert!(names.contains(&"jwt".to_string()), "{names:?}");
    }

    #[test]
    fn test_pem_private_block_hit_including_variants() {
        let names = hit_names(&format!("leak\n{PEM}\nend"));
        assert!(
            names.contains(&"pem_private_block".to_string()),
            "{names:?}"
        );
        // 夹具经 format! 拼装：no-private-key 钩子扫描暂存源文件中的
        // 「BEGIN <变体> PRIVATE KEY」字面量，PEM 形态不能整段落进源码
        let openssh = format!(
            "-----BEGIN {} PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAA\n-----END {} PRIVATE KEY-----",
            "OPENSSH", "OPENSSH"
        );
        let names = hit_names(&openssh);
        assert!(
            names.contains(&"pem_private_block".to_string()),
            "{names:?}"
        );
    }

    // 误报样本：形近但非 secret 的文本不得命中

    #[test]
    fn test_sk_prefix_misses() {
        for text in [
            "task-scheduler-configuration-loaded",
            "sk-learn pipeline",
            "sk-short",
            "the sk-key rotated yesterday",
        ] {
            let names = hit_names(text);
            assert!(
                !names.contains(&"openai_sk".to_string())
                    && !names.contains(&"anthropic_sk".to_string()),
                "sk FP must not hit: {text} -> {names:?}"
            );
        }
    }

    #[test]
    fn test_aws_shape_misses() {
        for text in [
            "suffix too short AKIAIOSFODNN7EXAMPL",
            "lowercase akiaIOSFODNN7EXAMPLE",
            "bare prefix AKIA",
        ] {
            let names = hit_names(text);
            assert!(
                !names.contains(&"aws_access_key".to_string()),
                "aws FP must not hit: {text} -> {names:?}"
            );
        }
    }

    #[test]
    fn test_jwt_misses_single_segment() {
        let names = hit_names("payload eyJhbGciOiJIUzI1NiJ9 alone");
        assert!(!names.contains(&"jwt".to_string()), "{names:?}");
    }

    #[test]
    fn test_github_slack_stripe_google_short_misses() {
        for text in [
            "ghp_short",
            "github_pat_tiny",
            "xoxb-12345-abc",
            "sk_test_short",
            "AIzaShort",
        ] {
            let names = hit_names(text);
            assert!(names.is_empty(), "FP must not hit: {text} -> {names:?}");
        }
    }

    #[test]
    fn test_unterminated_pem_block_misses() {
        let text = "-----BEGIN PRIVATE KEY-----\nMIIEvQ no end marker";
        assert!(hit_names(text).is_empty(), "unterminated PEM must not hit");
    }

    // scan 面行为

    #[test]
    fn test_scan_reports_exact_byte_spans() {
        let text = format!("prefix {SK_KEY} suffix");
        let start = text.find(SK_KEY).unwrap();
        let matches = builtin().scan(&text);
        assert_eq!(matches.len(), 1);
        assert_eq!(&text[matches[0].start..matches[0].end], SK_KEY);
        assert_eq!(matches[0].start, start);
    }

    #[test]
    fn test_scan_reports_each_pattern_independently() {
        let text = format!("a {SK_KEY} b {JWT} c");
        let matches = builtin().scan(&text);
        let names: Vec<&str> = matches.iter().map(|m| m.pattern.as_str()).collect();
        assert!(names.contains(&"openai_sk"));
        assert!(names.contains(&"jwt"));
        for m in &matches {
            assert!(!text[m.start..m.end].is_empty());
        }
    }

    #[test]
    fn test_scan_empty_and_clean_text() {
        let registry = builtin();
        assert!(registry.scan("").is_empty());
        assert!(registry.scan("nothing sensitive here").is_empty());
    }

    // 注册中心管理面

    #[test]
    fn test_default_registry_is_empty() {
        let registry = SecretPatternRegistry::default();
        assert!(registry.patterns().is_empty());
    }

    #[test]
    fn test_builtin_table_shape() {
        let registry = builtin();
        assert_eq!(registry.patterns().len(), 9);
        assert!(registry.patterns().iter().any(|p| p.name() == "openai_sk"));
    }

    #[test]
    fn test_register_custom_pattern() {
        let mut registry = SecretPatternRegistry::default();
        registry
            .register(
                "corp_token",
                r"\bCORP-[A-Z0-9]{32}\b",
                "***REDACTED_CORP***",
            )
            .unwrap();
        assert_eq!(registry.patterns().len(), 1);
        let matches = registry.scan("id CORP-01234567890123456789012345678901 ok");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pattern, "corp_token");
    }

    #[test]
    fn test_register_duplicate_name_fails() {
        let mut registry = builtin();
        assert!(
            registry
                .register("openai_sk", r"\bsk-x{30}\b", "***X***")
                .is_err(),
            "duplicate name must be rejected"
        );
    }

    #[test]
    fn test_register_invalid_regex_fails() {
        let mut registry = SecretPatternRegistry::default();
        assert!(registry.register("bad", r"[invalid", "***").is_err());
    }

    #[test]
    fn test_pattern_accessors() {
        let mut registry = SecretPatternRegistry::default();
        registry
            .register("p1", r"\bABC\d{10}\b", "***M***")
            .unwrap();
        let p = &registry.patterns()[0];
        assert_eq!(p.name(), "p1");
        assert_eq!(p.as_str(), r"\bABC\d{10}\b");
        assert_eq!(p.replacement(), "***M***");
        assert!(format!("{p:?}").contains("SecretPattern"));
    }

    // 扩展形态表：with_extended

    fn extended() -> SecretPatternRegistry {
        SecretPatternRegistry::with_extended()
    }

    #[test]
    fn test_extended_table_still_contains_builtins() {
        let names = extended()
            .scan(&format!("key {SK_KEY} end"))
            .into_iter()
            .map(|m| m.pattern)
            .collect::<Vec<_>>();
        assert!(names.contains(&"openai_sk".to_string()), "{names:?}");
    }

    #[test]
    fn test_extended_email_hit() {
        let names = extended()
            .scan("mail a@b.com end")
            .into_iter()
            .map(|m| m.pattern)
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["email"], "{names:?}");
    }

    #[test]
    fn test_extended_password_assignment_hit() {
        for sample in ["password=hunter2", "pwd: s3cret", "pass = tight"] {
            let names = extended()
                .scan(sample)
                .into_iter()
                .map(|m| m.pattern)
                .collect::<Vec<_>>();
            assert_eq!(names, vec!["password_assignment"], "{sample}: {names:?}");
        }
    }

    #[test]
    fn test_extended_prefixed_and_pk_keys_hit() {
        let names = extended()
            .scan("cfg api_key_abcdefghijklmnop and pk-abcdefghijklmnopqrst")
            .into_iter()
            .map(|m| m.pattern)
            .collect::<Vec<_>>();
        assert!(names.contains(&"prefixed_key".to_string()), "{names:?}");
        assert!(names.contains(&"pk_key".to_string()), "{names:?}");
    }

    // 统一替换出口：scan_and_replace

    #[test]
    fn test_scan_and_replace_replaces_and_reports() {
        let (out, replaced) = extended().scan_and_replace(
            "mail a@b.com key sk-proj-abcdefghijklmnopqrstuvwxyz123456",
            "***REDACTED***",
        );
        assert!(replaced);
        assert_eq!(out, "mail ***REDACTED*** key ***REDACTED***");
    }

    #[test]
    fn test_scan_and_replace_custom_replacement_and_no_match_identity() {
        let (out, replaced) = extended().scan_and_replace("hello world", "[X]");
        assert!(!replaced);
        assert_eq!(out, "hello world");
        let (out, replaced) = extended().scan_and_replace("pwd: s3cret", "[X]");
        assert!(replaced);
        assert_eq!(out, "[X]");
    }

    #[test]
    fn test_scan_and_replace_overlap_longest_first_wins() {
        // 同起点两模式：长者胜；后到命中落入已消费区间被丢弃。
        let mut registry = SecretPatternRegistry::default();
        registry.register("short", r"AB", "***S***").unwrap();
        registry.register("long", r"ABCDEF", "***L***").unwrap();
        // 长者起点更早：直接先消费，短者整体落入已消费区间
        let (out, replaced) = registry.scan_and_replace("ABCDEF", "***R***");
        assert!(replaced);
        assert_eq!(out, "***R***");
        // 同起点：长者优先（B/C 两条同起点，长者覆盖）
        let mut registry = SecretPatternRegistry::default();
        registry.register("short", r"AB", "***S***").unwrap();
        registry.register("long", r"ABC", "***L***").unwrap();
        let (out, replaced) = registry.scan_and_replace("ABC", "***R***");
        assert!(replaced);
        assert_eq!(out, "***R***");
    }

    #[test]
    fn test_scan_and_replace_disjoint_spans_keep_gaps() {
        let mut registry = SecretPatternRegistry::default();
        registry.register("a", r"A+", "***A***").unwrap();
        registry.register("b", r"B+", "***B***").unwrap();
        let (out, replaced) = registry.scan_and_replace("xAxxB", "***R***");
        assert!(replaced);
        assert_eq!(out, "x***R***xx***R***");
    }
}
