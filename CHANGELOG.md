# Changelog

## [0.5.0-rc.3] — 2026-09-10

### Added

- **观测端口**：新增 `src/kit/ports.rs`，定义 `MetricsPort`（counter/gauge/histogram）与 `LogPort`（结构化 log）trait + `NoOpMetricsPort`/`NoOpLogPort` 零开销默认实现；`Kit` 与 `AsyncKit` 构建器可注入 `Option<Arc<dyn MetricsPort>>`/`Option<Arc<dyn LogPort>>`
- **Toggle 完整落地**：`src/kit/toggle.rs` 从 doc-only 升级为完整开关句柄——`ToggleBackend` trait + `ToggleValue` 类型化枚举（Bool/Int/Float/Str）+ `MemoryToggle` 内存后端；`confers` feature 启用时自动切换为 `ConfersToggle`（委托 `confers::FeatureToggleRegistry`）；Kit/Kit\<Ready\> 新增 `set_toggle`/`get_toggle`/`remove_toggle`/`list_toggles` 方法
- **AsyncKit 配置对称**：补齐 `load_config` 系列、`subscribe`/`reload_config`、`snapshot_config`/`restore_config`、`set_encrypted`/`get_encrypted` 等 11 个方法，与同步 Kit 能力一致

### Changed

- `reload` feature 移除 `confers/watch` 依赖（自有 SubscriberMap 机制不受影响）
- `confers` feature 启用 `confers/feature-toggle` 以接入 `FeatureToggleRegistry`

### Fixed

- `derive_kit_field_key` 改为 `pub(crate)` 供 `async_kit.rs` 跨模块访问

## [0.5.0-rc.2] — Previous release

(See git history for details)
