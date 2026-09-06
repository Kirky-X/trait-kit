# 📖 Trait-Kit 用户指南

本指南带您从安装开始，逐步掌握 trait-kit 的核心概念与进阶用法。trait-kit 是一个轻量级 Rust 库，提供标准化的模块接口和集中式能力与配置管理中心（`Kit`），采用 typestate 模式（`Kit<Unbuilt>` → `Kit<Ready>`）进行构建时验证。

## 📋 目录

<details open>
<summary>目录</summary>

- [简介](#简介)
- [快速开始](#快速开始)
- [核心概念](#核心概念)
- [配置](#配置)
- [进阶用法](#进阶用法)
- [最佳实践](#最佳实践)
- [故障排查](#故障排查)

</details>

---

## 简介

trait-kit 解决的问题是：**在应用启动时，以类型安全、可验证的方式装配模块依赖**。

- **模块**通过 `ModuleMeta` + `AutoBuilder` 两个 trait 定义统一契约，用 `impl_module_meta!` 宏一行声明。
- **Kit** 是能力与配置的集中管理中心：`Kit<Unbuilt>` 阶段注册模块与配置，`build()` 验证依赖图后得到 `Kit<Ready>`，此后只读检索能力。
- **能力**按模块类型存储和检索（`kit.require::<LoggerModule>()`），无需字符串键、无需 downcast。
- 整个 crate 标注 `#![deny(unsafe_code)]`，无任何 `unsafe` 代码。

适合"手动装配太散、完整 DI 框架太重"的中间场景。API 全集见 [📘 API 参考](API_REFERENCE.md)，设计细节见 [🏗️ 架构文档](ARCHITECTURE.md)。

---

## 快速开始

### 环境要求

- Rust **1.97.1** 或更高版本（MSRV）。

### 安装

```sh
cargo add trait-kit
```

默认特性只含核心 `Module` + `Kit`，无额外依赖负担。

### 第一个模块

三步走：定义能力 → 定义模块 → 注册、构建、使用。

```rust
use std::sync::Arc;
use trait_kit::impl_module_meta;
use trait_kit::prelude::*;

// 1. 定义能力（任意 Clone 类型）
struct StdoutLogger;
impl StdoutLogger {
    fn info(&self, msg: &str) {
        println!("[LOG] {msg}");
    }
}

// 2. 定义模块（宏一行声明 ModuleMeta）
struct LoggerModule;
impl_module_meta!(LoggerModule, "logger");
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

运行完整的可运行示例见 [examples/](../examples/README.md)。

---

## 核心概念

### Typestate 两阶段

```
Kit<Unbuilt>                    Kit<Ready>
┌─────────────────┐   build()   ┌─────────────────┐
│ register()      │ ──────────→ │ require()       │
│ set_config()    │             │ optional()      │
│ build()         │             │ contains()      │
└─────────────────┘             └─────────────────┘
```

- **`Kit<Unbuilt>`（构建阶段）**：注册模块、存入配置、声明生命周期钩子。此阶段类型上不允许检索能力——未构建的模块无法被 `require()`，这类误用会直接**编译失败**。
- **`kit.build()`**：验证依赖图（缺失依赖检测、Kahn 算法环检测 + 拓扑排序），按拓扑序构建所有模块。
- **`Kit<Ready>`（运行阶段）**：只读检索能力与配置，不可再注册。

### 能力（Capability）

能力是模块构建后产出的值，需实现 `Clone`（通常用 `Arc<T>` 包装）。存放在 Kit 内部的 `TypeMap`（以 `TypeId` 为键）中，检索方式：

| 方法 | 行为 |
|---|---|
| `kit.require::<M>()` | 检索并克隆，缺失时报错 |
| `kit.optional::<M>()` | 缺失时返回 `None` |
| `kit.require_ref::<M>()` | 零拷贝借用检索 |
| `kit.require_all::<M>()` | 返回所有多绑定能力 |
| `kit.contains::<M>()` | 检查是否已构建 |

### 依赖声明与构建顺序

模块通过 `impl_module_meta!` 的 `deps` 参数声明依赖。Kit 在 `build()` 时验证依赖图并按拓扑顺序构建，确保被依赖的模块先构建：

```rust
impl_module_meta!(StorageModule, "storage", deps = [LoggerModule]);
```

构建失败的模块会通过 `TraitKitError::BuildFailed` 上报原始错误；整个 `build()` 在应用启动前完成，依赖问题不会拖到运行时。

### 线程模型

- **同步 `Kit`**：基于 `RefCell` 内部可变性，单线程 `!Sync` 设计，无锁开销。
- **`AsyncKit`**（`async` feature）：基于 `Arc<RwLock>`，`Send + Sync`，适合数据库连接池、HTTP 客户端等异步初始化场景。

---

## 配置

配置是存储在 Kit 的 `TypeMap` 中的**类型化值**，以类型为键，无需 `ConfigKey` 样板。

### 手动存取

`set_config` 在 `Kit<Unbuilt>` 与 `Kit<Ready>` 上均可调用；模块在 `build()` 时通过 `kit.config::<C>()` 检索：

```rust
use std::sync::Arc;
use trait_kit::impl_module_meta;
use trait_kit::prelude::*;

#[derive(Clone, Debug)]
struct DbConfig {
    url: String,
    max_connections: u32,
}

struct DbPoolModule;
impl_module_meta!(DbPoolModule, "db-pool");
impl AutoBuilder for DbPoolModule {
    type Capability = Arc<DbPool>;
    type Error = TraitKitError;

    fn build(kit: &Kit) -> Result<Self::Capability, Self::Error> {
        let config: DbConfig = kit.config()?;   // 构建时读取配置
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

### 通过 confers 从环境加载 `confers`

启用 `confers` feature 后，可对接 [`confers`](https://crates.io/crates/confers) 的 `#[derive(Config)]` 宏，从环境变量/默认值加载配置：

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
kit.load_config::<AppConfig>()?;   // 从环境变量/默认值加载
```

配套 API：

| API | 用途 |
|---|---|
| `load_and_validate::<C>()` | 加载并执行 `Validatable::validate`，失败不存入 |
| `load_config_with::<C>(vars)` | 加载时做 `${VAR}` 变量替换 |
| `snapshot_config::<C>()` / `restore_config::<C>()` | 配置快照与回滚 |
| `subscribe::<C>(cb)` / `reload_config::<C>()` `reload` | 热重载订阅与触发 |

### 模块配置元数据 `confers`

`ModuleConfig` trait 为配置类型声明 `PATH` 与 `default_value()`，把配置绑定到模块的配置路径（也是加密密钥派生的因子）：

```rust,ignore
use trait_kit::kit::config::ModuleConfig;

impl ModuleConfig for AppConfig {
    const PATH: &'static str = "config/app.toml";
    fn default_value() -> Self {
        Self { host: "localhost".to_string() }
    }
}
```

### 加密配置 `encryption`

启用 `encryption` feature 后，静态配置可用 XChaCha20-Poly1305 加密存储。加密密钥通过 HKDF 从主密钥与 `ModuleConfig::PATH` 派生，同一主密钥为不同模块生成不同字段密钥：

```rust,ignore
let kit = Kit::new();
let secret = AppConfig { host: "production-db".to_string() };
let master_key = [0u8; 32]; // 32 字节主密钥

kit.set_encrypted(&secret, &master_key)?;
let kit = kit.build()?;

let decrypted: AppConfig = kit.get_encrypted(&master_key)?;  // 错误密钥会失败
```

### 配置继承 `confers`

四层配置继承体系支持跨模块/跨项目的配置复用（`trait-kit-derive` 提供 `ConfigInherit` / `SharedConfig` derive 宏）：

```rust,ignore
use trait_kit::kit::{Kit, ModuleConfig};
use trait_kit_derive::{ConfigInherit, SharedConfig};

#[derive(Clone, ConfigInherit, SharedConfig)]
#[shared(host, port)]
struct DbConfig {
    host: String,
    port: u16,
    max_connections: u32,
}

let kit = Kit::new();
kit.populate_defaults::<DbConfig>();       // 零配置默认值
kit.extract_shared::<AppConfig>();         // 从 AppConfig 提取共享字段
kit.inject_shared::<DbConfig>();           // 注入到 DbConfig
kit.merge_config::<DbConfig>(ovr);         // 编译期安全字段覆盖
```

四层机制：`merge_json_deep` 深合并 → `ConfigInherit` 字段覆盖（`Option<T>` 仅 `Some` 时覆盖）→ `SharedConfig` 共享字段 overlay → `populate_defaults` 零配置填充。原理详见 [架构文档](ARCHITECTURE.md)。

---

## 进阶用法

### 异步模块 `async`

`AsyncKit` 与 `Kit` API 对称，能力类型要求 `Send + Sync`，构建方法为异步（`AsyncAutoBuilder`），使用 `AsyncKit::new()` → `register::<M>()` → `build().await` → `require::<M>().await` 的流程。

完整可运行示例见 [`examples/src/kit/async_basic.rs`](../examples/src/kit/async_basic.rs)：

```sh
cargo run -p trait-kit-examples --example async_basic --features async
```

### 特性门控能力一览

以下能力均通过 `Kit` / `AsyncKit` 上的方法使用（方法级 feature 门控，详见 [API 参考](API_REFERENCE.md)）：

| 能力 | Feature | 典型用途 |
|---|---|---|
| 生命周期钩子（`on_ready` / `on_shutdown`） | `lifecycle` | 构建后初始化、清理释放 |
| 健康检查（`HealthCheck` + `health_report`） | `health` | 运行时状态探针 |
| 作用域（`Scope` 每请求隔离） | `scope` | Web 请求级实例 |
| 特性开关（`enable_toggle` / `register_if_toggle`） | `toggle` | 运行时启停模块 |
| 构建观察者（`BuildObserver` 回调） | `observer` | 构建耗时统计、日志 |
| 装饰器（`decorate::<M>(f)`） | `decorator` | 能力包装（如加日志、指标） |
| 优雅关闭（`ShutdownCoordinator` 分阶段关闭 + 超时） | `shutdown` | 服务下线编排 |
| 接口分离（`register_as` / `resolve::<dyn Trait>()`） | `interface` | 面向接口编程、测试替身 |
| 国际化格式化（`I18nFormatter`） | `i18n` | 区域感知数字/日期/复数/排序（`tr()` 翻译默认可用） |

### 条件注册与工厂

无需任何 feature 即可使用：

- `kit.register_if::<M>(pred)` — 运行时谓词控制是否注册模块；
- `kit.factory::<M>()` — 每次调用创建新实例（对比 `require()` 的单例语义）。

### 测试注入

用 `override_module::<M>(cap)` 以预构建值覆盖模块能力，跳过真实构建函数；`override_module_strict::<M>(cap)` 额外验证依赖存在性：

```rust,ignore
kit.override_module::<LoggerModule>(Arc::new(test_logger));
let kit = kit.build()?;
```

---

## 最佳实践

1. **用宏声明模块**：`impl_module_meta!` / `impl_auto_builder!` 消除样板；依赖一律写进 `deps = [...]`，让环检测与缺失依赖检测在启动前生效。
2. **能力用 `Arc<T>` 包装**：能力需 `Clone`；共享可变服务用 `Arc<Service>`，避免深拷贝。
3. **配置显式声明**：需要配置的模块在 `build()` 里 `kit.config::<C>()`，缺失时 fail-fast（`MissingConfig`），不要在能力内部静默兜底。
4. **按需启用 feature**：高级 confers feature 自动继承低级（`encryption` → `reload` → `confers`），启用最高级别即可；不用的 feature 保持关闭以最小化编译与依赖。
5. **单线程用 `Kit`，多线程用 `AsyncKit`**：不要试图跨线程共享 `Kit`（`!Sync` 是有意设计）。
6. **测试用 `override_module`**：替换外部依赖（数据库、网络）为受控实现，保持测试确定性。
7. **错误处理统一走 `TraitKitError`**：其 `Display` 通过 `tr()` 自动本地化，业务侧只需 `match` 需要特殊处理的变体。

---

## 故障排查

常见错误均以 `TraitKitError` 变体出现，`Display` 自动本地化输出：

| 错误 | 触发场景 | 处理建议 |
|---|---|---|
| `CycleDetected { cycle }` | 依赖图中存在环 | 检查 `deps = [...]` 声明，打破循环（拆分模块或引入中间模块） |
| `DependencyMissing { module, missing }` | 声明的依赖未注册 | 在 `build()` 前补齐 `kit.register::<MissingModule>()` |
| `AlreadyRegistered { module }` | 模块重复注册 | 同一模块只需注册一次；多实例需求用 `register_multi` |
| `BuildFailed { context, source }` | 模块构建函数返回错误 | 查看 `source` 中的原始错误，通常是外部资源不可用 |
| `MissingCapability { key }` | `require::<M>()` 时能力不存在 | 确认模块已注册且 `build()` 已完成；可选场景改用 `optional::<M>()` |
| `MissingConfig { key }` | `config::<C>()` 时配置不存在 | 先 `set_config::<C>(value)` 或用 confers 的 `load_config::<C>()` |
| `LifecycleFailed { .. }` `lifecycle` | `on_ready` 钩子失败 | 检查钩子内的初始化逻辑与外部依赖 |
| `ShutdownTimedOut { phases }` `shutdown` | 优雅关闭超时 | 调整 `set_phase_timeout` / `set_global_timeout`，检查钩子是否阻塞 |

其他常见问题：

- **typestate 编译错误**（如 `ready_cannot_register`）：在 `Kit<Ready>` 上调用注册方法、或在 `Kit<Unbuilt>` 上调用 `require()` 都会编译失败——这是有意的构建期防护，请检查调用阶段。
- **feature 方法不存在**：方法级门控 API（如 `subscribe`、`set_encrypted`）需启用对应 feature，参见 [特性标志说明](../README.md#-特性标志)。
- **跨线程使用 `Kit` 报 `!Sync`**：改用 `AsyncKit`（`async` feature），或在单线程内使用 `Kit`。

仍有问题？请到 [GitHub Issues](https://github.com/Kirky-X/trait-kit/issues) 搜索或提问。
