// Copyright (c) 2026 Kirky.X🌠
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
// - 04/05 13 个 feature 名单与依赖链展开断言（运行时解析
//   Cargo.toml [features] 固化，组合矩阵的实际逐组合执行见
//   reviews/acceptance-report.md 台账）
// - PRS-03 `--all-features` 门禁测试化（cfg 全 13 项正向断言段）
// - PRS-06 examples crate 20 个示例 required-features 门控结构核对
//   （逐示例编译执行见台账）
// - TGL-09 `src/kit/toggle.rs` doc-only 结构核对（无独立运行时）
// - PRE-03 派生宏不随 prelude 导出的文档契约核对
//
// 其余引用声明：PRS-01 的 no-feature 行为组 →
// tests/e2e_feature_combinations.rs::e2e_no_feature_*；05 的行为面
// → e2e_confers_plus_reload / e2e_confers_plus_encryption。

use std::collections::HashMap;
use trait_kit::impl_module_meta;
use trait_kit::prelude::*;

/// 解析 workspace 根 Cargo.toml 的 `[features]` 段（dep: 前缀剔除、
/// 引号与逗号规整）。供 04/05 的展开断言使用。
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

/// feature 名单完整（与 Cargo.toml [features] 一致）。
/// rc4 批次新增 report/presets/compose/presets-remote/negotiate 五项（13 → 18）；
/// rc5 批次新增 di/request-scope/version-negotiation 三项（18 → 21）。
#[test]
fn prs02_feature_table_lists_all_features() {
    let features = parse_features();
    let mut names: Vec<&String> = features.keys().collect();
    names.sort();
    let expected = [
        "async",
        "compose",
        "confers",
        "decorator",
        "di",
        "encryption",
        "health",
        "i18n",
        "interface",
        "lifecycle",
        "negotiate",
        "observer",
        "presets",
        "presets-remote",
        "probe",
        "reload",
        "report",
        "request-scope",
        "scope",
        "shutdown",
        "toggle",
        "version-negotiation",
    ];
    for f in expected {
        assert!(
            features.contains_key(f),
            "Cargo.toml [features] 缺少 feature '{f}'：got {names:?}"
        );
    }
    // 除 22 个 feature 外仅允许空集 default（PRS-01 的可编译前提；数量
    // 与 prs02 期望表一致）。
    assert_eq!(
        features.get("default").map(Vec::is_empty),
        Some(true),
        "default 特性应为空集"
    );
    assert_eq!(
        names.len(),
        expected.len() + 1,
        "[features] 段应恰含 22 个 feature + default：got {names:?}"
    );
}

/// 依赖链自动生效——`reload` 展开含 `confers`（已剪除 `confers/watch` 死链）。
#[test]
fn prs04_reload_chain_expansion() {
    let features = parse_features();
    let reload = features.get("reload").expect("reload feature 应存在");
    assert!(
        reload.contains(&"confers".to_string()),
        "reload 应展开为 confers：got {reload:?}"
    );
    assert!(
        !reload.contains(&"confers/watch".to_string()),
        "reload 不应再含 confers/watch（已剪除死链）：got {reload:?}"
    );
}

/// 依赖链自动生效——`encryption` 展开含 `confers` 与
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

/// PRS-03 门禁测试化：`--all-features` 下全部 feature 激活（清单与计数以
/// `prs02` 的期望表为单一事实源；cfg 段必须逐一列全部非别名 feature——
/// 漏列一项该组合就会静默通过，门禁失效）。
/// 该测试仅在全 feature 组合编译时存在（cfg 段即门禁本体）。
#[cfg(all(
    feature = "async",
    feature = "compose",
    feature = "confers",
    feature = "decorator",
    feature = "encryption",
    feature = "health",
    feature = "i18n",
    feature = "di",
    feature = "lifecycle",
    feature = "version-negotiation",
    feature = "observer",
    feature = "probe",
    feature = "presets",
    feature = "presets-remote",
    feature = "reload",
    feature = "report",
    feature = "request-scope",
    feature = "shutdown",
    feature = "toggle",
))]
#[test]
fn prs03_all_features_gate_reached() {
    // 能编译并执行到此处 = 22 项 feature 全部激活（cfg 列表覆盖全部非
    // 别名 feature，数量与下一行断言的「22 feature + default」一致）。
    let features = parse_features();
    assert_eq!(features.len(), 23, "22 feature + default 空集");
}

/// examples crate 20 个示例全部显式 `[[example]]` 注册，且
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
        if in_features && let Some((name, _)) = t.split_once('=') {
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

/// TGL-09 结构核对：`src/kit/toggle.rs` 为完整开关句柄模块，
/// 包含 `ToggleBackend` trait、`ToggleValue` 枚举、`MemoryToggle` 实现，
/// 以及 `ConfersToggle`（confers feature 启用时）。
#[test]
fn tgl09_toggle_module_is_implemented() {
    let toggle_src = include_str!("../../src/kit/toggle.rs");
    // Must contain the core trait and types
    assert!(
        toggle_src.contains("pub trait ToggleBackend"),
        "toggle.rs 应包含 ToggleBackend trait"
    );
    assert!(
        toggle_src.contains("pub enum ToggleValue"),
        "toggle.rs 应包含 ToggleValue 枚举"
    );
    assert!(
        toggle_src.contains("pub struct MemoryToggle"),
        "toggle.rs 应包含 MemoryToggle 实现"
    );
    // ConfersToggle present when confers feature is enabled
    assert!(
        toggle_src.contains("pub struct ConfersToggle"),
        "toggle.rs 应包含 ConfersToggle（confers 后端）"
    );
}

/// PRE-03 编译面核对：派生宏（Module/ConfigInherit/SharedConfig）不随
/// prelude 导出——prelude.rs 无 trait_kit_macros 的 pub use（用户显式
/// 依赖 trait-kit-macros 使用派生宏，宏包为 opt-in）。
#[test]
fn pre03_derive_macros_not_reexported_via_prelude() {
    let prelude = include_str!("../../src/prelude.rs");
    assert!(
        !prelude.contains("pub use trait_kit_macros::"),
        "prelude 不得导出 trait-kit-macros 派生宏（宏包保持 opt-in，不强制 syn 依赖）"
    );
    let manifest = include_str!("../../Cargo.toml");
    assert!(
        manifest.contains("trait-kit-macros"),
        "宏包应保持 workspace 成员（derive 宏的唯一提供方）"
    );
}

// ─── presets 注册面异常（ProviderNotInjected / 二次注册） ──────────────

// 本段全部消费者均挂 presets 门；import 同门，其余组合下编译期闲置。
#[cfg(feature = "presets")]
use std::sync::Arc;

/// confers ConfigValue 的简单内存 provider mock（镜像 src 内部测试形态）。
#[cfg(feature = "presets")]
struct PresetMapProvider {
    pairs: std::collections::HashMap<String, confers::AnnotatedValue>,
}

#[cfg(feature = "presets")]
impl PresetMapProvider {
    fn from_pairs<const N: usize>(pairs: [(&str, confers::ConfigValue); N]) -> Arc<Self> {
        let map = pairs
            .into_iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    confers::AnnotatedValue::new(v, confers::SourceId::default(), k),
                )
            })
            .collect();
        Arc::new(Self { pairs: map })
    }
}

#[cfg(feature = "presets")]
impl confers::ConfigProvider for PresetMapProvider {
    fn get_raw(&self, key: &str) -> Option<&confers::AnnotatedValue> {
        self.pairs.get(key)
    }

    fn keys(&self) -> Vec<String> {
        self.pairs.keys().cloned().collect()
    }
}

/// 跳过 register_confers_config 直接 register ConfersConfigModule →
/// build 失败且 source 为 ProviderNotInjected（Display 含指引文本）；
/// register_confers_config 成功后二次注册 → AlreadyRegistered。
#[cfg(feature = "presets")]
#[test]
fn e2e_presets_provider_not_injected_and_duplicate_registration() {
    use trait_kit::kit::presets::{ConfersConfigModule, PresetError, register_confers_config};

    // 分支一：未注入 provider 直接 register。
    let mut kit = Kit::new();
    kit.register::<ConfersConfigModule>()
        .expect("register 本身应成功");
    let err = kit.build().expect_err("无 provider 构建必须失败");
    match &err {
        TraitKitError::BuildFailed { context, source } => {
            assert_eq!(context, "confers-config", "context 应为模块 NAME");
            let preset_err = source
                .downcast_ref::<PresetError>()
                .expect("source 应为 PresetError::ProviderNotInjected");
            assert!(matches!(preset_err, PresetError::ProviderNotInjected));
        }
        other => panic!("expected BuildFailed, got: {other:?}"),
    }
    assert!(
        err.to_string().contains("register_confers_config"),
        "Display 应含注入指引文本：got '{err}'"
    );

    // 分支二：成功注入后再注册同模块 → AlreadyRegistered。
    let provider = PresetMapProvider::from_pairs([("k", confers::ConfigValue::String("v".into()))]);
    let mut kit = Kit::new();
    register_confers_config(&mut kit, provider).expect("首次注册应成功");
    let provider2 =
        PresetMapProvider::from_pairs([("k", confers::ConfigValue::String("v".into()))]);
    let err = register_confers_config(&mut kit, provider2)
        .expect_err("二次注册必须返回 AlreadyRegistered");
    assert!(matches!(err, TraitKitError::AlreadyRegistered { .. }));
}

// ─── presets-remote 异常面（RemoteLoad / 无 slot ProviderNotInjected） ──

#[cfg(all(feature = "presets-remote", feature = "async"))]
mod presets_remote_error_e2e {
    use std::sync::Arc;
    use std::task::{Context, Poll, Waker};
    use trait_kit::kit::presets::PresetError;
    use trait_kit::kit::presets::remote::{
        ConfersRemoteConfigModule, register_confers_remote_config,
    };
    use trait_kit::prelude::*;

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(v) => return v,
                Poll::Pending => std::hint::spin_loop(),
            }
        }
    }

    /// load 返回 Err 的远端源。
    struct FailingSource;
    #[async_trait::async_trait]
    impl confers::interface::AsyncSource for FailingSource {
        async fn load(&self) -> confers::ConfigResult<confers::AnnotatedValue> {
            Err(confers::ConfigError::FileNotFound {
                filename: std::path::PathBuf::from("e2e-remote-mock"),
                source: None,
            })
        }
        fn source_id(&self) -> &confers::SourceId {
            static ID: std::sync::OnceLock<confers::SourceId> = std::sync::OnceLock::new();
            ID.get_or_init(confers::SourceId::default)
        }
        fn name(&self) -> &str {
            "e2e-failing-source"
        }
    }

    /// AsyncSource::load 失败 → build().await 失败，错误文本含源错误，
    /// source 链下探到 PresetError::RemoteLoad。
    #[test]
    fn e2e_presets_remote_source_failure_fails_build() {
        let mut kit = AsyncKit::new();
        register_confers_remote_config(&mut kit, Arc::new(FailingSource)).expect("注册助手应成功");
        let err = block_on(kit.build()).expect_err("远端源失败必须使 build 失败");
        assert!(
            err.to_string().contains("e2e-remote-mock"),
            "错误文本应携带源错误信息：got '{err}'"
        );
        if let TraitKitError::BuildFailed { source, .. } = &err {
            let preset_err = source.downcast_ref::<PresetError>();
            assert!(
                matches!(preset_err, Some(PresetError::RemoteLoad { .. })),
                "source 应为 PresetError::RemoteLoad：got {preset_err:?}"
            );
        } else {
            panic!("expected BuildFailed, got: {err:?}");
        }
    }

    /// 无 RemoteSourceSlot 配置直接 register → ProviderNotInjected 分支。
    #[test]
    fn e2e_presets_remote_without_slot_reports_provider_not_injected() {
        let mut kit = AsyncKit::new();
        kit.register::<ConfersRemoteConfigModule>()
            .expect("register 应成功");
        let err = block_on(kit.build()).expect_err("无 slot 构建必须失败");
        match &err {
            TraitKitError::BuildFailed { source, .. } => {
                let preset_err = source
                    .downcast_ref::<PresetError>()
                    .expect("source 应为 PresetError");
                assert!(matches!(preset_err, PresetError::ProviderNotInjected));
            }
            other => panic!("expected BuildFailed, got: {other:?}"),
        }
    }
}

/// 类型化访问器防错：get_string 于 int 值、get_int 于 string 值、
/// 未知 key 均返回 None，不 panic。
#[cfg(feature = "presets")]
#[test]
fn e2e_presets_handle_typed_accessors_return_none_on_mismatch() {
    use trait_kit::kit::presets::{ConfersConfigModule, register_confers_config};

    let provider = PresetMapProvider::from_pairs([
        ("str.key", confers::ConfigValue::String("text".into())),
        ("int.key", confers::ConfigValue::I64(42)),
    ]);
    let mut kit = Kit::new();
    register_confers_config(&mut kit, provider).expect("register preset");
    let ready = kit.build().expect("build ok");
    let handle = ready.require::<ConfersConfigModule>().expect("require");

    assert_eq!(
        handle.get_string("int.key"),
        None,
        "get_string 于 int 值应返回 None"
    );
    assert_eq!(
        handle.get_int("str.key"),
        None,
        "get_int 于 string 值应返回 None"
    );
    assert_eq!(
        handle.get_string("absent.key"),
        None,
        "未知 key 应返回 None"
    );
    assert_eq!(handle.get_int("int.key"), Some(42));
    assert_eq!(handle.get_string("str.key").as_deref(), Some("text"));
}
