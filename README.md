<p align="center">
  <img src="assets/trait-kit.svg" width="200" alt="trait-kit logo">
</p>

[![Crates.io][crates-badge]][crates-url][![Docs.rs][docs-badge]][docs-url][![MIT licensed][license-badge]][license-url][![MSRV][msrv-badge]][msrv-url]

[crates-badge]: https://img.shields.io/crates/v/trait-kit?style=flat-square
[crates-url]: https://crates.io/crates/trait-kit
[docs-badge]: https://img.shields.io/docsrs/trait-kit?style=flat-square
[docs-url]: https://docs.rs/trait-kit
[license-badge]: https://img.shields.io/badge/license-MIT-blue?style=flat-square
[license-url]: https://github.com/Kirky-X/trait-kit/blob/main/LICENSE
[msrv-badge]: https://img.shields.io/badge/MSRV-1.91-orange?style=flat-square
[msrv-url]: https://github.com/Kirky-X/trait-kit

**trait-kit** 是一个轻量级 Rust 库，提供标准化的模块接口和集中的能力与配置管理中心（`Kit`）。采用 typestate 模式（`Kit<Unbuilt>` → `Kit<Ready>`）进行构建期校验，基于 `RefCell` 的内部可变性实现单线程、按设计 `!Sync`。

[English](README_EN.md) | 中文

---

## 特性

- **标准化模块接口** — `ModuleMeta` + `AutoBuilder` trait 定义统一契约：每个模块声明其名称、依赖、能力类型和构建逻辑。到处都是一致的初始化方式。
- **Typestate 构建校验** — `Kit<Unbuilt>` 注册模块和配置；`kit.build()` 校验依赖图（循环检测、缺失依赖）并返回 `Kit<Ready>`。构建错误在应用启动前暴露。
- **类型安全的能力检索** — 能力按模块类型存储和检索（`kit.require::<LoggerModule>()`），而非字符串 key。无需向下转型，无需运行时查找。
- **配置中心** — `kit.set_config(value)` / `kit.config::<C>()` 通过以 `TypeId` 为 key 的 `TypeMap` 存取类型化配置。无需 `ConfigKey` 或 `ConfigHandle` 样板代码。
- **可选 confers 集成** — 四级 feature flag 集成 [`confers`](https://crates.io/crates/confers)，提供 derive 宏配置加载、热重载订阅和 XChaCha20-Poly1305 加密配置存储。
- **最小依赖** — 仅需 `thiserror`。`confers`、`serde`、`serde_json` 均为可选，仅在启用对应 feature 时引入。
- **`#![deny(unsafe_code)]`** — 整个 crate 无任何 `unsafe`。

---

## 快速开始

### MSRV

最低支持的 Rust 版本：**1.91**

### 安装

```sh
cargo add trait-kit
```

### 最小示例

定义一个 logger 模块，注册它，构建 Kit，然后检索能力：

```rust
use std::sync::Arc;
use trait_kit::prelude::*;

// 1. 定义一个能力（任何 Clone 类型）
struct StdoutLogger;
impl StdoutLogger {
    fn info(&self, msg: &str) {
        println!("[LOG] {msg}");
    }
}

// 2. 定义一个模块（ModuleMeta + AutoBuilder）
struct LoggerModule;
impl ModuleMeta for LoggerModule {
    const NAME: &'static str = "logger";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        &[]
    }
}
impl AutoBuilder for LoggerModule {
    type Capability = Arc<StdoutLogger>;
    type Error = KitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(StdoutLogger))
    }
}

// 3. 注册、构建并使用
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

## 用法

### 带配置的模块

配置是存储在 Kit 的 `TypeMap` 中的类型化值。模块在构建期间通过 `kit.config::<C>()` 检索：

```rust
use std::sync::Arc;
use trait_kit::prelude::*;

#[derive(Clone, Debug)]
struct DbConfig {
    url: String,
    max_connections: u32,
}

struct DbPool {
    config: DbConfig,
}

struct DbPoolModule;
impl ModuleMeta for DbPoolModule {
    const NAME: &'static str = "db_pool";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        &[]
    }
}
impl AutoBuilder for DbPoolModule {
    type Capability = Arc<DbPool>;
    type Error = KitError;
    fn build(kit: &Kit) -> Result<Self::Capability, Self::Error> {
        let config: DbConfig = kit.config()?;
        Ok(Arc::new(DbPool { config }))
    }
}

fn main() {
    let mut kit = Kit::new();
    kit.set_config(DbConfig {
        url: "postgres://localhost".into(),
        max_connections: 10,
    });
    kit.register::<DbPoolModule>().unwrap();
    let kit = kit.build().unwrap();

    let pool = kit.require::<DbPoolModule>().unwrap();
    assert_eq!(pool.config.max_connections, 10);
}
```

### 带依赖的模块

模块通过 `ModuleMeta::dependencies()` 声明依赖。Kit 在构建期校验依赖图并按拓扑序构造模块：

```rust
use std::sync::Arc;
use trait_kit::prelude::*;

struct Logger;
impl Logger {
    fn info(&self, msg: &str) { println!("[LOG] {msg}"); }
}

struct LoggerModule;
impl ModuleMeta for LoggerModule {
    const NAME: &'static str = "logger";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] { &[] }
}
impl AutoBuilder for LoggerModule {
    type Capability = Arc<Logger>;
    type Error = KitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(Logger))
    }
}

struct Storage {
    _logger: Arc<Logger>,
}

struct StorageModule;
impl ModuleMeta for StorageModule {
    const NAME: &'static str = "storage";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        static DEPS: &[(&str, std::any::TypeId)] =
            &[("logger", std::any::TypeId::of::<LoggerModule>())];
        DEPS
    }
}
impl AutoBuilder for StorageModule {
    type Capability = Arc<Storage>;
    type Error = KitError;
    fn build(kit: &Kit) -> Result<Self::Capability, Self::Error> {
        let logger = kit.require::<LoggerModule>()?;
        Ok(Arc::new(Storage { _logger: logger }))
    }
}

fn main() {
    let mut kit = Kit::new();
    kit.register::<LoggerModule>().unwrap();
    kit.register::<StorageModule>().unwrap();
    let kit = kit.build().unwrap();

    let storage = kit.require::<StorageModule>().unwrap();
    let _ = storage;
}
```

### Kit API 概览

| 方法                                 | 可用状态        | 描述                                                   |
| ----------------------------------- | --------------- | ------------------------------------------------------ |
| `Kit::new()`                        | —               | 创建一个空的 `Kit<Unbuilt>`。                          |
| `kit.register::<M>()`              | `Kit<Unbuilt>`  | 注册一个模块以供构造。                                 |
| `kit.set_config::<C>(value)`       | `Kit<Unbuilt>`  | 存储一个类型化配置值。                                 |
| `kit.config::<C>()`                | 两者皆可        | 检索一个克隆的配置值。                                 |
| `kit.build()`                       | `Kit<Unbuilt>`  | 校验依赖图并构建所有模块 → `Kit<Ready>`。              |
| `kit.require::<M>()`               | `Kit<Ready>`    | 检索一个能力（缺失时返回错误）。                       |
| `kit.optional::<M>()`              | `Kit<Ready>`    | 检索一个能力（缺失时返回 `None`）。                    |
| `kit.contains::<M>()`              | `Kit<Ready>`    | 检查某个能力是否已构建。                               |
| `kit.contains_config::<C>()`       | `Kit<Ready>`    | 检查某个配置值是否存在。                               |

---

## AsyncKit：异步能力管理

`AsyncKit` 是同步 `Kit` 的 `Send + Sync` 异步对应版本，采用 `Arc<RwLock>` 替代 `RefCell` 实现内部可变性，可跨线程共享、跨 `.await` 持有。镜像同步 `Kit` 的 typestate 模式（`AsyncKit<Unbuilt>` → `AsyncKit<Ready>`），支持异步模块构造（数据库连接池、HTTP 客户端、缓存后端）和跨模块依赖注入。

- **`Send + Sync`** — `AsyncKit` 本身可跨线程共享，能力对象要求 `Clone + Send + Sync + 'static`。
- **异步拓扑构造** — `build().await` 按依赖图拓扑序逐个调用模块的异步 `build`，循环检测和缺失依赖在启动期暴露。
- **跨模块依赖注入** — 模块 `build` 回调中可 `kit.require::<DepModule>()?` 获取已构造的依赖能力。
- **无额外依赖** — `async` feature 仅启用 Rust 原生 async，不引入 `async-trait` 或运行时依赖。

### Feature 启用

`AsyncKit` 通过 `async` feature 启用，不引入额外依赖：

```toml
[dependencies]
trait-kit = { version = "0.2.2", features = ["async"] }
```

> 运行时（如 `tokio`）由应用自行选择，trait-kit 不绑定。

### 最小示例

定义一个异步 logger 模块，注册、异步构建并检索能力：

```rust
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use trait_kit::prelude::*;

// 1. 能力类型：Clone + Send + Sync
#[derive(Clone)]
struct Logger { name: String }
impl Logger {
    fn info(&self, msg: &str) { println!("[{}] {msg}", self.name); }
}

// 2. 异步模块：ModuleMeta + AsyncAutoBuilder
struct LoggerModule;
impl ModuleMeta for LoggerModule {
    const NAME: &'static str = "logger";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] { &[] }
}
impl AsyncAutoBuilder for LoggerModule {
    type Capability = Arc<Logger>;
    type Error = KitError;
    fn build<'a>(
        _kit: &'a AsyncKit,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>> {
        Box::pin(async move {
            Ok(Arc::new(Logger { name: "async-logger".into() }))
        })
    }
}

// 3. 注册、异步构建、检索
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut kit = AsyncKit::new();
    kit.register::<LoggerModule>()?;

    let kit = kit.build().await?;                  // 拓扑构造
    let logger = kit.require::<LoggerModule>()?;   // Arc<Logger>
    logger.info("Hello from AsyncKit!");
    assert!(kit.contains::<LoggerModule>());
    Ok(())
}
```

### 跨模块依赖注入

模块通过 `ModuleMeta::dependencies()` 声明依赖，`build().await` 按拓扑序构造。依赖模块在 `build` 回调中通过 `kit.require::<DepModule>()?` 获取已构造的能力：

```rust
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use trait_kit::prelude::*;

struct Logger;
impl Logger { fn info(&self, msg: &str) { println!("[LOG] {msg}"); } }

struct LoggerModule;
impl ModuleMeta for LoggerModule {
    const NAME: &'static str = "logger";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] { &[] }
}
impl AsyncAutoBuilder for LoggerModule {
    type Capability = Arc<Logger>;
    type Error = KitError;
    fn build<'a>(
        _kit: &'a AsyncKit,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>> {
        Box::pin(async move { Ok(Arc::new(Logger)) })
    }
}

struct Storage { logger: Arc<Logger> }

struct StorageModule;
impl ModuleMeta for StorageModule {
    const NAME: &'static str = "storage";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        static DEPS: &[(&str, std::any::TypeId)] =
            &[("logger", std::any::TypeId::of::<LoggerModule>())];
        DEPS
    }
}
impl AsyncAutoBuilder for StorageModule {
    type Capability = Arc<Storage>;
    type Error = KitError;
    fn build<'a>(
        kit: &'a AsyncKit,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>> {
        Box::pin(async move {
            // 拓扑序保证 LoggerModule 已构造
            let logger = kit.require::<LoggerModule>()?;
            Ok(Arc::new(Storage { logger }))
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut kit = AsyncKit::new();
    kit.register::<LoggerModule>()?;
    kit.register::<StorageModule>()?;   // 声明依赖，注册顺序无关
    let kit = kit.build().await?;
    let storage = kit.require::<StorageModule>()?;
    storage.logger.info("dependency injected");
    Ok(())
}
```

### `AsyncKit` API 概览

| 方法                                 | 可用状态          | 描述                                                       |
| ----------------------------------- | ----------------- | ---------------------------------------------------------- |
| `AsyncKit::new()`                   | —                 | 创建一个空的 `AsyncKit<Unbuilt>`。                         |
| `kit.register::<M>()`               | `AsyncKit<Unbuilt>` | 注册一个异步模块以供构造。                                |
| `kit.set_config::<C>(value)`        | `AsyncKit<Unbuilt>` | 存储一个类型化配置值（`Send + Sync`）。                   |
| `kit.config::<C>()`                 | 两者皆可          | 检索一个克隆的配置值。                                     |
| `kit.build().await`                 | `AsyncKit<Unbuilt>` | 校验依赖图并异步构建所有模块 → `AsyncKit<Ready>`。         |
| `kit.require::<M>()`                | 两者皆可          | 检索一个能力（缺失时返回错误）。                           |
| `kit.optional::<M>()`               | `AsyncKit<Ready>`   | 检索一个能力（缺失时返回 `None`）。                        |
| `kit.contains::<M>()`               | `AsyncKit<Ready>`   | 检查某个能力是否已构建。                                   |
| `kit.contains_config::<C>()`        | `AsyncKit<Ready>`   | 检查某个配置值是否存在。                                   |

---

## 配置：confers 集成

trait-kit 通过四级 feature flag 集成 [`confers`](https://crates.io/crates/confers) 0.4。每一级继承前一级，形成分层能力系统。

### Feature Flag

| Feature               | 启用                                            | 描述                                             |
| --------------------- | ----------------------------------------------- | ------------------------------------------------ |
| `confers`             | `dep:confers`, `dep:serde`                      | `Configurable` trait + `Kit::load_config`        |
| `confers-macros`      | `confers`                                       | `ModuleConfig` trait + `Config` derive 再导出    |
| `hot-reload`  | `confers-macros`, `confers/watch`               | `subscribe` / `reload_config` API                |
| `encryption`  | `hot-reload`, `confers/encryption`, `dep:serde_json` | `set_encrypted` / `get_encrypted` API |

在 `Cargo.toml` 中启用所需级别：

```toml
[dependencies]
trait-kit = { version = "0.2", features = ["encryption"] }
```

### 三层继承系统

1. **模块能力继承**（Layer 1）：`ModuleConfig` trait 声明 `PATH` 和 `default_value()`，将配置类型绑定到其模块的配置路径。

2. **Cargo feature 继承**（Layer 2）：每一级 feature 继承前一级（`encryption` → `hot-reload` → `confers-macros` → `confers`）。启用高级别会自动启用所有低级别。

3. **配置值继承**（Layer 3）：加密密钥通过 HKDF 从 `ModuleConfig::PATH` 派生，因此同一主密钥可为不同模块生成不同的字段密钥。

### Level 1：配置加载器模式

定义一个 `Configurable` 实现，桥接到 confers 的 `#[derive(Config)]` 宏：

```rust,ignore
use trait_kit::prelude::*;
use trait_kit::kit::Config;

#[derive(Debug, Clone, PartialEq, serde::Deserialize, Config)]
#[config(env_prefix = "APP_")]
struct AppConfig {
    #[config(default = "localhost".to_string())]
    host: String,
}

impl Configurable for AppConfig {
    fn load() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(AppConfig::load_sync()?)
    }
}

let kit = Kit::new();
kit.load_config::<AppConfig>()?;  // 通过 confers 从环境变量/默认值加载
let kit = kit.build()?;
let config: AppConfig = kit.config()?;
```

### Level 2：模块配置元数据

添加 `ModuleConfig` 以声明配置路径和默认值：

```rust,ignore
use trait_kit::kit::config::ModuleConfig;

impl ModuleConfig for AppConfig {
    const PATH: &'static str = "config/app.toml";
    fn default_value() -> Self {
        Self { host: "localhost".to_string() }
    }
}
```

### Level 3：热重载订阅

订阅回调在配置重新加载时触发：

```rust,ignore
use std::cell::Cell;
use std::rc::Rc;

let kit = Kit::new();
let called = Rc::new(Cell::new(false));
let called_clone = Rc::clone(&called);
kit.subscribe::<AppConfig>(move || {
    called_clone.set(true);
});

kit.reload_config::<AppConfig>()?;  // 通过 Configurable::load 重新加载，通知订阅者
assert!(called.get());
```

### Level 4：加密配置存储

使用 XChaCha20-Poly1305 加密静态配置。加密密钥通过 HKDF 从主密钥和 `ModuleConfig::PATH` 派生：

```rust,ignore
let kit = Kit::new();
let secret = AppConfig { host: "production-db".to_string() };
let master_key = [0u8; 32]; // 32 字节主密钥

kit.set_encrypted(&secret, &master_key)?;
let kit = kit.build()?;

// 仅在主密钥正确时可解密
let decrypted: AppConfig = kit.get_encrypted(&master_key)?;
assert_eq!(decrypted, secret);
```

---

## 为什么选择 trait-kit？

trait-kit 介于"纯手动接线"和"完整 DI 框架"之间：

| 方案                     | 优点                                       | 缺点                                           |
| ------------------------ | ------------------------------------------ | ---------------------------------------------- |
| **手动接线**             | 简单，无依赖。                             | 模式临时化，每个项目不一致。                   |
| **trait-kit**            | 标准模式，类型安全，轻量。                 | 仍需显式接线依赖。                             |
| **完整 DI（shaku 等）**  | 自动解析，胶水代码少。                     | 依赖更重，魔法多，调试困难。                   |

trait-kit 给你 DI 框架的**标准化** + 手动接线的**显式性**。

---

## 贡献

### 构建要求

- Rust **1.91** 或更高版本（stable）。
- 无需外部工具（无需 protoc、openssl 或系统库）。

### 开发命令

```sh
# 运行所有测试（默认 feature）
cargo test

# 运行所有测试（所有 confers feature）
cargo test --all-features

# Lint
cargo clippy --all-features -- -D warnings

# 格式检查
cargo fmt --check
```

### 行为准则

本项目遵循 [Rust 行为准则](https://www.rust-lang.org/policies/code-of-conduct)。所有贡献者都应遵守。

### Pull Request 流程

1. 确保所有测试通过且 Clippy 无告警（`cargo clippy --all-features -- -D warnings`）。
2. 为新功能添加测试。
3. 保持 README 与 API 变更同步。

---

## 许可证

本项目基于 [MIT 许可证](LICENSE)授权。

© 2026 Kirky.X
