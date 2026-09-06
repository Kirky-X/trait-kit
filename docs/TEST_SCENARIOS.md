# trait-kit 验收测试场景穷举矩阵

> 适用版本：trait-kit **0.5.0-rc.2**（workspace 根，Rust 1.97.1 / edition 2024）
> 用途：验收工程第一步 —— 先穷举全部验收场景，后续按本文档逐条固化为 `tests/e2e/` 下的 E2E 测试。
> 编写依据（只读核对）：`Cargo.toml [features]`（13 个 feature，无 default）、`src/lib.rs` 导出面、`src/core`（meta/macros/health/lifecycle/observer）、`src/kit`（kit/graph/typemap/config/scope/toggle/shutdown/async_kit/async_typemap）、`src/i18n`（FTL 目录 + ICU4X）、`trait-kit-derive`（ConfigInherit/SharedConfig）、`tests/`（7 个文件）、`tests/ui/`（3 个 trybuild 用例）、`examples/`（20 个示例）。
> 所有引用的既有测试名均经 `grep` 核实存在。
> 组合兼容性经 `cargo check --all-features` 实测通过（exit 0）。
>
> **落地对账（阶段 2 完成时回填）**：
> 1. §2 全部场景已落地 `tests/e2e/`（13 个文件，根 `Cargo.toml` 显式 `[[test]]` 注册，目标名 `e2e_*`）；文中“无→需新增 / …→需新增断言”标记已全部清零，逐场景执行结果见 `reviews/acceptance-report.md` 阶段 2 台账。
> 2. **真实行为核正 2 处**（RLD-07、CCY-06）：原推测 `require_ref` 借用期间 `reload_config`/`set_config` 会 borrow 冲突 panic——实现核实 `configs` 与 `capabilities` 为两个独立 `TypeMap`（`src/kit/kit.rs`），借用存活期间配置写入正常工作；已按真实行为固化（防回归：若未来合并两 TypeMap，测试将失败并暴露隐蔽 panic）。另 `set_config` 为 `Kit<Unbuilt>` 态方法，Ready 态写配置唯一路径是 `reload_config`。
> 3. **CCY-03 等效落地**：未走 trybuild（stderr 快照依赖 feature 集，组合矩阵下脆弱），以 `tests/basic.rs:11-12` 的 `static_assertions::assert_not_impl_any!(Kit<Unbuilt>: Sync)`（含 `Kit<Ready>`）编译期断言等效固化。
> 4. 既有场景引用声明：各 e2e 文件头注释已标注「本文件新增落地 vs 既有覆盖引用」的逐场景归属。

## 阅读约定

- **类型**：`正常`（合法输入下的预期行为）/ `异常`（错误输入、违规操作，须给出明确错误或编译失败）/ `边界`（极限值、并发、竞态、组合临界、语义固化）。
- **既有覆盖**：`文件::测试名` 表示已有测试（`src/**` 内联测试模块不写 `src/` 前缀直接给 `相对路径::测试名`）；标注 `→需新增断言` 表示已有单测或示例但缺集成/行为级断言；`无→需新增` 表示无既有覆盖。
- **E2E 落点**：计划写入 `tests/e2e/` 的目标文件（后续落地阶段创建）；typestate 编译期违规落点为 `tests/ui/`。
- **依赖服务**：全部场景为 `无`（纯内存库，无 docker / 无外部进程依赖，理由见 §4）。
- feature 名一律使用 `Cargo.toml [features]` 原名；`Kit`/`Kit<Ready>` 指 typestate 两态。

### 重要发现（编写时盘点得出）

1. **`src/kit/toggle.rs` 是 doc-only 模块（10 行）**：toggle 能力并非独立运行时，而是实现为 `Kit`/`Kit<Ready>` 上的方法（`enable_toggle` / `is_toggle_enabled` / `register_if_toggle`），底层为 `RefCell<HashMap<String, bool>>`。验收时按 Kit 方法口径测，不存在独立注册表 API。
2. **sync `Kit` 为 `RefCell` 实现（`!Sync`），`AsyncKit` 为 `AsyncTypeMap` 实现（`Send + Sync`）**：两条产品线并发模型不同。sync 线所有 API 仅限单线程使用；负向 `!Sync` 断言目前缺失（CCY-03）。
3. **`tests/e2e_feature_combinations.rs` 与 `tests/e2e_advanced.rs` 内大量测试用 `#[cfg(all(test, feature = …))]` 门控**：裸 `cargo test`（无 feature）只执行 no-feature 组；验收必须按 feature 矩阵分别跑 `cargo test --features <组合>`，否则组合组静默不编译、不执行。
4. **typestate 违规目前只有 3 个 trybuild 用例守卫**（`tests/ui/ready_cannot_build.rs`、`ready_cannot_register.rs`、`unbuilt_cannot_optional.rs`），由 `tests/compile_fail.rs` 单 runner 驱动；Ready 态其余违规面（如 Ready 上 `override_module_strict` 不存在等）随 API 增长需补 UI 用例。
5. `tests/e2e_advanced.rs` 头部注明其场景 ID 沿用 `temp/feature-analysis.md` 的 B/A/E/C 系列（B=基础、A=高级、E=错误、C=覆盖率补盲），本矩阵已将其全部吸纳并重编号。
6. examples crate（`trait-kit-examples`）每个示例显式 `[[example]]` 注册并带 `required-features`，默认不编译；跑示例必须传对应 feature。
7. **无 `autotests = false`**：`tests/` 下新增 E2E 文件会被自动发现，无需增补 `[[test]]` 段（与 confers 不同）；但 feature 门控文件需自带 `#![cfg(feature = "…")]` 头。（落地更正：E2E 文件最终移入 `tests/e2e/` 子目录承载；子目录内文件不被自动发现，已在根 `Cargo.toml` 显式注册 13 个 `[[test]]` 目标，目标名保持 `e2e_*`，zh/en i18n 仍为独立进程二进制。）

---

## 1. 总览：功能域 × feature 分组

| # | 功能域 | 场景 ID 前缀 | 涉及 feature（Cargo.toml 名） | 主要公共 API |
|---|--------|-------------|------------------------------|--------------|
| 1 | 模块元数据与声明宏 | MET | —（MET-04/07/08 需 async、interface） | `core::{ModuleMeta, AutoBuilder, Interface, InterfaceBuilder}`、宏 `impl_module_meta!` / `impl_auto_builder!` / `impl_async_auto_builder!` |
| 2 | 注册与构建（typestate） | REG | — | `kit::{Kit, Unbuilt, Ready}`、`register / register_lazy / register_multi / register_if / override_module / override_module_strict / build` |
| 3 | 能力获取 | CAP | — | `Kit::require / optional / require_ref / require_all / contains / contains_config / factory` |
| 4 | 依赖图 | DEP | — | `kit::{DependencyGraph, GraphError, ModuleEntry}`、`graph_dot / graph_mermaid` |
| 5 | 配置中心 | CFG | confers | `Configurable / ModuleConfig / Validatable / ValidationError / ConfigInherit / SharedConfig`、`set_config / config / load_config / load_and_validate / load_config_with / load_config_or_default / snapshot_config / restore_config / has_snapshot / populate_defaults / merge_config / extract_shared / inject_shared`、自由函数 `interpolate_json_value / merge_json_deep`、派生宏 `ConfigInherit / SharedConfig`（trait-kit-derive） |
| 6 | 热重载 | RLD | reload（→confers→confers/watch） | `Kit::subscribe / reload_config` |
| 7 | 加密存储 | ENC | encryption（→confers→confers/encryption） | `Kit::set_encrypted / get_encrypted / contains_encrypted`、`EncryptedBlob`、再导出 `XChaCha20Crypto / derive_field_key` |
| 8 | 接口/实现分离 | ITF | interface | `register_as / resolve`、`core::{Interface, InterfaceBuilder}` |
| 9 | 生命周期 | LCY | lifecycle（×async） | `core::{Lifecycle, AsyncLifecycle}`、`register_lifecycle / shutdown` |
| 10 | 健康检查 | HLT | health（×async） | `core::{HealthCheck, AsyncHealthCheck, HealthStatus}`、`register_health_check / health_check / health_report` |
| 11 | 构建观察者 | OBS | observer（×async） | `core::BuildObserver`、`with_observer` |
| 12 | 装饰器 | DEC | decorator（×async） | `Kit::decorate`（覆盖 eager/lazy/multi/interface 四条构建路径） |
| 13 | 作用域 | SCP | scope（×async） | `kit::{Scope, AsyncScope}`、`create_scope`、`Scope::register / require / contains` |
| 14 | 特性开关 | TGL | toggle | `Kit::enable_toggle / is_toggle_enabled / register_if_toggle`（Unbuilt 与 Ready 两态） |
| 15 | 优雅关闭 | SHD | shutdown（×async） | `kit::{ShutdownCoordinator, ShutdownPhase, ShutdownPhaseResult, ShutdownResult, AsyncShutdownCoordinator}` |
| 16 | 异步 Kit | ASK | async | `AsyncKit / AsyncAutoBuilder / AsyncReady / AsyncUnbuilt / AsyncTypeMap`、`AsyncKit::register / build / require / factory / create_scope` |
| 17 | 国际化 | I18 | i18n | `i18n::{tr, I18nManager, I18nFormatter, I18nError}`、FTL 目录（en/zh）、ICU4X number/date/plural/collation |
| 18 | 错误体系 | ERR | —（部分变体按 feature 门控） | `TraitKitError`（8 变体）/ `TraitKitResult`，Display 经 `tr()` 走 Fluent 目录 |
| 19 | prelude 导出面 | PRE | —（随各 feature） | `prelude::*` 再导出一致性 |
| 20 | feature 组合交互 | CMP | 多 feature 叠加 | — |
| 21 | 并发与竞态 | CCY | async + 全库 | — |
| 22 | feature 编译矩阵 | PRS | 13 项全量 | `cargo check --features …` |

场景总数：**238**（正常 100 / 异常 57 / 边界 80，另含 1 条正常/异常双断言，程序化核对见 §6）。

---

## 2. 场景矩阵

### 2.1 模块元数据与声明宏（MET，9 条）

| ID | 场景描述 | 类型 | 涉及 feature | 依赖服务 | 既有覆盖 | E2E 落点 |
|----|---------|------|-------------|---------|---------|---------|
| MET-01 | 手写 `ModuleMeta`：`NAME` 生效、`dependencies()` 默认返回空切片 | 正常 | — | 无 | `src/core/meta.rs::default_dependencies_returns_empty_slice`、`src/core/meta.rs::mock_logger_module_dependencies_empty` | tests/e2e/e2e_core.rs |
| MET-02 | `impl_module_meta!(T, "name", deps = [A, B])` 生成 name/依赖对与手写 impl 完全一致（名称+TypeId 逐项相等） | 正常 | — | 无 | `src/core/macros.rs::macro_generates_correct_name_with_deps`、`macro_dependency_names_match_module_meta_names`、`macro_dependency_type_ids_match_hand_written` | tests/e2e/e2e_core.rs |
| MET-03 | `impl_auto_builder!` 生成完整 `AutoBuilder`（Capability/Error/build 与手写 impl 逐项一致） | 正常 | — | 无 | `src/core/macros.rs::macro_sync_*`（8 例，含 `macro_sync_build_returns_expected_capability`） | tests/e2e/e2e_core.rs |
| MET-04 | `impl_async_auto_builder!` 生成 `AsyncAutoBuilder`：`build` 返回 `Pin<Box<dyn Future>>`、Capability `Send+Sync`、Error `Send+'static` | 正常 | async | 无 | `src/core/macros.rs::macro_async_*`（9 例）、`src/core/meta.rs::async_auto_builder_returns_pin_box_future`、`async_auto_builder_capability_is_send_sync`、`async_auto_builder_error_is_send_static` | tests/e2e/e2e_async.rs |
| MET-05 | 宏生成代码对 `build` 返回 `Err` 的传播路径与手写 impl 一致（不吞错、不包装） | 异常 | — | 无 | `src/core/macros.rs::macro_sync_build_propagates_errors`、`macro_async_build_propagates_errors` | tests/e2e/e2e_core.rs |
| MET-06 | 无依赖宏形态（两参形式）生成空依赖，与手写空实现等价 | 边界 | — | 无 | `src/core/macros.rs::macro_generates_empty_dependencies_when_no_deps`、`macro_name_equals_hand_written_name` | tests/e2e/e2e_core.rs |
| MET-07 | `Interface` marker 对全部 `'static` 类型自动实现（含 `?Sized` trait object、原始类型、自定义类型） | 正常 | interface | 无 | `src/core/meta.rs::interface_auto_implemented_for_primitive_types`、`_custom_types`、`_reference_types` | tests/e2e/e2e_features.rs |
| MET-08 | `InterfaceBuilder` 关联 Capability/Interface，不要求实现 `AutoBuilder`，Interface 可为 `dyn Trait` | 正常 | interface | 无 | `src/core/meta.rs::interface_builder_does_not_require_autobuilder`、`interface_builder_interface_type_is_dyn_compatible`、`interface_builder_into_interface_produces_trait_object` | tests/e2e/e2e_features.rs |
| MET-09 | `a25` 同一模块一次验证三种声明形态（手写/两参宏/deps 宏）注册后行为一致 | 边界 | — | 无 | `tests/e2e_advanced.rs::a25_impl_module_meta_macro_three_forms` | tests/e2e/e2e_core.rs |

### 2.2 注册与构建 / typestate（REG，20 条）

| ID | 场景描述 | 类型 | 涉及 feature | 依赖服务 | 既有覆盖 | E2E 落点 |
|----|---------|------|-------------|---------|---------|---------|
| REG-01 | 空 `Kit::new()` 直接 `build()` 成功，得到空 `Kit<Ready>` | 正常 | — | 无 | `tests/e2e_advanced.rs::b01_empty_kit_build_succeeds`、`c01_empty_dependency_graph_build_succeeds` | tests/e2e/e2e_core.rs |
| REG-02 | `register::<M>()` → `build()` → `require::<M>()` 取回能力，全链路 | 正常 | — | 无 | `tests/basic.rs::test_basic_build_and_require` | tests/e2e/e2e_core.rs |
| REG-03 | 依赖解析：A 依赖 B 时 B 先构建，依赖能力可在 A::build 内取得 | 正常 | — | 无 | `tests/basic.rs::test_dependency_resolution`、`tests/e2e_advanced.rs::b04_chain_dependency_topological_build`、`c03_deep_dependency_chain_10_layers_builds_in_topo_order` | tests/e2e/e2e_core.rs |
| REG-04 | 注册模块依赖未注册模块 → `build()` 返回 `DependencyMissing{module, missing}` | 异常 | — | 无 | `tests/basic.rs::test_missing_dependency_error`、`src/kit/graph.rs::graph_validate_missing_dependency` | tests/e2e/e2e_core.rs |
| REG-05 | 两节点互赖 → `build()` 返回 `CycleDetected`，cycle 含两模块名 | 异常 | — | 无 | `tests/basic.rs::test_cycle_detection`、`kit_build_returns_cycle_detected_for_mutual_deps` | tests/e2e/e2e_core.rs |
| REG-06 | 三节点环与自依赖（A→A）均被判环 | 异常 | — | 无 | `tests/e2e_advanced.rs::e03_three_node_cycle_detected`、`c02_self_dependency_detected_as_cycle` | tests/e2e/e2e_core.rs |
| REG-07 | 重复注册：同模块两次 `register`，以及 `register`/`register_lazy`/`register_multi` 跨方法重复 → `AlreadyRegistered` | 异常 | — | 无 | `tests/basic.rs::test_duplicate_registration_error`、`tests/e2e_advanced.rs::e05_cross_method_duplicate_registration` | tests/e2e/e2e_core.rs |
| REG-08 | `build` 回调返回 `Err` → `build()` 失败，`BuildFailed{context=模块名, source=原错误}`；multi-binding 构建失败同型 | 异常 | — | 无 | `tests/e2e_advanced.rs::e08_build_fn_returns_err_propagates_build_failed`、`src/kit/kit.rs::multi_binding_build_error_returns_build_failed` | tests/e2e/e2e_core.rs |
| REG-09 | `register_lazy` 不在 `build()` 期构建（计数器为零），仅入图校验 | 正常 | — | 无 | `src/kit/kit.rs::register_lazy_does_not_build_during_build`、`register_lazy_adds_to_dependency_graph`、`tests/e2e_advanced.rs::a01_register_lazy_first_require_triggers_build` | tests/e2e/e2e_core.rs |
| REG-10 | lazy 模块首次 `require()` 触发构建，二次 `require()` 返回 OnceLock 缓存不重建 | 边界 | — | 无 | `tests/e2e_advanced.rs::a02_lazy_module_not_rebuilt_on_second_require`、`c11_build_then_require_lazy_twice_returns_cached`、`src/kit/kit.rs::require_does_not_rebuild_lazy_on_second_call` | tests/e2e/e2e_core.rs |
| REG-11 | lazy 构建（首次 require）失败 → `BuildFailed` 传播给调用方 | 异常 | — | 无 | `tests/e2e_advanced.rs::e11_lazy_build_fails_on_first_require`、`src/kit/kit.rs::lazy_require_build_error` | tests/e2e/e2e_core.rs |
| REG-12 | `register_multi` 同一 Capability 类型三实现聚合，`require_all` 保注册顺序 | 正常 | — | 无 | `tests/e2e_advanced.rs::a04_register_multi_require_all_preserves_order`、`c13_multi_binding_registration_order_preserved`、`src/kit/kit.rs::require_all_preserves_registration_order` | tests/e2e/e2e_core.rs |
| REG-13 | multi 绑定与单绑定 `register` 共存：`require` 取单例、`require_all` 取聚合，互不干扰 | 边界 | — | 无 | `tests/e2e_advanced.rs::a05_multi_binding_coexists_with_single_binding`、`src/kit/kit.rs::require_all_coexists_with_require_for_single_binding` | tests/e2e/e2e_core.rs |
| REG-14 | multi 重复注册同一模块 / 已 `register` 过的模块再 `register_multi` → `AlreadyRegistered` | 异常 | — | 无 | `tests/e2e_advanced.rs::e06_register_multi_duplicate_returns_already_registered`、`src/kit/kit.rs::register_multi_returns_already_registered_if_already_registered_via_register` | tests/e2e/e2e_core.rs |
| REG-15 | `register_if` 谓词真/假双分支：真则注册、假则跳过且返回 false | 正常 | — | 无 | `src/kit/kit.rs::register_if_true_registers_module`、`register_if_false_skips_module`；examples/conditional（运行验收） | tests/e2e/e2e_core.rs |
| REG-16 | `override_module` 注入预构建能力，跳过 `build_fn`（模块无需先注册）；未注册 override 在 build 尾部补插 | 正常 | — | 无 | `tests/e2e_advanced.rs::a06_override_module_skips_build_fn`、`a07_override_module_on_unregistered_module`、`src/kit/kit.rs::build_inserts_unregistered_override_after_topo_loop` | tests/e2e/e2e_core.rs |
| REG-17 | `override_module_strict` 依赖未注册 → `DependencyMissing`；依赖齐备则成功 | 异常 | — | 无 | `tests/e2e_advanced.rs::e19_override_module_strict_missing_dep_returns_dependency_missing`、`a08_override_module_strict_succeeds_when_deps_registered` | tests/e2e/e2e_core.rs |
| REG-18 | overrides 表生命周期：new 后为空、build 后清空（消费语义） | 边界 | — | 无 | `src/kit/kit.rs::overrides_field_is_empty_on_new`、`overrides_field_is_empty_after_build` | tests/e2e/e2e_core.rs |
| REG-19 | typestate 编译期违规：`Kit<Ready>` 上调 `build()`/`register()`、`Kit<Unbuilt>` 上调 `optional()` 均编译失败（E0119/E0599 级错误，stderr 快照锁定） | 异常 | — | 无 | `tests/compile_fail.rs::compile_fail_tests`（驱动 `tests/ui/ready_cannot_build.rs`、`ready_cannot_register.rs`、`unbuilt_cannot_optional.rs` 三例） | tests/ui/（新增 ui 用例） |
| REG-20 | 规模边界：菱形依赖、10 层深链、100 模块注册+构建全部成功且次序正确 | 边界 | — | 无 | `tests/e2e_advanced.rs::c04_diamond_dependency_builds_successfully`、`c03_deep_dependency_chain_10_layers_builds_in_topo_order`、`c06_large_number_of_modules_100_registers_and_builds` | tests/e2e/e2e_core.rs |

### 2.3 能力获取（CAP，12 条）

| ID | 场景描述 | 类型 | 涉及 feature | 依赖服务 | 既有覆盖 | E2E 落点 |
|----|---------|------|-------------|---------|---------|---------|
| CAP-01 | `require::<M>()` 返回构建单例的克隆，多次调用值相等 | 正常 | — | 无 | `tests/basic.rs::test_basic_build_and_require`、`src/kit/async_kit.rs::async_kit_ready_require_returns_capability` | tests/e2e/e2e_core.rs |
| CAP-02 | `require` 未注册/未构建模块 → `MissingCapability{key=NAME}`（sync 侧与 async 侧同型） | 异常 | — | 无 | `src/kit/scope.rs::scope_require_unregistered_returns_missing`（同型）、`src/kit/kit.rs::require_ref_returns_missing_capability_for_unbuilt`；sync `require` 直接断言→需新增断言 | tests/e2e/e2e_core.rs |
| CAP-03 | `optional`：已构建返回 `Some`，未构建返回 `None` 不报错 | 正常 | — | 无 | `tests/basic.rs::test_optional_missing`、`tests/e2e_advanced.rs::b09_optional_returns_some_for_built_module`、`src/kit/kit.rs::ready_optional_returns_none_for_unbuilt` | tests/e2e/e2e_core.rs |
| CAP-04 | `require_ref` 返回零拷贝 `Ref`，读到与 `require` 一致的值（含 override 值） | 正常 | — | 无 | `tests/e2e_advanced.rs::b13_require_ref_returns_zero_copy_reference`、`src/kit/kit.rs::require_ref_returns_reference_to_built_capability`、`require_ref_returns_override_value` | tests/e2e/e2e_core.rs |
| CAP-05 | `require_ref` 未构建 → `MissingCapability` | 异常 | — | 无 | `src/kit/kit.rs::require_ref_returns_missing_for_unbuilt` | tests/e2e/e2e_core.rs |
| CAP-06 | `require_all` 返回全部 multi 绑定（3 个）按注册顺序 | 正常 | — | 无 | `src/kit/kit.rs::require_all_returns_vec_of_three_after_three_register_multi` | tests/e2e/e2e_core.rs |
| CAP-07 | `require_all` 未注册 Capability → `MissingCapability` | 异常 | — | 无 | `tests/e2e_advanced.rs::e26_require_all_unregistered_returns_missing_capability`、`src/kit/kit.rs::require_all_returns_empty_for_unregistered_capability` | tests/e2e/e2e_core.rs |
| CAP-08 | `build()` 之前调用 `require_all` → `MissingCapability`（multi_capabilities 仅 build 后填充） | 边界 | — | 无 | `src/kit/kit.rs::require_all_returns_missing_capability_before_build` | tests/e2e/e2e_core.rs |
| CAP-09 | `contains` / `contains_config` 精确反映已构建/已设置状态 | 正常 | — | 无 | `tests/e2e_advanced.rs::b11_contains_reflects_built_state`、`b12_contains_config_reflects_set_state`、`src/kit/kit.rs::ready_contains_returns_false_for_unbuilt` | tests/e2e/e2e_core.rs |
| CAP-10 | `factory::<M>()` 每次调用产生全新实例（非单例，计数可证） | 正常 | — | 无 | `src/kit/kit.rs::factory_creates_new_instance_each_call`、`src/kit/async_kit.rs::async_factory_creates_new_instances`；examples/factory（运行验收） | tests/e2e/e2e_core.rs |
| CAP-11 | factory 构建失败 → `BuildFailed{context=NAME}` 传播（闭包内 Err 路径） | 异常 | — | 无 | 无→需新增 | tests/e2e/e2e_core.rs |
| CAP-12 | `build` 回调内 `require` 依赖模块能力（DI 注入路径）与读取 config 同用 | 正常 | — | 无 | `tests/e2e_advanced.rs::b07_build_callback_reads_config`、`src/kit/kit.rs::require_lazy_with_registered_dependency_succeeds` | tests/e2e/e2e_core.rs |

### 2.4 依赖图（DEP，10 条）

| ID | 场景描述 | 类型 | 涉及 feature | 依赖服务 | 既有覆盖 | E2E 落点 |
|----|---------|------|-------------|---------|---------|---------|
| DEP-01 | `DependencyGraph::add` 保序入表，`entries()` 反映注册顺序 | 正常 | — | 无 | `src/kit/graph.rs::graph_add_and_entries`、`tests/basic.rs::entries_returns_registration_order` | tests/e2e/e2e_core.rs |
| DEP-02 | 重复 `add` 同一 TypeId → 返回 `Err(name)` | 异常 | — | 无 | `src/kit/graph.rs::graph_add_duplicate_returns_err`、`tests/basic.rs::add_rejects_duplicate_type_id` | tests/e2e/e2e_core.rs |
| DEP-03 | `validate` 对无环图输出合法拓扑序 | 正常 | — | 无 | `src/kit/graph.rs::graph_validate_topo_order`、`tests/basic.rs::validate_succeeds_for_acyclic_graph` | tests/e2e/e2e_core.rs |
| DEP-04 | `validate` 缺失依赖 → `GraphError::DependencyMissing` | 异常 | — | 无 | `src/kit/graph.rs::graph_validate_missing_dependency`、`tests/basic.rs::validate_returns_dependency_missing_for_unknown_dep` | tests/e2e/e2e_core.rs |
| DEP-05 | `validate` 环检测：两节点/三节点环 + 从未访问分支进入环均可检出 | 异常 | — | 无 | `src/kit/graph.rs::graph_validate_cycle_two_nodes`、`graph_validate_cycle_three_nodes`、`tests/basic.rs::find_cycle_traverses_unvisited_branch` | tests/e2e/e2e_core.rs |
| DEP-06 | 空图/单节点 validate 通过 | 边界 | — | 无 | `src/kit/graph.rs::graph_validate_empty_succeeds`、`graph_validate_single_node` | tests/e2e/e2e_core.rs |
| DEP-07 | `name_of` 命中已注册/未知返回 None；`dependency_names` 未知模块返回空 | 正常 | — | 无 | `src/kit/graph.rs::graph_name_of`、`graph_dependency_names_unknown_returns_empty`、`tests/basic.rs::name_of_returns_registered_name` | tests/e2e/e2e_core.rs |
| DEP-08 | `to_dot` 生成 Graphviz DOT（空图/带点边两态合法输出） | 正常 | — | 无 | `src/kit/graph.rs::graph_to_dot_with_nodes_and_edges`、`src/kit/kit.rs::graph_dot_returns_valid_string` | tests/e2e/e2e_core.rs |
| DEP-09 | `to_mermaid` 生成流程图；模块名含连字符时节点 id 无碰撞 | 正常 | — | 无 | `src/kit/graph.rs::graph_to_mermaid_hyphen_names_no_collision`、`tests/e2e_feature_combinations.rs::e2e_no_feature_graph_export` | tests/e2e/e2e_core.rs |
| DEP-10 | `GraphError` Debug/Display 可格式化（错误链可用） | 边界 | — | 无 | `src/kit/graph.rs::graph_error_debug` | tests/e2e/e2e_core.rs |

### 2.5 配置中心（CFG，22 条）

| ID | 场景描述 | 类型 | 涉及 feature | 依赖服务 | 既有覆盖 | E2E 落点 |
|----|---------|------|-------------|---------|---------|---------|
| CFG-01 | `set_config` → `config` roundtrip，克隆语义 | 正常 | — | 无 | `tests/basic.rs::test_config_retrieval`、`src/kit/async_kit.rs::async_kit_set_config_stores_value` | tests/e2e/e2e_config.rs |
| CFG-02 | `config::<C>()` 未设置类型 → `MissingConfig{key=类型名}` | 异常 | — | 无 | `tests/basic.rs::test_missing_config_error`、`test_missing_config_retrieval`、`src/kit/async_kit.rs::async_kit_config_missing_returns_error` | tests/e2e/e2e_config.rs |
| CFG-03 | 同一类型二次 `set_config` 覆盖（last-write-wins），跨多类型互不影响 | 边界 | — | 无 | `tests/e2e_advanced.rs::c08_config_type_conflict_last_write_wins`、`b06_config_override_last_write_wins`、`src/kit/async_kit.rs::async_kit_set_config_overwrite` | tests/e2e/e2e_config.rs |
| CFG-04 | `load_config::<C>()`（`Configurable` 桥接 confers `#[derive(Config)]` 的 `load_sync`）加载并存储 | 正常 | confers | 无 | `tests/basic.rs::load_config_stores_value_when_load_succeeds`、`load_config_bridges_to_confers_derive_load_sync`、`real_module_builds_capability_from_config` | tests/e2e/e2e_config.rs |
| CFG-05 | `C::load()` 失败 → `BuildFailed{context="load_config"}` 传播，不存储 | 异常 | confers | 无 | `tests/basic.rs::load_config_propagates_error_when_load_fails` | tests/e2e/e2e_config.rs |
| CFG-06 | `load_config` 成功后覆盖先前 `set_config` 的同类型值 | 边界 | confers | 无 | `tests/basic.rs::load_config_overrides_prior_set_config` | tests/e2e/e2e_config.rs |
| CFG-07 | `load_and_validate` 合法配置通过校验并存储 | 正常 | confers | 无 | `tests/e2e_feature_combinations.rs::e2e_confers_load_and_validate_ok`、`src/kit/kit.rs::load_and_validate_succeeds_with_valid_config` | tests/e2e/e2e_config.rs |
| CFG-08 | 校验失败 → `BuildFailed`，source 为 `ValidationError`（聚合全部错误），**且配置不写入** | 异常 | confers | 无 | `tests/e2e_feature_combinations.rs::e2e_confers_load_and_validate_reject`、`src/kit/kit.rs::load_and_validate_does_not_store_on_failure`、`load_and_validate_fails_with_invalid_config` | tests/e2e/e2e_config.rs |
| CFG-09 | 校验失败后修正再 `load_and_validate` 可成功（失败不留脏状态） | 边界 | confers | 无 | `src/kit/kit.rs::load_and_validate_retry_after_failure` | tests/e2e/e2e_config.rs |
| CFG-10 | `load_config_with(vars)`：`${VAR}` / `${VAR:-default}` 序列化级插值后存储 | 正常 | confers | 无 | `tests/e2e_feature_combinations.rs::e2e_confers_load_config_with_interpolation` | tests/e2e/e2e_config.rs |
| CFG-11 | `load_config_or_default` 双分支：加载成功返回 true；失败落 `default_value()` 返回 false | 正常 | confers | 无 | `tests/basic.rs::load_config_or_default_uses_loaded_value_when_load_succeeds`、`load_config_or_default_uses_default_when_load_fails`、`load_config_or_default_overrides_prior_set_config` | tests/e2e/e2e_config.rs |
| CFG-12 | `snapshot_config` → 修改 → `restore_config` 回滚到快照值 | 正常 | confers | 无 | `tests/e2e_feature_combinations.rs::e2e_confers_snapshot_restore`、`src/kit/kit.rs::restore_overwrites_current_config` | tests/e2e/e2e_config.rs |
| CFG-13 | `restore_config` 无快照 → `MissingConfig{key="… (snapshot)"}` | 异常 | confers | 无 | `src/kit/kit.rs::restore_returns_error_when_no_snapshot` | tests/e2e/e2e_config.rs |
| CFG-14 | 快照边界：无 config 时 `snapshot_config` 返回 false；重复快照覆盖旧快照；`has_snapshot` 精确反映 | 边界 | confers | 无 | `src/kit/kit.rs::snapshot_returns_false_when_config_missing`、`snapshot_overwrite_replaces_previous`、`has_snapshot_reflects_state` | tests/e2e/e2e_config.rs |
| CFG-15 | `populate_defaults`：空 kit 填默认返回 true；已有值不覆盖返回 false | 正常 | confers | 无 | `tests/e2e_feature_combinations.rs::e2e_confers_populate_defaults`、`e2e_confers_populate_defaults_noop`、`src/kit/kit.rs::populate_defaults_fills_empty_kit`、`populate_defaults_does_not_overwrite_existing` | tests/e2e/e2e_config.rs |
| CFG-16 | `merge_config::<C>(Override)`：仅非 None 字段覆盖（派生宏 `ConfigInherit` 生成的 Override 类型） | 正常 | confers | 无 | `tests/config_inherit_derive.rs::derive_config_inherit_works_with_kit_merge_config`、`src/kit/kit.rs::merge_config_overrides_only_some_fields` | tests/e2e/e2e_config.rs |
| CFG-17 | `merge_config` 无该 config 时 no-op 不 panic；Override 全 None 时原值不变 | 边界 | confers | 无 | `src/kit/kit.rs::merge_config_noop_when_missing`、`tests/config_inherit_derive.rs::derive_config_inherit_default_override_is_all_none` | tests/e2e/e2e_config.rs |
| CFG-18 | `extract_shared` / `inject_shared` 共享字段流：从配置 A 提取 → 注入配置 B | 正常 | confers | 无 | `tests/shared_config_derive.rs::derive_shared_config_kit_extract_inject_flow`、`src/kit/kit.rs::extract_then_inject_shared_flows_values` | tests/e2e/e2e_config.rs |
| CFG-19 | inject 边界：类型不匹配静默跳过、缺失键跳过、无 config 时 extract/inject 双 no-op | 边界 | confers | 无 | `tests/shared_config_derive.rs::derive_shared_config_inject_skips_type_mismatch`、`derive_shared_config_inject_skips_missing_keys`、`src/kit/kit.rs::inject_shared_noop_when_config_missing`、`extract_shared_noop_when_config_missing`、`inject_shared_skips_type_mismatch_silently` | tests/e2e/e2e_config.rs |
| CFG-20 | `merge_json_deep`：嵌套递归合并、标量替换、数组替换不合并、null 覆盖语义（10 例形态矩阵） | 正常 | confers | 无 | `src/kit/config.rs::overlay_inserts_new_keys`、`nested_objects_merge_recursively`、`deeply_nested_merge`、`arrays_are_replaced_not_merged`、`null_overlay_replaces_value` 等、`tests/e2e_feature_combinations.rs::e2e_confers_merge_json_deep` | tests/e2e/e2e_config.rs |
| CFG-21 | `interpolate_json_value` 形态矩阵：单变量/多变量/嵌套对象/默认值命中与忽略/未命中保留原文/非字符串不动/对象键不替换/数组元素替换 | 边界 | confers | 无 | `src/kit/kit.rs::basic_var_replacement`、`multiple_vars_in_one_string`、`nested_object_replacement`、`default_value_when_var_missing`、`default_value_ignored_when_var_present`、`no_match_preserved`、`non_string_values_untouched`、`object_keys_not_replaced`、`array_string_elements_replaced` | tests/e2e/e2e_config.rs |
| CFG-22 | 派生宏端到端：`#[derive(ConfigInherit)]`（含 custom override 名/嵌套委托/pub 可见性）与 `#[derive(SharedConfig)]`，A/B 项目继承场景全链路 | 正常 | confers | 无 | `tests/config_inherit_derive.rs`（6 例）、`tests/shared_config_derive.rs`（5 例）、`tests/config_inheritance_e2e.rs::e2e_config_inheritance_a_b_project_scenario`、`e2e_merge_config_then_shared_inheritance`、`src/kit/kit.rs::config_inheritance_works_on_ready_kit` | tests/e2e/e2e_config.rs |

### 2.6 热重载（RLD，9 条）

| ID | 场景描述 | 类型 | 涉及 feature | 依赖服务 | 既有覆盖 | E2E 落点 |
|----|---------|------|-------------|---------|---------|---------|
| RLD-01 | `subscribe::<C>(cb)` → `reload_config::<C>()` 回调被触发，回调内读到新值 | 正常 | reload | 无 | `tests/basic.rs::subscribe_callback_invoked_on_reload`、`tests/e2e_feature_combinations.rs::e2e_reload_updates_and_notifies` | tests/e2e/e2e_config.rs |
| RLD-02 | `reload_config` 更新存储值（`config::<C>()` 返回新值） | 正常 | reload | 无 | `tests/basic.rs::reload_config_updates_stored_value`、`src/kit/kit.rs::reload_config_updates_value_and_fires_subscribers` | tests/e2e/e2e_config.rs |
| RLD-03 | `reload_config` 时 `C::load()` 失败 → `BuildFailed` 传播，旧值保留 | 异常 | reload | 无 | `tests/basic.rs::reload_config_propagates_load_error` | tests/e2e/e2e_config.rs |
| RLD-04 | 同类型多个订阅者全部按注册序收到通知 | 边界 | reload | 无 | `tests/basic.rs::reload_config_invokes_multiple_subscribers`、`src/kit/kit.rs::reload_config_fires_all_subscribers` | tests/e2e/e2e_config.rs |
| RLD-05 | 无订阅者时 `reload_config` 成功 no-op | 边界 | reload | 无 | `tests/basic.rs::subscribe_and_reload_with_no_subscribers_succeeds` | tests/e2e/e2e_config.rs |
| RLD-06 | 订阅者 panic 语义固化：新值已存储、剩余订阅者被跳过（panic 穿透 `reload_config`，文档化行为） | 异常 | reload | 无 | 无→需新增 | tests/e2e/e2e_config.rs |
| RLD-07 | `require_ref` 借用存活期间调用 `reload_config`。**真实行为核正**（落地阶段核实）：`configs` 与 `capabilities` 为两个独立 `TypeMap`（`src/kit/kit.rs`），借用存活期间 `reload_config` 写配置不触能力借用、正常工作，无 borrow 冲突；同时固化 `set_config` 为 Unbuilt 态方法（Ready 态写配置唯一路径为 `reload_config`）——防未来合并两 TypeMap 引入隐蔽 panic | 边界 | reload | 无 | `src/kit/typemap.rs::inner_ref_panics_if_mutably_borrowed`（TypeMap 层既有） | tests/e2e/e2e_config.rs::e2e_require_ref_borrow_survives_reload_and_set_config + tests/e2e/e2e_concurrency.rs |
| RLD-08 | feature 链映射：`reload` 自动启用 `confers` + `confers/watch`，`subscribe/reload_config` 与 confers watch 生态可桥接 | 正常 | reload | 无 | `Cargo.toml` 声明核对 + `tests/e2e_feature_combinations.rs::e2e_confers_plus_reload` | tests/e2e/e2e_feature_combinations.rs |
| RLD-09 | examples/hot_reload 运行验收：改值 → reload → 回调打印新值 | 正常 | reload | 无 | examples/hot_reload（运行验收） | tests/e2e/e2e_feature_combinations.rs（引用示例） |

### 2.7 加密存储（ENC，13 条）

| ID | 场景描述 | 类型 | 涉及 feature | 依赖服务 | 既有覆盖 | E2E 落点 |
|----|---------|------|-------------|---------|---------|---------|
| ENC-01 | `set_encrypted` → `get_encrypted` roundtrip 值相等 | 正常 | encryption | 无 | `tests/basic.rs::encrypted_config_roundtrip`、`tests/e2e_feature_combinations.rs::e2e_encryption_roundtrip`、`src/kit/kit.rs::set_and_get_encrypted_roundtrip` | tests/e2e/e2e_encryption.rs |
| ENC-02 | 错误 master_key 解密 → `BuildFailed`（解密失败），不返回明文；篡改密文同型（wrong-key 模拟） | 异常 | encryption | 无 | `tests/basic.rs::get_encrypted_fails_with_wrong_key`、`tests/e2e_advanced.rs::e17_tampered_ciphertext_analog_via_wrong_key`、`src/kit/kit.rs::get_encrypted_wrong_key_returns_error` | tests/e2e/e2e_encryption.rs |
| ENC-03 | 未 `set_encrypted` 直接 `get_encrypted` → `MissingConfig` | 异常 | encryption | 无 | `tests/basic.rs::get_encrypted_returns_missing_config_error_when_not_set`、`src/kit/kit.rs::get_encrypted_missing_returns_error` | tests/e2e/e2e_encryption.rs |
| ENC-04 | 同类型二次 `set_encrypted` 覆盖旧密文（新值可解、旧值不可再取） | 边界 | encryption | 无 | `tests/basic.rs::set_encrypted_overwrites_prior_value` | tests/e2e/e2e_encryption.rs |
| ENC-05 | 加密存储与明文 TypeMap 隔离：`contains_encrypted` 只反映加密侧；加密值不出现在 `config::<C>()` | 边界 | encryption | 无 | `tests/basic.rs::encrypted_storage_is_separate_from_plaintext_typemap`、`src/kit/kit.rs::contains_encrypted_false_for_missing` | tests/e2e/e2e_encryption.rs |
| ENC-06 | 不可序列化值 → `set_encrypted` 序列化错误以 `BuildFailed` 传播 | 异常 | encryption | 无 | `tests/basic.rs::set_encrypted_propagates_serialization_error` | tests/e2e/e2e_encryption.rs |
| ENC-07 | master_key < 16 字节 → set 侧与 get 侧均立即拒绝（InvalidInput，消息含实际长度） | 异常 | encryption | 无 | `tests/e2e_feature_combinations.rs::e2e_encryption_short_key_rejected`、`e2e_encryption_get_short_key_rejected` | tests/e2e/e2e_encryption.rs |
| ENC-08 | 空 master_key（0 字节）行为固化（同样走 <16 拒绝分支） | 边界 | encryption | 无 | `tests/e2e_advanced.rs::c14_empty_master_key_encryption_behavior` | tests/e2e/e2e_encryption.rs |
| ENC-09 | 1MB 大配置加密 roundtrip（XChaCha20 吞吐路径） | 边界 | encryption | 无 | `tests/e2e_advanced.rs::c15_large_config_value_encryption_1mb` | tests/e2e/e2e_encryption.rs |
| ENC-10 | `EncryptedBlob`：nonce/ciphertext getters 返回原始切片、空 blob、clone 相等、**Debug 脱敏**（不泄漏密文） | 正常 | encryption | 无 | `src/kit/config.rs::getters_return_raw_slices`、`getters_return_empty_for_empty_blob`、`clone_produces_equal_blob`、`debug_format_redacts_sensitive_data`、`tests/basic.rs::encrypted_blob_getters_via_roundtrip` | tests/e2e/e2e_encryption.rs |
| ENC-11 | 字段密钥 HKDF 绑定 `C::PATH` + 版本标签 `v1`：不同 PATH 派生不同密钥；HKDF 失败路径映射 `BuildFailed`（文档化） | 正常 | encryption | 无 | `src/kit/kit.rs`（`KEY_DERIVATION_VERSION`/`derive_kit_field_key`）、`tests/e2e_advanced.rs::e15_hkdf_failure_path_documented`；不同 PATH 隔离断言→需新增断言 | tests/e2e/e2e_encryption.rs |
| ENC-12 | confers `XChaCha20Crypto` / `derive_field_key` 再导出可用（trait-kit 面直接调用加解密原语） | 正常 | encryption | 无 | examples/encryption（运行验收）→库级直接断言需新增 | tests/e2e/e2e_encryption.rs |
| ENC-13 | confers+encryption 组合：明文 `load_config` 与密文 `set_encrypted` 同 Kit 共存互不干扰 | 正常 | confers,encryption | 无 | `tests/e2e_feature_combinations.rs::e2e_confers_plus_encryption` | tests/e2e/e2e_feature_combinations.rs |

### 2.8 接口/实现分离（ITF，9 条）

| ID | 场景描述 | 类型 | 涉及 feature | 依赖服务 | 既有覆盖 | E2E 落点 |
|----|---------|------|-------------|---------|---------|---------|
| ITF-01 | `register_as::<M>()` → `build()` → `resolve::<I>()` 返回 `Arc<dyn Trait>` | 正常 | interface | 无 | `src/kit/kit.rs::register_as_then_resolve_returns_arc_dyn_trait`、`tests/e2e_advanced.rs::a19_register_as_then_resolve`、`tests/e2e_feature_combinations.rs::e2e_interface_register_and_resolve` | tests/e2e/e2e_features.rs |
| ITF-02 | 同一 interface 类型二次 `register_as` → `AlreadyRegistered`（一个接口一个实现） | 异常 | interface | 无 | `src/kit/kit.rs::register_as_twice_same_interface_returns_already_registered`、`tests/e2e_advanced.rs::e07_duplicate_interface_registration_returns_already_registered` | tests/e2e/e2e_features.rs |
| ITF-03 | `build()` 之前 `resolve` → `MissingCapability` | 异常 | interface | 无 | `src/kit/kit.rs::resolve_before_build_returns_missing_capability` | tests/e2e/e2e_features.rs |
| ITF-04 | `resolve` 未注册 interface → `MissingCapability` | 异常 | interface | 无 | `src/kit/kit.rs::resolve_unregistered_interface_returns_missing_capability`、`tests/e2e_advanced.rs::e27_resolve_unregistered_interface_returns_missing_capability` | tests/e2e/e2e_features.rs |
| ITF-05 | 解析出的 `Arc<dyn Trait>` 可直接调用 trait 方法（动态分派生效） | 正常 | interface | 无 | `src/kit/kit.rs::resolve_returns_callable_trait_object`、`file_logger_interface_build_and_resolve` | tests/e2e/e2e_features.rs |
| ITF-06 | `register_as` 与 `register`（单绑定）/multi 共存互不干扰 | 边界 | interface | 无 | `src/kit/kit.rs::register_as_coexists_with_register`、`register_as_builds_during_build` | tests/e2e/e2e_features.rs |
| ITF-07 | 同一模块类型经 `register_as` 注册两次（按模块 TypeId 判定）→ `AlreadyRegistered` | 异常 | interface | 无 | `src/kit/kit.rs::register_as_same_module_twice_returns_already_registered` | tests/e2e/e2e_features.rs |
| ITF-08 | `InterfaceBuilder::build` 失败 → `BuildFailed{context=interface}` 传播 | 异常 | interface | 无 | `src/kit/kit.rs::interface_build_error_returns_build_failed` | tests/e2e/e2e_features.rs |
| ITF-09 | `Interface` blanket impl 使任意具体类型可直接作 Capability 转 `into_interface`（`?Sized` 约束边界） | 边界 | interface | 无 | `src/core/meta.rs::interface_builder_capability_is_clone`、`interface_builder_build_returns_concrete_capability` | tests/e2e/e2e_features.rs |

### 2.9 生命周期（LCY，9 条）

| ID | 场景描述 | 类型 | 涉及 feature | 依赖服务 | 既有覆盖 | E2E 落点 |
|----|---------|------|-------------|---------|---------|---------|
| LCY-01 | `register_lifecycle::<M>()` 后 `build()` 完成阶段自动调用 `on_ready` | 正常 | lifecycle | 无 | `src/kit/kit.rs::lifecycle_on_ready_called_during_build`、`tests/e2e_feature_combinations.rs::e2e_lifecycle_on_ready_after_build` | tests/e2e/e2e_features.rs |
| LCY-02 | `Kit<Ready>::shutdown()` 按**逆构建序**调用 `on_shutdown`（后建先关） | 正常 | lifecycle | 无 | `src/kit/kit.rs::lifecycle_shutdown_called_in_reverse_order`、`tests/e2e_feature_combinations.rs::e2e_lifecycle_on_shutdown_on_explicit_shutdown` | tests/e2e/e2e_features.rs |
| LCY-03 | `on_ready` 返回 `Err` → `build()` 整体失败，`LifecycleFailed{context=NAME}` 传播 | 异常 | lifecycle | 无 | `src/kit/kit.rs::lifecycle_on_ready_failure_propagates` | tests/e2e/e2e_features.rs |
| LCY-04 | `Lifecycle` 默认实现：`on_ready`/`on_shutdown` 缺省 no-op | 正常 | lifecycle | 无 | `src/core/lifecycle.rs::lifecycle_trait_has_default_on_ready`、`lifecycle_trait_has_default_on_shutdown`、`lifecycle_shutdown_counter_increments` | tests/e2e/e2e_features.rs |
| LCY-05 | 某模块 `on_shutdown` 失败不阻断其余模块关闭（文档契约："A failed shutdown does not prevent other modules"）→ 行为级断言缺失 | 异常 | lifecycle | 无 | 无→需新增 | tests/e2e/e2e_features.rs |
| LCY-06 | `AsyncLifecycle`（async+lifecycle）：异步 on_ready 在 build 后被调用 | 正常 | lifecycle,async | 无 | `src/kit/async_kit.rs::async_lifecycle_on_ready_called`、`async_lifecycle_test_module_full_kit_integration`、`src/core/lifecycle.rs::async_lifecycle_default_on_ready_returns_ok` | tests/e2e/e2e_async.rs |
| LCY-07 | 多模块 on_ready 按拓扑序执行（依赖者的 ready 晚于被依赖者） | 边界 | lifecycle | 无 | `src/core/lifecycle.rs::lifecycle_test_module_full_kit_integration`（部分）→显式顺序断言需新增 | tests/e2e/e2e_features.rs |
| LCY-08 | lifecycle+health 组合：ready 后健康检查可用 | 边界 | lifecycle,health | 无 | `tests/e2e_feature_combinations.rs::e2e_lifecycle_plus_health` | tests/e2e/e2e_feature_combinations.rs |
| LCY-09 | scope+lifecycle 组合：scope 内构建不影响 lifecycle 钩子语义 | 边界 | scope,lifecycle | 无 | `tests/e2e_feature_combinations.rs::e2e_scope_plus_lifecycle` | tests/e2e/e2e_feature_combinations.rs |

### 2.10 健康检查（HLT，9 条）

| ID | 场景描述 | 类型 | 涉及 feature | 依赖服务 | 既有覆盖 | E2E 落点 |
|----|---------|------|-------------|---------|---------|---------|
| HLT-01 | `HealthStatus` 三态构造器（Healthy/`degraded`/`unhealthy`）、`is_healthy`、Clone/Eq/Debug | 正常 | health | 无 | `src/core/health.rs::health_status_is_healthy`、`health_status_clone_and_eq`、`health_status_debug_format`、`health_status_ne_eq`、`tests/e2e_feature_combinations.rs::e2e_health_status_constructors` | tests/e2e/e2e_features.rs |
| HLT-02 | `register_health_check::<M>()` → `health_check::<M>()` 查询返回模块自报状态 | 正常 | health | 无 | `src/kit/kit.rs::health_check_registered_and_queryable`、`tests/e2e_feature_combinations.rs::e2e_health_single_module_check` | tests/e2e/e2e_features.rs |
| HLT-03 | `health_report()` 汇总全部已注册 checker（多模块全 Healthy 与混合两态） | 正常 | health | 无 | `src/kit/kit.rs::health_report_returns_all_checkers`、`tests/e2e_feature_combinations.rs::e2e_health_all_healthy`、`e2e_health_mixed` | tests/e2e/e2e_features.rs |
| HLT-04 | `health_check::<M>()` 未注册 checker → `MissingConfig{key=NAME}` | 异常 | health | 无 | `src/kit/kit.rs::health_check_unregistered_returns_error` | tests/e2e/e2e_features.rs |
| HLT-05 | `check` 返回 Unhealthy 分支（如零值）如实上报 | 异常 | health | 无 | `src/kit/kit.rs::health_check_unhealthy_for_zero_value`、`src/core/health.rs::health_check_returns_unhealthy_for_zero_value` | tests/e2e/e2e_features.rs |
| HLT-06 | `HealthStatus::degraded(detail)` 携带细节且 `is_healthy()==false`（降级非故障语义） | 边界 | health | 无 | `src/core/health.rs::health_check_returns_unhealthy_for_zero_value` 族 + `HealthStatus::degraded` 构造（`health_status_*` 组） | tests/e2e/e2e_features.rs |
| HLT-07 | checker 闭包找不到 capability 时返回 `Unhealthy("capability not found")`（防御分支） | 异常 | health | 无 | 无→需新增 | tests/e2e/e2e_features.rs |
| HLT-08 | `AsyncHealthCheck`（async+health）：异步 check 可查询、report 汇总、未注册报错 | 正常 | health,async | 无 | `src/kit/async_kit.rs::async_health_check_queryable`、`async_health_report_returns_all`、`async_health_check_unregistered_returns_error`、`src/core/health.rs::async_health_check_returns_healthy`、`_unhealthy` | tests/e2e/e2e_async.rs |
| HLT-09 | health 与 async 组合的 `AsyncKit` 面报告口径与 sync 一致（同构性） | 边界 | health,async | 无 | `src/kit/async_kit.rs`（async health 组）→同构对照断言需新增 | tests/e2e/e2e_async.rs |

### 2.11 构建观察者（OBS，7 条）

| ID | 场景描述 | 类型 | 涉及 feature | 依赖服务 | 既有覆盖 | E2E 落点 |
|----|---------|------|-------------|---------|---------|---------|
| OBS-01 | `BuildObserver` trait object 安全且 `Send + Sync`（可跨线程共享 `Arc`） | 正常 | observer | 无 | `src/core/observer.rs::observer_trait_is_object_safe`、`observer_is_send_sync` | tests/e2e/e2e_features.rs |
| OBS-02 | `with_observer` 后 `build()` 依次回调 `on_module_start` / `on_module_built`（含耗时参数） | 正常 | observer | 无 | `src/kit/kit.rs::observer_callbacks_fired_during_build`、`tests/e2e_feature_combinations.rs::e2e_observer_notified_on_build` | tests/e2e/e2e_features.rs |
| OBS-03 | 模块构建失败 → `on_build_error(module_name, &BuildFailed)` 被调用后错误继续传播 | 异常 | observer | 无 | `src/kit/kit.rs::observer_on_build_error_called_on_failure`、`src/kit/async_kit.rs::async_observer_on_build_error_called` | tests/e2e/e2e_features.rs |
| OBS-04 | `DefaultObserver` 三个回调缺省 no-op（含 dyn 分派形态），无 panic | 边界 | observer | 无 | `src/core/observer.rs::observer_default_on_module_start_does_not_panic`、`_on_module_built_…`、`_on_build_error_…`、`observer_default_via_dyn_dispatch` | tests/e2e/e2e_features.rs |
| OBS-05 | 多个 observer 注册时全部收到同一事件（遍历序 == 注册序）→ 断言缺失 | 边界 | observer | 无 | 无→需新增 | tests/e2e/e2e_features.rs |
| OBS-06 | observer+decorator 组合：回调观察到的是装饰后能力构建（顺序：build → decorate → on_module_built） | 正常 | observer,decorator | 无 | `tests/e2e_feature_combinations.rs::e2e_observer_plus_decorator` | tests/e2e/e2e_feature_combinations.rs |
| OBS-07 | `AsyncKit` 面 observer 回调（async 构建路径同型） | 正常 | observer,async | 无 | `src/kit/async_kit.rs::async_observer_callbacks_fired` | tests/e2e/e2e_async.rs |

### 2.12 装饰器（DEC，8 条）

| ID | 场景描述 | 类型 | 涉及 feature | 依赖服务 | 既有覆盖 | E2E 落点 |
|----|---------|------|-------------|---------|---------|---------|
| DEC-01 | `decorate::<M>(f)` 后 `build()` 产出的能力已被包装（如计数代理外层可见） | 正常 | decorator | 无 | `tests/e2e_feature_combinations.rs::e2e_decorator_wraps_capability`、`src/kit/kit.rs::decorate_registers_decorator` | tests/e2e/e2e_features.rs |
| DEC-02 | 无装饰器注册时构建 no-op，能力原样（零开销路径） | 边界 | decorator | 无 | `tests/e2e_feature_combinations.rs::e2e_decorator_no_op_when_absent` | tests/e2e/e2e_features.rs |
| DEC-03 | 同一能力多个装饰器按注册顺序洋葱式叠加（f1 先 f2 后 → f2 包 f1）→ 顺序断言缺失 | 边界 | decorator | 无 | 无→需新增 | tests/e2e/e2e_features.rs |
| DEC-04 | 装饰器覆盖全部四条构建路径：eager / lazy（首次 require）/ multi-binding / interface → 仅 eager 有断言 | 边界 | decorator | 无 | `src/kit/kit.rs::e2e` 面仅 eager（`e2e_decorator_wraps_capability`）→lazy/multi/interface 路径需新增 | tests/e2e/e2e_features.rs |
| DEC-05 | `decorate` 以 Capability TypeId 为键、维护模块→能力映射（`decorator_module_to_cap`）供 build 期查找 | 正常 | decorator | 无 | `src/kit/kit.rs::decorate_registers_decorator`（注册面） | tests/e2e/e2e_features.rs |
| DEC-06 | toggle+decorator 组合：开关关闭时模块不注册、装饰器不生效；开启后包装生效 | 正常 | decorator,toggle | 无 | `tests/e2e_feature_combinations.rs::e2e_toggle_plus_decorator` | tests/e2e/e2e_feature_combinations.rs |
| DEC-07 | 装饰器闭包 panic / 内部 downcast 失败 → 文档化 panic 语义（`expect("decorator type mismatch")`）固化 | 异常 | decorator | 无 | 无→需新增 | tests/e2e/e2e_features.rs |
| DEC-08 | `AsyncKit::decorate`（async 构建路径装饰）行为与 sync 一致 | 正常 | decorator,async | 无 | src 无 async decorate 测试→需新增（对照 examples/decorator 语义） | tests/e2e/e2e_async.rs |

### 2.13 作用域（SCP，10 条）

| ID | 场景描述 | 类型 | 涉及 feature | 依赖服务 | 既有覆盖 | E2E 落点 |
|----|---------|------|-------------|---------|---------|---------|
| SCP-01 | `Scope::register::<M>()` → `require::<M>()` 取得作用域内实例 | 正常 | scope | 无 | `src/kit/scope.rs::scope_register_then_require`、`tests/e2e_feature_combinations.rs::e2e_scope_isolation` | tests/e2e/e2e_runtime.rs |
| SCP-02 | `Scope::require` 二次调用返回缓存（每作用域单例，LazySlot 同源实现） | 边界 | scope | 无 | `src/kit/scope.rs::scope_require_caches_result` | tests/e2e/e2e_runtime.rs |
| SCP-03 | `Scope::register` 重复注册同模块 → 错误 | 异常 | scope | 无 | `src/kit/scope.rs::scope_register_duplicate_returns_error` | tests/e2e/e2e_runtime.rs |
| SCP-04 | `Scope::require` 未注册模块 → `MissingCapability` | 异常 | scope | 无 | `src/kit/scope.rs::scope_require_unregistered_returns_missing` | tests/e2e/e2e_runtime.rs |
| SCP-05 | 双作用域互不泄漏：A 注册的模块在 B 中不可见 | 边界 | scope | 无 | `src/kit/scope.rs::scope_registrations_do_not_leak_across_scopes`、`tests/e2e_feature_combinations.rs::e2e_scope_isolation` | tests/e2e/e2e_runtime.rs |
| SCP-06 | `Scope` drop 后资源清空（`scope_drop_clears_resources`） | 边界 | scope | 无 | `src/kit/scope.rs::scope_drop_clears_resources` | tests/e2e/e2e_runtime.rs |
| SCP-07 | `Kit<Ready>::create_scope()` 返回空作用域，与 Kit 能力互相独立 | 正常 | scope | 无 | `src/kit/kit.rs::create_scope_returns_empty_scope`、`tests/e2e_feature_combinations.rs::e2e_scope_create_empty` | tests/e2e/e2e_runtime.rs |
| SCP-08 | `AsyncScope`（scope+async）：register/require/insert 全链路 | 正常 | scope,async | 无 | `src/kit/scope.rs::async_scope_register_then_contains`、`async_scope_module_build_and_require`、`src/kit/async_kit.rs::async_create_scope_returns_empty` | tests/e2e/e2e_async.rs |
| SCP-09 | `AsyncScope` 异常面：重复注册报错、缺失 require 报错 | 异常 | scope,async | 无 | `src/kit/scope.rs::async_scope_register_duplicate_returns_error`、`async_scope_require_missing_returns_error` | tests/e2e/e2e_async.rs |
| SCP-10 | scope+lifecycle 组合：作用域实例参与（或不参与）Kit 级 shutdown，语义固化 | 边界 | scope,lifecycle | 无 | `tests/e2e_feature_combinations.rs::e2e_scope_plus_lifecycle` | tests/e2e/e2e_feature_combinations.rs |

### 2.14 特性开关（TGL，9 条）

| ID | 场景描述 | 类型 | 涉及 feature | 依赖服务 | 既有覆盖 | E2E 落点 |
|----|---------|------|-------------|---------|---------|---------|
| TGL-01 | `enable_toggle(key, true/false)` → `is_toggle_enabled` 状态正确；多 key 独立 | 正常 | toggle | 无 | `tests/e2e_feature_combinations.rs::e2e_toggle_enable_query`、`e2e_toggle_multiple`、`src/kit/kit.rs::enable_toggle_sets_value` | tests/e2e/e2e_runtime.rs |
| TGL-02 | 未知 key 查询返回 false（不 panic、不报错） | 边界 | toggle | 无 | `tests/e2e_feature_combinations.rs::e2e_toggle_default_disabled`、`src/kit/kit.rs::is_toggle_enabled_returns_false_for_unknown` | tests/e2e/e2e_runtime.rs |
| TGL-03 | `register_if_toggle`：开启时注册并返回 true；关闭时跳过返回 false（双分支） | 正常 | toggle | 无 | `src/kit/kit.rs::register_if_toggle_registers_when_enabled`、`register_if_toggle_skips_when_disabled`；examples/toggle_basic（运行验收） | tests/e2e/e2e_runtime.rs |
| TGL-04 | `register_if_toggle` 开启但模块已注册 → `AlreadyRegistered` | 异常 | toggle | 无 | `src/kit/kit.rs::register_if_toggle_returns_error_on_duplicate` | tests/e2e/e2e_runtime.rs |
| TGL-05 | toggle 状态跨 `build()` 保持（Unbuilt 期设置 → Ready 期仍可读） | 边界 | toggle | 无 | `tests/e2e_feature_combinations.rs::e2e_toggle_persists_after_build`、`src/kit/kit.rs::toggle_state_survives_build` | tests/e2e/e2e_runtime.rs |
| TGL-06 | `Kit<Ready>` 态仍可 `enable_toggle` / `is_toggle_enabled`（运行期动态开关） | 正常 | toggle | 无 | `src/kit/kit.rs::toggle_enable_on_ready_state` | tests/e2e/e2e_runtime.rs |
| TGL-07 | 开关关闭 → 对应模块能力不可获取（`require` 报缺失/`contains` 为 false），开启后可获取 | 边界 | toggle | 无 | `src/kit/kit.rs::toggle_disabled_capability_not_retrievable` | tests/e2e/e2e_runtime.rs |
| TGL-08 | toggle+scope 组合：开关门控作用域内注册（无既有断言） | 边界 | toggle,scope | 无 | 无→需新增 | tests/e2e/e2e_feature_combinations.rs |
| TGL-09 | 结构核对：`src/kit/toggle.rs` 为 doc-only 模块，无独立运行时（防止误判 API 面） | 边界 | toggle | 无 | `src/kit/toggle.rs`（10 行 doc）+ `src/kit/mod.rs`（无 toggle 导出）核对 | tests/e2e/e2e_presets.rs（编译面） |

### 2.15 优雅关闭（SHD，13 条）

| ID | 场景描述 | 类型 | 涉及 feature | 依赖服务 | 既有覆盖 | E2E 落点 |
|----|---------|------|-------------|---------|---------|---------|
| SHD-01 | 三阶段按 `StopRequests → DrainQueue → CloseConnections` 顺序执行各阶段 hook | 正常 | shutdown | 无 | `src/kit/shutdown.rs::shutdown_coordinator_executes_hooks_in_order`、`shutdown_coordinator_phase_order_is_correct`、`tests/e2e_feature_combinations.rs::e2e_shutdown_all_phases_execute` | tests/e2e/e2e_runtime.rs |
| SHD-02 | `ShutdownPhase::all_phases()` 有序三元组、`as_str()` 可读名（stop_requests/drain_queue/close_connections） | 正常 | shutdown | 无 | `src/kit/shutdown.rs::shutdown_phase_all_phases_returns_three`、`shutdown_phase_as_str_returns_readable_name` | tests/e2e/e2e_runtime.rs |
| SHD-03 | 阶段超时：单 hook 超时 → 跳过该阶段剩余 hook 继续下一阶段，结果标记超时 | 异常 | shutdown | 无 | `src/kit/shutdown.rs::shutdown_coordinator_timeout_skips_remaining_hooks` | tests/e2e/e2e_runtime.rs |
| SHD-04 | 全局超时：总时长超 `set_global_timeout` → 立即中止后续阶段 | 异常 | shutdown | 无 | `src/kit/shutdown.rs::shutdown_coordinator_global_timeout`、`tests/e2e_feature_combinations.rs::e2e_shutdown_set_timeouts` | tests/e2e/e2e_runtime.rs |
| SHD-05 | `ShutdownPhaseResult::is_ok` / `ShutdownResult::is_ok` / `timed_out_phases` 精确反映各阶段结局 | 正常 | shutdown | 无 | `src/kit/shutdown.rs::shutdown_result_into_result_ok`、`ShutdownPhaseResult` 字段断言（同文件组） | tests/e2e/e2e_runtime.rs |
| SHD-06 | `into_result()` 超时态 → `Err(ShutdownTimedOut{phases})`，错误消息含全部超时阶段名 | 异常 | shutdown | 无 | `src/kit/shutdown.rs::shutdown_result_into_result_timeout`；`TraitKitError::ShutdownTimedOut` Display 断言→需新增（见 ERR-07） | tests/e2e/e2e_runtime.rs |
| SHD-07 | 无任何 hook/空阶段 `shutdown()` 成功 | 边界 | shutdown | 无 | `src/kit/shutdown.rs::shutdown_coordinator_empty_phases_succeed`、`tests/e2e_feature_combinations.rs::e2e_shutdown_empty_phases_succeed` | tests/e2e/e2e_runtime.rs |
| SHD-08 | `ShutdownCoordinator::new()`/`default()` 可直接用（默认超时配置） | 边界 | shutdown | 无 | `src/kit/shutdown.rs::shutdown_coordinator_default_works` | tests/e2e/e2e_runtime.rs |
| SHD-09 | 不可重入：二次 `shutdown()` 对已清空 hook 表为 no-op（drain 语义） | 边界 | shutdown | 无 | `src/kit/shutdown.rs::shutdown_coordinator_hooks_not_reentrant` | tests/e2e/e2e_runtime.rs |
| SHD-10 | `AsyncShutdownCoordinator`（shutdown+async）：异步 hook 全阶段顺序执行 | 正常 | shutdown,async | 无 | `src/kit/shutdown.rs::async_shutdown_coordinator_executes_hooks` | tests/e2e/e2e_async.rs |
| SHD-11 | async 超时：hook 挂起触发阶段超时中止 | 异常 | shutdown,async | 无 | `src/kit/shutdown.rs::async_shutdown_coordinator_timeout` | tests/e2e/e2e_async.rs |
| SHD-12 | async 默认构造与不可重入语义 | 边界 | shutdown,async | 无 | `src/kit/shutdown.rs::async_shutdown_coordinator_default`、`async_shutdown_coordinator_hooks_not_reentrant` | tests/e2e/e2e_async.rs |
| SHD-13 | shutdown+decorator 组合：被装饰模块关闭时装饰层先于核心层释放（或按注册逆序）语义固化 | 边界 | shutdown,decorator | 无 | 无→需新增 | tests/e2e/e2e_feature_combinations.rs |

### 2.16 异步 Kit（ASK，15 条）

| ID | 场景描述 | 类型 | 涉及 feature | 依赖服务 | 既有覆盖 | E2E 落点 |
|----|---------|------|-------------|---------|---------|---------|
| ASK-01 | `AsyncKit::new()` → `register` → `build().await` → `AsyncKit<Ready>` typestate 全链路 | 正常 | async | 无 | `tests/e2e_advanced.rs::a09_async_kit_basic_flow`、`src/kit/async_kit.rs::async_kit_build_returns_ready_state`、`async_kit_new_returns_unbuilt_state` | tests/e2e/e2e_async.rs |
| ASK-02 | 多模块异步构建按拓扑序完成 | 正常 | async | 无 | `src/kit/async_kit.rs::async_kit_build_multiple_modules_in_topo_order` | tests/e2e/e2e_async.rs |
| ASK-03 | 异步缺失依赖 → `DependencyMissing` | 异常 | async | 无 | `src/kit/async_kit.rs::async_kit_build_missing_dependency_returns_error`、`tests/e2e_advanced.rs::e24_async_dependency_missing_returns_dependency_missing` | tests/e2e/e2e_async.rs |
| ASK-04 | 异步环检测 → `CycleDetected`（两节点与三节点） | 异常 | async | 无 | `src/kit/async_kit.rs::async_kit_build_cycle_returns_error`、`async_kit_di_three_node_cycle_returns_error`、`tests/e2e_advanced.rs::e25_async_cycle_detected` | tests/e2e/e2e_async.rs |
| ASK-05 | `AsyncAutoBuilder::build` 异步体真实被 await（async build_fn 生效） | 正常 | async | 无 | `src/kit/async_kit.rs::async_kit_build_calls_async_build_fn` | tests/e2e/e2e_async.rs |
| ASK-06 | 异步构建失败 → `BuildFailed` 传播 | 异常 | async | 无 | `src/kit/async_kit.rs::async_kit_build_propagates_build_error`、`tests/e2e_advanced.rs::e23_async_build_fails_returns_build_failed` | tests/e2e/e2e_async.rs |
| ASK-07 | Ready 态 `require` / `optional` / `contains` / `contains_config` 四件套与 sync 同构 | 正常 | async | 无 | `src/kit/async_kit.rs::async_kit_ready_require_returns_capability`、`async_kit_ready_optional_returns_some_for_built`、`async_kit_ready_contains_returns_true_for_built`、`async_kit_ready_contains_config_returns_true`（及对应 false 组） | tests/e2e/e2e_async.rs |
| ASK-08 | 跨模块 DI：依赖能力注入 + 传递链（A→B→C） | 正常 | async | 无 | `tests/e2e_advanced.rs::a10_async_cross_module_di`、`a11_async_transitive_chain_di`、`src/kit/async_kit.rs::async_kit_di_dependency_built_before_dependent`、`async_kit_di_transitive_dependency_chain` | tests/e2e/e2e_async.rs |
| ASK-09 | `set_config` / `config` 于 Unbuilt 与 build 回调内读取（`Send+Sync` 约束版） | 正常 | async | 无 | `tests/e2e_advanced.rs::a12_async_config_read_in_build`、`tests/e2e_feature_combinations.rs::e2e_async_kit_config_ops` | tests/e2e/e2e_async.rs |
| ASK-10 | Ready 态 `require` 未注册模块 → `BuildFailed`（async 面错误口径） | 异常 | async | 无 | `tests/e2e_advanced.rs::c19_async_build_require_unregistered_returns_build_failed`、`src/kit/async_kit.rs::async_kit_ready_require_missing_returns_error` | tests/e2e/e2e_async.rs |
| ASK-11 | 并发保证：`AsyncKit`/`AsyncKit<Ready>`/`TraitKitError` 均 `Send + Sync`（static_assertions 编译期断言） | 边界 | async | 无 | `src/kit/async_kit.rs::async_kit_is_send_sync`、`async_kit_build_result_is_send`、`async_kit_error_is_send` | tests/e2e/e2e_async.rs |
| ASK-12 | 并发注册多模块无死锁、全部可构建 | 边界 | async | 无 | `src/kit/async_kit.rs::async_kit_concurrent_registration` | tests/e2e/e2e_async.rs |
| ASK-13 | 异步取消：build future 被 drop 后状态不半更新（重试 build 可成功，cancel-safe 语义）→ 语义未固化 | 异常 | async | 无 | 无→需新增 | tests/e2e/e2e_async.rs |
| ASK-14 | `AsyncTypeMap` 全操作矩阵：insert/get/overwrite/contains/read_by_type_id（三态）/len/default/跨线程/Arc 共享/Debug | 正常 | async | 无 | `src/kit/async_typemap.rs`（15 例，含 `cross_thread_access_does_not_panic`、`arc_clone_shares_state`、`read_by_type_id_returns_none_for_wrong_type`） | tests/e2e/e2e_async.rs |
| ASK-15 | `TraitKitError` → mock 错误的 `From` 转换（async 错误面互转） | 边界 | async | 无 | `src/kit/async_kit.rs::async_from_trait_kit_error_for_mock_error`、`async_kit_di_missing_dependency_returns_error` | tests/e2e/e2e_async.rs |

### 2.17 国际化（I18，14 条）

| ID | 场景描述 | 类型 | 涉及 feature | 依赖服务 | 既有覆盖 | E2E 落点 |
|----|---------|------|-------------|---------|---------|---------|
| I18-01 | `tr(message_id, args)` 全局便捷函数：init 后按当前 locale 翻译 | 正常 | i18n | 无 | `src/i18n/mod.rs::tr_convenience_function_works`、`tests/e2e_feature_combinations.rs::e2e_i18n_tr_global_function` | tests/e2e/e2e_i18n.rs |
| I18-02 | `I18nManager::init()` 进程级单例，`global()` 初始化后返回 `Some` | 正常 | i18n | 无 | `src/i18n/mod.rs::manager_init_returns_valid_instance`、`i18n_manager_global_returns_some_after_init` | tests/e2e/e2e_i18n.rs |
| I18-03 | `init_with_locale("zh-CN")` 指定 locale 构建，`locale_tag()` 回读 | 正常 | i18n | 无 | `src/i18n/mod.rs::i18n_manager_init_with_locale`、`i18n_manager_build_zh_cn`、`tests/e2e_feature_combinations.rs::e2e_i18n_init_and_locale` | tests/e2e/e2e_i18n.rs |
| I18-04 | `translate(message_id, args)` 变量插值（`{ $cycle }` 等占位替换） | 正常 | i18n | 无 | `src/i18n/mod.rs::manager_translate_message`、`i18n_manager_translate_and_locale_tag` | tests/e2e/e2e_i18n.rs |
| I18-05 | 未知 message_id → 回退返回 key 本身（不 panic、不空串） | 异常 | i18n | 无 | `src/i18n/mod.rs::manager_translate_unknown_key_returns_key`、`tests/e2e_feature_combinations.rs::e2e_i18n_translate_fallback` | tests/e2e/e2e_i18n.rs |
| I18-06 | FTL 目录解析：合法消息解析、注释与空行跳过（en/zh 双目录内嵌） | 正常 | i18n | 无 | `src/i18n/mod.rs::catalog_parse_simple_ftl`、`catalog_parse_skips_comments_and_blanks` | tests/e2e/e2e_i18n.rs |
| I18-07 | locale 解析：`en`/`zh` 合法、非法串报 `I18nError::InvalidLocale` | 正常/异常 | i18n | 无 | `src/i18n/mod.rs::test_locale_parsing_en`、`test_locale_parsing_zh`、`test_invalid_locale`、`tests/e2e_advanced.rs::e20_invalid_locale_error_message` | tests/e2e/e2e_i18n.rs |
| I18-08 | ICU4X 数字本地化：en/zh 分组符差异、整数、非 en/zh 回退 | 边界 | i18n | 无 | `src/i18n/mod.rs::test_format_number_en`、`test_format_number_zh`、`test_format_number_integer` | tests/e2e/e2e_i18n.rs |
| I18-09 | 非有限数字（NaN/∞）→ `I18nError::InvalidNumber` | 异常 | i18n | 无 | `src/i18n/mod.rs::test_format_number_not_finite`、`tests/e2e_advanced.rs::e21_non_finite_number_returns_invalid_number` | tests/e2e/e2e_i18n.rs |
| I18-10 | 复数规则：en 复数类别判定 + zero 类别边界 | 正常 | i18n | 无 | `src/i18n/mod.rs::test_plural_rules_en`、`test_plural_category_zero` | tests/e2e/e2e_i18n.rs |
| I18-11 | ICU4X 日期格式化（年月日 → 本地化串） | 正常 | i18n | 无 | `src/i18n/mod.rs::test_format_date_en` | tests/e2e/e2e_i18n.rs |
| I18-12 | 非法日期（月 13/日 32）→ `I18nError::DateError` | 异常 | i18n | 无 | `src/i18n/mod.rs::test_format_date_invalid_month`、`test_format_date_invalid_day`、`tests/e2e_advanced.rs::e22_invalid_date_returns_date_error` | tests/e2e/e2e_i18n.rs |
| I18-13 | collation 排序：基础排序与相等串比较 `Ordering::Equal` | 边界 | i18n | 无 | `src/i18n/mod.rs::test_collator_basic`、`test_compare_equal_strings` | tests/e2e/e2e_i18n.rs |
| I18-14 | `I18nError` 四变体 Display 可读（InvalidLocale/DateError/InvalidNumber/FormatError） | 边界 | i18n | 无 | `src/i18n/mod.rs::error_display_invalid_locale`、`error_display_date_error`、`error_display_invalid_number`、`error_display_format_error` | tests/e2e/e2e_i18n.rs |

### 2.18 错误体系（ERR，9 条）

| ID | 场景描述 | 类型 | 涉及 feature | 依赖服务 | 既有覆盖 | E2E 落点 |
|----|---------|------|-------------|---------|---------|---------|
| ERR-01 | `CycleDetected` Display 经 `tr()` 输出，含全部环上模块名（` → ` 连接） | 正常 | — | 无 | `src/error.rs::cycle_detected_display_contains_modules`、`tests/basic.rs::kit_error_display_and_source_behavior` | tests/e2e/e2e_core.rs |
| ERR-02 | `DependencyMissing` Display 同时含发起模块与缺失依赖名 | 正常 | — | 无 | `src/error.rs::dependency_missing_display_contains_both_modules` | tests/e2e/e2e_core.rs |
| ERR-03 | `AlreadyRegistered` Display 含重复模块名 | 正常 | — | 无 | `src/error.rs::already_registered_display_contains_module` | tests/e2e/e2e_core.rs |
| ERR-04 | `BuildFailed` Display 含 context 与底层 source 文本；`Error::source()` 返回内层错误 | 正常 | — | 无 | `src/error.rs::build_failed_display_contains_context_and_source`、`error_source_returns_inner_for_build_failed` | tests/e2e/e2e_core.rs |
| ERR-05 | `MissingCapability` / `MissingConfig` Display 含 key（类型名或模块名） | 正常 | — | 无 | `src/error.rs::missing_capability_display_contains_key`、`missing_config_display_contains_key` | tests/e2e/e2e_core.rs |
| ERR-06 | `LifecycleFailed` Display 含 context+source，`source()` 返回内层（lifecycle 门控） | 正常 | lifecycle | 无 | `src/error.rs::lifecycle_failed_display_contains_context_and_source`、`error_source_returns_inner_for_lifecycle_failed` | tests/e2e/e2e_features.rs |
| ERR-07 | `ShutdownTimedOut` Display 含全部超时阶段名（shutdown 门控）→ 无内联断言 | 异常 | shutdown | 无 | 无→需新增 | tests/e2e/e2e_runtime.rs |
| ERR-08 | 无 source 变体（`MissingConfig` 等）`source()` 返回 `None`；Debug 输出含变体名 | 边界 | — | 无 | `src/error.rs::error_source_returns_none_for_simple_variants`、`error_debug_format` | tests/e2e/e2e_core.rs |
| ERR-09 | 错误 Display 双语目录绑定：en（默认回退）与 zh 两份 FTL 均覆盖全部 9 个错误消息 id（含 config-validation-failed） | 边界 | i18n | 无 | `src/i18n/messages/en.ftl`、`zh.ftl`（9 条消息 id 核对）+ `src/i18n/mod.rs::catalog_translate_with_variables` → 双 locale Display 断言需新增 | tests/e2e/e2e_i18n.rs |

### 2.19 prelude 导出面（PRE，3 条）

| ID | 场景描述 | 类型 | 涉及 feature | 依赖服务 | 既有覆盖 | E2E 落点 |
|----|---------|------|-------------|---------|---------|---------|
| PRE-01 | `prelude::*` 的 async 导出与 `async_kit` 原生标记类型同一（防 `Ready`/`Unbuilt` sync/async 错绑回归） | 正常 | async | 无 | `src/prelude.rs::prelude_async_kit_compiles`、`prelude_async_markers_match_async_kit_markers` | tests/e2e/e2e_async.rs |
| PRE-02 | confers trait 经 prelude/kit 双路径可达（`Configurable`/`Validatable`/`ModuleConfig`/`SharedConfig`/`ConfigInherit`），`confers::Config` derive 再导出可用 | 正常 | confers | 无 | `tests/basic.rs::derive_config_macro_re_exported`、`tests/config_inherit_derive.rs`（`use trait_kit::kit::ConfigInherit`） | tests/e2e/e2e_config.rs |
| PRE-03 | 派生宏（`ConfigInherit`/`SharedConfig`）不随 prelude 导出，需显式依赖 `trait-kit-derive` 的文档契约保持（防误迁移） | 边界 | confers | 无 | `src/prelude.rs`（NOTE 注释）+ `tests/config_inherit_derive.rs::derive_config_inherit_generates_override_type` | tests/e2e/e2e_presets.rs（编译面） |

### 2.20 feature 组合交互（CMP，15 条）

| ID | 场景描述 | 类型 | 涉及 feature | 依赖服务 | 既有覆盖 | E2E 落点 |
|----|---------|------|-------------|---------|---------|---------|
| CMP-01 | async+lifecycle → `AsyncLifecycle` 导出面（lib.rs/kit/mod/prelude 三处一致）且异步 on_ready 生效 | 边界 | async,lifecycle | 无 | `src/lib.rs` 导出核对 + `src/kit/async_kit.rs::async_lifecycle_on_ready_called` | tests/e2e/e2e_feature_combinations.rs |
| CMP-02 | confers+reload：`reload` 链上 `Configurable::load` 被复用于重载路径（`reload_config::<C>()` 即 `C::load()` 二次执行） | 正常 | reload,confers | 无 | `tests/e2e_feature_combinations.rs::e2e_confers_plus_reload` | tests/e2e/e2e_feature_combinations.rs |
| CMP-03 | confers+encryption：同一 C 类型明文配置与密文存储并存，加密链（encryption→confers）自动生效 | 正常 | encryption,confers | 无 | `tests/e2e_feature_combinations.rs::e2e_confers_plus_encryption` | tests/e2e/e2e_feature_combinations.rs |
| CMP-04 | lifecycle+health：on_ready 后健康报告可用，关闭后状态保持可读 | 边界 | lifecycle,health | 无 | `tests/e2e_feature_combinations.rs::e2e_lifecycle_plus_health` | tests/e2e/e2e_feature_combinations.rs |
| CMP-05 | observer+decorator：观察者时序覆盖装饰步骤（build → decorate → on_module_built） | 正常 | observer,decorator | 无 | `tests/e2e_feature_combinations.rs::e2e_observer_plus_decorator` | tests/e2e/e2e_feature_combinations.rs |
| CMP-06 | toggle+decorator：开关门控下装饰器仅对实际注册模块生效 | 正常 | toggle,decorator | 无 | `tests/e2e_feature_combinations.rs::e2e_toggle_plus_decorator` | tests/e2e/e2e_feature_combinations.rs |
| CMP-07 | scope+lifecycle：作用域与生命周期钩子共存语义 | 边界 | scope,lifecycle | 无 | `tests/e2e_feature_combinations.rs::e2e_scope_plus_lifecycle` | tests/e2e/e2e_feature_combinations.rs |
| CMP-08 | scope+async → `AsyncScope` 导出面与 `AsyncKit::create_scope` 打通 | 边界 | scope,async | 无 | `src/lib.rs`/`src/kit/mod.rs` 导出核对 + `src/kit/async_kit.rs::async_create_scope_returns_empty` | tests/e2e/e2e_feature_combinations.rs |
| CMP-09 | shutdown+async → `AsyncShutdownCoordinator` 导出面一致（lib.rs/kit/mod/prelude） | 边界 | shutdown,async | 无 | `src/kit/shutdown.rs::async_shutdown_coordinator_executes_hooks` + 导出核对 | tests/e2e/e2e_feature_combinations.rs |
| CMP-10 | shutdown+decorator：被装饰能力的关闭次序（装饰层与核心层）语义固化 | 边界 | shutdown,decorator | 无 | 无→需新增 | tests/e2e/e2e_feature_combinations.rs |
| CMP-11 | toggle+scope：开关门控 `create_scope` 后的作用域内注册 | 边界 | toggle,scope | 无 | 无→需新增 | tests/e2e/e2e_feature_combinations.rs |
| CMP-12 | interface+decorator：装饰器按 interface TypeId 应用于 `register_as` 构建路径（`build_interface_modules` 内 `apply_decorators(interface_id, …)`） | 边界 | interface,decorator | 无 | 无→需新增 | tests/e2e/e2e_feature_combinations.rs |
| CMP-13 | encryption+reload 双链共存：`encryption` 与 `reload` 同时开启时（confers 双引擎 confers/encryption + confers/watch）全部 API 可编译可用 | 异常 | encryption,reload | 无 | 无→需新增（编译级 + 行为级） | tests/e2e/e2e_feature_combinations.rs |
| CMP-14 | i18n+shutdown：`ShutdownTimedOut` 错误消息走 `tr()` 翻译链（zh locale 下错误文本本地化） | 边界 | shutdown,i18n | 无 | 无→需新增 | tests/e2e/e2e_feature_combinations.rs |
| CMP-15 | 全 feature 烟囱：`--all-features` 编译通过（已实测 exit 0）+ 全功能行为级一次打通（register/lazy/multi/interface/config/reload/encrypt/lifecycle/health/observer/decorator/scope/toggle/shutdown/i18n 同 Kit） | 正常 | 全部 13 项 | 无 | `cargo check --all-features`（本次编写时实测）→行为级需新增 | tests/e2e/e2e_feature_combinations.rs |

### 2.21 并发与竞态（CCY，7 条）

| ID | 场景描述 | 类型 | 涉及 feature | 依赖服务 | 既有覆盖 | E2E 落点 |
|----|---------|------|-------------|---------|---------|---------|
| CCY-01 | `AsyncTypeMap` 跨线程并发读写无 panic、`Arc` 克隆共享状态一致 | 边界 | async | 无 | `src/kit/async_typemap.rs::cross_thread_access_does_not_panic`、`arc_clone_shares_state` | tests/e2e/e2e_concurrency.rs |
| CCY-02 | `AsyncKit` 并发注册：多模块并发 `register` 全部成功 | 边界 | async | 无 | `src/kit/async_kit.rs::async_kit_concurrent_registration` | tests/e2e/e2e_concurrency.rs |
| CCY-03 | sync `Kit` 基于 `RefCell`（`!Sync`）的设计边界固化：跨线程共享 `Kit` 应编译失败（负向编译断言）。**等效落地**：以 `static_assertions::assert_not_impl_any!` 编译期断言固化（trybuild stderr 快照依赖 feature 集，组合矩阵下脆弱，故不走 tests/ui） | 边界 | — | 无 | `tests/basic.rs::assert_not_impl_any!(Kit<Unbuilt>: Sync)` + `!(Kit<Ready>: Sync)` | tests/basic.rs:11-12（编译期等效） |
| CCY-04 | 规模压力：100 模块注册+构建、拓扑序无栈溢出/无性能悬崖 | 边界 | — | 无 | `tests/e2e_advanced.rs::c06_large_number_of_modules_100_registers_and_builds` | tests/e2e/e2e_concurrency.rs |
| CCY-05 | 配置规模：20 个不同配置类型 set/read 全部正确 | 边界 | — | 无 | `tests/e2e_advanced.rs::c07_many_config_types_20_set_and_read` | tests/e2e/e2e_concurrency.rs |
| CCY-06 | `require_ref` 借用存活期间的 `reload_config` 并发风暴（50 轮交错）。**真实行为核正**：独立 `TypeMap` 设计下借用与配置写入互不冲突，守卫全程读到完整能力值；原推测的 borrow 冲突 panic 不成立（`set_config` 为 Unbuilt 态方法，见 RLD-07） | 边界 | reload | 无 | `src/kit/typemap.rs::inner_ref_panics_if_mutably_borrowed`（层内既有） | tests/e2e/e2e_concurrency.rs::e2e_ref_borrow_storm_interleaved_reload_set |
| CCY-07 | 1MB 加密大值与常规配置混用压力档（同 ENC-09 数据源，组合入并发场景） | 边界 | encryption | 无 | `tests/e2e_advanced.rs::c15_large_config_value_encryption_1mb` | tests/e2e/e2e_concurrency.rs |

### 2.22 feature 编译矩阵（PRS，6 条）

| ID | 场景描述 | 类型 | 涉及 feature | 依赖服务 | 既有覆盖 | E2E 落点 |
|----|---------|------|-------------|---------|---------|---------|
| PRS-01 | `cargo check`（无 feature，default=[]）全库编译 + no-feature 测试组通过（`e2e_no_feature_*`） | 边界 | — | 无 | `tests/e2e_feature_combinations.rs::e2e_no_feature_basic_build`、`e2e_no_feature_graph_export` | tests/e2e/e2e_presets.rs |
| PRS-02 | 13 个 feature 逐一单开（`--features async` 等）各自 `cargo check` 通过，导出面按门控正确伸缩（如仅 async 时无 `Lifecycle` 导出） | 边界 | 全部单 feature | 无 | 无→需新增（循环脚本） | tests/e2e/e2e_presets.rs |
| PRS-03 | `--all-features`（13 项全开）`cargo check` + 全部测试通过 | 边界 | 全部 | 无 | 本次编写时实测 `cargo check --all-features` exit 0 →固化为 CI 门禁需新增 | tests/e2e/e2e_presets.rs |
| PRS-04 | 依赖链自动生效：`--features reload` 隐式启用 `confers` 与 `confers/watch`，`Configurable` 面 API 可用 | 边界 | reload | 无 | `Cargo.toml` 声明核对 + `tests/e2e_feature_combinations.rs::e2e_confers_plus_reload` | tests/e2e/e2e_presets.rs |
| PRS-05 | 依赖链自动生效：`--features encryption` 隐式启用 `confers` 与 `confers/encryption`，XChaCha20 原语经 trait-kit 再导出可用 | 边界 | encryption | 无 | `Cargo.toml` 声明核对 + `tests/e2e_feature_combinations.rs::e2e_confers_plus_encryption` | tests/e2e/e2e_presets.rs |
| PRS-06 | examples crate 20 个示例按各自 `required-features` 门控编译（每个示例对应 feature 单独 check 通过） | 边界 | 全部 | 无 | 无→需新增（`cargo check -p trait-kit-examples --features <逐个>` 脚本） | tests/e2e/e2e_presets.rs |

---

## 3. feature 互斥 / 组合矩阵

### 3.1 依赖链（由 Cargo.toml `[features]` 固化，组合测试无需单独验证的部分）

```
reload     → confers, confers/watch
encryption → confers, confers/encryption
confers    → dep:confers, dep:serde, dep:serde_json
i18n       → dep:icu, dep:writeable, dep:sys-locale
default    → （空，无任何隐式 feature）
async / interface / lifecycle / health / scope / toggle / observer / decorator / shutdown
           → 空 feature（零依赖、零传递项）
```

结论：**不存在互斥 feature，也不存在预设（preset）**——9 个零依赖 feature 可自由叠加；约束只体现为"开 `reload`/`encryption` 自动带上 `confers` 引擎"。`cargo check --all-features`（13 项全开）实测通过（exit 0），全 feature 互容得到编译级验证。

### 3.2 跨 feature 门控（编译期联动，组合行为测试的输入依据）

| 组合 | 产生的导出/行为 | 声明位置 |
|------|----------------|---------|
| async + lifecycle | `AsyncLifecycle` | `src/lib.rs`、`src/core/mod.rs`、`src/prelude.rs` |
| async + health | `AsyncHealthCheck` | 同上 |
| async + scope | `AsyncScope` | 同上 |
| async + shutdown | `AsyncShutdownCoordinator` | 同上 |
| async（单独） | `AsyncKit`/`AsyncAutoBuilder`/`AsyncReady`/`AsyncUnbuilt`/`AsyncTypeMap` | `src/lib.rs`、`src/kit/mod.rs` |
| observer + decorator | build 主路径：先回调 `on_module_start` → build → `apply_decorators` → `on_module_built` | `src/kit/kit.rs::build_eager_modules` |
| decorator（单独） | 覆盖 eager/lazy/multi/interface 四条构建路径的包装点 | `src/kit/kit.rs`（4 处 `apply_decorators`） |
| lifecycle + health | `register_lifecycle` 与 `register_health_check` 共用能力 TypeMap（`get_ref_by_type_id`） | `src/kit/kit.rs` |
| confers + 任意 | `Config` derive 再导出 + 五大配置 trait 可用 | `src/kit/config.rs` |

### 3.3 组合测试矩阵（CMP-01…15 的输入依据）

| 组合 | 组成 feature | 交互点 | 对应场景 | 既有覆盖 |
|------|-------------|--------|---------|---------|
| C1 | async + lifecycle | `AsyncLifecycle` 异步 on_ready | CMP-01 | 有（src 内联） |
| C2 | confers + reload | load 复用于重载 | CMP-02 | 有（e2e_feature_combinations） |
| C3 | confers + encryption | 明文/密文并存 | CMP-03 | 有（e2e_feature_combinations） |
| C4 | lifecycle + health | ready 后健康可用 | CMP-04 | 有（e2e_feature_combinations） |
| C5 | observer + decorator | 回调覆盖装饰步骤 | CMP-05 | 有（e2e_feature_combinations） |
| C6 | toggle + decorator | 开关门控装饰 | CMP-06 | 有（e2e_feature_combinations） |
| C7 | scope + lifecycle | 作用域与钩子共存 | CMP-07 | 有（e2e_feature_combinations） |
| C8 | scope + async | `AsyncScope` | CMP-08 | 有（src 内联） |
| C9 | shutdown + async | `AsyncShutdownCoordinator` | CMP-09 | 有（src 内联） |
| C10 | shutdown + decorator | 关闭次序 | CMP-10 | **需新增** |
| C11 | toggle + scope | 开关门控作用域 | CMP-11 | **需新增** |
| C12 | interface + decorator | interface 构建路径装饰 | CMP-12 | **需新增** |
| C13 | encryption + reload | 双 confers 引擎共存 | CMP-13 | **需新增** |
| C14 | i18n + shutdown | 错误消息翻译链 | CMP-14 | **需新增** |
| C15 | 全 13 项 | 全功能烟囱 | CMP-15 | 编译已验 / 行为**需新增** |

---

## 4. Docker 服务需求汇总

**无。** trait-kit 是纯内存库，全部 238 个场景的依赖服务为"无"，理由如下：

1. **零 docker-compose 依赖**：仓库不存在 `docker-compose*.yml`；`Cargo.toml` 直接依赖仅 `confers`（可选）、`serde`/`serde_json`（可选）、`icu`/`writeable`/`sys-locale`（i18n 可选），无任何数据库/消息队列/网络服务客户端。
2. **confers 依赖面是纯进程内 trait**：trait-kit 仅消费 confers 的同步配置加载（`Config::load_sync` 桥接到 `Configurable::load`）、`watch`（进程内订阅回调，非文件 inotify）与 `encryption`（纯加密原语 XChaCha20/HKDF）三个能力面，**不触碰 confers 的 remote/etcd/consul/nats/redis 等需要 docker 服务的 feature**（trait-kit 的 feature 链从未开启它们）。
3. **"外部资源"仅两类且均可本地化**：本地文件（仅 confers `#[derive(Config)]` 的文件加载路径，E2E 用 `std::env::temp_dir()` 临时文件即可）与无（其余全为内存操作）。

因此：**E2E 全量可在无 docker 的 CI 沙箱执行**；"异常注入"一律使用显式错误输入（错误密钥、未注册类型、非法 locale、超时Duration）而非故障服务。无服务守卫（port guard / health check）需求。

---

## 5. 执行计划

### 5.1 层级与命令

| 层级 | 内容 | 命令 | 允许 mock？ |
|------|------|------|-----------|
| L1 单元 | src 内联 394 个 `#[test]`（16 处 `#[cfg(test)]` + 38 处 `#[cfg(all(test, feature = …))]` 门控块，覆盖 16 个文件） | `cargo test --all-features --lib` | 允许 |
| L2 集成 | `tests/{basic,e2e_advanced,e2e_feature_combinations,config_inherit_derive,shared_config_derive,config_inheritance_e2e}.rs`（185 个测试函数；注意 §重要发现-3：feature 组按 `--features` 分轮执行） | `cargo test --features confers,reload,encryption && cargo test --features async,lifecycle,health,scope,toggle,observer,decorator,shutdown,interface && cargo test --features i18n` | 允许（纯内存） |
| L2b 编译期 | `tests/ui/` 3 个 trybuild 用例（typestate 违规） | `cargo test --test compile_fail` | — |
| L3 examples | 20 个示例逐一运行（见 5.2，需按 `required-features` 传 feature） | `cargo run -p trait-kit-examples --features <f> --example <name>` | 禁 mock |
| L4 E2E | `tests/e2e/`（本文档 §2 场景 ID 落点，13 个文件已落地） | `cargo test --features <组合> --test e2e_<名>`（子目录不被自动发现，已由根 `Cargo.toml` 显式 `[[test]]` 注册 13 个目标） | 禁 mock |

前置事项（落地 E2E 前完成）：
1. 无死文件问题（与 confers 不同）：`tests/` 7 个文件全部被自动发现执行，无未注册模块。
2. 为 feature 门控的 E2E 文件统一加 `#![cfg(feature = "…")]` 头，避免无 feature 轮次编译失败。
3. 补齐 §2 中 19 处"无→需新增"场景与 13 处"…→需新增断言"场景（清单见 §6 口径）。

### 5.2 examples 运行清单（20 个，`cargo run -p trait-kit-examples --features <f> --example <name>`）

| # | 示例 | required-features | 预期结果（判验收通过） |
|---|------|-------------------|----------------------|
| 1 | `default_basic` | 无 | 退出码 0；`Kit<Unbuilt>`→`Kit<Ready>` 基础 typestate 演示输出 |
| 2 | `confers_loader` | confers | 退出码 0；`Configurable` 桥接 `#[derive(Config)]` 加载演示 |
| 3 | `confers_macros` | confers | 退出码 0；`ModuleConfig::PATH` 绑定与 derive 再导出演示 |
| 4 | `hot_reload` | reload | 退出码 0；`subscribe`/`reload_config` 回调触发输出 |
| 5 | `encryption` | encryption | 退出码 0；set/get_encrypted roundtrip 与三级继承演示 |
| 6 | `async_basic` | async | 退出码 0；`AsyncKit` 异步 typestate 演示 |
| 7 | `lifecycle` | lifecycle | 退出码 0；on_ready/on_shutdown 触发输出 |
| 8 | `health_check` | health | 退出码 0；health_report 汇总输出 |
| 9 | `observability` | observer | 退出码 0；构建回调序列输出 |
| 10 | `scope_basic` | scope | 退出码 0；作用域隔离演示 |
| 11 | `conditional` | 无 | 退出码 0；`register_if` 谓词注册双分支输出 |
| 12 | `factory` | 无 | 退出码 0；factory 每次新实例演示 |
| 13 | `decorator` | decorator | 退出码 0；能力包装前后对比输出 |
| 14 | `interface` | interface | 退出码 0；register_as/resolve dyn Trait 演示 |
| 15 | `i18n` | i18n | 退出码 0；FTL 翻译 + ICU4X 格式化输出 |
| 16 | `shutdown` | shutdown | 退出码 0；三阶段优雅关闭演示 |
| 17 | `validation` | confers | 退出码 0；合法通过+非法被拒双分支输出 |
| 18 | `snapshot_restore` | confers | 退出码 0；快照/回滚演示 |
| 19 | `toggle_basic` | toggle | 退出码 0；`register_if_toggle` 开/关双分支输出 |
| 20 | `config_inheritance` | confers | 退出码 0；ConfigInherit/SharedConfig 继承链演示 |

批跑脚本（建议固化为 `scripts/run_examples.sh`）：
`for e in default_basic confers_loader confers_macros hot_reload encryption async_basic lifecycle health_check observability scope_basic conditional factory decorator interface i18n shutdown validation snapshot_restore toggle_basic config_inheritance; do …按 5.2 表传 feature…; done`

### 5.3 场景 ID → E2E 文件映射汇总

| tests/e2e/ 文件 | 覆盖场景 | 需 docker |
|----------------|---------|-----------|
| e2e_core.rs | MET-01…09、REG-01…20、CAP-01…12、DEP-01…10、ERR-01…09、PRE-02 | 否 |
| e2e_config.rs | CFG-01…22、RLD-01…09 | 否 |
| e2e_encryption.rs | ENC-01…13 | 否 |
| e2e_features.rs | ITF-01…09、LCY-01…09、HLT-01…09、OBS-01…07、DEC-01…08 | 否 |
| e2e_runtime.rs | SCP-01…10、TGL-01…09、SHD-01…13、ERR-07 | 否 |
| e2e_async.rs | ASK-01…15、MET-04、LCY-06、HLT-08/09、OBS-07、SCP-08/09、DEC-08、PRE-01 | 否 |
| e2e_i18n.rs | I18-01…14、ERR-09（zh 侧；en 侧 Display 断言落 e2e_i18n_en.rs，独立进程锚定 locale） | 否 |
| e2e_feature_combinations.rs | CMP-01…15、RLD-08/09、ENC-13、LCY-08/09、OBS-06、DEC-06、SCP-10、TGL-08 | 否 |
| e2e_concurrency.rs | CCY-01…07 | 否 |
| e2e_presets.rs | PRS-01…06、TGL-09、PRE-03 | 否 |
| tests/ui/（扩展） | REG-19（现有 3 例）；CCY-03 已等效落地于 tests/basic.rs（见头部对账 3） | 否 |

> 实际落位共 13 个文件（上表 10 个 + 既有 `e2e_advanced.rs` 84 例为 B/A/E/C 系列场景主体承载 + `e2e_i18n_en.rs` en 侧独立进程目标）；`basic.rs`、`compile_fail.rs` 等顶层文件保持 tests/ 自动发现。

### 5.4 建议执行顺序

1. L1 全绿：`cargo test --all-features --lib`（394 个内联测试）。
2. L2 按 feature 分轮全绿（§5.1 三轮命令）+ L2b trybuild 3 例。
3. L3 20 个示例按 feature 逐一运行（5.2 清单）。
4. 落地 `tests/e2e/`（10 个文件），优先补 CMP-10…15、CCY-03/06、DEC-03/04/07/08、ASK-13、ERR-07、RLD-06、HLT-07、LCY-05、SHD-13、CAP-11（19 处纯缺口 + 13 处需补断言，既有覆盖最薄）。
5. PRS 编译门禁进 CI：无 feature / 单 feature ×13 / `--all-features` 三档，每个 PR 与 nightly 各跑一轮；examples crate 门控编译随 PRS-06 固化。

---

## 6. 统计汇总

（按本文档场景行逐条程序化统计得出，口径见备注）

| 维度 | 数量 |
|------|------|
| 场景总数 | **238** |
| 类型=正常 | 100 |
| 类型=异常 | 57 |
| 类型=边界 | 80 |
| 类型=正常/异常（双断言） | 1（I18-07） |
| 需新增的 E2E 场景（既有覆盖为"无→需新增"） | 19 |
| 需新增断言/集成的场景（已有单测或示例，但缺集成/行为级断言，"…→需新增断言"） | 13 |
| 引用 `tests/` 既有集成测试的场景（与其他口径有重叠） | 121 |
| 引用 src 内联测试的场景（与其他口径有重叠） | 164 |
| 引用 examples 验收的场景 | 7 |
| 引用 tests/ui（trybuild）的场景 | 2（REG-19 既有、CCY-03 新增落点） |
| 依赖 docker 服务的场景 | **0** |
| 依赖本地临时文件的场景 | ≤3（CFG 组 confers derive 文件加载路径） |
| 完全无外部依赖（纯内存）的场景 | 235+ |

> 既有测试资产基线（grep 实测）：src 内联 **394** 个 `#[test]`；`tests/` **185** 个测试函数（basic 45 / e2e_advanced 84 / e2e_feature_combinations 42 / config_inherit_derive 6 / shared_config_derive 5 / config_inheritance_e2e 2 / compile_fail 1 runner→3 个 ui 用例）；examples **20** 个。
> 说明：既有覆盖口径存在合法重叠（一条场景可同时引用 tests/ 集成测试与 src 内联测试）；"需新增"取最强缺口判定。后续落地阶段允许将粒度过细的场景合并实现，但 **ID 保持稳定**以便追溯。

### 与任务输入的偏离说明

1. **src 内 `#[cfg(test)]` 计数**：任务输入为 17 处，实测字面 `#[cfg(test)]` 为 **16 处**，另有 38 处 `#[cfg(all(test, feature = …))]` 门控测试块（共 394 个内联测试）。以实测为准。
2. **examples 计数**：任务输入为 21 个，实测 `examples/Cargo.toml` 显式 `[[example]]` 注册 **20 个**（`examples/src` 下亦为 20 个 .rs 文件）。以实测为准。
3. **依赖服务**：任务输入提示"仅 confers feature 需要文件/远程场景"；核对后 trait-kit 的 confers 面只含进程内 trait 桥接，文件路径仅出现在 confers derive 的文件加载（E2E 可用临时文件），**无任何远程场景**（§4）。
