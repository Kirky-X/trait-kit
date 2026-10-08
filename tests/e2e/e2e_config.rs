// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//
// 热重载 E2E 测试。
//
// 覆盖场景 ID（docs/TEST_SCENARIOS.md §2.6）：
// - RLD-06 订阅者 panic 语义固化：新值已存储、剩余订阅者被跳过
//   （panic 穿透 reload_config，文档化行为）
// - RLD-07 require_ref 借用存活期间 reload_config/set_config 的真实行为
//   （见文件尾部：场景描述按实现修正后固化）

#![cfg(feature = "reload")]

use std::cell::Cell;
use std::error::Error;
use std::rc::Rc;
use std::sync::Arc;
use trait_kit::impl_module_meta;
use trait_kit::prelude::*;

#[derive(Clone, Debug, PartialEq)]
struct PanicCfg {
    v: u32,
}
impl Configurable for PanicCfg {
    fn load() -> Result<Self, Box<dyn Error + Send>> {
        Ok(Self { v: 2 })
    }
}

#[test]
fn e2e_reload_subscriber_panic_stores_new_value_skips_rest() {
    let kit = Kit::new();
    kit.set_config(PanicCfg { v: 1 });

    let second_notified = Rc::new(Cell::new(false));
    let flag = Rc::clone(&second_notified);
    // 订阅者一（先注册）：重载回调内 panic。
    kit.subscribe::<PanicCfg>(|| panic!("subscriber boom"));
    // 订阅者二（后注册）：若被通知则置位。
    kit.subscribe::<PanicCfg>(move || flag.set(true));

    // panic 穿透 reload_config 传播给调用方。
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        kit.reload_config::<PanicCfg>().unwrap()
    }));
    assert!(result.is_err(), "订阅者 panic 应穿透 reload_config");

    // 文档化语义一：新值已在回调前存储。
    assert_eq!(
        kit.config::<PanicCfg>().unwrap().v,
        2,
        "panic 前 C::load() 的新值应已写入配置表"
    );
    // 文档化语义二：剩余订阅者被跳过。
    assert!(!second_notified.get(), "panic 之后的订阅者不应收到本次通知");

    // panic 后 Kit 状态一致：另一配置类型的订阅与重载正常工作
    // （不重触仍处订阅表中的 panic 回调）。
    let after = Rc::new(Cell::new(false));
    let a = Rc::clone(&after);
    kit.subscribe::<FreshCfg>(move || a.set(true));
    kit.reload_config::<FreshCfg>().unwrap();
    assert!(after.get(), "panic 后其他配置类型重载应正常通知订阅者");
}

#[derive(Clone, Debug, PartialEq)]
struct FreshCfg {
    v: u32,
}
impl Configurable for FreshCfg {
    fn load() -> Result<Self, Box<dyn Error + Send>> {
        Ok(Self { v: 9 })
    }
}

// ─── require_ref 借用与配置写入的真实行为 ───────────────────────

struct RefCapMod;
impl_module_meta!(RefCapMod, "ref-cap-mod");
impl AutoBuilder for RefCapMod {
    type Capability = Arc<u32>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
        Ok(Arc::new(42))
    }
}

/// RLD-07 真实行为固化（场景描述修正）：`require_ref` 借用的是
/// capabilities TypeMap（kit.rs），`reload_config`/`set_config` 写的是
/// configs TypeMap——两者为独立 RefCell，借用存活期间重载/写配置
/// 均正常工作，无 borrow 冲突。
///
/// 修正依据：TEST_SCENARIOS 编写时推测两者共享 inner_ref 会 panic；
/// 实现核实（kit.rs 字段布局）为两个独立 TypeMap。同 TypeMap 的
/// 借用冲突语义由 src/kit/typemap.rs::inner_ref_panics_if_mutably_borrowed
/// 在层内固化；本测试防止未来合并两个 TypeMap 时引入隐蔽 panic。
#[test]
fn e2e_require_ref_borrow_survives_reload_and_set_config() {
    let mut kit = Kit::new();
    kit.register::<RefCapMod>().unwrap();
    kit.set_config(FreshCfg { v: 1 });
    let ready = kit.build().unwrap();

    // 借用存活期间：重载（configs 写 + 订阅通知）正常执行。
    // （set_config 为 Unbuilt 态方法；Ready 态写配置的唯一路径是
    // reload_config——这正是本次固化的真实写路径。）
    let guard = ready.require_ref::<RefCapMod>().unwrap();
    ready.reload_config::<FreshCfg>().unwrap();
    assert_eq!(
        ready.config::<FreshCfg>().unwrap().v,
        9,
        "借用存活期间 reload_config 应正常更新配置"
    );

    // capabilities 侧借用不受 configs 写影响，读到原能力值。
    assert_eq!(**guard, 42, "require_ref 借用应读到完整能力值");
    drop(guard);

    // 释放借用后再次重载，Kit 状态一致。
    ready.reload_config::<FreshCfg>().unwrap();
    assert_eq!(ready.config::<FreshCfg>().unwrap().v, 9);
}

// ─── BuildReport::to_json → serde_json 解析往返序列化（report 落点） ────

#[cfg(feature = "report")]
mod build_report_json_roundtrip_e2e {
    use std::sync::Arc;
    use trait_kit::impl_module_meta;
    use trait_kit::prelude::*;

    struct RptEagerDep;
    impl_module_meta!(RptEagerDep, "rpt-json-dep");
    impl AutoBuilder for RptEagerDep {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(1))
        }
    }

    struct RptJsonTop;
    impl_module_meta!(RptJsonTop, "rpt-json-top", deps = [RptEagerDep]);
    impl AutoBuilder for RptJsonTop {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(2))
        }
    }

    struct RptJsonLazy;
    impl_module_meta!(RptJsonLazy, "rpt-json-lazy");
    impl AutoBuilder for RptJsonLazy {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            Ok(Arc::new(3))
        }
    }

    struct RptJsonOverridden;
    impl_module_meta!(RptJsonOverridden, "rpt-json-overridden");
    impl AutoBuilder for RptJsonOverridden {
        type Capability = Arc<u32>;
        type Error = TraitKitError;
        fn build(_kit: &Kit) -> Result<Self::Capability, TraitKitError> {
            panic!("override 注入后 build_fn 不应被调用")
        }
    }

    /// eager/lazy/override 三态 + merge_config 记录的 build_report 经
    /// to_json → serde_json 解析往返：JSON 合法且 modules/topo_order/
    /// overrides/config_overrides 字段齐全可回读（消费方落盘/上报通路）。
    #[test]
    fn e2e_build_report_json_roundtrip_includes_all_sections() {
        use trait_kit::kit::{ConfigInherit, Kit};

        // 手写 ConfigInherit（Override 全 None 默认 + 部分 merge）。
        #[derive(Clone, Debug, PartialEq)]
        struct RptMergeCfg {
            host: String,
        }
        #[derive(Clone, Default)]
        struct RptMergeCfgOverride {
            host: Option<String>,
        }
        impl ConfigInherit for RptMergeCfg {
            type Override = RptMergeCfgOverride;
            fn apply_override(&mut self, ovr: &Self::Override) {
                if let Some(host) = &ovr.host {
                    self.host = host.clone();
                }
            }
        }

        let mut kit = Kit::new();
        kit.register::<RptEagerDep>().unwrap();
        kit.register::<RptJsonTop>().unwrap();
        kit.register_lazy::<RptJsonLazy>().unwrap();
        kit.override_module::<RptJsonOverridden>(Arc::new(42));
        kit.set_config(RptMergeCfg {
            host: "orig".into(),
        });
        kit.merge_config::<RptMergeCfg>(RptMergeCfgOverride {
            host: Some("rpt-host".into()),
        });

        let ready = kit.build().expect("build 应成功");
        // 触发 lazy 首建，使报告后读到的状态完整（报告为 Unbuilt→Ready
        // 构建面快照，lazy 首建不改变其 modules 记录）。
        let _ = ready.require::<RptJsonLazy>().unwrap();

        let json = ready.build_report().to_json().expect("序列化应成功");
        let value = serde_json::from_str::<serde_json::Value>(&json).expect("往返解析应成功");

        assert_eq!(value["schema_version"], 1);
        // topo_order：依赖先于消费者。
        let topo = value["topo_order"].as_array().expect("topo 数组");
        let topo_names: Vec<&str> = topo.iter().filter_map(|v| v.as_str()).collect();
        assert!(topo_names.contains(&"rpt-json-dep"));
        assert!(topo_names.contains(&"rpt-json-top"));
        let dep_pos = topo.iter().position(|v| v == "rpt-json-dep").unwrap();
        let top_pos = topo.iter().position(|v| v == "rpt-json-top").unwrap();
        assert!(dep_pos < top_pos, "topo 序应满足依赖先于消费者：{topo:?}");

        // modules：eager 带 elapsed、lazy/overridden 各一条。
        let modules = value["modules"].as_array().expect("modules 数组");
        assert!(
            modules.len() >= 3,
            "报告应含 eager+lazy+override 模块：{modules:?}"
        );
        let eager = modules
            .iter()
            .find(|m| m["name"] == "rpt-json-top")
            .expect("eager 条目");
        assert_eq!(eager["state"], "built");
        assert!(eager["elapsed_us"].as_u64().is_some());
        let lazy = modules
            .iter()
            .find(|m| m["name"] == "rpt-json-lazy")
            .expect("lazy 条目");
        assert_eq!(lazy["state"], "lazy");

        // overrides：module-level override 记录来源。
        let overrides = value["overrides"].as_array().expect("overrides 数组");
        assert!(
            overrides
                .iter()
                .any(|o| o["module"] == "rpt-json-overridden"),
            "override 记录应出现在报告中：{overrides:?}"
        );

        // config_overrides：merge_config 记录 applied==true。
        let cfg_overrides = value["config_overrides"].as_array().expect("cfg 数组");
        assert_eq!(cfg_overrides.len(), 1, "应恰有一条 merge_config 记录");
        assert_eq!(cfg_overrides[0]["applied"], true);

        // total_elapsed_us 往返可读。
        assert!(value["total_elapsed_us"].as_u64().is_some());
    }
}

// ─── load_config_with ${VAR} 插值的 $$ 转义形态（CFG-08 矩阵补全） ────

/// e2e 层固化 `$$` 转义矩阵：`$${VAR}` → 字面 `${VAR}` 不插值、相邻
/// 转义与插值混用（实现契约 src/kit/config.rs interpolate_string；
/// 单元级 5 用例见 src 内 interpolate_string_tests）。
#[test]
fn e2e_load_config_with_double_dollar_escape_forms() {
    use std::collections::HashMap;

    #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct EscapeCfg {
        escaped_var: String,
        literal_dollar: String,
        mixed: String,
        still_interpolates: String,
    }
    impl Configurable for EscapeCfg {
        fn load() -> Result<Self, Box<dyn std::error::Error + Send>> {
            Ok(Self {
                escaped_var: "$${HOST}".into(),
                literal_dollar: "cost: $$5".into(),
                mixed: "$${HOST} and ${HOST}".into(),
                still_interpolates: "after $$ then ${HOST}".into(),
            })
        }
    }

    let mut vars = HashMap::new();
    vars.insert("HOST".to_string(), "db.internal".to_string());

    let kit = Kit::new();
    kit.load_config_with::<EscapeCfg, _>(&vars)
        .expect("加载应成功");
    let cfg = kit.config::<EscapeCfg>().expect("配置应已存入");
    assert_eq!(
        cfg.escaped_var, "${HOST}",
        "$$转义应产出字面占位文本，不插值"
    );
    assert_eq!(cfg.literal_dollar, "cost: $5", "$$ 应折叠为字面 $");
    assert_eq!(
        cfg.mixed, "${HOST} and db.internal",
        "转义与插值混用应各归各"
    );
    assert_eq!(
        cfg.still_interpolates, "after $ then db.internal",
        "转义后的后续占位符仍应插值"
    );
}
