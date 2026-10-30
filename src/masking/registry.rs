// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! # 脱敏规则注册中心
//!
//! 提供 [`MaskRuleRegistry`] 用于管理内置和自定义脱敏规则。
//! 支持规则的增删改查、启用/禁用、优先级排序，以及从 TOML 配置加载自定义规则。

use super::engine::MaskRule;
use super::error::MaskError;

/// 脱敏规则注册中心，管理内置和自定义规则。
///
/// 本类型不做跨线程共享：不含内部同步，多线程需要各自的
/// [`DataMasker`](super::engine::DataMasker) 时按既有模式把规则快照
/// 克隆进各自的 masker（`DataMasker::builder().with_registry(registry)`）。
///
/// # Example
///
/// ```rust
/// use trait_kit::masking::MaskRuleRegistry;
///
/// let mut registry = MaskRuleRegistry::with_builtins();
/// assert!(registry.active_rules().len() >= 21);
///
/// // 禁用某规则
/// registry.set_enabled("email", false);
///
/// // 获取活跃规则（按优先级排序）
/// let active = registry.active_rules();
/// ```
#[derive(Debug, Clone, Default)]
pub struct MaskRuleRegistry {
    rules: Vec<MaskRule>,
}

impl MaskRuleRegistry {
    /// 创建包含所有内置规则的注册中心。
    ///
    /// 内置规则包含 21 条预定义脱敏规则，涵盖邮箱、电话、身份证、
    /// 银行卡、信用卡、IP 地址、MAC 地址、护照号、SSN、数据库连接串、
    /// API 密钥、AWS 密钥、JWT、GitHub/Slack/Stripe/Google 令牌、私钥等。
    #[must_use]
    pub fn with_builtins() -> Self {
        use super::engine::DataMasker;
        let masker = DataMasker::new();
        Self {
            rules: masker.into_rules(),
        }
    }

    /// 注册自定义规则。
    ///
    /// # Errors
    /// 若已存在同名规则，返回 `Err(MaskError)`。
    pub fn register(&mut self, rule: MaskRule) -> Result<(), MaskError> {
        if self.rules.iter().any(|r| r.name() == rule.name()) {
            return Err(MaskError::ConfigError(format!(
                "rule '{}' already registered",
                rule.name()
            )));
        }
        self.rules.push(rule);
        // 内部存储保持 priority 升序（稳定排序，同优先级保留注册顺序），
        // 作为 rules 有序快照的不变量；active_rules 自行排序，不受影响
        self.rules.sort_by_key(super::engine::MaskRule::priority);
        Ok(())
    }

    /// 按名称移除规则，返回被移除的规则。
    pub fn remove(&mut self, name: &str) -> Option<MaskRule> {
        if let Some(pos) = self.rules.iter().position(|r| r.name() == name) {
            Some(self.rules.remove(pos))
        } else {
            None
        }
    }

    /// 启用或禁用指定规则。
    ///
    /// 返回操作是否成功（规则是否存在）。
    pub fn set_enabled(&mut self, name: &str, enabled: bool) -> bool {
        if let Some(rule) = self.rules.iter_mut().find(|r| r.name() == name) {
            rule.set_enabled(enabled);
            true
        } else {
            false
        }
    }

    /// 获取所有活跃规则（已启用），按 priority 升序排列。
    #[must_use]
    pub fn active_rules(&self) -> Vec<&MaskRule> {
        let mut active: Vec<&MaskRule> = self.rules.iter().filter(|r| r.is_enabled()).collect();
        active.sort_by_key(|r| r.priority());
        active
    }

    /// 全量规则快照（含已禁用），按 priority 升序排列。
    ///
    /// 与 [`active_rules()`](Self::active_rules) 的差别：本方法不过滤
    /// `enabled`，适合规则清单导出、诊断展示等需要看到禁用项的场景。
    #[must_use]
    pub fn rules(&self) -> &[MaskRule] {
        &self.rules
    }

    /// 从既有规则集合构造注册中心。
    ///
    /// 规则按 priority 升序（稳定排序）重排为内部存储；不做重名查重，
    /// 重名约束由调用方保证（如 [`load_from_toml()`](Self::load_from_toml)
    /// 的输出已满足）——重名规则的后果是检测面按名归因不可区分
    /// （[`MaskMatch`](super::engine::MaskMatch).rule 同名），debug 构建
    /// 下直接断言暴露违约。
    #[must_use]
    pub fn from_rules(mut rules: Vec<MaskRule>) -> Self {
        debug_assert!(
            {
                let mut names: Vec<&str> =
                    rules.iter().map(super::engine::MaskRule::name).collect();
                names.sort_unstable();
                names.dedup();
                names.len() == rules.len()
            },
            "{}",
            "from_rules: duplicate rule names make detect attribution ambiguous"
        );
        rules.sort_by_key(super::engine::MaskRule::priority);
        Self { rules }
    }

    /// 返回注册中心所有规则的数量（含禁用）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// 注册中心是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// 从 TOML 字符串解析自定义规则定义。
    ///
    /// TOML 格式：
    /// ```toml
    /// [[masking_rules]]
    /// name = "custom_id"
    /// pattern = "\\bCUSTOM-\\d{6}\\b"
    /// replacement = "***CUSTOM***"
    /// priority = 100
    /// enabled = true
    /// ```
    ///
    /// 重复的 `name` 与 [`register()`](Self::register) 的查重语义对齐：
    /// 跳过后出现的定义并记录 `tracing::warn`，首个定义生效。
    ///
    /// # Errors
    /// - TOML 解析失败返回 `Err(MaskError)`
    /// - 缺少 `name` 或 `pattern` 字段返回 `Err(MaskError)`
    /// - 无效正则在构建规则时返回 `Err(MaskError)`
    pub fn load_from_toml(toml_str: &str) -> Result<Vec<MaskRule>, MaskError> {
        let parsed: toml::Value = toml::from_str(toml_str)
            .map_err(|e| MaskError::ConfigError(format!("failed to parse TOML: {e}")))?;

        let rules_tables = parsed
            .get("masking_rules")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                MaskError::ConfigError("TOML missing '[[masking_rules]]' array".to_string())
            })?;

        let mut rules: Vec<MaskRule> = Vec::new();
        for table in rules_tables {
            let name = table.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                MaskError::ConfigError("masking_rules entry missing 'name' field".to_string())
            })?;
            if rules.iter().any(|r| r.name() == name) {
                tracing::warn!(
                    rule = name,
                    "{}",
                    "duplicate masking rule name in TOML config; keeping the first definition and skipping the later one"
                );
                continue;
            }
            let pattern = table
                .get("pattern")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    MaskError::ConfigError(format!(
                        "masking_rules entry '{name}' missing 'pattern' field"
                    ))
                })?;
            let replacement = table
                .get("replacement")
                .and_then(|v| v.as_str())
                .unwrap_or("***REDACTED***");
            let priority_raw = table
                .get("priority")
                .and_then(toml::Value::as_integer)
                .unwrap_or(100);
            // Clamp i64 to i32 range to avoid silent wrapping on extreme values;
            // the cast is exact after the clamp
            #[allow(clippy::cast_possible_truncation)]
            let priority = priority_raw.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
            let enabled = table
                .get("enabled")
                .and_then(toml::Value::as_bool)
                .unwrap_or(true);

            let rule = MaskRule::builder(name)
                .pattern(pattern)
                .replacement(replacement)
                .priority(priority)
                .enabled(enabled)
                .build()?;
            rules.push(rule);
        }

        Ok(rules)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_builtins() {
        let registry = MaskRuleRegistry::with_builtins();
        assert!(registry.len() >= 21);
        assert!(!registry.is_empty());
    }

    #[test]
    fn test_register_and_remove() {
        let mut registry = MaskRuleRegistry::default();
        let rule = MaskRule::builder("custom")
            .pattern(r"\d+")
            .replacement("***")
            .build()
            .unwrap();
        assert!(registry.register(rule).is_ok());
        assert_eq!(registry.len(), 1);

        // Duplicate name should fail
        let dup = MaskRule::builder("custom")
            .pattern(r"\w+")
            .replacement("###")
            .build()
            .unwrap();
        assert!(registry.register(dup).is_err());

        // Remove
        let removed = registry.remove("custom");
        assert!(removed.is_some());
        assert_eq!(registry.len(), 0);

        // Remove non-existent
        assert!(registry.remove("nonexistent").is_none());
    }

    #[test]
    fn test_set_enabled() {
        let mut registry = MaskRuleRegistry::with_builtins();
        let initial_active = registry.active_rules().len();

        // Disable email rule
        assert!(registry.set_enabled("email", false));
        assert_eq!(registry.active_rules().len(), initial_active - 1);

        // Re-enable
        assert!(registry.set_enabled("email", true));
        assert_eq!(registry.active_rules().len(), initial_active);

        // Non-existent rule
        assert!(!registry.set_enabled("nonexistent", false));
    }

    #[test]
    fn test_active_rules_sorted_by_priority() {
        let registry = MaskRuleRegistry::with_builtins();
        let active = registry.active_rules();
        for window in active.windows(2) {
            assert!(window[0].priority() <= window[1].priority());
        }
    }

    #[test]
    fn test_load_from_toml() {
        let toml_str = r#"
[[masking_rules]]
name = "custom_id"
pattern = "\\bCUSTOM-\\d{6}\\b"
replacement = "***CUSTOM***"
priority = 100
enabled = true

[[masking_rules]]
name = "another_rule"
pattern = "\\bTEST-\\w+\\b"
"#;
        let rules = MaskRuleRegistry::load_from_toml(toml_str).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].name(), "custom_id");
        assert_eq!(rules[0].priority(), 100);
        assert!(rules[0].is_enabled());
        assert_eq!(rules[1].name(), "another_rule");
        // Default priority and enabled
        assert_eq!(rules[1].priority(), 100);
        assert!(rules[1].is_enabled());
    }

    #[test]
    fn test_load_from_toml_missing_name() {
        let toml_str = r#"
[[masking_rules]]
pattern = "\\d+"
"#;
        assert!(MaskRuleRegistry::load_from_toml(toml_str).is_err());
    }

    #[test]
    fn test_load_from_toml_invalid_regex() {
        let toml_str = r#"
[[masking_rules]]
name = "bad"
pattern = "[invalid"
"#;
        assert!(MaskRuleRegistry::load_from_toml(toml_str).is_err());
    }

    #[test]
    fn test_load_from_toml_malformed_syntax_fails() {
        // TOML 语法级错误（区别于缺字段的语义错误）
        let result = MaskRuleRegistry::load_from_toml("not [ valid toml ==");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("TOML") || err.to_string().contains("toml"),
            "parse error should mention TOML, got: {err}"
        );
    }

    #[test]
    fn test_load_from_toml_missing_pattern_fails() {
        let toml_str = r#"
[[masking_rules]]
name = "no_pattern_rule"
replacement = "x"
"#;
        let err = MaskRuleRegistry::load_from_toml(toml_str).unwrap_err();
        assert!(
            err.to_string().contains("no_pattern_rule"),
            "missing-pattern error must name the offending rule, got: {err}"
        );
    }

    #[test]
    fn test_load_from_toml_missing_name_fails() {
        let toml_str = r#"
[[masking_rules]]
pattern = "\\bNAMELESS-\\d+\\b"
"#;
        assert!(MaskRuleRegistry::load_from_toml(toml_str).is_err());
    }

    #[test]
    fn test_load_from_toml_missing_array() {
        let toml_str = "[other]\nkey = \"value\"\n";
        assert!(MaskRuleRegistry::load_from_toml(toml_str).is_err());
    }

    #[test]
    fn test_load_from_toml_skips_duplicate_names() {
        // 与 register 的查重语义对齐：重复 name 跳过后者（首个定义生效）
        let toml_str = r#"
[[masking_rules]]
name = "dup_rule"
pattern = "\\bFIRST-\\d+\\b"
replacement = "first"

[[masking_rules]]
name = "dup_rule"
pattern = "\\bSECOND-\\d+\\b"
replacement = "second"

[[masking_rules]]
name = "other_rule"
pattern = "\\bOTHER-\\d+\\b"
"#;
        let rules = MaskRuleRegistry::load_from_toml(toml_str).unwrap();
        assert_eq!(rules.len(), 2, "duplicate name must be skipped");
        assert_eq!(rules[0].name(), "dup_rule");
        // 首个定义生效：replacement 保持 "first"
        assert!(
            format!("{:?}", rules[0]).contains(r#"replacement: "first""#),
            "first definition should win, got: {:?}",
            rules[0]
        );
        assert_eq!(rules[1].name(), "other_rule");
    }

    #[test]
    fn test_rules_returns_full_snapshot_including_disabled() {
        let mut registry = MaskRuleRegistry::with_builtins();
        let total = registry.len();
        assert!(registry.set_enabled("email", false));

        // rules 是全量快照：禁用的规则仍在，len 不变
        assert_eq!(registry.rules().len(), total);
        assert!(registry.rules().iter().any(|r| r.name() == "email"));
        // active_rules 只含已启用的
        assert_eq!(registry.active_rules().len(), total - 1);
    }

    #[test]
    fn test_rules_sorted_by_priority() {
        let registry = MaskRuleRegistry::with_builtins();
        let snapshot = registry.rules();
        for window in snapshot.windows(2) {
            assert!(
                window[0].priority() <= window[1].priority(),
                "snapshot must be priority-ascending: {} > {}",
                window[0].priority(),
                window[1].priority()
            );
        }
    }

    #[test]
    fn test_from_rules_sorts_and_keeps_disabled() {
        let low = MaskRule::builder("low_p")
            .pattern(r"\bLOWP-\d+\b")
            .priority(500)
            .build()
            .unwrap();
        let high = MaskRule::builder("high_p")
            .pattern(r"\bHIGHP-\d+\b")
            .priority(1)
            .build()
            .unwrap();
        let disabled = MaskRule::builder("disabled_p")
            .pattern(r"\bDISP-\d+\b")
            .priority(10)
            .enabled(false)
            .build()
            .unwrap();

        let registry = MaskRuleRegistry::from_rules(vec![low, disabled, high]);
        let snapshot = registry.rules();

        assert_eq!(snapshot.len(), 3, "disabled rules must be kept");
        assert_eq!(snapshot[0].name(), "high_p");
        assert_eq!(snapshot[1].name(), "disabled_p");
        assert_eq!(snapshot[2].name(), "low_p");
        assert!(!snapshot[1].is_enabled());
    }

    #[test]
    fn test_register_maintains_sorted_snapshot() {
        let mut registry = MaskRuleRegistry::with_builtins();
        let late = MaskRule::builder("late_rule")
            .pattern(r"\bLATE-\d+\b")
            .priority(10_000)
            .build()
            .unwrap();
        registry.register(late).unwrap();

        for window in registry.rules().windows(2) {
            assert!(window[0].priority() <= window[1].priority());
        }
        // 后注册的规则也能以全量快照读到
        assert!(registry.rules().iter().any(|r| r.name() == "late_rule"));
    }
}
