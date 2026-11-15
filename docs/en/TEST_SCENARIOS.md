# 🧪 Trait-Kit Acceptance Test Scenarios

> Applies to version: trait-kit **0.5.0-rc.2** (matrix authoring and reconciliation baseline; feature-surface extensions since 0.5.0-rc.3 are tracked in the [CHANGELOG](../CHANGELOG.md), scenario semantics unchanged)
> Purpose: exhaustively enumerate all acceptance scenarios as the reconciliation baseline for the `tests/e2e/` E2E tests.
> Authoring sources (read-only verification): `Cargo.toml [features]`, the `src/lib.rs` export surface, `src/core` (meta/macros/health/lifecycle/observer), `src/kit` (kit/graph/typemap/config/scope/toggle/shutdown/async_kit/async_typemap), `src/i18n` (FTL catalogs + ICU4X), `trait-kit-macros` (ConfigInherit/SharedConfig), `tests/`, `tests/ui/` (4 cases at authoring time + 1 in `probe_sensitive/`, 5 trybuild cases total; now 17 — 11 typestate + 6 macro diagnostics, see §5.1 L2b), `examples/` (20 examples).
> All referenced existing test names were verified to exist via `grep`.
> Combination compatibility was verified by running `cargo check --all-features` (exit 0).
>
> **Landing reconciliation (backfilled)**:
> 1. All §2 scenarios have landed in `tests/e2e/` (13 files, explicitly registered as `[[test]]` targets named `e2e_*` in the root `Cargo.toml`); per-scenario execution results are tracked in the `reviews/acceptance-report.md` ledger.
> 2. **2 real-behavior corrections** (RLD-07, CCY-06): the original assumption was that calling `reload_config`/`set_config` while a `require_ref` borrow is alive would panic on a borrow conflict; the implementation confirms `configs` and `capabilities` are two independent `TypeMap`s (`src/kit/kit.rs`), and config writes work normally while the borrow is alive; codified per the real behavior (regression guard: if the two TypeMaps are ever merged, the test will fail and expose the hidden panic). Also, `set_config` is a `Kit<Unbuilt>`-state method; Ready-state config-write paths include `reload_config` / `merge_config` / `populate_defaults`.
> 3. **CCY-03 equivalent landing**: not via trybuild (stderr snapshots depend on the feature set and are fragile across the combination matrix), but codified equivalently as the compile-time assertion `static_assertions::assert_not_impl_any!(Kit<Unbuilt>: Sync)` (plus `Kit<Ready>`) in `tests/basic.rs:11-12`.
> 4. Existing-coverage attribution: each e2e file's header comment marks the per-scenario attribution of "newly landed in this file vs. existing coverage referenced".

## 📋 Table of Contents

<details open>
<summary>📑 Table of Contents</summary>

- [📌 Reading Conventions](#-reading-conventions)
- [🗺️ 1. Overview: Capability Domains × Feature Groups](#️-1-overview-capability-domains--feature-groups)
- [📋 2. Scenario Matrix](#-2-scenario-matrix)
  - [2.1 Module Metadata and Declarative Macros](#21-module-metadata-and-declarative-macrosmet9)
  - [2.2 Registration and Build / Typestate](#22-registration-and-build--typesatereg20)
  - [2.3 Capability Retrieval](#23-capability-retrievalcap12)
  - [2.4 Dependency Graph](#24-dependency-graphdep10)
  - [2.5 Config Center](#25-config-centercfg22)
  - [2.6 Hot Reload](#26-hot-reloadrld10)
  - [2.7 Encrypted Storage](#27-encrypted-storageenc13)
  - [2.8 Interface/Implementation Separation](#28-interfaceimplementation-separationitf9)
  - [2.9 Lifecycle](#29-lifecyclelcy9)
  - [2.10 Health Checks](#210-health-checkshlt9)
  - [2.11 Build Observers](#211-build-observersobs7)
  - [2.12 Decorators](#212-decoratorsdec8)
  - [2.13 Scopes](#213-scopesscp10)
  - [2.14 Feature Toggles](#214-feature-togglestgl9)
  - [2.15 Graceful Shutdown](#215-graceful-shutdownshd13)
  - [2.16 Async Kit](#216-async-kitask15)
  - [2.17 Internationalization](#217-internationalizationi1814)
  - [2.18 Error System](#218-error-systemerr9)
  - [2.19 Prelude Export Surface](#219-prelude-export-surfacepre3)
  - [2.20 Feature Combination Interactions](#220-feature-combination-interactionscmp15)
  - [2.21 Concurrency and Races](#221-concurrency-and-racesccy7)
  - [2.22 Feature Compilation Matrix](#222-feature-compilation-matrixprs6)
- [🔗 3. Feature Mutual Exclusion / Combination Matrix](#-3-feature-mutual-exclusion--combination-matrix)
- [🐳 4. Docker Service Requirements Summary](#-4-docker-service-requirements-summary)
- [🧭 5. Execution Plan](#-5-execution-plan)
- [📊 6. Statistics Summary](#-6-statistics-summary)

</details>

## 📌 Reading Conventions

- **Type**: `Normal` (expected behavior under valid input) / `Error` (invalid input, violation; must yield a clear error or compile failure) / `Boundary` (extreme values, concurrency, races, combination edges, semantics pinning).
- **Existing coverage**: `file::test_name` denotes an existing test (inline test modules inside `src/**` omit the `src/` prefix and are given directly as `relative/path::test_name`); `→assertions to add` means a unit test or example exists but lacks integration/behavior-level assertions; `none→to add` means no existing coverage.
- **E2E target**: the target file planned for `tests/e2e/` (created in the landing phase); typestate compile-time violations land in `tests/ui/`.
- **Dependent services**: `None` for all scenarios (pure in-memory library; no docker, no external process dependencies — rationale in §4).
- Feature names always use the original names from `Cargo.toml [features]`; `Kit`/`Kit<Ready>` refer to the two typestate states.

### Key Findings (from the authoring-time inventory)

1. **`src/kit/toggle.rs` is already a complete toggle implementation (609 lines; 10 lines of doc-only code at the 0.5.0-rc.2 authoring point, fully landed since rc.4, see the [CHANGELOG](../CHANGELOG.md))**: beyond the methods on `Kit`/`Kit<Ready>` (`enable_toggle` / `is_toggle_enabled` / `register_if_toggle`, typed `set_toggle` / `get_toggle`, etc.), `src/kit/mod.rs` also exports standalone APIs: the `ToggleBackend` trait, the `MemoryToggle` in-memory backend, `ConfersToggle` (confers-gated), and `ToggleValue` / `ToggleKey` / `ToggleHandle`.
2. **The sync `Kit` is `RefCell`-based (`!Sync`); `AsyncKit` is `AsyncTypeMap`-based (`Send + Sync`)**: the two product lines have different concurrency models. All APIs on the sync line are single-threaded only; the negative `!Sync` assertion is currently missing (CCY-03).
3. **Many tests in `tests/e2e/e2e_feature_combinations.rs` and `tests/e2e/e2e_advanced.rs` are gated with `#[cfg(all(test, feature = …))]`**: a bare `cargo test` (no feature) only runs the no-feature group; acceptance must run `cargo test --features <combination>` per the feature matrix, otherwise combination groups silently do not compile or execute.
4. **Typestate violations are guarded by 11 trybuild cases** (`tests/ui/` root 7: `ready_cannot_build.rs`, `ready_cannot_register.rs`, `ready_cannot_set_config.rs`, `ready_cannot_override_module.rs`, `ready_cannot_with_metrics_port.rs`, `unbuilt_cannot_optional.rs`, `async_unbuilt_cannot_optional.rs`; 4 gated subdirectories: `probe_sensitive/async_ready_cannot_register.rs`, `observer_ready/ready_cannot_with_observer.rs`, `decorator_ready/ready_cannot_decorate.rs`, `lifecycle_ready/ready_cannot_register_lifecycle.rs`), driven by the single runner `tests/compile_fail.rs` (5 glob groups); the remaining Ready-state violation surfaces (e.g. registration-class methods that do not exist on Ready) need new UI cases as the API grows.
5. The header of `tests/e2e/e2e_advanced.rs` notes that its scenario IDs follow the B/A/E/C series from `temp/feature-analysis.md` (B=basic, A=advanced, E=errors, C=coverage gaps); this matrix has absorbed all of them and renumbered.
6. In the examples crate (`trait-kit-examples`) every example is explicitly registered via `[[example]]` with `required-features` and is not compiled by default; running an example requires passing the corresponding feature.
7. **No `autotests = false`**: new E2E files under `tests/` are auto-discovered, no need to append `[[test]]` sections (unlike confers); but feature-gated files need their own `#![cfg(feature = "…")]` header. (Landing correction: the E2E files ultimately moved into the `tests/e2e/` subdirectory; files in subdirectories are not auto-discovered, so 13 `[[test]]` targets were explicitly registered in the root `Cargo.toml`, target names kept as `e2e_*`, with zh/en i18n still as separate process binaries.)

---

## 🗺️ 1. Overview: Capability Domains × Feature Groups

| # | Domain | ID prefix | Features (Cargo.toml names) | Main public APIs |
|---|--------|-------------|------------------------------|--------------|
| 1 | Module metadata and declarative macros | MET | — (MET-04/07/08 need async, di) | `core::{ModuleMeta, AutoBuilder, Interface, InterfaceBuilder}`, macros `impl_module_meta!` / `impl_auto_builder!` / `impl_async_auto_builder!` |
| 2 | Registration and build (typestate) | REG | — | `kit::{Kit, Unbuilt, Ready}`, `register / register_lazy / register_multi / register_if / override_module / build` |
| 3 | Capability retrieval | CAP | — | `Kit::require / optional / require_ref / require_all / contains / contains_config / factory` |
| 4 | Dependency graph | DEP | — | `kit::{DependencyGraph, GraphError, ModuleEntry}`, `graph_dot / graph_mermaid` |
| 5 | Config center | CFG | confers | `Configurable / ModuleConfig / Validatable / ValidationError / ConfigInherit / SharedConfig`, `set_config / config / load_config / load_and_validate / load_config_with / load_config_or_default / snapshot_config / restore_config / has_snapshot / populate_defaults / merge_config / extract_shared / inject_shared`, free functions `interpolate_json_value / merge_json_deep`, derive macros `ConfigInherit / SharedConfig` (trait-kit-macros) |
| 6 | Hot reload | RLD | reload (→confers, Kit's own SubscriberMap) | `Kit::subscribe / reload_config` |
| 7 | Encrypted storage | ENC | encryption (→confers→confers/encryption) | `Kit::set_encrypted / get_encrypted / contains_encrypted`, `EncryptedBlob`, re-exports `XChaCha20Crypto / derive_field_key` |
| 8 | Interface/implementation separation | ITF | di | `register_as / resolve`, `core::{Interface, InterfaceBuilder}` |
| 9 | Lifecycle | LCY | lifecycle (×async) | `core::{Lifecycle, AsyncLifecycle}`, `register_lifecycle / shutdown` |
| 10 | Health checks | HLT | health (×async) | `core::{HealthCheck, AsyncHealthCheck, HealthStatus}`, `register_health_check / health_check / health_report` |
| 11 | Build observers | OBS | observer (×async) | `core::BuildObserver`, `with_observer` |
| 12 | Decorators | DEC | decorator (×async) | `Kit::decorate` (covers the eager/lazy/multi/interface build paths) |
| 13 | Scopes | SCP | request-scope (×async) | `kit::{Scope, AsyncScope}`, `create_scope`, `Scope::register / require / contains` |
| 14 | Feature toggles | TGL | toggle | `Kit::enable_toggle / is_toggle_enabled / register_if_toggle` (both Unbuilt and Ready states) |
| 15 | Graceful shutdown | SHD | shutdown (×async) | `kit::{ShutdownCoordinator, ShutdownPhase, ShutdownPhaseResult, ShutdownResult, AsyncShutdownCoordinator}` |
| 16 | Async Kit | ASK | async | `AsyncKit / AsyncAutoBuilder / AsyncReady / AsyncUnbuilt / AsyncTypeMap`, `AsyncKit::register / build / require / factory / create_scope` |
| 17 | Internationalization | I18 | i18n | `i18n::{tr, I18nManager, I18nFormatter, I18nError}`, FTL catalogs (en/zh), ICU4X number/date/plural/collation |
| 18 | Error system | ERR | — (some variants feature-gated) | `TraitKitError` (11 variants) / `TraitKitResult`; Display goes through the Fluent catalog via `tr()` |
| 19 | Prelude export surface | PRE | — (follows each feature) | `prelude::*` re-export consistency |
| 20 | Feature combination interactions | CMP | multiple features stacked | — |
| 21 | Concurrency and races | CCY | async + whole library | — |
| 22 | Feature compilation matrix | PRS | all features, full set | `cargo check --features …` |

Total scenarios: **244** (100 Normal / 60 Error / 83 Boundary, plus 1 Normal/Error dual-assertion scenario; programmatic verification in §6).

---

## 📋 2. Scenario Matrix

### 2.1 Module Metadata and Declarative Macros (MET, 9)

| ID | Scenario | Type | Features | Services | Existing coverage | E2E target |
|----|---------|------|-------------|---------|---------|---------|
| MET-01 | Hand-written `ModuleMeta`: `NAME` takes effect, `dependencies()` returns an empty slice by default | Normal | — | None | `src/core/meta.rs::default_dependencies_returns_empty_slice`, `src/core/meta.rs::mock_logger_module_dependencies_empty` | tests/e2e/e2e_core.rs |
| MET-02 | `impl_module_meta!(T, "name", deps = [A, B])` generates name/dependency pairs exactly matching the hand-written impl (name + TypeId equal item by item) | Normal | — | None | `src/core/macros.rs::macro_generates_correct_name_with_deps`, `macro_dependency_names_match_module_meta_names`, `macro_dependency_type_ids_match_hand_written` | tests/e2e/e2e_core.rs |
| MET-03 | `impl_auto_builder!` generates a complete `AutoBuilder` (Capability/Error/build identical item by item to the hand-written impl) | Normal | — | None | `src/core/macros.rs::macro_sync_*` (8 cases, incl. `macro_sync_build_returns_expected_capability`) | tests/e2e/e2e_core.rs |
| MET-04 | `impl_async_auto_builder!` generates an `AsyncAutoBuilder`: `build` returns `Pin<Box<dyn Future>>`, Capability `Send+Sync`, Error `Send+'static` | Normal | async | None | `src/core/macros.rs::macro_async_*` (9 cases), `src/core/meta.rs::async_auto_builder_returns_pin_box_future`, `async_auto_builder_capability_is_send_sync`, `async_auto_builder_error_is_send_static` | tests/e2e/e2e_async.rs |
| MET-05 | The macro-generated code propagates `build` returning `Err` exactly like the hand-written impl (no swallowing, no wrapping) | Error | — | None | `src/core/macros.rs::macro_sync_build_propagates_errors`, `macro_async_build_propagates_errors` | tests/e2e/e2e_core.rs |
| MET-06 | The no-dependency macro form (two-argument form) generates empty dependencies, equivalent to a hand-written empty impl | Boundary | — | None | `src/core/macros.rs::macro_generates_empty_dependencies_when_no_deps`, `macro_name_equals_hand_written_name` | tests/e2e/e2e_core.rs |
| MET-07 | The `Interface` marker is auto-implemented for all `'static` types (incl. `?Sized` trait objects, primitives, custom types) | Normal | di | None | `src/core/meta.rs::interface_auto_implemented_for_primitive_types`, `_custom_types`, `_reference_types` | tests/e2e/e2e_features.rs |
| MET-08 | `InterfaceBuilder` has associated Capability/Interface, does not require `AutoBuilder`, and Interface may be a `dyn Trait` | Normal | di | None | `src/core/meta.rs::interface_builder_does_not_require_autobuilder`, `interface_builder_interface_type_is_dyn_compatible`, `interface_builder_into_interface_produces_trait_object` | tests/e2e/e2e_features.rs |
| MET-09 | `a25` one module registers via all three declaration forms (hand-written / two-arg macro / deps macro) with identical behavior | Boundary | — | None | `tests/e2e/e2e_advanced.rs::a25_impl_module_meta_macro_three_forms` | tests/e2e/e2e_core.rs |

### 2.2 Registration and Build / Typestate (REG, 20)

| ID | Scenario | Type | Features | Services | Existing coverage | E2E target |
|----|---------|------|-------------|---------|---------|---------|
| REG-01 | An empty `Kit::new()` builds directly and successfully, yielding an empty `Kit<Ready>` | Normal | — | None | `tests/e2e/e2e_advanced.rs::b01_empty_kit_build_succeeds`, `c01_empty_dependency_graph_build_succeeds` | tests/e2e/e2e_core.rs |
| REG-02 | `register::<M>()` → `build()` → `require::<M>()` retrieves the capability, full chain | Normal | — | None | `tests/basic.rs::test_basic_build_and_require` | tests/e2e/e2e_core.rs |
| REG-03 | Dependency resolution: when A depends on B, B builds first and B's capability is retrievable inside A::build | Normal | — | None | `tests/basic.rs::test_dependency_resolution`, `tests/e2e/e2e_advanced.rs::b04_chain_dependency_topological_build`, `c03_deep_dependency_chain_10_layers_builds_in_topo_order` | tests/e2e/e2e_core.rs |
| REG-04 | Registering a module that depends on an unregistered module → `build()` returns `DependencyMissing{module, missing}` | Error | — | None | `tests/basic.rs::test_missing_dependency_error`, `src/kit/graph.rs::graph_validate_missing_dependency` | tests/e2e/e2e_core.rs |
| REG-05 | Two nodes mutually depending → `build()` returns `CycleDetected`, cycle contains both module names | Error | — | None | `tests/basic.rs::test_cycle_detection`, `kit_build_returns_cycle_detected_for_mutual_deps` | tests/e2e/e2e_core.rs |
| REG-06 | Both a three-node cycle and self-dependency (A→A) are detected as cycles | Error | — | None | `tests/e2e/e2e_advanced.rs::e03_three_node_cycle_detected`, `c02_self_dependency_detected_as_cycle` | tests/e2e/e2e_core.rs |
| REG-07 | Duplicate registration: the same module registered twice, plus cross-method duplicates of `register`/`register_lazy`/`register_multi` → `AlreadyRegistered` | Error | — | None | `tests/basic.rs::test_duplicate_registration_error`, `tests/e2e/e2e_advanced.rs::e05_cross_method_duplicate_registration` | tests/e2e/e2e_core.rs |
| REG-08 | A `build` callback returning `Err` → `build()` fails with `BuildFailed{context=module name, source=original error}`; multi-binding build failures have the same shape | Error | — | None | `tests/e2e/e2e_advanced.rs::e08_build_fn_returns_err_propagates_build_failed`, `src/kit/kit.rs::multi_binding_build_error_returns_build_failed` | tests/e2e/e2e_core.rs |
| REG-09 | `register_lazy` does not build during `build()` (counter is zero), only enters the graph for validation | Normal | — | None | `src/kit/kit.rs::register_lazy_does_not_build_during_build`, `register_lazy_adds_to_dependency_graph`, `tests/e2e/e2e_advanced.rs::a01_register_lazy_first_require_triggers_build` | tests/e2e/e2e_core.rs |
| REG-10 | A lazy module's first `require()` triggers the build; a second `require()` returns the OnceLock cache without rebuilding | Boundary | — | None | `tests/e2e/e2e_advanced.rs::a02_lazy_module_not_rebuilt_on_second_require`, `c11_build_then_require_lazy_twice_returns_cached`, `src/kit/kit.rs::require_does_not_rebuild_lazy_on_second_call` | tests/e2e/e2e_core.rs |
| REG-11 | Lazy build (first require) failing → `BuildFailed` propagates to the caller | Error | — | None | `tests/e2e/e2e_advanced.rs::e11_lazy_build_fails_on_first_require`, `src/kit/kit.rs::lazy_require_build_error` | tests/e2e/e2e_core.rs |
| REG-12 | `register_multi` aggregates three implementations of one capability type; `require_all` preserves registration order | Normal | — | None | `tests/e2e/e2e_advanced.rs::a04_register_multi_require_all_preserves_order`, `c13_multi_binding_registration_order_preserved`, `src/kit/kit.rs::require_all_preserves_registration_order` | tests/e2e/e2e_core.rs |
| REG-13 | Multi bindings coexist with a single-binding `register`: `require` gets the singleton, `require_all` the aggregate, without interference | Boundary | — | None | `tests/e2e/e2e_advanced.rs::a05_multi_binding_coexists_with_single_binding`, `src/kit/kit.rs::require_all_coexists_with_require_for_single_binding` | tests/e2e/e2e_core.rs |
| REG-14 | `register_multi` on an already-registered module / `register_multi` on a module already `register`ed → `AlreadyRegistered` | Error | — | None | `tests/e2e/e2e_advanced.rs::e06_register_multi_duplicate_returns_already_registered`, `src/kit/kit.rs::register_multi_returns_already_registered_if_already_registered_via_register` | tests/e2e/e2e_core.rs |
| REG-15 | `register_if` predicate true/false branches: true registers, false skips and returns false | Normal | — | None | `src/kit/kit.rs::register_if_true_registers_module`, `register_if_false_skips_module`; examples/conditional (run acceptance) | tests/e2e/e2e_core.rs |
| REG-16 | `override_module` injects a pre-built capability, skipping `build_fn` (module need not be registered first; takes priority over `register_lazy` modules, lazy builder executes zero times); an unregistered override is appended at the end of build, does not enter the graph/contract manifest, and is recorded in `build_report.overrides` as `"(unregistered)"` (a never-registered type) | Normal | — | None | `tests/e2e/e2e_advanced.rs::a06_override_module_skips_build_fn`, `a07_override_module_on_unregistered_module`, `a27_override_module_takes_priority_over_register_lazy`, `src/kit/kit.rs::build_inserts_unregistered_override_after_topo_loop`, `build_override_takes_priority_over_register_lazy`, `src/kit/report.rs::build_report_marks_never_registered_override`, `build_report_keeps_name_for_override_registered_after_call`, `build_report_strict_override_of_unregistered_target`, `tests/e2e/e2e_config.rs::e2e_unregistered_override_outside_graph_and_manifest`, `e2e_override_wins_over_lazy_registration` | tests/e2e/e2e_core.rs |
| REG-17 | `override_module` validates dependencies at injection: unregistered dependency → `DependencyMissing`; with dependencies present it succeeds (the original `override_module_strict` was removed; its behavior merged into `override_module`) | Error | — | None | `tests/e2e/e2e_advanced.rs::e19_override_module_missing_dep_returns_dependency_missing`, `a08_override_module_succeeds_when_deps_registered` | tests/e2e/e2e_core.rs |
| REG-18 | Overrides-table lifecycle: empty after new, cleared after build (consume semantics) | Boundary | — | None | `src/kit/kit.rs::overrides_field_is_empty_on_new`, `overrides_field_is_empty_after_build` | tests/e2e/e2e_core.rs |
| REG-19 | Typestate compile-time violations: calling `build()`/`register()` on `Kit<Ready>` and `optional()` on `Kit<Unbuilt>` all fail to compile (E0119/E0599-class errors, stderr snapshots pinned) | Error | — | None | `tests/compile_fail.rs::compile_fail_tests` (drives all 17 cases in `tests/ui/`: 13 at root — 7 typestate + 6 macro diagnostics — plus 4 gated subdirectories, see Key Finding 4 and §5.1 L2b; 3 cases at authoring time) | tests/ui/ (new UI cases) |
| REG-20 | Scale edges: diamond dependencies, a 10-layer deep chain, and 100-module register+build all succeed in correct order | Boundary | — | None | `tests/e2e/e2e_advanced.rs::c04_diamond_dependency_builds_successfully`, `c03_deep_dependency_chain_10_layers_builds_in_topo_order`, `c06_large_number_of_modules_100_registers_and_builds` | tests/e2e/e2e_core.rs |

### 2.3 Capability Retrieval (CAP, 12)

| ID | Scenario | Type | Features | Services | Existing coverage | E2E target |
|----|---------|------|-------------|---------|---------|---------|
| CAP-01 | `require::<M>()` returns a clone of the built singleton; multiple calls yield equal values | Normal | — | None | `tests/basic.rs::test_basic_build_and_require`, `src/kit/async_kit.rs::async_kit_ready_require_returns_capability` | tests/e2e/e2e_core.rs |
| CAP-02 | `require` on an unregistered/unbuilt module → `MissingCapability{key=NAME}` (same shape on sync and async sides) | Error | — | None | `src/kit/scope.rs::scope_require_unregistered_returns_missing` (same shape), `src/kit/kit.rs::require_ref_returns_missing_capability_for_unbuilt`; sync `require` direct assertion→assertions to add | tests/e2e/e2e_core.rs |
| CAP-03 | `optional`: returns `Some` when built, `None` without error when unbuilt | Normal | — | None | `tests/basic.rs::test_optional_missing`, `tests/e2e/e2e_advanced.rs::b09_optional_returns_some_for_built_module`, `src/kit/kit.rs::ready_optional_returns_none_for_unbuilt` | tests/e2e/e2e_core.rs |
| CAP-04 | `require_ref` returns a zero-copy `Ref` reading the same value as `require` (incl. override values) | Normal | — | None | `tests/e2e/e2e_advanced.rs::b13_require_ref_returns_zero_copy_reference`, `src/kit/kit.rs::require_ref_returns_reference_to_built_capability`, `require_ref_returns_override_value` | tests/e2e/e2e_core.rs |
| CAP-05 | `require_ref` on unbuilt → `MissingCapability` | Error | — | None | `src/kit/kit.rs::require_ref_returns_missing_for_unbuilt` | tests/e2e/e2e_core.rs |
| CAP-06 | `require_all` returns all multi bindings (3) in registration order | Normal | — | None | `src/kit/kit.rs::require_all_returns_vec_of_three_after_three_register_multi` | tests/e2e/e2e_core.rs |
| CAP-07 | `require_all` on an unregistered capability → `MissingCapability` | Error | — | None | `tests/e2e/e2e_advanced.rs::e26_require_all_unregistered_returns_missing_capability`, `src/kit/kit.rs::require_all_returns_empty_for_unregistered_capability` | tests/e2e/e2e_core.rs |
| CAP-08 | Calling `require_all` before `build()` → `MissingCapability` (multi_capabilities filled only after build) | Boundary | — | None | `src/kit/kit.rs::require_all_returns_missing_capability_before_build` | tests/e2e/e2e_core.rs |
| CAP-09 | `contains` / `contains_config` precisely reflect built/set state | Normal | — | None | `tests/e2e/e2e_advanced.rs::b11_contains_reflects_built_state`, `b12_contains_config_reflects_set_state`, `src/kit/kit.rs::ready_contains_returns_false_for_unbuilt` | tests/e2e/e2e_core.rs |
| CAP-10 | `factory::<M>()` produces a brand-new instance per call (non-singleton, provable by counter) | Normal | — | None | `src/kit/kit.rs::factory_creates_new_instance_each_call`, `src/kit/async_kit.rs::async_factory_creates_new_instances`; examples/factory (run acceptance) | tests/e2e/e2e_core.rs |
| CAP-11 | Factory build failure → `BuildFailed{context=NAME}` propagates (Err path inside the closure) | Error | — | None | none→to add | tests/e2e/e2e_core.rs |
| CAP-12 | Using `require` on dependency capabilities inside a build callback (DI injection path) together with config reads | Normal | — | None | `tests/e2e/e2e_advanced.rs::b07_build_callback_reads_config`, `src/kit/kit.rs::require_lazy_with_registered_dependency_succeeds` | tests/e2e/e2e_core.rs |

### 2.4 Dependency Graph (DEP, 10)

| ID | Scenario | Type | Features | Services | Existing coverage | E2E target |
|----|---------|------|-------------|---------|---------|---------|
| DEP-01 | `DependencyGraph::add` inserts in order; `entries()` reflects registration order | Normal | — | None | `src/kit/graph.rs::graph_add_and_entries`, `tests/basic.rs::entries_returns_registration_order` | tests/e2e/e2e_core.rs |
| DEP-02 | Duplicate `add` of the same TypeId → returns `Err(name)` | Error | — | None | `src/kit/graph.rs::graph_add_duplicate_returns_err`, `tests/basic.rs::add_rejects_duplicate_type_id` | tests/e2e/e2e_core.rs |
| DEP-03 | `validate` outputs a valid topological order for an acyclic graph | Normal | — | None | `src/kit/graph.rs::graph_validate_topo_order`, `tests/basic.rs::validate_succeeds_for_acyclic_graph` | tests/e2e/e2e_core.rs |
| DEP-04 | `validate` with a missing dependency → `GraphError::DependencyMissing` | Error | — | None | `src/kit/graph.rs::graph_validate_missing_dependency`, `tests/basic.rs::validate_returns_dependency_missing_for_unknown_dep` | tests/e2e/e2e_core.rs |
| DEP-05 | `validate` cycle detection: two-node/three-node cycles + entering a cycle from an unvisited branch are all detected | Error | — | None | `src/kit/graph.rs::graph_validate_cycle_two_nodes`, `graph_validate_cycle_three_nodes`, `tests/basic.rs::find_cycle_traverses_unvisited_branch` | tests/e2e/e2e_core.rs |
| DEP-06 | Empty graph / single node validate passes | Boundary | — | None | `src/kit/graph.rs::graph_validate_empty_succeeds`, `graph_validate_single_node` | tests/e2e/e2e_core.rs |
| DEP-07 | `name_of` returns None for unknown; `dependency_names` returns empty for unknown modules | Normal | — | None | `src/kit/graph.rs::graph_name_of`, `graph_dependency_names_unknown_returns_empty`, `tests/basic.rs::name_of_returns_registered_name` | tests/e2e/e2e_core.rs |
| DEP-08 | `to_dot` generates Graphviz DOT (valid output for both empty graph and node/edge states) | Normal | — | None | `src/kit/graph.rs::graph_to_dot_with_nodes_and_edges`, `src/kit/kit.rs::graph_dot_returns_valid_string` | tests/e2e/e2e_core.rs |
| DEP-09 | `to_mermaid` generates a flowchart; no node-id collision when module names contain hyphens | Normal | — | None | `src/kit/graph.rs::graph_to_mermaid_hyphen_names_no_collision`, `tests/e2e/e2e_feature_combinations.rs::e2e_no_feature_graph_export` | tests/e2e/e2e_core.rs |
| DEP-10 | `GraphError` Debug/Display formattable (error chain usable) | Boundary | — | None | `src/kit/graph.rs::graph_error_debug` | tests/e2e/e2e_core.rs |

### 2.5 Config Center (CFG, 24)

| ID | Scenario | Type | Features | Services | Existing coverage | E2E target |
|----|---------|------|-------------|---------|---------|---------|
| CFG-01 | `set_config` → `config` roundtrip with clone semantics | Normal | — | None | `tests/basic.rs::test_config_retrieval`, `src/kit/async_kit.rs::async_kit_set_config_stores_value` | tests/e2e/e2e_config.rs |
| CFG-02 | `config::<C>()` on an unset type → `MissingConfig{key=type name}` | Error | — | None | `tests/basic.rs::test_missing_config_error`, `test_missing_config_retrieval`, `src/kit/async_kit.rs::async_kit_config_missing_returns_error` | tests/e2e/e2e_config.rs |
| CFG-03 | A second `set_config` of the same type overwrites (last-write-wins); multiple types do not interfere | Boundary | — | None | `tests/e2e/e2e_advanced.rs::c08_config_type_conflict_last_write_wins`, `b06_config_override_last_write_wins`, `src/kit/async_kit.rs::async_kit_set_config_overwrite` | tests/e2e/e2e_config.rs |
| CFG-04 | `load_config::<C>()` (the `Configurable` bridge to confers' `#[derive(Config)]` `load_sync`) loads and stores | Normal | confers | None | `tests/basic.rs::load_config_stores_value_when_load_succeeds`, `load_config_bridges_to_confers_derive_load_sync`, `real_module_builds_capability_from_config` | tests/e2e/e2e_config.rs |
| CFG-05 | `C::load()` failing → `BuildFailed{context="load_config"}` propagates; nothing stored | Error | confers | None | `tests/basic.rs::load_config_propagates_error_when_load_fails` | tests/e2e/e2e_config.rs |
| CFG-06 | A successful `load_config` overwrites a prior `set_config` value of the same type | Boundary | confers | None | `tests/basic.rs::load_config_overrides_prior_set_config` | tests/e2e/e2e_config.rs |
| CFG-07 | `load_and_validate` stores valid configs that pass validation | Normal | confers | None | `tests/e2e/e2e_feature_combinations.rs::e2e_confers_load_and_validate_ok`, `src/kit/kit.rs::load_and_validate_succeeds_with_valid_config` | tests/e2e/e2e_config.rs |
| CFG-08 | Validation failure → `BuildFailed`, source is a `ValidationError` (aggregating all errors), **and the config is not written** | Error | confers | None | `tests/e2e/e2e_feature_combinations.rs::e2e_confers_load_and_validate_reject`, `src/kit/kit.rs::load_and_validate_does_not_store_on_failure`, `load_and_validate_fails_with_invalid_config` | tests/e2e/e2e_config.rs |
| CFG-09 | After a validation failure, correcting and re-running `load_and_validate` succeeds (failures leave no dirty state) | Boundary | confers | None | `src/kit/kit.rs::load_and_validate_retry_after_failure` | tests/e2e/e2e_config.rs |
| CFG-10 | `load_config_with(vars)`: `${VAR}` / `${VAR:-default}` serialization-level interpolation then storage | Normal | confers | None | `tests/e2e/e2e_feature_combinations.rs::e2e_confers_load_config_with_interpolation` | tests/e2e/e2e_config.rs |
| CFG-11 | `load_config_or_default` both branches: returns true on success; on failure falls to `default_value()` and returns false | Normal | confers | None | `tests/basic.rs::load_config_or_default_uses_loaded_value_when_load_succeeds`, `load_config_or_default_uses_default_when_load_fails`, `load_config_or_default_overrides_prior_set_config` | tests/e2e/e2e_config.rs |
| CFG-12 | `snapshot_config` → modify → `restore_config` rolls back to the snapshot value | Normal | confers | None | `tests/e2e/e2e_feature_combinations.rs::e2e_confers_snapshot_restore`, `src/kit/kit.rs::restore_overwrites_current_config` | tests/e2e/e2e_config.rs |
| CFG-13 | `restore_config` with no snapshot → `MissingConfig` (key is the localized no-snapshot message) | Error | confers | None | `src/kit/kit.rs::restore_returns_error_when_no_snapshot` | tests/e2e/e2e_config.rs |
| CFG-14 | Snapshot edges: `snapshot_config` returns false when no config exists; a repeated snapshot overwrites the old one; `has_snapshot` reflects state precisely | Boundary | confers | None | `src/kit/kit.rs::snapshot_returns_false_when_config_missing`, `snapshot_overwrite_replaces_previous`, `has_snapshot_reflects_state` | tests/e2e/e2e_config.rs |
| CFG-15 | `populate_defaults`: on an empty kit fills defaults and returns true; existing values are not overwritten and it returns false | Normal | confers | None | `tests/e2e/e2e_feature_combinations.rs::e2e_confers_populate_defaults`, `e2e_confers_populate_defaults_noop`, `src/kit/kit.rs::populate_defaults_fills_empty_kit`, `populate_defaults_does_not_overwrite_existing` | tests/e2e/e2e_config.rs |
| CFG-16 | `merge_config::<C>(Override)`: only non-None fields are overridden (the Override type generated by the `ConfigInherit` derive macro) | Normal | confers | None | `tests/config_inherit_derive.rs::derive_config_inherit_works_with_kit_merge_config`, `src/kit/kit.rs::merge_config_overrides_only_some_fields` | tests/e2e/e2e_config.rs |
| CFG-17 | `merge_config` is a no-op without panic when the config is missing; with an all-None Override the original value is unchanged | Boundary | confers | None | `src/kit/kit.rs::merge_config_noop_when_missing`, `tests/config_inherit_derive.rs::derive_config_inherit_default_override_is_all_none` | tests/e2e/e2e_config.rs |
| CFG-18 | `extract_shared` / `inject_shared` shared-field flow: extract from config A → inject into config B | Normal | confers | None | `tests/shared_config_derive.rs::derive_shared_config_kit_extract_inject_flow`, `src/kit/kit.rs::extract_then_inject_shared_flows_values` | tests/e2e/e2e_config.rs |
| CFG-19 | Inject edges: type mismatches skip silently, missing keys skip, extract/inject are dual no-ops when no config exists | Boundary | confers | None | `tests/shared_config_derive.rs::derive_shared_config_inject_skips_type_mismatch`, `derive_shared_config_inject_skips_missing_keys`, `src/kit/kit.rs::inject_shared_noop_when_config_missing`, `extract_shared_noop_when_config_missing`, `inject_shared_skips_type_mismatch_silently` | tests/e2e/e2e_config.rs |
| CFG-20 | `merge_json_deep`: nested recursive merge, scalar replacement, array replacement (not merged), null-overlay semantics (10-case shape matrix) | Normal | confers | None | `src/kit/config.rs::overlay_inserts_new_keys`, `nested_objects_merge_recursively`, `deeply_nested_merge`, `arrays_are_replaced_not_merged`, `null_overlay_replaces_value`, etc., `tests/e2e/e2e_feature_combinations.rs::e2e_confers_merge_json_deep` | tests/e2e/e2e_config.rs |
| CFG-21 | `interpolate_json_value` shape matrix: single/multiple variables, nested objects, default-value hit and ignored, miss keeps original text, non-strings untouched, object keys not replaced, array elements replaced | Boundary | confers | None | `src/kit/kit.rs::basic_var_replacement`, `multiple_vars_in_one_string`, `nested_object_replacement`, `default_value_when_var_missing`, `default_value_ignored_when_var_present`, `no_match_preserved`, `non_string_values_untouched`, `object_keys_not_replaced`, `array_string_elements_replaced` | tests/e2e/e2e_config.rs |
| CFG-22 | Derive macros end-to-end: `#[derive(ConfigInherit)]` (incl. custom override names/nested delegation/pub visibility) and `#[derive(SharedConfig)]`, full chain of the A/B project inheritance scenario | Normal | confers | None | `tests/config_inherit_derive.rs` (6 cases), `tests/shared_config_derive.rs` (6 cases), `tests/config_inheritance_e2e.rs::e2e_config_inheritance_a_b_project_scenario`, `e2e_merge_config_then_shared_inheritance`, `src/kit/kit.rs::config_inheritance_works_on_ready_kit` | tests/e2e/e2e_config.rs |
| CFG-23 | An ultra-deeply nested config (5000 levels) → `load_config_with` returns a catchable `BuildFailed` (the depth guard aborts at the serialization stage; the process is not terminated) | Error | confers | None | `src/kit/kit_tests.rs::deep_config_tests::load_config_with_rejects_deeply_nested_config_instead_of_crashing`, `src/kit/kit.rs::load_config_with_rejects_over_deep_config` | tests/e2e/e2e_config.rs |
| CFG-24 | Credential-type field names (`password` / `api_key`) do not enter the shared namespace: filtered on both extract and inject sides; credential plaintext does not flow across config types | Boundary | confers | None | `tests/shared_config_derive.rs::kit_shared_overlay_does_not_flow_credentials_across_config_types` | tests/e2e/e2e_config.rs |

### 2.6 Hot Reload (RLD, 10)

| ID | Scenario | Type | Features | Services | Existing coverage | E2E target |
|----|---------|------|-------------|---------|---------|---------|
| RLD-01 | `subscribe::<C>(cb)` → `reload_config::<C>()` triggers the callback, which reads the new value | Normal | reload | None | `tests/basic.rs::subscribe_callback_invoked_on_reload`, `tests/e2e/e2e_feature_combinations.rs::e2e_reload_updates_and_notifies` | tests/e2e/e2e_config.rs |
| RLD-02 | `reload_config` updates the stored value (`config::<C>()` returns the new value) | Normal | reload | None | `tests/basic.rs::reload_config_updates_stored_value`, `src/kit/kit.rs::reload_config_updates_value_and_fires_subscribers` | tests/e2e/e2e_config.rs |
| RLD-03 | `C::load()` failing during `reload_config` → `BuildFailed` propagates, old value preserved | Error | reload | None | `tests/basic.rs::reload_config_propagates_load_error` | tests/e2e/e2e_config.rs |
| RLD-04 | Multiple subscribers of the same type are all notified in registration order | Boundary | reload | None | `tests/basic.rs::reload_config_invokes_multiple_subscribers`, `src/kit/kit.rs::reload_config_fires_all_subscribers` | tests/e2e/e2e_config.rs |
| RLD-05 | `reload_config` succeeds as a no-op with no subscribers | Boundary | reload | None | `tests/basic.rs::subscribe_and_reload_with_no_subscribers_succeeds` | tests/e2e/e2e_config.rs |
| RLD-06 | Subscriber panic semantics pinned: the new value is stored and remaining subscribers are skipped (the panic pierces `reload_config`, documented behavior) | Error | reload | None | none→to add | tests/e2e/e2e_config.rs |
| RLD-07 | Calling `reload_config` while a `require_ref` borrow is alive. **Real-behavior correction** (verified at landing): `configs` and `capabilities` are two independent `TypeMap`s (`src/kit/kit.rs`); `reload_config` writing config while the borrow is alive does not touch the capability borrow and works normally, no borrow conflict; also pins `set_config` as an Unbuilt-state method (Ready-state config-write paths include `reload_config` / `merge_config` / `populate_defaults`) to prevent a hidden panic if the two TypeMaps are ever merged | Boundary | reload | None | `src/kit/typemap.rs::inner_ref_panics_if_mutably_borrowed` (existing at the TypeMap layer) | tests/e2e/e2e_config.rs::e2e_require_ref_borrow_survives_reload_and_set_config + tests/e2e/e2e_concurrency.rs |
| RLD-08 | Feature-chain mapping: `reload` automatically enables `confers` (the dead `confers/watch` link was pruned); `subscribe/reload_config` work via Kit's own SubscriberMap | Normal | reload | None | `Cargo.toml` declaration verification + `tests/e2e/e2e_feature_combinations.rs::e2e_confers_plus_reload` | tests/e2e/e2e_feature_combinations.rs |
| RLD-09 | examples/hot_reload run acceptance: change value → reload → callback prints the new value | Normal | reload | None | examples/hot_reload (run acceptance) | tests/e2e/e2e_feature_combinations.rs (references the example) |
| RLD-10 | **AsyncKit subscriber panic isolation** (contrast with RLD-06's piercing semantics): registering a panicking callback first and a flag-setting callback second for the same type, `AsyncKit::reload_config` still returns `Ok`, the flag-setting callback is still notified, and the new value remains stored. Async-side registration bookkeeping is dispatched via confers `DynamicField`; a single subscriber's panic is isolated and counted, not piercing the caller; the sync Kit's RLD-06 panic-piercing semantics are unaffected | Error | async,reload | None | `src/kit/async_kit.rs::async_reload_tests::reload_config_isolates_panicking_subscriber`, `reload_config_callback_reads_new_value_via_config` | src/kit/async_kit.rs (async RLD-10) + tests/e2e/e2e_config.rs (sync RLD-06) |

### 2.7 Encrypted Storage (ENC, 14)

| ID | Scenario | Type | Features | Services | Existing coverage | E2E target |
|----|---------|------|-------------|---------|---------|---------|
| ENC-01 | `set_encrypted` → `get_encrypted` roundtrip with equal values | Normal | encryption | None | `tests/basic.rs::encrypted_config_roundtrip`, `tests/e2e/e2e_feature_combinations.rs::e2e_encryption_roundtrip`, `src/kit/kit.rs::set_and_get_encrypted_roundtrip` | tests/e2e/e2e_encryption.rs |
| ENC-02 | Decrypting with a wrong master_key → `BuildFailed` (decryption failure), no plaintext returned; tampered ciphertext has the same shape (wrong-key analog) | Error | encryption | None | `tests/basic.rs::get_encrypted_fails_with_wrong_key`, `tests/e2e/e2e_advanced.rs::e17_tampered_ciphertext_analog_via_wrong_key`, `src/kit/kit.rs::get_encrypted_wrong_key_returns_error` | tests/e2e/e2e_encryption.rs |
| ENC-03 | `get_encrypted` without a prior `set_encrypted` → `MissingConfig` | Error | encryption | None | `tests/basic.rs::get_encrypted_returns_missing_config_error_when_not_set`, `src/kit/kit.rs::get_encrypted_missing_returns_error` | tests/e2e/e2e_encryption.rs |
| ENC-04 | A second `set_encrypted` of the same type overwrites the old ciphertext (new value decryptable, old value no longer retrievable) | Boundary | encryption | None | `tests/basic.rs::set_encrypted_overwrites_prior_value` | tests/e2e/e2e_encryption.rs |
| ENC-05 | Encrypted storage is isolated from the plaintext TypeMap: `contains_encrypted` reflects only the encrypted side; encrypted values do not appear in `config::<C>()` | Boundary | encryption | None | `tests/basic.rs::encrypted_storage_is_separate_from_plaintext_typemap`, `src/kit/kit.rs::contains_encrypted_false_for_missing` | tests/e2e/e2e_encryption.rs |
| ENC-06 | A non-serializable value → `set_encrypted` propagates the serialization error as `BuildFailed` | Error | encryption | None | `tests/basic.rs::set_encrypted_propagates_serialization_error` | tests/e2e/e2e_encryption.rs |
| ENC-07 | master_key < 32 bytes → rejected immediately on both set and get sides (InvalidInput, message includes the actual length) | Error | encryption | None | `tests/e2e/e2e_feature_combinations.rs::e2e_encryption_short_key_rejected`, `e2e_encryption_get_short_key_rejected` | tests/e2e/e2e_encryption.rs |
| ENC-08 | Empty master_key (0 bytes) behavior pinned (also takes the <32 rejection branch) | Boundary | encryption | None | `tests/e2e/e2e_advanced.rs::c14_empty_master_key_encryption_behavior` | tests/e2e/e2e_encryption.rs |
| ENC-09 | Constant-byte master_key (all-zero / repeated bytes) → rejected on both set and get sides, consistent with the `WeakKey` policy of the provider supply path | Error | encryption | None | `src/kit/kit.rs::weak_master_key_is_rejected_on_set_and_get`, `src/kit/async_kit.rs::weak_master_key_is_rejected` | tests/e2e/e2e_encryption.rs |
| ENC-10 | `EncryptedBlob`: nonce/ciphertext getters return raw slices, empty blob, clone equality, **Debug redaction** (no ciphertext leak) | Normal | encryption | None | `src/kit/config.rs::getters_return_raw_slices`, `getters_return_empty_for_empty_blob`, `clone_produces_equal_blob`, `debug_format_redacts_sensitive_data`, `tests/basic.rs::encrypted_blob_getters_via_roundtrip` | tests/e2e/e2e_encryption.rs |
| ENC-11 | Field-key HKDF binds `C::PATH` + the version label `v1`: different PATHs derive different keys; the HKDF failure path maps to `BuildFailed` (documented) | Normal | encryption | None | `src/kit/kit.rs` (`KEY_DERIVATION_VERSION`/`derive_kit_field_key`), `tests/e2e/e2e_advanced.rs::e15_hkdf_failure_path_documented`; different-PATH isolation assertion→assertions to add | tests/e2e/e2e_encryption.rs |
| ENC-12 | The confers `XChaCha20Crypto` / `derive_field_key` re-exports are usable (calling the crypto primitives directly from the trait-kit surface) | Normal | encryption | None | examples/encryption (run acceptance) → library-level direct assertions to add | tests/e2e/e2e_encryption.rs |
| ENC-13 | confers+encryption combination: plaintext `load_config` and ciphertext `set_encrypted` coexist in one Kit without interference | Normal | confers,encryption | None | `tests/e2e/e2e_feature_combinations.rs::e2e_confers_plus_encryption` | tests/e2e/e2e_feature_combinations.rs |
| ENC-14 | 1MB large-config encryption roundtrip (the XChaCha20 throughput path) | Boundary | encryption | None | `tests/e2e/e2e_advanced.rs::c15_large_config_value_encryption_1mb` | tests/e2e/e2e_encryption.rs |

### 2.8 Interface/Implementation Separation (ITF, 9)

| ID | Scenario | Type | Features | Services | Existing coverage | E2E target |
|----|---------|------|-------------|---------|---------|---------|
| ITF-01 | `register_as::<M>()` → `build()` → `resolve::<I>()` returns `Arc<dyn Trait>` | Normal | di | None | `src/kit/kit.rs::register_as_then_resolve_returns_arc_dyn_trait`, `tests/e2e/e2e_advanced.rs::a19_register_as_then_resolve`, `tests/e2e/e2e_feature_combinations.rs::e2e_interface_register_and_resolve` | tests/e2e/e2e_features.rs |
| ITF-02 | A second `register_as` for the same interface type → `AlreadyRegistered` (one interface, one implementation) | Error | di | None | `src/kit/kit.rs::register_as_twice_same_interface_returns_already_registered`, `tests/e2e/e2e_advanced.rs::e07_duplicate_interface_registration_returns_already_registered` | tests/e2e/e2e_features.rs |
| ITF-03 | `resolve` before `build()` → `MissingCapability` | Error | di | None | `src/kit/kit.rs::resolve_before_build_returns_missing_capability` | tests/e2e/e2e_features.rs |
| ITF-04 | `resolve` on an unregistered interface → `MissingCapability` | Error | di | None | `src/kit/kit.rs::resolve_unregistered_interface_returns_missing_capability`, `tests/e2e/e2e_advanced.rs::e27_resolve_unregistered_interface_returns_missing_capability` | tests/e2e/e2e_features.rs |
| ITF-05 | The resolved `Arc<dyn Trait>` can call trait methods directly (dynamic dispatch works) | Normal | di | None | `src/kit/kit.rs::resolve_returns_callable_trait_object`, `file_logger_interface_build_and_resolve` | tests/e2e/e2e_features.rs |
| ITF-06 | `register_as` coexists with `register` (single binding)/multi without interference | Boundary | di | None | `src/kit/kit.rs::register_as_coexists_with_register`, `register_as_builds_during_build` | tests/e2e/e2e_features.rs |
| ITF-07 | The same module type registered twice via `register_as` (judged by module TypeId) → `AlreadyRegistered` | Error | di | None | `src/kit/kit.rs::register_as_same_module_twice_returns_already_registered` | tests/e2e/e2e_features.rs |
| ITF-08 | `InterfaceBuilder::build` failure → `BuildFailed{context=interface}` propagates | Error | di | None | `src/kit/kit.rs::interface_build_error_returns_build_failed` | tests/e2e/e2e_features.rs |
| ITF-09 | The `Interface` blanket impl lets any concrete type serve directly as a capability convertible via `into_interface` (`?Sized` bound edge) | Boundary | di | None | `src/core/meta.rs::interface_builder_capability_is_clone`, `interface_builder_build_returns_concrete_capability` | tests/e2e/e2e_features.rs |

### 2.9 Lifecycle (LCY, 9)

| ID | Scenario | Type | Features | Services | Existing coverage | E2E target |
|----|---------|------|-------------|---------|---------|---------|
| LCY-01 | After `register_lifecycle::<M>()`, `build()` automatically calls `on_ready` at the completion stage | Normal | lifecycle | None | `src/kit/kit.rs::lifecycle_on_ready_called_during_build`, `tests/e2e/e2e_feature_combinations.rs::e2e_lifecycle_on_ready_after_build` | tests/e2e/e2e_features.rs |
| LCY-02 | `Kit<Ready>::shutdown()` calls `on_shutdown` in **reverse build order** (later-built closes first) | Normal | lifecycle | None | `src/kit/kit.rs::lifecycle_shutdown_called_in_reverse_order`, `tests/e2e/e2e_feature_combinations.rs::e2e_lifecycle_on_shutdown_on_explicit_shutdown` | tests/e2e/e2e_features.rs |
| LCY-03 | `on_ready` returning `Err` → `build()` fails overall, `LifecycleFailed{context=NAME}` propagates | Error | lifecycle | None | `src/kit/kit.rs::lifecycle_on_ready_failure_propagates` | tests/e2e/e2e_features.rs |
| LCY-04 | `Lifecycle` default implementation: `on_ready`/`on_shutdown` default to no-op | Normal | lifecycle | None | `src/core/lifecycle.rs::lifecycle_trait_has_default_on_ready`, `lifecycle_trait_has_default_on_shutdown`, `lifecycle_shutdown_counter_increments` | tests/e2e/e2e_features.rs |
| LCY-05 | One module's `on_shutdown` failure does not block other modules from shutting down (documented contract: "A failed shutdown does not prevent other modules") → behavior-level assertion missing | Error | lifecycle | None | none→to add | tests/e2e/e2e_features.rs |
| LCY-06 | `AsyncLifecycle` (async+lifecycle): the async on_ready is called after build | Normal | lifecycle,async | None | `src/kit/async_kit.rs::async_lifecycle_on_ready_called`, `async_lifecycle_test_module_full_kit_integration`, `src/core/lifecycle.rs::async_lifecycle_default_on_ready_returns_ok` | tests/e2e/e2e_async.rs |
| LCY-07 | Multiple modules' on_ready execute in topological order (dependents' ready after dependencies') | Boundary | lifecycle | None | `src/core/lifecycle.rs::lifecycle_test_module_full_kit_integration` (partial) → explicit order assertion to add | tests/e2e/e2e_features.rs |
| LCY-08 | lifecycle+health combination: health checks available after ready | Boundary | lifecycle,health | None | `tests/e2e/e2e_feature_combinations.rs::e2e_lifecycle_plus_health` | tests/e2e/e2e_feature_combinations.rs |
| LCY-09 | scope+lifecycle combination: in-scope builds do not disturb lifecycle hook semantics | Boundary | request-scope,lifecycle | None | `tests/e2e/e2e_feature_combinations.rs::e2e_scope_plus_lifecycle` | tests/e2e/e2e_feature_combinations.rs |

### 2.10 Health Checks (HLT, 9)

| ID | Scenario | Type | Features | Services | Existing coverage | E2E target |
|----|---------|------|-------------|---------|---------|---------|
| HLT-01 | `HealthStatus` three-state constructors (Healthy/`degraded`/`unhealthy`), `is_healthy`, Clone/Eq/Debug | Normal | health | None | `src/core/health.rs::health_status_is_healthy`, `health_status_clone_and_eq`, `health_status_debug_format`, `health_status_ne_eq`, `tests/e2e/e2e_feature_combinations.rs::e2e_health_status_constructors` | tests/e2e/e2e_features.rs |
| HLT-02 | `register_health_check::<M>()` → `health_check::<M>()` query returns the module's self-reported status | Normal | health | None | `src/kit/kit.rs::health_check_registered_and_queryable`, `tests/e2e/e2e_feature_combinations.rs::e2e_health_single_module_check` | tests/e2e/e2e_features.rs |
| HLT-03 | `health_report()` aggregates all registered checkers (all-Healthy and mixed for multiple modules) | Normal | health | None | `src/kit/kit.rs::health_report_returns_all_checkers`, `tests/e2e/e2e_feature_combinations.rs::e2e_health_all_healthy`, `e2e_health_mixed` | tests/e2e/e2e_features.rs |
| HLT-04 | `health_check::<M>()` with no registered checker → `MissingConfig{key=NAME}` | Error | health | None | `src/kit/kit.rs::health_check_unregistered_returns_error` | tests/e2e/e2e_features.rs |
| HLT-05 | A `check` returning the Unhealthy branch (e.g. a zero value) is reported truthfully | Error | health | None | `src/kit/kit.rs::health_check_unhealthy_for_zero_value`, `src/core/health.rs::health_check_returns_unhealthy_for_zero_value` | tests/e2e/e2e_features.rs |
| HLT-06 | `HealthStatus::degraded(detail)` carries detail with `is_healthy()==false` (degraded ≠ faulty semantics) | Boundary | health | None | the `src/core/health.rs::health_check_returns_unhealthy_for_zero_value` family + the `HealthStatus::degraded` constructor (the `health_status_*` group) | tests/e2e/e2e_features.rs |
| HLT-07 | A checker closure that cannot find the capability returns `Unhealthy("capability not found")` (defensive branch) | Error | health | None | none→to add | tests/e2e/e2e_features.rs |
| HLT-08 | `AsyncHealthCheck` (async+health): async check queryable, report aggregates, unregistered errors | Normal | health,async | None | `src/kit/async_kit.rs::async_health_check_queryable`, `async_health_report_returns_all`, `async_health_check_unregistered_returns_error`, `src/core/health.rs::async_health_check_returns_healthy`, `_unhealthy` | tests/e2e/e2e_async.rs |
| HLT-09 | The `AsyncKit` report semantics for the health+async combination match the sync side (isomorphism) | Boundary | health,async | None | `src/kit/async_kit.rs` (async health group) → isomorphism comparison assertion to add | tests/e2e/e2e_async.rs |

### 2.11 Build Observers (OBS, 7)

| ID | Scenario | Type | Features | Services | Existing coverage | E2E target |
|----|---------|------|-------------|---------|---------|---------|
| OBS-01 | The `BuildObserver` trait object is safe and `Send + Sync` (shareable across threads as `Arc`) | Normal | observer | None | `src/core/observer.rs::observer_trait_is_object_safe`, `observer_is_send_sync` | tests/e2e/e2e_features.rs |
| OBS-02 | After `with_observer`, `build()` invokes the `on_module_start` / `on_module_built` callbacks in sequence (with timing arguments) | Normal | observer | None | `src/kit/kit.rs::observer_callbacks_fired_during_build`, `tests/e2e/e2e_feature_combinations.rs::e2e_observer_notified_on_build` | tests/e2e/e2e_features.rs |
| OBS-03 | A module build failure → `on_build_error(module_name, &BuildFailed)` is called, then the error continues to propagate | Error | observer | None | `src/kit/kit.rs::observer_on_build_error_called_on_failure`, `src/kit/async_kit.rs::async_observer_on_build_error_called` | tests/e2e/e2e_features.rs |
| OBS-04 | `DefaultObserver`'s three callbacks default to no-op (including the dyn-dispatch form) without panicking | Boundary | observer | None | `src/core/observer.rs::observer_default_on_module_start_does_not_panic`, `_on_module_built_…`, `_on_build_error_…`, `observer_default_via_dyn_dispatch` | tests/e2e/e2e_features.rs |
| OBS-05 | With multiple observers registered, all receive the same event (iteration order == registration order) → assertion missing | Boundary | observer | None | none→to add | tests/e2e/e2e_features.rs |
| OBS-06 | observer+decorator combination: callbacks observe the post-decoration capability build (order: build → decorate → on_module_built) | Normal | observer,decorator | None | `tests/e2e/e2e_feature_combinations.rs::e2e_observer_plus_decorator` | tests/e2e/e2e_feature_combinations.rs |
| OBS-07 | Observer callbacks on the `AsyncKit` surface (same shape on the async build path) | Normal | observer,async | None | `src/kit/async_kit.rs::async_observer_callbacks_fired` | tests/e2e/e2e_async.rs |

### 2.12 Decorators (DEC, 9)

| ID | Scenario | Type | Features | Services | Existing coverage | E2E target |
|----|---------|------|-------------|---------|---------|---------|
| DEC-01 | After `decorate::<M>(f)`, the capability produced by `build()` is wrapped (e.g. a counting proxy visible outside) | Normal | decorator | None | `tests/e2e/e2e_feature_combinations.rs::e2e_decorator_wraps_capability`, `src/kit/kit.rs::decorate_registers_decorator` | tests/e2e/e2e_features.rs |
| DEC-02 | Building without a decorator registered is a no-op, capability as-is (zero-overhead path) | Boundary | decorator | None | `tests/e2e/e2e_feature_combinations.rs::e2e_decorator_no_op_when_absent` | tests/e2e/e2e_features.rs |
| DEC-03 | Multiple decorators on one capability layer onion-style in registration order (f1 first, f2 later → f2 wraps f1) → order assertion missing | Boundary | decorator | None | none→to add | tests/e2e/e2e_features.rs |
| DEC-04 | Decorators cover all four build paths: eager / lazy (first require) / multi-binding / interface → only eager has an assertion | Boundary | decorator | None | the `src/kit/kit.rs::e2e` surface only has eager (`e2e_decorator_wraps_capability`) → lazy/multi/interface paths to add | tests/e2e/e2e_features.rs |
| DEC-05 | `decorate` keys on the capability TypeId and maintains a module→capability map (`decorator_module_to_cap`) for build-time lookup; each decorator record has an owner module, and a decorated module only applies its own decorators | Normal | decorator | None | `src/kit/kit.rs::decorate_registers_decorator` (registration surface), `src/kit/kit_tests.rs::decorator_owner_isolation_tests` (owner isolation) | tests/e2e/e2e_features.rs |
| DEC-06 | toggle+decorator combination: when the toggle is off the module is not registered and decorators do not apply; once on, wrapping takes effect | Normal | decorator,toggle | None | `tests/e2e/e2e_feature_combinations.rs::e2e_toggle_plus_decorator` | tests/e2e/e2e_feature_combinations.rs |
| DEC-07 | Decorator closure panic / internal downcast failure → documented panic semantics (`expect("decorator type mismatch")`) pinned | Error | decorator | None | none→to add | tests/e2e/e2e_features.rs |
| DEC-08 | `AsyncKit::decorate` (async build-path decoration) behaves identically to sync | Normal | decorator,async | None | no async decorate test in src→to add (contrast with examples/decorator semantics) | tests/e2e/e2e_async.rs |
| DEC-09 | Decorator owner isolation on the multi-binding path: other bindings of the same capability type are not wrapped by someone else's decorator (same policy as eager / lazy / interface) | Boundary | decorator | None | `src/kit/kit_tests.rs::decorator_owner_isolation_tests::multi_binding_decorator_only_applies_to_its_own_binding`, `tests/e2e/e2e_features.rs::e2e_decorator_applies_on_multi_binding_path` | tests/e2e/e2e_features.rs |

### 2.13 Scopes (SCP, 10)

| ID | Scenario | Type | Features | Services | Existing coverage | E2E target |
|----|---------|------|-------------|---------|---------|---------|
| SCP-01 | `Scope::register::<M>()` → `require::<M>()` retrieves an in-scope instance | Normal | request-scope | None | `src/kit/scope.rs::scope_register_then_require`, `tests/e2e/e2e_feature_combinations.rs::e2e_scope_isolation` | tests/e2e/e2e_runtime.rs |
| SCP-02 | A second `Scope::require` returns the cache (per-scope singleton, same LazySlot implementation) | Boundary | request-scope | None | `src/kit/scope.rs::scope_require_caches_result` | tests/e2e/e2e_runtime.rs |
| SCP-03 | `Scope::register` on an already-registered module → error | Error | request-scope | None | `src/kit/scope.rs::scope_register_duplicate_returns_error` | tests/e2e/e2e_runtime.rs |
| SCP-04 | `Scope::require` on an unregistered module → `MissingCapability` | Error | request-scope | None | `src/kit/scope.rs::scope_require_unregistered_returns_missing` | tests/e2e/e2e_runtime.rs |
| SCP-05 | Two scopes do not leak into each other: a module registered in A is invisible in B | Boundary | request-scope | None | `src/kit/scope.rs::scope_registrations_do_not_leak_across_scopes`, `tests/e2e/e2e_feature_combinations.rs::e2e_scope_isolation` | tests/e2e/e2e_runtime.rs |
| SCP-06 | Resources are cleared when `Scope` drops (`scope_drop_clears_resources`) | Boundary | request-scope | None | `src/kit/scope.rs::scope_drop_clears_resources` | tests/e2e/e2e_runtime.rs |
| SCP-07 | `Kit<Ready>::create_scope()` returns an empty scope, independent of Kit capabilities | Normal | request-scope | None | `src/kit/kit.rs::create_scope_returns_empty_scope`, `tests/e2e/e2e_feature_combinations.rs::e2e_scope_create_empty` | tests/e2e/e2e_runtime.rs |
| SCP-08 | `AsyncScope` (scope+async): the full insert/require/contains chain | Normal | request-scope,async | None | `src/kit/scope.rs::async_scope_insert_then_contains`, `async_scope_insert_and_require`, `src/kit/async_kit.rs::async_create_scope_returns_empty` | tests/e2e/e2e_async.rs |
| SCP-09 | `AsyncScope` error surface: a duplicate insert overwrites the old value, a missing require errors | Error | request-scope,async | None | `src/kit/scope.rs::async_scope_insert_twice_replaces_value`, `async_scope_require_missing_returns_error` | tests/e2e/e2e_async.rs |
| SCP-10 | scope+lifecycle combination: whether scope instances participate (or not) in Kit-level shutdown, semantics pinned | Boundary | request-scope,lifecycle | None | `tests/e2e/e2e_feature_combinations.rs::e2e_scope_plus_lifecycle` | tests/e2e/e2e_feature_combinations.rs |

### 2.14 Feature Toggles (TGL, 9)

| ID | Scenario | Type | Features | Services | Existing coverage | E2E target |
|----|---------|------|-------------|---------|---------|---------|
| TGL-01 | `enable_toggle(key, true/false)` → `is_toggle_enabled` state correct; multiple keys independent | Normal | toggle | None | `tests/e2e/e2e_feature_combinations.rs::e2e_toggle_enable_query`, `e2e_toggle_multiple`, `src/kit/kit.rs::enable_toggle_sets_value` | tests/e2e/e2e_runtime.rs |
| TGL-02 | Querying an unknown key returns false (no panic, no error) | Boundary | toggle | None | `tests/e2e/e2e_feature_combinations.rs::e2e_toggle_default_disabled`, `src/kit/kit.rs::is_toggle_enabled_returns_false_for_unknown` | tests/e2e/e2e_runtime.rs |
| TGL-03 | `register_if_toggle`: registers and returns true when enabled; skips and returns false when disabled (both branches) | Normal | toggle | None | `src/kit/kit.rs::register_if_toggle_registers_when_enabled`, `register_if_toggle_skips_when_disabled`; examples/toggle_basic (run acceptance) | tests/e2e/e2e_runtime.rs |
| TGL-04 | `register_if_toggle` enabled but the module already registered → `AlreadyRegistered` | Error | toggle | None | `src/kit/kit.rs::register_if_toggle_returns_error_on_duplicate` | tests/e2e/e2e_runtime.rs |
| TGL-05 | Toggle state persists across `build()` (set during Unbuilt → still readable during Ready) | Boundary | toggle | None | `tests/e2e/e2e_feature_combinations.rs::e2e_toggle_persists_after_build`, `src/kit/kit.rs::toggle_state_survives_build` | tests/e2e/e2e_runtime.rs |
| TGL-06 | `enable_toggle` / `is_toggle_enabled` remain callable on `Kit<Ready>` (runtime dynamic toggling) | Normal | toggle | None | `src/kit/kit.rs::toggle_enable_on_ready_state` | tests/e2e/e2e_runtime.rs |
| TGL-07 | Toggle off → the module's capability is not retrievable (`require` reports missing/`contains` false); retrievable once on | Boundary | toggle | None | `src/kit/kit.rs::toggle_disabled_capability_not_retrievable` | tests/e2e/e2e_runtime.rs |
| TGL-08 | toggle+scope combination: toggle-gated registration within a scope (no existing assertion) | Boundary | toggle,request-scope | None | none→to add | tests/e2e/e2e_feature_combinations.rs |
| TGL-09 | Structural verification: `src/kit/toggle.rs` is a complete toggle implementation (609 lines; 10 lines of doc-only at rc.2 authoring, fully landed since rc.4 — the original "no standalone API surface" verification premise is obsolete); `ToggleBackend` / `MemoryToggle` / `ConfersToggle` / `ToggleValue` / `ToggleKey` / `ToggleHandle` are exported by `src/kit/mod.rs` | Boundary | toggle | None | `src/kit/toggle.rs` (609 lines) + `src/kit/mod.rs` (export verification) | tests/e2e/e2e_presets.rs (compile surface) |

### 2.15 Graceful Shutdown (SHD, 13)

| ID | Scenario | Type | Features | Services | Existing coverage | E2E target |
|----|---------|------|-------------|---------|---------|---------|
| SHD-01 | Three phases execute their hooks in `StopRequests → DrainQueue → CloseConnections` order | Normal | shutdown | None | `src/kit/shutdown.rs::shutdown_coordinator_executes_hooks_in_order`, `shutdown_coordinator_phase_order_is_correct`, `tests/e2e/e2e_feature_combinations.rs::e2e_shutdown_all_phases_execute` | tests/e2e/e2e_runtime.rs |
| SHD-02 | `ShutdownPhase::all_phases()` ordered triple, `as_str()` readable names (stop_requests/drain_queue/close_connections) | Normal | shutdown | None | `src/kit/shutdown.rs::shutdown_phase_all_phases_returns_three`, `shutdown_phase_as_str_returns_readable_name` | tests/e2e/e2e_runtime.rs |
| SHD-03 | Phase timeout: a single hook timing out → skips the phase's remaining hooks and moves to the next phase, result marked timed out | Error | shutdown | None | `src/kit/shutdown.rs::shutdown_coordinator_timeout_skips_remaining_hooks` | tests/e2e/e2e_runtime.rs |
| SHD-04 | Global timeout: total duration exceeding `set_global_timeout` → immediately aborts subsequent phases | Error | shutdown | None | `src/kit/shutdown.rs::shutdown_coordinator_global_timeout`, `tests/e2e/e2e_feature_combinations.rs::e2e_shutdown_set_timeouts` | tests/e2e/e2e_runtime.rs |
| SHD-05 | `ShutdownPhaseResult::is_ok` / `ShutdownResult::is_ok` / `timed_out_phases` precisely reflect each phase's outcome | Normal | shutdown | None | `src/kit/shutdown.rs::shutdown_result_into_result_ok`, `ShutdownPhaseResult` field assertions (same-file group) | tests/e2e/e2e_runtime.rs |
| SHD-06 | `into_result()` in the timeout state → `Err(ShutdownTimedOut{phases})`, error message includes all timed-out phase names | Error | shutdown | None | `src/kit/shutdown.rs::shutdown_result_into_result_timeout`; a `TraitKitError::ShutdownTimedOut` Display assertion→to add (see ERR-07) | tests/e2e/e2e_runtime.rs |
| SHD-07 | `shutdown()` succeeds with no hooks/empty phases | Boundary | shutdown | None | `src/kit/shutdown.rs::shutdown_coordinator_empty_phases_succeed`, `tests/e2e/e2e_feature_combinations.rs::e2e_shutdown_empty_phases_succeed` | tests/e2e/e2e_runtime.rs |
| SHD-08 | `ShutdownCoordinator::new()`/`default()` directly usable (default timeout configuration) | Boundary | shutdown | None | `src/kit/shutdown.rs::shutdown_coordinator_default_works` | tests/e2e/e2e_runtime.rs |
| SHD-09 | Non-reentrant: a second `shutdown()` is a no-op against the emptied hook table (drain semantics) | Boundary | shutdown | None | `src/kit/shutdown.rs::shutdown_coordinator_hooks_not_reentrant` | tests/e2e/e2e_runtime.rs |
| SHD-10 | `AsyncShutdownCoordinator` (shutdown+async): async hooks execute in order across all phases | Normal | shutdown,async | None | `src/kit/shutdown.rs::async_shutdown_coordinator_executes_hooks` | tests/e2e/e2e_async.rs |
| SHD-11 | Async timeout: a suspended hook triggers phase-timeout abort | Error | shutdown,async | None | `src/kit/shutdown.rs::async_shutdown_coordinator_timeout` | tests/e2e/e2e_async.rs |
| SHD-12 | Async default construction and non-reentrant semantics | Boundary | shutdown,async | None | `src/kit/shutdown.rs::async_shutdown_coordinator_default`, `async_shutdown_coordinator_hooks_not_reentrant` | tests/e2e/e2e_async.rs |
| SHD-13 | shutdown+decorator combination: when a decorated module shuts down, the decorator layer releases before the core layer (or in reverse registration order), semantics pinned | Boundary | shutdown,decorator | None | none→to add | tests/e2e/e2e_feature_combinations.rs |

### 2.16 Async Kit (ASK, 15)

| ID | Scenario | Type | Features | Services | Existing coverage | E2E target |
|----|---------|------|-------------|---------|---------|---------|
| ASK-01 | `AsyncKit::new()` → `register` → `build().await` → `AsyncKit<Ready>` typestate full chain | Normal | async | None | `tests/e2e/e2e_advanced.rs::a09_async_kit_basic_flow`, `src/kit/async_kit.rs::async_kit_build_returns_ready_state`, `async_kit_new_returns_unbuilt_state` | tests/e2e/e2e_async.rs |
| ASK-02 | Multiple modules build asynchronously in topological order | Normal | async | None | `src/kit/async_kit.rs::async_kit_build_multiple_modules_in_topo_order` | tests/e2e/e2e_async.rs |
| ASK-03 | Async missing dependency → `DependencyMissing` | Error | async | None | `src/kit/async_kit.rs::async_kit_build_missing_dependency_returns_error`, `tests/e2e/e2e_advanced.rs::e24_async_dependency_missing_returns_dependency_missing` | tests/e2e/e2e_async.rs |
| ASK-04 | Async cycle detection → `CycleDetected` (two-node and three-node) | Error | async | None | `src/kit/async_kit.rs::async_kit_build_cycle_returns_error`, `async_kit_di_three_node_cycle_returns_error`, `tests/e2e/e2e_advanced.rs::e25_async_cycle_detected` | tests/e2e/e2e_async.rs |
| ASK-05 | `AsyncAutoBuilder::build`'s async body is genuinely awaited (the async build_fn takes effect) | Normal | async | None | `src/kit/async_kit.rs::async_kit_build_calls_async_build_fn` | tests/e2e/e2e_async.rs |
| ASK-06 | Async build failure → `BuildFailed` propagates | Error | async | None | `src/kit/async_kit.rs::async_kit_build_propagates_build_error`, `tests/e2e/e2e_advanced.rs::e23_async_build_fails_returns_build_failed` | tests/e2e/e2e_async.rs |
| ASK-07 | Ready-state `require` / `optional` / `contains` / `contains_config` quartet isomorphic to sync | Normal | async | None | `src/kit/async_kit.rs::async_kit_ready_require_returns_capability`, `async_kit_ready_optional_returns_some_for_built`, `async_kit_ready_contains_returns_true_for_built`, `async_kit_ready_contains_config_returns_true` (and the corresponding false groups) | tests/e2e/e2e_async.rs |
| ASK-08 | Cross-module DI: dependency capability injection + transitive chain (A→B→C) | Normal | async | None | `tests/e2e/e2e_advanced.rs::a10_async_cross_module_di`, `a11_async_transitive_chain_di`, `src/kit/async_kit.rs::async_kit_di_dependency_built_before_dependent`, `async_kit_di_transitive_dependency_chain` | tests/e2e/e2e_async.rs |
| ASK-09 | `set_config` / `config` on Unbuilt and config reads inside build callbacks (`Send+Sync`-constrained edition) | Normal | async | None | `tests/e2e/e2e_advanced.rs::a12_async_config_read_in_build`, `tests/e2e/e2e_feature_combinations.rs::e2e_async_kit_config_ops` | tests/e2e/e2e_async.rs |
| ASK-10 | Ready-state `require` on an unregistered module → `BuildFailed` (async-surface error policy) | Error | async | None | `tests/e2e/e2e_advanced.rs::c19_async_build_require_unregistered_returns_build_failed`, `src/kit/async_kit.rs::async_kit_ready_require_missing_returns_error` | tests/e2e/e2e_async.rs |
| ASK-11 | Concurrency guarantees: `AsyncKit`/`AsyncKit<Ready>`/`TraitKitError` are all `Send + Sync` (static_assertions compile-time assertions) | Boundary | async | None | `src/kit/async_kit.rs::async_kit_is_send_sync`, `async_kit_build_result_is_send`, `async_kit_error_is_send` | tests/e2e/e2e_async.rs |
| ASK-12 | Concurrent registration of multiple modules without deadlock, all buildable | Boundary | async | None | `src/kit/async_kit.rs::async_kit_concurrent_registration` | tests/e2e/e2e_async.rs |
| ASK-13 | Async cancellation: dropping the build future leaves no half-updated state (a retried build can succeed, cancel-safe semantics) → semantics not pinned | Error | async | None | none→to add | tests/e2e/e2e_async.rs |
| ASK-14 | Full `AsyncTypeMap` operation matrix: insert/get/overwrite/contains/read_by_type_id (three states)/len/default/cross-thread/Arc sharing/Debug | Normal | async | None | `src/kit/async_typemap.rs` (16 cases, incl. `cross_thread_access_does_not_panic`, `arc_clone_shares_state`, `read_by_type_id_returns_none_for_wrong_type`) | tests/e2e/e2e_async.rs |
| ASK-15 | `From` conversion of `TraitKitError` → mock errors (async error-surface interconversion) | Boundary | async | None | `src/kit/async_kit.rs::async_from_trait_kit_error_for_mock_error`, `async_kit_di_missing_dependency_returns_error` | tests/e2e/e2e_async.rs |

### 2.17 Internationalization (I18, 14)

| ID | Scenario | Type | Features | Services | Existing coverage | E2E target |
|----|---------|------|-------------|---------|---------|---------|
| I18-01 | The global convenience function `tr(message_id, args)`: translates per the current locale after init | Normal | i18n | None | `src/i18n/mod.rs::tr_convenience_function_works`, `tests/e2e/e2e_feature_combinations.rs::e2e_i18n_tr_global_function` | tests/e2e/e2e_i18n.rs |
| I18-02 | `I18nManager::init()` is a process-level singleton; `global()` returns `Some` after init | Normal | i18n | None | `src/i18n/mod.rs::manager_init_returns_valid_instance`, `i18n_manager_global_returns_some_after_init` | tests/e2e/e2e_i18n.rs |
| I18-03 | `init_with_locale("zh-CN")` builds with the specified locale; `locale_tag()` reads it back | Normal | i18n | None | `src/i18n/mod.rs::i18n_manager_init_with_locale`, `i18n_manager_build_zh_cn`, `tests/e2e/e2e_feature_combinations.rs::e2e_i18n_init_and_locale` | tests/e2e/e2e_i18n.rs |
| I18-04 | `translate(message_id, args)` variable interpolation (`{ $cycle }` and similar placeholder replacement) | Normal | i18n | None | `src/i18n/mod.rs::manager_translate_message`, `i18n_manager_translate_and_locale_tag` | tests/e2e/e2e_i18n.rs |
| I18-05 | Unknown message_id → fallback returns the key itself (no panic, no empty string) | Error | i18n | None | `src/i18n/mod.rs::manager_translate_unknown_key_returns_key`, `tests/e2e/e2e_feature_combinations.rs::e2e_i18n_translate_fallback` | tests/e2e/e2e_i18n.rs |
| I18-06 | FTL catalog parsing: valid messages parse, comments and blank lines are skipped (en/zh dual catalogs embedded) | Normal | i18n | None | `src/i18n/mod.rs::catalog_parse_simple_ftl`, `catalog_parse_skips_comments_and_blanks` | tests/e2e/e2e_i18n.rs |
| I18-07 | Locale parsing: `en`/`zh` valid; invalid strings report `I18nError::InvalidLocale` | Normal/Error | i18n | None | `src/i18n/mod.rs::test_locale_parsing_en`, `test_locale_parsing_zh`, `test_invalid_locale`, `tests/e2e/e2e_advanced.rs::e20_invalid_locale_error_message` | tests/e2e/e2e_i18n.rs |
| I18-08 | ICU4X number localization: en/zh grouping-separator differences, integers, fallback for non-en/zh | Boundary | i18n | None | `src/i18n/mod.rs::test_format_number_en`, `test_format_number_zh`, `test_format_number_integer` | tests/e2e/e2e_i18n.rs |
| I18-09 | Non-finite numbers (NaN/∞) → `I18nError::InvalidNumber` | Error | i18n | None | `src/i18n/mod.rs::test_format_number_not_finite`, `tests/e2e/e2e_advanced.rs::e21_non_finite_number_returns_invalid_number` | tests/e2e/e2e_i18n.rs |
| I18-10 | Plural rules: en plural-category determination + the zero category edge | Normal | i18n | None | `src/i18n/mod.rs::test_plural_rules_en`, `test_plural_category_zero` | tests/e2e/e2e_i18n.rs |
| I18-11 | ICU4X date formatting (year/month/day → localized string) | Normal | i18n | None | `src/i18n/mod.rs::test_format_date_en` | tests/e2e/e2e_i18n.rs |
| I18-12 | Invalid dates (month 13/day 32) → `I18nError::DateError` | Error | i18n | None | `src/i18n/mod.rs::test_format_date_invalid_month`, `test_format_date_invalid_day`, `tests/e2e/e2e_advanced.rs::e22_invalid_date_returns_date_error` | tests/e2e/e2e_i18n.rs |
| I18-13 | Collation sorting: basic sorting and equal-string comparison returns `Ordering::Equal` | Boundary | i18n | None | `src/i18n/mod.rs::test_collator_basic`, `test_compare_equal_strings` | tests/e2e/e2e_i18n.rs |
| I18-14 | The four `I18nError` variants have readable Display (InvalidLocale/DateError/InvalidNumber/FormatError) | Boundary | i18n | None | `src/i18n/mod.rs::error_display_invalid_locale`, `error_display_date_error`, `error_display_invalid_number`, `error_display_format_error` | tests/e2e/e2e_i18n.rs |

### 2.18 Error System (ERR, 10)

| ID | Scenario | Type | Features | Services | Existing coverage | E2E target |
|----|---------|------|-------------|---------|---------|---------|
| ERR-01 | `CycleDetected` Display via `tr()` includes all modules in the cycle (joined with ` → `) | Normal | — | None | `src/error.rs::cycle_detected_display_contains_modules`, `tests/basic.rs::kit_error_display_and_source_behavior` | tests/e2e/e2e_core.rs |
| ERR-02 | `DependencyMissing` Display includes both the originating module and the missing dependency name | Normal | — | None | `src/error.rs::dependency_missing_display_contains_both_modules` | tests/e2e/e2e_core.rs |
| ERR-03 | `AlreadyRegistered` Display includes the duplicate module name | Normal | — | None | `src/error.rs::already_registered_display_contains_module` | tests/e2e/e2e_core.rs |
| ERR-04 | `BuildFailed` Display includes context and the underlying source text; `Error::source()` returns the inner error | Normal | — | None | `src/error.rs::build_failed_display_contains_context_and_source`, `error_source_returns_inner_for_build_failed` | tests/e2e/e2e_core.rs |
| ERR-05 | `MissingCapability` / `MissingConfig` Display include the key (type name or module name) | Normal | — | None | `src/error.rs::missing_capability_display_contains_key`, `missing_config_display_contains_key` | tests/e2e/e2e_core.rs |
| ERR-06 | `LifecycleFailed` Display includes context+source, `source()` returns the inner error (lifecycle gated) | Normal | lifecycle | None | `src/error.rs::lifecycle_failed_display_contains_context_and_source`, `error_source_returns_inner_for_lifecycle_failed` | tests/e2e/e2e_features.rs |
| ERR-07 | `ShutdownTimedOut` Display includes all timed-out phase names (shutdown gated) → no inline assertion | Error | shutdown | None | none→to add | tests/e2e/e2e_runtime.rs |
| ERR-08 | Variants without a source (e.g. `MissingConfig`) return `None` from `source()`; Debug output includes the variant name | Boundary | — | None | `src/error.rs::error_source_returns_none_for_simple_variants`, `error_debug_format` | tests/e2e/e2e_core.rs |
| ERR-09 | Error Display dual-locale catalog binding: both the en (default fallback) and zh FTL catalogs cover all 9 error message ids (incl. config-validation-failed) | Boundary | i18n | None | `locales/en/errors.ftl`, `locales/zh/errors.ftl` (9 message ids verified) + `src/i18n/mod.rs::catalog_translate_with_variables` → dual-locale Display assertions to add | tests/e2e/e2e_i18n.rs |
| ERR-10 | External error text redaction: the `context` of `BuildFailed` / `LifecycleFailed` is folded into external category labels (internal call-site names stay out of text), and the underlying error text is redacted on both the `Display` and `Debug` sides (paths and secrets do not leak) | Boundary | — | None | `src/error.rs::build_failed_display_hides_internal_call_site_labels`, `debug_output_redacts_source_text`, `lifecycle_failed_display_contains_public_context_and_source` | tests/e2e/e2e_core.rs |

### 2.19 Prelude Export Surface (PRE, 3)

| ID | Scenario | Type | Features | Services | Existing coverage | E2E target |
|----|---------|------|-------------|---------|---------|---------|
| PRE-01 | The async exports of `prelude::*` are the same types as the native `async_kit` markers (preventing Ready/Unbuilt sync/async mismatch regressions) | Normal | async | None | `src/prelude.rs::prelude_async_kit_compiles`, `prelude_async_markers_match_async_kit_markers` | tests/e2e/e2e_async.rs |
| PRE-02 | The confers traits are reachable via both the prelude and kit paths (`Configurable`/`Validatable`/`ModuleConfig`/`SharedConfig`/`ConfigInherit`), and the `confers::Config` derive re-export is usable | Normal | confers | None | `tests/basic.rs::derive_config_macro_re_exported`, `tests/config_inherit_derive.rs` (`use trait_kit::kit::ConfigInherit`) | tests/e2e/e2e_config.rs |
| PRE-03 | The derive macros (`ConfigInherit`/`SharedConfig`) are not exported via the prelude and require an explicit `trait-kit-macros` dependency — the documented contract holds (preventing accidental migration) | Boundary | confers | None | `src/prelude.rs` (NOTE comment) + `tests/config_inherit_derive.rs::derive_config_inherit_generates_override_type` | tests/e2e/e2e_presets.rs (compile surface) |

### 2.20 Feature Combination Interactions (CMP, 15)

| ID | Scenario | Type | Features | Services | Existing coverage | E2E target |
|----|---------|------|-------------|---------|---------|---------|
| CMP-01 | async+lifecycle → the `AsyncLifecycle` export surface is consistent across lib.rs/kit/mod/prelude and the async on_ready takes effect | Boundary | async,lifecycle | None | `src/lib.rs` export verification + `src/kit/async_kit.rs::async_lifecycle_on_ready_called` | tests/e2e/e2e_feature_combinations.rs |
| CMP-02 | confers+reload: on the `reload` chain, `Configurable::load` is reused for the reload path (`reload_config::<C>()` is a second execution of `C::load()`) | Normal | reload,confers | None | `tests/e2e/e2e_feature_combinations.rs::e2e_confers_plus_reload` | tests/e2e/e2e_feature_combinations.rs |
| CMP-03 | confers+encryption: the same C type holds plaintext config and ciphertext storage side by side, with the encryption chain (encryption→confers) automatically in effect | Normal | encryption,confers | None | `tests/e2e/e2e_feature_combinations.rs::e2e_confers_plus_encryption` | tests/e2e/e2e_feature_combinations.rs |
| CMP-04 | lifecycle+health: the health report is available after on_ready and stays readable after shutdown | Boundary | lifecycle,health | None | `tests/e2e/e2e_feature_combinations.rs::e2e_lifecycle_plus_health` | tests/e2e/e2e_feature_combinations.rs |
| CMP-05 | observer+decorator: observer timing covers the decoration step (build → decorate → on_module_built) | Normal | observer,decorator | None | `tests/e2e/e2e_feature_combinations.rs::e2e_observer_plus_decorator` | tests/e2e/e2e_feature_combinations.rs |
| CMP-06 | toggle+decorator: under toggle gating, decorators apply only to actually registered modules | Normal | toggle,decorator | None | `tests/e2e/e2e_feature_combinations.rs::e2e_toggle_plus_decorator` | tests/e2e/e2e_feature_combinations.rs |
| CMP-07 | scope+lifecycle: coexistence semantics of scopes and lifecycle hooks | Boundary | request-scope,lifecycle | None | `tests/e2e/e2e_feature_combinations.rs::e2e_scope_plus_lifecycle` | tests/e2e/e2e_feature_combinations.rs |
| CMP-08 | scope+async → the `AsyncScope` export surface and `AsyncKit::create_scope` connect | Boundary | request-scope,async | None | `src/lib.rs`/`src/kit/mod.rs` export verification + `src/kit/async_kit.rs::async_create_scope_returns_empty` | tests/e2e/e2e_feature_combinations.rs |
| CMP-09 | shutdown+async → the `AsyncShutdownCoordinator` export surface is consistent (lib.rs/kit/mod/prelude) | Boundary | shutdown,async | None | `src/kit/shutdown.rs::async_shutdown_coordinator_executes_hooks` + export verification | tests/e2e/e2e_feature_combinations.rs |
| CMP-10 | shutdown+decorator: the shutdown order (decorator layer vs core layer) of decorated capabilities, semantics pinned | Boundary | shutdown,decorator | None | none→to add | tests/e2e/e2e_feature_combinations.rs |
| CMP-11 | toggle+scope: toggle-gated in-scope registration after `create_scope` | Boundary | toggle,request-scope | None | none→to add | tests/e2e/e2e_feature_combinations.rs |
| CMP-12 | interface+decorator: decorators apply by interface TypeId to the `register_as` build path (`apply_decorators(interface_id, …)` inside `build_interface_modules`) | Boundary | di,decorator | None | none→to add | tests/e2e/e2e_feature_combinations.rs |
| CMP-13 | encryption+reload dual-chain coexistence: with both `encryption` and `reload` enabled (the confers dual engines confers/encryption + Kit SubscriberMap), all APIs compile and work | Error | encryption,reload | None | none→to add (compile-level + behavior-level) | tests/e2e/e2e_feature_combinations.rs |
| CMP-14 | i18n+shutdown: the `ShutdownTimedOut` error message goes through the `tr()` translation chain (error text localized under the zh locale) | Boundary | shutdown,i18n | None | none→to add | tests/e2e/e2e_feature_combinations.rs |
| CMP-15 | All-feature smoke: compiles under `--all-features` (verified exit 0) + a single full-capability behavior-level pass (register/lazy/multi/interface/config/reload/encrypt/lifecycle/health/observer/decorator/scope/toggle/shutdown/i18n in one Kit) | Normal | all features | None | `cargo check --all-features` (measured at authoring time) → behavior-level to add | tests/e2e/e2e_feature_combinations.rs |

### 2.21 Concurrency and Races (CCY, 7)

| ID | Scenario | Type | Features | Services | Existing coverage | E2E target |
|----|---------|------|-------------|---------|---------|---------|
| CCY-01 | `AsyncTypeMap` cross-thread concurrent reads/writes without panic, and consistent state via `Arc` clone sharing | Boundary | async | None | `src/kit/async_typemap.rs::cross_thread_access_does_not_panic`, `arc_clone_shares_state` | tests/e2e/e2e_concurrency.rs |
| CCY-02 | `AsyncKit` concurrent registration: multiple modules concurrently `register`, all succeeding | Boundary | async | None | `src/kit/async_kit.rs::async_kit_concurrent_registration` | tests/e2e/e2e_concurrency.rs |
| CCY-03 | The sync `Kit`'s `RefCell`-based (`!Sync`) design boundary pinned: sharing a `Kit` across threads must fail to compile (negative compile assertion). **Equivalent landing**: codified via the `static_assertions::assert_not_impl_any!` compile-time assertion (trybuild stderr snapshots depend on the feature set and are fragile across the combination matrix, hence not via tests/ui) | Boundary | — | None | `tests/basic.rs::assert_not_impl_any!(Kit<Unbuilt>: Sync)` + `!(Kit<Ready>: Sync)` | tests/basic.rs:11-12 (compile-time equivalent) |
| CCY-04 | Scale stress: 100-module register+build, topological order with no stack overflow / no performance cliff | Boundary | — | None | `tests/e2e/e2e_advanced.rs::c06_large_number_of_modules_100_registers_and_builds` | tests/e2e/e2e_concurrency.rs |
| CCY-05 | Config scale: 20 different config types all set/read correctly | Boundary | — | None | `tests/e2e/e2e_advanced.rs::c07_many_config_types_20_set_and_read` | tests/e2e/e2e_concurrency.rs |
| CCY-06 | A `reload_config` concurrency storm (50 interleaved rounds) while a `require_ref` borrow is alive. **Real-behavior correction**: under the independent-`TypeMap` design, the borrow and config writes do not conflict, and the guard reads a complete capability value throughout; the originally assumed borrow-conflict panic does not occur (`set_config` is an Unbuilt-state method, see RLD-07) | Boundary | reload | None | `src/kit/typemap.rs::inner_ref_panics_if_mutably_borrowed` (existing at that layer) | tests/e2e/e2e_concurrency.rs::e2e_ref_borrow_storm_interleaved_reload_set |
| CCY-07 | A 1MB encrypted large value mixed with regular configs under stress (same data source as ENC-09, combined into a concurrency scenario) | Boundary | encryption | None | `tests/e2e/e2e_advanced.rs::c15_large_config_value_encryption_1mb` | tests/e2e/e2e_concurrency.rs |

### 2.22 Feature Compilation Matrix (PRS, 6)

> The feature count in this section reflects the historical basis at authoring time (the 13-feature era); since then features such as `probe=["health"]` were added.
> The **single source of truth** for the count and list is `tests/e2e/e2e_presets.rs` (the `prs02` expected table +
> the `prs03` cfg gate, maintained in sync with the `Cargo.toml [features]` section); this section keeps only the scenario semantics.

| ID | Scenario | Type | Features | Services | Existing coverage | E2E target |
|----|---------|------|-------------|---------|---------|---------|
| PRS-01 | `cargo check` (no features, default=[]) compiles the whole workspace + the no-feature test group passes (`e2e_no_feature_*`) | Boundary | — | None | `tests/e2e/e2e_feature_combinations.rs::e2e_no_feature_basic_build`, `e2e_no_feature_graph_export` | tests/e2e/e2e_presets.rs |
| PRS-02 | Every feature enabled alone (`--features async` etc., count per `prs02`) passes its own `cargo check`, with the export surface scaling correctly per gating (e.g. no `Lifecycle` export when only async is on) | Boundary | every single feature | None | none→to add (loop script) | tests/e2e/e2e_presets.rs |
| PRS-03 | `--all-features` (all features on) `cargo check` + all tests pass | Boundary | all | None | `cargo check --all-features` measured exit 0 at authoring time → needs pinning as a CI gate | tests/e2e/e2e_presets.rs |
| PRS-04 | Dependency chains take effect automatically: `--features reload` implicitly enables `confers` (`confers/watch` pruned), and the `Configurable`-surface APIs are available | Boundary | reload | None | `Cargo.toml` declaration verification + `tests/e2e/e2e_feature_combinations.rs::e2e_confers_plus_reload` | tests/e2e/e2e_presets.rs |
| PRS-05 | Dependency chains take effect automatically: `--features encryption` implicitly enables `confers` and `confers/encryption`, with the XChaCha20 primitives usable via the trait-kit re-exports | Boundary | encryption | None | `Cargo.toml` declaration verification + `tests/e2e/e2e_feature_combinations.rs::e2e_confers_plus_encryption` | tests/e2e/e2e_presets.rs |
| PRS-06 | The examples crate's 20 examples compile gated by their own `required-features` (each example's feature passes a separate check) | Boundary | all | None | none→to add (a `cargo check -p trait-kit-examples --features <one by one>` script) | tests/e2e/e2e_presets.rs |

---

## 🔗 3. Feature Mutual Exclusion / Combination Matrix

### 3.1 Dependency Chains (fixed by `Cargo.toml [features]`; no separate verification needed in combination tests)

```text
reload     → confers (the confers/watch dead link has been pruned)
encryption → confers, confers/encryption
confers    → dep:confers, dep:serde, dep:serde_json
i18n       → dep:icu_collator, dep:icu_datetime, dep:icu_decimal, dep:icu_locale,
             dep:icu_plurals (ICU4X component sub-crate), dep:writeable, dep:sys-locale
probe      → health, dep:futures-timer (executor-agnostic timer)
presets / presets-remote / report / version-negotiation / compose / di
           → see each dependency declaration in Cargo.toml (single source of truth)
default    → (empty; no implicit feature of any kind)
async / di / lifecycle / health / request-scope / toggle / observer / decorator / shutdown
           → empty feature (zero dependencies, zero transitive items)
```

Conclusion: **no mutually exclusive features and no presets exist**: zero-dependency features can be stacked freely; the only constraint is that "enabling `reload`/`encryption`/`probe` automatically brings in their dependency engines". `cargo check --all-features` (all features on; current count per `prs02`) passed in testing (exit 0), giving compile-level verification that all features are mutually compatible.

### 3.2 Cross-Feature Gating (compile-time linkage; the input basis for combination behavior tests)

| Combination | Resulting exports/behavior | Declared in |
|------|----------------|---------|
| async + lifecycle | `AsyncLifecycle` | `src/lib.rs`, `src/core/mod.rs`, `src/prelude.rs` |
| async + health | `AsyncHealthCheck` | same as above |
| async + request-scope | `AsyncScope` | same as above |
| async + shutdown | `AsyncShutdownCoordinator` | same as above |
| async (alone) | `AsyncKit`/`AsyncAutoBuilder`/`AsyncReady`/`AsyncUnbuilt`/`AsyncTypeMap` | `src/lib.rs`, `src/kit/mod.rs` |
| observer + decorator | main build path: callback `on_module_start` first → build → `apply_decorators` → `on_module_built` | `src/kit/kit.rs::build_eager_modules` |
| decorator (alone) | wrapping points covering the eager/lazy/multi/interface build paths | `src/kit/kit.rs` (4 `apply_decorators` sites) |
| lifecycle + health | `register_lifecycle` and `register_health_check` share the capability TypeMap (`get_ref_by_type_id`) | `src/kit/kit.rs` |
| confers + any | `Config` derive re-export + the five config traits available | `src/kit/config.rs` |

### 3.3 Combination Test Matrix (the input basis for CMP-01…15)

| Combination | Features | Interaction point | Scenario | Existing coverage |
|------|-------------|--------|---------|---------|
| C1 | async + lifecycle | `AsyncLifecycle` async on_ready | CMP-01 | yes (src inline) |
| C2 | confers + reload | load reused for reload | CMP-02 | yes (e2e_feature_combinations) |
| C3 | confers + encryption | plaintext/ciphertext coexist | CMP-03 | yes (e2e_feature_combinations) |
| C4 | lifecycle + health | health available after ready | CMP-04 | yes (e2e_feature_combinations) |
| C5 | observer + decorator | callbacks cover the decoration step | CMP-05 | yes (e2e_feature_combinations) |
| C6 | toggle + decorator | toggle-gated decoration | CMP-06 | yes (e2e_feature_combinations) |
| C7 | request-scope + lifecycle | scopes and hooks coexist | CMP-07 | yes (e2e_feature_combinations) |
| C8 | request-scope + async | `AsyncScope` | CMP-08 | yes (src inline) |
| C9 | shutdown + async | `AsyncShutdownCoordinator` | CMP-09 | yes (src inline) |
| C10 | shutdown + decorator | shutdown order | CMP-10 | **to add** |
| C11 | toggle + request-scope | toggle-gated scopes | CMP-11 | **to add** |
| C12 | di + decorator | interface build-path decoration | CMP-12 | **to add** |
| C13 | encryption + reload | dual confers engines coexist | CMP-13 | **to add** |
| C14 | i18n + shutdown | error-message translation chain | CMP-14 | **to add** |
| C15 | all features | full-capability smoke | CMP-15 | compile verified / behavior **to add** |

---

## 🐳 4. Docker Service Requirements Summary

**None.** trait-kit is a pure in-memory library; all 244 scenarios have "None" dependent services, for the following reasons:

1. **Zero docker-compose dependencies**: the repo has no `docker-compose*.yml`; direct `Cargo.toml` dependencies are only `confers` (optional), `serde`/`serde_json` (optional), and `icu_*` (ICU4X component sub-crates)/`writeable`/`sys-locale` (i18n optional) — no database/message-queue/network-service clients at all.
2. **The confers dependency surface is purely in-process traits**: trait-kit consumes only three confers capability surfaces — synchronous config loading (`Config::load_sync` bridged to `Configurable::load`), `watch` (in-process subscription callbacks, not file inotify), and `encryption` (pure crypto primitives XChaCha20/HKDF) — and **never touches confers' remote/etcd/consul/nats/redis features that need docker services** (trait-kit's feature chain never enables them).
3. **"External resources" are only two kinds, both localizable**: local files (only the confers `#[derive(Config)]` file-loading path; E2E can use `std::env::temp_dir()` temp files) and none (everything else is in-memory operations).

Therefore: **the full E2E suite runs in a docker-free CI sandbox**; "error injection" always uses explicit invalid inputs (wrong keys, unregistered types, invalid locales, timeout Durations) rather than faulty services. No service guards (port guard / health check) are needed.

---

## 🧭 5. Execution Plan

### 5.1 Levels and Commands

| Level | Content | Command | Mock allowed? |
|------|------|------|-----------|
| L1 unit | 726 inline `#[test]`s in src (45 `#[cfg(test)]` + 67 `#[cfg(all(test, feature = …))]` gated blocks, covering 28 files) | `cargo test --all-features --lib` | allowed |
| L2 integration | 9 top-level integration files in `tests/` (`api_reference_gate` / `basic` / `compile_fail` (1 runner test) / `config_inherit_derive` / `config_inheritance_e2e` / `e2e_gap_round1` / `e2e_gap_round2` / `e2e_scenarios_26_50` / `shared_config_derive`, 109 test functions total) + 13 registered targets in `tests/e2e/` (205 test functions; `e2e_advanced` / `e2e_feature_combinations` etc. moved into the subdirectory, see §5.3; note Key Finding 3: feature groups run in separate rounds by `--features`) | `cargo test --features confers,reload,encryption && cargo test --features async,lifecycle,health,request-scope,toggle,observer,decorator,shutdown,di && cargo test --features i18n` | allowed (pure in-memory) |
| L2b compile-time | 17 trybuild cases in `tests/ui/` (13 at root: 7 typestate + 6 macro diagnostics; plus 1 each in `probe_sensitive/`, `observer_ready/`, `decorator_ready/`, `lifecycle_ready/`) | `cargo test --test compile_fail` | — |
| L3 examples | run all 20 examples one by one (see 5.2; pass features per `required-features`) | `cargo run -p trait-kit-examples --features <f> --example <name>` | no mock |
| L4 E2E | `tests/e2e/` (the §2 scenario-ID landing; 13 files landed) | `cargo test --features <combination> --test e2e_<name>` (subdirectories are not auto-discovered; 13 targets explicitly registered via root `Cargo.toml` `[[test]]`) | no mock |

Pre-work (complete before landing E2E):
1. No dead-file problem (unlike confers): all 7 files under `tests/` are auto-discovered and run; no unregistered modules.
2. Add a uniform `#![cfg(feature = "…")]` header to feature-gated E2E files to avoid compile failures in no-feature rounds.
3. Fill the 19 "none→to add" scenarios and 13 "…→assertions to add" scenarios in §2 (list per §6).

### 5.2 examples Run List (20, `cargo run -p trait-kit-examples --features <f> --example <name>`)

| # | Example | required-features | Expected result (acceptance pass) |
|---|------|-------------------|----------------------|
| 1 | `default_basic` | none | exit code 0; `Kit<Unbuilt>`→`Kit<Ready>` basic typestate demo output |
| 2 | `confers_loader` | confers | exit code 0; `Configurable` bridging `#[derive(Config)]` loading demo |
| 3 | `confers_macros` | confers | exit code 0; `ModuleConfig::PATH` binding and derive re-export demo |
| 4 | `hot_reload` | reload | exit code 0; `subscribe`/`reload_config` callback-triggered output |
| 5 | `encryption` | encryption | exit code 0; set/get_encrypted roundtrip and three-level inheritance demo |
| 6 | `async_basic` | async | exit code 0; `AsyncKit` async typestate demo |
| 7 | `lifecycle` | lifecycle | exit code 0; on_ready/on_shutdown trigger output |
| 8 | `health_check` | health | exit code 0; health_report aggregation output |
| 9 | `observability` | observer | exit code 0; build callback sequence output |
| 10 | `scope_basic` | request-scope | exit code 0; scope isolation demo |
| 11 | `conditional` | none | exit code 0; `register_if` predicate registration dual-branch output |
| 12 | `factory` | none | exit code 0; factory new-instance-per-call demo |
| 13 | `decorator` | decorator | exit code 0; before/after capability wrapping comparison output |
| 14 | `interface` | di | exit code 0; register_as/resolve dyn Trait demo |
| 15 | `i18n` | i18n | exit code 0; FTL translation + ICU4X formatting output |
| 16 | `shutdown` | shutdown | exit code 0; three-phase graceful shutdown demo |
| 17 | `validation` | confers | exit code 0; valid-pass + invalid-rejected dual-branch output |
| 18 | `snapshot_restore` | confers | exit code 0; snapshot/rollback demo |
| 19 | `toggle_basic` | toggle | exit code 0; `register_if_toggle` on/off dual-branch output |
| 20 | `config_inheritance` | confers | exit code 0; ConfigInherit/SharedConfig inheritance chain demo |

Batch run command:
`for e in default_basic confers_loader confers_macros hot_reload encryption async_basic lifecycle health_check observability scope_basic conditional factory decorator interface i18n shutdown validation snapshot_restore toggle_basic config_inheritance; do …按 5.2 表传 feature…; done`

### 5.3 Scenario ID → E2E File Mapping Summary

| tests/e2e/ file | Scenarios covered | Needs docker |
|----------------|---------|-----------|
| e2e_core.rs | MET-01…09, REG-01…20, CAP-01…12, DEP-01…10, ERR-01…09, PRE-02 | No |
| e2e_config.rs | CFG-01…22, RLD-01…09 | No |
| e2e_encryption.rs | ENC-01…13 | No |
| e2e_features.rs | ITF-01…09, LCY-01…09, HLT-01…09, OBS-01…07, DEC-01…08 | No |
| e2e_runtime.rs | SCP-01…10, TGL-01…09, SHD-01…13, ERR-07 | No |
| e2e_async.rs | ASK-01…15, MET-04, LCY-06, HLT-08/09, OBS-07, SCP-08/09, DEC-08, PRE-01 | No |
| e2e_i18n.rs | I18-01…14, ERR-09 (zh side; en-side Display assertions land in e2e_i18n_en.rs, a separate process anchoring the locale) | No |
| e2e_feature_combinations.rs | CMP-01…15, RLD-08/09, ENC-13, LCY-08/09, OBS-06, DEC-06, SCP-10, TGL-08 | No |
| e2e_concurrency.rs | CCY-01…07 | No |
| e2e_presets.rs | PRS-01…06, TGL-09, PRE-03 | No |
| e2e_hooks.rs | LCY-05/07, HLT-07, OBS-05 (runtime hook gaps pinned; other LCY/HLT/OBS scenarios reference existing coverage) | No |
| tests/ui/ (extended) | REG-19 (current 17 cases: 13 at `tests/ui/` root — 7 typestate + 6 macro diagnostics — plus 1 each in `probe_sensitive/`, `observer_ready/`, `decorator_ready/`, `lifecycle_ready/`); CCY-03 landed equivalently in tests/basic.rs (see header reconciliation 3) | No |

> 13 files landed in total (the 11 above + the existing `e2e_advanced.rs` with 86 cases carrying the bulk of the B/A/E/C series scenarios + `e2e_i18n_en.rs` as the en-side separate-process target); top-level files such as `basic.rs` and `compile_fail.rs` remain under tests/ auto-discovery.

### 5.4 Suggested Execution Order

1. L1 all green: `cargo test --all-features --lib` (726 inline tests).
2. L2 all green per feature rounds (the §5.1 three-round commands) + L2b's 17 trybuild cases.
3. L3: run the 20 examples one by one with their features (the 5.2 list).
4. Land `tests/e2e/` (10 files), prioritizing CMP-10…15, CCY-03/06, DEC-03/04/07/08, ASK-13, ERR-07, RLD-06, HLT-07, LCY-05, SHD-13, CAP-11 (19 pure gaps + 13 missing assertions, the thinnest existing coverage).
5. PRS compile gates into CI: three tiers — no features / single feature ×13 / `--all-features`, each run per PR and nightly; examples-crate gated compilation pinned via PRS-06.

---

## 📊 6. Statistics Summary

(Computed programmatically from the scenario rows of this document; basis in the notes)

| Dimension | Count |
|------|------|
| Total scenarios | **244** |
| Type=Normal | 100 |
| Type=Error | 60 |
| Type=Boundary | 83 |
| Type=Normal/Error (dual assertion) | 1 (I18-07) |
| E2E scenarios to add (existing coverage "none→to add") | 19 |
| Scenarios needing added assertions/integration (unit test or example exists, but lacks integration/behavior-level assertions, "…→assertions to add") | 13 |
| Scenarios referencing existing `tests/` integration tests (overlaps other dimensions) | 123 |
| Scenarios referencing src inline tests (overlaps other dimensions) | 167 |
| Scenarios referencing examples acceptance | 7 |
| Scenarios referencing tests/ui (trybuild) | 2 (REG-19 existing, CCY-03 new landing) |
| Scenarios depending on docker services | **0** |
| Scenarios depending on local temp files | ≤3 (the confers derive file-loading path in the CFG group) |
| Scenarios with no external dependencies at all (pure in-memory) | 240+ |

> Existing test-asset baseline (rc.2 authoring-time grep measurements): **394** inline `#[test]`s in src; **185** test functions in `tests/` (basic 45 / e2e_advanced 84 / e2e_feature_combinations 42 / config_inherit_derive 6 / shared_config_derive 5 / config_inheritance_e2e 2 / compile_fail 1 runner→3 ui cases at the time; the e2e_* files were then at the `tests/` top level, moving into `tests/e2e/` after landing, see Key Finding 7); **20** examples.
> Post-landing status (current repo grep measurements): **323** test functions in `tests/` — 205 in the 13 registered `tests/e2e/` targets, 118 in the 10 top-level integration files, and `compile_fail.rs` as 1 runner driving 17 ui cases (13 at root + 1 each in 4 gated subdirectories); **957** inline in src (1280 total for `src` + `tests`).
> Note: the existing-coverage basis allows legitimate overlaps (one scenario may reference both tests/ integration tests and src inline tests); "to add" takes the strongest-gap verdict. The landing phase may merge overly fine-grained scenarios into single implementations, but **IDs stay stable** for traceability.

### Authoring-Baseline Verification Notes

1. **Count of `#[cfg(test)]` in src**: the draft said 17; measurement at the rc.2 authoring point found **38** literal `#[cfg(test)]` plus 65 `#[cfg(all(test, feature = …))]` gated test blocks (657 inline tests in total); the current measurement is **45 + 67 gated blocks (726 inline tests total, covering 28 files)**. Current measurements prevail.
2. **examples count**: the draft said 21; measurement found **20** explicitly `[[example]]`-registered in `examples/Cargo.toml` (also 20 .rs files under `examples/src`). Measurements prevail.
3. **Dependent services**: the draft noted "only the confers feature needs file/remote scenarios"; verification shows trait-kit's confers surface contains only in-process trait bridges, with file paths appearing only in confers derive file loading (E2E can use temp files) — **no remote scenarios at all** (§4).
