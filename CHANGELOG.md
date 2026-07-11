# 更新日志

本项目所有显著变更将记录在此文件中。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，
并遵循 [语义化版本](https://semver.org/lang/zh-CN/v2.0.0.html)。

## [Unreleased]

### 变更

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

[Unreleased]: https://github.com/Kirky-X/trait-kit/compare/v0.2.3...HEAD
[0.2.3]: https://github.com/Kirky-X/trait-kit/releases/tag/v0.2.3
[0.2.2]: https://github.com/Kirky-X/trait-kit/releases/tag/v0.2.2
