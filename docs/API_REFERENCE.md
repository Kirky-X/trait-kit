# 📘 Trait-Kit API 参考

trait-kit 的完整 API 参考。按模块组织，标注各 API 所需的 feature flag。

> 本文由原 `docs/API.md` 与 `docs/API_REFERENCE.md` 合并而成，是唯一的 API 参考文档。

## 📋 目录

<details open>
<summary>目录</summary>

- [📖 概述](#-概述)
- [🔩 核心 API](#-核心-api)
- [⚙️ 配置与扩展 API（confers）](#️-配置与扩展-apiconfers)
- [🚪 错误类型](#-错误类型)
- [🎛️ 特性门控 API](#️-特性门控-api)
- [💻 使用示例](#-使用示例)
- [✅ 最佳实践](#-最佳实践)

</details>

---

## 📖 概述

- **组织方式**：按「核心 API → 配置/扩展 API → 错误类型 → 特性门控 API」的层次组织，与 crate 的 feature 分层一致。
- **feature 标注**：标题后以内联代码标注的（如 `async`、`confers`）表示该 API 需在 `Cargo.toml` 中启用对应 feature；无标注的 API 在默认特性下即可用。
- **权威来源**：本文为人工整理的速查文档，最精确的签名与文档以 [docs.rs](https://docs.rs/trait-kit) 自动生成的文档为准。

---

## 🔩 核心 API

### `ModuleMeta`

模块身份与依赖声明。所有模块必须实现。

```rust
pub trait ModuleMeta: 'static {
    const NAME: &'static str;
    fn dependencies() -> &'static [(&'static str, TypeId)] { &[] }
}
```

| 成员 | 说明 |
|---|---|
| `NAME` | 模块诊断名称，用于错误消息和日志 |
| `dependencies()` | 返回依赖模块的 `(name, TypeId)` 对。默认返回空切片 |

### `AutoBuilder`

同步模块构建 trait。

```rust
pub trait AutoBuilder: ModuleMeta {
    type Capability: Clone + 'static;
    type Error: std::error::Error + Send + 'static;
    fn build(kit: &Kit) -> Result<Self::Capability, Self::Error>;
}
```

### Kit API

#### `Kit<Unbuilt>` — 构建阶段

| 方法 | Feature | 说明 |
|---|---|---|
| `Kit::new()` | — | 创建空 Kit |
| `register::<M>()` | — | 注册模块（即时构建） |
| `register_lazy::<M>()` | — | 注册模块（延迟构建，首次 require 时触发） |
| `register_multi::<M>()` | — | 多绑定注册（同类型聚合为 Vec） |
| `register_as::<M>()` | `interface` | 按接口类型注册（`dyn Trait` 类型擦除） |
| `register_if::<M>(pred)` | — | 条件注册（运行时谓词） |
| `register_lifecycle::<M>()` | `lifecycle` | 注册生命周期钩子 |
| `register_health_check::<M>()` | `health` | 注册健康检查 |
| `with_observer(obs)` | `observer` | 附加构建观察者 |
| `decorate::<M>(f)` | `decorator` | 注册能力装饰器 |
| `override_module::<M>(cap)` | — | 覆盖模块能力（测试注入） |
| `override_module_strict::<M>(cap)` | — | 覆盖并验证依赖存在性 |
| `set_config::<C>(value)` | — | 存储类型化配置 |
| `load_config::<C>()` | `confers` | 通过 `Configurable::load()` 加载配置 |
| `load_and_validate::<C>()` | `confers` | 加载配置并验证，失败不存入 |
| `snapshot_config::<C>()` | `confers` | 快照当前配置（返回是否成功） |
| `restore_config::<C>()` | `confers` | 回滚配置到最近快照 |
| `has_snapshot::<C>()` | `confers` | 检查指定类型快照是否存在 |
| `load_config_with::<C, S>(vars)` | `confers` | 加载配置并做 `${VAR}` 变量替换 |
| `enable_toggle(key, bool)` | `toggle` | 设置 feature flag |
| `is_toggle_enabled(key)` | `toggle` | 查询 flag 状态 |
| `register_if_toggle::<M>(key)` | `toggle` | 按 toggle 条件注册模块 |
| `subscribe::<C>(cb)` | `reload` | 订阅配置热重载回调 |
| `reload_config::<C>()` | `reload` | 重新加载配置并通知订阅者 |
| `set_encrypted(val, key)` | `encryption` | 加密存储配置 |
| `build()` | — | 验证依赖图 → 拓扑排序 → 构建 → `Kit<Ready>` |

> 配置继承体系的方法（`populate_defaults` / `merge_config` / `extract_shared` / `inject_shared`）见下文[配置继承体系](#配置继承体系-confers)。

#### `Kit<Ready>` — 运行阶段

| 方法 | Feature | 说明 |
|---|---|---|
| `require::<M>()` | — | 检索能力（Clone，缺失则报错） |
| `require_ref::<M>()` | — | 零拷贝检索（返回 `Ref<'_, Cap>`） |
| `optional::<M>()` | — | 可选检索（返回 `Option`） |
| `require_all::<M>()` | — | 检索所有多绑定能力 |
| `resolve::<I>()` | `interface` | 按接口类型检索 `Arc<I>` |
| `contains::<M>()` | — | 检查能力是否存在 |
| `contains_config::<C>()` | — | 检查配置是否存在 |
| `config::<C>()` | — | 检索配置（Clone） |
| `get_encrypted::<C>(key)` | `encryption` | 解密检索配置 |
| `health_check::<M>()` | `health` | 查询单模块健康状态 |
| `health_report()` | `health` | 查询所有模块健康报告 |
| `factory::<M>()` | — | 创建工厂闭包，每次调用产生新实例 |
| `shutdown()` | `lifecycle` | 按逆拓扑序执行 `on_shutdown` |
| `set_config::<C>(value)` | — | 运行时更新配置 |
| `subscribe::<C>(cb)` | `reload` | 订阅热重载 |
| `reload_config::<C>()` | `reload` | 重新加载配置 |
| `enable_toggle(key, bool)` | `toggle` | 运行时修改 feature flag |
| `is_toggle_enabled(key)` | `toggle` | 查询 flag 状态 |

### 宏

#### `impl_module_meta!`

生成 `ModuleMeta` 实现。

```rust
// 无依赖
impl_module_meta!(MyModule, "my-module");

// 有依赖
impl_module_meta!(MyModule, "my-module", deps = [DepA, DepB]);
```

#### `impl_auto_builder!`

生成 `AutoBuilder` 实现。

```rust
impl_auto_builder!(MyModule, Arc<Cap>, MyError, |kit| Ok(Arc::new(Cap { ... })));
```

### 国际化

`tr()` 与 `I18nManager` 在默认特性下即可用（轻量 Fluent FTL 翻译）；`I18nFormatter` 的 ICU4X 区域感知格式化需启用 `i18n` feature。

#### `I18nManager`

```rust
let mgr = I18nManager::init();           // 自动检测系统 locale
let mgr = I18nManager::init_with_locale("zh-CN")?;  // 指定 locale
```

#### `tr()` — 消息翻译

```rust
use trait_kit::i18n::tr;
let msg = tr("trait-kit-error-cycle-detected", &[("cycle", "A → B → A")]);
```

#### `I18nFormatter` — 本地化格式化 `i18n`

```rust
use trait_kit::i18n::I18nFormatter;
let fmt = I18nFormatter::new("zh-CN")?;
fmt.format_number(1234567.89);  // "1,234,567.89"
fmt.format_date(...);
```

### Prelude

`use trait_kit::prelude::*` 导出最常用类型：

| 类型 | Feature |
|---|---|
| `ModuleMeta`, `AutoBuilder` | — |
| `Kit`, `Unbuilt`, `Ready` | — |
| `TraitKitError` | — |
| `I18nManager`, `I18nFormatter`, `I18nError`, `tr` | — |
| `AsyncAutoBuilder` | `async` |
| `AsyncKit`, `AsyncUnbuilt`, `AsyncReady` | `async` |
| `Configurable` | `confers` |
| `ModuleConfig` | `confers` |
| `Lifecycle` | `lifecycle` |
| `AsyncLifecycle` | `lifecycle` + `async` |
| `HealthCheck`, `HealthStatus` | `health` |
| `AsyncHealthCheck` | `health` + `async` |
| `BuildObserver` | `observer` |
| `Scope` | `scope` |
| `AsyncScope` | `scope` + `async` |
| `ShutdownCoordinator`, `ShutdownPhase`, `ShutdownPhaseResult`, `ShutdownResult` | `shutdown` |
| `AsyncShutdownCoordinator` | `shutdown` + `async` |

---

## ⚙️ 配置与扩展 API（confers）

以下 API 均需启用 `confers` feature（`reload` / `encryption` 自动继承启用）。

### `Validatable` `confers`

配置验证 trait，用户实现后通过 `Kit::load_and_validate` 在加载后自动检查。

```rust
pub trait Validatable: Clone + 'static {
    fn validate(&self) -> Result<(), Vec<String>>;
}
```

### `interpolate_json_value` `confers`

递归替换 JSON 值中的 `${VAR}` 和 `${VAR:-default}` 模式。

```rust
pub fn interpolate_json_value<S: BuildHasher>(
    value: &mut serde_json::Value,
    vars: &HashMap<String, String, S>,
)
```

### 配置继承体系 `confers`

四层配置继承系统，支持跨模块/跨项目配置丝滑继承：

| 层级 | 机制 | API | 说明 |
|---|---|---|---|
| Layer 1 | 深合并 | `merge_json_deep` | 递归合并 JSON Object，非 Object 替换 |
| Layer 2 | 字段覆盖 | `ConfigInherit` trait | 编译期安全，`Option<T>` 字段仅 `Some` 时覆盖 |
| Layer 3 | 共享字段 | `SharedConfig` trait | `serde_json::Value` overlay 跨类型继承 |
| Layer 4 | 零配置 | `populate_defaults` | 空 Kit 自动填充 `ModuleConfig::default_value()` |

#### `ConfigInherit` trait

```rust
pub trait ConfigInherit: Clone + 'static {
    type Override: Clone + Default + 'static;
    fn apply_override(&mut self, ovr: &Self::Override);
}
```

- `Kit::merge_config::<C>(ovr)` — 应用字段覆盖
- `#[derive(ConfigInherit)]` — 自动生成 Override 类型（`trait-kit-derive`）
- `#[config_inherit(nested)]` — 嵌套字段递归委托

#### `SharedConfig` trait

```rust
pub trait SharedConfig: Clone + 'static {
    fn extract_shared(&self) -> serde_json::Map<String, serde_json::Value>;
    fn inject_shared(&mut self, shared: &serde_json::Map<String, serde_json::Value>);
}
```

- `Kit::extract_shared::<C>()` — 从配置提取共享字段到 overlay
- `Kit::inject_shared::<C>()` — 从 overlay 注入共享字段到配置
- `#[derive(SharedConfig)]` + `#[shared(field1, field2)]` — 自动生成实现

#### `populate_defaults`

```rust
impl Kit {
    pub fn populate_defaults<C: ModuleConfig>(&self) -> bool;
}
```

空 Kit 时填充 `C::default_value()`，已有值不覆盖。返回 `true` 表示填充了默认值。

---

## 🚪 错误类型

### `TraitKitError`

```rust
pub enum TraitKitError {
    CycleDetected { cycle: Vec<&'static str> },
    DependencyMissing { module: &'static str, missing: &'static str },
    AlreadyRegistered { module: &'static str },
    BuildFailed { context: String, source: Box<dyn Error + Send> },
    MissingCapability { key: String },
    MissingConfig { key: String },
    LifecycleFailed { context: String, source: Box<dyn Error + Send> }, // lifecycle
    ShutdownTimedOut { phases: Vec<ShutdownPhase> },                     // shutdown
}
```

`Display` 实现通过 `tr()` 自动本地化输出。

### `TraitKitResult<T>`

```rust
pub type TraitKitResult<T> = Result<T, TraitKitError>;
```

---

## 🎛️ 特性门控 API

### `async` — 异步模块构建

#### `AsyncAutoBuilder` `async`

```rust
pub trait AsyncAutoBuilder: ModuleMeta {
    type Capability: Clone + Send + Sync + 'static;
    type Error: std::error::Error + Send + 'static;
    fn build<'a>(kit: &'a AsyncKit)
        -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>;
}
```

#### `impl_async_auto_builder!` `async`

生成 `AsyncAutoBuilder` 实现。

```rust
impl_async_auto_builder!(MyModule, Arc<Cap>, MyError, |kit| Box::pin(async move {
    Ok(Arc::new(Cap { ... }))
}));
```

### `interface` — 接口/实现分离

#### `Interface` `interface`

接口标记 trait。所有 `'static` 类型（含 `?Sized`）自动实现。

#### `InterfaceBuilder` `interface`

接口/实现分离扩展 trait。

```rust
pub trait InterfaceBuilder: ModuleMeta {
    type Interface: ?Sized + 'static;
    type Capability: Clone + 'static;
    type Error: std::error::Error + Send + 'static;
    fn build(kit: &Kit) -> Result<Self::Capability, Self::Error>;
    fn into_interface(cap: Self::Capability) -> Arc<Self::Interface>;
}
```

### `lifecycle` — 生命周期

#### `Lifecycle` `lifecycle`

```rust
pub trait Lifecycle: AutoBuilder {
    fn on_ready(kit: &Kit<Ready>) -> Result<(), Self::Error> { Ok(()) }
    fn on_shutdown(cap: &Self::Capability) {}  // 默认空操作
}
```

两个方法均有默认实现（no-op），可按需覆盖。

#### `AsyncLifecycle` `lifecycle` + `async`

```rust
pub trait AsyncLifecycle: AsyncAutoBuilder {
    fn on_ready<'a>(kit: &'a AsyncKit<Ready>)
        -> Pin<Box<dyn Future<Output = Result<(), Self::Error>> + Send + 'a>>
        { Box::pin(async { Ok(()) }) }
    fn on_shutdown<'a>(cap: &'a Self::Capability)
        -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>
        { Box::pin(async {}) }
}
```

两个方法均有默认实现（no-op），可按需覆盖。

### `health` — 健康检查

#### `HealthStatus` `health`

```rust
pub enum HealthStatus {
    Healthy,
    Degraded { detail: String },
    Unhealthy { detail: String },
}
```

#### `HealthCheck` `health`

```rust
pub trait HealthCheck: AutoBuilder {
    fn check(cap: &Self::Capability) -> HealthStatus;
}
```

#### `AsyncHealthCheck` `health` + `async`

```rust
pub trait AsyncHealthCheck: AsyncAutoBuilder {
    fn check(cap: &Self::Capability) -> HealthStatus;
}
```

### `observer` — 构建可观测

#### `BuildObserver` `observer`

```rust
pub trait BuildObserver: Send + Sync + 'static {
    fn on_module_start(&self, module_name: &'static str) {}                        // 默认 no-op
    fn on_module_built(&self, module_name: &'static str, elapsed: Duration) {}      // 默认 no-op
    fn on_build_error(&self, module_name: &'static str, error: &TraitKitError) {}   // 默认 no-op
}
```

所有方法均有默认 no-op 实现，可按需覆盖。

### `scope` — 作用域

#### `Scope` `scope`

轻量级每请求实例隔离容器（`!Send + !Sync`）。

| 方法 | 说明 |
|---|---|
| `Scope::new()` | 创建空作用域 |
| `register::<M>()` | 注册模块 |
| `require::<M>()` | 检索能力（首次构建并缓存） |
| `contains::<M>()` | 检查是否已注册 |

#### `AsyncScope` `scope` + `async`

线程安全异步作用域（`Send + Sync`）。

| 方法 | 说明 |
|---|---|
| `AsyncScope::new()` | 创建空异步作用域 |
| `register::<M>()` | 注册模块 |
| `insert::<M>(cap)` | 插入预构建能力 |
| `require::<M>()` | 检索能力 |
| `contains::<M>()` | 检查是否已注册 |

### 其他方法级门控速查

`reload`（热重载）、`encryption`（加密配置）、`toggle`（特性开关）、`decorator`（装饰器）、`shutdown`（优雅关闭协调器）等 feature 以**方法级门控**挂在 `Kit` / `AsyncKit` 上，详见上文 [Kit API](#kit-api) 表格的 Feature 列。

---

## 💻 使用示例

### 同步模块（基础流程）

```rust
use std::sync::Arc;
use trait_kit::impl_module_meta;
use trait_kit::prelude::*;

struct StdoutLogger;
impl StdoutLogger {
    fn info(&self, msg: &str) { println!("[LOG] {msg}"); }
}

struct LoggerModule;
impl_module_meta!(LoggerModule, "logger");
impl AutoBuilder for LoggerModule {
    type Capability = Arc<StdoutLogger>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(StdoutLogger))
    }
}

fn main() {
    let mut kit = Kit::new();
    kit.register::<LoggerModule>().unwrap();
    let kit = kit.build().unwrap();

    let logger = kit.require::<LoggerModule>().unwrap();
    logger.info("Hello from trait-kit!");
}
```

### confers 配置加载 `confers`

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

let mut kit = Kit::new();
kit.load_config::<AppConfig>()?;  // 通过 confers 从环境变量/默认值加载
let kit = kit.build()?;
let config: AppConfig = kit.config()?;
```

### 更多示例

全部可运行示例见 [examples/](../examples/README.md)，覆盖每个 feature 门控的完整用法。

---

## ✅ 最佳实践

- **用宏声明模块**：`impl_module_meta!` / `impl_auto_builder!` 一行声明 `ModuleMeta` / `AutoBuilder`，减少样板代码。
- **显式声明依赖**：通过 `impl_module_meta!(M, "name", deps = [...])` 声明依赖，让 `build()` 的环检测与缺失依赖检测在应用启动前暴露问题。
- **配置走 `TypeMap`**：用 `set_config` / `config::<C>()` 存取类型化配置，避免为配置再定义模块。
- **按需启用 feature**：高级 feature 自动继承低级 feature（`encryption` → `reload` → `confers`），只需启用最高级别；不用的 feature 保持关闭以最小化依赖。
- **测试注入用 `override_module`**：测试中用 `override_module::<M>(cap)` 跳过真实构建函数，替换为受控能力。
- **复用错误本地化**：`TraitKitError` 的 `Display` 通过 `tr()` 自动本地化，业务代码无需自行翻译 Kit 错误消息。
