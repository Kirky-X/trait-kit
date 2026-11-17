# 🔒 Trait-Kit Security

This document describes trait-kit's version support policy, vulnerability reporting process, and the library's real security mechanisms and usage recommendations.

## 📋 Table of Contents

<details open>
<summary>📑 Table of Contents</summary>

- [📌 Supported Versions and Maintenance Policy](#-supported-versions-and-maintenance-policy)
- [🚨 Vulnerability Reporting Process](#-vulnerability-reporting-process)
- [🛡️ Security Design Overview](#️-security-design-overview)
- [✅ Security Best Practices](#-security-best-practices)

</details>

---

## 📌 Supported Versions and Maintenance Policy

The project is in its 0.x pre-release stage and **only maintains the current minor line** (`0.5.x`, including rc pre-releases within that line).

- **Current line**: receives defect and security fixes; fixes ship with the latest release on that line.
- **Older minor lines**: enter EOL as soon as the first release of the next minor line is published; no further fixes of any kind, no backports of security patches.
- **No LTS**: before 1.0 there is no cross-minor API compatibility promise; minor bumps may contain breaking changes — read the [CHANGELOG](../CHANGELOG.md) before upgrading.
- **crates.io visibility ≠ supported**: old versions remain downloadable (unless yanked for severe defects) but are out of support scope.

| Version line | Status | Notes |
|--------|------|------|
| `0.5.x` (current) | ✅ Supported | The only line receiving fixes |
| `≤ 0.4.x` | ❌ EOL | Superseded by the current line, unmaintained — please upgrade |

> This crate follows semantic versioning: during 0.x, minor bumps may include breaking changes (e.g. the BREAKING CHANGES of 0.4.0) — read the [CHANGELOG](../CHANGELOG.md) when upgrading.

---

## 🚨 Vulnerability Reporting Process

- **Please do not report security vulnerabilities through public GitHub Issues** to avoid exposing vulnerability details before a fix is available.
- **Preferred channel**: GitHub's private vulnerability reporting channel, [Security Advisories](https://github.com/Kirky-X/trait-kit/security/policy) (repo page **Security → Report a vulnerability**).
- **Fallback channel**: if the preferred channel is unavailable, email **Kirky-X@outlook.com**.
- When reporting, please include: affected versions, reproduction steps or proof-of-concept code, and an impact assessment where possible.

### ⏱️ Response Commitments

- Acknowledge receipt within **48 hours**
- Provide an initial assessment (accepted/rejected/more info needed) within **7 days**
- Before the fix ships, the reporter gets a preview-validation opportunity for the patch
- After the fix ships, the reporter is credited in the CHANGELOG and the advisory (anonymously if preferred)

**Security hardening channel**: beyond vulnerability reports, repo CI continuously runs dependency and static-analysis gates (cargo-audit + cargo-deny, CodeQL, plus detect-secrets / detect-private-key secret scanning in pre-commit; see "CI security gates" below), with a historical record of zero unhandled Critical/High findings.

---

## 🛡️ Security Design Overview

Below are security-relevant mechanisms that really exist in trait-kit (all verifiable from the source and the CHANGELOG):

### unsafe code denied by default

The whole crate is annotated `#![deny(unsafe_code)]`, compile-time exclusion of one of the main sources of undefined behavior. Dependency-graph validation, type retrieval, and the like are all implemented in safe Rust. Production code currently has 5 locally audited exemptions (`#[allow(unsafe_code)]`) with SAFETY justifications and code review: typestate layout-assert casts ×3 (the require/factory paths of `Kit`/`AsyncKit`), volatile key zeroization ×1, and guard lifetime re-anchoring ×1 (the `RwLock` read guard of `AsyncTypeMap`; the sync `TypeMap`'s `RefCell` guard now uses a `Ref::map` wrapper, keeping the read borrow and the value reference inseparable without unsafe); plus 1 test-module exemption (`src/kit/config.rs`'s `key_bytes_zeroize_clears_spare_capacity`, verifying via volatile reads that spare capacity is zeroed) — each carries a SAFETY comment and is subject to the review gate.

### Type-safe capability retrieval

Capabilities are stored in the `TypeMap` keyed by `TypeId` and retrieved by module type (`kit.require::<M>()`). There are no string-key lookups and no downcast paths, reducing exposure to runtime type-confusion classes of problems.

### Build-time validation (typestate)

The two-phase typestate `Kit<Unbuilt>` → `Kit<Ready>` turns typical misuse into compile errors (see the trybuild assertions under `tests/ui/`: calling `optional()` on `Kit<Unbuilt>` and register/build on `Kit<Ready>` are rejected at compile time; `require()` compiles in both states and returns a runtime error for unbuilt modules). The dependency graph is checked at `build()` for missing dependencies and cycles, surfacing assembly errors before application startup.

### Explicit thread-safety boundary

The sync side (`Kit` / `TypeMap` / `Scope`) is based on `RefCell`, single-threaded `!Send + !Sync`; the async side (`AsyncKit` / `AsyncTypeMap` / `AsyncScope`) is based on `Arc<RwLock>`, `Send + Sync`. For the per-type Send/Sync table see [Architecture · Thread-Safety Model](ARCHITECTURE.md#-thread-safety-model).

The compiler enforces that `Kit` cannot be shared across threads, excluding that misuse at the type level.

### Encrypted config storage `encryption`

- Algorithm: XChaCha20-Poly1305 authenticated encryption (AEAD) for static config storage.
- Key derivation: encryption keys are derived via HKDF from the master key and `ModuleConfig::PATH`; the same master key yields distinct field keys for different modules, limiting cross-module key reuse.
- Anti-leak: `EncryptedBlob`'s `Debug` implementation never outputs encrypted material (a security fix in 0.4.1, see the [CHANGELOG](../CHANGELOG.md)).
- Master-key admission: byte-passing entry points (`set_encrypted` / `get_encrypted` / `rotate_master_key`) require a master key of at least 32 bytes with non-constant byte material (all-zero, repeated bytes); constant-byte keys can be recomputed by anyone, so accepting them would mean no encryption. This policy matches the provider supply path (`confers::SecretKeyProvider` rejects the same material with `WeakKey`), so the two supply paths no longer give opposite verdicts for the same material.

### Error messages and internationalization

`TraitKitError` is the unified error type; `Display` output is localized via `tr()`; raw underlying error text is uniformly redacted before entering public text (credential fragments and server absolute paths are masked, text truncated), while the unredacted original is preserved in the structured `source` for programmatic handling rather than relying on text alone.

- **Internal call sites stay out of text**: the `context` of `BuildFailed` / `LifecycleFailed` is folded into external category labels before entering Fluent templates (`load_config` → `configuration load`, `set_encrypted` → `encrypted configuration write`). Folding applies only to registered internal call-site labels (a closed set; new labels must be registered or compilation fails), not guessed from string shapes — module names (`ModuleMeta::NAME`) and localized diagnostic phrases are preserved as-is; the provider type name of master-key suppliers is folded by family prefix (`key provider (env)` → `master key provider`). The external category labels themselves are localized via the Fluent catalog (`trait-kit-context-*`), producing the localized text under the Chinese catalog. Raw labels remain readable through the variants' public fields, so programmatic matching is unaffected.
- **Debug output is redacted too**: `TraitKitError`'s `Debug` is a hand-written implementation that outputs underlying error text after redaction — downstream `{:?}` or tracing's `?err` logging will not leak server absolute paths or secrets.
- **Internal type paths stay out of text**: config type names entering error text, config-change events, and override records are trimmed of crate/module paths first (`my_crate::config::DbConfig` → `DbConfig`); generic arguments are trimmed while the container name is kept (`core::option::Option<crate::Db>` → `Option`).

### Field filtering for outbound payloads

- **Health / readiness detail**: credential fragments (URL userinfo, `password=` / `token=` key-value pairs (including whitespace around the separator), credentials following auth schemes such as `Bearer` / `Basic`, and private-key material in PEM blocks from `-----BEGIN` to `-----END`) are replaced with `[REDACTED]` before entering JSON payloads and the event bus, with overlong text truncated.
- **Config key names**: `ConfersConfigHandle::keys()` and `RemoteConfigProvider::keys()` filter out credential-type key names such as `password` / `api_key` / `private_key` — the key names themselves reveal which credentials a deployment holds; values remain readable via explicit accessors by name.
- **Shared namespace**: `extract_shared` / `inject_shared` filter credential-type field names: the overlay carries ownership (the first extractor owns the key; a different config type extracting the same key is rejected with a warning), but ownership only blocks same-name conflicts — once credential plaintext enters, it will still flow into other config types via injection and leave copies that are never zeroized.

### Structural constraints on untrusted configs

The depth guard of `load_config_with` covers both the serialization and deserialization sides: `serde_json`'s `to_value` / `from_value` have no depth limits, and overly deep structures would exhaust the call stack inside them and terminate the process directly (stack overflow cannot be caught by `catch_unwind`). Serialization goes through a depth-guarded writer (aborting once nesting exceeds the cap, pinning recursion depth to the limit), and deserialization additionally performs a depth pre-check — overly deep configs return as catchable errors instead of terminating the process.

### CI security gates

- `cargo deny check`: dependency license/advisory/duplicate-dependency audit (mandatory CI gate).
- `cargo audit`: RustSec advisory audit (mandatory CI gate; also runs in lefthook's pre-push stage).
- CodeQL static analysis (`.github/workflows/codeql.yml`).
- pre-commit hooks: `detect-secrets` (with the `.secrets.baseline` baseline) and `detect-private-key` private-key detection.
- The release process (`release.yml`) performs all-feature builds, test validation, and crates.io listing checks; SAST static analysis is handled by CI's CodeQL, and the release workflow itself contains no separate scan step.

---

## ✅ Security Best Practices

When building applications with trait-kit:

1. **Guard the master key**: the 32-byte master key of the `encryption` feature is the root of trust for encrypted config; obtain it from environment variables, a key management service, or a hardware key source; never hardcode it into source or commit it to the repo (`set_encrypted` / `get_encrypted` validate the master key at call time: undersized (under 32 bytes) or all-zero and similar weak key material returns `BuildFailed`; for a demo key see the [User Guide](USER_GUIDE.md)).
2. **Encrypt sensitive configs**: static sensitive config such as database passwords and API keys should use `set_encrypted` / `get_encrypted`, avoiding plaintext residing in memory structures or logs.
3. **Validate configs up front**: externally sourced configs should be validated at load time with `Validatable` + `load_and_validate::<C>()`; failures are not stored, preventing malicious/corrupted configs from entering the runtime.
4. **Keep dependencies updated**: run `cargo audit` and `cargo deny check` regularly (built into CI) and follow up on transitive-dependency advisories promptly.
5. **Enable features on demand**: enable only the features you actually need to shrink the code surface compiled into the binary; higher-level confers features automatically inherit lower-level ones (`encryption` → `reload` → `confers`).
6. **Pick single-thread/multi-thread as needed**: use `Kit` (`!Sync`) for single-threaded scenarios and do not force cross-thread sharing past the type system; use `AsyncKit` for multi-threaded scenarios.
7. **Upgrade promptly**: watch security-related entries in the [CHANGELOG](../CHANGELOG.md) (e.g. the 0.4.1 `EncryptedBlob` Debug fix) and upgrade to versions containing fixes.
