<div align="center">

<img src="docs/assets/trait-kit.svg" alt="trait-kit logo" width="180">

[![CI Status](https://github.com/Kirky-X/trait-kit/actions/workflows/ci.yml/badge.svg)](https://github.com/Kirky-X/trait-kit/actions/workflows/ci.yml) [![Version](https://img.shields.io/crates/v/trait-kit.svg)](https://crates.io/crates/trait-kit) [![Docs.rs](https://docs.rs/trait-kit/badge.svg)](https://docs.rs/trait-kit) [![Downloads](https://img.shields.io/crates/d/trait-kit.svg)](https://crates.io/crates/trait-kit) [![License](https://img.shields.io/crates/l/trait-kit.svg)](LICENSE) [![Rust](https://img.shields.io/badge/rust-1.97.1%2B-orange.svg)](https://www.rust-lang.org/)

[中文](README.md) | **English**

**Standardized module interfaces + a `Kit` capability center**

[✨ Features](#-features) • [🚀 Quick Start](#-quick-start) • [📚 Documentation](#-documentation) • [💻 Examples](#-examples) • [🤝 Contributing](#-contributing)

</div>

---

<div align="center" style="padding: 32px; margin: 24px 0">

### 🧩 Standardized Module Assembly

Modules declare a contract via `ModuleMeta` + `AutoBuilder`; the Kit centralizes assembly, validation, and capability lookup:

<table style="width:100%; border-collapse: collapse">
<tr><td align="center" width="25%" style="padding: 12px">🧩<br><b>Standard Module Interface</b><br><span style="color:#64748B">uniform contract, one-line macro declaration</span></td><td align="center" width="25%" style="padding: 12px">🏗️<br><b>Build-Time Validation</b><br><span style="color:#64748B">typestate dependency-graph checks before startup</span></td><td align="center" width="25%" style="padding: 12px">🔎<br><b>Type-Safe Retrieval</b><br><span style="color:#64748B">by module type, no string keys, no downcast</span></td><td align="center" width="25%" style="padding: 12px">⚡<br><b>Extensible on Demand</b><br><span style="color:#64748B">18 optional features, all gated, zero default cost</span></td></tr>
</table>

</div>

---

## 📋 Table of Contents

<details open>
<summary>📑 目录</summary>

- [✨ Features](#-features)
- [🚀 Quick Start](#-quick-start)
- [🎨 Feature Flags](#-feature-flags)
- [📚 Documentation](#-documentation)
- [💻 Examples](#-examples)
- [🏗️ Architecture](#️-architecture)
- [🧪 Testing](#-testing)
- [📊 Performance](#-performance)
- [🔒 Security](#-security)
- [🗺️ Roadmap](#️-roadmap)
- [🤝 Contributing](#-contributing)
- [📋 Changelog](#-changelog)
- [📄 License](#-license)
- [🙏 Acknowledgments](#-acknowledgments)
- [📞 Contact & Support](#-contact--support)
- [⭐ Star History](#-star-history)

</details>

---

## ✨ Features

<div align="center">

<table>
<tr>
<td width="50%">🧩 <b>Standardized Module Interface</b><br><sub>The <code>ModuleMeta</code> + <code>AutoBuilder</code> traits define a uniform contract; <code>impl_module_meta!</code> / <code>impl_auto_builder!</code> macros declare a module in one line.</sub></td>
<td width="50%">🏗️ <b>Typestate Build Validation</b><br><sub><code>Kit&lt;Unbuilt&gt;</code> registers modules and configs; <code>build()</code> validates the dependency graph (missing deps + cycle detection) and returns <code>Kit&lt;Ready&gt;</code>, surfacing errors before startup.</sub></td>
</tr>
<tr>
<td width="50%">🔎 <b>Type-Safe Capability Retrieval</b><br><sub>Capabilities are stored and retrieved by module type (<code>kit.require::&lt;M&gt;()</code>): no string keys, no downcast, no runtime lookup tables.</sub></td>
<td width="50%">🗂️ <b>Configuration Center</b><br><sub><code>set_config</code> / <code>config::&lt;C&gt;</code> store and retrieve typed configs via a <code>TypeId</code>-keyed <code>TypeMap</code>.</sub></td>
</tr>
<tr>
<td width="50%">⚙️ <b>confers Config Integration</b><br><sub>Three-level feature inheritance integrates <a href="https://crates.io/crates/confers">confers</a>: derive-macro config loading, hot-reload subscriptions, and XChaCha20-Poly1305 encrypted storage.</sub></td>
<td width="50%">🌐 <b>AsyncKit Async Support</b><br><sub>The <code>async</code> feature provides a <code>Send + Sync</code> <code>AsyncKit</code> for DB pools, HTTP clients, and other async initialization scenarios.</sub></td>
</tr>
<tr>
<td width="50%">🩺 <b>Runtime Observability</b><br><sub><code>lifecycle</code> hooks, <code>health</code> checks, <code>observer</code> build callbacks, and <code>shutdown</code> phased graceful shutdown.</sub></td>
<td width="50%">🌍 <b>ICU4X Internationalization</b><br><sub>Locale-aware number / date / plural / collation formatting, plus built-in Fluent FTL translation (<code>tr()</code>) for English and Chinese.</sub></td>
</tr>
<tr>
<td width="50%">🧱 <b>Minimal Default Dependencies</b><br><sub><code>default = []</code>: zero default dependencies; <code>confers</code>, <code>serde</code>, <code>serde_json</code>, <code>icu</code>, etc. are all optional and feature-gated.</sub></td>
<td width="50%">🚫 <b>No unsafe</b><br><sub>The entire crate carries <code>#![deny(unsafe_code)]</code>, enforced at compile time.</sub></td>
</tr>
</table>

</div>

<details>
<summary>More capabilities (registration modes, build reports, composition, negotiation)</summary>

- **Flexible registration modes**: `register_lazy` (built and cached on first `require`), `register_multi` + `require_all` (multi-binding aggregation), `register_if` (runtime predicates), `override_module` (test injection), `factory::<M>()` (a fresh instance per call).
- **Interface/implementation separation** (`interface`): `register_as` / `resolve::<dyn Trait>()` type-erased registration and retrieval.
- **Scoped dependencies** (`scope`): `Scope` / `AsyncScope` per-request instance isolation.
- **Feature toggles** (`toggle`): `enable_toggle` / `is_toggle_enabled` / `register_if_toggle` runtime string-keyed enable/disable.
- **Module decorators** (`decorator`): `decorate::<M>(fn)` post-build capability wrapping/enhancement.
- **Structured build reports** (`report`): `BuildReport` JSON export plus `graph_dot()` / `graph_mermaid()` dependency-graph exports.
- **Preset modules** (`presets` / `presets-remote`): `ConfersConfigModule` turns the confers config hub into a first-class Kit module, with remote config source support.
- **Sub-Kit composition** (`compose`): register a child `Kit` as a single module in a parent `Kit`, with namespaced capabilities.
- **Version negotiation** (`negotiate`): `ModuleMeta::VERSION` vs `required_versions` semver-compat validation at `build()` time.
- **Event bus and observation ports** (always available, no feature gate): `KitEvent` / `EventBus` lifecycle events and injectable `MetricsPort` / `LogPort`, with zero-cost `NoOp` defaults.

</details>

### 💡 Why trait-kit?

Compared with common dependency-wiring approaches, trait-kit sits between "raw manual wiring" and "full DI framework":

| Approach | Pros | Cons |
| --- | --- | --- |
| **Manual wiring** | Simple, no deps. | Ad-hoc patterns, inconsistent per project. |
| **trait-kit** | Standard pattern, type-safe, lightweight. | You still wire dependencies explicitly. |
| **Full DI (shaku etc.)** | Auto-resolved, less glue code. | Heavier deps, magic, harder to debug. |

trait-kit gives you the **standardization** of a DI framework with the **explicitness** of manual wiring.

---

## 🚀 Quick Start

### 📦 Installation

Minimum Supported Rust Version (MSRV): **1.97.1** (edition 2024).

```sh
cargo add trait-kit
```

The default features only include the core `ModuleMeta` + `AutoBuilder` + `Kit`, with no extra dependencies.

### 💡 Minimal Example

Define a logger module, register it, build the Kit, then retrieve the capability (source: [examples/src/core/default_basic.rs](examples/src/core/default_basic.rs), trimmed):

```rust
use std::sync::Arc;
use trait_kit::prelude::*;

// 1. Define a capability (any Clone type)
struct StdoutLogger;
impl StdoutLogger {
    fn info(&self, msg: &str) {
        println!("[LOG] {msg}");
    }
}

// 2. Define the module: ModuleMeta declares the name and dependencies,
//    AutoBuilder declares how the capability is built
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

// 3. Register, build, and use
fn main() {
    let mut kit = Kit::new();
    kit.register::<LoggerModule>().unwrap();
    let kit = kit.build().unwrap();

    let logger = kit.require::<LoggerModule>().unwrap();
    logger.info("Hello from trait-kit!");
    assert!(kit.contains::<LoggerModule>());
}
```

### 🧭 Core Concepts

- **Module**: a type implementing `ModuleMeta` (name + dependency declaration) and `AutoBuilder` (capability construction); the `impl_module_meta!` macro removes the hand-written impl.
- **Capability**: the `Clone` value a module produces at build time, stored in the `TypeMap` and retrieved by module type.
- **Typestate**: `Kit<Unbuilt>` only registers, `Kit<Ready>` only retrieves; misuse is a compile-time error.
- **Feature gates**: async, config, lifecycle, and other capabilities are enabled on demand; see [Feature Flags](#-feature-flags).

### ⚙️ Module with Configuration

Configs are typed values stored in the Kit's `TypeMap`; modules retrieve them via `kit.config::<C>()` during build:

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

### 🔗 Module with Dependencies

Modules declare dependencies via `ModuleMeta::dependencies()` (the `impl_module_meta!` macro supports a `deps = [...]` syntax). The Kit validates the dependency graph at `build()` and constructs modules in topological order:

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
// One-line macro declaration of the name + dependencies
// (equivalent to a hand-written dependencies())
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

For the complete `Kit<Unbuilt>` / `Kit<Ready>` method list (including feature gates), see the [📘 API Reference](docs/API_REFERENCE.md) and [docs.rs](https://docs.rs/trait-kit).

---

## 🎨 Feature Flags

`default = []`: no optional feature is enabled by default. Higher-level features automatically inherit lower ones (`encryption` → `reload` → `confers`).

<div align="center">

<table>
<tr><th>Feature</th><th>Enables</th><th>Description</th><th>Default</th></tr>
<tr><td><code>async</code></td><td>—</td><td><code>AsyncKit</code>: <code>Send + Sync</code> async capability management, no extra deps.</td><td>—</td></tr>
<tr><td><code>confers</code></td><td><code>dep:confers</code>, <code>dep:serde</code>, <code>dep:serde_json</code>, <code>confers/feature-toggle</code></td><td><code>Configurable</code> + <code>ModuleConfig</code> trait + <code>Config</code> derive re-export.</td><td>—</td></tr>
<tr><td><code>reload</code></td><td><code>confers</code></td><td><code>subscribe</code> / <code>reload_config</code> hot-reload subscriptions.</td><td>—</td></tr>
<tr><td><code>encryption</code></td><td><code>confers</code>, <code>confers/encryption</code></td><td><code>set_encrypted</code> / <code>get_encrypted</code> encrypted config storage.</td><td>—</td></tr>
<tr><td><code>interface</code></td><td>—</td><td>Interface/implementation separation: <code>register_as</code> / <code>resolve</code> with <code>dyn Trait</code> type erasure.</td><td>—</td></tr>
<tr><td><code>lifecycle</code></td><td>—</td><td>Lifecycle hooks: <code>on_ready</code> (after build) + <code>on_shutdown</code> (cleanup).</td><td>—</td></tr>
<tr><td><code>health</code></td><td>—</td><td>Health checks: <code>HealthCheck</code> trait + <code>HealthStatus</code> reporting.</td><td>—</td></tr>
<tr><td><code>scope</code></td><td>—</td><td>Scoped dependencies: <code>Scope</code> / <code>AsyncScope</code> per-request instance isolation.</td><td>—</td></tr>
<tr><td><code>toggle</code></td><td>—</td><td>Feature toggle: runtime string-keyed module enable/disable.</td><td>—</td></tr>
<tr><td><code>observer</code></td><td>—</td><td>Build observability: <code>BuildObserver</code> callbacks (start/complete/error).</td><td>—</td></tr>
<tr><td><code>decorator</code></td><td>—</td><td>Module decorator: post-build capability wrapping/enhancement.</td><td>—</td></tr>
<tr><td><code>shutdown</code></td><td>—</td><td>Graceful shutdown coordinator: phased ordered shutdown + timeout force-exit.</td><td>—</td></tr>
<tr><td><code>i18n</code></td><td><code>dep:icu</code>, <code>dep:writeable</code>, <code>dep:sys-locale</code></td><td>ICU4X internationalization: locale-aware number/date/plural/collation formatting.</td><td>—</td></tr>
<tr><td><code>report</code></td><td><code>dep:serde</code>, <code>dep:serde_json</code></td><td>Structured build report: <code>BuildReport</code> JSON, dependency-graph DOT/Mermaid exports.</td><td>—</td></tr>
<tr><td><code>presets</code></td><td><code>confers</code></td><td>Preset module packages: <code>ConfersConfigModule</code> (the config hub as a Kit module) + composition builder.</td><td>—</td></tr>
<tr><td><code>compose</code></td><td>—</td><td>Sub-Kit composition: register a child <code>Kit</code> as a single parent module (namespaced capabilities + cross-Kit dependency validation).</td><td>—</td></tr>
<tr><td><code>presets-remote</code></td><td><code>presets</code>, <code>confers/remote</code></td><td>Remote config bridge: <code>ConfersConfigModule</code> over the confers remote <code>AsyncSource</code> (<code>AsyncKit</code> only).</td><td>—</td></tr>
<tr><td><code>negotiate</code></td><td>—</td><td>Capability version negotiation: <code>ModuleMeta::VERSION</code> vs <code>required_versions</code> semver-compat validation at <code>build()</code> time.</td><td>—</td></tr>
</table>

</div>

Enable the desired level in `Cargo.toml`:

```toml
[dependencies]
trait-kit = { version = "0.5.0-rc.3", features = ["encryption"] }
```

---

## 📚 Documentation

| Document | Description |
|----------|-------------|
| [📖 User Guide](docs/USER_GUIDE.md) | Complete tutorial from installation to advanced usage |
| [📘 API Reference](docs/API_REFERENCE.md) | Quick reference for all public APIs, each annotated with its feature gate |
| [🏗️ Architecture](docs/ARCHITECTURE.md) | Design patterns, data flow, thread-safety model, and directory layout |
| [🧪 Test Scenarios](docs/TEST_SCENARIOS.md) | Exhaustive acceptance-scenario matrix mapped to e2e tests |
| [⚡ Performance](docs/PERFORMANCE.md) | Criterion benchmark setup, baseline data, and reproduction method |
| [🔒 Security](docs/SECURITY.md) | Support policy, vulnerability reporting process, and security design |
| [📋 Changelog](docs/CHANGELOG.md) | Release notes for every version |
| [🤝 Contributing](docs/CONTRIBUTING.md) | Environment setup, TDD workflow, and commit conventions |
| [📦 crates.io](https://crates.io/crates/trait-kit) / [docs.rs](https://docs.rs/trait-kit) | Released versions and the online API docs |

---

## 💻 Examples

`examples/` is a standalone workspace member `trait-kit-examples` (`publish = false`) with 20 standalone runnable examples covering every public API and feature gate. Run with:

```sh
cargo run -p trait-kit-examples --example <name> --features <feature>
```

| Example | Feature | Demonstrates |
|---------|---------|--------------|
| `default_basic` | — | `ModuleMeta` + `AutoBuilder` + basic `Kit` register/build/require flow |
| `conditional` | — | `register_if::<M>(predicate)` runtime predicate-gated registration |
| `factory` | — | `Kit<Ready>::factory::<M>()` per-call instance creation (vs singleton `require()`) |
| `interface` | `interface` | `InterfaceBuilder` + `register_as` / `resolve::<dyn Trait>()` type-erased DI |
| `lifecycle` | `lifecycle` | `Lifecycle` trait (`on_ready` + `on_shutdown`) + `Kit::shutdown()` |
| `health_check` | `health` | `HealthCheck` trait + `HealthStatus` + `health_report` |
| `observability` | `observer` | `BuildObserver` callbacks (`on_module_start` / `on_module_built`) |
| `confers_loader` | `confers` | `#[derive(Config)]` + `Configurable` + `Kit::load_config` (env-var loading) |
| `confers_macros` | `confers` | `ModuleConfig` trait (`PATH` + `default_value`) + consuming config in `build()` |
| `validation` | `confers` | `Validatable` trait + `Kit::load_and_validate` config validation |
| `snapshot_restore` | `confers` | `snapshot_config` / `restore_config` / `has_snapshot` snapshot & rollback |
| `config_inheritance` | `confers` | Four-layer config inheritance (`merge_json_deep` → `ConfigInherit` → `SharedConfig` → `populate_defaults`) |
| `hot_reload` | `reload` | `subscribe::<C>` + `reload_config::<C>` hot-reload subscriptions |
| `encryption` | `encryption` | `set_encrypted` / `get_encrypted` roundtrip + wrong-key rejection |
| `async_basic` | `async` | `AsyncAutoBuilder` + `AsyncKit` async register/build/require |
| `scope_basic` | `scope` | `Scope` per-request instance isolation + lazy caching |
| `toggle_basic` | `toggle` | `enable_toggle` / `is_toggle_enabled` / `register_if_toggle` runtime toggles |
| `decorator` | `decorator` | `Kit::decorate::<M>(fn)` post-build capability wrapping |
| `shutdown` | `shutdown` | `ShutdownCoordinator` phased graceful shutdown (`StopRequests` → `DrainQueue` → `CloseConnections`) + timeout control |
| `i18n` | `i18n` | `I18nFormatter` locale-aware number/date/plural/collation formatting |

See [examples/README.md](examples/README.md) for details.

---

## 🏗️ Architecture

The trait-kit workspace has four members: the main `trait-kit` crate, the proc-macro crates `trait-kit-derive` and `trait-kit-macros`, and the examples crate `trait-kit-examples`; the main crate's `src/` is organized into the `core` interface layer, the `kit` capability management center, and the `i18n` layer.

**Core Design**:

- **Typestate Pattern**: `Kit<Unbuilt>` → `Kit<Ready>`, build-time dependency-graph validation, zero runtime overhead.
- **Interior Mutability**: the sync `Kit` is `RefCell`-based with a single-threaded `!Sync` design that avoids lock overhead; `AsyncKit` uses `Arc<RwLock>` for multi-threading.
- **Three-Level Feature Inheritance** (confers integration): see [Feature Flags](#-feature-flags) for each feature's enablement chain.

The workspace diagram, dependency-graph validation, data flow, thread-safety model, and directory layout are documented in full in the [Architecture document](docs/ARCHITECTURE.md).

---

### 🔄 Build Lifecycle

All capability retrieval happens after `build()`: registration methods live on `Kit<Unbuilt>`, retrieval methods on `Kit<Ready>`, and "require before build" is ruled out at compile time (asserted by trybuild UI tests in `tests/ui/`).

| Phase | Typestate | Available operations |
|-------|-----------|----------------------|
| Registration | `Kit<Unbuilt>` | `register` / `register_lazy` / `register_multi` / `register_if` / `register_as` / `override_module` / `set_config` / `build` |
| Runtime | `Kit<Ready>` | `require` / `require_ref` / `require_all` / `optional` / `factory` / `resolve` / `contains` / `config` / `health_check` / `shutdown` |

The actual execution path inside `build()` (missing-dep check → cycle detection and topological sort → per-module build in topo order → return `Kit<Ready>`; per `src/kit/kit.rs`): for the full sequence diagram see the [Architecture document · Data flow](docs/ARCHITECTURE.md#-数据流).

---

### ⚙️ Configuration: confers Integration

trait-kit integrates with [`confers`](https://crates.io/crates/confers) 0.6 via three-level feature flags; each level inherits the previous one, forming a layered capability system.

| Level | Feature | Capability |
|-------|---------|------------|
| 1. Config loading | `confers` | `Configurable` bridges confers' `#[derive(Config)]`; `load_config::<C>()` loads from env vars/defaults |
| 2. Module config metadata | `confers` | `ModuleConfig` trait declares `PATH` and `default_value()`, binding a config type to its module's config path |
| 3. Hot reload | `reload` | `subscribe::<C>()` subscriptions + `reload_config::<C>()` reload with subscriber notification |
| 4. Encrypted storage | `encryption` | `set_encrypted` / `get_encrypted`: XChaCha20-Poly1305 AEAD with keys derived via HKDF from the master key and `ModuleConfig::PATH` |
| 5. Config inheritance | `confers` + `trait-kit-derive` | Four-layer system: `merge_json_deep` deep merge → `ConfigInherit` compile-time safe field override → `SharedConfig` cross-type shared fields → `populate_defaults` zero-config defaults |

Typical usage of config loading and config inheritance (fully runnable versions live in the `confers_loader` and `config_inheritance` examples):

```rust,ignore
use trait_kit::prelude::*;
use trait_kit_derive::{ConfigInherit, SharedConfig};

// Levels 1-2: derive-based config loading + module config metadata
#[derive(Debug, Clone, serde::Deserialize, confers::Config)]
#[config(env_prefix = "APP_")]
struct TraitKitConfig {
    #[config(default = "localhost".to_string())]
    host: String,
}

// Level 5: cross-type shared field inheritance
#[derive(Clone, ConfigInherit, SharedConfig)]
#[shared(host)]
struct DbConfig {
    host: String,
    port: u16,
}

let mut kit = Kit::new();
kit.load_config::<TraitKitConfig>()?;        // load from env/defaults via confers
kit.populate_defaults::<DbConfig>()?;   // zero-config defaults
kit.extract_shared::<TraitKitConfig>()?;     // extract shared fields
kit.inject_shared::<DbConfig>()?;       // inject into DbConfig
```

- `trait-kit-derive` provides the `#[derive(ConfigInherit)]` and `#[derive(SharedConfig)]` macros; shared fields use `serde_json::Value` to preserve type information.
- `AsyncKit` offers a fully symmetric `Send + Sync` config API.

---

## 🧪 Testing

### Test Strategy

| Type | Location | Description |
|------|----------|-------------|
| Unit tests | `src/` (`#[cfg(test)]`) | Module-internal logic |
| Integration tests | `tests/` (6 targets) + `tests/e2e/` (13 registered targets) | `basic`, `config_inheritance_e2e`, `e2e_core`, `e2e_async`, `e2e_concurrency`, `e2e_feature_combinations`, etc. |
| Compile-time UI tests | `tests/compile_fail.rs` + `tests/ui/` (3 cases) | trybuild-based: assert typestate misuse (e.g. require before build) fails to compile |
| Macro-crate tests | `trait-kit-macros/tests/` | `#[derive(Module)]` expansion correctness and compile-fail cases |
| Doc tests | `rust` code blocks in the README and doc comments | Compiled and run by `cargo test` |
| Example validation | `examples/` (20) | Each example runs standalone; failed assertions panic |
| Benchmarks | `benches/kit_bench.rs` | criterion benchmarks (requires the `toggle` feature) |

Test scale: the main crate's `src/` and `tests/` contain **754** `#[test]` functions (as of **0.5.0-rc.3**, `grep` count), plus 6 in `trait-kit-macros/tests/`.

### Common Commands (same as CI)

```sh
# Full test run (workspace + all features, same as the CI test job)
cargo test --workspace --all-features

# Run with a specific feature combination
cargo test --features confers

# Lint and format checks (CI gates, zero warnings)
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check

# Zero-warning docs
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features

# Dependency audit (advisories / licenses / bans / sources)
cargo deny check

# Coverage gate (CI coverage job: ≥ 80% line coverage)
cargo llvm-cov --workspace --all-features --fail-under-lines 80
```

---

## 📊 Performance

Performance comes from design: typestate moves dependency-graph validation entirely into `build()`, capability retrieval on `Kit<Ready>` is a `TypeId` lookup + clone, and the sync `Kit` is lock-free via `RefCell`. The criterion benchmark setup covers the build / require / config / toggle hot-path axes (`benches/kit_bench.rs`; benchmark targets require `--features toggle`); the baseline (2026-09-10) measured `require` on the same magnitude as a bare `Arc::clone` + `TypeId` lookup, validating the "zero-cost capability retrieval" design goal — for large capability structs prefer `require_ref` (borrowed read).

Baseline numbers, the measurement environment, and reproduction commands live in [docs/PERFORMANCE.md](docs/PERFORMANCE.md).

---

## 🔒 Security

- **Vulnerability reporting**: do not open public issues; use GitHub's private [Security Advisories](https://github.com/Kirky-X/trait-kit/security/advisories/new) channel ("Report a vulnerability"). The maintainer commits to acknowledging reports within 48 hours and providing an initial assessment within 7 days (see [SECURITY.md](docs/SECURITY.md)).
- **No unsafe**: `#![deny(unsafe_code)]` is enforced crate-wide.
- **Compile-time misuse prevention**: typestate turns "require before build" into a compile error; the dependency graph is checked for missing deps and cycles at `build()`.
- **Explicit thread-safety boundary**: the sync `Kit` is `!Sync` (compiler-enforced, see the `static_assertions` assertions); use `AsyncKit` (`Send + Sync`) for multi-threading.
- **Encrypted config storage** (`encryption`): XChaCha20-Poly1305 AEAD with field keys derived via HKDF from the master key and `ModuleConfig::PATH`; `EncryptedBlob`'s `Debug` implementation never leaks encrypted material.
- **Supply-chain gates**: `cargo deny check` (`deny.toml`: advisories / licenses / bans / sources) + `cargo audit` are mandatory CI gates; CodeQL static analysis runs continuously; the lefthook private-key scan blocks credentials from entering the repo.

For the complete security design, best practices, and fix history, see [docs/SECURITY.md](docs/SECURITY.md).

---

## 🗺️ Roadmap

Entries carry over the existing plan; status is aligned with the current repository state:

<div align="center">

<table>
<tr><th>Status</th><th>Item</th><th>Description</th></tr>
<tr><td>✅</td><td><b>0.5.0-rc.2</b> (2026-09-03)</td><td>Docs & Kit API table sync; workspace dependency path localization (<code>path</code> + <code>version</code> dual specification).</td></tr>
<tr><td>✅</td><td><b>Performance benchmarks</b></td><td>Criterion benchmarks in <code>benches/kit_bench.rs</code> and the <code>docs/PERFORMANCE.md</code> baseline report are in place (baseline 2026-09-10).</td></tr>
<tr><td>📋</td><td><b>0.5.0 stable release</b></td><td>After the minor version bump, sync the <code>path + version</code> dependency requirements of downstream crates (oxcache, dbnexus, inklog, limiteron, sdforge) per the workspace release plan.</td></tr>
<tr><td>📋</td><td><b>cfg gate completeness</b></td><td>Add missing <code>observer</code> cfg gates for the <code>--no-default-features --features async</code> combination (known low-priority item).</td></tr>
</table>

</div>

---

## 🤝 Contributing

Contributions are welcome! For full environment setup, the TDD workflow, and commit conventions, see the [Contributing Guide](docs/CONTRIBUTING.md).

### Build Requirements

- Rust **1.97.1** or later (stable, see `rust-toolchain.toml`; edition 2024).
- No external tooling required (no protoc, no openssl, no system libraries; only the workspace member `examples` may need `protobuf-compiler` for local confers integration).

### Commit Convention and Local Gates

- Commit messages follow **conventional commits** (`feat(scope): ...`, `fix(scope): ...`, `docs: ...`, etc.), enforced by the lefthook `commit-msg` hook.
- **lefthook** hook gates (`lefthook.yml`):
  - `pre-commit`: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo deny check`, private-key content scan;
  - `pre-push`: `cargo audit`, coverage ≥ 80% lines.
- Never bypass the hooks with `--no-verify`.

### Development Commands

Common commands matching CI (full tests, lint, zero-warning docs, dependency audit, coverage gate) are listed in the [Testing](#-testing) section and the [Contributing Guide](docs/CONTRIBUTING.md).

### Pull Request Process

1. Create a feature branch from `main` (`feat/<name>` / `fix/<name>`); committing directly to main is not allowed.
2. Ensure all tests pass and Clippy is clean; add tests for new functionality.
3. Keep the README in sync with API changes (`docs/API_REFERENCE.md`, `docs/USER_GUIDE.md`).
4. Wait for review after CI (fmt / clippy / check / test / three-platform builds / doc / security / coverage) passes.

This project follows the [Rust Code of Conduct](https://www.rust-lang.org/policies/code-of-conduct).

---

## 📋 Changelog

See [CHANGELOG.md](docs/CHANGELOG.md). Recent highlights:

- **0.5.0-rc.2** (2026-09-03): docs version and Kit API table sync; `confers` dependency path localization (`path` + `version` dual specification).
- **0.4.2** (2026-08-06): fixed the `AsyncKit::decorate()` storage-key bug (decorators previously never applied).
- **0.4.1** (2026-08-06): i18n enhancements (`tr()` / `I18nManager` no longer require the `i18n` feature); `EncryptedBlob` Debug no longer leaks encrypted material; new config extension API docs and examples.

---

## 📄 License

This project is licensed under **MIT + Commons Clause**: the MIT license with the additional Commons Clause v1.0 condition, which excludes selling the Software without separate written authorization from the Licensor; it is a source-available license, and commercial use requires separate authorization. See [LICENSE](LICENSE).

Copyright (c) 2026 Kirky.X🌠

---

## 🙏 Acknowledgments

- [`confers`](https://crates.io/crates/confers) — the underlying config loading, hot-reload, and encrypted storage capabilities.
- [ICU4X](https://github.com/unicode-org/icu4x) — internationalization formatting (number/date/plural/collation).
- [Project Fluent](https://projectfluent.org/) — the Fluent FTL message localization approach.
- [syn](https://github.com/dtolnay/syn) / [quote](https://github.com/dtolnay/quote) / [proc-macro2](https://github.com/dtolnay/proc-macro2) — the proc-macro foundation (for `trait-kit-derive` and `trait-kit-macros`).
- [The Rust Community](https://www.rust-lang.org/community) — for the excellent language ecosystem and tooling.

---

## 📞 Contact & Support

- **Bugs & feature requests**: [GitHub Issues](https://github.com/Kirky-X/trait-kit/issues)
- **Security vulnerabilities**: do not report via public issues; see the vulnerability reporting process in the [Security document](docs/SECURITY.md).
- **Maintainer**: Kirky.X

---

## ⭐ Star History

[![Star History Chart](https://api.star-history.com/svg?repos=Kirky-X/trait-kit&type=Date)](https://star-history.com/#Kirky-X/trait-kit&Date)

### 💝 Support the Project

If you find this project useful, please consider giving it a ⭐️!
