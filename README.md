<div align="center">

<img src="docs/assets/trait-kit.svg" alt="trait-kit logo" width="180">

[![CI Status](https://github.com/Kirky-X/trait-kit/actions/workflows/ci.yml/badge.svg)](https://github.com/Kirky-X/trait-kit/actions/workflows/ci.yml) [![Version](https://img.shields.io/crates/v/trait-kit.svg)](https://crates.io/crates/trait-kit) [![Docs.rs](https://docs.rs/trait-kit/badge.svg)](https://docs.rs/trait-kit) [![Downloads](https://img.shields.io/crates/d/trait-kit.svg)](https://crates.io/crates/trait-kit) [![License](https://img.shields.io/crates/l/trait-kit.svg)](LICENSE) [![Rust](https://img.shields.io/badge/rust-1.97.1%2B-orange.svg)](https://www.rust-lang.org/)

**中文** | [English](README_EN.md)

**模块标准接口 + `Kit` 能力管理中心**

[✨ 功能特性](#-功能特性) • [🚀 快速开始](#-快速开始) • [📚 文档](#-文档) • [💻 示例](#-示例) • [🤝 参与贡献](#-参与贡献)

</div>

---

<div align="center" style="padding: 32px; margin: 24px 0">

### 🧩 标准化模块装配

模块以 `ModuleMeta` + `AutoBuilder` 声明契约，Kit 集中完成装配校验与能力检索：

<table style="width:100%; border-collapse: collapse">
<tr><td align="center" width="25%" style="padding: 12px">🧩<br><b>标准模块接口</b><br><span style="color:#64748B">ModuleMeta + AutoBuilder 统一契约，宏一行声明模块</span></td><td align="center" width="25%" style="padding: 12px">🏗️<br><b>构建期验证</b><br><span style="color:#64748B">typestate 依赖图校验，装配错误前置到启动之前</span></td><td align="center" width="25%" style="padding: 12px">🔎<br><b>类型安全检索</b><br><span style="color:#64748B">能力按模块类型存取，无字符串键、无 downcast</span></td><td align="center" width="25%" style="padding: 12px">⚡<br><b>按需扩展</b><br><span style="color:#64748B">18 个可选 feature 全部门控，默认零开销</span></td></tr>
</table>

</div>

---

## 📋 目录

<details open>
<summary>📑 目录</summary>

- [✨ 功能特性](#-功能特性)
- [🚀 快速开始](#-快速开始)
- [🎨 特性标志](#-特性标志)
- [📚 文档](#-文档)
- [💻 示例](#-示例)
- [🏗️ 架构](#️-架构)
- [🔄 构建生命周期](#-构建生命周期)
- [⚙️ 配置：confers 集成](#️-配置confers-集成)
- [💡 为什么选择 trait-kit？](#-为什么选择-trait-kit)
- [🧪 测试](#-测试)
- [📊 性能](#-性能)
- [🔒 安全](#-安全)
- [🗺️ 开发路线图](#️-开发路线图)
- [🤝 参与贡献](#-参与贡献)
- [📋 更新日志](#-更新日志)
- [📄 许可证](#-许可证)
- [🙏 致谢](#-致谢)
- [📞 联系与支持](#-联系与支持)
- [⭐ Star 历史](#-star-历史)

</details>

---

## ✨ 功能特性

<div align="center">

<table>
<tr>
<td width="50%">🧩 <b>标准模块接口</b><br><sub><code>ModuleMeta</code> + <code>AutoBuilder</code> 定义统一契约，<code>impl_module_meta!</code> / <code>impl_auto_builder!</code> 宏一行声明模块。</sub></td>
<td width="50%">🏗️ <b>Typestate 构建验证</b><br><sub><code>Kit&lt;Unbuilt&gt;</code> 注册模块与配置，<code>build()</code> 做依赖图校验（缺失依赖 + 环检测）后返回 <code>Kit&lt;Ready&gt;</code>，错误在启动前暴露。</sub></td>
</tr>
<tr>
<td width="50%">🔎 <b>类型安全能力检索</b><br><sub>能力按模块类型存储与检索（<code>kit.require::&lt;M&gt;()</code>），无字符串键、无 downcast、无运行时查找表。</sub></td>
<td width="50%">🗂️ <b>配置中心</b><br><sub><code>set_config</code> / <code>config::&lt;C&gt;</code> 基于 <code>TypeId</code> 键的 <code>TypeMap</code> 存取类型化配置。</sub></td>
</tr>
<tr>
<td width="50%">⚙️ <b>confers 配置集成</b><br><sub>三级 feature 继承接入 <a href="https://crates.io/crates/confers">confers</a>：derive 宏配置加载、热重载订阅、XChaCha20-Poly1305 加密存储。</sub></td>
<td width="50%">🌐 <b>AsyncKit 异步支持</b><br><sub><code>async</code> feature 提供 <code>Send + Sync</code> 的 <code>AsyncKit</code>，适配连接池、HTTP 客户端等异步初始化场景。</sub></td>
</tr>
<tr>
<td width="50%">🩺 <b>运行时可观测</b><br><sub><code>lifecycle</code> 生命周期钩子、<code>health</code> 健康检查、<code>observer</code> 构建回调、<code>shutdown</code> 分阶段优雅关闭。</sub></td>
<td width="50%">🌍 <b>ICU4X 国际化</b><br><sub>区域感知的数字 / 日期 / 复数 / 排序格式化，内置 Fluent FTL 中英文消息翻译（<code>tr()</code>）。</sub></td>
</tr>
<tr>
<td width="50%">🧱 <b>极简默认依赖</b><br><sub><code>default = []</code> 零默认依赖；<code>confers</code>、<code>serde</code>、<code>serde_json</code>、<code>icu</code> 等全部为可选依赖并经 feature 门控。</sub></td>
<td width="50%">🚫 <b>无 unsafe</b><br><sub>整个 crate 标注 <code>#![deny(unsafe_code)]</code>，编译期强制排除 <code>unsafe</code>。</sub></td>
</tr>
</table>

</div>

<details>
<summary>更多能力（注册模式、构建报告、组合与协商）</summary>

- **灵活注册模式**：`register_lazy`（首次 `require` 时构建并缓存）、`register_multi` + `require_all`（多绑定聚合）、`register_if`（运行时谓词）、`override_module`（测试注入）、`factory::<M>()`（每次调用创建新实例）。
- **接口/实现分离**（`interface`）：`register_as` / `resolve::<dyn Trait>()` 类型擦除注册与检索。
- **作用域依赖**（`scope`）：`Scope` / `AsyncScope` 每请求实例隔离。
- **特性开关**（`toggle`）：`enable_toggle` / `is_toggle_enabled` / `register_if_toggle` 运行时字符串键控启停。
- **模块装饰器**（`decorator`）：`decorate::<M>(fn)` 构建后能力包装/增强。
- **结构化构建报告**（`report`）：`BuildReport` JSON 导出与依赖图 `graph_dot()` / `graph_mermaid()` 导出。
- **预设模块**（`presets` / `presets-remote`）：`ConfersConfigModule` 将 confers 配置中心纳入 Kit 模块体系，支持远程配置源。
- **子 Kit 组合**（`compose`）：子 `Kit` 以单一模块身份注册进父 `Kit`，能力命名空间隔离。
- **版本协商**（`negotiate`）：`ModuleMeta::VERSION` 与 `required_versions` 在 `build()` 时做 semver 兼容校验。
- **事件总线与观测端口**（默认可用，无 feature 门控）：`KitEvent` / `EventBus` 生命周期事件与 `MetricsPort` / `LogPort` 注入式观测，默认 `NoOp` 实现零开销。

</details>

---

## 🚀 快速开始

### 📦 安装

最低支持 Rust 版本（MSRV）：**1.97.1**（edition 2024）。

```sh
cargo add trait-kit
```

默认特性只含核心 `ModuleMeta` + `AutoBuilder` + `Kit`，无任何额外依赖。

### 💡 最小示例

定义一个 logger 模块，注册、构建 Kit，然后检索能力（出处：[examples/src/core/default_basic.rs](examples/src/core/default_basic.rs)，有精简）：

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

// 2. 定义模块：ModuleMeta 声明名称与依赖，AutoBuilder 声明如何构建能力
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

### 🧭 核心概念

- **模块**：实现 `ModuleMeta`（名称 + 依赖声明）与 `AutoBuilder`（构建能力）的类型，可用 `impl_module_meta!` 宏省去手写 impl。
- **能力**：模块构建产出的 `Clone` 值，存入 `TypeMap`，按模块类型检索。
- **Typestate**：`Kit<Unbuilt>` 只能注册，`Kit<Ready>` 只能检索，误用在编译期报错。
- **Feature 门控**：异步、配置、生命周期等能力按需启用，详见[特性标志](#-特性标志)。

### ⚙️ 带配置的模块

配置是存储在 Kit 的 `TypeMap` 中的类型化值，模块在构建时通过 `kit.config::<C>()` 检索：

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
    const NAME: &'static str = "db-pool";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        &[]
    }
}

impl AutoBuilder for DbPoolModule {
    type Capability = Arc<DbPool>;
    type Error = TraitKitError;
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

### 🔗 带依赖的模块

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

完整的 `Kit<Unbuilt>` / `Kit<Ready>` 方法列表（含各 feature 门控）见 [📘 API 参考](docs/API_REFERENCE.md) 与 [docs.rs](https://docs.rs/trait-kit)。

---

## 🎨 特性标志

`default = []`：默认不启用任何可选 feature。高级 feature 自动继承低级 feature（`encryption` → `reload` → `confers`）。

<div align="center">

<table>
<tr><th>Feature</th><th>启用</th><th>说明</th><th>默认</th></tr>
<tr><td><code>async</code></td><td>—</td><td><code>AsyncKit</code>：<code>Send + Sync</code> 异步能力管理，无额外依赖。</td><td>—</td></tr>
<tr><td><code>confers</code></td><td><code>dep:confers</code>, <code>dep:serde</code>, <code>dep:serde_json</code>, <code>confers/feature-toggle</code></td><td><code>Configurable</code> + <code>ModuleConfig</code> trait + <code>Config</code> derive 宏再导出。</td><td>—</td></tr>
<tr><td><code>reload</code></td><td><code>confers</code></td><td><code>subscribe</code> / <code>reload_config</code> 热重载订阅。</td><td>—</td></tr>
<tr><td><code>encryption</code></td><td><code>confers</code>, <code>confers/encryption</code></td><td><code>set_encrypted</code> / <code>get_encrypted</code> 加密配置存储。</td><td>—</td></tr>
<tr><td><code>interface</code></td><td>—</td><td>接口/实现分离：<code>register_as</code> / <code>resolve</code> 支持 <code>dyn Trait</code> 类型擦除。</td><td>—</td></tr>
<tr><td><code>lifecycle</code></td><td>—</td><td>生命周期钩子：<code>on_ready</code>（构建后）+ <code>on_shutdown</code>（清理）。</td><td>—</td></tr>
<tr><td><code>health</code></td><td>—</td><td>健康检查：<code>HealthCheck</code> trait + <code>HealthStatus</code> 状态报告。</td><td>—</td></tr>
<tr><td><code>scope</code></td><td>—</td><td>作用域依赖：<code>Scope</code> / <code>AsyncScope</code> 每请求实例隔离。</td><td>—</td></tr>
<tr><td><code>toggle</code></td><td>—</td><td>特性开关：运行时字符串键控的模块启用/禁用。</td><td>—</td></tr>
<tr><td><code>observer</code></td><td>—</td><td>构建可观测：<code>BuildObserver</code> 回调（开始/完成/错误）。</td><td>—</td></tr>
<tr><td><code>decorator</code></td><td>—</td><td>模块装饰器：构建后能力包装/增强。</td><td>—</td></tr>
<tr><td><code>shutdown</code></td><td>—</td><td>优雅关闭协调器：分阶段有序关闭 + 超时强退。</td><td>—</td></tr>
<tr><td><code>i18n</code></td><td><code>dep:icu</code>, <code>dep:writeable</code>, <code>dep:sys-locale</code></td><td>ICU4X 国际化：本地化数字/日期/复数/排序格式化。</td><td>—</td></tr>
<tr><td><code>report</code></td><td><code>dep:serde</code>, <code>dep:serde_json</code></td><td>结构化构建报告：<code>BuildReport</code> JSON、依赖图 DOT/Mermaid 导出。</td><td>—</td></tr>
<tr><td><code>presets</code></td><td><code>confers</code></td><td>预设模块包：<code>ConfersConfigModule</code>（配置中心作为 Kit 模块）+ 组合 builder。</td><td>—</td></tr>
<tr><td><code>compose</code></td><td>—</td><td>子 Kit 组合：子 <code>Kit</code> 注册为父 <code>Kit</code> 的单一模块（能力命名空间隔离 + 跨 Kit 依赖校验）。</td><td>—</td></tr>
<tr><td><code>presets-remote</code></td><td><code>presets</code>, <code>confers/remote</code></td><td>远程配置桥接：<code>ConfersConfigModule</code> 走 confers 远程 <code>AsyncSource</code>（仅 <code>AsyncKit</code>）。</td><td>—</td></tr>
<tr><td><code>negotiate</code></td><td>—</td><td>能力版本协商：<code>ModuleMeta::VERSION</code> 与 <code>required_versions</code> 在 <code>build()</code> 时做 semver 兼容校验。</td><td>—</td></tr>
</table>

</div>

在 `Cargo.toml` 中启用所需级别：

```toml
[dependencies]
trait-kit = { version = "0.5.0-rc.3", features = ["encryption"] }
```

---

## 📚 文档

| 文档 | 说明 |
|------|------|
| [📖 用户指南](docs/USER_GUIDE.md) | 从安装到进阶用法的完整教程 |
| [📘 API 参考](docs/API_REFERENCE.md) | 全部公开 API 速查，逐项标注 feature 门控 |
| [🏗️ 架构文档](docs/ARCHITECTURE.md) | 设计模式、数据流、线程安全模型与目录结构 |
| [🧪 验收测试场景](docs/TEST_SCENARIOS.md) | 验收场景穷举矩阵与 e2e 落地对账 |
| [⚡ 性能基准](docs/PERFORMANCE.md) | criterion 基准设施、基线数据与复现方法 |
| [🔒 安全文档](docs/SECURITY.md) | 支持策略、漏洞报告流程与安全设计 |
| [📋 更新日志](docs/CHANGELOG.md) | 每个版本的变更记录 |
| [🤝 贡献指南](docs/CONTRIBUTING.md) | 环境准备、TDD 工作流与提交规范 |
| [📦 crates.io](https://crates.io/crates/trait-kit) / [docs.rs](https://docs.rs/trait-kit) | 发布版本与在线 API 文档 |

---

## 💻 示例

`examples/` 是一个独立的 workspace 成员 `trait-kit-examples`（`publish = false`），共 20 个可独立运行的示例，覆盖全部公开 API 与 feature 门控。运行方式：

```sh
cargo run -p trait-kit-examples --example <名称> --features <特性>
```

| 示例 | Feature | 说明 |
|------|---------|------|
| `default_basic` | — | `ModuleMeta` + `AutoBuilder` + `Kit` 注册/构建/检索基础流程 |
| `conditional` | — | `register_if::<M>(predicate)` 运行时谓词条件注册 |
| `factory` | — | `Kit<Ready>::factory::<M>()` 每次调用创建新实例（对比单例 `require()`） |
| `interface` | `interface` | `InterfaceBuilder` + `register_as` / `resolve::<dyn Trait>()` 类型擦除 DI |
| `lifecycle` | `lifecycle` | `Lifecycle` trait（`on_ready` + `on_shutdown`）+ `Kit::shutdown()` |
| `health_check` | `health` | `HealthCheck` trait + `HealthStatus` + `health_report` |
| `observability` | `observer` | `BuildObserver` 回调（`on_module_start` / `on_module_built`） |
| `confers_loader` | `confers` | `#[derive(Config)]` + `Configurable` + `Kit::load_config`（环境变量加载） |
| `confers_macros` | `confers` | `ModuleConfig` trait（`PATH` + `default_value`）+ 构建时消费配置 |
| `validation` | `confers` | `Validatable` trait + `Kit::load_and_validate` 配置验证 |
| `snapshot_restore` | `confers` | `snapshot_config` / `restore_config` / `has_snapshot` 配置快照与回滚 |
| `config_inheritance` | `confers` | 四层配置继承体系（`merge_json_deep` → `ConfigInherit` → `SharedConfig` → `populate_defaults`） |
| `hot_reload` | `reload` | `subscribe::<C>` + `reload_config::<C>` 热重载订阅 |
| `encryption` | `encryption` | `set_encrypted` / `get_encrypted` 加密存取 + 错误密钥拒绝 |
| `async_basic` | `async` | `AsyncAutoBuilder` + `AsyncKit` 异步注册/构建/检索 |
| `scope_basic` | `scope` | `Scope` 每请求实例隔离 + 懒构建缓存 |
| `toggle_basic` | `toggle` | `enable_toggle` / `is_toggle_enabled` / `register_if_toggle` 运行时开关 |
| `decorator` | `decorator` | `Kit::decorate::<M>(fn)` 构建后能力包装 |
| `shutdown` | `shutdown` | `ShutdownCoordinator` 分阶段优雅关闭（`StopRequests` → `DrainQueue` → `CloseConnections`）+ 超时控制 |
| `i18n` | `i18n` | `I18nFormatter` 本地化数字/日期/复数/排序 |

示例详情见 [examples/README.md](examples/README.md)。

---

## 🏗️ 架构

trait-kit workspace 由四个成员组成：主 crate `trait-kit`、过程宏 crate `trait-kit-derive` 与 `trait-kit-macros`、示例 crate `trait-kit-examples`；主 crate 的 `src/` 分 `core` 接口层、`kit` 能力管理中心、`i18n` 国际化三层。

**核心设计**：

- **Typestate 模式**：`Kit<Unbuilt>` → `Kit<Ready>`，构建时验证依赖图，运行时零开销。
- **内部可变性**：同步 `Kit` 基于 `RefCell`，单线程 `!Sync` 设计，避免锁开销；`AsyncKit` 使用 `Arc<RwLock>` 支持多线程。
- **三级 Feature 继承**（confers 集成）：各 feature 的启用关系见[特性标志](#-特性标志)。

workspace 结构图、依赖图验证、数据流、线程安全模型与目录结构等完整设计见 [架构文档](docs/ARCHITECTURE.md)。

---

## 🔄 构建生命周期

`Kit` 的全部能力检索都发生在 `build()` 之后：注册期方法位于 `Kit<Unbuilt>`，检索期方法位于 `Kit<Ready>`，"未构建就检索"由编译期排除（trybuild UI 测试断言，见 `tests/ui/`）。

| 阶段 | 类型状态 | 可用操作 |
|------|----------|----------|
| 注册期 | `Kit<Unbuilt>` | `register` / `register_lazy` / `register_multi` / `register_if` / `register_as` / `override_module` / `set_config` / `build` |
| 运行期 | `Kit<Ready>` | `require` / `require_ref` / `require_all` / `optional` / `factory` / `resolve` / `contains` / `config` / `health_check` / `shutdown` |

`build()` 内部的真实执行路径（缺失依赖检测 → 环检测与拓扑排序 → 按拓扑序逐模块构建 → 返回 `Kit<Ready>`，依据 `src/kit/kit.rs`）的完整时序图见 [架构文档 · 数据流](docs/ARCHITECTURE.md#-数据流)。

---

## ⚙️ 配置：confers 集成

trait-kit 通过三级 feature flag 集成 [`confers`](https://crates.io/crates/confers) 0.6，每一级继承前一级，形成分层能力系统。

| 层级 | Feature | 能力 |
|------|---------|------|
| 1. 配置加载 | `confers` | `Configurable` 桥接 confers `#[derive(Config)]`，`load_config::<C>()` 从环境变量/默认值加载 |
| 2. 模块配置元数据 | `confers` | `ModuleConfig` trait 声明 `PATH` 与 `default_value()`，将配置类型绑定到模块配置路径 |
| 3. 热重载 | `reload` | `subscribe::<C>()` 订阅 + `reload_config::<C>()` 重载并通知订阅者 |
| 4. 加密存储 | `encryption` | `set_encrypted` / `get_encrypted`：XChaCha20-Poly1305 认证加密，密钥经 HKDF 从主密钥与 `ModuleConfig::PATH` 派生 |
| 5. 配置继承 | `confers` + `trait-kit-derive` | 四层继承体系：`merge_json_deep` 深合并 → `ConfigInherit` 编译期安全字段覆盖 → `SharedConfig` 跨类型共享字段 → `populate_defaults` 零配置默认值 |

配置加载与配置继承的典型用法（完整可运行版本见 `confers_loader` 与 `config_inheritance` 示例）：

```rust,ignore
use trait_kit::prelude::*;
use trait_kit_derive::{ConfigInherit, SharedConfig};

// 第一/二级：derive 配置加载 + 模块配置元数据
#[derive(Debug, Clone, serde::Deserialize, confers::Config)]
#[config(env_prefix = "APP_")]
struct AppConfig {
    #[config(default = "localhost".to_string())]
    host: String,
}

// 第五级：跨类型共享字段继承
#[derive(Clone, ConfigInherit, SharedConfig)]
#[shared(host)]
struct DbConfig {
    host: String,
    port: u16,
}

let mut kit = Kit::new();
kit.load_config::<AppConfig>()?;        // 经 confers 从环境变量/默认值加载
kit.populate_defaults::<DbConfig>()?;   // 零配置默认值
kit.extract_shared::<AppConfig>()?;     // 提取共享字段
kit.inject_shared::<DbConfig>()?;       // 注入到 DbConfig
```

- `trait-kit-derive` 提供 `#[derive(ConfigInherit)]` 与 `#[derive(SharedConfig)]` 宏，共享字段以 `serde_json::Value` 保留类型信息。
- `AsyncKit` 提供完全对称的 `Send + Sync` 配置 API。

---

## 💡 为什么选择 trait-kit？

trait-kit 定位在"手动装配"和"完整 DI 框架"之间：

| 方案 | 优点 | 缺点 |
| --- | --- | --- |
| **手动装配** | 简单，无依赖。 | 模式不统一，每个项目各自为政。 |
| **trait-kit** | 标准化模式，类型安全，轻量级。 | 仍需显式声明依赖关系。 |
| **完整 DI（shaku 等）** | 自动解析，更少胶水代码。 | 依赖更重，魔法行为，调试困难。 |

trait-kit 提供 DI 框架的**标准化**，同时保持手动装配的**显式性**。

---

## 🧪 测试

### 测试策略

| 类型 | 位置 | 说明 |
|------|------|------|
| 单元测试 | `src/`（`#[cfg(test)]`） | 模块内部逻辑 |
| 集成测试 | `tests/`（6 个目标）+ `tests/e2e/`（13 个注册目标） | `basic`、`config_inheritance_e2e`、`e2e_core`、`e2e_async`、`e2e_concurrency`、`e2e_feature_combinations` 等 |
| 编译期 UI 测试 | `tests/compile_fail.rs` + `tests/ui/`（3 个用例） | 基于 trybuild，断言 typestate 误用（未构建即检索等）无法编译 |
| 宏 crate 测试 | `trait-kit-macros/tests/` | `#[derive(Module)]` 展开正确性与编译失败用例 |
| 文档测试 | README 与 doc 注释中的 `rust` 代码块 | 随 `cargo test` 编译运行 |
| 示例验证 | `examples/`（20 个） | 每个示例独立运行，断言失败即 panic |
| 基准测试 | `benches/kit_bench.rs` | criterion 基准（需 `toggle` feature） |

测试规模：主 crate `src/` 与 `tests/` 共 **754** 个 `#[test]` 函数（截至 **0.5.0-rc.3**，`grep` 统计），另有 `trait-kit-macros/tests/` 6 个。

### 常用命令（与 CI 一致）

```sh
# 全量测试（workspace + 全部 feature，与 CI test job 一致）
cargo test --workspace --all-features

# 按特性组合运行
cargo test --features confers

# Lint 与格式检查（CI 门禁，零告警）
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check

# 文档零告警
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features

# 依赖审计（advisories / licenses / bans / sources）
cargo deny check

# 覆盖率门禁（CI coverage job：行覆盖 ≥ 80%）
cargo llvm-cov --workspace --all-features --fail-under-lines 80
```

---

## 📊 性能

性能来自设计层面：typestate 把依赖图验证全部前置到 `build()`，`Kit<Ready>` 上的能力检索是 `TypeId` 查表 + 克隆；同步 `Kit` 基于 `RefCell` 无锁设计。criterion 基准设施覆盖 build / require / config / toggle 四个热路径轴（`benches/kit_bench.rs`，基准项需 `--features toggle`）；基线（2026-09-10）实测 `require` 与裸 `Arc::clone` + `TypeId` 查找同量级，验证了"零开销能力检索"的设计目标，大结构能力建议改用 `require_ref`（借用读）。

基线数字、测量环境与复现命令见 [⚡ 性能基准](docs/PERFORMANCE.md)。

---

## 🔒 安全

- **漏洞报告**：请勿通过公开 Issue 报告，使用 GitHub 私密通道 [Security Advisories](https://github.com/Kirky-X/trait-kit/security/advisories/new)（"Report a vulnerability"）。维护者承诺 48 小时内确认、7 天内给出初步评估（见 [SECURITY.md](docs/SECURITY.md)）。
- **无 unsafe**：`#![deny(unsafe_code)]` 全 crate 强制。
- **编译期排除误用**：typestate 使"未构建就检索"成为编译错误；依赖图在 `build()` 时做缺失依赖与环检测。
- **明确的线程安全边界**：同步 `Kit` 为 `!Sync`（编译器强制，见 `static_assertions` 断言），多线程用 `AsyncKit`（`Send + Sync`）。
- **加密配置存储**（`encryption`）：XChaCha20-Poly1305 AEAD，HKDF 从主密钥与 `ModuleConfig::PATH` 派生字段密钥；`EncryptedBlob` 的 `Debug` 实现不泄露加密材料。
- **供应链门禁**：`cargo deny check`（`deny.toml`：advisories / licenses / bans / sources）+ `cargo audit` 为 CI 必过项；CodeQL 静态分析常开；lefthook 私钥扫描拦截凭据入库。

完整的安全设计、最佳实践与安全修复记录见 [docs/SECURITY.md](docs/SECURITY.md)。

---

## 🗺️ 开发路线图

条目沿用既有规划，状态对齐当前仓库事实：

<div align="center">

<table>
<tr><th>状态</th><th>条目</th><th>说明</th></tr>
<tr><td>✅</td><td><b>0.5.0-rc.2</b>（2026-09-03）</td><td>文档与 Kit API 表同步；workspace 依赖路径本地化（<code>path</code> + <code>version</code> 双写）。</td></tr>
<tr><td>✅</td><td><b>性能基准</b></td><td>criterion 基准 <code>benches/kit_bench.rs</code> 与 <code>docs/PERFORMANCE.md</code> 基线报告已建立（基线 2026-09-10）。</td></tr>
<tr><td>📋</td><td><b>0.5.0 正式发布</b></td><td>次版本位 +1 后，按工作区发布计划同步下游仓库（oxcache、dbnexus、inklog、limiteron、sdforge）的 <code>path + version</code> 依赖要求。</td></tr>
<tr><td>📋</td><td><b>cfg 门控完整性</b></td><td>补齐 <code>--no-default-features --features async</code> 组合下 <code>observer</code> 相关 cfg 门控（已知低优先级项）。</td></tr>
</table>

</div>

---

## 🤝 参与贡献

欢迎参与贡献！完整的开发环境准备、TDD 工作流与提交规范请参见 [贡献指南](docs/CONTRIBUTING.md)。

### 构建要求

- Rust **1.97.1** 或更高版本（stable，见 `rust-toolchain.toml`，edition 2024）。
- 无需外部工具链（无 protoc、无 openssl、无系统库；仅 workspace 成员 `examples` 的 confers 联调路径可能需要 `protobuf-compiler`）。

### 提交规范与本地门禁

- 提交信息遵循 **conventional commits**（`feat(scope): ...`、`fix(scope): ...`、`docs: ...` 等），由 lefthook `commit-msg` 钩子校验。
- **lefthook** 钩子门禁（`lefthook.yml`）：
  - `pre-commit`：`cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo deny check`、私钥内容扫描；
  - `pre-push`：`cargo audit`、覆盖率 ≥ 80% 行覆盖。
- 禁止使用 `--no-verify` 跳过钩子。

### 开发命令

与 CI 一致的常用命令（全量测试、lint、文档零告警、依赖审计、覆盖率门禁）见[🧪 测试](#-测试)章节与 [贡献指南](docs/CONTRIBUTING.md)。

### PR 流程

1. 从 `main` 创建 feature 分支（`feat/<功能>` / `fix/<问题>`），禁止直接提交到 main。
2. 确保所有测试通过且 Clippy 零告警，新功能附带测试。
3. 保持 README 与 API 变更同步（`docs/API_REFERENCE.md`、`docs/USER_GUIDE.md`）。
4. CI（fmt / clippy / check / test / 三平台构建 / doc / 安全 / 覆盖率）全部通过后等待 review。

本项目遵循 [Rust 行为准则](https://www.rust-lang.org/policies/code-of-conduct)。

---

## 📋 更新日志

详见 [CHANGELOG.md](docs/CHANGELOG.md)。近期版本要点：

- **0.5.0-rc.2**（2026-09-03）：文档版本号与 Kit API 表格同步；`confers` 依赖路径本地化（`path` + `version` 双写）。
- **0.4.2**（2026-08-06）：修复 `AsyncKit::decorate()` 装饰器存储键错误（decorator 此前从未生效）。
- **0.4.1**（2026-08-06）：国际化增强（`tr()` / `I18nManager` 不再依赖 `i18n` feature）；`EncryptedBlob` Debug 不再泄露加密材料；新增配置扩展 API 文档与示例。

---

## 📄 许可证

本项目基于 **MIT + Commons Clause** 许可证发布：MIT 主许可之上附加 Commons Clause v1.0 条件，未经许可方单独书面授权不得销售本软件；属于 source-available 许可证，商业使用需单独授权。详见 [LICENSE](LICENSE)。

Copyright (c) 2026 Kirky.X

---

## 🙏 致谢

- [`confers`](https://crates.io/crates/confers) — 配置加载、热重载与加密存储的底层能力。
- [ICU4X](https://github.com/unicode-org/icu4x) — 国际化格式化（数字/日期/复数/排序）。
- [Project Fluent](https://projectfluent.org/) — Fluent FTL 消息本地化方案。
- [syn](https://github.com/dtolnay/syn) / [quote](https://github.com/dtolnay/quote) / [proc-macro2](https://github.com/dtolnay/proc-macro2) — 过程宏基础设施（`trait-kit-derive` 与 `trait-kit-macros`）。
- [Rust 社区](https://www.rust-lang.org/community) — 优秀的语言生态与工具链。

---

## 📞 联系与支持

- **Bug 与功能建议**：[GitHub Issues](https://github.com/Kirky-X/trait-kit/issues)
- **安全漏洞**：请勿通过公开 Issue 报告，参见 [安全文档](docs/SECURITY.md) 的漏洞报告流程。
- **维护者**：Kirky.X

---

## ⭐ Star 历史

[![Star History Chart](https://api.star-history.com/svg?repos=Kirky-X/trait-kit&type=Date)](https://star-history.com/#Kirky-X/trait-kit&Date)

### 💝 支持本项目

如果您觉得这个项目有用，请考虑给它一个 ⭐️！
