# 📘 Trait-Kit API 参考

trait-kit 的 API 参考。覆盖核心与常用公开项——非穷尽全量，完整清单以 rustdoc / docs.rs 为准；按模块组织，标注各 API 所需的 feature flag（当前版本 **0.5.0-rc.6**）。

## 📋 目录

<details open>
<summary>📑 目录</summary>

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

    // 版本协商（version-negotiation feature 在 build() 时校验，声明本身无门控）
    const VERSION: &'static str = "0.0.0";
    fn required_versions() -> &'static [(&'static str, &'static str)] { &[] }

    // 模块自带 FTL 翻译片段（需 i18n feature）
    #[cfg(feature = "i18n")]
    fn i18n_ftl() -> &'static [(&'static str, &'static str)] { &[] }
}
```

| 成员 | 说明 |
|---|---|
| `NAME` | 模块诊断名称，用于错误消息和日志 |
| `dependencies()` | 返回依赖模块的 `(name, TypeId)` 对。默认返回空切片 |
| `VERSION` | 模块能力版本，默认 `"0.0.0"` 表示未声明；`version-negotiation` feature 下参与 `build()` 时的 semver 兼容校验 |
| `required_versions()` | 对依赖模块要求的最低版本 `(name, min_version)` 列表，默认为空 |
| `i18n_ftl()` `i18n` | 模块自带的 `(locale, ftl_source)` 翻译片段，`build()` 时合并为 kit 本地翻译 overlay |

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
| `register_as::<M>()` | `di` | 按接口类型注册（`dyn Trait` 类型擦除） |
| `register_if::<M>(pred)` | — | 条件注册（运行时谓词） |
| `register_lifecycle::<M>()` | `lifecycle` | 注册生命周期钩子 |
| `register_health_check::<M>()` | `health` | 注册健康检查 |
| `with_observer(obs)` | `observer` | 附加构建观察者 |
| `decorate::<M>(f)` | `decorator` | 注册能力装饰器（目标未注册时 `build()` 期 panic） |
| `try_decorate::<M>(f)` | `decorator` | 装饰器注册（目标未注册时返回 `DecoratorTargetMissing`） |
| `override_module::<M>(cap)` | — | 覆盖模块能力（测试注入） |
| `override_module_strict::<M>(cap)` | — | 覆盖并验证依赖存在性 |
| `set_config::<C>(value)` | — | 存储类型化配置 |
| `set_config_arc::<C>(value)` | — | 存储配置并以 `Arc` 快照共享 |
| `load_config::<C>()` | `confers` | 通过 `Configurable::load()` 加载配置 |
| `load_and_validate::<C>()` | `confers` | 加载配置并验证，失败不存入 |
| `load_config_or_default::<C>()` | `confers` | 加载失败时落 `default_value()`（返回是否加载成功） |
| `load_config_with::<C, S>(vars)` | `confers` | 加载配置并做 `${VAR}` 变量替换 |
| `snapshot_config::<C>()` | `confers` | 快照当前配置（返回是否成功） |
| `restore_config::<C>()` | `confers` | 回滚配置到最近快照 |
| `has_snapshot::<C>()` | `confers` | 检查指定类型快照是否存在 |
| `populate_defaults::<C>()` | `confers` | 空 Kit 时填充 `ModuleConfig::default_value()` |
| `merge_config::<C>(ovr)` | `confers` | 任意状态可用（`impl<S>`，构建期与运行期皆可调用）；应用 `ConfigInherit` 字段覆盖；每次调用记录进 `build_report().config_overrides`（`report` 特性，`applied=false` = 目标配置不存在被丢弃；记录无上限，高频调用配 `take_config_overrides()` 轮转） |
| `extract_shared::<C>()` | `confers` | 从配置提取共享字段到 overlay |
| `inject_shared::<C>()` | `confers` | 从 overlay 注入共享字段到配置 |
| `enable_toggle(key, bool)` | `toggle` | 设置 feature flag |
| `is_toggle_enabled(key)` | `toggle` | 查询 flag 状态 |
| `set_toggle` / `get_toggle` / `remove_toggle` / `list_toggles` | `toggle` | 类型化开关句柄（`ToggleValue`：Bool/Int/Float/Str） |
| `register_if_toggle::<M>(key)` | `toggle` | 按 toggle 条件注册模块 |
| `subscribe::<C>(cb)` | `reload` | 订阅配置热重载回调 |
| `reload_config::<C>()` | `reload` | 重新加载配置并通知订阅者 |
| `set_encrypted::<C>(val, key)` | `encryption` | 加密存储配置（XChaCha20-Poly1305） |
| `set_encrypted_with_version::<C>(val, key, version)` | `encryption` | 带密钥版本的加密存储（配合轮换） |
| `set_encrypted_with_key_provider::<C, P>(val, provider)` | `encryption` | 经 `KeyProvider` 调用时取钥（fail-closed） |
| `with_metrics_port(port)` | — | 注入 `MetricsPort` 观测端口 |
| `with_log_port(port)` | — | 注入 `LogPort` 观测端口 |
| `with_event_bus(bus)` | — | 注入 `EventBus` 事件总线 |
| `build()` | — | 验证依赖图 → 拓扑排序 → 构建 → `Kit<Ready>` |

#### `Kit<Ready>` — 运行阶段

> `require` / `get_arc` / `require_all` / `resolve` / `config` / `config_arc` / `subscribe` / `reload_config` / `merge_config` 定义在 `impl<S> Kit<S>` 上，`Kit<Unbuilt>` 态亦可编译调用（供 `AutoBuilder::build` 回调读取依赖与配置）；`require_ref` / `contains` / `factory` 定义在 `impl Kit<Ready>` 上，`Kit<Unbuilt>` 态调用即编译错误。下表列出时以主要使用场景为准。编译期排除的误用包括：`Kit<Unbuilt>` 上的 `optional()`（`unbuilt_cannot_optional`）与 `Kit<Ready>` 上的注册/构建方法（`ready_cannot_register` / `ready_cannot_build`，UI 测试断言）。

| 方法 | Feature | 说明 |
|---|---|---|
| `require::<M>()` | — | 检索能力（Clone，缺失则报错；`register_lazy` 模块首次调用触发构建并缓存） |
| `require_ref::<M>()` | — | 零拷贝检索（返回 `Ref<'_, Cap>`；lazy 模块构建后可借；守卫存活期间勿触发其他 lazy 模块首建，会 panic） |
| `get_arc::<M, T>()` | — | `Arc` 能力免克隆检索（返回 `Arc<T>`，同指针；兼容 lazy 缓存） |
| `optional::<M>()` | — | 可选检索（返回 `Option`；兼容 lazy 缓存） |
| `require_all::<M>()` | — | 检索所有多绑定能力 |
| `resolve::<I>()` | `di` | 按接口类型检索 `Arc<I>` |
| `contains::<M>()` | — | 检查能力是否已构建（兼容 lazy 缓存） |
| `contains_config::<C>()` | — | 检查配置是否存在 |
| `config::<C>()` | — | 检索配置（Clone） |
| `config_arc::<C>()` | — | 以 `Arc` 快照检索配置 |
| `factory::<M>()` | — | 创建工厂闭包，每次调用产生新实例 |
| `create_scope()` | `request-scope` | 创建空 `Scope`（与 Kit 能力互相独立） |
| `create_scope_from(self)` | `request-scope` | 从持有的 `Rc<Kit>`（`self: &Rc<Self>`）创建带父上下文的 `Scope`，不消费 Kit（父侧为 `Rc::downgrade` 弱引用，防保留环；`scope.parent::<T>()` 只读查询） |
| `health_check::<M>()` | `health` | 查询单模块健康状态 |
| `health_report()` | `health` | 查询所有模块健康报告 |
| `health_aggregate()` | `health` + `report` | worst-of 整体状态 + 各模块明细（modules 顺序不承诺确定；每次调用执行全部 checker，注入事件总线时发布 HealthChanged） |
| `health_json()` | `health` + `report` | 健康聚合 JSON 导出（供 /healthz 消费） |
| `record_health_history()` / `health_history()` | `health` | 环形缓冲健康采样与读取 |
| `shutdown()` | `lifecycle` | 按逆拓扑序执行 `on_shutdown` |
| `subscribe::<C>(cb)` | `reload` | 订阅热重载 |
| `reload_config::<C>()` | `reload` | 重新加载配置（Ready 态写配置的唯一路径；`set_config` 仅 `Kit<Unbuilt>` 可用，运行期更新配置另可用 `merge_config`，`confers` feature） |
| `enable_toggle(key, bool)` / `is_toggle_enabled(key)` | `toggle` | 运行时修改/查询 feature flag |
| `set_toggle` / `get_toggle` / `remove_toggle` / `list_toggles` | `toggle` | 类型化开关句柄 |
| `get_encrypted::<C>(key)` | `encryption` | 解密检索配置 |
| `contains_encrypted::<C>()` | `encryption` | 检查加密配置是否存在 |
| `get_encrypted_with_version::<C>(key, expected_version)` | `encryption` | 按密钥版本解密检索 |
| `rotate_master_key::<C>(old, new)` | `encryption` | 主密钥轮换并迁移旧密文（错钥 fail-closed，版本递增） |
| `encrypted_key_version::<C>()` | `encryption` | 查询密文当前密钥版本 |
| `module_tr(message_id)` | `i18n` | 经 kit 本地翻译 overlay 翻译（模块 `i18n_ftl` 优先，全局 `tr()` 兜底） |
| `graph_dot()` / `graph_mermaid()` | — | 依赖图文本导出 |
| `module_count()` | — | 已注册模块数 |
| `build_report()` | `report` | 结构化构建报告（JSON） |
| `contract_manifest()` | `report` | 契约清单导出 |
| `take_config_overrides()` | `report` | 任意状态可用（`impl<S>`，与 `merge_config` 同 impl 块，列于本表仅因排空轮转点多在运行期）；排空并返回 `config_overrides` 历史（记录序）——高频 `merge_config` 的长生命周期 Kit 以此防无界累积（每条约 40B，`build_report()` 快照整段 clone） |
| `emit_event(event)` | — | 发布自定义 `KitEvent` |

### 声明宏

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

#### `impl_async_auto_builder!` `async`

生成 `AsyncAutoBuilder` 实现。

```rust
impl_async_auto_builder!(MyModule, Arc<Cap>, MyError, |kit| Box::pin(async move {
    Ok(Arc::new(Cap { ... }))
}));
```

### derive 宏（`trait-kit-macros` crate）

三个 derive 由独立 crate `trait-kit-macros` 提供，不随 prelude 导出，需在 `Cargo.toml` 中显式依赖 `trait-kit-macros`：

| derive | 辅助属性 | 说明 |
|---|---|---|
| `#[derive(Module)]` | `#[module(...)]` | 为 struct 生成 `ModuleMeta` 实现（等价于 `impl_module_meta!`）：`name = "literal"` 覆盖诊断名（默认 struct 标识符），`deps(TypeA, TypeB)` 或 `deps = [TypeA, TypeB]` 声明依赖；不支持泛型 struct |
| `#[derive(ConfigInherit)]` | `#[config_inherit(nested)]` | 编译期安全的字段级覆盖类型 + `ConfigInherit` 实现 |
| `#[derive(SharedConfig)]` | `#[shared(field1, field2)]` | 自动生成 `extract_shared` / `inject_shared`（跨类型共享字段） |

```rust,ignore
use trait_kit_macros::Module;

#[derive(Module)]
#[module(name = "my-module", deps(DepA, DepB))]
struct MyModule;
```

### 依赖图导出

| API | 说明 |
| --- | --- |
| `DependencyGraph` | 依赖图容器（Kahn 拓扑排序 + DFS 环检测） |
| `GraphError` / `ModuleEntry` | 图校验错误与模块图节点 |

方法级导出：`graph_dot()` / `graph_mermaid()` / `module_count()` 无 feature 门控，`build_report()` 需 `report` feature，见 [Kit API](#kit-api) 的 `Kit<Ready>` 表，不再重复列出。

### 事件总线与观测端口

默认可用（无 feature 门控），默认 `NoOp` 实现零开销：

| API | 说明 |
| --- | --- |
| `KitEvent` | 生命周期事件枚举（`ModuleBuilt` / `HealthChanged` / `ConfigChanged` 等） |
| `EventBus` trait + `MemoryEventBus` / `NoOpEventBus` | 事件总线（`MemoryEventBus` 含订阅者 panic 隔离），`with_event_bus` 注入 |
| `MetricsPort` trait（counter/gauge/histogram） | 指标观测端口，`with_metrics_port` 注入 |
| `LogPort` trait | 结构化日志观测端口，`with_log_port` 注入 |
| `KeyProvider` trait + `KeyBytes` `encryption` | 密钥提供方抽象，`KeyBytes` 为零化容器 |

### 国际化

`tr()` 与 `I18nManager` 在默认特性下即可用（轻量 Fluent FTL 翻译）；`I18nFormatter` 的 ICU4X 区域感知格式化需启用 `i18n` feature。

#### `I18nManager`

```rust
let mgr = I18nManager::init();                      // 自动检测系统 locale
let mgr = I18nManager::init_with_locale("zh-CN");   // 指定 locale（无效 locale 回退英文目录）
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
let text: String = fmt.format_number(1234567.89)?;  // 分组符按 locale 输出
fmt.format_date(2026, 9, 10)?;
```

### Prelude

`use trait_kit::prelude::*` 导出最常用类型：

| 类型 | Feature |
|---|---|
| `ModuleMeta`, `AutoBuilder` | — |
| `Kit`, `Unbuilt`, `Ready` | — |
| `TraitKitError` | — |
| `I18nManager`, `I18nError`, `tr` | — |
| `I18nFormatter` | `i18n` |
| `AsyncAutoBuilder` | `async` |
| `AsyncKit`, `AsyncUnbuilt`, `AsyncReady` | `async` |
| `Configurable`, `ModuleConfig`, `Validatable` | `confers` |
| `ConfigInherit`, `SharedConfig` | `confers` |
| `Lifecycle` | `lifecycle` |
| `AsyncLifecycle` | `lifecycle` + `async` |
| `HealthCheck`, `HealthStatus` | `health` |
| `AsyncHealthCheck` | `health` + `async` |
| `BuildObserver` | `observer` |
| `Scope` | `request-scope` |
| `AsyncScope` | `request-scope` + `async` |
| `ShutdownCoordinator`, `ShutdownPhase`, `ShutdownPhaseResult`, `ShutdownResult` | `shutdown` |
| `AsyncShutdownCoordinator` | `shutdown` + `async` |

> derive 宏（`#[derive(Module)]` / `#[derive(ConfigInherit)]` / `#[derive(SharedConfig)]`）不随 prelude 导出，需在 `Cargo.toml` 中显式依赖 `trait-kit-macros`，见 [derive 宏](#derive-宏trait-kit-macros-crate)。

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

- `Kit::merge_config::<C>(ovr)` — 应用字段覆盖；覆盖事实（含目标配置不存在时的 `applied=false` 丢弃记录）按记录顺序进 `build_report().config_overrides`（`report` 特性，`AsyncKit` 同；`AsyncKit` 为共享 `Send + Sync` 类型，并发调用下记录序为加锁到达序，不代表实际应用顺序，且读-改-写非原子——同类型并发 `merge_config` 可能互相丢失覆盖，需独占写语义的调用方自行串行化）。记录历史无上限（每条约 40B），高频调用场景用 `take_config_overrides()`（`report` 特性）在轮转点排空
- `#[derive(ConfigInherit)]` — 自动生成 Override 类型（`trait-kit-macros`）
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
    DecoratorTargetMissing { module: &'static str },
    VersionIncompatible { module: &'static str, dependency: &'static str,
                          required: &'static str, provided: &'static str },
    BuildFailed { context: String, source: Box<dyn Error + Send + 'static> },
    MissingCapability { key: String },
    CapabilityTypeMismatch { key: String },
    MissingConfig { key: String },
    LifecycleFailed { context: String, source: Box<dyn Error + Send + 'static> }, // lifecycle
    ShutdownTimedOut { phases: Vec<ShutdownPhase> },                              // shutdown
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

`AsyncKit` 与同步 `Kit` API 对称（注册/构建/检索/配置/加密/开关全部可用），另提供：

| API | 说明 |
|---|---|
| `with_max_concurrency(limit)` | 按拓扑分层并发构建的并发上限 |
| `create_scope()` | 创建异步作用域 |
| `AsyncKit<Ready>` 配置面 | `load_config` 系列、`subscribe` / `reload_config`、`snapshot_config` / `restore_config`、`set_encrypted` / `get_encrypted` 与同步 Kit 能力一致 |
| `health_aggregate()` | `health` + `report` | worst-of 整体状态 + 各模块明细（`/healthz` 结构化载荷），与同步 `Kit<Ready>` 口径一致（空集 healthy-by-convention；modules 顺序不承诺确定；每次调用执行全部 checker，注入事件总线时发布 HealthChanged） |
| `health_json()` | `health` + `report` | 健康聚合 JSON 导出，与同步 `Kit<Ready>` 对位 |
| `build_report()` `report` | 结构化构建报告（JSON），与同步 `Kit<Ready>` 对位；async 构建状态集仅 `built`（无 override/lazy 面） |
| `contract_manifest()` `report` | 契约清单导出（NAME/VERSION/capability/deps），与同步 `Kit<Ready>` 对位 |
| `take_config_overrides()` `report` | 排空并返回 `config_overrides` 历史（构建前 `AsyncKit<Unbuilt>` 态 API，与 `set_config` 同口径——累积仅发生在构建前，Ready 后历史冻结、无运行期无界累积；排空动作语义（换出并返回记录序）与同步侧一致） |

### `di` — 接口/实现分离

旧 feature 名 `interface` 保留为兼容别名（转发到 `di`，已标记 deprecated）。

#### `Interface` `di`

接口标记 trait。所有 `'static` 类型（含 `?Sized`）自动实现。

#### `InterfaceBuilder` `di`

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

### `probe` — 服务探针注册面

`probe = ["health"]`。与 `HealthCheck`/`AsyncHealthCheck` 的分工：`check` 刻意同步、只读缓存状态，跑在报告路径；`ServiceProbe` 才做真正的异步网络探活，只在显式调用时执行（延迟由框架在 `probe().await` 外侧实测，见 `ProbeEntry::latency_ms`）。对象安全 `trait`，手写 `Pin<Box<dyn Future>>` 分派（对齐 `AsyncLifecycle`，不引 `async-trait`）。

#### `ServiceProbe` `probe`

```rust
pub struct ProbeOutcome {
    pub status: HealthStatus,
    pub latency: Duration,
}

pub trait ServiceProbe: Send + Sync {
    fn probe<'a>(&'a self)
        -> Pin<Box<dyn Future<Output = ProbeOutcome> + Send + 'a>>;
}
```

#### 注册面（`AsyncKit`，`probe` + `async`，任意状态可用）

```rust
impl<S> AsyncKit<S> {
    pub fn register_probe(&self, name: &'static str, probe: Arc<dyn ServiceProbe>);
    pub fn unregister_probe(&self, name: &str) -> bool;
    pub fn probe_names(&self) -> Vec<&'static str>;   // 注册序 = 执行序；同名重注册落到尾部
}
```

#### 执行面（`AsyncKit<Ready>`，`probe` + `async`）

```rust
impl AsyncKit<Ready> {
    // 顺序执行全部探针；worst-of（unhealthy > degraded > healthy）聚合；
    // 空注册表 healthy-by-convention（停机清空后除外，见 stopped）。
    // probe+report 下 ProbeReport 可 to_json()。
    pub async fn run_probes(&self) -> ProbeReport;
    // 只取 worst-of 结论；全部探针都会执行（无短路）。
    pub async fn probe_aggregate(&self) -> HealthStatus;
    // 单探针硬上限变体：悬挂探针记 unhealthy（detail 含 timed out），
    // 继续跑其余探针；最坏总延迟 ≤ 探针数 × timeout。探针 future 在
    // 超时处被 drop（须取消安全）。
    pub async fn run_probes_with_timeout(&self, timeout: Duration) -> ProbeReport;
    pub async fn probe_aggregate_with_timeout(&self, timeout: Duration) -> HealthStatus;
}
```

`ProbeReport { overall, healthy, probes: Vec<ProbeEntry { name, status, detail, latency_ms }>, stopped }` 在 `report` 特性下派生 `Serialize` 并提供 `to_json()`。`latency_ms` 为框架在 `probe().await` 外侧实测的墙钟延迟（不采信实现自报的 `ProbeOutcome::latency`——后者仅为实现方诊断字段）；`detail` 原样进入对外 JSON 载荷，**不得包含凭据/连接串/内网拓扑**。`stopped == true` 表示本报告不来自探针执行：注册表已被停机协议清空，结论为显性不可服务（`overall == "unhealthy"`、`probes == []`）——空注册表 ≠ 全部健康，就绪消费方必须区分。shutdown 协同：`shutdown_async()`（`probe`+`lifecycle`）与 `register_shutdown_into()`（`probe`+`lifecycle`+`shutdown`）完成后自动注销全部探针并标记 stopped——已停机的 Kit 无服务可探。

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

### `request-scope` — 作用域

旧 feature 名 `scope` 保留为兼容别名（转发到 `request-scope`，已标记 deprecated）。

#### `Scope` `request-scope`

轻量级每请求实例隔离容器（`!Send + !Sync`）。

| 方法 | 说明 |
|---|---|
| `Scope::new()` | 创建空作用域 |
| `register::<M>()` | 注册模块 |
| `require::<M>()` | 检索能力（首次构建并缓存） |
| `contains::<M>()` | 检查是否已注册 |

#### `AsyncScope` `request-scope` + `async`

线程安全异步作用域（`Send + Sync`）。

| 方法 | 说明 |
|---|---|
| `AsyncScope::new()` | 创建空异步作用域 |
| `insert::<M>(cap)` | 插入预构建能力（唯一入口；重复 insert 覆盖旧值） |
| `require::<M>()` | 检索能力 |
| `contains::<M>()` | 检查能力是否已插入 |

### `toggle` — 类型化开关句柄

`toggle` feature 除 `Kit` 上的方法级 API（`enable_toggle` / `is_toggle_enabled` / `register_if_toggle`）外，还提供独立开关后端：

| API | 说明 |
|---|---|
| `ToggleBackend` trait | 开关后端抽象 |
| `ToggleValue` | 类型化开关值（Bool / Int / Float / Str） |
| `MemoryToggle` | 内存后端（`confers` feature 启用时自动切换 `ConfersToggle`） |
| `ToggleKey` / `ToggleHandle<T>` | 强类型开关键与句柄 |
| `define_toggle_key!` 宏 | 编译期防 key 拼写错 |

### 扩展组合 API

| API | Feature | 说明 |
|---|---|---|
| `SubKitSpec` / `SubKitModule` / `SubKitHandle` | `compose` | 子 Kit 以单一模块身份注册进父 Kit（能力命名空间隔离 + 跨 Kit 依赖校验） |
| `ConfersConfigModule` | `presets` | confers 配置中心作为 Kit 模块纳入体系（`presets-remote` 走 confers 远程 `AsyncSource`） |

`report` feature 的 `BuildReport` / `contract_manifest`，以及无门控的 `graph_dot` / `graph_mermaid` 依赖图文本导出，见 [Kit API](#kit-api) 的 `Kit<Ready>` 表与 [依赖图导出](#依赖图导出) 一节。

`BuildReport` 的 override 语义分两层：

- **模块级** `overrides`：`override_module` / `override_module_strict` 家族的注册记录（sync-only，async 报告恒为空）
- **配置级** `config_overrides`：`merge_config`（sync 与 async）每次调用的 `{ config, source, applied }`，按记录顺序排列（顺序调用/单线程下即调用顺序；`AsyncKit` 并发调用下为加锁到达序，不代表实际应用顺序）；`applied=false` 表示目标配置不存在、override 被丢弃——显性呈现而非静默吞并；`applied=true` 表示本次调用执行了 apply，不承诺最终配置仍包含该 override（并发 `set_config`/`merge_config` 可能随后覆盖）。**使用警告（运行期累积仅 sync 侧成立）**：`Kit::merge_config`/`Kit::take_config_overrides` 任意状态可用（`impl<S>`），记录随调用无界累积（每条约 40B，`build_report()` 快照整段 clone 该历史）——高频调用（如每次 reload 回调）的长生命周期 Kit 应在轮转点调用 `take_config_overrides()` 排空并取回记录；`AsyncKit` 侧两者为构建前（Unbuilt）API（与 `set_config` 同口径），Ready 后历史冻结、无运行期累积

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
kit.load_config::<TraitKitConfig>()?;  // 通过 confers 从环境变量/默认值加载
let kit = kit.build()?;
let config: TraitKitConfig = kit.config()?;
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
