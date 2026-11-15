# 📖 Trait-Kit 用户指南

本指南带您从安装开始，逐步掌握 trait-kit 的核心概念与进阶用法。trait-kit 是一个轻量级 Rust 库，提供标准化的模块接口和集中式能力与配置管理中心（`Kit`），采用 typestate 模式（`Kit<Unbuilt>` → `Kit<Ready>`）进行构建时验证。

## 📋 目录

<details open>
<summary>📑 目录</summary>

- [🌟 简介](#-简介)
- [🚀 快速开始](#-快速开始)
- [🧭 核心概念](#-核心概念)
- [⚙️ 配置](#️-配置)
- [🔧 进阶用法](#-进阶用法)
- [✅ 最佳实践](#-最佳实践)
- [🩺 故障排查](#-故障排查)
- [🧩 更多能力（注册模式、构建报告、组合与协商）](#-更多能力注册模式构建报告组合与协商)
- [💡 为什么选择 trait-kit？](#-为什么选择-trait-kit)
- [🔗 带依赖的模块](#-带依赖的模块)
- [🎨 特性标志](#-特性标志)

</details>

---

## 🌟 简介

trait-kit 解决的问题是：**在应用启动时，以类型安全、可验证的方式装配模块依赖**。

- **模块**通过 `ModuleMeta` + `AutoBuilder` 两个 trait 定义统一契约，用 `impl_module_meta!` 宏一行声明。
- **Kit** 是能力与配置的集中管理中心：`Kit<Unbuilt>` 阶段注册模块与配置，`build()` 验证依赖图后得到 `Kit<Ready>`，此后只读检索能力。
- **能力**按模块类型存储和检索（`kit.require::<LoggerModule>()`），无需字符串键、无需 downcast。
- 整个 crate 标注 `#![deny(unsafe_code)]`，生产代码仅 5 处经 SAFETY 论证与 code review 的局部豁免（typestate 布局断言 cast ×3、volatile 清零 ×1、guard 生命周期重锚 ×1：`AsyncTypeMap` 的 `RwLock` 读守卫；同步 `TypeMap` 改用 `Ref::map` 守卫包装，无需 unsafe），另有 1 处测试模块豁免（`src/kit/config.rs` 的 zeroize spare-capacity 断言），详见 [安全文档](SECURITY.md)。

适合"手动装配太散、完整 DI 框架太重"的中间场景。API 全集见 [📘 API 参考](API_REFERENCE.md)，设计细节见 [🏗️ 架构文档](ARCHITECTURE.md)。

---

## 🚀 快速开始

### 环境要求

- Rust **1.97.1** 或更高版本（MSRV）。

### 安装

```sh
cargo add trait-kit
```

默认特性（`default = []`）只含核心 `ModuleMeta` + `AutoBuilder` + `Kit`。crate 固定携带 Fluent 翻译（`fluent-bundle` / `unic-langid`）与 `log` 门面 3 个必选依赖，其余依赖（`confers`、`serde`、`icu_*` 等 ICU4X 组件）全部经 feature 门控可选。

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

完整可运行示例见 [examples/README.md](../../examples/README.md)。

---

## 🧭 核心概念

### Typestate 两阶段

- **`Kit<Unbuilt>`（构建阶段）**：注册模块、存入配置、声明生命周期钩子。此阶段调用 `optional()` 会直接**编译失败**；`require()` / `config()` 等检索方法在类型上即可调用（供 `AutoBuilder::build` 回调读取依赖与配置），对未构建的模块则返回错误。
- **`kit.build()`**：验证依赖图（缺失依赖检测、Kahn 算法环检测 + 拓扑排序），按拓扑序构建所有模块。
- **`Kit<Ready>`（运行阶段）**：只读检索能力与配置，不可再注册（注册与构建方法在此阶段被**编译期拒绝**）。

两阶段的完整方法面与流程图见 [架构文档](ARCHITECTURE.md)，逐方法 feature 门控见 [API 参考](API_REFERENCE.md)。

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

## ⚙️ 配置

配置是存储在 Kit 的 `TypeMap` 中的**类型化值**，以类型为键，无需 `ConfigKey` 样板。

### 手动存取

`set_config` 仅在 `Kit<Unbuilt>` 上可调用；模块在 `build()` 时通过 `kit.config::<C>()` 检索。`Kit<Ready>` 上写配置的路径包括 `reload_config::<C>()`（`reload` feature）、`merge_config::<C>(ovr)` 与 `populate_defaults::<C>()`（`confers` feature）：

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
struct TraitKitConfig {
    #[config(default = "localhost".to_string())]
    host: String,
}

impl Configurable for TraitKitConfig {
    fn load() -> Result<Self, Box<dyn std::error::Error + Send + 'static>> {
        Ok(TraitKitConfig::load_sync()?)
    }
}

let mut kit = Kit::new();
kit.load_config::<TraitKitConfig>()?;   // 从环境变量/默认值加载
```

配套 API：

| API | 用途 |
|---|---|
| `load_and_validate::<C>()` | 加载并执行 `Validatable::validate`，失败不存入 |
| `load_config_with::<C>(vars)` | 加载时做 `${VAR}` 变量替换（嵌套深度超上限即返回错误，不终止进程） |
| `load_config_or_default::<C>()` | 加载失败时落 `ModuleConfig::default_value()`（返回是否加载成功） |
| `snapshot_config::<C>()` / `restore_config::<C>()` | 配置快照与回滚 |
| `subscribe::<C>(cb)` / `reload_config::<C>()` `reload` | 热重载订阅与触发 |

### 模块配置元数据 `confers`

`ModuleConfig` trait 为配置类型声明 `PATH` 与 `default_value()`，把配置绑定到模块的配置路径（也是加密密钥派生的因子）：

```rust,ignore
use trait_kit::kit::config::ModuleConfig;

impl ModuleConfig for TraitKitConfig {
    const PATH: &'static str = "config/app.toml";
    fn default_value() -> Self {
        Self { host: "localhost".to_string() }
    }
}
```

### 加密配置 `encryption`

启用 `encryption` feature 后，静态配置可用 XChaCha20-Poly1305 加密存储。加密密钥通过 HKDF 从主密钥与 `ModuleConfig::PATH` 派生，同一主密钥为不同模块生成不同字段密钥：

```rust,ignore
let mut kit = Kit::new();
let secret = TraitKitConfig { host: "production-db".to_string() };
let master_key = *b"demo-master-key-0123456789abcdef"; // 32 字节主密钥（演示用，生产环境请从密钥管理服务获取）

kit.set_encrypted(&secret, &master_key)?;
let kit = kit.build()?;

let decrypted: TraitKitConfig = kit.get_encrypted(&master_key)?;  // 错误密钥会失败
```

### 配置继承 `confers`

四层配置继承体系支持跨模块/跨项目的配置复用（`trait-kit-macros` 提供 `ConfigInherit` / `SharedConfig` derive 宏）：

```rust,ignore
use trait_kit::kit::{Kit, ModuleConfig};
use trait_kit_macros::{ConfigInherit, SharedConfig};

#[derive(Clone, ConfigInherit, SharedConfig)]
#[shared(host, port)]
struct DbConfig {
    host: String,
    port: u16,
    max_connections: u32,
}

let mut kit = Kit::new();
kit.populate_defaults::<DbConfig>();       // 零配置默认值
kit.extract_shared::<TraitKitConfig>();         // 从 TraitKitConfig 提取共享字段
kit.inject_shared::<DbConfig>();           // 注入到 DbConfig
kit.merge_config::<DbConfig>(ovr);         // 编译期安全字段覆盖
```

四层机制：`merge_json_deep` 深合并 → `ConfigInherit` 字段覆盖（`Option<T>` 仅 `Some` 时覆盖）→ `SharedConfig` 共享字段 overlay → `populate_defaults` 零配置填充。原理详见 [架构文档](ARCHITECTURE.md)。

---

## 🔧 进阶用法

### 异步模块 `async`

`AsyncKit` 与 `Kit` API 对称，能力类型要求 `Send + Sync`，构建方法为异步（`AsyncAutoBuilder`），使用 `AsyncKit::new()` → `register::<M>()` → `build().await` → `require::<M>()` 的流程（`require` 为同步方法，无需 `.await`）。

完整可运行示例见 [`examples/src/kit/async_basic.rs`](../../examples/src/kit/async_basic.rs)：

```sh
cargo run -p trait-kit-examples --example async_basic --features async
```

### 特性门控能力一览

以下能力均通过 `Kit` / `AsyncKit` 上的方法使用（方法级 feature 门控，详见 [API 参考](API_REFERENCE.md)）：

| 能力 | Feature | 典型用途 |
|---|---|---|
| 生命周期钩子（`on_ready` / `on_shutdown`） | `lifecycle` | 构建后初始化、清理释放 |
| 健康检查（`HealthCheck` + `health_report`） | `health` | 运行时状态探针 |
| 作用域（`Scope` 每请求隔离） | `request-scope` | Web 请求级实例 |
| 特性开关（`enable_toggle` / `register_if_toggle`） | `toggle` | 运行时启停模块 |
| 构建观察者（`BuildObserver` 回调） | `observer` | 构建耗时统计、日志 |
| 装饰器（`decorate::<M>(f)`） | `decorator` | 能力包装（如加日志、指标） |
| 优雅关闭（`ShutdownCoordinator` 分阶段关闭 + 超时） | `shutdown` | 服务下线编排 |
| 接口分离（`register_as` / `resolve::<dyn Trait>()`） | `di` | 面向接口编程、测试替身 |
| 国际化格式化（`I18nFormatter`） | `i18n` | 区域感知数字/日期/复数/排序（`tr()` 翻译默认可用） |

### 条件注册与工厂

无需任何 feature 即可使用：

- `kit.register_if::<M>(pred)` — 运行时谓词控制是否注册模块；
- `kit.factory::<M>()` — 每次调用创建新实例（对比 `require()` 的单例语义）。

### 测试注入

用 `override_module::<M>(cap)` 以预构建值覆盖模块能力，跳过真实构建函数；注入期会校验依赖存在性，未注册依赖即返回 `DependencyMissing`：

```rust,ignore
kit.override_module::<LoggerModule>(Arc::new(test_logger));
let kit = kit.build()?;
```

---

## ✅ 最佳实践

1. **用宏声明模块**：`impl_module_meta!` / `impl_auto_builder!` 消除样板；依赖一律写进 `deps = [...]`，让环检测与缺失依赖检测在启动前生效。
2. **能力用 `Arc<T>` 包装**：能力需 `Clone`；共享可变服务用 `Arc<Service>`，避免深拷贝。
3. **配置显式声明**：需要配置的模块在 `build()` 里 `kit.config::<C>()`，缺失时 fail-fast（`MissingConfig`），不要在能力内部静默兜底。
4. **按需启用 feature**：高级 confers feature 自动继承低级（`encryption` → `reload` → `confers`），启用最高级别即可；不用的 feature 保持关闭以最小化编译与依赖。
5. **单线程用 `Kit`，多线程用 `AsyncKit`**：不要试图跨线程共享 `Kit`（`!Sync` 是有意设计）。
6. **测试用 `override_module`**：替换外部依赖（数据库、网络）为受控实现，保持测试确定性。
7. **错误处理统一走 `TraitKitError`**：其 `Display` 通过 `tr()` 自动本地化，业务侧只需 `match` 需要特殊处理的变体。

---

## 🩺 故障排查

常见错误均以 `TraitKitError` 变体出现，`Display` 自动本地化输出：

| 错误 | 触发场景 | 处理建议 |
|---|---|---|
| `CycleDetected { cycle }` | 依赖图中存在环 | 检查 `deps = [...]` 声明，打破循环（拆分模块或引入中间模块） |
| `DependencyMissing { module, missing }` | 声明的依赖未注册 | 在 `build()` 前补齐 `kit.register::<MissingModule>()` |
| `AlreadyRegistered { module }` | 模块重复注册 | 同一模块只需注册一次；多实例需求用 `register_multi` |
| `DecoratorTargetMissing { module }` | 装饰器目标模块未注册 | 改用 `try_decorate::<M>(f)` 在注册时校验，或先 `register::<M>()` |
| `VersionIncompatible { .. }` | 依赖模块版本不满足 `required_versions`（`version-negotiation` feature） | 对齐 `ModuleMeta::VERSION` 的 semver 兼容版本 |
| `BuildFailed { context, source }` | 模块构建函数返回错误 | 查看 `source` 中的原始错误，通常是外部资源不可用 |
| `MissingCapability { key }` | `require::<M>()` 时能力不存在 | 确认模块已注册且 `build()` 已完成；可选场景改用 `optional::<M>()` |
| `CapabilityTypeMismatch { key }` | 能力已构建但类型不符（如 override 注入了另一种能力类型） | 检查 `override_module` 注入值的类型与模块声明一致 |
| `MissingConfig { key }` | `config::<C>()` 时配置不存在 | 先 `set_config(value)` 或用 confers 的 `load_config::<C>()` |
| `LifecycleFailed { .. }` `lifecycle` | `on_ready` 钩子失败 | 检查钩子内的初始化逻辑与外部依赖 |
| `ShutdownTimedOut { phases }` `shutdown` | 优雅关闭超时 | 调整 `set_phase_timeout` / `set_global_timeout`，检查钩子是否阻塞 |

其他常见问题：

- **typestate 编译错误**：编译期排除的误用包括 `Kit<Ready>` 上调用注册/构建方法（`ready_cannot_register` / `ready_cannot_build` / `ready_cannot_set_config` / `ready_cannot_override_module` / `ready_cannot_with_metrics_port`）与 `Kit<Unbuilt>` 上调用 `optional()`（同步 `unbuilt_cannot_optional` / 异步 `async_unbuilt_cannot_optional`）；完整夹具清单见 `tests/ui/`（11 个，另含 decorator / lifecycle / observer / probe 门控组的 Ready 态违规），请检查调用阶段。`require()` 在 `Kit<Unbuilt>` 上可编译（供 `AutoBuilder::build` 回调使用），对未构建的模块返回 `MissingCapability` 错误而非编译失败。
- **feature 方法不存在**：方法级门控 API（如 `subscribe`、`set_encrypted`）需启用对应 feature，参见 [特性标志](#-特性标志)。
- **跨线程使用 `Kit` 报 `!Sync`**：改用 `AsyncKit`（`async` feature），或在单线程内使用 `Kit`。

仍有问题？请到 [GitHub Issues](https://github.com/Kirky-X/trait-kit/issues) 搜索或提问。

---

## 🧩 更多能力（注册模式、构建报告、组合与协商）

- **灵活注册模式**：`register_lazy`（首次 `require` 时构建并缓存）、`register_multi` + `require_all`（多绑定聚合）、`register_if`（运行时谓词）、`override_module`（测试注入）、`factory::<M>()`（每次调用创建新实例）。
- **接口/实现分离**（`di`）：`register_as` / `resolve::<dyn Trait>()` 类型擦除注册与检索。
- **作用域依赖**（`request-scope`）：`Scope` / `AsyncScope` 每请求实例隔离。
- **特性开关**（`toggle`）：`enable_toggle` / `is_toggle_enabled` / `register_if_toggle` 运行时字符串键控启停。
- **模块装饰器**（`decorator`）：`decorate::<M>(fn)` 构建后能力包装/增强。
- **结构化构建报告**（`report`）：`BuildReport` JSON 导出与 `contract_manifest()` 契约清单；依赖图 `graph_dot()` / `graph_mermaid()` 文本导出无门控，默认可用。
- **预设模块**（`presets` / `presets-remote`）：`ConfersConfigModule` 将 confers 配置中心纳入 Kit 模块体系，支持远程配置源。
- **子 Kit 组合**（`compose`）：子 `Kit` 以单一模块身份注册进父 `Kit`，能力命名空间隔离。
- **版本协商**（`version-negotiation`）：`ModuleMeta::VERSION` 与 `required_versions` 在 `build()` 时做 semver 兼容校验。
- **事件总线与观测端口**（默认可用，无 feature 门控）：`KitEvent` / `EventBus` 生命周期事件与 `MetricsPort` / `LogPort` 注入式观测，默认 `NoOp` 实现零开销。

## 💡 为什么选择 trait-kit？

对比常见的依赖装配方案，trait-kit 定位在"手动装配"和"完整 DI 框架"之间：

| 方案 | 优点 | 缺点 |
| --- | --- | --- |
| **手动装配** | 简单，无依赖。 | 模式不统一，每个项目各自为政。 |
| **trait-kit** | 标准化模式，类型安全，轻量级。 | 仍需显式声明依赖关系。 |
| **完整 DI（shaku 等）** | 自动解析，更少胶水代码。 | 依赖更重，魔法行为，调试困难。 |

trait-kit 提供 DI 框架的**标准化**，同时保持手动装配的**显式性**。

## 🔗 带依赖的模块

模块通过 `ModuleMeta::dependencies()` 声明依赖（`impl_module_meta!` 宏支持 `deps = [...]` 语法）。Kit 在 `build()` 时验证依赖图，并按拓扑顺序构造模块：

```rust
use std::sync::Arc;
use trait_kit::impl_module_meta;
use trait_kit::prelude::*;

struct Logger;
impl Logger {
    fn info(&self, msg: &str) { println!("[LOG] {msg}"); }
}

struct LoggerModule;
impl ModuleMeta for LoggerModule {
    const NAME: &'static str = "logger";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        &[]
    }
}

impl AutoBuilder for LoggerModule {
    type Capability = Arc<Logger>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(Logger))
    }
}

struct Storage {
    _logger: Arc<Logger>,
}

struct StorageModule;
// 宏一行声明名称 + 依赖（等价于手写 dependencies()）
impl_module_meta!(StorageModule, "storage", deps = [LoggerModule]);

impl AutoBuilder for StorageModule {
    type Capability = Arc<Storage>;
    type Error = TraitKitError;
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

## 🎨 特性标志

`default = []`：默认不启用任何可选 feature。高级 feature 自动继承低级 feature（`encryption` → `reload` → `confers`）。

<table>
<tr><th>Feature</th><th>启用</th><th>说明</th><th>默认</th></tr>
<tr><td><code>async</code></td><td>—</td><td><code>AsyncKit</code>：<code>Send + Sync</code> 异步能力管理，无额外依赖。</td><td>—</td></tr>
<tr><td><code>confers</code></td><td><code>dep:confers</code>, <code>dep:serde</code>, <code>dep:serde_json</code>, <code>confers/feature-toggle</code></td><td><code>Configurable</code> + <code>ModuleConfig</code> trait + <code>Config</code> derive 宏再导出。</td><td>—</td></tr>
<tr><td><code>reload</code></td><td><code>confers</code>, <code>confers/dynamic</code></td><td><code>subscribe</code> / <code>reload_config</code> 热重载订阅。</td><td>—</td></tr>
<tr><td><code>masking</code></td><td><code>regex</code>, <code>fancy-regex</code>, <code>aho-corasick</code>, <code>toml</code>, <code>serde_json</code>, <code>tracing</code></td><td><code>DataMasker</code> / <code>MaskRule</code> / <code>MaskRuleRegistry</code> PII/凭据掩码引擎与 secret-scan 出站门（自 inklog 下沉的单一来源）。</td><td>—</td></tr>
<tr><td><code>encryption</code></td><td><code>reload</code>, <code>confers</code>, <code>confers/encryption</code>, <code>dep:zeroize</code></td><td><code>set_encrypted</code> / <code>get_encrypted</code> 加密配置存储。</td><td>—</td></tr>
<tr><td><code>di</code></td><td>—</td><td>接口/实现分离：<code>register_as</code> / <code>resolve</code> 支持 <code>dyn Trait</code> 类型擦除。</td><td>—</td></tr>
<tr><td><code>lifecycle</code></td><td>—</td><td>生命周期钩子：<code>on_ready</code>（构建后）+ <code>on_shutdown</code>（清理）。</td><td>—</td></tr>
<tr><td><code>health</code></td><td>—</td><td>健康检查：<code>HealthCheck</code> trait + <code>HealthStatus</code> 状态报告；<code>AsyncHealthCheck</code>（health+async）的 <code>check</code> 刻意保持同步——网络探活应走 <code>on_ready</code> 探测一次并缓存结论，或用 <code>probe</code> 特性的 <code>ServiceProbe</code>；勿在 <code>check</code> 内做网络 I/O。</td><td>—</td></tr>
<tr><td><code>probe</code></td><td><code>health</code>, <code>dep:futures-timer</code></td><td>服务探针注册面：对象安全 <code>ServiceProbe</code> 异步探针——<code>register_probe</code>/<code>unregister_probe</code>/<code>probe_names</code>（任意状态可动态增减）、<code>AsyncKit&lt;Ready&gt;::run_probes()</code>（明细 + worst-of 聚合 + JSON；<code>latency_ms</code> 为框架实测墙钟）、<code>probe_aggregate()</code>（worst-of 结论）及 <code>run_probes_with_timeout()</code>/<code>probe_aggregate_with_timeout()</code>（单探针硬上限，悬挂探针记 unhealthy 继续跑完）；shutdown 自动注销并标记 stopped——停机后 <code>run_probes()</code> 返回显性不可服务（<code>stopped=true</code>），空注册表 ≠ 全部健康。与 <code>health</code> 的分工：<code>check</code> 只读缓存状态，<code>ServiceProbe</code> 才做真正的网络探活。</td><td>—</td></tr>
<tr><td><code>request-scope</code></td><td>—</td><td>作用域依赖：<code>Scope</code> / <code>AsyncScope</code> 每请求实例隔离。</td><td>—</td></tr>
<tr><td><code>toggle</code></td><td>—</td><td>特性开关：运行时字符串键控的模块启用/禁用。</td><td>—</td></tr>
<tr><td><code>observer</code></td><td>—</td><td>构建可观测：<code>BuildObserver</code> 回调（开始/完成/错误）。</td><td>—</td></tr>
<tr><td><code>decorator</code></td><td>—</td><td>模块装饰器：构建后能力包装/增强。</td><td>—</td></tr>
<tr><td><code>shutdown</code></td><td>—</td><td>优雅关闭协调器：分阶段有序关闭 + 超时强退。</td><td>—</td></tr>
<tr><td><code>i18n</code></td><td><code>dep:icu_collator</code>, <code>dep:icu_datetime</code>, <code>dep:icu_decimal</code>, <code>dep:icu_locale</code>, <code>dep:icu_plurals</code>, <code>dep:writeable</code>, <code>dep:sys-locale</code></td><td>ICU4X 国际化：本地化数字/日期/复数/排序格式化（按组件引入，不引 <code>icu</code> 门面 crate）。</td><td>—</td></tr>
<tr><td><code>report</code></td><td><code>dep:serde</code>, <code>dep:serde_json</code></td><td>结构化构建报告：<code>BuildReport</code> JSON 与 <code>contract_manifest()</code> 契约清单（依赖图 DOT/Mermaid 导出 <code>graph_dot()</code> / <code>graph_mermaid()</code> 无门控，默认可用）。</td><td>—</td></tr>
<tr><td><code>presets</code></td><td><code>confers</code></td><td>预设模块包：<code>ConfersConfigModule</code>（配置中心作为 Kit 模块）+ 组合 builder。</td><td>—</td></tr>
<tr><td><code>compose</code></td><td>—</td><td>子 Kit 组合：子 <code>Kit</code> 注册为父 <code>Kit</code> 的单一模块（能力命名空间隔离 + 跨 Kit 依赖校验）。</td><td>—</td></tr>
<tr><td><code>presets-remote</code></td><td><code>presets</code>, <code>confers/remote</code>, <code>async</code></td><td>远程配置桥接：<code>ConfersConfigModule</code> 走 confers 远程 <code>AsyncSource</code>（remote 模块以 <code>AsyncKit</code> 为构建载体，故 imply <code>async</code>）。</td><td>—</td></tr>
<tr><td><code>version-negotiation</code></td><td>—</td><td>能力版本协商：<code>ModuleMeta::VERSION</code> 与 <code>required_versions</code> 在 <code>build()</code> 时做 semver 兼容校验。</td><td>—</td></tr>
</table>

向后兼容别名：旧 feature 名 `interface` / `scope` / `negotiate` 仍可启用，分别转发到 `di` / `request-scope` / `version-negotiation`（已标记 deprecated，新代码请使用新名）。

在 `Cargo.toml` 中启用所需级别：

```toml
[dependencies]
trait-kit = { version = "0.5.0", features = ["encryption"] }
```
