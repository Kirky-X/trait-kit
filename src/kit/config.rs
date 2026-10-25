// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Configuration loader trait for the "loader pattern" integration with confers.
//!
//! trait-kit defines the `Configurable` trait as a backend-agnostic interface;
//! users bridge to `confers::Config` derive macro's `load_sync` (or any other
//! source) by implementing this trait. The Kit then loads and stores the value
//! through its `TypeMap` backend, keeping `set_config`/`config` synchronous and
//! type-safe.
//!
//! Level 2 (`confers` feature) adds the `ModuleConfig` trait for
//! module-level config metadata (path + default) and re-exports the
//! `confers::Config` derive macro so users can `use trait_kit::kit::Config;`.
//!
//! # Three-tier inheritance system (三层继承)
//!
//! The confers integration is built on a three-tier inheritance model:
//!
//! 1. **Module capability inheritance (模块能力继承)** — `#[derive(Config)]`
//!    auto-implements serialization, deserialization, reload subscription,
//!    encryption markers, and validation rules. `ModuleConfig` binds each
//!    config type to its module's configuration path (`PATH`).
//!
//! 2. **Cargo feature inheritance (cargo feature 继承)** — feature flags form
//!    a dependency chain: `encryption` → `reload` →
//!    `confers`. Enabling a higher level automatically
//!    enables all lower levels.
//!
//! 3. **Config value inheritance (配置值继承)** — the encryption key is
//!    derived from `ModuleConfig::PATH` via HKDF, so the same master key
//!    produces different field keys for different modules.

/// Trait for types that can load themselves from a configuration source.
///
/// Implementors typically delegate to `confers::Config`'s derived `load_sync`
/// method, but any loader (file parse, env scan, network fetch) is allowed.
///
/// # Errors
///
/// Implementations should return an error when loading fails (missing file,
/// invalid format, type mismatch, etc.).
#[cfg(feature = "confers")]
pub trait Configurable: Clone + 'static {
    /// Load the configuration value from its source.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration could not be loaded.
    fn load() -> Result<Self, Box<dyn std::error::Error + Send + 'static>>;
}

/// Re-export of the `confers::Config` derive macro.
///
/// Allows `use trait_kit::kit::Config;` to derive the configuration loader
/// implementation backed by confers' `load_sync` / `load_file` codegen.
#[cfg(feature = "confers")]
pub use confers::Config;

/// Trait for module-level configuration metadata.
///
/// Layer 1 of the three-tier inheritance system: each module declares its
/// configuration path and a default value. Combined with `#[derive(Config)]`
/// (re-exported as [`Config`]), modules gain both loading and fallback
/// capabilities. `ModuleConfig` does not require `Configurable` — a module
/// may provide a default without a loader, or vice versa.
///
/// `default_value` is not invoked automatically by `Kit` internally;
/// callers must opt-in via [`Kit::load_config_or_default`](super::kit::Kit::load_config_or_default)
/// when they want load-with-fallback semantics.
#[cfg(feature = "confers")]
pub trait ModuleConfig: Clone + 'static {
    /// Configuration file path relative to the application root.
    const PATH: &'static str;

    /// Return the default configuration value (fallback when loading fails
    /// or no source is configured).
    fn default_value() -> Self;
}

/// Trait for configuration types that support validation after loading.
///
/// Implementors define validation rules that are checked by
/// `Kit::load_and_validate` after the configuration is loaded. If validation
/// fails, the configuration is not stored in the Kit.
///
/// This trait is backend-agnostic — users may implement validation by hand,
/// via `garde`, or any other mechanism.
#[cfg(feature = "confers")]
pub trait Validatable: Clone + 'static {
    /// Validate the configuration value.
    ///
    /// Returns `Ok()` if valid, or `Err` with all failure reasons.
    ///
    /// # Errors
    ///
    /// Returns `Err(Vec<String>)` containing every validation failure
    /// when the configuration is invalid.
    fn validate(&self) -> Result<(), Vec<String>>;
}

/// Error type for configuration validation failures.
///
/// Wraps a list of validation error messages into a single `Error + Send`
/// suitable for `TraitKitError::BuildFailed::source`.
#[cfg(feature = "confers")]
#[derive(Debug)]
pub struct ValidationError {
    /// Individual validation failure messages.
    pub errors: Vec<String>,
}

#[cfg(feature = "confers")]
impl ValidationError {
    /// Create a `ValidationError` from a list of validation failure messages.
    #[must_use]
    pub fn new(errors: Vec<String>) -> Self {
        Self { errors }
    }
}

#[cfg(feature = "confers")]
impl From<Vec<String>> for ValidationError {
    fn from(errors: Vec<String>) -> Self {
        Self { errors }
    }
}

#[cfg(feature = "confers")]
impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let errors = self.errors.join("; ");
        write!(
            f,
            "{}",
            crate::i18n::tr(
                "trait-kit-error-config-validation-failed",
                &[("errors", errors.as_str())],
            )
        )
    }
}

#[cfg(feature = "confers")]
impl std::error::Error for ValidationError {}

/// Trait for compile-time safe field-level configuration override.
///
/// Types implementing `ConfigInherit` declare an associated `Override` type
/// where each field is wrapped in `Option<T>`. Only fields set to `Some`
/// in the override are applied; `None` fields leave the original value intact.
///
/// Designed for use with `#[derive(ConfigInherit)]` from `trait-kit-macros`,
/// which auto-generates the `Override` type and `apply_override` implementation.
///
/// # Example (manual implementation)
///
/// ```ignore
/// struct DbConfig { host: String, port: u16 }
///
/// #[derive(Clone, Default)]
/// struct DbConfigOverride {
///     host: Option<String>,
///     port: Option<u16>,
/// }
///
/// impl ConfigInherit for DbConfig {
///     type Override = DbConfigOverride;
///     fn apply_override(&mut self, ovr: &Self::Override) {
///         if let Some(ref h) = ovr.host { self.host = h.clone; }
///         if let Some(ref p) = ovr.port { self.port = *p; }
///     }
/// }
/// ```
#[cfg(feature = "confers")]
pub trait ConfigInherit: Clone + 'static {
    /// Override type with each field wrapped in `Option<T>`.
    type Override: Clone + Default + 'static;

    /// Apply non-`None` fields from the override to `self`.
    fn apply_override(&mut self, ovr: &Self::Override);
}

/// Trait for declaring which config fields participate in the shared namespace.
///
/// The shared namespace is an owner-tracked `serde_json::Value` overlay inside
/// the Kit that bridges values across different config types. When project A
/// and project B both declare `host` as a shared field, A's `host` value
/// flows to B automatically via `extract_shared` → `inject_shared`.
///
/// # Ownership contract
///
/// The overlay records which config type declared each key. The first type
/// to extract a key owns it: a second, different type extracting the same
/// key is refused (with a warning) instead of silently overwriting — two
/// types declaring `host` may mean different things (DB host vs Redis host),
/// and the later declaration must not replace the earlier value.
/// Re-extracting from the owning type itself (e.g. after a config reload)
/// still updates the value. Cross-type flow stays one-directional by design:
/// values enter the overlay from the owner and reach other types only via
/// `inject_shared`.
///
/// Values are kept as plain `serde_json::Value` (no zeroization).
/// Credential-like field names (`password`, `api_key`, ...) are filtered out
/// by the Kit before they enter the overlay and before they are injected, so
/// a plaintext credential does not cross config types — declare only fields
/// whose cross-type flow is intended.
///
/// Uses `serde_json::Value` (not `String`) to preserve type information and
/// avoid parse failures.
///
/// Designed for use with `#[derive(SharedConfig)]` from `trait-kit-macros`,
/// which parses `#[shared(field1, field2)]` attributes to auto-generate
/// both methods.
#[cfg(feature = "confers")]
pub trait SharedConfig: Clone + 'static {
    /// Extract shared fields as a JSON map.
    fn extract_shared(&self) -> serde_json::Map<String, serde_json::Value>;

    /// Inject shared fields from a JSON map into `self`.
    ///
    /// # Contract (implementors)
    ///
    /// Implementors MUST silently skip keys whose value type does not match
    /// the target field type, and MUST NOT panic on any input — including
    /// unknown keys, mismatched types, and malformed values. Unknown keys are
    /// ignored; mismatched values leave the corresponding field unchanged.
    /// This mirrors the behavior of `#[derive(SharedConfig)]` (see the
    /// `derive_shared_config_inject_skips_type_mismatch` test in
    /// `tests/shared_config_derive.rs`).
    fn inject_shared(&mut self, shared: &serde_json::Map<String, serde_json::Value>);
}

use std::collections::HashMap;
use std::hash::BuildHasher;

/// Owner-tracked shared-field overlay (one per Kit).
///
/// `fields` carries the shared values; `owners` records the short config
/// type name that declared each key. See [`SharedConfig`] for the ownership
/// contract.
#[cfg(feature = "confers")]
#[derive(Default)]
pub(crate) struct SharedOverlay {
    pub(crate) fields: serde_json::Map<String, serde_json::Value>,
    owners: HashMap<String, String>,
}

#[cfg(feature = "confers")]
impl SharedOverlay {
    /// Merge `fields` into the overlay under owner `owner`.
    ///
    /// A key already owned by a different config type is refused (returned
    /// in the rejected list) instead of overwritten; re-merging from the
    /// owning type updates the value.
    pub(crate) fn merge(
        &mut self,
        fields: serde_json::Map<String, serde_json::Value>,
        owner: &str,
    ) -> Vec<String> {
        let mut rejected = Vec::new();
        for (key, value) in fields {
            match self.owners.get(&key) {
                Some(existing) if *existing != owner => rejected.push(key),
                _ => {
                    self.owners.insert(key.clone(), owner.to_string());
                    self.fields.insert(key, value);
                }
            }
        }
        rejected
    }

    pub(crate) fn snapshot(&self) -> serde_json::Map<String, serde_json::Value> {
        self.fields.clone()
    }
}

/// Interpolate `${VAR}` and `${VAR:-default}` patterns in a JSON value.
///
/// Walks the JSON structure, replacing patterns in String values only. Object
/// keys and non-String variants are left unchanged. Unknown variables without
/// a default are preserved as-is. `$$` escapes to a literal `$` (see
/// `interpolate_string`).
#[cfg(feature = "confers")]
pub fn interpolate_json_value<S: BuildHasher>(
    value: &mut serde_json::Value,
    vars: &HashMap<String, String, S>,
) {
    // 显式工作栈代替函数递归：输入可能来自不受信的深层嵌套 JSON，
    // 递归展开会在超深结构上耗尽调用栈。
    let mut stack: Vec<&mut serde_json::Value> = vec![value];
    while let Some(current) = stack.pop() {
        match current {
            serde_json::Value::String(s) => {
                *s = interpolate_string(s, vars);
            }
            serde_json::Value::Array(arr) => {
                stack.extend(arr.iter_mut());
            }
            serde_json::Value::Object(map) => {
                stack.extend(map.values_mut());
            }
            _ => {}
        }
    }
}

/// Deep-merge `overlay` into `base`.
///
/// - When both `base` and `overlay` are JSON Objects, merge key-by-key:
///   - Keys only in `overlay` are inserted into `base`.
///   - Keys in both where both values are Objects are merged recursively.
///   - Keys in both where values are not both Objects: `overlay` wins (replaces).
/// - Arrays are treated as atomic values (replaced, not element-wise merged).
/// - Scalars are replaced.
///
/// 合并深度与输入嵌套深度一致。实现改用显式工作栈而非函数递归：合并
/// 深度再深也不会耗尽调用栈（`serde_json` 解析器另有默认 128 层嵌套上限，
/// 该上限只作用于文本解析路径，不约束 `from_value`/`to_value` 与程序化
/// 构造的 `Value`）。
///
/// 注意 `serde_json::Value` 的 `clone`/`Drop` 由 `serde_json` 实现且本身
/// 递归：手工构造的超深 `Value` 在这些操作上仍可能耗尽栈，这属于
/// `serde_json` 的固有属性，不在本函数的控制范围内。
#[cfg(feature = "confers")]
pub fn merge_json_deep(base: &mut serde_json::Value, overlay: &serde_json::Value) {
    // 显式工作栈代替函数递归：合并深度等于输入嵌套深度，递归展开会在
    // 超深结构上耗尽调用栈。栈元素是「base 内路径 + overlay 节点」——
    // serde_json 的 `Map::get_mut` 每次独占借用整张 map，无法像
    // `interpolate_json_value` 那样同时持有多个子引用。
    let mut stack: Vec<(Vec<String>, &serde_json::Value)> = vec![(Vec::new(), overlay)];
    while let Some((path, node)) = stack.pop() {
        let base_is_object = matches!(
            lookup_json_path(base, &path),
            Some(serde_json::Value::Object(_))
        );
        match node {
            serde_json::Value::Object(overlay_map) if base_is_object => {
                for (key, child) in overlay_map {
                    let mut child_path = path.clone();
                    child_path.push(key.clone());
                    if lookup_json_path(base, &child_path).is_some() {
                        stack.push((child_path, child));
                    } else {
                        write_json_path(base, &child_path, child.clone());
                    }
                }
            }
            other => {
                write_json_path(base, &path, other.clone());
            }
        }
    }
}

/// 查找 base 中 `path` 指向的节点；`path` 为 base 内对象键的序列。
#[cfg(feature = "confers")]
fn lookup_json_path<'a>(
    base: &'a serde_json::Value,
    path: &[String],
) -> Option<&'a serde_json::Value> {
    let mut current = base;
    for key in path {
        current = current.as_object()?.get(key)?;
    }
    Some(current)
}

/// 把 `value` 写入 base 的 `path` 处：末段键整体替换，空路径替换根节点。
///
/// 调用方保证 `path` 的父节点存在且为对象（合并过程只会下降到对象内部）。
#[cfg(feature = "confers")]
fn write_json_path(base: &mut serde_json::Value, path: &[String], value: serde_json::Value) {
    let Some((last, parents)) = path.split_last() else {
        *base = value;
        return;
    };
    let mut parent = base;
    for key in parents {
        parent = parent
            .get_mut(key)
            .expect("merge path parent exists while merging");
    }
    parent
        .as_object_mut()
        .expect("merge path parent is an object while merging")
        .insert(last.clone(), value);
}

/// 配置值允许的最大嵌套层数。
///
/// 序列化与反序列化两侧都用这个上限把递归深度钉死：
/// - 序列化走 [`to_json_text_bounded`]（深度守卫 `Formatter`）：嵌套超过
///   上限即中止序列化，递归深度不再随配置深度增长。
/// - 反序列化前有 [`exceeds_json_depth`] 预检：`serde_json::from_value`
///   不限深度，超深结构会在其中耗尽调用栈并直接终止进程。
///
/// 取 64：远低于 `serde_json` 文本解析器自身的 128 层上限，深度受限的
/// 序列化产物回读时仍在解析器上限内。
#[cfg(feature = "confers")]
pub(crate) const MAX_CONFIG_DEPTH: usize = 64;

/// 深度受限的 JSON 构造失败。
#[cfg(feature = "confers")]
pub(crate) enum BoundedJsonError {
    /// 嵌套深度超过上限：序列化已在此中止（递归深度被上限钉死）。
    DepthExceeded {
        /// 生效的深度上限。
        max_depth: usize,
    },
    /// 其它序列化错误。
    Serialize(serde_json::Error),
}

/// 深度守卫写入器：统计输出 JSON 的括号深度，超过上限即中止写入。
///
/// `serde_json` 的 `Formatter` 钩子默认不写任何内容（分隔符由
/// `CompactFormatter` 之类负责），深度守卫因此放在写入侧：输出文本的
/// 括号深度就是回读解析时的嵌套深度，上限命中即中止序列化——递归深度
/// 被上限钉死，超深配置走不到把调用栈耗尽的那一步。
///
/// 局限：字符串值里的括号也计入计数。配置里的花括号通常成对出现，净
/// 不平衡的括号要累积到上限才会误判，实践中不可达。
#[cfg(feature = "confers")]
struct DepthGuardWriter<W> {
    inner: W,
    depth: usize,
    max_depth: usize,
    depth_exceeded: std::rc::Rc<std::cell::Cell<bool>>,
}

#[cfg(feature = "confers")]
impl<W: std::io::Write> std::io::Write for DepthGuardWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for &byte in buf {
            match byte {
                b'{' | b'[' => {
                    self.depth += 1;
                    if self.depth > self.max_depth {
                        self.depth_exceeded.set(true);
                        return Err(std::io::Error::other(crate::i18n::tr(
                            "trait-kit-error-config-nesting-depth-exceeded",
                            &[("max_depth", &self.max_depth.to_string())],
                        )));
                    }
                }
                b'}' | b']' => self.depth = self.depth.saturating_sub(1),
                _ => {}
            }
        }
        self.inner.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// 深度受限的 JSON 文本构造。
///
/// `serde_json::to_value` 的递归深度等于配置嵌套深度：超深配置会在
/// 序列化阶段耗尽调用栈并终止进程（`catch_unwind` 也捕不到栈溢出）。
/// 这里改用带深度守卫的写入器：嵌套超过上限即中止序列化，递归深度
/// 被上限钉死；产物回读时仍受 `serde_json` 文本解析器自身的上限保护。
#[cfg(feature = "confers")]
pub(crate) fn to_json_text_bounded<T: serde::Serialize>(
    value: &T,
    max_depth: usize,
) -> Result<String, BoundedJsonError> {
    let depth_exceeded = std::rc::Rc::new(std::cell::Cell::new(false));
    // `String` 不实现 `io::Write`：先写进字节缓冲，再按 UTF-8 读回。
    let mut out: Vec<u8> = Vec::new();
    {
        let mut writer = DepthGuardWriter {
            inner: &mut out,
            depth: 0,
            max_depth,
            depth_exceeded: std::rc::Rc::clone(&depth_exceeded),
        };
        let mut serializer =
            serde_json::Serializer::with_formatter(&mut writer, serde_json::ser::CompactFormatter);
        match serde::Serialize::serialize(value, &mut serializer) {
            Ok(()) => {}
            Err(_) if depth_exceeded.get() => {
                return Err(BoundedJsonError::DepthExceeded { max_depth });
            }
            Err(e) => return Err(BoundedJsonError::Serialize(e)),
        }
    }
    match String::from_utf8(out) {
        Ok(text) => Ok(text),
        Err(e) => Err(BoundedJsonError::Serialize(serde_json::Error::io(
            std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        ))),
    }
}

/// 判定 JSON 值的嵌套深度是否超过上限（显式工作栈，不递归）。
#[cfg(feature = "confers")]
pub(crate) fn exceeds_json_depth(value: &serde_json::Value, max_depth: usize) -> bool {
    let mut stack: Vec<(&serde_json::Value, usize)> = vec![(value, 0)];
    while let Some((current, depth)) = stack.pop() {
        if depth > max_depth {
            return true;
        }
        match current {
            serde_json::Value::Array(items) => {
                stack.extend(items.iter().map(|item| (item, depth + 1)));
            }
            serde_json::Value::Object(map) => {
                stack.extend(map.values().map(|item| (item, depth + 1)));
            }
            _ => {}
        }
    }
    false
}

/// Replace `${VAR}` and `${VAR:-default}` patterns in a single string.
///
/// `$$` is an escape sequence for a literal `$`: `$$` collapses to `$`, so
/// `$${VAR}` yields the literal text `${VAR}` without interpolation. A single
/// `$` not followed by `{` is emitted as-is.
#[cfg(feature = "confers")]
fn interpolate_string<S: BuildHasher>(s: &str, vars: &HashMap<String, String, S>) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'$') {
            // `$$` escapes to a literal `$` (so `$${VAR}` is not interpolated).
            chars.next(); // consume second '$'
            result.push('$');
        } else if ch == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut var_name = String::new();
            let mut found_close = false;
            let mut has_default = false;
            let mut default_value = String::new();
            while let Some(c) = chars.next() {
                if c == '}' {
                    found_close = true;
                    break;
                }
                if c == ':' && !has_default {
                    // Check for `:-` default syntax
                    if chars.peek() == Some(&'-') {
                        chars.next(); // consume '-'
                        has_default = true;
                        continue;
                    }
                }
                if has_default {
                    default_value.push(c);
                } else {
                    var_name.push(c);
                }
            }
            if found_close {
                if let Some(val) = vars.get(&var_name) {
                    result.push_str(val);
                } else if has_default {
                    result.push_str(&default_value);
                } else {
                    // Preserve original pattern
                    result.push_str("${");
                    result.push_str(&var_name);
                    result.push('}');
                }
            } else {
                // Unclosed `${`, preserve as-is
                result.push_str("${");
                result.push_str(&var_name);
                if has_default {
                    result.push_str(":-");
                    result.push_str(&default_value);
                }
            }
        } else {
            result.push(ch);
        }
    }
    result
}

/// Re-export of confers' XChaCha20-Poly1305 cipher (synchronous API).
#[cfg(feature = "encryption")]
pub use confers::XChaCha20Crypto;

/// Re-export of confers' HKDF-based per-field key derivation.
#[cfg(feature = "encryption")]
pub use confers::derive_field_key;

/// Encrypted configuration blob: nonce + ciphertext.
///
/// Stored in `Kit`'s `encrypted_configs` map keyed by `TypeId`. Use
/// [`Kit::set_encrypted`](super::kit::Kit::set_encrypted) /
/// [`Kit::get_encrypted`](super::kit::Kit::get_encrypted) to populate
/// and read values.
///
/// Layer 3 of the inheritance system: the encryption key is derived from
/// `ModuleConfig::PATH`, so the encrypted blob is bound to the module's
/// declared configuration path.
#[cfg(feature = "encryption")]
#[derive(Clone)]
pub struct EncryptedBlob {
    /// XChaCha20-Poly1305 nonce (24 bytes).
    nonce: Vec<u8>,
    /// Ciphertext + Poly1305 authentication tag.
    ciphertext: Vec<u8>,
}

#[cfg(feature = "encryption")]
impl std::fmt::Debug for EncryptedBlob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptedBlob")
            .field("nonce", &"[REDACTED]")
            .field("ciphertext", &"[REDACTED]")
            .finish()
    }
}

#[cfg(feature = "encryption")]
impl EncryptedBlob {
    /// Create a new encrypted blob from raw nonce and ciphertext.
    #[must_use]
    pub(crate) fn new(nonce: Vec<u8>, ciphertext: Vec<u8>) -> Self {
        Self { nonce, ciphertext }
    }

    /// Returns the XChaCha20-Poly1305 nonce (24 bytes).
    #[must_use]
    pub fn nonce(&self) -> &[u8] {
        &self.nonce
    }

    /// Returns the ciphertext + Poly1305 authentication tag.
    #[must_use]
    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }
}

#[cfg(all(test, feature = "encryption"))]
mod encrypted_blob_tests {
    use super::EncryptedBlob;

    #[test]
    fn getters_return_raw_slices() {
        let blob = EncryptedBlob::new(vec![1, 2, 3], vec![4, 5, 6]);
        assert_eq!(blob.nonce(), &[1, 2, 3]);
        assert_eq!(blob.ciphertext(), &[4, 5, 6]);
    }

    #[test]
    fn getters_return_empty_for_empty_blob() {
        let blob = EncryptedBlob::new(Vec::new(), Vec::new());
        assert!(blob.nonce().is_empty());
        assert!(blob.ciphertext().is_empty());
    }

    #[test]
    fn clone_produces_equal_blob() {
        let blob = EncryptedBlob::new(vec![1, 2, 3], vec![4, 5, 6]);
        let cloned = blob.clone();
        assert_eq!(blob.nonce(), cloned.nonce());
        assert_eq!(blob.ciphertext(), cloned.ciphertext());
    }

    #[test]
    fn debug_format_redacts_sensitive_data() {
        let blob = EncryptedBlob::new(vec![1, 2, 3], vec![4, 5, 6]);
        let s = format!("{blob:?}");
        assert!(s.contains("EncryptedBlob"));
        assert!(s.contains("[REDACTED]"));
        // Ensure raw byte values are NOT leaked
        assert!(!s.contains("[1, 2, 3]"));
        assert!(!s.contains("[4, 5, 6]"));
    }
}

#[cfg(all(test, feature = "confers"))]
mod merge_json_tests {
    use super::merge_json_deep;
    use serde_json::json;

    #[test]
    fn empty_objects_merge_to_empty() {
        let mut base = json!({});
        let overlay = json!({});
        merge_json_deep(&mut base, &overlay);
        assert_eq!(base, json!({}));
    }

    #[test]
    fn overlay_inserts_new_keys() {
        let mut base = json!({"a": 1});
        let overlay = json!({"b": 2});
        merge_json_deep(&mut base, &overlay);
        assert_eq!(base, json!({"a": 1, "b": 2}));
    }

    #[test]
    fn overlay_replaces_scalars() {
        let mut base = json!({"a": 1, "b": "old"});
        let overlay = json!({"a": 99, "b": "new"});
        merge_json_deep(&mut base, &overlay);
        assert_eq!(base, json!({"a": 99, "b": "new"}));
    }

    #[test]
    fn nested_objects_merge_recursively() {
        let mut base = json!({"a": {"b": 1, "c": 2}});
        let overlay = json!({"a": {"c": 3, "d": 4}});
        merge_json_deep(&mut base, &overlay);
        assert_eq!(base, json!({"a": {"b": 1, "c": 3, "d": 4}}));
    }

    #[test]
    fn deeply_nested_merge() {
        let mut base = json!({"l1": {"l2": {"l3": {"keep": true, "override": "old"}}}});
        let overlay = json!({"l1": {"l2": {"l3": {"override": "new"}}}});
        merge_json_deep(&mut base, &overlay);
        assert_eq!(
            base,
            json!({"l1": {"l2": {"l3": {"keep": true, "override": "new"}}}})
        );
    }

    #[test]
    fn arrays_are_replaced_not_merged() {
        let mut base = json!({"arr": [1, 2, 3]});
        let overlay = json!({"arr": [4]});
        merge_json_deep(&mut base, &overlay);
        assert_eq!(base, json!({"arr": [4]}));
    }

    #[test]
    fn object_replaces_non_object() {
        let mut base = json!({"x": 42});
        let overlay = json!({"x": {"nested": true}});
        merge_json_deep(&mut base, &overlay);
        assert_eq!(base, json!({"x": {"nested": true}}));
    }

    #[test]
    fn non_object_replaces_object() {
        let mut base = json!({"x": {"nested": true}});
        let overlay = json!({"x": 42});
        merge_json_deep(&mut base, &overlay);
        assert_eq!(base, json!({"x": 42}));
    }

    /// 合并走显式工作栈：深嵌套结构不会耗尽调用栈（旧实现按嵌套深度
    /// 递归展开，深结构直接冲垮调用栈）。
    #[test]
    fn deep_nesting_merges_without_recursion() {
        const DEPTH: usize = 500;

        fn nest(depth: usize, leaf: serde_json::Value) -> serde_json::Value {
            let mut current = leaf;
            for _ in 0..depth {
                let mut map = serde_json::Map::new();
                map.insert("n".to_string(), current);
                current = serde_json::Value::Object(map);
            }
            current
        }

        let mut base = nest(DEPTH, serde_json::Value::String("old".into()));
        let overlay = nest(DEPTH, serde_json::Value::String("new".into()));
        merge_json_deep(&mut base, &overlay);

        let mut current = &base;
        for _ in 0..DEPTH {
            current = &current["n"];
        }
        assert_eq!(current, &serde_json::Value::String("new".into()));
    }

    #[test]
    fn null_overlay_replaces_value() {
        let mut base = json!({"a": 1});
        let overlay = json!({"a": null});
        merge_json_deep(&mut base, &overlay);
        assert_eq!(base, json!({"a": null}));
    }
}

#[cfg(all(test, feature = "confers"))]
mod interpolate_string_tests {
    use super::interpolate_string;
    use std::collections::HashMap;

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn double_dollar_yields_literal_brace_pattern() {
        let v = vars(&[]);
        assert_eq!(interpolate_string("$${foo}", &v), "${foo}");
    }

    #[test]
    fn double_dollar_is_not_interpolated_even_when_var_defined() {
        let v = vars(&[("foo", "bar")]);
        assert_eq!(interpolate_string("$${foo}", &v), "${foo}");
    }

    #[test]
    fn double_dollar_collapses_to_single_dollar() {
        let v = vars(&[]);
        assert_eq!(interpolate_string("price: 5$$", &v), "price: 5$");
        assert_eq!(interpolate_string("$$$", &v), "$$");
        assert_eq!(interpolate_string("$$$$", &v), "$$");
    }

    #[test]
    fn single_dollar_without_brace_stays_literal() {
        let v = vars(&[("foo", "bar")]);
        assert_eq!(interpolate_string("cost $5", &v), "cost $5");
        assert_eq!(interpolate_string("$foo", &v), "$foo");
    }

    #[test]
    fn escaped_and_interpolated_patterns_mix() {
        let v = vars(&[("name", "kit")]);
        assert_eq!(
            interpolate_string("hello ${name}, literal $${name}", &v),
            "hello kit, literal ${name}"
        );
    }

    #[test]
    fn escaped_dollar_before_default_pattern() {
        let v = vars(&[]);
        assert_eq!(interpolate_string("$${X:-y}", &v), "${X:-y}");
    }

    #[test]
    fn interpolation_still_works_after_escape() {
        let v = vars(&[("A", "1")]);
        assert_eq!(interpolate_string("$${A}${A}", &v), "${A}1");
    }
}

/// Zeroizing master-key container.
///
/// Alias of confers [`SecretBytes`](confers::SecretBytes): borrow via
/// [`KeyBytes::as_slice`], construct via [`KeyBytes::new`]. Drop zeroization
/// covers the entire allocation — including spare capacity beyond `len` —
/// whereas the former in-house struct only cleared the initialized range.
#[cfg(feature = "encryption")]
pub type KeyBytes = confers::SecretBytes;

/// Master-key provider port.
///
/// Decouples encrypted-config storage from hardcoded key material: callers
/// inject a provider (env, file, KMS, ...) and `Kit::set_encrypted_with_key_provider`
/// pulls the key at call time. Requires the `encryption` feature.
#[cfg(feature = "encryption")]
pub trait KeyProvider: Send + Sync {
    /// Provide the master key. Called once per encryption operation.
    ///
    /// # Errors
    ///
    /// Returns a `TraitKitError` if the key cannot be retrieved (missing env
    /// var, locked keystore, wrong length, ...). Fail closed — never return a
    /// fallback key.
    fn master_key(&self) -> Result<KeyBytes, crate::error::TraitKitError>;

    /// Provider type name for diagnostics (e.g. `"env"`, `"confers-secret"`).
    fn provider_type(&self) -> &'static str {
        "custom"
    }
}

/// Adapter: any confers [`SecretKeyProvider`](confers::secret::SecretKeyProvider)
/// becomes a trait-kit [`KeyProvider`].
///
/// This is the bridge that lets downstream crates reuse the confers key
/// ecosystem (env/file/KMS providers) inside trait-kit's encrypted config.
#[cfg(feature = "encryption")]
pub struct ConfersKeyProvider<P> {
    inner: P,
}

#[cfg(feature = "encryption")]
impl<P> ConfersKeyProvider<P> {
    /// Wrap a confers secret-key provider.
    #[must_use]
    pub fn new(inner: P) -> Self {
        Self { inner }
    }
}

#[cfg(feature = "encryption")]
impl<P> KeyProvider for ConfersKeyProvider<P>
where
    P: confers::secret::SecretKeyProvider,
{
    fn master_key(&self) -> Result<KeyBytes, crate::error::TraitKitError> {
        let secret =
            self.inner
                .get_key()
                .map_err(|e| crate::error::TraitKitError::BuildFailed {
                    context: crate::error::key_provider_context(self.inner.provider_type()),
                    source: Box::new(e),
                })?;
        Ok(KeyBytes::new(secret.as_slice().to_vec()))
    }

    fn provider_type(&self) -> &'static str {
        self.inner.provider_type()
    }
}

#[cfg(all(test, feature = "encryption"))]
mod key_provider_tests {
    use super::*;
    use crate::error::TraitKitError;
    use crate::kit::Kit;
    use static_assertions::assert_not_impl_any;

    // 密钥容器不得可 Clone（克隆即旁路 Drop 零化）：别名底层 confers
    // `SecretBytes` 显式不实现 Clone，编译期钉住防回归。
    assert_not_impl_any!(KeyBytes: Clone);

    #[derive(Clone, serde::Serialize, serde::Deserialize)]
    struct SecretConfig {
        password: String,
    }

    impl ModuleConfig for SecretConfig {
        const PATH: &'static str = "test/key-provider";
        fn default_value() -> Self {
            Self {
                password: String::new(),
            }
        }
    }

    /// Fixed 32-byte confers provider mock (no env mutation needed).
    struct FixedConfersProvider;
    impl confers::secret::SecretKeyProvider for FixedConfersProvider {
        fn get_key(&self) -> Result<confers::SecretBytes, confers::CryptoError> {
            Ok(confers::SecretBytes::new(
                b"provider-key-0123456789abcdefghi".to_vec(),
            ))
        }
        fn provider_type(&self) -> &'static str {
            "fixed-test"
        }
    }

    /// Key provider that fails closed.
    struct BrokenProvider;
    impl KeyProvider for BrokenProvider {
        fn master_key(&self) -> Result<KeyBytes, TraitKitError> {
            Err(TraitKitError::BuildFailed {
                context: "broken".into(),
                source: Box::new(std::io::Error::other("no key")),
            })
        }
    }

    #[test]
    fn confers_adapter_round_trips_encrypted_config() {
        let kit = Kit::new();
        let provider = ConfersKeyProvider::new(FixedConfersProvider);

        kit.set_encrypted_with_key_provider(
            &SecretConfig {
                password: "hunter2".into(),
            },
            &provider,
        )
        .expect("encrypt with provider");

        // Same key material round-trips (`get_encrypted` lives on Kit<Ready>).
        let ready = kit.build().expect("build ok");
        let plain: SecretConfig = ready
            .get_encrypted(provider.master_key().expect("key").as_slice())
            .expect("decrypt");
        assert_eq!(plain.password, "hunter2");
        assert_eq!(provider.provider_type(), "fixed-test");
    }

    #[test]
    fn failing_provider_fails_closed() {
        let kit = Kit::new();
        let err = kit
            .set_encrypted_with_key_provider(
                &SecretConfig {
                    password: "x".into(),
                },
                &BrokenProvider,
            )
            .expect_err("must fail closed");
        assert_eq!(err.kind(), crate::ErrorKind::InitFailed);
        let ready = kit.build().expect("build ok");
        assert!(
            ready
                .get_encrypted::<SecretConfig>(b"other-key-0123456789abcdefghijkl")
                .is_err()
        );
    }

    #[test]
    fn key_bytes_debug_redacts() {
        let key = KeyBytes::new(vec![1, 2, 3]);
        assert_eq!(format!("{key:?}"), "[REDACTED]");
        assert_eq!(key.len(), 3);
    }

    #[test]
    fn key_bytes_len_and_is_empty() {
        assert!(KeyBytes::new(Vec::new()).is_empty(), "empty key is empty");
        let key = KeyBytes::new(vec![9u8; 32]);
        assert!(!key.is_empty());
        assert_eq!(key.len(), 32);
    }

    /// 零化覆盖整个分配（含 spare capacity）：容量大于长度的缓冲经显式
    /// `zeroize` 后全部字节为零（confer 的 `SecretBytes::drop` 即经
    /// `Zeroize for SecretBytes` 走同一路径）。
    ///
    /// 自研 `KeyBytes` 结构体只清 `len` 范围且无 `zeroize` 入口，本测试
    /// 随别名替换落地后生效。
    #[test]
    #[allow(unsafe_code)]
    fn key_bytes_zeroize_clears_spare_capacity() {
        use zeroize::Zeroize;

        let mut inner = Vec::with_capacity(64);
        inner.extend_from_slice(b"top-secret");
        let capacity = inner.capacity();
        assert!(capacity > inner.len(), "fixture needs spare capacity");
        let ptr = inner.as_ptr();

        let mut key = KeyBytes::new(inner);
        assert_eq!(key.as_slice(), b"top-secret");
        key.zeroize();

        // SAFETY: `Vec::zeroize` 先以 volatile 写覆盖 spare capacity 再覆盖
        // 已初始化元素且不重新分配；`ptr` 取自迁移前持有的分配，读写之间
        // 无其他指向该分配的引用存活。volatile 读防止断言被优化掉。
        unsafe {
            for offset in 0..capacity {
                assert_eq!(
                    std::ptr::read_volatile(ptr.add(offset)),
                    0,
                    "byte {offset} of the allocation must be zeroed"
                );
            }
        }
        assert!(key.is_empty(), "zeroize clears the length");
        assert_eq!(
            format!("{key:?}"),
            "[REDACTED]",
            "zeroize must not break Debug redaction"
        );
    }

    #[cfg(feature = "confers")]
    #[test]
    fn validation_error_constructors_carry_messages() {
        let from_new = ValidationError::new(vec!["a".to_string(), "b".to_string()]);
        assert_eq!(from_new.errors, vec!["a", "b"]);

        let from_vec: ValidationError = vec!["c".to_string()].into();
        assert_eq!(from_vec.errors, vec!["c"]);
    }

    #[test]
    fn interpolate_json_value_applies_inline_default() {
        use serde_json::json;

        let mut value = json!({ "port": "${PORT:-8080}", "host": "${HOST}" });
        let mut vars: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        vars.insert("HOST".to_string(), "example.com".to_string());
        interpolate_json_value(&mut value, &vars);
        assert_eq!(value["port"], "8080", "unset var falls back to its default");
        assert_eq!(value["host"], "example.com");
    }

    #[test]
    fn interpolate_json_value_preserves_unclosed_placeholder() {
        use serde_json::json;

        let mut value = json!({ "endpoint": "host${port", "plain": "no-var" });
        interpolate_json_value(
            &mut value,
            &std::collections::HashMap::<String, String>::new(),
        );
        assert_eq!(
            value["endpoint"], "host${port",
            "unclosed placeholder untouched"
        );
        assert_eq!(value["plain"], "no-var");
    }
}
