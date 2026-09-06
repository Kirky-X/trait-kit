// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//
// feature 编译矩阵 / 结构契约 E2E 测试。
//
// 本文件【故意不设任何 cfg 门控与 required-features】：无论以哪种
// feature 组合编译（含 `--no-default-features`，PRS-01 门禁），本文件
// 都会被编译并运行，从而把「组合矩阵跑到了本测试」这一事实变成
// 每轮测试的天然副产品。
//
// 覆盖场景 ID（docs/TEST_SCENARIOS.md §2.22 / §2.14 / §2.19）：
// - PRS-01 `--no-default-features`（default=[]）core API 门禁测试化
// - PRS-02/04/05 13 个 feature 名单与依赖链展开断言（运行时解析
//   Cargo.toml [features] 固化，组合矩阵的实际逐组合执行见
//   reviews/acceptance-report.md 台账）
// - PRS-03 `--all-features` 门禁测试化（cfg 全 13 项正向断言段）
// - PRS-06 examples crate 20 个示例 required-features 门控结构核对
//   （逐示例编译执行见台账）
// - TGL-09 `src/kit/toggle.rs` doc-only 结构核对（无独立运行时）
// - PRE-03 派生宏不随 prelude 导出的文档契约核对
//
// 其余引用声明：PRS-01 的 no-feature 行为组 →
// tests/e2e_feature_combinations.rs::e2e_no_feature_*；PRS-04/05 的行为面
// → e2e_confers_plus_reload / e2e_confers_plus_encryption。

use std::collections::HashMap;
use trait_kit::impl_module_meta;
use trait_kit::prelude::*;

/// 解析 workspace 根 Cargo.toml 的 `[features]` 段（dep: 前缀剔除、
/// 引号与逗号规整）。供 PRS-02/04/05 的展开断言使用。
fn parse_features() -> HashMap<String, Vec<String>> {
    let manifest = include_str!("../../Cargo.toml");
    let mut features: HashMap<String, Vec<String>> = HashMap::new();
    let mut in_features = false;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed == "[features]" {
            in_features = true;
            continue;
        }
        if in_features && trimmed.starts_with('[') {
            break;
        }
        if !in_features || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((name, rhs)) = trimmed.split_once('=') else {
            continue;
        };
        // 取 `#` 前的部分（去行内注释）并去除首尾空白（等号后空格）。
        let rhs = rhs.split('#').next().unwrap_or("").trim();
        let deps: Vec<String> = rhs
            .trim_start_matches('[')
            .trim_end_matches(']')
            .split(',')
            .map(|s| {
                s.trim()
                    .trim_matches('"')
                    .trim()
                    .split("dep:")
                    .last()
                    .unwrap_or("")
                    .trim()
                    .to_string()
            })
            .filter(|s| !s.is_empty())
            .collect();
        features.insert(name.trim().to_string(), deps);
    }
    features
}

/// PRS-01 门禁测试化：无任何 feature 时 core API 编译可用。
/// 本文件在 `--no-default-features`（default=[]）下也会编译运行——
/// 跑到本测试即证明 no-default 组合存活。
#[test]
fn prs01_no_default_features_core_api_gate() {
    let mut kit = Kit::new();
    struct BareMod;
    impl_module_meta!(BareMod, "bare-mod");
    impl AutoBuilder for BareMod {
        type Capability = u32;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(1)
        }
    }
    kit.register::<BareMod>().unwrap();
    let ready = kit.build().unwrap();
    assert_eq!(ready.require::<BareMod>().unwrap(), 1);
    assert!(ready.graph_mermaid().contains("bare-mod"));
}

/// PRS-02：13 个 feature 名单完整（与 Cargo.toml [features] 一致）。
#[test]
fn prs02_feature_table_lists_all_thirteen() {
    let features = parse_features();
    let mut names: Vec<&String> = features.keys().collect();
    names.sort();
    let expected = [
        "async",
        "confers",
        "decorator",
        "encryption",
        "health",
        "i18n",
        "interface",
        "lifecycle",
        "observer",
        "reload",
        "scope",
        "shutdown",
        "toggle",
    ];
    for f in expected {
        assert!(
            features.contains_key(f),
            "Cargo.toml [features] 缺少 feature '{f}'：got {names:?}"
        );
    }
    // 除 13 个 feature 外仅允许空集 default（PRS-01 的可编译前提）。
    assert_eq!(
        features.get("default").map(Vec::is_empty),
        Some(true),
        "default 特性应为空集"
    );
    assert_eq!(
        names.len(),
        expected.len() + 1,
        "[features] 段应恰含 13 个 feature + default：got {names:?}"
    );
}

/// PRS-04：依赖链自动生效——`reload` 展开含 `confers` 与 `confers/watch`。
#[test]
fn prs04_reload_chain_expansion() {
    let features = parse_features();
    let reload = features.get("reload").expect("reload feature 应存在");
    assert!(
        reload.contains(&"confers".to_string()) && reload.contains(&"confers/watch".to_string()),
        "reload 应展开为 confers + confers/watch：got {reload:?}"
    );
}

/// PRS-05：依赖链自动生效——`encryption` 展开含 `confers` 与
/// `confers/encryption`（XChaCha20 原语经再导出可用，行为面见
/// tests/e2e_encryption.rs）。
#[test]
fn prs05_encryption_chain_expansion() {
    let features = parse_features();
    let enc = features
        .get("encryption")
        .expect("encryption feature 应存在");
    assert!(
        enc.contains(&"confers".to_string()) && enc.contains(&"confers/encryption".to_string()),
        "encryption 应展开为 confers + confers/encryption：got {enc:?}"
    );
}

/// PRS-03 门禁测试化：`--all-features` 下全 13 项激活。
/// 该测试仅在全 feature 组合编译时存在（cfg 段即门禁本体）。
#[cfg(all(
    feature = "async",
    feature = "confers",
    feature = "decorator",
    feature = "encryption",
    feature = "health",
    feature = "i18n",
    feature = "interface",
    feature = "lifecycle",
    feature = "observer",
    feature = "reload",
    feature = "scope",
    feature = "shutdown",
    feature = "toggle",
))]
#[test]
fn prs03_all_features_gate_reached() {
    // 能编译并执行到此处 = 13 项 feature 全部激活。
    let features = parse_features();
    assert_eq!(features.len(), 14, "13 feature + default 空集");
}

/// PRS-06：examples crate 20 个示例全部显式 `[[example]]` 注册，且
/// required-features 引用 examples crate 自身的 feature 名单
/// （逐示例 `cargo check -p trait-kit-examples --features <组合>` 的
/// 执行记录见 reviews/acceptance-report.md）。
#[test]
fn prs06_examples_manifest_required_features_structure() {
    let manifest = include_str!("../../examples/Cargo.toml");
    // 解析 [[example]] 段：name + required-features。
    let mut examples: Vec<(String, Option<Vec<String>>)> = Vec::new();
    let mut current: Option<(String, Option<Vec<String>>)> = None;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed == "[[example]]" {
            if let Some(done) = current.take() {
                examples.push(done);
            }
            current = Some((String::new(), None));
            continue;
        }
        let Some(entry) = current.as_mut() else {
            continue;
        };
        if trimmed.starts_with('[') {
            // 进入其他段，当前 example 结束。
            if let Some(done) = current.take() {
                examples.push(done);
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("name = ") {
            entry.0 = rest.trim_matches('"').to_string();
        } else if let Some(rest) = trimmed.strip_prefix("required-features = ") {
            let feats: Vec<String> = rest
                .trim_start_matches('[')
                .trim_end_matches(']')
                .split(',')
                .map(|s| s.trim().trim_matches('"').to_string())
                .filter(|s| !s.is_empty())
                .collect();
            entry.1 = Some(feats);
        }
    }
    if let Some(done) = current.take() {
        examples.push(done);
    }

    assert_eq!(
        examples.len(),
        20,
        "examples crate 应恰有 20 个 [[example]]"
    );
    // 解析 examples crate 自身 [features] 名单。
    let ex_manifest = include_str!("../../examples/Cargo.toml");
    let mut ex_features = Vec::new();
    let mut in_features = false;
    for line in ex_manifest.lines() {
        let t = line.trim();
        if t == "[features]" {
            in_features = true;
            continue;
        }
        if in_features && t.starts_with('[') {
            break;
        }
        if in_features
            && let Some((name, _)) = t.split_once('=')
        {
            ex_features.push(name.trim().to_string());
        }
    }

    for (name, required) in &examples {
        assert!(!name.is_empty(), "示例名不得为空");
        if let Some(feats) = required {
            assert!(
                !feats.is_empty(),
                "示例 {name} 声明了 required-features 却为空"
            );
            for f in feats {
                assert!(
                    ex_features.contains(f),
                    "示例 {name} 的 required-features 引用未知 feature '{f}'"
                );
            }
        }
    }
    // 核心域示例（无 feature 依赖）不加门控也能跑：default_basic 应存在。
    assert!(examples.iter().any(|(n, _)| n == "default_basic"));
}

/// TGL-09 结构核对：`src/kit/toggle.rs` 为 doc-only 模块（无独立运行时），
/// toggle 能力是 `Kit`/`Kit<Ready>` 上的方法（enable_toggle 等）。
/// 防止未来误判 API 面或误增独立注册表。
#[test]
fn tgl09_toggle_module_is_doc_only() {
    let toggle_src = include_str!("../../src/kit/toggle.rs");
    let line_count = toggle_src.lines().count();
    assert!(
        line_count <= 40,
        "toggle.rs 应保持 doc-only 规模（当前 {line_count} 行）"
    );
    assert!(
        !toggle_src.contains("pub fn "),
        "toggle.rs 不得引入独立运行时 API（doc-only 契约）"
    );
    assert!(
        !toggle_src.contains("pub struct") && !toggle_src.contains("pub enum"),
        "toggle.rs 不得引入独立类型（doc-only 契约）"
    );
}

/// PRE-03 编译面核对：派生宏（ConfigInherit/SharedConfig）不随 prelude
/// 导出——prelude.rs 无 trait_kit_derive 的 pub use（文档契约 NOTE 在位）。
#[test]
fn pre03_derive_macros_not_reexported_via_prelude() {
    let prelude = include_str!("../../src/prelude.rs");
    assert!(
        !prelude.contains("pub use trait_kit_derive::"),
        "prelude 不得导出 trait-kit-derive 派生宏（用户需显式依赖 derive crate）"
    );
    let manifest = include_str!("../../Cargo.toml");
    assert!(
        manifest.contains("trait-kit-derive"),
        "derive crate 应保持 workspace 成员"
    );
}
