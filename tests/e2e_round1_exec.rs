// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Round-1 E2E executor addendum: the two scenarios no shipped test covers
//! at the asked scale — macro-vs-handwritten build-error propagation parity,
//! and the over-deep config guard driven with a 5000-layer nested value.

use std::sync::Arc;

use trait_kit::impl_module_meta;
use trait_kit::prelude::*;

const PARITY_SOURCE: &str = "macro parity failure";

/// Hand-written `ModuleMeta` module whose `build` fails.
struct ManualFailing;
impl trait_kit::core::ModuleMeta for ManualFailing {
    const NAME: &'static str = "manual-failing";
}
impl AutoBuilder for ManualFailing {
    type Capability = Arc<u32>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Err(TraitKitError::MissingCapability {
            key: PARITY_SOURCE.into(),
        })
    }
}

/// `impl_module_meta!`-generated meta, identical failing `build`.
struct MacroFailing;
impl_module_meta!(MacroFailing, "macro-failing");
impl AutoBuilder for MacroFailing {
    type Capability = Arc<u32>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Err(TraitKitError::MissingCapability {
            key: PARITY_SOURCE.into(),
        })
    }
}

/// `#[derive(Module)]`-generated meta, identical failing `build`.
#[derive(trait_kit_macros::Module)]
#[module(name = "derive-failing")]
struct DeriveFailing;
impl AutoBuilder for DeriveFailing {
    type Capability = Arc<u32>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Err(TraitKitError::MissingCapability {
            key: PARITY_SOURCE.into(),
        })
    }
}

/// 场景 1：宏生成代码（`impl_module_meta!` / `#[derive(Module)]`）与手写
/// `ModuleMeta` 的 `build` 同样返回 Err 时，`build()` 的错误传播路径必须
/// 一致——同为 `BuildFailed{context=NAME}`、source 保留原错误文本，
/// 不吞错、不因 meta 来源不同而包装出不同型别。
#[test]
fn macro_meta_build_err_propagates_identically_to_manual() {
    for (form, expected_name) in [
        ("manual", "manual-failing"),
        ("impl_module_meta!", "macro-failing"),
        ("derive(Module)", "derive-failing"),
    ] {
        let mut kit = Kit::new();
        match form {
            "manual" => kit.register::<ManualFailing>().unwrap(),
            "impl_module_meta!" => kit.register::<MacroFailing>().unwrap(),
            _ => kit.register::<DeriveFailing>().unwrap(),
        }
        let err = kit
            .build()
            .expect_err("a failing build must surface as an error for every meta form");
        match err {
            TraitKitError::BuildFailed { context, source } => {
                assert_eq!(
                    &*context, expected_name,
                    "context must be the module NAME for the {form} form"
                );
                assert!(
                    source.to_string().contains(PARITY_SOURCE),
                    "the {form} form must preserve the original error, got: {source}"
                );
            }
            other => panic!("expected BuildFailed for the {form} form, got: {other:?}"),
        }
    }
}

#[cfg(feature = "confers")]
mod over_deep_config {
    use super::*;
    use std::collections::HashMap;

    /// 场景 23：5000 层嵌套配置（现网仅 200 层驱动过该守卫）→
    /// `load_config_with` 返回可捕获的 `BuildFailed`（深度守卫在序列化
    /// 阶段中止），而非进程因栈耗尽而终止。
    #[derive(Clone, serde::Serialize, serde::Deserialize)]
    struct Deep5kCfg {
        nested: serde_json::Value,
    }

    impl ModuleConfig for Deep5kCfg {
        const PATH: &'static str = "test/deep-5k-cfg";
        fn default_value() -> Self {
            Self {
                nested: serde_json::Value::Null,
            }
        }
    }

    impl Configurable for Deep5kCfg {
        fn load() -> Result<Self, Box<dyn std::error::Error + Send + 'static>> {
            let mut nested = serde_json::Value::String("leaf".into());
            for _ in 0..5000 {
                let mut map = serde_json::Map::new();
                map.insert("n".to_string(), nested);
                nested = serde_json::Value::Object(map);
            }
            Ok(Self { nested })
        }
    }

    /// 场景 21：校验失败 → `BuildFailed{context="load_and_validate"}`
    /// （source=ValidationError 聚合全部原因）且配置不写入。
    /// 现网测试只断言 `is_err()`，未固定未存储与聚合两条语义。
    #[derive(Clone, serde::Serialize, serde::Deserialize)]
    struct RejectTwoCfg {
        v: u32,
    }

    impl ModuleConfig for RejectTwoCfg {
        const PATH: &'static str = "test/reject-two-cfg";
        fn default_value() -> Self {
            Self { v: 0 }
        }
    }

    impl Configurable for RejectTwoCfg {
        fn load() -> Result<Self, Box<dyn std::error::Error + Send + 'static>> {
            Ok(Self { v: 1 })
        }
    }

    impl Validatable for RejectTwoCfg {
        fn validate(&self) -> Result<(), Vec<String>> {
            Err(vec!["first reason".into(), "second reason".into()])
        }
    }

    #[test]
    fn load_and_validate_reject_aggregates_reasons_and_stores_nothing() {
        let kit = Kit::new();
        let err = kit
            .load_and_validate::<RejectTwoCfg>()
            .expect_err("a rejecting validate() must fail load_and_validate");
        match err {
            TraitKitError::BuildFailed { context, source } => {
                assert_eq!(&*context, "load_and_validate");
                let text = source.to_string();
                assert!(
                    text.contains("first reason") && text.contains("second reason"),
                    "the source must aggregate every validation reason: {text}"
                );
            }
            other => panic!("expected BuildFailed, got: {other:?}"),
        }
        let ready = kit
            .build()
            .expect("a kit without modules should still build");
        assert!(
            !ready.contains_config::<RejectTwoCfg>(),
            "a rejected load_and_validate must not store the config type"
        );
    }

    #[test]
    fn load_config_with_5000_layer_nesting_returns_catchable_error() {
        let kit = Kit::new();
        let vars = HashMap::<String, String>::new();
        let err = kit
            .load_config_with::<Deep5kCfg, _>(&vars)
            .expect_err("5000-layer nesting must be rejected as an error, not kill the process");
        match err {
            TraitKitError::BuildFailed { context, source } => {
                assert_eq!(
                    &*context, "load_config_with (serialize)",
                    "the depth guard must abort in the serialize phase"
                );
                assert!(
                    source.to_string().contains("nesting depth"),
                    "the source must report the depth limit: {source}"
                );
            }
            other => panic!("expected BuildFailed, got: {other:?}"),
        }
    }
}
