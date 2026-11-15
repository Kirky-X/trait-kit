# 📖 Trait-Kit User Guide

This guide takes you from installation to mastery of trait-kit's core concepts and advanced usage. trait-kit is a lightweight Rust library providing standardized module interfaces and a centralized capability & config management center (`Kit`), using the typestate pattern (`Kit<Unbuilt>` → `Kit<Ready>`) for build-time validation.

## 📋 Table of Contents

<details open>
<summary>📑 Table of Contents</summary>

- [🌟 Introduction](#-introduction)
- [🚀 Quick Start](#-quick-start)
- [🧭 Core Concepts](#-core-concepts)
- [⚙️ Configuration](#️-configuration)
- [🔧 Advanced Usage](#-advanced-usage)
- [✅ Best Practices](#-best-practices)
- [🩺 Troubleshooting](#-troubleshooting)

</details>

---

## 🌟 Introduction

The problem trait-kit solves: **assembling module dependencies at application startup in a type-safe, verifiable way**.

- **Modules** define a uniform contract via the two traits `ModuleMeta` + `AutoBuilder`, declared in one line with the `impl_module_meta!` macro.
- **Kit** is the centralized management center for capabilities and configs: at the `Kit<Unbuilt>` stage you register modules and configs; `build()` validates the dependency graph and yields `Kit<Ready>`, after which retrieval is read-only.
- **Capabilities** are stored and retrieved by module type (`kit.require::<LoggerModule>()`), with no string keys and no downcast.
- The whole crate is annotated `#![deny(unsafe_code)]`; production code has only 5 locally audited exemptions with SAFETY justifications and code review (typestate layout-assert casts ×3, volatile zeroization ×1, guard lifetime re-anchoring ×1: the `AsyncTypeMap` `RwLock` read guard; the sync `TypeMap` now uses a `Ref::map` guard wrapper and needs no unsafe), plus 1 test-module exemption (the zeroize spare-capacity assertion in `src/kit/config.rs`); see the [Security document](SECURITY.md).

It fits the middle ground where "manual wiring is too scattered and a full DI framework too heavy". For the full API see the [📘 API Reference](../API_REFERENCE.md); for design details see the [🏗️ Architecture document](ARCHITECTURE.md).

---

## 🚀 Quick Start

### Requirements

- Rust **1.97.1** or later (MSRV).

### Installation

```sh
cargo add trait-kit
```

The default features (`default = []`) only include the core `ModuleMeta` + `AutoBuilder` + `Kit`. The crate always ships the 3 mandatory dependencies — Fluent translation (`fluent-bundle` / `unic-langid`) and the `log` facade — while all remaining dependencies (`confers`, `serde`, `icu_*` and other ICU4X components) are optional and feature-gated.

### Your First Module

Three steps: define the capability → define the module → register, build, use.

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

For fully runnable examples see [examples/README.md](../../examples/README.md).

---

## 🧭 Core Concepts

### Two-Phase Typestate

- **`Kit<Unbuilt>` (build phase)**: register modules, store configs, declare lifecycle hooks. Calling `optional()` in this phase fails **to compile**; retrieval methods such as `require()` / `config()` do compile in this phase (for use inside `AutoBuilder::build` callbacks to read dependencies and configs) and return errors for unbuilt modules.
- **`kit.build()`**: validates the dependency graph (missing-dependency detection, Kahn-algorithm cycle detection + topological sort) and builds all modules in topological order.
- **`Kit<Ready>` (runtime phase)**: read-only retrieval of capabilities and configs; registration is no longer possible (registration and build methods are **rejected at compile time** in this phase).

For the full method surface and flow diagrams of both phases see the [Architecture document](ARCHITECTURE.md); for per-method feature gates see the [API Reference](../API_REFERENCE.md).

### Capability

A capability is the value a module produces after building; it must implement `Clone` (usually wrapped in `Arc<T>`). It is stored in the Kit's internal `TypeMap` (keyed by `TypeId`); retrieval methods:

| Method | Behavior |
|---|---|
| `kit.require::<M>()` | Retrieve and clone; errors when missing |
| `kit.optional::<M>()` | Returns `None` when missing |
| `kit.require_ref::<M>()` | Zero-copy borrowed retrieval |
| `kit.require_all::<M>()` | Returns all multi-binding capabilities |
| `kit.contains::<M>()` | Checks whether the module is built |

### Dependency Declaration and Build Order

Modules declare dependencies via the `deps` parameter of `impl_module_meta!`. The Kit validates the dependency graph at `build()` and builds in topological order, ensuring dependencies build first:

```rust
impl_module_meta!(StorageModule, "storage", deps = [LoggerModule]);
```

A module whose build fails reports the original error via `TraitKitError::BuildFailed`; the whole `build()` completes before application startup, so dependency problems never leak into runtime.

### Threading Model

- **Sync `Kit`**: based on `RefCell` interior mutability, a single-threaded `!Sync` design with no lock overhead.
- **`AsyncKit`** (`async` feature): based on `Arc<RwLock>`, `Send + Sync`, suited to async initialization scenarios such as database connection pools and HTTP clients.

---

## ⚙️ Configuration

Configs are **typed values** stored in the Kit's `TypeMap`, keyed by type — no `ConfigKey` boilerplate.

### Manual Storage and Retrieval

`set_config` is callable only on `Kit<Unbuilt>`; modules retrieve configs during `build()` via `kit.config::<C>()`. Config-write paths on `Kit<Ready>` include `reload_config::<C>()` (`reload` feature), `merge_config::<C>(ovr)` and `populate_defaults::<C>()` (`confers` feature):

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

### Loading from the Environment via confers `confers`

With the `confers` feature enabled, you can hook into [`confers`](https://crates.io/crates/confers)' `#[derive(Config)]` macro to load configs from environment variables/defaults:

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

Companion APIs:

| API | Purpose |
|---|---|
| `load_and_validate::<C>()` | Load and run `Validatable::validate`; failures are not stored |
| `load_config_with::<C>(vars)` | `${VAR}` variable substitution during loading (returning an error when the nesting depth exceeds the cap, never terminating the process) |
| `load_config_or_default::<C>()` | On load failure, fall back to `ModuleConfig::default_value()` (returns whether loading succeeded) |
| `snapshot_config::<C>()` / `restore_config::<C>()` | Config snapshot and rollback |
| `subscribe::<C>(cb)` / `reload_config::<C>()` `reload` | Hot-reload subscription and triggering |

### Module Config Metadata `confers`

The `ModuleConfig` trait declares `PATH` and `default_value()` for a config type, binding the config to the module's config path (also a factor in encryption key derivation):

```rust,ignore
use trait_kit::kit::config::ModuleConfig;

impl ModuleConfig for TraitKitConfig {
    const PATH: &'static str = "config/app.toml";
    fn default_value() -> Self {
        Self { host: "localhost".to_string() }
    }
}
```

### Encrypted Config `encryption`

With the `encryption` feature enabled, static configs can be stored encrypted with XChaCha20-Poly1305. Encryption keys are derived via HKDF from the master key and `ModuleConfig::PATH`; the same master key yields distinct field keys for different modules:

```rust,ignore
let mut kit = Kit::new();
let secret = TraitKitConfig { host: "production-db".to_string() };
let master_key = *b"demo-master-key-0123456789abcdef"; // 32 字节主密钥（演示用，生产环境请从密钥管理服务获取）

kit.set_encrypted(&secret, &master_key)?;
let kit = kit.build()?;

let decrypted: TraitKitConfig = kit.get_encrypted(&master_key)?;  // 错误密钥会失败
```

### Config Inheritance `confers`

The four-layer config inheritance system supports config reuse across modules/projects (`trait-kit-macros` provides the `ConfigInherit` / `SharedConfig` derive macros):

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

Four-layer mechanism: `merge_json_deep` deep merge → `ConfigInherit` field override (`Option<T>` overridden only when `Some`) → `SharedConfig` shared-field overlay → `populate_defaults` zero-config fill. See the [Architecture document](ARCHITECTURE.md) for the principles.

---

## 🔧 Advanced Usage

### Async Modules `async`

`AsyncKit` is API-symmetric with `Kit`; capability types require `Send + Sync`, build methods are async (`AsyncAutoBuilder`), and the flow is `AsyncKit::new()` → `register::<M>()` → `build().await` → `require::<M>()` (`require` is a synchronous method, no `.await` needed).

For the fully runnable example see [`examples/src/kit/async_basic.rs`](../../examples/src/kit/async_basic.rs):

```sh
cargo run -p trait-kit-examples --example async_basic --features async
```

### Feature-Gated Capabilities at a Glance

The following capabilities are all used via methods on `Kit` / `AsyncKit` (method-level feature gating; see the [API Reference](../API_REFERENCE.md) for details):

| Capability | Feature | Typical use |
|---|---|---|
| Lifecycle hooks (`on_ready` / `on_shutdown`) | `lifecycle` | Post-build initialization, cleanup and release |
| Health checks (`HealthCheck` + `health_report`) | `health` | Runtime status probes |
| Scopes (`Scope` per-request isolation) | `request-scope` | Web request-scoped instances |
| Feature toggles (`enable_toggle` / `register_if_toggle`) | `toggle` | Runtime module enable/disable |
| Build observers (`BuildObserver` callbacks) | `observer` | Build timing stats, logging |
| Decorators (`decorate::<M>(f)`) | `decorator` | Capability wrapping (e.g. logging, metrics) |
| Graceful shutdown (`ShutdownCoordinator` phased shutdown + timeout) | `shutdown` | Service teardown orchestration |
| Interface separation (`register_as` / `resolve::<dyn Trait>()`) | `di` | Interface-oriented programming, test doubles |
| Internationalized formatting (`I18nFormatter`) | `i18n` | Locale-aware number/date/plural/collation (`tr()` translation is always available) |

### Conditional Registration and Factories

Usable without any feature:

- `kit.register_if::<M>(pred)` — a runtime predicate decides whether the module registers;
- `kit.factory::<M>()` — a fresh instance per call (vs the singleton semantics of `require()`).

### Test Injection

Use `override_module::<M>(cap)` to override a module's capability with a pre-built value, skipping the real build function; injection validates dependency existence, returning `DependencyMissing` for unregistered dependencies:

```rust,ignore
kit.override_module::<LoggerModule>(Arc::new(test_logger));
let kit = kit.build()?;
```

---

## ✅ Best Practices

1. **Declare modules with macros**: `impl_module_meta!` / `impl_auto_builder!` remove boilerplate; always write dependencies into `deps = [...]` so cycle detection and missing-dependency detection take effect before startup.
2. **Wrap capabilities in `Arc<T>`**: capabilities must be `Clone`; use `Arc<Service>` for shared mutable services to avoid deep copies.
3. **Declare configs explicitly**: modules needing config read `kit.config::<C>()` in `build()`, failing fast when missing (`MissingConfig`) instead of silently falling back inside the capability.
4. **Enable features on demand**: higher-level confers features automatically inherit lower ones (`encryption` → `reload` → `confers`) — enabling the highest level you need suffices; keep unused features off to minimize compilation and dependencies.
5. **`Kit` for single thread, `AsyncKit` for many**: do not try to share `Kit` across threads (`!Sync` is intentional).
6. **Use `override_module` in tests**: replace external dependencies (databases, networks) with controlled implementations to keep tests deterministic.
7. **Handle errors uniformly via `TraitKitError`**: its `Display` is automatically localized via `tr()`; business code only needs to `match` the variants requiring special handling.

---

## 🩺 Troubleshooting

Common errors all appear as `TraitKitError` variants, with `Display` localized automatically:

| Error | Trigger scenario | Handling advice |
|---|---|---|
| `CycleDetected { cycle }` | The dependency graph contains a cycle | Check the `deps = [...]` declarations and break the cycle (split the module or introduce an intermediate module) |
| `DependencyMissing { module, missing }` | A declared dependency is not registered | Add `kit.register::<MissingModule>()` before `build()` |
| `AlreadyRegistered { module }` | The module was registered twice | Register each module once; for multi-instance needs use `register_multi` |
| `DecoratorTargetMissing { module }` | The decorator's target module is not registered | Use `try_decorate::<M>(f)` to validate at registration, or `register::<M>()` first |
| `VersionIncompatible { .. }` | A dependency's version does not satisfy `required_versions` (`version-negotiation` feature) | Align `ModuleMeta::VERSION` with a semver-compatible version |
| `BuildFailed { context, source }` | The module's build function returned an error | Inspect the original error in `source`; usually an unavailable external resource |
| `MissingCapability { key }` | The capability does not exist at `require::<M>()` | Confirm the module is registered and `build()` has completed; use `optional::<M>()` where appropriate |
| `CapabilityTypeMismatch { key }` | The capability was built but its type mismatches (e.g. override injected another capability type) | Check that the `override_module`-injected value's type matches the module's declaration |
| `MissingConfig { key }` | The config does not exist at `config::<C>()` | Call `set_config(value)` first, or use confers' `load_config::<C>()` |
| `LifecycleFailed { .. }` `lifecycle` | The `on_ready` hook failed | Check the hook's initialization logic and external dependencies |
| `ShutdownTimedOut { phases }` `shutdown` | Graceful shutdown timed out | Tune `set_phase_timeout` / `set_global_timeout` and check for blocking hooks |

Other common issues:

- **typestate compile errors**: compile-time rejection covers calling registration/build methods on `Kit<Ready>` (`ready_cannot_register` / `ready_cannot_build` / `ready_cannot_set_config` / `ready_cannot_override_module` / `ready_cannot_with_metrics_port`) and calling `optional()` on `Kit<Unbuilt>` (sync `unbuilt_cannot_optional` / async `async_unbuilt_cannot_optional`); for the full fixture list see `tests/ui/` (11, including Ready-state violations in the decorator / lifecycle / observer / probe gate groups) and check your call phase. `require()` compiles on `Kit<Unbuilt>` (for use inside `AutoBuilder::build` callbacks) and returns a `MissingCapability` error for unbuilt modules rather than failing compilation.
- **feature-gated method not found**: method-level gated APIs (e.g. `subscribe`, `set_encrypted`) require their feature; see [Feature Flags](#-feature-flags).
- **`!Sync` error when using `Kit` across threads**: switch to `AsyncKit` (`async` feature), or use `Kit` within a single thread.

Still stuck? Search or ask on [GitHub Issues](https://github.com/Kirky-X/trait-kit/issues).
