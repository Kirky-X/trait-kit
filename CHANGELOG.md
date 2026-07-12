# 更新日志

本项目所有显著变更将记录在此文件中。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，
并遵循 [语义化版本](https://semver.org/lang/zh-CN/v2.0.0.html)。

## [Unreleased]

## [0.3.0] - 2026-07-12

### 新增

#### Phase 1: Override + require_ref
- `Kit::override_module<M>()` — 用预构建值覆盖模块能力，跳过 build_fn（测试注入）
- `Kit::override_module_strict<M>()` — 覆盖但验证依赖存在性
- `Kit::require_ref<M>()` — 零拷贝能力检索，返回 `Ref<'_, M::Capability>`
- `TypeMap::inner_ref()` — 暴露内部 HashMap 借用

#### Phase 2: Lazy + 多绑定
- `Kit::register_lazy<M>()` — 延迟构造，首次 `require()` 时触发构建并缓存
- `Kit::register_multi<M>()` — 多绑定注册，相同能力类型聚合为 Vec
- `Kit::require_all<M>()` — 按注册顺序返回所有多绑定能力

#### Phase 3: 接口分离（feature = "interface"）
- `Interface` marker trait — 支持 `dyn Trait` 类型擦除（`?Sized` blanket impl）
- `InterfaceBuilder` 扩展 trait — 关联 `Capability` 与 `Interface`，通过 `into_interface` 执行类型擦除
- `Kit::register_as<M>()` — 按接口类型注册，`M::Interface` 作为 key
- `Kit::resolve<I>()` — 按接口类型检索 `Arc<I>`

#### Phase 4: 宏扩展
- `impl_module_meta!` 宏 — 生成 `ModuleMeta` impl（无依赖 / 有依赖两种语法）
- `impl_async_auto_builder!` 宏（feature = "async"）— 生成 `AsyncAutoBuilder` impl

### 变更

- `build()` 方法优先检查 overrides map，跳过 build_fn
- `build()` 新增 lazy_slots / multi_capabilities / interface_builders 构建循环
- `build()` 中 topo-sorted 循环对 multi-binding 和 interface 模块 `continue`（与单绑定模式一致）

## [0.2.5] - 2026-07-12

### ⚠️ BREAKING CHANGES

- `KitError` 重命名为 `TraitKitError`，遵循 `ProjectNameError` 命名约定
- 新增 `TraitKitResult<T>` 类型别名
- `error` 模块从 `src/core/error.rs` 迁移到 `src/error.rs`，导入路径 `crate::core::error::KitError` → `crate::error::TraitKitError`

## [0.2.4] - 2026-07-11

### 变更

- 无代码变更，版本号对齐 workspace 同步升级

### 变更（Phase 6 前置）

- 升级至 Rust edition 2024
- 最低支持 Rust 版本 (MSRV) 设为 1.85
- 统一采用 MIT 许可证

### 新增

- `i18n` feature：集成 ICU4X，提供区域感知的数字、日期、复数和排序能力
- `async` feature：`AsyncKit` 支持 `Send + Sync` 异步能力管理

## [0.2.3]

### 新增

- `ModuleConfig` trait：模块级配置元数据（PATH + default_value）
- 四级 confers feature flag 体系（confers / confers-macros / hot-reload / encryption）
- XChaCha20-Poly1305 加密配置存储（HKDF 密钥派生）
- 热重载订阅 API（subscribe / reload_config）

### 变更

- `Kit` 采用 typestate 模式（`Kit<Unbuilt>` → `Kit<Ready>`）
- 能力检索改为按模块类型（TypeId），移除字符串键查找

## [0.2.2]

### 新增

- `ModuleMeta` + `AutoBuilder` 标准模块接口
- `Kit` 能力与配置管理中心
- `TypeMap` 类型安全存储（以 `TypeId` 为键）
- 依赖图验证：环检测 + 拓扑排序构建

[Unreleased]: https://github.com/Kirky-X/trait-kit/compare/v0.2.5...HEAD
[0.2.5]: https://github.com/Kirky-X/trait-kit/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/Kirky-X/trait-kit/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/Kirky-X/trait-kit/releases/tag/v0.2.3
[0.2.2]: https://github.com/Kirky-X/trait-kit/releases/tag/v0.2.2
