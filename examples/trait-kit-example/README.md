# trait-kit-example

Demonstrates every public API surface of `trait-kit` across the 4-level feature chain (`default` → `confers` → `confers-macros` → `hot-reload` → `encryption`). Each example is a standalone binary gated by `required-features` so it only compiles when its feature is enabled.

## Run

```sh
# default feature — no extras
cargo run -p trait-kit-example --example default_basic

# Level 1: confers (Configurable + load_config)
cargo run -p trait-kit-example --example confers_loader --features confers

# Level 2: confers-macros (ModuleConfig trait + Config derive re-export)
cargo run -p trait-kit-example --example confers_macros --features confers-macros

# Level 3: hot-reload (subscribe + reload_config)
cargo run -p trait-kit-example --example hot_reload --features hot-reload

# Level 4: encryption (set_encrypted + get_encrypted + HKDF key derivation)
cargo run -p trait-kit-example --example encryption --features encryption
```

## Examples

| Example            | Feature         | Demonstrates                                                                   |
| ------------------ | --------------- | ------------------------------------------------------------------------------ |
| `default_basic`    | `default`       | `ModuleMeta` + `AutoBuilder` + `Kit::new`/`register`/`build`/`require`/`contains`/`optional` |
| `confers_loader`   | `confers`       | `#[derive(Config)]` + `Configurable` impl + `Kit::load_config` + env-var fallback |
| `confers_macros`   | `confers-macros`| `ModuleConfig` trait (`PATH` + `default_value`) + module consuming config in `build()` |
| `hot_reload`       | `hot-reload`    | `subscribe::<C>` + `reload_config::<C>` + callback counting via `Rc<Cell<_>>` |
| `encryption`       | `encryption`    | `set_encrypted` + `get_encrypted` roundtrip + wrong-key rejection + `contains_encrypted` |

## Notes

- The example crate is `publish = false` and is a workspace member of the root `trait-kit` workspace. It is never published to crates.io.
- Each example exits 0 on success and panics on failure (assertions).

---

[← Back to trait-kit](../../README.md)
