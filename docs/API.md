# API 参考

trait-kit 的完整 API 参考。按模块组织，标注各 API 所需的 feature flag。

---

## 核心 Traits

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

### `AsyncAutoBuilder` `async`

异步模块构建 trait。

```rust
pub trait AsyncAutoBuilder: ModuleMeta {
    type Capability: Clone + Send + Sync + 'static;
    type Error: std::error::Error + Send + 'static;
    fn build<'a>(kit: &'a AsyncKit)
        -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>;
}
```

### `Interface` `interface`

接口标记 trait。所有 `'static` 类型（含 `?Sized`）自动实现。

### `InterfaceBuilder` `interface`

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

---

## Kit API

### `Kit<Unbuilt>` — 构建阶段

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

### `Kit<Ready>` — 运行阶段

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

---

## 配置扩展

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

---

## AsyncKit API

`AsyncKit` 是 `Kit` 的异步对应，API 基本对称。

### `AsyncKit<Unbuilt>`

| 方法 | 说明 |
|---|---|
| `AsyncKit::new()` | 创建空异步 Kit |
| `register::<M>()` | 注册异步模块 |
| `register_lifecycle::<M>()` | 注册异步生命周期钩子 |
| `register_health_check::<M>()` | 注册异步健康检查 |
| `with_observer(obs)` | 附加构建观察者 |
| `set_config::<C>(value)` | 存储配置 |
| `build().await` | 异步构建 → `AsyncKit<Ready>` |

### `AsyncKit<Ready>`

| 方法 | 说明 |
|---|---|
| `require::<M>().await` | 异步检索能力 |
| `optional::<M>().await` | 异步可选检索 |
| `contains::<M>()` | 检查能力存在 |
| `health_check::<M>()` | 查询健康状态 |
| `shutdown()` | 异步关闭 |

---

## 宏

### `impl_module_meta!`

生成 `ModuleMeta` 实现。

```rust
// 无依赖
impl_module_meta!(MyModule, "my-module");

// 有依赖
impl_module_meta!(MyModule, "my-module", deps = [DepA, DepB]);
```

### `impl_auto_builder!`

生成 `AutoBuilder` 实现。

```rust
impl_auto_builder!(MyModule, Arc<Cap>, MyError, |kit| Ok(Arc::new(Cap { ... })));
```

### `impl_async_auto_builder!` `async`

生成 `AsyncAutoBuilder` 实现。

```rust
impl_async_auto_builder!(MyModule, Arc<Cap>, MyError, |kit| Box::pin(async move {
    Ok(Arc::new(Cap { ... }))
}));
```

---

## 生命周期 `lifecycle`

### `Lifecycle`

```rust
pub trait Lifecycle: AutoBuilder {
    fn on_ready(kit: &Kit<Ready>) -> Result<(), Self::Error> { Ok(()) }
    fn on_shutdown(cap: &Self::Capability) {}  // 默认空操作
}
```

两个方法均有默认实现（no-op），可按需覆盖。

### `AsyncLifecycle` `async`

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

---

## 健康检查 `health`

### `HealthStatus`

```rust
pub enum HealthStatus {
    Healthy,
    Degraded { detail: String },
    Unhealthy { detail: String },
}
```

### `HealthCheck`

```rust
pub trait HealthCheck: AutoBuilder {
    fn check(cap: &Self::Capability) -> HealthStatus;
}
```

### `AsyncHealthCheck` `async`

```rust
pub trait AsyncHealthCheck: AsyncAutoBuilder {
    fn check(cap: &Self::Capability) -> HealthStatus;
}
```

---

## 构建可观测 `observer`

### `BuildObserver`

```rust
pub trait BuildObserver: Send + Sync + 'static {
    fn on_module_start(&self, module_name: &'static str) {}                        // 默认 no-op
    fn on_module_built(&self, module_name: &'static str, elapsed: Duration) {}      // 默认 no-op
    fn on_build_error(&self, module_name: &'static str, error: &TraitKitError) {}   // 默认 no-op
}
```

所有方法均有默认 no-op 实现，可按需覆盖。

---

## 作用域 `scope`

### `Scope`

轻量级每请求实例隔离容器（`!Send + !Sync`）。

| 方法 | 说明 |
|---|---|
| `Scope::new()` | 创建空作用域 |
| `register::<M>()` | 注册模块 |
| `require::<M>()` | 检索能力（首次构建并缓存） |
| `contains::<M>()` | 检查是否已注册 |

### `AsyncScope` `async`

线程安全异步作用域（`Send + Sync`）。

| 方法 | 说明 |
|---|---|
| `AsyncScope::new()` | 创建空异步作用域 |
| `register::<M>()` | 注册模块 |
| `insert::<M>(cap)` | 插入预构建能力 |
| `require::<M>()` | 检索能力 |
| `contains::<M>()` | 检查是否已注册 |

---

## 错误类型

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

## 国际化

### `I18nManager`

```rust
let mgr = I18nManager::init();           // 自动检测系统 locale
let mgr = I18nManager::init_with_locale("zh-CN")?;  // 指定 locale
```

### `tr()` — 消息翻译

```rust
use trait_kit::i18n::tr;
let msg = tr("trait-kit-error-cycle-detected", &[("cycle", "A → B → A")]);
```

### `I18nFormatter` — 本地化格式化

```rust
use trait_kit::i18n::I18nFormatter;
let fmt = I18nFormatter::new("zh-CN")?;
fmt.format_number(1234567.89);  // "1,234,567.89"
fmt.format_date(...);
```

---

## Prelude

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
