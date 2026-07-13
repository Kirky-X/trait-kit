<div align="center">

[![CI][ci-badge]][ci-url] [![crates.io][crates-badge]][crates-url] [![docs.rs][docs-badge]][docs-url] [![downloads][downloads-badge]][downloads-url] [![MIT licensed][license-badge]][license-url] [![Rust 1.91+][rust-badge]][rust-url]

[English](./README_EN.md)

</div>

# trait-kit

**trait-kit** 是一个轻量级 Rust 库，提供标准化的模块接口和集中式能力与配置管理中心（`Kit`）。采用 typestate 模式（`Kit<Unbuilt>` → `Kit<Ready>`）进行构建时验证，基于 `RefCell` 的内部可变性实现单线程设计（`!Sync`）。

---

## ✨ 核心特性

- **标准化模块接口** — `ModuleMeta` + `AutoBuilder` trait 定义统一契约：每个模块声明其名称、依赖、能力类型和构建逻辑，确保初始化方式一致。
- **Typestate 构建验证** — `Kit<Unbuilt>` 注册模块和配置；`kit.build()` 验证依赖图（环检测、缺失依赖检测）并返回 `Kit<Ready>`，构建错误在应用启动前暴露。
- **类型安全的能力检索** — 能力按模块类型存储和检索（`kit.require::<LoggerModule>()`），而非字符串键。无需 downcast，无需运行时查找。
- **配置中心** — `kit.set_config(value)` / `kit.config::<C>()` 通过 `TypeMap`（以 `TypeId` 为键）存储和检索类型化配置，无需 `ConfigKey` 或 `ConfigHandle` 样板代码。
- **可选 confers 集成** — 四级 feature flag 集成 [`confers`](https://crates.io/crates/confers)，支持 derive 宏配置加载、热重载订阅和 XChaCha20-Poly1305 加密配置存储。
- **`AsyncKit` 异步支持** — `async` feature 提供 `AsyncKit`，支持 `Send + Sync` 的异步能力管理，适用于数据库连接池、HTTP 客户端等异步初始化场景。
- **ICU4X 国际化** — `i18n` feature 集成 ICU4X，提供区域感知的数字、日期、复数和排序能力。
- **最小依赖** — 仅 `thiserror` 为必需依赖。`confers`、`serde`、`serde_json`、`icu` 均为可选，仅在启用对应 feature 时引入。
- **`#![deny(unsafe_code)]`** — 整个 crate 无任何 `unsafe` 代码。

---

## 🌍 跨平台支持

trait-kit 在 Linux、macOS (apple)、Windows 三大平台上经过 CI 验证，确保跨平台兼容性。

---

## 📦 快速开始

### 安装

```sh
cargo add trait-kit
```

### 基础使用

定义一个 logger 模块，注册、构建 Kit，然后检索能力：

```rust
use std::sync::Arc;
use trait_kit::prelude::*;

// 1. 定义能力（任意 Clone 类型）
struct StdoutLogger;
impl StdoutLogger {
    fn info(&self, msg: &str) {
        println!("[LOG] {msg}");
    }
}

// 2. 定义模块（实现 ModuleMeta + AutoBuilder）
struct LoggerModule;
impl ModuleMeta for LoggerModule {
    const NAME: &'static str = "logger";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        &[]
    }
}
impl AutoBuilder for LoggerModule {
    type Capability = Arc<StdoutLogger>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(StdoutLogger))
    }
}

// 3. 注册、构建、使用
fn main() {
    let mut kit = Kit::new();
    kit.register::<LoggerModule>().unwrap();
    let kit = kit.build().unwrap();

    let logger = kit.require::<LoggerModule>().unwrap();
    logger.info("Hello from trait-kit!");
    assert!(kit.contains::<LoggerModule>());
}
```

---

## 🔧 特性标志

| Feature | 启用 | 说明 |
| --- | --- | --- |
| `default` | — | 无额外特性，仅核心 `Module` + `Kit`。 |
| `async` | — | `AsyncKit`：`Send + Sync` 异步能力管理，无需额外依赖。 |
| `confers` | `dep:confers`, `dep:serde` | `Configurable` trait + `Kit::load_config`。 |
| `confers-macros` | `confers` | `ModuleConfig` trait + `Config` derive 宏再导出。 |
| `hot-reload` | `confers-macros`, `confers/watch` | `subscribe` / `reload_config` 热重载 API。 |
| `encryption` | `hot-reload`, `confers/encryption`, `dep:serde_json` | `set_encrypted` / `get_encrypted` 加密配置存储。 |
| `i18n` | `dep:icu`, `dep:writeable` | ICU4X 国际化：区域感知的数字/日期/复数/排序。 |
| `interface` | — | 接口/实现分离：`register_as` / `resolve` 支持 `dyn Trait` 类型擦除注册与检索。 |

在 `Cargo.toml` 中启用所需级别：

```toml
[dependencies]
trait-kit = { version = "0.3", features = ["encryption"] }
```

---

## 🏗️ 架构

```text
src/
├── lib.rs          # crate 入口，#![deny(unsafe_code)]
├── prelude.rs      # 常用类型再导出
├── core/
│   ├── mod.rs      # 模块声明
│   ├── meta.rs     # ModuleMeta + AutoBuilder + AsyncAutoBuilder trait
│   └── error.rs    # TraitKitError 错误类型
├── kit/
│   ├── mod.rs      # Kit 模块声明
│   ├── kit.rs      # Kit<Unbuilt> → Kit<Ready> typestate 实现
│   ├── graph.rs    # 依赖图：环检测 + 拓扑排序
│   ├── typemap.rs  # TypeMap：以 TypeId 为键的能力存储
│   ├── async_kit.rs    # AsyncKit（async feature）
│   ├── async_typemap.rs # AsyncTypeMap（async feature）
│   └── config.rs   # confers 集成（confers feature）
└── i18n/
    └── mod.rs      # ICU4X 国际化（i18n feature）
```

**核心设计**：

- **Typestate 模式**：`Kit<Unbuilt>` → `Kit<Ready>`，构建时验证依赖图，运行时零开销。
- **内部可变性**：基于 `RefCell`，单线程 `!Sync` 设计，避免锁开销。
- **三级继承体系**（confers 集成）：模块能力继承 → Cargo feature 继承 → 配置值继承（HKDF 密钥派生）。

---

## 📚 文档

- [API 文档 (docs.rs)][docs-url]
- [更新日志](CHANGELOG.md)
- [贡献指南](CONTRIBUTING.md)

---

## 🤝 贡献

详见 [CONTRIBUTING.md](CONTRIBUTING.md)。

---

## 📋 更新日志

详见 [CHANGELOG.md](CHANGELOG.md)。

---

## 📄 许可证

MIT License, Copyright (c) 2026 Kirky.X

详见 [LICENSE](LICENSE)。

[ci-badge]: https://github.com/Kirky-X/trait-kit/actions/workflows/ci.yml/badge.svg
[ci-url]: https://github.com/Kirky-X/trait-kit/actions/workflows/ci.yml
[crates-badge]: https://img.shields.io/crates/v/trait-kit?style=flat-square
[crates-url]: https://crates.io/crates/trait-kit
[docs-badge]: https://img.shields.io/docsrs/trait-kit?style=flat-square
[docs-url]: https://docs.rs/trait-kit
[downloads-badge]: https://img.shields.io/crates/d/trait-kit?style=flat-square
[downloads-url]: https://crates.io/crates/trait-kit
[license-badge]: https://img.shields.io/badge/license-MIT-blue?style=flat-square
[license-url]: https://github.com/Kirky-X/trait-kit/blob/main/LICENSE
[rust-badge]: https://img.shields.io/badge/rust-1.91+-orange?style=flat-square
[rust-url]: https://www.rust-lang.org
