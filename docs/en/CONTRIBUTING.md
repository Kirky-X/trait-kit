# 🤝 Trait-Kit Contributing Guide

Thank you for your interest in the trait-kit project! This document describes the tools, processes, and conventions required to participate in development.

## 📋 Table of Contents

<details open>
<summary>📑 Table of Contents</summary>

- [👋 Welcome](#-welcome)
- [🧰 Environment Setup](#-environment-setup)
- [🔁 Development Workflow (TDD)](#-development-workflow-tdd)
- [📐 Code Conventions](#-code-conventions)
- [🚀 Commits and PR Process](#-commits-and-pr-process)
- [📜 Code of Conduct](#-code-of-conduct)
- [🧪 Test Strategy](#-test-strategy)
- [🗺️ Roadmap](#️-roadmap)

</details>

---

## 👋 Welcome

Welcome to trait-kit development! Whether fixing bugs, adding feature-gated capabilities, improving docs, or completing examples — everything is valuable.

- Submit bugs and feature requests to [GitHub Issues](https://github.com/Kirky-X/trait-kit/issues).
- Security vulnerabilities must **not** go through public Issues; see the vulnerability reporting process in the [Security document](SECURITY.md).
- Before starting work, discuss your plan in an Issue to avoid conflicts with existing feature planning.

## 🧰 Environment Setup

- **Rust 1.97.1+** (`rust-version = "1.97.1"` in `Cargo.toml`, edition 2024)
- **cargo**, **rustfmt**, **clippy** (installed with rustup)
- **cargo-deny**: `cargo install cargo-deny` (needed by lefthook and CI dependency audits)
- **pre-commit** or **lefthook** (choose one; their coverage is not fully equivalent — pre-commit includes `detect-secrets` / `cargo-check` and more, lefthook includes `cargo-deny` and private-key grep at commit time and the coverage gate at push time; `cargo-deny` / `cargo-audit` are pre-push-stage hooks in the pre-commit framework (enabled via `pre-commit install -t pre-push`)):

```sh
uv tool install pre-commit   # or pip install pre-commit
pre-commit install
```

> trait-kit itself has no system-library dependencies; the `confers` dependency of the `examples` workspace member comes from crates.io (no cross-repo path), and no workspace member needs `protoc` / `protobuf-compiler`.

### Common Commands

```sh
# 构建（全特性 / 无默认特性，与 CI 一致）
cargo build --all-features --workspace
cargo build --no-default-features --workspace

# 测试（全特性，与 CI 一致）
cargo test --workspace --all-features

# 格式化与 Lint
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings

# 文档（零告警标准）
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps

# 依赖审计
cargo deny check

# MSRV 验证
cargo +1.97.1 check
```

## 🔁 Development Workflow (TDD)

Every development task follows the Red → Green → Commit → Analyze → Next loop:

1. **Define the interface**: first define the trait / API signature (e.g. `trait Xxx { ... }`) without an implementation.
2. **Write tests**: write unit tests against the interface (`#[cfg(test)] mod tests`); they should fail at this point (red). For typestate-misuse requirements, add trybuild UI tests (`tests/compile_fail.rs` + `tests/ui/*.stderr`).
3. **Write code**: implement the interface until tests pass (green).
4. **Run tests**: `cargo test --features <the-feature> --lib`.
5. **commit**: `git commit -m "feat(<module>): <description>"`.
6. **analyze**: analyze the task's impact on other modules and downstream dependents, identifying code that needs coordinated changes.
7. **Move on to the next**.

### Feature Combination Requirements

Features in trait-kit have inheritance relationships (`encryption` → `reload` → `confers`); before committing, cover at least the following combinations (aligned with CI and the acceptance matrix):

```sh
cargo test --workspace --all-features   # 与 CI test job 一致
cargo test                              # 默认特性
cargo test --no-default-features
cargo test --features confers
cargo test --features shutdown
cargo test --features i18n
```

When adding a feature:

- Declare it in `Cargo.toml`'s `[features]` with clear enablement relationships and explanatory comments.
- Register it in the "Feature Flags" table of the READMEs (CN/EN) and in [docs/API_REFERENCE.md](../API_REFERENCE.md).
- If it involves confers integration, note that higher-level features automatically enable lower-level ones, and methods need `cfg(feature = "...")` gating.

## 📐 Code Conventions

- Follow the existing codebase's naming and module-organization conventions (`src/core` interface layer, `src/kit` management center, `src/i18n` internationalization).
- **Simplicity first**: write the minimum code that solves the problem; no unnecessary comments, docstrings, or type annotations.
- **Dependencies must be feature-gated**: optional dependencies (`confers`, `serde`, `serde_json`, etc.) are forbidden in default features; `icu_collator` / `icu_datetime` / `icu_decimal` / `icu_locale` / `icu_plurals` (ICU4X component sub-crates, not the `icu` facade crate) / `writeable` / `sys-locale` are optional dependencies of the `i18n` feature only.
- **No new unsafe**: the crate has a global `#![deny(unsafe_code)]`; production code currently has only 5 locally audited exemptions with SAFETY justifications and code review (typestate layout-assert casts ×3, volatile zeroization ×1, guard lifetime re-anchoring ×1: the `AsyncTypeMap` `RwLock` read guard; the sync `TypeMap` now uses a `Ref::map` guard wrapper and needs no unsafe), plus 1 test-module exemption (see the [Security document](SECURITY.md)). No PR may introduce new `unsafe`; if truly necessary, attach SAFETY comments and argue the case separately in the PR.
- **Errors must be explicit**: new error scenarios get a new variant in `TraitKitError` (or reuse an existing one), with messages wired into `tr()` localization (written per domain into `locales/en/*.ftl` and `locales/zh/*.ftl`; both key sets must stay level); swallowing errors or hiding them in defaults is strictly forbidden.
- **Doc examples must be verifiable**: code examples in the README / docs must match actual API signatures; when APIs change, keep [docs/API_REFERENCE.md](../API_REFERENCE.md) and [docs/USER_GUIDE.md](USER_GUIDE.md) in sync.

### Pre-commit Hooks

| Hook | Description |
|------|------|
| `trailing-whitespace` / `end-of-file-fixer` | Trailing whitespace and end-of-file newline |
| `check-yaml` / `check-toml` / `check-merge-conflict` | Config syntax and conflict-marker checks |
| `check-added-large-files` | Large-file interception (`--maxkb=1000`, i.e. >1000KB) |
| `detect-private-key` / `detect-secrets` | Private-key leak detection and baseline secret scanning |
| `typos` | Spell check |
| `cargo-fmt` | `cargo fmt --all -- --check` |
| `cargo-check` | `cargo check --all-features` |
| `cargo-clippy` | `cargo clippy --all-targets --all-features -- -D warnings` |

> `cargo-deny` (license/advisory/banned-dependency audit) is configured in both hook sets: as a pre-push-stage hook in the pre-commit framework (enabled via `pre-commit install -t pre-push`) and in lefthook's pre-commit stage; CI also has a corresponding job (`.github/workflows/ci.yml`).

> **Never bypass the hooks with `--no-verify`** — this is a security red line.

### Code Quality Tooling

- **Code quality review**: code simplification, architecture optimization, performance review
- **SAST security scanning**: 0 CRITICAL required before release
- **kueiku**: hard-bug analysis and root-cause localization

## 🚀 Commits and PR Process

1. This repository's workflow is **direct commits to main** (`.pre-commit-config.yaml` notes: no `no-commit-to-branch` is configured — that hook protects `master` and would never fire in this repo); external contributors can create a feature branch from `main` (e.g. `feat/<feature>` or `fix/<issue>`) and open a PR.
2. Commit messages follow conventional commits: `feat(scope): ...`, `fix(scope): ...`, `docs: ...`, etc.
3. Ensure all local checks pass:
   - pre-commit hooks all green
   - `cargo test --workspace --all-features` passes
   - new features come with tests; API/doc changes keep the README and CHANGELOG in sync
4. Push the branch and open a PR whose description includes: change summary, test results, affected features.
5. Wait for review after CI (fmt / clippy / test / cross-platform builds / cargo deny / doc) passes.

Releases are performed by Kirky.X: a tag push triggers `release.yml` (build verification → GitHub Release → crates.io publish); contributors need not act.

## 📜 Code of Conduct

This project follows the [Rust Code of Conduct](https://www.rust-lang.org/policies/code-of-conduct). All contributors must comply: stay friendly, respectful, and professional, keeping the community welcoming for everyone.

---

## 🧪 Test Strategy

| Type | Location | Description |
|------|------|------|
| Unit tests | `src/` (`#[cfg(test)]`) | Module-internal logic |
| Integration tests | `tests/` (9 targets) + `tests/e2e/` (13 registered targets) | `basic`, `config_inheritance_e2e`, `e2e_core`, `e2e_async`, `e2e_concurrency`, `e2e_feature_combinations`, etc. |
| Compile-time UI tests | `tests/compile_fail.rs` + `tests/ui/` (11 cases: 7 at the root + one each in `probe_sensitive/`, `observer_ready/`, `decorator_ready/`, `lifecycle_ready/`) | trybuild-based: assert typestate misuse (retrieve before build, etc.) fails to compile |
| Macro-crate tests | `macros/tests/` | `#[derive(Module)]` expansion correctness and compile-fail cases |
| Doc tests | `rust` code blocks in the README and doc comments | Compiled and run by `cargo test` |
| Example validation | `examples/` (20) | Each example runs standalone; failed assertions panic |
| Benchmarks | `benches/kit_bench.rs` | criterion benchmarks (requires the `toggle` feature) |

Test scale: the main crate's `src/` and `tests/` contain **1040** `#[test]` functions (via `grep -rEo '#\[(tokio::)?test\]' --include='*.rs' src tests | wc -l`), plus 6 in `macros/tests/`.

### Common Commands (same as CI)

```sh
# 全量测试（workspace + 全部 feature，与 CI test job 一致）
cargo test --workspace --all-features

# 按特性组合运行
cargo test --features confers

# Lint 与格式检查（CI 门禁，零告警）
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check

# 文档零告警
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features

# 依赖审计（advisories / licenses / bans / sources）
cargo deny check

# 覆盖率门禁（CI coverage job：行覆盖 ≥ 95%）
cargo llvm-cov --workspace --all-features --fail-under-lines 95
```

## 🗺️ Roadmap

Entries carry over the existing plan; status is aligned with the current repository state:

<table>
<tr><th>Status</th><th>Item</th><th>Description</th></tr>
<tr><td>✅</td><td><b>0.5.0-rc.12</b> (2026-10-31)</td><td>Current release: new <code>masking</code> feature — the PII/credential masking engine (<code>DataMasker</code> / <code>MaskRule</code> / <code>MaskRuleRegistry</code> / <code>AcMasker</code>) sunk from inklog into this crate as the single source, with the secret-scan egress gate family (<code>SecretScanGate</code> / <code>SecretPatternRegistry</code> / <code>EntropyScanner</code>) and <code>MaskError</code>; shared by inklog and limiteron with the 21 builtin rules' semantics and order unchanged (see the <a href="../CHANGELOG.md">CHANGELOG</a>).</td></tr>
<tr><td>✅</td><td><b>0.5.0-rc.11</b> (2026-10-28)</td><td>tightened <code>override_module</code> dependency-graph validation (unregistered dependencies rejected at injection time) and shared-overlay ownership isolation; full redaction of external text and error <code>Debug</code> output; master-key admission policy; <code>TypeMap</code> read-reference guard (unsafe exemptions 6→5); depth guard moved earlier for deeply nested JSON; <code>confers</code> dependency 0.6.0-rc.12 (see the <a href="../CHANGELOG.md">CHANGELOG</a>).</td></tr>
<tr><td>✅</td><td><b>0.5.0-rc.10</b> (2026-10-19)</td><td>strict-English default locale with domain-split <code>locales/{en,zh}/</code>, <code>rustls</code> 0.23.46 security upgrade, unified pure-MIT license, release switched to crates.io trusted publishing (OIDC) with the idempotency triple (see the <a href="../CHANGELOG.md">CHANGELOG</a>).</td></tr>
<tr><td>✅</td><td><b>0.5.0-rc.9</b> (2026-10-10)</td><td>service-probe registry (new <code>probe</code> feature), async health aggregation (<code>health_aggregate</code> / <code>health_json</code>), explicit <code>config_overrides</code> in build reports, feature-matrix and benchmark-regression CI gates; fixes for version-negotiation path parity, unified lazy retrieval semantics, and shutdown-coordinator hook panic isolation (see the <a href="../CHANGELOG.md">CHANGELOG</a>).</td></tr>
<tr><td>✅</td><td><b>0.5.0-rc.6</b> (2026-09-21)</td><td>rc.4–rc.6 released: observability ports and event bus, full Toggle implementation, <code>trait-kit-derive</code> merged into <code>trait-kit-macros</code>, <code>confers</code> dependency moved to crates.io (see the <a href="../CHANGELOG.md">CHANGELOG</a>).</td></tr>
<tr><td>✅</td><td><b>Performance benchmarks</b></td><td>Criterion benchmarks in <code>benches/kit_bench.rs</code> and the <code>docs/cn/PERFORMANCE.md</code> baseline report are in place (current baseline 2026-09-29).</td></tr>
<tr><td>📋</td><td><b>0.5.0 stable release</b></td><td>After the minor version bump, sync the <code>path + version</code> dependency requirements of downstream crates (oxcache, dbnexus, inklog, limiteron, sdforge) per the workspace release plan.</td></tr>
<tr><td>✅</td><td><b>cfg gate completeness</b></td><td><code>--no-default-features --features async</code> and <code>async,observer</code> check combos, <code>clippy --features async --all-targets</code> are warning-free, and async doc tests all pass — the gate gap is resolved (re-verified 2026-09-27).</td></tr>
</table>
