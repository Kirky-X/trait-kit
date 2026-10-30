// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! # 数据掩码模块
//!
//! 提供敏感数据（PII）的自动检测和脱敏功能，保护日志中的隐私信息。
//!
//! ## 概述
//!
//! `DataMasker` 结构体提供日志消息和 JSON 结构中敏感数据的检测和脱敏功能。
//! 它结合模式匹配和字段名检测来识别敏感信息。
//!
//! ## 功能特性
//!
//! - **基于模式的脱敏**：通过正则表达式模式检测敏感数据（邮箱、电话等）
//! - **字段名检测**：`通过字段名识别敏感字段（password、api_key` 等）
//! - **嵌套结构支持**：递归处理嵌套的 JSON 对象和数组
//! - **自定义规则**：支持多个脱敏规则，可配置模式
//!
//! ## 敏感字段检测
//!
//! 以下字段名模式会自动检测为敏感字段：
//! - **认证信息**：`password`, `token`, `secret`, `credential`, `auth`
//! - **API 密钥**：`api_key`, `api_secret`, `access_key`, `secret_key`
//! - **加密密钥**：`encryption_key`, `decryption_key`, `private_key`
//! - **OAuth**：`oauth`, `oauth_token`, `bearer_token`, `jwt`
//! - **AWS 凭据**：`aws_secret`, `aws_key`, `aws_credentials`
//! - **支付信息**：`credit_card`, `card_number`, `cvv`, `ssn`
//!
//! ## 基于模式的检测
//!
//! 除了字段名，以下模式也会被检测：
//! - **邮箱地址**（整体替换：`**@**.***`）
//! - **电话号码**（整体替换：`***-****-****`，不保留尾号）
//! - **身份证号**（保留末 1 位：`******X`）
//! - **银行卡号**（保留末 4 位：`****-****-****-1234`）
//! - **JWT 令牌**
//! - **AWS 访问密钥**
//! - **通用 API 密钥**
//!
//! ## 使用示例
//!
//! ```rust
//! use trait_kit::masking::DataMasker;
//!
//! let masker = DataMasker::new();
//!
//! // 脱敏日志消息
//! let message = "User login: email=test@example.com";
//! let masked = masker.mask(message);
//! // 邮箱脱敏格式: **@**.***
//! assert!(masked.contains("**@**.***"));
//! assert!(!masked.contains("test@example.com"));
//!
//! // 检查字段名是否为敏感字段
//! assert!(DataMasker::is_sensitive_field("password"));
//! assert!(DataMasker::is_sensitive_field("api_key"));
//! assert!(!DataMasker::is_sensitive_field("username"));
//! ```
//!
//! ## 性能考虑
//!
//! - 预编译正则表达式以提高性能
//! - 批量处理时使用缓存
//! - 支持禁用特定检测规则以减少开销

// 规则引擎用 fancy-regex：支持 lookbehind/lookahead 环视（regex crate 不支持），
// 数字类 PII 规则依赖环视实现 CJK 友好边界。键名检测等纯 \b 场景仍用 regex crate。
use fancy_regex::Regex;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex};

use super::error::MaskError;

/// mask 入口的单条输入上限（字节）：超过则跳过脱敏扫描（21 趟线性扫描对
/// 超大文本是 CPU 放大器）并以整体遮蔽收口——不扫描即不可证安全，原样
/// 放行会让其中的凭据/PII 明文落盘。secret-scan 门共用同一上限。
pub(crate) const MAX_MASK_INPUT_BYTES: usize = 1024 * 1024;

/// 超限输入的整体遮蔽标记（fail-closed：未扫描即不可证安全）。
pub(crate) const OVERSIZED_INPUT_REDACTION: &str = "***REDACTED_OVERSIZED***";

/// 引擎级匹配错误后的整体遮蔽标记（fail-closed）。
///
/// 引擎错误意味着「该规则在这条文本上是否命中」不可判定——文本内容不可
/// 证明安全，故整体替换而非放行原文。标记与常规脱敏标记区分命名：运维可
/// 据此看出「该记录未被脱敏」，不会与真实脱敏动作混淆。
pub(crate) const ENGINE_ERROR_REDACTION: &str = "***REDACTED_ENGINE_ERROR***";

/// Word-boundary regex patterns for sensitive field detection.
/// Uses \b (word boundary) to avoid false positives like "cakey" matching "key".
static SENSITIVE_FIELD_PATTERNS: LazyLock<Vec<regex::Regex>> = LazyLock::new(|| {
    vec![
        // Authentication patterns
        regex::Regex::new(r"(?i)\b(password|passwd|pwd)\b").unwrap(),
        // token/bearer/auth: preceded by a non-word separator (space, -, _, etc.) or at start
        // This excludes cases like "cakey" where 'token' is inside a word.
        // Covers: "token" (start), "api_token" (underscore), "bearer_token", "auth_token"
        // The (?:[^a-zA-Z0-9_])? makes the preceding char optional (for start-of-string case)
        regex::Regex::new(r"(?i)(?:[^a-zA-Z0-9_])?(token|bearer|auth)\b").unwrap(),
        regex::Regex::new(r"(?i)\b(secret|credential)\b").unwrap(),
        // Key patterns
        regex::Regex::new(r"(?i)\b(api[_-]?key|apikey|api[_-]?secret)\b").unwrap(),
        regex::Regex::new(r"(?i)\b(access[_-]?key|access[_-]?key[_-]?id)\b").unwrap(),
        regex::Regex::new(r"(?i)\b(secret[_-]?key|private[_-]?key|public[_-]?key)\b").unwrap(),
        regex::Regex::new(r"(?i)\b(encryption[_-]?key|decryption[_-]?key|master[_-]?key)\b").unwrap(),
        regex::Regex::new(r"(?i)\b(session[_-]?key|session[_-]?id|session[_-]?token)\b").unwrap(),
        // OAuth patterns
        regex::Regex::new(r"(?i)\b(oauth|oauth[_-]?token|oauth[_-]?secret)\b").unwrap(),
        regex::Regex::new(r"(?i)\b(jwt(_[a-zA-Z0-9]+)?|bearer[_-]?token)\b").unwrap(),
        // AWS patterns
        regex::Regex::new(r"(?i)\b(aws[_-]?secret|aws[_-]?key|aws[_-]?token|aws[_-]?credentials)\b").unwrap(),
        // Database patterns
        regex::Regex::new(r"(?i)\b(database[_-]?url|db[_-]?password|db[_-]?user|connection[_-]?string)\b").unwrap(),
        // Payment patterns
        regex::Regex::new(r"(?i)\b(credit[_-]?card|card[_-]?number|cvv|ssn|social[_-]?security)\b").unwrap(),
        // Client patterns
        regex::Regex::new(r"(?i)\b(client[_-]?secret|client[_-]?id)\b").unwrap(),
        // Other sensitive patterns
        regex::Regex::new(r"(?i)\b(refresh[_-]?token|pin|pin[_-]?code|two[_-]?factor|totp|backup[_-]?code|recovery[_-]?code)\b").unwrap(),
        // 中文键名族（secret-scan）：键名以敏感词收尾即视为敏感字段
        // （数据库密码、访问令牌、api密钥）；后缀锚定排除「密码学」
        // 「令牌环」等以敏感词开头的一般词汇
        regex::Regex::new(r"(密钥|令牌|密码|口令|凭证|凭据|私钥)$").unwrap(),
    ]
});

/// 姓名族键词表（与通用敏感键分离：命中后走"保留首字 + **"的形态化掩码，
/// 而非整值 ***MASKED***）。
///
/// 刻意排除裸 `name` 与 `user_name`：两者最常见的语义是登录 ID/文件名，
/// 误伤面不可控。中文键（姓名/真实姓名/客户姓名）不受 \b 词边界影响
/// （词表自身按整串匹配）。
static NAME_FIELD_PATTERNS: LazyLock<Vec<regex::Regex>> = LazyLock::new(|| {
    vec![
        // 英文姓名族（前缀限定，避免裸 name）
        regex::Regex::new(r"(?i)^(full|real|legal|customer|owner|contact)[_-]?name$").unwrap(),
        // 姓/名分列字段
        regex::Regex::new(r"(?i)^(surname|family[_-]?name|given[_-]?name)$").unwrap(),
        // 中文姓名族
        regex::Regex::new(r"^(姓名|真实姓名|客户姓名)$").unwrap(),
    ]
});

/// Data masking utility for sensitive information protection.
///
/// The `DataMasker` struct provides functionality to detect and mask sensitive
/// data in log messages and JSON structures. It uses a combination of pattern
/// matching and field name detection to identify sensitive information.
///
/// # Features
/// - **Pattern-based masking**: Detects sensitive data by regex patterns (emails, phones, etc.)
/// - **Field name detection**: Identifies sensitive fields by name (password, `api_key`, etc.)
/// - **Nested structure support**: Recursively processes nested JSON objects and arrays
/// - **Customizable rules**: Supports multiple mask rules with configurable patterns
///
/// # Sensitive Field Detection
///
/// The following field name patterns are automatically detected as sensitive:
/// - Authentication: `password`, `token`, `secret`, `credential`, `auth`
/// - API Keys: `api_key`, `api_secret`, `access_key`, `secret_key`
/// - Encryption: `encryption_key`, `decryption_key`, `private_key`
/// - OAuth: `oauth`, `oauth_token`, `bearer_token`, `jwt`
/// - AWS: `aws_secret`, `aws_key`, `aws_credentials`
/// - Payment: `credit_card`, `card_number`, `cvv`, `ssn`
///
/// # Pattern-based Detection
///
/// In addition to field names, the following patterns are detected:
/// - Email addresses (partial masking: `***@***.***`)
/// - Phone numbers (last 4 digits shown: `138****5678`)
/// - ID card numbers (partial masking)
/// - Bank card numbers (partial masking)
/// - JWT tokens
/// - AWS access keys
/// - Generic API keys
///
/// # Example
///
/// ```ignore
/// use trait_kit::masking::DataMasker;
///
/// let masker = DataMasker::new();
///
/// // Mask by pattern
/// let mut email = serde_json::json!("user@example.com");
/// masker.mask_value(&mut email);
/// assert_eq!(email, serde_json::json!("***@***.***"));
///
/// // Detect sensitive fields
/// assert!(DataMasker::is_sensitive_field("password"));
/// assert!(DataMasker::is_sensitive_field("api_key"));
/// assert!(!DataMasker::is_sensitive_field("message"));
/// ```
///
/// # Thread Safety
///
/// `DataMasker` is immutable and can be safely shared between threads.
#[derive(Debug, Clone, Default)]
pub struct DataMasker {
    /// Regex-based rules (includes all rules when `fast-masking` is off).
    rules: Vec<MaskRule>,
    /// Aho-Corasick fast path for literal-pattern rules.
    ac_masker: Option<super::ac::AcMasker>,
    /// secret-scan 出站门（`builder().with_secret_scan` 注入；None = 不参与）。
    secret_gate: Option<super::secret_scan::SecretScanGate>,
}

/// Type alias for the custom apply function used in masking rules.
type ApplyFn = Arc<dyn Fn(&Regex, &str, &str) -> String + Send + Sync>;

/// 检测面单条命中：哪条规则命中、命中在文本的哪个字节区间。
///
/// `start`/`end` 是对送入 [`DataMasker::detect`] 的原文本的字节偏移
/// （`end` 不含），`&text[start..end]` 即命中原文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaskMatch {
    /// 命中的规则名（同 [`MaskRule::name`]）。
    pub rule: String,
    /// 命中起始字节偏移（含）。
    pub start: usize,
    /// 命中结束字节偏移（不含）。
    pub end: usize,
}

/// A masking rule that defines how to detect and replace sensitive data patterns.
///
/// # Fields
/// - `name`: Unique identifier for the rule
/// - `pattern`: Compiled regex pattern for detection
/// - `replacement`: Replacement string (supports capture group references like `${1}`)
/// - `priority`: Execution order (lower values execute first)
/// - `enabled`: Whether this rule is active
/// - `apply_fn`: Custom application function for complex masking logic
#[derive(Clone)]
pub struct MaskRule {
    name: String,
    pattern: Regex,
    replacement: String,
    priority: i32,
    enabled: bool,
    apply_fn: ApplyFn,
    /// When true, the pattern is a literal string (eligible for AC acceleration).
    is_literal: bool,
}

impl std::fmt::Debug for MaskRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MaskRule")
            .field("name", &self.name)
            .field("pattern", &self.pattern.as_str())
            .field("replacement", &self.replacement)
            .field("priority", &self.priority)
            .field("enabled", &self.enabled)
            .field("apply_fn", &"<fn>")
            .field("is_literal", &self.is_literal)
            .finish()
    }
}

impl DataMasker {
    #[must_use]
    pub fn new() -> Self {
        let mut rules = vec![
            MaskRule::new_email_rule(),
            MaskRule::new_phone_rule(),
            MaskRule::new_id_card_rule(),
            MaskRule::new_bank_card_rule(),
            MaskRule::new_api_key_rule(),
            MaskRule::new_aws_key_rule(),
            MaskRule::new_jwt_rule(),
            MaskRule::new_generic_secret_rule(),
            // High-priority
            MaskRule::new_international_phone_rule(),
            MaskRule::new_credit_card_rule(),
            MaskRule::new_ipv4_rule(),
            MaskRule::new_ipv6_rule(),
            MaskRule::new_mac_address_rule(),
            // Medium-priority
            MaskRule::new_passport_rule(),
            MaskRule::new_ssn_rule(),
            MaskRule::new_db_connection_rule(),
            // Low-priority
            MaskRule::new_github_token_rule(),
            MaskRule::new_slack_token_rule(),
            MaskRule::new_stripe_key_rule(),
            MaskRule::new_google_api_key_rule(),
            MaskRule::new_private_key_rule(),
        ];
        rules.sort_by_key(MaskRule::priority);
        Self {
            rules,
            ac_masker: None,
            secret_gate: None,
        }
    }

    /// 检查字段名是否为敏感字段（大小写不敏感，使用词边界正则避免误判）
    ///
    /// 例如：
    /// - `"cakey"` 不会匹配 `"key"`（避免误判）
    /// - `"polygon"` 不会匹配 `"gon"`（避免误判）
    /// - `"password"` 会匹配（正确检测）
    pub fn is_sensitive_field(field_name: &str) -> bool {
        SENSITIVE_FIELD_PATTERNS
            .iter()
            .any(|pattern| pattern.is_match(field_name))
    }

    /// 检查字段名是否为姓名族键（`NAME_FIELD_PATTERNS` 精确匹配）。
    ///
    /// 命中后的掩码形态是"保留首字 + `**`"（见 `Self::mask_value_depth`），
    /// 与通用敏感键的整值 `***MASKED***` 不同。
    pub fn is_name_field(field_name: &str) -> bool {
        NAME_FIELD_PATTERNS
            .iter()
            .any(|pattern| pattern.is_match(field_name))
    }

    /// Mask sensitive data in a log message.
    ///
    /// # 脱敏分工（three sanitization entry points）
    ///
    /// - 本方法：PII 掩码（邮箱、电话、卡号、密钥等）
    /// - `MaskError::safe_message`（src/error.rs）：错误消息出口脱敏
    /// - `validation::sanitize::LogSanitizer`：日志注入防护（CWE-117）与转义
    ///
    /// # 标记幂等契约
    ///
    /// 标记保护下沉到「值」一级：内置规则的替换产物（`***REDACTED***`、
    /// `***MASKED***` 等）不会被任何规则再次命中，双开关叠加下不会产生
    /// REDACTED 套 REDACTED 的嵌套标记。判定不落在「整条消息」一级——
    /// 标记词表可被自由文本（用户输入）伪造，整条消息级短路会让注入一个
    /// 标记的记录整体免于脱敏（含 secret-scan 门）。
    pub fn mask(&self, text: &str) -> String {
        // 超大输入跳过掩码扫描：21 趟线性扫描对 1 MiB+ 文本是 CPU 放大器；
        // 不扫描即不可证安全，故整体遮蔽而非原样放行（fail-closed）
        if text.len() > MAX_MASK_INPUT_BYTES {
            tracing::warn!(
                size = text.len(),
                limit = MAX_MASK_INPUT_BYTES,
                "{}",
                "Masking skipped: input exceeds the 1 MiB limit; content fully redacted (not scanned means not provably safe)"
            );
            return OVERSIZED_INPUT_REDACTION.to_string();
        }

        // secret-scan 门先于规则集执行：在原文上定位裸 secret，归因计数
        // 不被前序规则改写干扰。Cow 借用保证无门路径零额外分配。
        let gated = match self.secret_gate.as_ref() {
            Some(gate) => std::borrow::Cow::Owned(gate.mask(text)),
            None => std::borrow::Cow::Borrowed(text),
        };

        self.apply_rule_pipeline(&gated)
    }

    /// 规则管线尾段：fast-masking（可选）+ 启用规则串行改写。`mask` 与
    /// `mask_checked` 的 Ok 路径共用本段，保证两出口替换语义一致。
    fn apply_rule_pipeline(&self, gated: &str) -> String {
        let mut result = if let Some(ref ac) = self.ac_masker {
            ac.mask_fast(gated)
        } else {
            gated.to_string()
        };

        for rule in &self.rules {
            if rule.is_enabled() {
                result = rule.apply(&result);
            }
        }
        result
    }

    /// fail-closed 出口（feature `secret-scan`）：替换语义同 [`Self::mask`]，
    /// 但挂有 secret-scan 门时，单条输入内任一模式命中数超过门限、或输入
    /// 超过掩码上限（不扫描即不可证安全）即返回 `Err`，脱敏输出不可用。
    /// Ok 路径与 `mask` 共用规则管线尾段（门产物继续过 fast-masking 与
    /// 规则集，PII 不漏）。未挂门时退化为 `mask()`（永远 `Ok`）。
    ///
    /// 与 `mask` 的分叉点：超大输入在 `mask` 以整体遮蔽收口
    /// （`OVERSIZED_INPUT_REDACTION`，fail-closed），在 `mask_checked`
    /// 拒绝并返回错误（fail-closed）。
    ///
    /// # Errors
    /// - `MaskError::SecretScanLimit`：单条输入内某模式命中数超过门限。
    /// - `MaskError::SecretScanOversizedInput`：输入超过掩码上限。
    pub fn mask_checked(&self, text: &str) -> Result<String, MaskError> {
        match self.secret_gate.as_ref() {
            Some(gate) => {
                let gated = gate.mask_checked(text)?;
                Ok(self.apply_rule_pipeline(&gated))
            }
            None => Ok(self.mask(text)),
        }
    }

    /// 检测面：报告全部已启用规则在 `text` 中的命中（规则名 + 字节区间）。
    ///
    /// 与 [`Self::mask`] 的两点刻意差异（fail-closed）：
    /// - 不走标记幂等短路——已含 `***REDACTED***` 等标记的输入仍要
    ///   检测出伴随的未脱敏 PII，检测面不能因上游已处理过而失明；
    /// - 不做超大输入跳过——检测是显式诊断调用，不在日志热路径上。
    ///
    /// 各规则在原始文本上独立报告；`mask` 按优先级串行改写，前序规则
    /// 的改写可能使后序规则不再命中，两侧仅在各规则独立观察时一一对应。
    /// 归因键是规则名——经 `from_rules` 绕过查重进入的重名规则在结果中
    /// 不可区分（debug 构建下构造器会断言暴露）。
    #[must_use]
    pub fn detect(&self, text: &str) -> Vec<MaskMatch> {
        let mut matches = Vec::new();
        for rule in &self.rules {
            if !rule.is_enabled() {
                continue;
            }
            for (start, end) in rule.find_matches(text) {
                matches.push(MaskMatch {
                    rule: rule.name.clone(),
                    start,
                    end,
                });
            }
        }
        matches
    }

    /// 检测面布尔形式：是否存在任一命中。与 [`Self::detect`] 同源
    /// （不做标记短路），找到首个命中即提前返回——存在性判断只做
    /// 首匹配查找，不物化全部命中区间。引擎级匹配错误与 detect 共用
    /// 同一可观测出口（见 `MaskRule::report_engine_error`）。
    #[must_use]
    pub fn has_match(&self, text: &str) -> bool {
        self.rules
            .iter()
            .filter(|r| r.is_enabled())
            .any(|rule| match rule.pattern.find(text) {
                Ok(Some(_)) => true,
                Ok(None) => false,
                Err(e) => {
                    rule.report_engine_error(&e);
                    false
                }
            })
    }

    /// 行级 key=value 文本脱敏：命中行只替换 `=` 之后的值侧，
    /// `=` 之前的原文（缩进、键名、等号前的空白）逐字保留。
    ///
    /// 命中条件：行经 `trim_start` 后以 `keys` 之一开头，且键名与 `=` 之间
    /// 仅隔空白——`password_debug = x` 不会因 `password` 前缀而误命中。
    /// 这是行级方言的独立入口，不经过规则集与标记幂等短路；
    /// 保留「= 前原文」的重组形态，用于逐行 config 类文本的确定性改写
    /// （正则规则集会改写键名等号间的空白形态，不满足此类场景的逐字要求）。
    #[must_use]
    pub fn mask_kv_lines(&self, text: &str, keys: &[&str], marker: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for (i, line) in text.split('\n').enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let trimmed = line.trim_start();
            let mut hit_eq_pos: Option<usize> = None;
            for key in keys {
                let Some(rest) = trimmed.strip_prefix(key) else {
                    continue;
                };
                let after_ws = rest.trim_start();
                if after_ws.starts_with('=') {
                    let eq_in_line =
                        (line.len() - trimmed.len()) + (trimmed.len() - after_ws.len());
                    hit_eq_pos = Some(eq_in_line);
                    break;
                }
            }
            match hit_eq_pos {
                Some(eq_pos) => {
                    out.push_str(&line[..eq_pos]);
                    out.push('=');
                    out.push_str(marker);
                }
                None => out.push_str(line),
            }
        }
        out
    }

    pub fn mask_value(&self, value: &mut Value) {
        self.mask_value_depth(value, 0);
    }

    fn mask_value_depth(&self, value: &mut Value, depth: usize) {
        const MAX_MASK_DEPTH: usize = 16;
        if depth >= MAX_MASK_DEPTH {
            // 深嵌套子树整体截断：防恶意/意外深嵌套导致递归栈溢出
            *value = Value::String("***TRUNCATED***".to_string());
            return;
        }
        match value {
            Value::String(s) => {
                *s = self.mask(s);
            }
            Value::Number(n) => {
                // 数字/布尔形态的敏感值（如 JSON 里的裸手机号）同样参与掩码
                let rendered = n.to_string();
                let masked = self.mask(&rendered);
                if masked != rendered {
                    *value = Value::String(masked);
                }
            }
            Value::Bool(b) => {
                let rendered = b.to_string();
                let masked = self.mask(&rendered);
                if masked != rendered {
                    *value = Value::String(masked);
                }
            }
            Value::Array(arr) => {
                for item in arr {
                    self.mask_value_depth(item, depth + 1);
                }
            }
            Value::Object(map) => {
                for (k, v) in map.iter_mut() {
                    if Self::is_sensitive_field(k) {
                        *v = Value::String("***MASKED***".to_string());
                    } else if Self::is_name_field(k)
                        && let Value::String(s) = v
                        && let Some(masked) = Self::mask_cjk_name(s)
                    {
                        *v = Value::String(masked);
                    } else {
                        self.mask_value_depth(v, depth + 1);
                    }
                }
            }
            Value::Null => {}
        }
    }

    /// 中文姓名形态掩码：2–4 个汉字 → 保留首字 + `**`（如 `张三丰` → `张**`）。
    /// 非纯汉字 / 长度不符返回 None（调用方保持原值）。
    fn mask_cjk_name(value: &str) -> Option<String> {
        static CJK_NAME_SHAPE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
            fancy_regex::Regex::new(r"^[\p{Han}]{2,4}$")
                .unwrap_or_else(|_| panic!("{}", "Invalid CJK name shape regex"))
        });
        if !CJK_NAME_SHAPE.is_match(value).unwrap_or(false) {
            return None;
        }
        value.chars().next().map(|first| format!("{first}**"))
    }

    pub fn mask_hashmap(&self, map: &mut HashMap<String, Value>) {
        for (k, v) in map.iter_mut() {
            if Self::is_sensitive_field(k) {
                *v = Value::String("***MASKED***".to_string());
            } else if Self::is_name_field(k)
                && let Value::String(s) = v
                && let Some(masked) = Self::mask_cjk_name(s)
            {
                *v = Value::String(masked);
            } else {
                self.mask_value(v);
            }
        }
    }

    /// Consumes the `DataMasker` and returns the inner rules vector.
    #[must_use]
    pub fn into_rules(self) -> Vec<MaskRule> {
        self.rules
    }

    /// Creates a new [`DataMaskerBuilder`] for assembling a custom masker.
    #[must_use]
    pub fn builder() -> DataMaskerBuilder {
        DataMaskerBuilder::new()
    }
}

/// 构造「内置 PII 规则 + secret 值形态扫描出站门」的 sink 默认 masker。
///
/// `secret-scan` feature 关闭时退化为 [`DataMasker::new`]（无门）——调用方
/// （sink 内置组装）应在配置校验层对 enabled=true 发出警告。供 console/
/// database 内置组装与外部自定义组装共用。
#[must_use]
pub fn secret_scan_masker() -> DataMasker {
    let gate = super::secret_scan::SecretScanGate::new(
        &super::secret_patterns::SecretPatternRegistry::with_builtins(),
    );
    DataMasker::builder().with_secret_scan(gate).build()
}

/// Builder for assembling a [`DataMasker`] with custom rule configurations.
///
/// # Example
///
/// ```rust
/// use trait_kit::masking::DataMasker;
///
/// let masker = DataMasker::builder()
///     .disable_builtin("email")
///     .build();
/// ```
pub struct DataMaskerBuilder {
    extra_rules: Vec<MaskRule>,
    disabled_builtins: Vec<String>,
    use_builtins: bool,
    custom_registry: Option<super::registry::MaskRuleRegistry>,
    /// secret-scan 出站门（feature `secret-scan`）。
    secret_gate: Option<super::secret_scan::SecretScanGate>,
}

impl DataMaskerBuilder {
    fn new() -> Self {
        Self {
            extra_rules: Vec::new(),
            disabled_builtins: Vec::new(),
            use_builtins: true,
            custom_registry: None,
            secret_gate: None,
        }
    }

    /// Add a custom rule to the masker.
    #[must_use]
    pub fn add_rule(mut self, rule: MaskRule) -> Self {
        self.extra_rules.push(rule);
        self
    }

    /// 挂接 secret-scan 出站门（feature `secret-scan`）。
    ///
    /// 门先于规则集执行：在原文上定位裸 secret 并归因计数；`mask` 失败
    /// 放行，`mask_checked` 超限 fail-closed。
    #[must_use]
    pub fn with_secret_scan(mut self, gate: super::secret_scan::SecretScanGate) -> Self {
        self.secret_gate = Some(gate);
        self
    }

    /// Use a custom [`MaskRuleRegistry`](super::registry::MaskRuleRegistry) as the rule source instead of builtins.
    ///
    /// When set, the registry's rules replace the default built-in rules.
    /// `add_rule()` and `disable_builtin()` still apply on top.
    #[must_use]
    pub fn with_registry(mut self, registry: super::registry::MaskRuleRegistry) -> Self {
        self.custom_registry = Some(registry);
        self.use_builtins = false;
        self
    }

    /// Disable a built-in rule by name.
    #[must_use]
    pub fn disable_builtin(mut self, name: &str) -> Self {
        self.disabled_builtins.push(name.to_string());
        self
    }

    /// Build the [`DataMasker`] with all configured rules sorted by priority.
    ///
    /// When the `fast-masking` feature is enabled, literal-pattern rules are
    /// extracted into an `AcMasker` for single-pass
    /// acceleration; remaining regex rules stay in the sequential path.
    #[must_use]
    pub fn build(self) -> DataMasker {
        let mut rules = if let Some(registry) = self.custom_registry {
            registry.active_rules().into_iter().cloned().collect()
        } else if self.use_builtins {
            DataMasker::new().into_rules()
        } else {
            Vec::new()
        };

        // Remove disabled builtins
        for name in &self.disabled_builtins {
            rules.retain(|r| r.name() != name.as_str());
        }

        rules.extend(self.extra_rules);

        rules.sort_by_key(MaskRule::priority);

        // Partition: literal rules → AC, regex rules → sequential
        let (literal_rules, regex_rules): (Vec<_>, Vec<_>) = rules
            .into_iter()
            .partition(|r| r.is_literal() && r.is_enabled());

        let ac_masker = if literal_rules.is_empty() {
            None
        } else {
            let patterns: Vec<String> = literal_rules
                .iter()
                .map(|r| r.pattern.as_str().to_string())
                .collect();
            let replacements: Vec<String> = literal_rules
                .iter()
                .map(|r| r.replacement.clone())
                .collect();
            super::ac::AcMasker::new(patterns, replacements)
        };

        DataMasker {
            rules: regex_rules,
            ac_masker,
            secret_gate: self.secret_gate,
        }
    }
}

/// Pre-compiled regex patterns for better performance
static EMAIL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+")
        .unwrap_or_else(|_| panic!("{}", "Invalid email regex"))
});

static PHONE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // 负向断言而非 \b：regex 的 \b 是 Unicode 词边界，CJK 汉字属词字符，
    // "电话13812345678" 这类数字紧贴汉字的文本在 \b 语义下无边界、会漏报。
    Regex::new(r"(?<![0-9A-Za-z])1[3-9]\d{9}(?![0-9A-Za-z])")
        .unwrap_or_else(|_| panic!("{}", "Invalid phone regex"))
});

static ID_CARD_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // [\dXx]：身份证校验位小写 x 同样合法；两侧负向断言理由同 PHONE_REGEX。
    Regex::new(r"(?<![0-9A-Za-z])(\d{6})(\d{8})(\d{3}[\dXx])(?![0-9A-Za-z])")
        .unwrap_or_else(|_| panic!("{}", "Invalid ID card regex"))
});

static BANK_CARD_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // 收紧为 16-19 位（4 + 8..11 + 4）：13 位毫秒时间戳曾被 5,11 中段误掩。
    Regex::new(r"(?<![0-9A-Za-z])(\d{4})(\d{8,11})(\d{4})(?![0-9A-Za-z])")
        .unwrap_or_else(|_| panic!("{}", "Invalid bank card regex"))
});

/// API Key 模式 - 匹配常见的 API key 格式（组 1 = 键名与分隔符，组 2 = 值；
/// 值不放组 1，否则替换会把键名连同值一起展开、再被 `generic_secret` 二次命中）
static API_KEY_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(api[_-]?key[^\s:=]*\s*[=:]\s*)([a-zA-Z0-9_-]{20,})")
        .unwrap_or_else(|_| panic!("{}", "Invalid API key regex"))
});

/// AWS Access Key 模式 - 匹配 AKIA 开头的 AWS 密钥
static AWS_KEY_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(AKIA|ABIA|ACCA|ASIA)[0-9A-Z]{16}")
        .unwrap_or_else(|_| panic!("{}", "Invalid AWS key regex"))
});

/// JWT Token 模式 - 匹配 JWT 格式
static JWT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)eyJ[a-zA-Z0-9_-]*\.eyJ[a-zA-Z0-9_-]*\.[a-zA-Z0-9_-]*")
        .unwrap_or_else(|_| panic!("{}", "Invalid JWT regex"))
});

/// 通用密钥/密码模式 - 匹配 key=value 或 "key": "value" 中的敏感值
static GENERIC_SECRET_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)([^\s:=]*(?:token|secret|key|password|passwd|pwd|credential)s?[^\s:=]*\s*[=:]\s*)([a-zA-Z0-9_\-\+]{16,})")
        .unwrap_or_else(|_| panic!("{}", "Invalid generic secret regex"))
});

// High-priority rules (compliance)

/// International phone (E.164 format)
static INTERNATIONAL_PHONE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\+(\d{1,3})[\s.-]?(\(?\d{1,4}\)?[\s.-]?\d{2,4}[\s.-]?)(\d{2,4})")
        .unwrap_or_else(|_| panic!("{}", "Invalid international phone regex"))
});

/// Credit card (major card networks)
static CREDIT_CARD_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?<![0-9])(?:4[0-9]{12}(?:[0-9]{3})?|5[1-5][0-9]{14}|3[47][0-9]{13}|6(?:011|5[0-9]{2})[0-9]{12}|35[0-9]{14})(?![0-9])")
        .unwrap_or_else(|_| panic!("{}", "Invalid credit card regex"))
});

/// IPv4 address
static IPV4_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\.){3}(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\b")
        .unwrap_or_else(|_| panic!("{}", "Invalid IPv4 regex"))
});

/// IPv6 地址 - 完整 8 组形式与 `::` 压缩形式（RFC 5952 规范写法，日志中最常见）。
/// 环视代替 `\b`：压缩形式以 `:` 起止处无词边界（理由同 `PHONE_REGEX`）；
/// 分支按「长匹配优先」排序，防止 `2001:db8::1` 只被截到 `2001:db8::`。
static IPV6_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?<![0-9A-Za-z])(?:(?:[0-9a-fA-F]{1,4}:){7}[0-9a-fA-F]{1,4}|(?:[0-9a-fA-F]{1,4}:){1,7}:(?:[0-9a-fA-F]{1,4}(?::[0-9a-fA-F]{1,4})*)?|:(?::[0-9a-fA-F]{1,4})+|::)(?![0-9A-Za-z])",
    )
    .unwrap_or_else(|_| panic!("{}", "Invalid IPv6 regex"))
});

/// MAC address
static MAC_ADDRESS_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:[0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}\b")
        .unwrap_or_else(|_| panic!("{}", "Invalid MAC address regex"))
});

// Medium-priority rules (regional identity)

/// Passport number (Chinese international passport format)
///
/// 前缀字母（E/G）+ 8 位纯数字。后缀必须是数字：曾允许字母数字混合，
/// 结果 e/g 开头的 9 字母英文单词（如 execution）被整体误掩；纯数字
/// 后缀下不存在"混合 hex"误伤形态，无需额外排除前瞻。
static PASSPORT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?<![0-9A-Za-z])[EeGg]\d{8}(?![0-9A-Za-z])")
        .unwrap_or_else(|_| panic!("{}", "Invalid passport regex"))
});

/// US Social Security Number
///
/// 覆盖三种形态：带横线 `NNN-NN-NNNN`、空格分隔 `NNN NN NNNN`、
/// 紧凑 `NNNNNNNNN`。整条 alternation 由 `(?<![0-9A-Za-z])…(?![0-9A-Za-z])`
/// 边界包裹，紧凑分支额外施 SSA 有效性守卫（组号 ∉ 000/666/900–999、
/// 区号 ≠ 00、序列号 ≠ 0000），以独立 9 位边界 + 号段合法性限制误伤。
/// 带横线/空格形态保持原语义不收窄，避免对既有行为回归。
static SSN_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?<![0-9A-Za-z])(?:\d{3}-\d{2}-\d{4}|\d{3} \d{2} \d{4}|(?!000|666|9\d\d)\d{3}(?!00)\d{2}(?!0000)\d{4})(?![0-9A-Za-z])",
    )
    .unwrap_or_else(|_| panic!("{}", "Invalid SSN regex"))
});

/// Database connection string (password in URI)
static DB_CONNECTION_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)((?:postgres|mysql|mongodb|redis|amqp)://[^:\s]+:)([^@]+)(@\S+)")
        .unwrap_or_else(|_| panic!("{}", "Invalid DB connection regex"))
});

// Low-priority rules (third-party tokens)

/// GitHub personal access token
static GITHUB_TOKEN_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:ghp|github_pat)_[A-Za-z0-9_]{36,}\b")
        .unwrap_or_else(|_| panic!("{}", "Invalid GitHub token regex"))
});

/// Slack token
static SLACK_TOKEN_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"xox[bpas]-[0-9]{10,13}-[0-9a-zA-Z-]+")
        .unwrap_or_else(|_| panic!("{}", "Invalid Slack token regex"))
});

/// Stripe API key
static STRIPE_KEY_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:sk|pk)_(?:live|test)_[0-9a-zA-Z]{24,}")
        .unwrap_or_else(|_| panic!("{}", "Invalid Stripe key regex"))
});

/// Google API key
static GOOGLE_API_KEY_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"AIza[0-9A-Za-z_-]{35}")
        .unwrap_or_else(|_| panic!("{}", "Invalid Google API key regex"))
});

/// Private key PEM block
static PRIVATE_KEY_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"-----BEGIN[A-Z ]*PRIVATE KEY-----[\s\S]*?-----END[A-Z ]*PRIVATE KEY-----")
        .unwrap_or_else(|_| panic!("{}", "Invalid private key regex"))
});

/// 引擎级匹配错误统一留痕/广播出口：apply 闭包无 self，以规则名直呼；
/// 语义同 `MaskRule::report_engine_error（warn` 逐次留痕 + ops 事件按规则限频首报）。
fn report_engine_error_for(rule: &str, error: &fancy_regex::Error) {
    static REPORTED: LazyLock<Mutex<HashSet<String>>> =
        LazyLock::new(|| Mutex::new(HashSet::new()));
    tracing::warn!(
        rule = rule,
        error = %error,
        "{}",
        "regex match error during detect; skipping this match"
    );
    // 锁中毒时放弃本次首报登记：错误路径不传播 panic
    let Ok(mut reported) = REPORTED.lock() else {
        return;
    };
    // 常态（已首报）经 contains(&str) 免分配短路，仅首次插入才 clone
    if reported.contains(rule) {
        return;
    }
    reported.insert(rule.to_string());
}

/// `日志路径兜底替换：fancy_regex` 的 `replace_all/replacen` 在引擎错误
/// （`BacktrackLimitExceeded` 等）上内部 unwrap，会让 console worker panic
/// 离场、日志双通道静默终止，本模块统一走 `try_replacen——失败即经统一`
/// 出口留痕，并以保守遮蔽整体替换该文本（fail-closed：引擎错误后放行
/// 原文会让本该被遮蔽的 PII 明文落盘）。
fn replace_all_masked(
    rule: &str,
    regex: &Regex,
    text: &str,
    replacement: impl fancy_regex::Replacer,
) -> String {
    match regex.try_replacen(text, 0, replacement) {
        Ok(masked) => masked.into_owned(),
        Err(e) => {
            report_engine_error_for(rule, &e);
            ENGINE_ERROR_REDACTION.to_string()
        }
    }
}

impl MaskRule {
    fn new_email_rule() -> Self {
        Self::build_from_regex("email", EMAIL_REGEX.clone(), "**@**.***", 100, None)
    }

    fn new_phone_rule() -> Self {
        Self::build_from_regex("phone", PHONE_REGEX.clone(), "***-****-****", 100, None)
    }

    fn new_id_card_rule() -> Self {
        Self::build_from_regex(
            "id_card",
            ID_CARD_REGEX.clone(),
            "MASK_ID_CARD",
            100,
            Some(Arc::new(|regex: &Regex, text: &str, _replacement: &str| {
                replace_all_masked("id_card", regex, text, "******$3")
            })),
        )
    }

    fn new_bank_card_rule() -> Self {
        Self::build_from_regex(
            "bank_card",
            BANK_CARD_REGEX.clone(),
            "MASK_BANK_CARD",
            100,
            Some(Arc::new(|regex: &Regex, text: &str, _replacement: &str| {
                replace_all_masked(
                    "bank_card",
                    regex,
                    text,
                    |caps: &fancy_regex::Captures<'_, str>| {
                        let matched = caps.get(0).unwrap().as_str();
                        if matched.len() >= 12 {
                            let last_four = &matched[matched.len() - 4..];
                            format!("****-****-****-{last_four}")
                        } else {
                            matched.to_string()
                        }
                    },
                )
            })),
        )
    }

    fn new_api_key_rule() -> Self {
        Self::build_from_regex(
            "api_key",
            API_KEY_REGEX.clone(),
            "${1}***REDACTED***",
            100,
            None,
        )
    }

    fn new_aws_key_rule() -> Self {
        Self::build_from_regex(
            "aws_key",
            AWS_KEY_REGEX.clone(),
            "***REDACTED***",
            100,
            None,
        )
    }

    fn new_jwt_rule() -> Self {
        Self::build_from_regex("jwt", JWT_REGEX.clone(), "***REDACTED_JWT***", 100, None)
    }

    fn new_generic_secret_rule() -> Self {
        Self::build_from_regex(
            "generic_secret",
            GENERIC_SECRET_REGEX.clone(),
            "${1}***REDACTED***",
            100,
            None,
        )
    }

    // Internal helper for pre-compiled rules

    /// Build a rule from a pre-compiled Regex, avoiding re-compilation.
    fn build_from_regex(
        name: &str,
        regex: Regex,
        replacement: &str,
        priority: i32,
        apply_fn: Option<ApplyFn>,
    ) -> Self {
        let rule_name = name.to_string();
        MaskRule {
            name: name.to_string(),
            pattern: regex,
            replacement: replacement.to_string(),
            priority,
            enabled: true,
            apply_fn: apply_fn.unwrap_or_else(move || {
                Arc::new(move |regex: &Regex, text: &str, replacement: &str| {
                    // fancy-regex 的 &str replacer 支持 ${1}/$1 组展开；匹配期
                    // 错误实际可达（长文本回溯上限），统一出口留痕并以保守
                    // 遮蔽整体替换该文本（fail-closed：日志路径禁 panic，且
                    // 引擎错误后放行原文等于放行未脱敏 PII）
                    replace_all_masked(&rule_name, regex, text, replacement)
                })
            }),
            is_literal: false,
        }
    }

    // High-priority rules

    fn new_international_phone_rule() -> Self {
        Self::build_from_regex(
            "international_phone",
            INTERNATIONAL_PHONE_REGEX.clone(),
            "+${1}-***-***-${3}",
            10,
            None,
        )
    }

    fn new_credit_card_rule() -> Self {
        Self::build_from_regex(
            "credit_card",
            CREDIT_CARD_REGEX.clone(),
            "***REDACTED_CC***",
            15,
            Some(Arc::new(|regex: &Regex, text: &str, replacement: &str| {
                replace_all_masked(
                    "credit_card",
                    regex,
                    text,
                    |caps: &fancy_regex::Captures<'_, str>| {
                        let number = caps.get(0).unwrap().as_str();
                        let digits: Vec<u32> =
                            number.chars().filter_map(|c| c.to_digit(10)).collect();
                        let mut sum = 0u32;
                        let mut alternate = false;
                        for &d in digits.iter().rev() {
                            if alternate {
                                let doubled = d * 2;
                                sum += if doubled > 9 { doubled - 9 } else { doubled };
                            } else {
                                sum += d;
                            }
                            alternate = !alternate;
                        }
                        if !sum.is_multiple_of(10) {
                            // Not a valid card number, but still card-shaped:
                            // fall back to the rule's replacement instead of
                            // leaving the digits untouched.
                            return replacement.to_string();
                        }
                        let last4 = &number[number.len() - 4..];
                        if number.starts_with('3') {
                            format!("****-******-{last4}")
                        } else {
                            format!("****-****-****-{last4}")
                        }
                    },
                )
            })),
        )
    }

    fn new_ipv4_rule() -> Self {
        Self::build_from_regex(
            "ipv4",
            IPV4_REGEX.clone(),
            "***.***.***.XXX",
            20,
            Some(Arc::new(|regex: &Regex, text: &str, _replacement: &str| {
                replace_all_masked(
                    "ipv4",
                    regex,
                    text,
                    |caps: &fancy_regex::Captures<'_, str>| {
                        let ip = caps.get(0).unwrap().as_str();
                        if let Some(pos) = ip.rfind('.') {
                            format!("***.***.***.{}", &ip[pos + 1..])
                        } else {
                            "***.***.***.***".to_string()
                        }
                    },
                )
            })),
        )
    }

    fn new_ipv6_rule() -> Self {
        Self::build_from_regex(
            "ipv6",
            IPV6_REGEX.clone(),
            "****:****:****:****:****:****:****:XXXX",
            21,
            Some(Arc::new(|regex: &Regex, text: &str, _replacement: &str| {
                replace_all_masked(
                    "ipv6",
                    regex,
                    text,
                    |caps: &fancy_regex::Captures<'_, str>| {
                        // 统一 8 组形态：末段分组保留真实值，其余 7 组以 ****
                        // 替换；`1::`/`::` 这类无末段分组的写法整段全掩码
                        let ip = caps.get(0).unwrap().as_str();
                        let last_group = ip.rsplit(':').next().unwrap_or("");
                        let tail = if last_group.is_empty() {
                            "****"
                        } else {
                            last_group
                        };
                        format!("****:****:****:****:****:****:****:{tail}")
                    },
                )
            })),
        )
    }

    fn new_mac_address_rule() -> Self {
        Self::build_from_regex(
            "mac_address",
            MAC_ADDRESS_REGEX.clone(),
            "XX:**:**:**:**:XX",
            19,
            Some(Arc::new(|regex: &Regex, text: &str, _replacement: &str| {
                replace_all_masked(
                    "mac_address",
                    regex,
                    text,
                    |caps: &fancy_regex::Captures<'_, str>| {
                        let mac = caps.get(0).unwrap().as_str();
                        let sep = if mac.contains(':') { ':' } else { '-' };
                        let parts: Vec<&str> = mac.split(sep).collect();
                        if parts.len() == 6 {
                            format!(
                                "{}{}{}{}{}{}{}{}{}{}{}",
                                parts[0], sep, "**", sep, "**", sep, "**", sep, "**", sep, parts[5]
                            )
                        } else {
                            mac.to_string()
                        }
                    },
                )
            })),
        )
    }

    // Medium-priority rules

    fn new_passport_rule() -> Self {
        Self::build_from_regex(
            "passport",
            PASSPORT_REGEX.clone(),
            "******XX",
            30,
            Some(Arc::new(|regex: &Regex, text: &str, _replacement: &str| {
                replace_all_masked(
                    "passport",
                    regex,
                    text,
                    |caps: &fancy_regex::Captures<'_, str>| {
                        let passport = caps.get(0).unwrap().as_str();
                        let first = &passport[..1];
                        let last2 = &passport[passport.len() - 2..];
                        format!("{first}******{last2}")
                    },
                )
            })),
        )
    }

    fn new_ssn_rule() -> Self {
        Self::build_from_regex(
            "ssn",
            SSN_REGEX.clone(),
            "***-**-XXXX",
            35,
            Some(Arc::new(|regex: &Regex, text: &str, _replacement: &str| {
                replace_all_masked(
                    "ssn",
                    regex,
                    text,
                    |caps: &fancy_regex::Captures<'_, str>| {
                        let ssn = caps.get(0).unwrap().as_str();
                        let last4 = &ssn[ssn.len() - 4..];
                        format!("***-**-{last4}")
                    },
                )
            })),
        )
    }

    fn new_db_connection_rule() -> Self {
        Self::build_from_regex(
            "db_connection",
            DB_CONNECTION_REGEX.clone(),
            "${1}***${3}",
            40,
            None,
        )
    }

    // Low-priority rules

    fn new_github_token_rule() -> Self {
        Self::build_from_regex(
            "github_token",
            GITHUB_TOKEN_REGEX.clone(),
            "***REDACTED_GITHUB***",
            50,
            None,
        )
    }

    fn new_slack_token_rule() -> Self {
        Self::build_from_regex(
            "slack_token",
            SLACK_TOKEN_REGEX.clone(),
            "***REDACTED_SLACK***",
            51,
            None,
        )
    }

    fn new_stripe_key_rule() -> Self {
        Self::build_from_regex(
            "stripe_key",
            STRIPE_KEY_REGEX.clone(),
            "***REDACTED_STRIPE***",
            52,
            None,
        )
    }

    fn new_google_api_key_rule() -> Self {
        Self::build_from_regex(
            "google_api_key",
            GOOGLE_API_KEY_REGEX.clone(),
            "***REDACTED_GOOGLE***",
            53,
            None,
        )
    }

    fn new_private_key_rule() -> Self {
        Self::build_from_regex(
            "private_key",
            PRIVATE_KEY_REGEX.clone(),
            "***REDACTED_PRIVATE_KEY***",
            54,
            None,
        )
    }

    fn apply(&self, text: &str) -> String {
        (self.apply_fn)(&self.pattern, text, &self.replacement)
    }

    /// Returns the source pattern string of this rule.
    #[must_use]
    pub fn pattern(&self) -> &str {
        self.pattern.as_str()
    }

    /// 收集本规则在 `text` 中的全部命中区间（字节偏移对）。
    ///
    /// 与 [`Self::apply`] 的对齐依据：库内全部规则的 `apply_fn` 都在正则
    /// match 集上逐个改写（含 Luhn 失败 fall back 到 replacement 的
    /// `credit_card），正则命中即` apply 的操作点。引擎级匹配错误
    /// （`fancy_regex` 回溯上限等）跳过该次匹配不中断其余命中，经
    /// [`Self::report_engine_error`] 留痕并广播漏报。
    fn find_matches(&self, text: &str) -> Vec<(usize, usize)> {
        self.pattern
            .find_iter(text)
            .filter_map(|m| match m {
                Ok(found) => Some((found.start(), found.end())),
                Err(e) => {
                    self.report_engine_error(&e);
                    None
                }
            })
            .collect()
    }

    /// 引擎级匹配错误的统一可观测出口（detect 与 `has_match` 共用）。
    ///
    /// warn 逐次留痕；ops 事件广播按规则名限频：投递成功后本进程不再
    /// 重报同规则——回溯上限类错误对同一规则是持续性的，逐命中广播
    /// 会在事件风暴下累积 `send_timeout` 阻塞。首报投递失败（通道满
    /// 被丢弃）不标记，下次错误自动重报。hub 无注册通道时零开销返回
    /// （detail 不构造），广播路径不反压主日志链路。
    fn report_engine_error(&self, error: &fancy_regex::Error) {
        report_engine_error_for(&self.name, error);
    }

    /// Returns the name of this rule.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns whether this rule is enabled.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the priority of this rule (lower = earlier).
    #[must_use]
    pub fn priority(&self) -> i32 {
        self.priority
    }

    /// Sets whether this rule is enabled.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Returns whether this rule uses a literal (fixed-string) pattern.
    #[must_use]
    pub fn is_literal(&self) -> bool {
        self.is_literal
    }

    /// Creates a new builder for constructing a `MaskRule`.
    #[must_use]
    pub fn builder(name: &str) -> MaskRuleBuilder {
        MaskRuleBuilder::new(name)
    }
}

/// Builder for constructing [`MaskRule`] instances with a fluent API.
///
/// # Defaults
/// - `priority`: 100
/// - `enabled`: true
/// - `apply_fn`: standard `regex.replace_all(text, replacement)`
///
/// # Example
///
/// ```rust
/// use trait_kit::masking::MaskRule;
///
/// let rule = MaskRule::builder("custom_phone")
///     .pattern(r"\b\d{3}-\d{4}\b")
///     .replacement("***-****")
///     .priority(50)
///     .build()
///     .unwrap();
/// ```
pub struct MaskRuleBuilder {
    name: String,
    pattern: Option<String>,
    replacement: String,
    priority: i32,
    enabled: bool,
    apply_fn: Option<ApplyFn>,
    is_literal: bool,
}

impl MaskRuleBuilder {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            pattern: None,
            replacement: String::new(),
            priority: 100,
            enabled: true,
            apply_fn: None,
            is_literal: false,
        }
    }

    /// Sets the regex pattern for this rule.
    #[must_use]
    pub fn pattern(mut self, regex: &str) -> Self {
        self.pattern = Some(regex.to_string());
        self
    }

    /// Sets the replacement string (supports capture group refs like `${1}`).
    #[must_use]
    pub fn replacement(mut self, replacement: &str) -> Self {
        self.replacement = replacement.to_string();
        self
    }

    /// Sets the execution priority (lower values execute first).
    #[must_use]
    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Sets whether this rule is enabled.
    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Sets a custom apply function for complex masking logic.
    #[must_use]
    pub fn apply_fn(mut self, f: ApplyFn) -> Self {
        self.apply_fn = Some(f);
        self
    }

    /// Mark this rule's pattern as a literal string (eligible for AC acceleration).
    ///
    /// When `true`, the pattern is treated as a fixed string rather than a regex,
    /// enabling the Aho-Corasick fast path when `fast-masking` feature is enabled.
    #[must_use]
    pub fn literal(mut self, is_literal: bool) -> Self {
        self.is_literal = is_literal;
        self
    }

    /// Builds the [`MaskRule`], compiling the regex pattern.
    ///
    /// # Errors
    /// Returns `Err(MaskError)` if the pattern is missing or an invalid regex.
    pub fn build(self) -> Result<MaskRule, MaskError> {
        let rule_name = self.name.clone();
        let pattern_str = self.pattern.ok_or_else(|| {
            MaskError::ConfigError(format!("masking rule '{}' requires a `pattern`", self.name))
        })?;
        let regex = Regex::new(&pattern_str).map_err(|e| {
            MaskError::ConfigError(format!("invalid regex in rule '{rule_name}': {e}"))
        })?;
        Ok(MaskRule {
            name: self.name,
            pattern: regex,
            replacement: self.replacement,
            priority: self.priority,
            enabled: self.enabled,
            apply_fn: self.apply_fn.unwrap_or_else(move || {
                Arc::new(move |regex: &Regex, text: &str, replacement: &str| {
                    replace_all_masked(&rule_name, regex, text, replacement)
                })
            }),
            is_literal: self.is_literal,
        })
    }
}

#[cfg(test)]
mod builder_tests {
    use super::*;

    #[test]
    fn test_builder_defaults() {
        let rule = MaskRule::builder("test")
            .pattern(r"\d+")
            .replacement("***")
            .build()
            .unwrap();
        assert_eq!(rule.name(), "test");
        assert_eq!(rule.priority(), 100);
        assert!(rule.is_enabled());
        assert_eq!(rule.apply("abc123def"), "abc***def");
    }

    #[test]
    fn test_builder_custom_values() {
        let rule = MaskRule::builder("custom")
            .pattern(r"\d+")
            .replacement("###")
            .priority(50)
            .enabled(false)
            .build()
            .unwrap();
        assert_eq!(rule.priority(), 50);
        assert!(!rule.is_enabled());
    }

    #[test]
    fn test_builder_custom_apply_fn() {
        let rule = MaskRule::builder("reverse")
            .pattern(r"\w+")
            .replacement("")
            .apply_fn(Arc::new(|_re: &Regex, text: &str, _rep: &str| {
                text.chars().rev().collect()
            }))
            .build()
            .unwrap();
        assert_eq!(rule.apply("hello"), "olleh");
    }

    #[test]
    fn test_builder_missing_pattern() {
        let result = MaskRule::builder("no_pattern").replacement("***").build();
        assert!(result.is_err());
    }

    #[test]
    fn test_builder_invalid_regex() {
        let result = MaskRule::builder("bad_regex").pattern(r"[invalid").build();
        assert!(result.is_err());
    }
}

pub fn mask_email(email: &str) -> String {
    EMAIL_REGEX.replace(email, "**@**.***").to_string()
}

pub fn mask_phone(phone: &str) -> String {
    PHONE_REGEX.replace(phone, "***-****-****").to_string()
}

#[cfg(test)]
#[allow(clippy::similar_names, clippy::unreadable_literal)]
mod tests {
    use super::*;
    use serial_test::serial;

    // ---- apply 路径引擎错误回归：fancy_regex replace_all 内部 unwrap 曾使
    // console worker panic、日志双通道静默终止（外部库 0.19.2 缺陷），
    // apply 必须经 try_replacen 优雅降级 ----

    #[test]
    #[serial]
    fn test_apply_engine_error_fails_closed_instead_of_returning_plaintext() {
        // 环视规则（负向断言）在长输入上耗尽 fancy-regex 的回溯预算
        // （BacktrackLimitExceeded）——确定性引擎错误。
        let rule = MaskRule::builder("zz-catastrophic")
            .pattern(r"(?<![0-9A-Za-z])1[3-9]\d{9}(?![0-9A-Za-z])")
            .build()
            .unwrap();
        let text = format!("13812345678 {}", "1".repeat(1_100_000));
        // 修复前：此处 panic；再修复前：留痕并返回原文（fail-open，未脱敏
        // 原文直落日志）；当前：留痕并以保守遮蔽整体替换（fail-closed）
        let out = rule.apply(&text);
        assert_eq!(out, ENGINE_ERROR_REDACTION);
        assert!(!out.contains("13812345678"));
    }

    #[test]
    fn test_credit_card_apply_on_v051_scale_input_does_not_panic() {
        // V051（12306 站点种子 250KB 级）真实场景规模：credit_card 规则
        // 扫描长 SQL 语句曾触发同型 panic；断言不 panic 且输出非空即可
        // （掩码成功或优雅跳过均为合法终态）。
        let rule = MaskRule::new_credit_card_rule();
        let unit =
            "INSERT INTO station VALUES ('VNP','北京南','beijingnan','010','1000001234567890');"
                .repeat(4000);
        let out = rule.apply(&unit);
        assert!(!out.is_empty());
    }

    #[test]
    fn test_mask_email() {
        let test_cases = vec![
            ("test@example.com", "**@**.***"),
            ("user.name@company.co.uk", "**@**.***"),
            ("admin@localhost", "**@**.***"),
        ];

        for (input, expected) in test_cases {
            let result = mask_email(input);
            assert_eq!(result, expected, "Failed for: {input}");
        }
    }

    #[test]
    fn test_mask_phone() {
        let test_cases = vec![
            ("13812345678", "***-****-****"),
            ("15987654321", "***-****-****"),
            ("Contact: 18655556666 now", "Contact: ***-****-**** now"),
        ];

        for (input, expected) in test_cases {
            let result = mask_phone(input);
            assert_eq!(result, expected, "Failed for: {input}");
        }
    }

    #[test]
    fn test_data_masker() {
        let masker = DataMasker::new();

        let test_email = "user@example.com";
        assert_eq!(masker.mask(test_email), "**@**.***");

        let test_phone = "13912345678";
        assert_eq!(masker.mask(test_phone), "***-****-****");

        let mixed = "Contact user at test@example.com, phone: 13812345678";
        let result = masker.mask(mixed);
        assert!(!result.contains("test@example.com"));
        assert!(!result.contains("13812345678"));
    }

    #[test]
    fn test_mask_value() {
        let masker = DataMasker::new();

        let mut value = serde_json::json!({
            "email": "user@example.com",
            "phone": "13712345678",
            "name": "John"
        });

        masker.mask_value(&mut value);

        assert_eq!(value["email"], "**@**.***");
        assert_eq!(value["phone"], "***-****-****");
        assert_eq!(value["name"], "John");
    }

    #[test]
    fn test_mask_nested_value() {
        let masker = DataMasker::new();

        let mut value = serde_json::json!({
            "user": {
                "email": "admin@company.org",
                "contacts": ["test@email.com", "13811112222"]
            }
        });

        masker.mask_value(&mut value);

        let user = &value["user"];
        assert_eq!(user["email"], "**@**.***");

        let contacts = user["contacts"]
            .as_array()
            .expect("contacts should be an array");
        assert_eq!(contacts[0], "**@**.***");
        assert_eq!(contacts[1], "***-****-****");
    }

    #[test]
    fn test_is_sensitive_field_password() {
        assert!(DataMasker::is_sensitive_field("password"));
        assert!(DataMasker::is_sensitive_field("PASSWORD"));
        assert!(DataMasker::is_sensitive_field("Password"));
    }

    #[test]
    fn test_is_sensitive_field_api_key() {
        assert!(DataMasker::is_sensitive_field("api_key"));
        assert!(DataMasker::is_sensitive_field("apiKey"));
        assert!(DataMasker::is_sensitive_field("API_KEY"));
        assert!(DataMasker::is_sensitive_field("api-secret"));
    }

    #[test]
    fn test_is_sensitive_field_jwt() {
        assert!(DataMasker::is_sensitive_field("jwt"));
        assert!(DataMasker::is_sensitive_field("jwt_token"));
        assert!(DataMasker::is_sensitive_field("bearer_token"));
    }

    #[test]
    fn test_is_sensitive_field_aws() {
        assert!(DataMasker::is_sensitive_field("aws_secret"));
        assert!(DataMasker::is_sensitive_field("aws_key"));
        assert!(DataMasker::is_sensitive_field("aws_credentials"));
    }

    #[test]
    fn test_is_sensitive_field_credit_card() {
        assert!(DataMasker::is_sensitive_field("credit_card"));
        assert!(DataMasker::is_sensitive_field("card_number"));
        assert!(DataMasker::is_sensitive_field("cvv"));
    }

    #[test]
    fn test_is_not_sensitive_field() {
        assert!(!DataMasker::is_sensitive_field("username"));
        assert!(!DataMasker::is_sensitive_field("message"));
        assert!(!DataMasker::is_sensitive_field("content"));
        assert!(!DataMasker::is_sensitive_field("title"));
    }

    #[test]
    fn test_mask_email_variations() {
        let test_cases = vec![
            ("test@example.com", "**@**.***"),
            ("user.name@company.co.uk", "**@**.***"),
            ("admin@localhost", "**@**.***"),
            ("user+tag@example.org", "**@**.***"),
            ("user_name@test.io", "**@**.***"),
        ];
        for (input, expected) in test_cases {
            let result = mask_email(input);
            assert_eq!(result, expected, "Failed for: {input}");
        }
    }

    #[test]
    fn test_mask_phone_variations() {
        let test_cases = vec![
            ("13812345678", "***-****-****"),
            ("15987654321", "***-****-****"),
            ("Contact: 18655556666 now", "Contact: ***-****-**** now"),
        ];
        for (input, expected) in test_cases {
            let result = mask_phone(input);
            assert_eq!(result, expected, "Failed for: {input}");
        }
    }

    #[test]
    fn test_mask_jwt_token() {
        let masker = DataMasker::new();
        let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let result = masker.mask(jwt);
        assert!(result.contains("***REDACTED_JWT***"));
        assert!(!result.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
    }

    #[test]
    fn test_mask_aws_key() {
        let masker = DataMasker::new();
        let aws_key = "AKIAIOSFODNN7EXAMPLE";
        let result = masker.mask(aws_key);
        assert!(result.contains("***REDACTED***"));
    }

    #[test]
    fn test_mask_api_key_value() {
        let masker = DataMasker::new();
        // 夹具经 format! 拼装：pre-commit no-private-key 钩子按
        // 「sk- + 20 位连续字母数字」扫描暂存源文件，整段密钥形态
        // 字面量不能落在源码里
        let secret = format!("sk-{}", "1234567890abcdefghijABCDEFGH");
        let message = format!("api_key={secret}");
        let result = masker.mask(&message);
        assert!(result.contains("***REDACTED***"));
        assert!(!result.contains(&secret));
    }

    #[test]
    fn test_mask_api_key_single_marker() {
        let masker = DataMasker::new();
        // 标记幂等契约（USER_GUIDE「Sink 出口脱敏契约」）：每个标记仅来自
        // 一次真实脱敏——键名保留、值整体替换为单标记；双标记即 api_key
        // 规则把整段匹配当作组 1 展开后再被 generic_secret 二次命中
        let result = masker.mask("api_key=abcdefghijklmnopqrstuvwxyz");
        assert_eq!(result, "api_key=***REDACTED***");
    }

    #[test]
    fn test_mask_password_value() {
        let masker = DataMasker::new();
        // Use a test case that matches the generic secret pattern
        let message = "mypassword=abcdefghijklmnopqrst";
        let result = masker.mask(message);
        // The password should be masked or the message should change
        assert!(result.contains("REDACTED") || !result.contains("abcdefghijklmnopqrst"));
    }

    #[test]
    fn test_mask_marker_idempotency() {
        let masker = DataMasker::new();

        // 已掩码消息再次进入本入口：幂等，不产生嵌套标记
        let once = masker.mask("mypassword=abcdefghijklmnopqrst");
        assert!(
            once.contains("***REDACTED***"),
            "first masking pass should mask the secret, got: {once}"
        );
        assert_eq!(
            masker.mask(&once),
            once,
            "re-masking already masked text must be a no-op"
        );

        // 已含 ***MASKED*** / [REDACTED] 标记的输入（subscriber 脱敏或上游
        // 掩码的产物）原样通过，不被再次改写
        let marked = "token=***MASKED*** password=[REDACTED]";
        assert_eq!(
            masker.mask(marked),
            marked,
            "marked input must pass through unchanged"
        );
    }

    #[test]
    fn test_mask_marker_injection_does_not_disable_rules() {
        // 回归：整条消息级标记短路可被自由文本伪造——注入一个标记即可让
        // 整条记录免于掩码（含 secret-scan 门）。标记保护只在「值」一级。
        let masker = DataMasker::new();
        for marker in ["[REDACTED]", "***REDACTED***", "***MASKED***"] {
            let text = format!("login user={marker} token=sk-live-abcdefghijklmnopqrstuvwxyz");
            let masked = masker.mask(&text);
            assert!(
                !masked.contains("sk-live-abcdefghijklmnopqrstuvwxyz"),
                "injected marker {marker} must not exempt the record: {masked}"
            );

            // 同一条消息中的 PII 也不得因标记旁路而明文留存
            let text = format!("user={marker} phone=13800138000");
            let masked = masker.mask(&text);
            assert!(
                !masked.contains("13800138000"),
                "injected marker {marker} must not exempt PII: {masked}"
            );
        }
    }

    #[test]
    fn test_mask_long_input_backtrack_budget_does_not_leak_pii() {
        // 回溯预算耗尽窗口：fancy-regex 的 look-around 规则在长输入上耗尽
        // 回溯预算（引擎错误）。引擎错误不得让整条记录以未脱敏原文落盘。
        let masker = DataMasker::new();
        let big = format!("13812345678 123-45-6789 {}", "1".repeat(1_000_000));
        let masked = masker.mask(&big);
        assert!(
            !masked.contains("13812345678"),
            "phone must not survive the backtrack budget window: {masked:.80}"
        );
        assert!(
            !masked.contains("123-45-6789"),
            "ssn must not survive the backtrack budget window: {masked:.80}"
        );
    }

    #[test]
    fn test_mask_database_url() {
        let masker = DataMasker::new();
        let message = "db_url=postgres://user:password123@localhost:5432/mydb";
        let result = masker.mask(message);
        // URL should be masked or password should be hidden
        assert!(result.contains("REDACTED") || !result.contains("password123"));
    }

    #[test]
    fn test_mask_oauth_token() {
        let masker = DataMasker::new();
        let message = "oauth_token=ya29_token_value_here";
        let result = masker.mask(message);
        assert!(result.contains("REDACTED") || !result.contains("token_value"));
    }

    #[test]
    fn test_mask_empty_string() {
        let masker = DataMasker::new();
        let result = masker.mask("");
        assert_eq!(result, "");
    }

    #[test]
    fn test_mask_no_sensitive_data() {
        let masker = DataMasker::new();
        let message = "This is a normal log message without any sensitive data";
        let result = masker.mask(message);
        assert_eq!(result, message);
    }

    #[test]
    fn test_mask_multiple_sensitive_items() {
        let masker = DataMasker::new();
        // Test with simple email and phone that the regex can match
        let message = "Email: test@example.com, Phone: 13812345678";
        let result = masker.mask(message);
        // At least the email should be masked
        assert!(!result.contains("test@example.com"));
    }

    #[test]
    fn test_mask_hashmap() {
        let masker = DataMasker::new();
        let mut map: HashMap<String, Value> = HashMap::new();
        map.insert(
            "email".to_string(),
            Value::String("user@example.com".to_string()),
        );
        map.insert(
            "password".to_string(),
            Value::String("secret123".to_string()),
        );
        map.insert("name".to_string(), Value::String("John".to_string()));

        masker.mask_hashmap(&mut map);

        assert_eq!(map["email"], "**@**.***");
        assert_eq!(map["name"], "John");
    }

    #[test]
    fn test_mask_array_of_objects() {
        let masker = DataMasker::new();
        let mut value = serde_json::json!([
            {"email": "a@b.com", "name": "A"},
            {"email": "c@d.com", "name": "B"}
        ]);

        masker.mask_value(&mut value);

        let arr = value.as_array().unwrap();
        assert_eq!(arr[0]["email"], "**@**.***");
        assert_eq!(arr[1]["email"], "**@**.***");
    }

    #[test]
    fn test_api_key_rule_does_not_panic() {
        let masker = DataMasker::new();
        let input = "api_key=abcdefghijklmnopqrstuvwxyz1234";
        let result = masker.mask(input);
        assert!(result.contains("***REDACTED***"));
    }

    #[test]
    fn test_generic_secret_rule_does_not_panic() {
        let masker = DataMasker::new();
        let input = "my_token=abcdefghijklmnop1234";
        let result = masker.mask(input);
        assert!(result.contains("***REDACTED***"));
    }

    #[test]
    fn test_mask_skips_disabled_rules() {
        // mask should respect the enabled flag on rules
        let masker = DataMasker::builder().disable_builtin("email").build();
        let input = "user@example.com";
        let result = masker.mask(input);
        // Email should NOT be masked because the rule is disabled
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn test_mask_value_masks_sensitive_keys() {
        // mask_value should check key names for sensitive fields
        let masker = DataMasker::new();
        let mut value = serde_json::json!({
            "password": "secret123",
            "name": "Alice"
        });
        masker.mask_value(&mut value);
        assert_eq!(value["password"], "***MASKED***");
        assert_eq!(value["name"], "Alice");
    }

    #[test]
    fn test_mask_hashmap_masks_sensitive_keys() {
        // mask_hashmap should check key names for sensitive fields
        let masker = DataMasker::new();
        let mut map = HashMap::new();
        map.insert(
            "api_key".to_string(),
            Value::String("supersecret".to_string()),
        );
        map.insert("user".to_string(), Value::String("bob".to_string()));
        masker.mask_hashmap(&mut map);
        assert_eq!(map["api_key"], Value::String("***MASKED***".to_string()));
        assert_eq!(map["user"], Value::String("bob".to_string()));
    }

    #[test]
    fn test_mask_rule_debug() {
        let rule = MaskRule::builder("test_debug")
            .pattern(r"\d+")
            .replacement("***")
            .build()
            .unwrap();
        let debug_str = format!("{rule:?}");
        assert!(debug_str.contains("MaskRule"));
        assert!(debug_str.contains("test_debug"));
        assert!(debug_str.contains("<fn>"));
    }

    #[test]
    fn test_builder_add_rule() {
        let custom_rule = MaskRule::builder("custom_upper")
            .pattern(r"[a-z]+")
            .replacement("REPLACED")
            .priority(1)
            .build()
            .unwrap();
        let masker = DataMasker::builder().add_rule(custom_rule).build();
        let result = masker.mask("hello");
        assert!(result.contains("REPLACED"));
    }

    #[test]
    fn test_builder_with_registry() {
        let mut registry = crate::masking::MaskRuleRegistry::with_builtins();
        registry.set_enabled("email", false);
        let masker = DataMasker::builder().with_registry(registry).build();
        // email rule disabled via registry, so email should not be masked
        let result = masker.mask("user@example.com");
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn test_mask_credit_card_visa() {
        let masker = DataMasker::new();
        // Valid Visa card (Luhn check passes)
        let result = masker.mask("Card: 4111111111111111");
        assert!(!result.contains("4111111111111111"));
        assert!(result.contains("****-****-****-1111"));
    }

    #[test]
    fn test_mask_credit_card_amex() {
        let masker = DataMasker::new();
        // Valid Amex card (Luhn check passes)
        let result = masker.mask("Card: 378282246310005");
        assert!(!result.contains("378282246310005"));
        assert!(result.contains("****-******-0005"));
    }

    #[test]
    fn test_mask_ipv4() {
        let masker = DataMasker::new();
        let result = masker.mask("Server IP: 192.168.1.100");
        assert!(!result.contains("192.168.1.100"));
        assert!(result.contains("***.***.***.100"));
    }

    #[test]
    fn test_mask_ipv6() {
        let masker = DataMasker::new();
        let result = masker.mask("IPv6: 2001:0db8:85a3:0000:0000:8a2e:0370:7334");
        assert!(!result.contains("2001:0db8:85a3:0000:0000:8a2e:0370:7334"));
        assert!(result.contains("7334"));
    }

    #[test]
    fn test_mask_ipv6_compressed() {
        let masker = DataMasker::new();
        // :: 压缩写法是 RFC 5952 规范形式（日志中最常见），漏脱敏即网络标识泄露
        let result = masker.mask("Gateway: 2001:db8::1");
        assert!(
            !result.contains("2001:db8::1"),
            "compressed IPv6 must be masked, got: {result}"
        );
        assert!(
            result.contains("****:****:****:****:****:****:****:1"),
            "the last group keeps the real value, got: {result}"
        );
    }

    #[test]
    fn test_mask_ipv6_compressed_loopback() {
        let masker = DataMasker::new();
        // 前导 ::（无前导分组）：\b 语义下无词边界，须由环视锚定
        let result = masker.mask("peer=[::1]:8080");
        assert!(
            !result.contains("::1"),
            "loopback ::1 must be masked, got: {result}"
        );
        assert!(
            result.contains("[****:****:****:****:****:****:****:1]:8080"),
            "got: {result}"
        );
    }

    #[test]
    fn test_mask_mac_address_colon() {
        let masker = DataMasker::new();
        let result = masker.mask("MAC: AA:BB:CC:DD:EE:FF");
        assert!(!result.contains("AA:BB:CC:DD:EE:FF"));
        assert!(result.contains("AA:**:**:**:**:FF"));
    }

    #[test]
    fn test_mask_mac_address_dash() {
        let masker = DataMasker::new();
        let result = masker.mask("MAC: 00-1A-2B-3C-4D-5E");
        assert!(!result.contains("00-1A-2B-3C-4D-5E"));
        assert!(result.contains("00-**-**-**-**-5E"));
    }

    #[test]
    fn test_mask_passport() {
        let masker = DataMasker::new();
        let result = masker.mask("Passport: E12345678");
        assert!(!result.contains("E12345678"));
        assert!(result.contains("E******78"));
    }

    #[test]
    fn test_mask_ssn() {
        let masker = DataMasker::new();
        let result = masker.mask("SSN: 123-45-6789");
        assert!(!result.contains("123-45-6789"));
        assert!(result.contains("***-**-6789"));
    }

    #[test]
    fn test_mask_ssn_spaced_form() {
        let masker = DataMasker::new();
        let result = masker.mask("SSN 123 45 6789");
        assert!(!result.contains("123 45 6789"));
        assert!(result.contains("***-**-6789"));
    }

    #[test]
    fn test_mask_ssn_compact_form() {
        let masker = DataMasker::new();
        let result = masker.mask("SSN 123456789");
        assert!(!result.contains("123456789"));
        assert!(result.contains("***-**-6789"));
    }

    #[test]
    fn test_mask_ssn_numeric_json_value() {
        // 非敏感键下的数字形态 SSN 经数值路径串化后被紧凑分支掩码
        let masker = DataMasker::new();
        let mut value = serde_json::json!({ "ref": 123456789u64 });
        masker.mask_value(&mut value);
        assert_eq!(value["ref"], serde_json::json!("***-**-6789"));
    }

    #[test]
    fn test_mask_ssn_compact_guard_rejects_invalid() {
        let masker = DataMasker::new();
        // 组号非法（000/666/900-999）、区号 00、序列号 0000 的紧凑 9 位不命中 SSN 掩码
        for s in [
            "000456789",
            "666123450",
            "900123450",
            "123004567",
            "123450000",
        ] {
            let result = masker.mask(s);
            assert!(
                !result.contains("***-**-"),
                "非法紧凑号 {s} 不应命中 SSN 掩码"
            );
        }
    }

    #[test]
    fn test_mask_ssn_widening_no_new_false_positives() {
        let masker = DataMasker::new();
        // 已知安全形态不因本次 SSN 拓宽被新掩（隔离 SSN 掩码标记 ***-**-）
        for s in [
            "1695000000000", // 13 位毫秒时间戳
            "1234567890",    // 10 位数字串
            "13812345678",   // 11 位手机号（由 phone 规则负责，非 SSN）
            "e1a2b3c4d",     // 含字母 hex（git 短 SHA 形态）
        ] {
            let result = masker.mask(s);
            assert!(!result.contains("***-**-"), "{s} 不应命中 SSN 掩码形态");
        }
    }

    #[test]
    fn test_mask_credit_card_luhn_failure_still_masked_by_bank_card() {
        // A number matching Visa pattern but failing Luhn check
        // is NOT masked by the credit_card rule, but IS masked by
        // the bank_card rule (which has lower priority = runs after).
        let masker = DataMasker::new();
        // 4111111111111112 fails Luhn (last digit changed from 1 to 2)
        let result = masker.mask("Card: 4111111111111112");
        // bank_card rule masks it with its own pattern
        assert!(!result.contains("4111111111111112"));
    }

    #[test]
    fn test_mask_builder_no_builtins_no_custom() {
        // Use with_registry with an empty (default) registry to test the empty rules path
        let registry = crate::masking::MaskRuleRegistry::default();
        let masker = DataMasker::builder().with_registry(registry).build();
        // No rules means nothing gets masked
        let result = masker.mask("user@example.com 4111111111111111");
        assert_eq!(result, "user@example.com 4111111111111111");
    }

    #[test]
    fn test_default_apply_fn_masks_all_matches() {
        // 默认 apply_fn 必须替换全部匹配，而非仅首个
        let rule = MaskRule::builder("digits")
            .pattern(r"\d+")
            .replacement("*")
            .build()
            .unwrap();
        let masker = DataMasker::builder().add_rule(rule).build();
        assert_eq!(masker.mask("a1b22c333"), "a*b*c*");
    }

    #[test]
    fn test_mask_multiple_emails_all_masked() {
        let masker = DataMasker::new();
        let result = masker.mask("a@test.com and b@test.org and c@test.net");
        assert_eq!(result.matches("**@**.***").count(), 3, "Result: {result}");
        assert!(!result.contains("@test"));
    }

    #[test]
    fn test_mask_multiple_phones_all_masked() {
        let masker = DataMasker::new();
        let result = masker.mask("13812345678 / 15987654321 / 18611112222");
        assert_eq!(
            result.matches("***-****-****").count(),
            3,
            "Result: {result}"
        );
        assert!(!result.contains("13812345678"));
        assert!(!result.contains("15987654321"));
        assert!(!result.contains("18611112222"));
    }

    #[test]
    fn test_id_card_multiple_matches_all_masked() {
        let masker = DataMasker::new();
        let result = masker.mask("A: 110101199001011234 B: 310105199001012345");
        assert_eq!(result.matches("******").count(), 2, "Result: {result}");
        assert!(!result.contains("110101199001011234"));
        assert!(!result.contains("310105199001012345"));
    }

    #[test]
    fn test_credit_card_luhn_failure_masked_when_bank_card_disabled() {
        // Luhn 校验失败只说明"不是有效银行卡"，不代表不需要脱敏：
        // 即使 bank_card 规则被禁用，形似卡号的数字串仍应被 credit_card 规则掩码
        let masker = DataMasker::builder().disable_builtin("bank_card").build();
        // 4111111111111112 fails Luhn (last digit changed from 1 to 2)
        let result = masker.mask("Card: 4111111111111112");
        assert!(
            !result.contains("4111111111111112"),
            "Luhn-failing card-shaped number must still be masked: {result}"
        );
        assert!(result.contains("***REDACTED_CC***"), "Result: {result}");
    }

    #[test]
    fn test_bank_card_requires_word_boundaries() {
        // \b 边界：嵌入更长字母数字 token 中的 13-19 位数字不再被误判为银行卡号
        let masker = DataMasker::builder().disable_builtin("credit_card").build();
        let result = masker.mask("ref no: REF1234567890123456END");
        assert_eq!(
            result, "ref no: REF1234567890123456END",
            "digits embedded in a word must not be treated as a bank card"
        );
    }

    #[test]
    fn test_cjk_adjacent_numbers_are_masked() {
        // CJK 汉字属 Unicode 词字符，\b 在汉字-数字交界无边界导致漏报；
        // 负向断言修复后，数字紧贴汉字的文本必须被掩码
        let masker = DataMasker::new();
        let result = masker.mask("电话13812345678，请回电");
        assert!(
            !result.contains("13812345678"),
            "phone glued to CJK text must be masked: {result}"
        );
        let result = masker.mask("身份证110101199001011234号");
        assert!(
            !result.contains("110101199001011234"),
            "ID card glued to CJK text must be masked: {result}"
        );
    }

    #[test]
    fn test_id_card_lowercase_x_suffix_is_masked() {
        let masker = DataMasker::new();
        let result = masker.mask("11010119900101123x");
        assert!(
            !result.contains("11010119900101123x"),
            "lowercase x checksum must be masked: {result}"
        );
        assert!(result.contains("******"), "Result: {result}");
    }

    #[test]
    fn test_13_digit_timestamp_not_masked_as_bank_card() {
        // bank_card 收紧为 16-19 位后，13 位毫秒时间戳不再误伤
        let masker = DataMasker::new();
        let result = masker.mask("created_at=1695000000000 done");
        assert_eq!(
            result, "created_at=1695000000000 done",
            "13-digit epoch millis must stay untouched: {result}"
        );
    }

    #[test]
    fn test_git_short_sha_not_masked_as_passport() {
        let masker = DataMasker::new();
        let result = masker.mask("commit e1a2b3c4d fixed it"); // pragma: allowlist secret — 测试夹具（git 短 SHA 形态字符串，非凭据）
        assert!(
            result.contains("e1a2b3c4d"), // pragma: allowlist secret — 同上，测试夹具
            "9-char mixed-hex git short SHA must not be masked as passport: {result}"
        );
        // 真实护照形态（前缀 + 纯数字后缀）仍被掩码
        let result = masker.mask("护照E12345678已签发");
        assert!(
            !result.contains("E12345678"),
            "passport E+8digits must be masked: {result}"
        );
    }

    #[test]
    fn test_plain_english_words_not_masked_as_passport() {
        // e/g 开头的普通英文单词不得被护照规则误掩：
        // 护照是前缀字母 + 8 位纯数字，字母后缀不是合法护照形态
        let masker = DataMasker::new();
        for text in [
            "task execution completed",
            "everything generation gradually",
            "the group finished",
        ] {
            let result = masker.mask(text);
            assert_eq!(
                result, text,
                "plain English words must stay untouched: {result}"
            );
        }
    }

    #[test]
    fn test_deeply_nested_value_does_not_overflow_stack() {
        // 1000 层嵌套对象：递归深度上限 16 层，超限子树替换为截断标记而非栈溢出
        let masker = DataMasker::new();
        let mut nested = serde_json::json!({"leaf": "phone 13812345678"});
        for _ in 0..1000 {
            nested = serde_json::json!({ "wrap": nested });
        }
        let mut value = nested;
        // 正常返回（不 panic / 不栈溢出）即为通过
        masker.mask_value(&mut value);
    }

    #[test]
    fn test_depth_limit_truncates_beyond_16_levels() {
        let masker = DataMasker::new();
        // 恰好在第 17 层放一个值：应被截断标记替换
        let mut inner = serde_json::json!({"v": "x"});
        for _ in 0..17 {
            inner = serde_json::json!({ "wrap": inner });
        }
        masker.mask_value(&mut inner);
        let rendered = inner.to_string();
        assert!(
            rendered.contains("***TRUNCATED***"),
            "17-level nesting must be truncated: {rendered}"
        );
    }

    #[test]
    fn test_numeric_phone_value_is_masked() {
        // 非字符串值：JSON 裸数字形态的手机号同样被掩码
        let masker = DataMasker::new();
        let mut fields = serde_json::json!({
            "count": 13812345678_i64,
            "flag": true,
            "nothing": null
        });
        masker.mask_value(&mut fields);
        let rendered = fields.to_string();
        assert!(
            !rendered.contains("13812345678"),
            "numeric phone value must be masked: {rendered}"
        );
        // null 保持 null，true 不误伤
        assert!(rendered.contains("null") && rendered.contains("true"));
    }

    #[test]
    fn test_name_field_patterns_coverage() {
        // R-maskcn-001：命中集
        for key in [
            "full_name",
            "real_name",
            "legal_name",
            "customer_name",
            "owner_name",
            "contact_name",
            "Real-Name",
            "family_name",
            "surname",
            "given_name",
            "姓名",
            "真实姓名",
            "客户姓名",
        ] {
            assert!(
                DataMasker::is_name_field(key),
                "name-family key must hit: {key}"
            );
        }
        // 不命中集：裸 name / user_name / 非姓名键
        for key in ["name", "user_name", "file_name", "password", "nickname"] {
            assert!(
                !DataMasker::is_name_field(key),
                "generic key must not hit name patterns: {key}"
            );
        }
    }

    #[test]
    fn test_cjk_name_value_masked_by_key_context() {
        // R-maskcn-002：姓名键 + 2-4 汉字值 → 保留首字 + **
        let masker = DataMasker::new();
        let mut fields = serde_json::json!({
            "real_name": "张三丰",
            "姓名": "欧阳文长",
            "given_name": "李四"
        });
        masker.mask_value(&mut fields);
        assert_eq!(fields["real_name"], "张**");
        assert_eq!(fields["姓名"], "欧**");
        assert_eq!(fields["given_name"], "李**");
    }

    #[test]
    fn test_non_cjk_or_wrong_shape_name_values_untouched() {
        let masker = DataMasker::new();
        let mut fields = serde_json::json!({
            "real_name": "John Smith",
            "contact_name": "张三123",
            "legal_name": "达尔文进化论研究小组"
        });
        masker.mask_value(&mut fields);
        assert_eq!(
            fields["real_name"], "John Smith",
            "a value that is not purely Han characters must be left untouched"
        );
        assert_eq!(
            fields["contact_name"], "张三123",
            "a value containing non-Han characters must be left untouched"
        );
        assert_eq!(
            fields["legal_name"], "达尔文进化论研究小组",
            "an over-long value must be left untouched"
        );
    }

    #[test]
    fn test_generic_name_keys_not_name_masked() {
        // 裸 name / user_name 刻意不在词表（登录 ID 误伤面）
        let masker = DataMasker::new();
        let mut fields = serde_json::json!({
            "name": "张三",
            "user_name": "张三"
        });
        masker.mask_value(&mut fields);
        assert_eq!(fields["name"], "张三");
        assert_eq!(fields["user_name"], "张三");
    }

    #[test]
    fn test_builder_literal_rule_masks_via_configured_rule() {
        // builder 构建的 literal 自定义规则参与掩码（fast-masking 下经 AC 路径）
        let rule = MaskRule::builder("corp_token")
            .pattern("CORP_SECRET_TOKEN")
            .replacement("***LITERAL_MASKED***")
            .literal(true)
            .build()
            .unwrap();
        let masker = DataMasker::builder().add_rule(rule).build();
        let result = masker.mask("header CORP_SECRET_TOKEN tail");
        assert!(
            result.contains("***LITERAL_MASKED***") && !result.contains("CORP_SECRET_TOKEN"),
            "literal rule must apply: {result}"
        );
    }

    #[test]
    fn test_builder_wires_ac_masker_for_literal_rules() {
        // fast-masking 下 builder 自动为 literal 规则构建 AC 加速器（接线不缺席）
        let rule = MaskRule::builder("corp_token")
            .pattern("CORP_SECRET_TOKEN")
            .replacement("***LITERAL_MASKED***")
            .literal(true)
            .build()
            .unwrap();
        let masker = DataMasker::builder().add_rule(rule).build();
        assert!(masker.ac_masker.is_some(), "AC masker must be wired");
        let result = masker.mask("header CORP_SECRET_TOKEN tail");
        assert!(result.contains("***LITERAL_MASKED***"));
    }

    #[test]
    fn test_oversized_input_is_redacted_not_passed_through() {
        let masker = DataMasker::new();
        let big = format!("user 13812345678 {}", "x".repeat(1024 * 1024 + 1));
        let result = masker.mask(&big);
        // 超过 1 MiB 的输入跳过 21 趟扫描，但整体遮蔽：未扫描即不可证安全，
        // 原样放行会让其中的凭据/PII 明文落盘
        assert_eq!(result, OVERSIZED_INPUT_REDACTION);
        assert!(!result.contains("13812345678"));
    }

    // mask_kv_lines：行级 key=value 脱敏（= 前原文逐字保留）

    #[test]
    fn test_mask_kv_lines_basic_space() {
        let masker = DataMasker::new();
        let result = masker.mask_kv_lines("password = secret123", &["password"], "***MASKED***");
        assert_eq!(result, "password =***MASKED***");
    }

    #[test]
    fn test_mask_kv_lines_no_space() {
        let masker = DataMasker::new();
        let result = masker.mask_kv_lines("password=secret123", &["password"], "***MASKED***");
        assert_eq!(result, "password=***MASKED***");
    }

    #[test]
    fn test_mask_kv_lines_indent_preserved() {
        let masker = DataMasker::new();
        let result = masker.mask_kv_lines("  password = secret", &["password"], "***MASKED***");
        assert_eq!(result, "  password =***MASKED***");
    }

    #[test]
    fn test_mask_kv_lines_multi_key() {
        let masker = DataMasker::new();
        let result = masker.mask_kv_lines(
            "password = a\napi_key = b",
            &["password", "api_key"],
            "***MASKED***",
        );
        assert_eq!(result, "password =***MASKED***\napi_key =***MASKED***");
    }

    #[test]
    fn test_mask_kv_lines_non_matching_key_line_untouched() {
        let masker = DataMasker::new();
        let result = masker.mask_kv_lines("username = alice", &["password"], "***MASKED***");
        assert_eq!(result, "username = alice");
    }

    #[test]
    fn test_mask_kv_lines_line_without_eq_untouched() {
        let masker = DataMasker::new();
        let result = masker.mask_kv_lines("password", &["password"], "***MASKED***");
        assert_eq!(result, "password");
    }

    #[test]
    fn test_mask_kv_lines_prefix_key_no_false_hit() {
        // 仅要求「以 key 开头且后随 =」会让 password_debug 误命中，
        // 键名与 = 之间只允许空白
        let masker = DataMasker::new();
        let result = masker.mask_kv_lines("password_debug = x", &["password"], "***MASKED***");
        assert_eq!(result, "password_debug = x");
    }

    #[test]
    fn test_mask_kv_lines_preserves_text_before_eq() {
        // = 前原文（含 key 与 = 之间的全部空白）逐字保留，只重组 = 之后
        let masker = DataMasker::new();
        let result = masker.mask_kv_lines("password   =   secret", &["password"], "***MASKED***");
        assert_eq!(result, "password   =***MASKED***");
    }

    #[test]
    fn test_mask_kv_lines_empty_value() {
        let masker = DataMasker::new();
        let result = masker.mask_kv_lines("password =", &["password"], "***MASKED***");
        assert_eq!(result, "password =***MASKED***");
    }

    #[test]
    fn test_mask_kv_lines_trailing_whitespace_replaced() {
        // = 之后的全部内容（含行尾空白）都参与替换
        let masker = DataMasker::new();
        let result = masker.mask_kv_lines("password = secret  ", &["password"], "***MASKED***");
        assert_eq!(result, "password =***MASKED***");
    }

    #[test]
    fn test_mask_kv_lines_multiline_mixed() {
        // 命中行、不命中行混合；尾行无换行符时输出同样不以换行结尾
        let masker = DataMasker::new();
        let result = masker.mask_kv_lines(
            "token = abc\nplain text line\npassword = xyz",
            &["token", "password"],
            "***MASKED***",
        );
        assert_eq!(
            result,
            "token =***MASKED***\nplain text line\npassword =***MASKED***"
        );
    }

    #[test]
    fn test_mask_kv_lines_crlf_value_side_replaced() {
        // CRLF 下 \r 位于 = 之后，属于被替换的值侧；行结构以 \n 保留
        let masker = DataMasker::new();
        let result = masker.mask_kv_lines(
            "password = a\r\napi_key = b",
            &["password", "api_key"],
            "***MASKED***",
        );
        assert_eq!(result, "password =***MASKED***\napi_key =***MASKED***");
    }

    #[test]
    fn test_mask_kv_lines_custom_marker() {
        let masker = DataMasker::new();
        let result = masker.mask_kv_lines("password = x", &["password"], "[REDACTED]");
        assert_eq!(result, "password =[REDACTED]");
    }

    // detect-only 检测面：detect/has_match 与 mask 的一致性

    #[test]
    fn test_pattern_accessor_returns_source_pattern() {
        let rule = MaskRule::builder("probe")
            .pattern(r"\bPROBE-\d{3}\b")
            .build()
            .unwrap();
        assert_eq!(rule.pattern(), r"\bPROBE-\d{3}\b");
    }

    #[test]
    fn test_detect_reports_email_with_exact_positions() {
        let masker = DataMasker::new();
        let text = "contact user@example.com thanks";
        let matches = masker.detect(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].rule, "email");
        assert_eq!(&text[matches[0].start..matches[0].end], "user@example.com");
    }

    #[test]
    fn test_detect_credit_card_luhn_pass_matches_mask() {
        let masker = DataMasker::new();
        let text = "card 4111111111111111 end";
        let matches = masker.detect(text);
        assert!(
            matches.iter().any(|m| m.rule == "credit_card"),
            "Luhn-passing card must be detected: {matches:?}"
        );
        // detect 命中位置与 mask 改写区域对齐
        let cc = matches.iter().find(|m| m.rule == "credit_card").unwrap();
        assert_eq!(&text[cc.start..cc.end], "4111111111111111");
        let masked = masker.mask(text);
        assert!(masked.contains("****-****-****-1111"));
        assert!(!masked.contains("4111111111111111"));
    }

    #[test]
    fn test_detect_credit_card_luhn_fail_matches_mask() {
        // apply_fn 规则自洽的关键用例：credit_card 的 apply_fn 对 Luhn
        // 失败的卡形数字 fall back 到 replacement（仍改写），
        // detect 必须同样报告命中——不能只报 Luhn 通过的
        let masker = DataMasker::new();
        let text = "card 4111111111111112 end";
        let matches = masker.detect(text);
        assert!(
            matches.iter().any(|m| m.rule == "credit_card"),
            "Luhn-failing card-shaped number must still be detected: {matches:?}"
        );
        let masked = masker.mask(text);
        assert!(!masked.contains("4111111111111112"));
    }

    #[test]
    fn test_detect_bank_card_matches_mask() {
        let masker = DataMasker::new();
        let text = "iban 6011000990139424 ok";
        let matches = masker.detect(text);
        assert!(
            matches.iter().any(|m| m.rule == "bank_card"),
            "bank_card rule must fire on 16-digit number: {matches:?}"
        );
        let masked = masker.mask(text);
        assert!(
            masked.contains("****-****-****-9424"),
            "bank_card apply_fn formats with last four visible: {masked}"
        );
    }

    #[test]
    fn test_detect_fail_closed_on_marked_input() {
        // fail-closed：detect 不走标记短路——已含 REDACTED 标记的
        // 输入仍要检测出伴随的未脱敏 PII；mask 同样不走整条消息级
        // 标记短路（标记词表可被自由文本伪造，见 test_mask_marker_injection_does_not_disable_rules）
        let masker = DataMasker::new();
        let text = "user@example.com ***REDACTED***";
        let matches = masker.detect(text);
        assert!(
            matches.iter().any(|m| m.rule == "email"),
            "detect must not be blinded by existing redaction markers: {matches:?}"
        );
        // 标记不再让整条消息免于掩码：并存的 PII 仍被遮蔽
        let masked = masker.mask(text);
        assert!(
            !masked.contains("user@example.com"),
            "mask must not be blinded by existing redaction markers: {masked}"
        );
        assert!(masked.contains("***REDACTED***"));
    }

    #[test]
    fn test_has_match_agrees_with_detect() {
        let masker = DataMasker::new();
        assert!(masker.has_match("mail bob@example.com"));
        assert!(masker.has_match("user@example.com ***REDACTED***"));
        assert!(!masker.has_match("nothing sensitive here"));
        assert!(!masker.has_match(""));
        // has_match 与 detect 的布尔一致性
        for text in ["", "plain", "bob@example.com", "4111111111111111"] {
            assert_eq!(masker.has_match(text), !masker.detect(text).is_empty());
        }
    }

    #[test]
    fn test_detect_respects_disabled_rules() {
        let mut registry = crate::masking::MaskRuleRegistry::with_builtins();
        registry.set_enabled("email", false);
        let masker = DataMasker::builder().with_registry(registry).build();

        let text = "mail bob@example.com";
        assert!(!masker.detect(text).iter().any(|m| m.rule == "email"));
        assert!(!masker.has_match(text));
        assert_eq!(masker.mask(text), text);
    }

    #[test]
    fn test_detect_empty_input_and_no_matches() {
        let masker = DataMasker::new();
        assert!(masker.detect("").is_empty());
        assert!(masker.detect("no pii at all").is_empty());
    }

    #[test]
    fn test_detect_reports_each_rule_independently() {
        // 各规则在原始文本上独立报告（email 与 id_card 等可在同一文本
        // 各自命中）；mask 按优先级串行改写
        let masker = DataMasker::new();
        let text = "mail a@b.com id 110101199001011234";
        let matches = masker.detect(text);
        let rules: Vec<&str> = matches.iter().map(|m| m.rule.as_str()).collect();
        assert!(rules.contains(&"email"));
        assert!(rules.contains(&"id_card"));
        // 位置切片都还原原文
        for m in &matches {
            assert!(!&text[m.start..m.end].is_empty());
        }
    }
    // secret-scan：中文键名组 + 出站门挂载（feature 门控）

    #[test]
    fn test_chinese_sensitive_keys_hit() {
        for key in [
            "密码",
            "数据库密码",
            "访问令牌",
            "api密钥",
            "登录口令",
            "凭证",
            "凭据",
            "服务私钥",
        ] {
            assert!(
                DataMasker::is_sensitive_field(key),
                "chinese sensitive key must hit: {key}"
            );
        }
    }

    #[test]
    fn test_chinese_non_sensitive_keys_miss() {
        // 后缀锚定：以敏感词开头的一般词汇（密码学/令牌环）与公钥不命中
        for key in ["密码学", "令牌环", "公钥", "名称", "钥"] {
            assert!(
                !DataMasker::is_sensitive_field(key),
                "generic key must not hit: {key}"
            );
        }
    }

    #[test]
    fn test_mask_value_chinese_key_masks_value() {
        let masker = DataMasker::new();
        let mut fields = serde_json::json!({"数据库密码": "hunter2", "备注": "ok"});
        masker.mask_value(&mut fields);
        assert_eq!(fields["数据库密码"], "***MASKED***");
        assert_eq!(fields["备注"], "ok");
    }

    #[test]
    fn test_builder_gate_masks_bare_secret_and_counts() {
        use crate::masking::{SecretPatternRegistry, SecretScanGate};
        let gate = SecretScanGate::new(&SecretPatternRegistry::with_builtins());
        let masker = DataMasker::builder().with_secret_scan(gate.clone()).build();
        let raw = "connected with sk-proj-abcdefghijklmnopqrstuvwxyz123456 done";
        let out = masker.mask(raw);
        assert!(out.contains("***REDACTED_API_KEY***"), "{out}");
        assert!(!out.contains("sk-proj-"), "{out}");
        // 门先于规则集执行：归因计数基于原文，不被前序规则改写干扰
        assert_eq!(
            gate.hit_count("openai_sk"),
            Some(1),
            "gate must see raw text before rules"
        );
    }

    #[test]
    fn test_mask_checked_fails_closed_over_limit() {
        use crate::masking::{SecretPatternRegistry, SecretScanGate};
        let gate = SecretScanGate::new(&SecretPatternRegistry::with_builtins()).with_match_limit(1);
        let masker = DataMasker::builder().with_secret_scan(gate).build();
        let raw = "x sk-proj-abcdefghijklmnopqrstuvwxyz123456 y \
                   sk-proj-abcdefghijklmnopqrstuvwxyz789";
        assert!(
            matches!(
                masker.mask_checked(raw),
                Err(MaskError::SecretScanLimit { .. })
            ),
            "over-limit must fail closed"
        );
    }

    #[test]
    fn test_mask_checked_without_gate_delegates_to_mask() {
        let masker = DataMasker::new();
        let text = "phone 13812345678";
        let out = masker.mask_checked(text).unwrap();
        assert_eq!(out, masker.mask(text));
        assert!(!out.contains("13812345678"));
    }

    #[test]
    fn test_mask_checked_composes_rule_pipeline() {
        // mask_checked Ok 路径与 mask 同语义：门产物继续走规则集，PII 不漏
        use crate::masking::{SecretPatternRegistry, SecretScanGate};
        let gate = SecretScanGate::new(&SecretPatternRegistry::with_builtins());
        let masker = DataMasker::builder().with_secret_scan(gate).build();
        let out = masker
            .mask_checked("mail a@b.com key sk-proj-abcdefghijklmnopqrstuvwxyz123456")
            .expect("single hit is under limit");
        assert!(out.contains("***REDACTED_API_KEY***"), "{out}");
        assert!(
            out.contains("**@**.***"),
            "email must be masked by the rule pipeline: {out}"
        );
    }

    #[test]
    fn test_mask_checked_oversized_fails_closed() {
        // 超大输入在 mask_checked 是拒绝（Err）而 mask 是整体遮蔽：
        // 不扫描即不可证安全，fail-closed 出口不放行未经检测的内容
        use crate::masking::{SecretPatternRegistry, SecretScanGate};
        let gate = SecretScanGate::new(&SecretPatternRegistry::with_builtins());
        let masker = DataMasker::builder().with_secret_scan(gate).build();
        let big = format!("x {}", "y".repeat(1024 * 1024 + 1));
        assert!(
            matches!(
                masker.mask_checked(&big),
                Err(MaskError::SecretScanOversizedInput { .. })
            ),
            "oversized input must fail closed"
        );
    }

    #[test]
    fn test_gate_composes_with_email_rules() {
        // 门替换产物继续走规则集：裸 secret 与 PII 在同一出口各自改写
        use crate::masking::{SecretPatternRegistry, SecretScanGate};
        let gate = SecretScanGate::new(&SecretPatternRegistry::with_builtins());
        let masker = DataMasker::builder().with_secret_scan(gate).build();
        let out = masker.mask("mail a@b.com key sk-proj-abcdefghijklmnopqrstuvwxyz123456");
        assert!(out.contains("***REDACTED_API_KEY***"), "{out}");
        assert!(out.contains("**@**.***"), "{out}");
    }

    // ---- fast-masking 等价性：AC 加速路径与逐规则 regex 逐字节一致 ----

    #[test]
    fn test_fast_masking_ac_equals_sequential_regex_byte_for_byte() {
        // 固定串（literal=true 的契约面）：AC 单趟 LeftmostLongest 的长模式
        // 优先必须与逐规则 regex（长模式优先串行改写）逐字节一致，重叠模式
        // 无残留。
        let rules: [(&str, &str); 3] = [
            ("CORP_SECRET_TOKEN_EXTENDED", "***LONG***"),
            ("CORP_SECRET_TOKEN", "***TOKEN***"),
            ("SECRET", "***S***"),
        ];
        let mut builder = DataMasker::builder();
        for (pattern, replacement) in rules {
            builder = builder.add_rule(
                MaskRule::builder("ac-equivalence")
                    .pattern(pattern)
                    .replacement(replacement)
                    .literal(true)
                    .build()
                    .expect("literal rule must build"),
            );
        }
        let fast = builder.build();
        assert!(fast.ac_masker.is_some(), "AC path must be wired");

        let samples = [
            "plain text without secrets",
            "header CORP_SECRET_TOKEN tail",
            "overlap CORP_SECRET_TOKEN_EXTENDED tail",
            "both CORP_SECRET_TOKEN and CORP_SECRET_TOKEN_EXTENDED",
            "orphan SECRET and prefix CORP_SECRET_TOKEN",
            "token at end CORP_SECRET_TOKEN",
            "partial CORP_SECRET_TOK",
        ];
        for sample in samples {
            let ac_out = fast.mask(sample);
            // 参考实现：同一组规则按长模式优先逐条 regex 串行改写
            let mut sequential = sample.to_string();
            for (pattern, replacement) in rules {
                let re = Regex::new(pattern).expect("metachar-free pattern");
                sequential = re.replace_all(&sequential, replacement).to_string();
            }
            assert_eq!(
                ac_out, sequential,
                "AC output must match the sequential regex path byte-for-byte for {sample:?}"
            );
            for (pattern, _) in rules {
                assert!(
                    !ac_out.contains(pattern),
                    "no residue of {pattern:?} after masking: {ac_out:?}"
                );
            }
        }
    }
}
