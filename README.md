# TraitKit

[![Crates.io][crates-badge]][crates-url]
[![Docs.rs][docs-badge]][docs-url]
[![MIT licensed][license-badge]][license-url]
[![MSRV][msrv-badge]][msrv-url]

<!-- Note: Badges will resolve once the crate is published to crates.io -->

[crates-badge]: https://img.shields.io/crates/v/trait-kit?style=flat-square
[crates-url]: https://crates.io/crates/trait-kit
[docs-badge]: https://img.shields.io/docsrs/trait-kit?style=flat-square
[docs-url]: https://docs.rs/trait-kit
[license-badge]: https://img.shields.io/badge/license-MIT-blue?style=flat-square
[license-url]: https://github.com/Kirky-X/trait-kit/blob/main/LICENSE
[msrv-badge]: https://img.shields.io/badge/MSRV-1.71-orange?style=flat-square
[msrv-url]: https://github.com/Kirky-X/trait-kit

**trait-kit** is a lightweight Rust library that defines a standardized module interface and provides a centralized capability & configuration management center (`Kit`). It gives you a consistent, type-safe way to define modules, inject dependencies, and manage capabilities — without committing to a heavy DI framework.

---

## Features

- **Standardized Module Interface** — The `Module` trait defines a uniform contract: every module declares its Config, Requirements, Capability, Error, and Builder. Consistent initialization everywhere.
- **Type-Safe Capability Management** — Register and retrieve capabilities via typed `CapabilityKey`s. No stringly-typed lookups. Trait-object-safe with full `Send + Sync` support.
- **Thread-Safe Config Center** — `ConfigHandle<T>` provides live configuration updates with lock-free reads via `arc-swap`. Multiple handles share the same underlying storage; updates propagate instantly.
- **Clean Builder Integration** — `.kit(&kit).provide::<K>()` builds a module and registers its capability in one fluent chain. Config and requirements are injected before `.kit()`, enforced at compile time.
- **Explicit Composition** — No magic auto-wiring. You control the initialization order and dependency construction. The code is readable, debuggable, and easy to refactor.
- **Minimal Dependencies** — Only `arc-swap` (internal) and `thiserror` (public error types). No heavy DI framework, no proc macros, no runtime reflection.

---

## Quick Start

### MSRV

Minimum Supported Rust Version: **1.71**

### Installation

```sh
cargo add trait-kit
```

### Minimal Example

Define a logger module, register it to Kit, and retrieve it:

```rust
use std::sync::Arc;
use trait_kit::prelude::*;

// 1. Define a capability trait
trait Logger: Send + Sync {
    fn info(&self, msg: &str);
}

// 2. Define a capability key
struct MainLogger;

impl CapabilityKey for MainLogger {
    type Capability = dyn Logger + Send + Sync;
    const NAME: &'static str = "main_logger";
}

// 3. Define a module
struct LoggerModule;

impl Module for LoggerModule {
    const NAME: &'static str = "logger_module";
    type Config = NoConfig;
    type Requirements = NoRequirements;
    type Capability = Arc<dyn Logger + Send + Sync>;
    type Error = std::convert::Infallible;
    type Builder = LoggerModuleBuilder;
}

// 4. Define a builder
struct LoggerModuleBuilder;

impl ModuleBuilder<LoggerModule> for LoggerModuleBuilder {
    fn build(self) -> Result<Arc<dyn Logger + Send + Sync>, std::convert::Infallible> {
        Ok(Arc::new(ConsoleLogger))
    }
}

struct ConsoleLogger;

impl Logger for ConsoleLogger {
    fn info(&self, msg: &str) {
        println!("[INFO] {msg}");
    }
}

// 5. Use it
fn main() {
    let kit = Kit::new();

    let logger = LoggerModuleBuilder
        .kit(&kit)
        .provide::<MainLogger>()
        .unwrap();

    logger.info("Hello from trait-kit!");

    // Retrieve from Kit later
    let from_kit: Arc<dyn Logger + Send + Sync> = kit.require::<MainLogger>().unwrap();
    from_kit.info("Retrieved from Kit");
}
```

---

## Usage

### Module with Configuration

```rust
use std::sync::Arc;
use trait_kit::prelude::*;

#[derive(Debug, Clone, PartialEq)]
struct AppConfig {
    pub debug: bool,
}

struct CfgModule;
struct CfgBuilder {
    config: Option<AppConfig>,
}

impl CfgBuilder {
    fn new() -> Self {
        CfgBuilder { config: None }
    }
}

impl Module for CfgModule {
    const NAME: &'static str = "cfg_module";
    type Config = AppConfig;
    type Requirements = NoRequirements;
    type Capability = Arc<dyn Send + Sync>;
    type Error = BuildError;
    type Builder = CfgBuilder;
}

impl WithConfig<CfgModule> for CfgBuilder {
    fn config(self, config: AppConfig) -> Self {
        CfgBuilder { config: Some(config) }
    }
}

impl ModuleBuilder<CfgModule> for CfgBuilder {
    fn build(self) -> Result<Arc<dyn Send + Sync>, BuildError> {
        let _cfg = self.config.ok_or(BuildError::MissingConfig {
            module: "cfg_module",
        })?;
        // Initialize module with configuration
        Ok(Arc::new(()))
    }
}

struct CfgCapKey;
impl CapabilityKey for CfgCapKey {
    type Capability = dyn Send + Sync;
    const NAME: &'static str = "cfg_capability";
}

// Usage
let kit = Kit::new();
CfgBuilder::new()
    .config(AppConfig { debug: true })
    .kit(&kit)
    .provide::<CfgCapKey>()
    .unwrap();
```

### Module with Dependencies

```rust
use std::sync::Arc;
use trait_kit::prelude::*;

trait Logger: Send + Sync {
    fn info(&self, msg: &str);
}

struct ConsoleLogger;
impl Logger for ConsoleLogger {
    fn info(&self, msg: &str) {
        println!("{msg}");
    }
}

struct MyReqs {
    pub logger: Arc<dyn Logger + Send + Sync>,
}

struct DepModule;
struct DepBuilder {
    requirements: Option<MyReqs>,
}

impl DepBuilder {
    fn new() -> Self {
        DepBuilder { requirements: None }
    }
}

impl Module for DepModule {
    const NAME: &'static str = "dep_module";
    type Config = NoConfig;
    type Requirements = MyReqs;
    type Capability = Arc<dyn Send + Sync>;
    type Error = BuildError;
    type Builder = DepBuilder;
}

impl WithRequirements<DepModule> for DepBuilder {
    fn requirements(self, reqs: MyReqs) -> Self {
        DepBuilder { requirements: Some(reqs) }
    }
}

impl ModuleBuilder<DepModule> for DepBuilder {
    fn build(self) -> Result<Arc<dyn Send + Sync>, BuildError> {
        let _reqs = self.requirements.ok_or(BuildError::MissingRequirements {
            module: "dep_module",
        })?;
        // Use _reqs.logger during initialization
        Ok(Arc::new(()))
    }
}

struct DepCapKey;
impl CapabilityKey for DepCapKey {
    type Capability = dyn Send + Sync;
    const NAME: &'static str = "dep_capability";
}

// Usage
let kit = Kit::new();
let logger: Arc<dyn Logger + Send + Sync> = Arc::new(ConsoleLogger);

DepBuilder::new()
    .requirements(MyReqs { logger: logger.clone() })
    .kit(&kit)
    .provide::<DepCapKey>()
    .unwrap();
```

### Configuration Center

```rust
use trait_kit::prelude::*;

#[derive(Debug, Clone, PartialEq)]
struct AppConfig {
    version: String,
    debug: bool,
}

struct AppConfigKey;

impl ConfigKey for AppConfigKey {
    type Config = AppConfig;
    const NAME: &'static str = "app_config";
}

let kit = Kit::new();

kit.set_config::<AppConfigKey>(AppConfig {
    version: "1.0.0".to_string(),
    debug: false,
});

let handle = kit.config::<AppConfigKey>().unwrap();
println!("Current: {:?}", handle.load());

// All handles share the same underlying storage
handle.set(AppConfig {
    version: "2.0.0".to_string(),
    debug: true,
});
```

### Layered Composition

Build a logger → inject it into storage → inject both into a user service — all managed by Kit:

```rust
// See full example: examples/layered_app.rs
let kit = Kit::new();

let logger = LoggerModuleBuilder.kit(&kit).provide::<MainLogger>()?;
let storage = StorageBuilder::new()
    .logger(logger.clone())
    .kit(&kit)
    .provide::<MainStorage>()?;
let user_service = UserBuilder::new()
    .logger(logger)
    .storage(storage)
    .kit(&kit)
    .provide::<UserServiceKey>()?;

user_service.create_user("Alice");
```

### Kit API Overview

| Method                       | Description                                   |
| ---------------------------- | --------------------------------------------- |
| `Kit::new()`                 | Create an empty Kit.                          |
| `kit.provide::<K>()`         | Register a capability (fails if key exists).  |
| `kit.replace::<K>()`         | Register or overwrite a capability.           |
| `kit.require::<K>()`         | Retrieve a capability (fails if missing).     |
| `kit.contains::<K>()`        | Check if a capability is registered.          |
| `kit.set_config::<K>()`      | Set a configuration value.                    |
| `kit.config::<K>()`          | Get a shared `ConfigHandle` for live updates. |
| `kit.contains_config::<K>()` | Check if a config key exists.                 |

### Crate Feature Flags

trait-kit currently has **no optional features**. All functionality is available out of the box.

---

## Why trait-kit?

trait-kit sits between "raw manual wiring" and "full DI framework":

| Approach                 | Pros                                      | Cons                                       |
| ------------------------ | ----------------------------------------- | ------------------------------------------ |
| **Manual wiring**        | Simple, no deps.                          | Ad-hoc patterns, inconsistent per project. |
| **trait-kit**            | Standard pattern, type-safe, lightweight. | You still wire dependencies explicitly.    |
| **Full DI (shaku etc.)** | Auto-resolved, less glue code.            | Heavier deps, magic, harder to debug.      |

trait-kit gives you the **standardization** of a DI framework with the **explicitness** of manual wiring.

---

## Contributing

### Build Requirements

- Rust **1.71** or later (stable).
- No external tooling required (no protoc, no openssl, no system libraries).

### Development Commands

```sh
# Run all tests
cargo test

# Run example programs
cargo test --examples
cargo run --example basic_logger
cargo run --example service_injection
cargo run --example config_center
cargo run --example layered_app

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt -- --check

# Compile-fail tests (trybuild)
cargo test --test compile_fail
```

### Code of Conduct

This project follows the [Rust Code of Conduct](https://www.rust-lang.org/policies/code-of-conduct). All contributors are expected to uphold it.

### Pull Request Process

1. Ensure all tests pass and Clippy is clean.
2. Add tests for new functionality (unit, integration, or compile-fail as appropriate).
3. Update examples if public API changes.
4. Keep the README in sync with any API changes.

---

## License

This project is licensed under the [MIT License](LICENSE).

© 2026 Kirky.X

---

## Related Links

- [API Documentation (docs.rs)](https://docs.rs/trait_kit)
- [Crate on crates.io](https://crates.io/crates/trait_kit)
- [GitHub Repository](https://github.com/Kirky-X/trait-kit)
- [Issue Tracker](https://github.com/Kirky-X/trait-kit/issues)
