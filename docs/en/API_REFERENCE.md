# 📘 Trait-Kit API Reference

API reference for trait-kit. Covers the core and commonly used public items — not an exhaustive inventory; the full list lives in rustdoc / docs.rs. Organized by module, with each API annotated with the feature flag it requires (current version **0.5.0**).

## 📋 Table of Contents

<details open>
<summary>📑 Table of Contents</summary>

- [📖 Overview](#-overview)
- [🔩 Core API](#-core-api)
- [⚙️ Configuration and Extension APIs (confers)](#️-configuration-and-extension-apis-confers)
- [🚪 Error Types](#-error-types)
- [🎛️ Feature-Gated APIs](#️-feature-gated-apis)
- [💻 Usage Examples](#-usage-examples)
- [✅ Best Practices](#-best-practices)

</details>

---

## 📖 Overview

- **Organization**: structured along the hierarchy "core API → configuration/extension API → error types → feature-gated API", mirroring the crate's feature layering.
- **Feature annotations**: an inline-code annotation after a heading (e.g. `async`, `confers`) means the API requires the corresponding feature to be enabled in `Cargo.toml`; APIs without an annotation are available under the default features.
- **Authoritative source**: this is a hand-curated quick-reference document; the most precise signatures and documentation are the auto-generated docs on [docs.rs](https://docs.rs/trait-kit).

---

## 🔩 Core API

### `ModuleMeta`

Module identity and dependency declaration. Every module must implement it.

```rust
pub trait ModuleMeta: 'static {
    const NAME: &'static str;
    fn dependencies() -> &'static [(&'static str, TypeId)] { &[] }

    // Version negotiation (checked by the version-negotiation feature during build(); the declaration itself is ungated)
    const VERSION: &'static str = "0.0.0";
    fn required_versions() -> &'static [(&'static str, &'static str)] { &[] }

    // FTL translation fragments bundled with the module (requires the i18n feature)
    #[cfg(feature = "i18n")]
    fn i18n_ftl() -> &'static [(&'static str, &'static str)] { &[] }
}
```

| Member | Description |
|---|---|
| `NAME` | Diagnostic module name, used in error messages and logs |
| `dependencies()` | Returns `(name, TypeId)` pairs of dependency modules. Defaults to an empty slice |
| `VERSION` | Module capability version; defaults to `"0.0.0"`, meaning undeclared; under the `version-negotiation` feature it participates in the semver compatibility check during `build()` |
| `required_versions()` | List of `(name, min_version)` minimum versions required from dependency modules; defaults to empty |
| `i18n_ftl()` `i18n` | `(locale, ftl_source)` translation fragments bundled with the module, merged at `build()` time into the kit's local translation overlay |

### `version-negotiation` — Version Negotiation

During `build()`, the semver compatibility between `VERSION` declarations and dependents' `required_versions()` is validated (the declaration itself is not feature-gated; see `VERSION` / `required_versions()` in the table above). When a dependency module is missing (no registered provider), negotiation is skipped and the dependency graph reports the missing-dependency error directly. The old feature name `negotiate` is kept as a compatibility alias (forwarding to `version-negotiation`, already marked deprecated).

### `AutoBuilder`

Synchronous module build trait.

```rust
pub trait AutoBuilder: ModuleMeta {
    type Capability: Clone + 'static;
    type Error: std::error::Error + Send + 'static;
    fn build(kit: &Kit) -> Result<Self::Capability, Self::Error>;
}
```

### Kit API

#### `Kit<Unbuilt>` — Build Phase

| Method | Feature | Description |
|---|---|---|
| `Kit::new()` | — | Creates an empty Kit |
| `register::<M>()` | — | Registers a module (built immediately) |
| `register_lazy::<M>()` | — | Registers a module (built lazily, triggered on first require) |
| `register_multi::<M>()` | — | Multi-binding registration (same type aggregated into a Vec) |
| `register_as::<M>()` | `di` | Registers by interface type (`dyn Trait` type erasure) |
| `register_if::<M>(pred)` | — | Conditional registration (runtime predicate) |
| `register_lifecycle::<M>()` | `lifecycle` | Registers lifecycle hooks |
| `register_health_check::<M>()` | `health` | Registers a health check |
| `with_observer(obs)` | `observer` | Attaches a build observer |
| `decorate::<M>(f)` | `decorator` | Registers a capability decorator (panics at `build()` time if the target is not registered; decorators are isolated per owner and only apply to the decorated module) |
| `try_decorate::<M>(f)` | `decorator` | Decorator registration (returns `DecoratorTargetMissing` when the target is not registered) |
| `override_module::<M>(cap)` | — | Overrides a module capability (test injection); at injection time validates that all `M::dependencies` are registered, returning `DependencyMissing` otherwise |
| `set_config::<C>(value)` | — | Stores a typed config |
| `set_config_arc::<C>(value)` | — | Stores a config and shares it as an `Arc` snapshot |
| `load_config::<C>()` | `confers` | Loads config via `Configurable::load()` |
| `load_and_validate::<C>()` | `confers` | Loads and validates config; on failure nothing is stored |
| `load_config_or_default::<C>()` | `confers` | Falls back to `default_value()` when loading fails (returns whether loading succeeded) |
| `load_config_with::<C, S>(vars)` | `confers` | Loads config and performs `${VAR}` variable substitution (returns an error when the nesting depth exceeds the limit instead of terminating the process) |
| `snapshot_config::<C>()` | `confers` | Snapshots the current config (returns whether it succeeded) |
| `restore_config::<C>()` | `confers` | Rolls the config back to the most recent snapshot |
| `has_snapshot::<C>()` | `confers` | Checks whether a snapshot exists for the given type |
| `populate_defaults::<C>()` | `confers` | Fills `ModuleConfig::default_value()` when the Kit is empty (`impl<S>`, usable in both states) |
| `merge_config::<C>(ovr)` | `confers` | Usable in any state (`impl<S>`, callable both at build time and at runtime); applies `ConfigInherit` field overrides; every call is recorded into `build_report().config_overrides` (`report` feature, `applied=false` = discarded because the target config does not exist; the history is bounded — the most recent `MAX_CONFIG_OVERRIDE_HISTORY` entries (1024) are kept, older ones are dropped and counted by `config_overrides_dropped()`; long-lived Kits with high-frequency calls can still drain and retrieve the records at rotation points via `take_config_overrides()`) |
| `extract_shared::<C>()` | `confers` | Extracts shared fields from the config into the overlay (`impl<S>`, usable in both states) |
| `inject_shared::<C>()` | `confers` | Injects shared fields from the overlay into the config (`impl<S>`, usable in both states) |
| `enable_toggle(key, bool)` | `toggle` | Sets a feature flag |
| `is_toggle_enabled(key)` | `toggle` | Queries flag state |
| `set_toggle` / `get_toggle` / `remove_toggle` / `list_toggles` | `toggle` | Typed toggle handles (`ToggleValue`: Bool/Int/Float/Str) |
| `register_if_toggle::<M>(key)` | `toggle` | Registers a module conditionally on a toggle |
| `register_with_visibility::<M>(vis)` | `visibility` | Registers a module and declares its capability visibility (`CapabilityVisibility::Public` / `Restricted(patterns)`; a pattern ending in `*` matches by prefix, otherwise exact `ModuleMeta::NAME`) |
| `set_visibility::<M>(vis)` | `visibility` | Sets/overwrites the visibility of an already registered module (companion to `register_lazy` / `register_multi`; recording a visibility for an unregistered module is a silent no-op) |
| `subscribe::<C>(cb)` | `reload` | Subscribes a config hot-reload callback |
| `reload_config::<C>()` | `reload` | Reloads config and notifies subscribers |
| `set_encrypted::<C>(val, key)` | `encryption` | Stores config encrypted (XChaCha20-Poly1305) |
| `set_encrypted_with_version::<C>(val, key, version)` | `encryption` | Encrypted storage with a key version (for rotation) |
| `set_encrypted_with_key_provider::<C, P>(val, provider)` | `encryption` | Fetches the key via `KeyProvider` at call time (fail-closed) |
| `contains_encrypted::<C>()` | `encryption` | Checks whether an encrypted config is registered (defined on `impl Kit<Unbuilt>`, only callable before build; not available on `Kit<Ready>`) |
| `with_metrics_port(port)` | — | Injects a `MetricsPort` observability port |
| `with_log_port(port)` | — | Injects a `LogPort` observability port |
| `with_event_bus(bus)` | — | Injects an `EventBus` event bus (`impl<S>`, usable in both states) |
| `build()` | — | Validates the dependency graph → topological sort → build → `Kit<Ready>` |

#### `Kit<Ready>` — Runtime Phase

> `require` / `get_arc` / `require_all` / `resolve` / `config` / `config_arc` / `subscribe` / `reload_config` / `merge_config` / `extract_shared` / `inject_shared` / `with_event_bus` / `emit_event` are defined on `impl<S> Kit<S>`, so they can also be compiled and called in the `Kit<Unbuilt>` state (letting `AutoBuilder::build` callbacks read dependencies and configs; among them `extract_shared` / `inject_shared` require `confers`, and `subscribe` / `reload_config` require `reload`); `require_ref` / `contains` / `factory` are defined on `impl Kit<Ready>`, and calling them in the `Kit<Unbuilt>` state is a compile error. The table below lists them by their primary use case. Misuses excluded at compile time include: `optional()` on `Kit<Unbuilt>` (`unbuilt_cannot_optional`) and the register/build methods on `Kit<Ready>` (`ready_cannot_register` / `ready_cannot_build`, asserted by UI tests).

| Method | Feature | Description |
|---|---|---|
| `require::<M>()` | — | Retrieves a capability (Clone, errors when missing; the first call for a `register_lazy` module triggers the build and caches it) |
| `require_for::<M>(caller)` | `visibility` | Caller-identified retrieval (defined on `impl Kit<Ready>`); returns `AccessDenied` when `M` is `Restricted` and the caller matches no allow-list pattern; caller-less `require` keeps container-holder semantics |
| `require_as::<M, Caller>()` | `visibility` | Type-driven variant of `require_for`: caller identity comes from `Caller::NAME` (no string typos) |
| `require_guarded::<M>(caller, guard)` | `guard` | Conditional retrieval: the runtime `CapabilityGuard` and the static visibility boundary must both pass; either denial yields the same `AccessDenied` (evaluation order: visibility first, guard second) |
| `visibility_of::<M>()` | `visibility` | Read-only visibility query (`None` = undeclared, behaves as `Public`); consumed by the security-posture audit |
| `require_ref::<M>()` | — | Zero-copy retrieval (returns `Ref<'_, Cap>`; a lazy module can be borrowed once built; while the guard is alive, do not trigger another lazy module's first build — it panics) |
| `get_arc::<M, T>()` | — | Clone-free `Arc` capability retrieval (returns `Arc<T>`, same pointer; lazy-cache aware) |
| `optional::<M>()` | — | Optional retrieval (returns `Option`; lazy-cache aware) |
| `require_all::<M>()` | — | Retrieves all multi-binding capabilities |
| `resolve::<I>()` | `di` | Retrieves `Arc<I>` by interface type |
| `contains::<M>()` | — | Checks whether a capability is built (lazy-cache aware) |
| `contains_config::<C>()` | — | Checks whether a config exists |
| `config::<C>()` | — | Retrieves a config (Clone) |
| `config_arc::<C>()` | — | Retrieves a config as an `Arc` snapshot |
| `factory::<M>()` | — | Creates a factory closure producing a new instance per call |
| `create_scope()` | `request-scope` | Creates an empty `Scope` (independent of Kit capabilities) |
| `create_scope_from(self)` | `request-scope` | Creates a `Scope` with parent context from a held `Rc<Kit>` (`self: &Rc<Self>`), without consuming the Kit (the parent side is an `Rc::downgrade` weak reference, preventing retention cycles; `scope.parent::<T>()` is a read-only query) |
| `health_check::<M>()` | `health` | Queries a single module's health status |
| `health_report()` | `health` | Queries the health report of all modules |
| `health_aggregate()` | `health` + `report` | worst-of overall status + per-module detail (modules order is not guaranteed deterministic; every call runs all checkers and publishes HealthChanged when an event bus is injected) |
| `health_json()` | `health` + `report` | Health aggregate JSON export (for /healthz consumption) |
| `record_health_history()` / `health_history()` | `health` | Ring-buffer health sampling and reading |
| `shutdown()` | `lifecycle` | Runs `on_shutdown` in reverse topological order |
| `subscribe::<C>(cb)` | `reload` | Subscribes to hot reload |
| `reload_config::<C>()` | `reload` | Reloads config (usable in the Ready state; `set_config` is only available on `Kit<Unbuilt>` — for runtime config writes there are also the `confers`-side methods such as `merge_config` / `populate_defaults`) |
| `enable_toggle(key, bool)` / `is_toggle_enabled(key)` | `toggle` | Modifies/queries feature flags at runtime |
| `set_toggle` / `get_toggle` / `list_toggles` | `toggle` | Typed toggle handles (`remove_toggle` is provided only on `Kit<Unbuilt>` (`impl Kit`), outside this table's method surface — see the table above) |
| `get_encrypted::<C>(key)` | `encryption` | Decrypts and retrieves a config |
| `get_encrypted_with_version::<C>(key, expected_version)` | `encryption` | Decrypts and retrieves by key version |
| `rotate_master_key::<C>(old, new)` | `encryption` | Rotates the master key and migrates old ciphertexts (fail-closed on a wrong key, version increments) |
| `encrypted_key_version::<C>()` | `encryption` | Queries the ciphertext's current key version |
| `module_tr(message_id)` | `i18n` | Translates via the kit's local translation overlay (module `i18n_ftl` first, global `tr()` as fallback) |
| `graph_dot()` / `graph_mermaid()` | — | Dependency graph text export |
| `module_count()` | — | Number of registered modules |
| `build_report()` | `report` | Structured build report (JSON) |
| `contract_manifest()` | `report` | Contract manifest export |
| `take_config_overrides()` | `report` | Usable in any state (`impl<S>`, same impl block as `merge_config`; listed in this table only because drain rotation points usually fall at runtime); drains and returns the `config_overrides` history (in record order) — the history is already bounded (1024-entry ring semantics); draining retrieves the records at a rotation point for further processing and also prevents, once at capacity, the recurring costs of `merge_config`'s O(n) front deletion and `build_report()`'s whole-history clone on every snapshot |
| `emit_event(event)` | — | Publishes a custom `KitEvent` (`impl<S>`, usable in both states) |

### Declarative Macros

#### `impl_module_meta!`

Generates a `ModuleMeta` implementation.

```rust
// No dependencies
impl_module_meta!(MyModule, "my-module");

// With dependencies
impl_module_meta!(MyModule, "my-module", deps = [DepA, DepB]);
```

#### `impl_auto_builder!`

Generates an `AutoBuilder` implementation.

```rust
impl_auto_builder!(MyModule, Arc<Cap>, MyError, |kit| Ok(Arc::new(Cap { ... })));
```

#### `impl_async_auto_builder!` `async`

Generates an `AsyncAutoBuilder` implementation.

```rust
impl_async_auto_builder!(MyModule, Arc<Cap>, MyError, |kit| Box::pin(async move {
    Ok(Arc::new(Cap { ... }))
}));
```

### Derive Macros (the `trait-kit-macros` crate)

The three derives are provided by the separate crate `trait-kit-macros`; they are not exported through the prelude and require an explicit `trait-kit-macros` dependency in `Cargo.toml`:

| derive | Helper attribute | Description |
|---|---|---|
| `#[derive(Module)]` | `#[module(...)]` | Generates a `ModuleMeta` implementation for a struct (equivalent to `impl_module_meta!`): `name = "literal"` overrides the diagnostic name (defaults to the struct identifier), `deps(TypeA, TypeB)` or `deps = [TypeA, TypeB]` declares dependencies; generic structs are not supported |
| `#[derive(ConfigInherit)]` | `#[config_inherit(nested)]` | Compile-time-safe field-level override type + `ConfigInherit` implementation |
| `#[derive(SharedConfig)]` | `#[shared(field1, field2)]` | Auto-generates `extract_shared` / `inject_shared` (cross-type shared fields) |

```rust,ignore
use trait_kit_macros::Module;

#[derive(Module)]
#[module(name = "my-module", deps(DepA, DepB))]
struct MyModule;
```

### Dependency Graph Exports

| API | Description |
| --- | --- |
| `DependencyGraph` | Dependency graph container (Kahn topological sort + DFS cycle detection) |
| `GraphError` / `ModuleEntry` | Graph validation errors and module graph nodes |

Method-level exports: `graph_dot()` / `graph_mermaid()` / `module_count()` carry no feature gate, while `build_report()` requires the `report` feature — see the `Kit<Ready>` table under [Kit API](#kit-api); not repeated here.

### Event Bus and Observability Ports

Available by default (no feature gate); the default `NoOp` implementations are zero-cost:

| API | Description |
|---|---|
| `KitEvent` | Lifecycle event enum (`ModuleBuilt` / `HealthChanged` / `ConfigChanged`, etc.) |
| `EventBus` trait + `MemoryEventBus` / `NoOpEventBus` | Event bus (`MemoryEventBus` includes subscriber panic isolation), injected via `with_event_bus` |
| `MetricsPort` trait (counter/gauge/histogram) | Metrics observability port, injected via `with_metrics_port` |
| `LogPort` trait | Structured logging observability port, injected via `with_log_port` |
| `KeyProvider` trait + `KeyBytes` `encryption` | Key provider abstraction; `KeyBytes` is an alias of confers `SecretBytes` (constructed with `new(Vec<u8>)`, read with `as_slice()`), Drop zeroes the entire capacity (including spare capacity), Debug always prints `[REDACTED]`, and it is not Cloneable |

### Internationalization

`tr()` and `I18nManager` are available under the default features (lightweight Fluent FTL translation); `I18nFormatter`'s ICU4X locale-aware formatting requires the `i18n` feature.

#### `I18nManager`

```rust
let mgr = I18nManager::init();                      // English by default (strict; never drifts with the host system language)
let mgr = I18nManager::init_with_locale("zh-CN");   // explicit locale (an invalid locale falls back to the English catalog)
let mgr = I18nManager::init_with_system_locale();   // negotiate from the system language (opt-in, requires the i18n feature)
```

#### `tr()` — Message Translation

```rust
use trait_kit::i18n::tr;
let msg = tr("trait-kit-error-cycle-detected", &[("cycle", "A → B → A")]);
```

#### `I18nFormatter` — Locale-Aware Formatting `i18n`

```rust
use trait_kit::i18n::I18nFormatter;
let fmt = I18nFormatter::new("zh-CN")?;
let text: String = fmt.format_number(1234567.89)?;  // grouping separators follow the locale
fmt.format_date(2026, 9, 10)?;
```

### Prelude

`use trait_kit::prelude::*` exports the most frequently used types:

| Type | Feature |
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

> The derive macros (`#[derive(Module)]` / `#[derive(ConfigInherit)]` / `#[derive(SharedConfig)]`) are not exported through the prelude; add an explicit `trait-kit-macros` dependency in `Cargo.toml` — see [Derive Macros](#derive-macros-the-trait-kit-macros-crate).

---

## ⚙️ Configuration and Extension APIs (confers)

All APIs below require the `confers` feature (`reload` / `encryption` are enabled automatically by inheritance).

### `Validatable` `confers`

Config validation trait; implement it, and `Kit::load_and_validate` checks automatically after loading.

```rust
pub trait Validatable: Clone + 'static {
    fn validate(&self) -> Result<(), Vec<String>>;
}
```

### `interpolate_json_value` `confers`

Recursively replaces `${VAR}` and `${VAR:-default}` patterns in a JSON value.

```rust
pub fn interpolate_json_value<S: BuildHasher>(
    value: &mut serde_json::Value,
    vars: &HashMap<String, String, S>,
)
```

### Configuration Inheritance System `confers`

A four-layer configuration inheritance system for smooth cross-module / cross-project config inheritance:

| Layer | Mechanism | API | Description |
|---|---|---|---|
| Layer 1 | Deep merge | `merge_json_deep` | Recursively merges JSON Objects; non-Object values are replaced |
| Layer 2 | Field override | `ConfigInherit` trait | Compile-time safe; `Option<T>` fields override only when `Some` |
| Layer 3 | Shared fields | `SharedConfig` trait | `serde_json::Value` overlay inherited across types |
| Layer 4 | Zero config | `populate_defaults` | Fills `ModuleConfig::default_value()` automatically for an empty Kit |

#### `ConfigInherit` trait

```rust
pub trait ConfigInherit: Clone + 'static {
    type Override: Clone + Default + 'static;
    fn apply_override(&mut self, ovr: &Self::Override);
}
```

- `Kit::merge_config::<C>(ovr)` — applies field overrides; the override facts (including `applied=false` drop records when the target config does not exist) enter `build_report().config_overrides` in record order (`report` feature, same on `AsyncKit`; `AsyncKit` is a shared `Send + Sync` type, so under concurrent calls the record order is the lock-serialized arrival order and does not represent the actual application order, and read-modify-write is not atomic — concurrent `merge_config` on the same type can drop each other's overrides; callers that need exclusive-write semantics must serialize themselves). The record history is bounded (the most recent `MAX_CONFIG_OVERRIDE_HISTORY` entries are kept, older ones are dropped and counted by `config_overrides_dropped()`); in high-frequency scenarios, `take_config_overrides()` (`report` feature) can still drain at rotation points so processing can continue
- `#[derive(ConfigInherit)]` — auto-generates the Override type (`trait-kit-macros`)
- `#[config_inherit(nested)]` — recursive delegation for nested fields

#### `SharedConfig` trait

```rust
pub trait SharedConfig: Clone + 'static {
    fn extract_shared(&self) -> serde_json::Map<String, serde_json::Value>;
    fn inject_shared(&mut self, shared: &serde_json::Map<String, serde_json::Value>);
}
```

- `Kit::extract_shared::<C>()` — extracts shared fields from the config into the overlay
- `Kit::inject_shared::<C>()` — injects shared fields from the overlay into the config
- `#[derive(SharedConfig)]` + `#[shared(field1, field2)]` — auto-generates the implementation

The shared namespace carries an owner dimension: the overlay records which config type first declared each key — **the first extractor owns the key** (ownership contract). When two config types declare a same-named field (common names such as `host`, `port`, `token` are frequent), the later `extract_shared` caller's same-named key is rejected and a `log::warn!` is emitted (two `host` fields are not necessarily the same host); the previously written value is never silently overwritten; the owner's own re-extraction (on config reload) still updates the value. Cross-type flow stays one-directional: values enter the overlay via `extract_shared` and only reach other config types that declare the same-named field via `inject_shared`. Values in the overlay are stored as plaintext `serde_json::Value` and bypass the zeroization path. Credential-like field names (`password`, `api_key`, etc.) are filtered out by the Kit on both the write side and the inject side, so plaintext credentials never flow across config types; even though ownership blocks same-name conflicts, credential fields marked shared would still flow to other types via injection — mark fields shared only when they genuinely need cross-type flow.

#### `populate_defaults`

```rust
impl<S> Kit<S> {
    pub fn populate_defaults<C: ModuleConfig>(&self) -> bool;
}
```

Fills `C::default_value()` when the Kit is empty; existing values are not overwritten. Returns `true` when a default was filled. Defined on `impl<S> Kit<S>`, usable in both the `Kit<Unbuilt>` and `Kit<Ready>` states.

---

## 🚪 Error Types

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
    AccessDenied { module: &'static str, caller: &'static str },                 // visibility
}
```

The `Display` implementation localizes output automatically via `tr()`.

Redaction rules for outward-facing text: the `context` of `BuildFailed` / `LifecycleFailed` is collapsed into an outward category label before entering the message template (`load_config` → `configuration load`, `set_encrypted` → `encrypted configuration write`); category labels are localized through the Fluent catalog (`trait-kit-context-*`), while non-label forms such as module names are kept as-is; the underlying error text (credential fragments, absolute server paths) is redacted on both the `Display` and `Debug` sides; config type names appear with the crate / module paths trimmed (`my_crate::config::DbConfig` → `DbConfig`; generic arguments are trimmed as well but container names are kept, `core::option::Option<crate::Db>` → `Option`). The original `context` and the unredacted text remain retrievable through the variants' public fields and `Error::source()`.

### `TraitKitResult<T>`

```rust
pub type TraitKitResult<T> = Result<T, TraitKitError>;
```

---

## 🎛️ Feature-Gated APIs

### `visibility` — Capability Visibility

Restricts which caller modules may require a capability. Soft in-process boundary: prevents misuse and unauthorized retrieval by code holding a `Kit<Ready>` reference (e.g. untrusted plugins); not a defense against adversarial memory access.

```rust
pub enum CapabilityVisibility {
    Public,
    Restricted(&'static [&'static str]),
}
```

- Patterns match against `ModuleMeta::NAME`: trailing `*` = prefix match, otherwise exact.
- `register_with_visibility::<M>(vis)` registers + declares; `set_visibility::<M>(vis)` patches afterwards.
- Enforcement applies only to caller-identified retrieval (`require_for` / `require_as`); caller-less `require` acts as the container-level holder.
- Denials return `TraitKitError::AccessDenied { module, caller }` (module existence is not revealed via `ModuleNotFound`).

### `guard` — Conditional Retrieval Guard

Runtime policy hook on the require path (implies `visibility`). The trait is defined here and implemented by ecosystem crates (limiteron: tenant not rate-limited; garrison: session authenticated) — this crate never depends on them.

```rust
pub trait CapabilityGuard: Send + Sync {
    fn allow(&self, caller: &'static str, target: &'static str) -> bool;
}
```

- `require_guarded::<M>(caller, &guard)`: visibility (static boundary) is checked first, then `guard.allow(caller, M::NAME)`; either denial returns `AccessDenied` — callers cannot distinguish boundary from policy denials, so error differences cannot probe policy state.

### `async` — Asynchronous Module Building

#### `AsyncAutoBuilder` `async`

```rust
pub trait AsyncAutoBuilder: ModuleMeta {
    type Capability: Clone + Send + Sync + 'static;
    type Error: std::error::Error + Send + 'static;
    fn build<'a>(kit: &'a AsyncKit)
        -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>;
}
```

`AsyncKit` mirrors the synchronous `Kit` API (registration / build / retrieval / config / encryption / toggles are all available) and additionally provides:

| API | Description |
|---|---|
| `with_max_concurrency(limit)` | Concurrency cap for topology-layered concurrent builds |
| `create_scope()` | Creates an async scope |
| Config surface (`AsyncKit<Unbuilt>`) | The `load_config` family, `subscribe` / `reload_config`, `snapshot_config` / `restore_config`, `set_encrypted` / `get_encrypted` match the synchronous `Kit`'s config capabilities; these methods are defined on `AsyncKit<Unbuilt>` (the pre-build config surface), while `AsyncKit<Ready>` keeps only read-only retrieval (`config` / `contains_config`; `config_arc` is defined only on `AsyncKit<Unbuilt>` and is not available in the Ready state) |
| `health_aggregate()` | `health` + `report` | worst-of overall status + per-module detail (`/healthz` structured payload), consistent with the synchronous `Kit<Ready>` (empty set healthy-by-convention; modules order not guaranteed deterministic; every call runs all checkers and publishes HealthChanged when an event bus is injected) |
| `health_json()` | `health` + `report` | Health aggregate JSON export, the counterpart of the synchronous `Kit<Ready>` |
| `build_report()` `report` | Structured build report (JSON), the counterpart of the synchronous `Kit<Ready>`; the async build state set is only `built` (no override/lazy surface) |
| `contract_manifest()` `report` | Contract manifest export (NAME/VERSION/capability/deps), the counterpart of the synchronous `Kit<Ready>` |
| `take_config_overrides()` `report` | Drains and returns the `config_overrides` history (a pre-build `AsyncKit<Unbuilt>` API, same surface as `set_config` — accumulation happens only before build; after Ready the history is frozen, with no runtime accumulation; the drain semantics (swap out and return in record order) match the synchronous side) |
| `populate_defaults::<C>()` | Pre-build `AsyncKit<Unbuilt>` config-surface entry (same surface as `merge_config` / `take_config_overrides` in the same impl block; not available in the Ready state — the async config surface is a build-time concern): fills `C::default_value()` when the Kit is empty, never overwrites existing values, and returns a `bool` indicating whether a fill happened. The `AsyncKit` version's bound is `C: ModuleConfig + Send + Sync` (the synchronous `Kit` requires only `C: ModuleConfig`) — the difference stems from `AsyncKit` being a shared `Send + Sync` type |

### `di` — Interface/Implementation Separation

The old feature name `interface` is kept as a compatibility alias (forwarding to `di`, already marked deprecated).

#### `Interface` `di`

Interface marker trait. Implemented automatically by all `'static` types (including `?Sized`).

#### `InterfaceBuilder` `di`

Interface/implementation separation extension trait.

```rust
pub trait InterfaceBuilder: ModuleMeta {
    type Interface: ?Sized + 'static;
    type Capability: Clone + 'static;
    type Error: std::error::Error + Send + 'static;
    fn build(kit: &Kit) -> Result<Self::Capability, Self::Error>;
    fn into_interface(cap: Self::Capability) -> Arc<Self::Interface>;
}
```

### `lifecycle` — Lifecycle

#### `Lifecycle` `lifecycle`

```rust
pub trait Lifecycle: AutoBuilder {
    fn on_ready(kit: &Kit<Ready>) -> Result<(), Self::Error> { Ok(()) }
    fn on_shutdown(cap: &Self::Capability) {}  // no-op by default
}
```

Both methods have default (no-op) implementations; override as needed.

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

Both methods have default (no-op) implementations; override as needed.

### `health` — Health Checks

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

#### `HealthAggregate` / `HealthModuleEntry` `health` + `report`

```rust
pub struct HealthAggregate {
    pub schema_version: u32,              // payload schema version (HEALTH_SCHEMA_VERSION = 1)
    pub status: &'static str,             // worst-of overall status name
    pub healthy: bool,                    // convenience flag: status == "healthy"
    pub modules: Vec<HealthModuleEntry>,  // per-module detail
}

pub struct HealthModuleEntry {
    pub module: &'static str,             // module name (ModuleMeta::NAME)
    pub status: &'static str,             // flat status name: healthy / degraded / unhealthy
    pub detail: Option<String>,           // detail for degraded / unhealthy; None when absent
}
```

Both exist only when `health` + `report` are enabled together, and derive `serde::Serialize`; `HealthAggregate::to_json()` is exactly the `/healthz` payload. The overall status is the worst-of all registered checkers (`unhealthy` > `degraded` > `healthy`), and the empty set is healthy by convention; the `modules` order follows the checker registry's iteration order and is **not guaranteed deterministic**. `detail` is redacted uniformly by the framework before entering the outward JSON (`core::health::redact_detail`: URL userinfo, `password=`/`token=`-style key-value pairs (also covered when the separators carry surrounding spaces), credentials following `Bearer` / `Basic`-style auth schemes, and private key material from `-----BEGIN` through `-----END` inside PEM blocks are replaced with `[REDACTED]`, and overly long text is truncated) — checker implementers still must not write credentials, connection strings, or intranet topology into it.

### `probe` — Service Probe Registry

`probe = ["health"]`. Division of labor with `HealthCheck`/`AsyncHealthCheck`: `check` is deliberately synchronous, reads cached state, and runs on the reporting path; `ServiceProbe` performs the real asynchronous network probing and executes only on explicit invocation (latency is measured by the framework around `probe().await`; see `ProbeEntry::latency_ms`). It is an object-safe trait with hand-written `Pin<Box<dyn Future>>` dispatch (aligned with `AsyncLifecycle`, without pulling in `async-trait`).

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

#### Registry Surface (`AsyncKit`, `probe` + `async`, usable in any state)

```rust
impl<S> AsyncKit<S> {
    pub fn register_probe(&self, name: &'static str, probe: Arc<dyn ServiceProbe>);
    pub fn unregister_probe(&self, name: &str) -> bool;
    pub fn probe_names(&self) -> Vec<&'static str>;   // registration order = execution order; re-registering an existing name moves it to the tail
}
```

#### Execution Surface (`AsyncKit<Ready>`, `probe` + `async`)

```rust
impl AsyncKit<Ready> {
    // Runs all probes sequentially; worst-of aggregation
    // (unhealthy > degraded > healthy); an empty registry is
    // healthy-by-convention (except after shutdown cleared it, see stopped).
    // Under probe+report, ProbeReport supports to_json().
    pub async fn run_probes(&self) -> ProbeReport;
    // Returns only the worst-of verdict; every probe still runs (no short-circuit).
    pub async fn probe_aggregate(&self) -> HealthStatus;
    // Hard per-probe deadline variant: a hanging probe is recorded as
    // unhealthy (detail includes "timed out") and the remaining probes keep
    // running; worst-case total latency <= probe count x timeout. The probe
    // future is dropped at the deadline (must be cancel-safe).
    pub async fn run_probes_with_timeout(&self, timeout: Duration) -> ProbeReport;
    pub async fn probe_aggregate_with_timeout(&self, timeout: Duration) -> HealthStatus;
}
```

`ProbeReport { schema_version, overall, healthy, probes: Vec<ProbeEntry { name, status, detail, latency_ms }>, stopped }` derives `Serialize` and provides `to_json()` under the `report` feature; `schema_version` is the payload schema version (`PROBE_SCHEMA_VERSION = 1`). `latency_ms` is the wall-clock latency measured by the framework around `probe().await` (the implementation-reported `ProbeOutcome::latency` is not trusted — it is merely a diagnostic field for implementers); `detail` is redacted uniformly by the framework before entering the outward JSON payload (`core::health::redact_detail`), and implementers still **must not** write credentials / connection strings / intranet topology into it. `stopped == true` means this report does not come from probe execution: the registry has been cleared by the shutdown protocol and the verdict is explicitly not-serving (`overall == "unhealthy"`, `probes == []`) — an empty registry does not mean all healthy, and readiness consumers must tell them apart. Shutdown coordination: `shutdown_async()` (`probe`+`lifecycle`) and `register_shutdown_into()` (`probe`+`lifecycle`+`shutdown`) automatically unregister all probes and mark stopped when they complete — a shut-down Kit has no services left to probe.

### `report` — Build Reports and Contract Manifests

The `report` feature solidifies the build process's observable artifacts into two serializable structures, both produced by methods on `Kit<Ready>` / `AsyncKit<Ready>` (`build_report()` / `contract_manifest()` / `take_config_overrides()`; see the corresponding tables in [Kit API](#kit-api) and [`async`](#async--asynchronous-module-building)).

#### `BuildReport`

```rust
pub struct BuildReport {
    pub schema_version: u32,                        // report structure version, currently SCHEMA_VERSION = 1
    pub topo_order: Vec<&'static str>,              // module names in the validated topological order
    pub modules: Vec<ModuleReportEntry>,            // per-module records in build-completion order
    pub overrides: Vec<OverrideRecord>,             // capability overrides observed during registration (override_module family)
    pub config_overrides: Vec<ConfigOverrideRecord>, // config-level override records (merge_config family, bounded history)
    pub config_overrides_dropped: u64,              // override records dropped once the bounded history filled up
    pub total_elapsed_us: Option<u64>,              // build wall-clock time (microseconds)
}
```

| Method | Description |
|------|------|
| `to_json()` | Serializes to a JSON string (returns `serde_json::Result`; does not nest the error inside a seemingly valid JSON body) |
| `from_json_str(s)` | Deserializes into a `serde_json::Value` for consumers to read field by field |

`schema_version` increments on structural breaking changes; `SCHEMA_VERSION` is the public constant holding the current value.

#### Per-Entry Types

```rust
pub struct ModuleReportEntry {
    pub name: &'static str,               // ModuleMeta::NAME
    pub state: ModuleBuildState,          // how the capability was produced
    pub deps: Vec<&'static str>,          // declared dependency names
    pub elapsed_us: Option<u64>,          // build time (microseconds); Some only when state == Built
}

pub enum ModuleBuildState {   // serde emits snake_case
    Built,       // built by build_fn during Kit::build
    Lazy,        // lazy: build_fn runs on the first require()
    Overridden,  // satisfied by a pre-built capability override (override_module family)
}

pub struct OverrideRecord {
    pub module: &'static str,   // overridden module name; "(unregistered)" for a never-registered type
    pub source: &'static str,   // which API injected the override
}

pub struct ConfigOverrideRecord {
    pub config: &'static str,   // overridden config type name (std::any::type_name, diagnostic only, matched by suffix)
    pub source: &'static str,   // API that applied the override (e.g. "merge_config")
    pub applied: bool,          // false = target config absent, that override was dropped
}
```

> The `config` field comes from `std::any::type_name`, whose exact format is not promised stable across compilers/versions — diagnostic use only; never match on full equality.
> The record order of `config_overrides` equals the call order only under single-threaded sequential calls; `AsyncKit` is a shared `Send + Sync` type, so the record order of concurrent `merge_config` calls is the lock-serialized arrival order, and read-modify-write is not atomic.
> This history is **bounded**: the most recent `MAX_CONFIG_OVERRIDE_HISTORY` (1024) entries are kept, older ones are dropped and counted by `config_overrides_dropped()`. Long-lived Kits with high-frequency `merge_config` should still drain with `take_config_overrides()` at rotation points — once at capacity, every `merge_config` triggers an O(n) front deletion, and every `build_report()` snapshot clones the whole segment.

#### `ContractManifest` / `ContractEntry`

```rust
pub struct ContractManifest {
    pub schema_version: u32,            // manifest structure version, currently CONTRACT_SCHEMA_VERSION = 1
    pub modules: Vec<ContractEntry>,    // one entry per registered module, in registration order
}

pub struct ContractEntry {
    pub module: &'static str,      // ModuleMeta::NAME
    pub version: &'static str,     // declared capability version, ModuleMeta::VERSION
    pub capability: &'static str,  // concrete capability type name (std::any::type_name, diagnostic only)
    pub deps: Vec<&'static str>,   // dependency module names
}
```

`ContractManifest::to_json()` produces a machine-readable contract manifest (for consumers to enforce version/dependency gates); `CONTRACT_SCHEMA_VERSION` is the current version constant. `capability` comes from `std::any::type_name`, whose format is not promised stable across compilers/versions — diagnostic use only, match by suffix and never compare for equality; the stable module identifier is `module` (`ModuleMeta::NAME`).

### `observer` — Build Observability

#### `BuildObserver` `observer`

```rust
pub trait BuildObserver: Send + Sync + 'static {
    fn on_module_start(&self, module_name: &'static str) {}                        // no-op by default
    fn on_module_built(&self, module_name: &'static str, elapsed: Duration) {}      // no-op by default
    fn on_build_error(&self, module_name: &'static str, error: &TraitKitError) {}   // no-op by default
}
```

All methods have default no-op implementations; override as needed.

### `request-scope` — Scopes

The old feature name `scope` is kept as a compatibility alias (forwarding to `request-scope`, already marked deprecated).

#### `Scope` `request-scope`

Lightweight per-request instance-isolation container (`!Send + !Sync`).

| Method | Description |
|---|---|
| `Scope::new()` | Creates an empty scope |
| `register::<M>()` | Registers a module |
| `require::<M>()` | Retrieves a capability (built on first use and cached) |
| `contains::<M>()` | Checks whether it is registered |

#### `AsyncScope` `request-scope` + `async`

Thread-safe async scope (`Send + Sync`).

| Method | Description |
|---|---|
| `AsyncScope::new()` | Creates an empty async scope |
| `insert::<M>(cap)` | Inserts a pre-built capability (the only entry point; a duplicate insert overwrites the old value) |
| `require::<M>()` | Retrieves a capability |
| `contains::<M>()` | Checks whether a capability has been inserted |

### `toggle` — Typed Toggle Handles

Besides the method-level API on `Kit` (`enable_toggle` / `is_toggle_enabled` / `register_if_toggle`), the `toggle` feature also provides a standalone toggle backend:

| API | Description |
|---|---|
| `ToggleBackend` trait | Toggle backend abstraction |
| `ToggleValue` | Typed toggle value (Bool / Int / Float / Str) |
| `MemoryToggle` | In-memory backend (switches to `ConfersToggle` automatically when the `confers` feature is enabled) |
| `ToggleKey` / `ToggleHandle<T>` | Strongly typed toggle keys and handles |
| `define_toggle_key!` macro | Compile-time guard against key typos |

### Extension Composition APIs

| API | Feature | Description |
|---|---|---|
| `SubKitSpec` / `SubKitModule` / `SubKitHandle` | `compose` | Sub-Kit registered into a parent Kit as a single module (capability namespace isolation + cross-Kit dependency validation) |
| `ConfersConfigModule` | `presets` | Brings the confers configuration center into the system as a Kit module (`presets-remote` uses the confers remote `AsyncSource`) |

The `report` feature's `BuildReport` / `contract_manifest`, plus the ungated `graph_dot` / `graph_mermaid` dependency-graph text exports, are covered by the `Kit<Ready>` table in [Kit API](#kit-api) and the [Dependency Graph Exports](#dependency-graph-exports) section.

`BuildReport` override semantics have two layers:

- **Module-level** `overrides`: registration records of the `override_module` family (sync-only; the async report is always empty)
- **Config-level** `config_overrides`: the `{ config, source, applied }` of every `merge_config` call (sync and async), in record order (under sequential calls / single-threading this is the call order; under concurrent `AsyncKit` calls it is the lock-serialized arrival order and does not represent the actual application order); `applied=false` means the target config did not exist and the override was dropped — surfaced explicitly rather than silently swallowed; `applied=true` means this call performed the apply and does not promise the final config still contains that override (a concurrent `set_config`/`merge_config` may overwrite it afterwards). **Usage warning (runtime accumulation holds only on the sync side)**: `Kit::merge_config`/`Kit::take_config_overrides` are usable in any state (`impl<S>`), with bounded history accumulation (the most recent `MAX_CONFIG_OVERRIDE_HISTORY` entries kept, older ones dropped and counted by `config_overrides_dropped()`; each entry is about 40B, and every `build_report()` snapshot clones the whole history) — long-lived Kits with high-frequency calls (e.g. inside every reload callback) should call `take_config_overrides()` at rotation points to drain and retrieve the records; on `AsyncKit` both are pre-build (Unbuilt) APIs (same surface as `set_config`), and after Ready the history is frozen, with no runtime accumulation

### Other Method-Level Gate Quick Reference

Features such as `reload` (hot reload), `encryption` (encrypted config), `toggle` (feature toggles), `decorator` (decorators), `shutdown` (graceful shutdown coordinator) attach to `Kit` / `AsyncKit` as **method-level gates**; see the Feature column of the [Kit API](#kit-api) tables above.

---

## 💻 Usage Examples

### Synchronous Module (Basic Flow)

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

### confers Configuration Loading `confers`

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
kit.load_config::<TraitKitConfig>()?;  // loaded via confers from environment variables / defaults
let kit = kit.build()?;
let config: TraitKitConfig = kit.config()?;
```

### More Examples

All runnable examples live in [examples/](../../examples/README.md), covering the complete usage of every feature gate.

---

## ✅ Best Practices

- **Declare modules with macros**: `impl_module_meta!` / `impl_auto_builder!` declare `ModuleMeta` / `AutoBuilder` in a single line, reducing boilerplate.
- **Declare dependencies explicitly**: declare dependencies via `impl_module_meta!(M, "name", deps = [...])` so that `build()`'s cycle detection and missing-dependency detection surface problems before the application starts.
- **Keep config in the `TypeMap`**: store and read typed config with `set_config` / `config::<C>()` instead of defining yet another module for config.
- **Enable features on demand**: advanced features automatically inherit lower-level ones (`encryption` → `reload` → `confers`), so enabling only the highest level is enough; keep unused features off to minimize dependencies.
- **Use `override_module` for test injection**: in tests, use `override_module::<M>(cap)` to skip the real build function and substitute a controlled capability.
- **Reuse error localization**: `TraitKitError`'s `Display` localizes automatically via `tr()`; business code does not need to translate Kit error messages itself.
