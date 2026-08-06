# trait-kit-example

Demonstrates every public API surface of `trait-kit` across all feature gates. Each example is a standalone binary gated by `required-features` so it only compiles when its feature is enabled.

## Run

```sh
# default feature — no extras
cargo run -p trait-kit-example --example default_basic

# Level 1: confers (Configurable + load_config)
cargo run -p trait-kit-example --example confers_loader --features confers

# confers: ModuleConfig trait + Config derive re-export
cargo run -p trait-kit-example --example confers_macros --features confers

# confers: Validatable trait + load_and_validate
cargo run -p trait-kit-example --example validation --features confers

# reload: subscribe + reload_config
cargo run -p trait-kit-example --example hot_reload --features reload

# confers: snapshot_config + restore_config + has_snapshot
cargo run -p trait-kit-example --example snapshot_restore --features confers

# encryption: encrypted config storage (set_encrypted + get_encrypted + HKDF key derivation)
cargo run -p trait-kit-example --example encryption --features encryption

# Async: AsyncKit typestate flow
cargo run -p trait-kit-example --example async_basic --features async

# Lifecycle: on_ready + on_shutdown hooks
cargo run -p trait-kit-example --example lifecycle --features lifecycle

# Health: HealthCheck + health_report
cargo run -p trait-kit-example --example health_check --features health

# Observer: BuildObserver callbacks
cargo run -p trait-kit-example --example observability --features observer

# Scope: per-request instance isolation
cargo run -p trait-kit-example --example scope_basic --features scope

# Toggle: runtime feature flag module enable/disable
cargo run -p trait-kit-example --example toggle_basic --features toggle

# Conditional: predicate-gated registration (no feature gate — always available)
cargo run -p trait-kit-example --example conditional

# Factory: per-call instance creation (no feature gate — always available)
cargo run -p trait-kit-example --example factory

# Decorator: post-build capability wrapping
cargo run -p trait-kit-example --example decorator --features decorator

# Interface: dyn Trait DI
cargo run -p trait-kit-example --example interface --features interface

# i18n: ICU4X locale-aware formatting
cargo run -p trait-kit-example --example i18n --features i18n
```

## Examples

| Example            | Feature          | Demonstrates                                                                                    |
| ------------------ | ---------------- | --------------------------------------------------------------------------------------------- |
| `default_basic`    | `default`        | `ModuleMeta` + `AutoBuilder` + `Kit::new`/`register`/`build`/`require`/`contains`/`optional`   |
| `confers_loader`   | `confers`        | `#[derive(Config)]` + `Configurable` impl + `Kit::load_config` + env-var fallback              |
| `confers_macros`   | `confers`        | `ModuleConfig` trait (`PATH` + `default_value`) + module consuming config in `build()`          |
| `validation`       | `confers`        | `Validatable` trait + `Kit::load_and_validate` — config validation on load                      |
| `hot_reload`       | `reload`         | `subscribe::<C>` + `reload_config::<C>` + callback counting via `Rc<Cell<_>>`                  |
| `snapshot_restore` | `confers`        | `snapshot_config` + `restore_config` + `has_snapshot` — config snapshot & rollback              |
| `encryption`       | `encryption`     | `set_encrypted` + `get_encrypted` roundtrip + wrong-key rejection + `contains_encrypted`        |
| `async_basic`      | `async`          | `AsyncAutoBuilder` + `AsyncKit::new`/`register`/`build`/`require`/`contains`/`set_config`       |
| `lifecycle`        | `lifecycle`      | `Lifecycle` trait (`on_ready` + `on_shutdown`) + `register_lifecycle` + `Kit::shutdown()`       |
| `health_check`     | `health`         | `HealthCheck` trait + `HealthStatus` + `register_health_check` + `health_check` + `health_report` |
| `observability`    | `observer`       | `BuildObserver` trait + `with_observer` + `on_module_start`/`on_module_built` callbacks          |
| `scope_basic`      | `scope`          | `Scope::new`/`register`/`require`/`contains` + per-request instance isolation + lazy caching     |
| `toggle_basic`     | `toggle`         | `enable_toggle` + `is_toggle_enabled` + `register_if_toggle` — runtime feature flag control     |
| `conditional`      | —                | `register_if::<M>(predicate)` + runtime predicate-gated registration                            |
| `factory`          | —                | `Kit<Ready>::factory::<M>()` + per-call instance creation vs singleton `require()`               |
| `decorator`        | `decorator`      | `Kit::decorate::<M>(fn)` + post-build capability transformation                                 |
| `interface`        | `interface`      | `InterfaceBuilder` + `register_as::<M>()` + `resolve::<dyn Trait>()` for type-erased DI          |
| `i18n`             | `i18n`           | `I18nFormatter` + `format_number`/`format_date`/`plural_category`/`compare` + error handling     |

## Notes

- The example crate is `publish = false` and is a workspace member of the root `trait-kit` workspace. It is never published to crates.io.
- Each example exits 0 on success and panics on failure (assertions).
- The `async_basic` example uses a minimal `block_on` executor (no tokio required) because the async futures resolve immediately.

---

[← Back to trait-kit](../README.md)
