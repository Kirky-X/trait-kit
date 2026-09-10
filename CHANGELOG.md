# Changelog

## [0.5.0-rc.3] — 2026-09-10

### Added

- **观测端口**：新增 `src/kit/ports.rs`，定义 `MetricsPort`（counter/gauge/histogram）与 `LogPort`（结构化 log）trait + `NoOpMetricsPort`/`NoOpLogPort` 零开销默认实现；`Kit` 与 `AsyncKit` 构建器可注入 `Option<Arc<dyn MetricsPort>>`/`Option<Arc<dyn LogPort>>`
- **Toggle 完整落地**：`src/kit/toggle.rs` 从 doc-only 升级为完整开关句柄——`ToggleBackend` trait + `ToggleValue` 类型化枚举（Bool/Int/Float/Str）+ `MemoryToggle` 内存后端；`confers` feature 启用时自动切换为 `ConfersToggle`（委托 `confers::FeatureToggleRegistry`）；Kit/Kit\<Ready\> 新增 `set_toggle`/`get_toggle`/`remove_toggle`/`list_toggles` 方法
- **AsyncKit 配置对称**：补齐 `load_config` 系列、`subscribe`/`reload_config`、`snapshot_config`/`restore_config`、`set_encrypted`/`get_encrypted` 等 11 个方法，与同步 Kit 能力一致

- **运行时事件总线**：`EventBus` port + `MemoryEventBus`（订阅者 panic 隔离）+ `NoOpEventBus` 零开销默认；`Kit`/`AsyncKit` 经 `with_event_bus` 注入，在模块构建（`ModuleBuilt`）、健康采样（`HealthChanged`）、配置变更（`ConfigChanged`）关键点发布，公开 `emit_event` 供模块自定义事件
- **AsyncKit 并发构建**：按拓扑分层对无依赖模块并发 `build`（`BatchJoin` 限流 join，`with_max_concurrency` 上限控制），依赖序语义与既有 e2e 行为完全保持
- **密钥提供方抽象**：`KeyProvider` port + `KeyBytes` 零化容器 + `ConfersKeyProvider` 适配 confers `SecretKeyProvider`；`set_encrypted_with_key_provider` 调用时取钥（fail-closed）
- **加密密钥轮换**：加密存储带 key-version 信封；`set_encrypted_with_version`/`get_encrypted_with_version`/`rotate_master_key` 旧钥迁移（错钥 fail-closed，版本递增）
- **健康聚合导出**：`health_aggregate()`/`health_json()` 输出 worst-of 整体状态 + 各模块明细，供 /healthz 直接消费
- **健康历史环形采样**：`HealthSample` 环形缓冲（容量可配）+ `record_health_history()`/`health_history()`
- **Scope 父上下文**：`create_scope_from` + `scope.parent::<T>()` 只读查询父 Kit 能力，Weak 引用循环防护
- **子 Kit 组合**：`SubKitSpec`/`SubKitModule`/`SubKitHandle`，child Kit 作为父 Kit 单模块注册（能力命名空间隔离 + 跨 Kit 依赖图验证）
- **i18n 模块化**：`ModuleMeta::i18n_ftl` 模块自带 .ftl 片段，`build()` 合并为 kit 本地翻译 overlay（`module_tr` 优先、全局 `tr()` 兜底）
- **强类型开关句柄**：`ToggleKey`/`ToggleHandle<T>` + `define_toggle_key!` 宏，编译期防 key 拼写错
- **零克隆读取**：`get_arc::<M,T>()` Arc 能力免克隆检索（同指针）；`set_config_arc`/`config_arc` Arc 配置快照（sync + async）
- **装饰器契约前移**：`try_decorate` 注册时校验目标已注册（`DecoratorTargetMissing`），类型由泛型钉死
- **能力版本协商**：`ModuleMeta::VERSION`/`required_versions` + `negotiate` feature 构建期 semver-compat 校验（`VersionIncompatible`）
- **配置变更审计**：`set_config`/`set_config_arc`/`reload_config`/`restore_config` 经 EventBus 发布 `ConfigChanged` 审计事件（含变更摘要）
- **API 文档一致性门禁**：`tests/api_reference_gate.rs` 关键项编译存在性 + docs/API_REFERENCE.md 收录双向断言

### Changed

- `reload` feature 移除 `confers/watch` 依赖（自有 SubscriberMap 机制不受影响）
- `confers` feature 启用 `confers/feature-toggle` 以接入 `FeatureToggleRegistry`

### Fixed

- `derive_kit_field_key` 改为 `pub(crate)` 供 `async_kit.rs` 跨模块访问

## [0.5.0-rc.2] — Previous release

(See git history for details)
