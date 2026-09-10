// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Preset module packages (T204): ready-made modules for common integration
//! needs.
//!
//! The headline preset is [`ConfersConfigModule`], which wraps a
//! `confers::ConfigProvider` as a first-class Kit module — the configuration
//! hub becomes an ordinary capability other modules can `require`, instead of
//! living outside the module system. A [`Presets`] builder composes the
//! presets in one call.
//!
//! Requires the `presets` feature (implies `confers`).

use std::sync::Arc;

use confers::ConfigProvider;

use crate::core::{AutoBuilder, ModuleMeta};
use crate::error::TraitKitError;
use crate::kit::Kit;

// ─── Provider slot ──────────────────────────────────────────────────────────

/// Internal config-slot type carrying the provider into
/// [`ConfersConfigModule::build`].
///
/// Stored in the Kit's `TypeMap` under its own `TypeId`, so it cannot collide
/// with any user configuration type.
struct ConfersProviderSlot(Arc<dyn ConfigProvider>);

impl Clone for ConfersProviderSlot {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

// ─── ConfersConfigHandle ────────────────────────────────────────────────────

/// Cloneable handle to the confers configuration capability.
///
/// Obtained via `kit.require::<ConfersConfigModule>()`. All reads delegate to
/// the wrapped `ConfigProvider` — the handle adds typed accessors so module
/// builders can consume configuration without depending on confers types.
#[derive(Clone)]
pub struct ConfersConfigHandle {
    provider: Arc<dyn ConfigProvider>,
}

impl ConfersConfigHandle {
    /// Raw annotated-value access (full confers fidelity).
    #[must_use]
    pub fn get_raw(&self, key: &str) -> Option<&confers::AnnotatedValue> {
        self.provider.get_raw(key)
    }

    /// String value for `key`, or `None` when absent / not a string.
    #[must_use]
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.provider.get_raw(key)?.inner.as_str().map(str::to_owned)
    }

    /// Integer value for `key`, or `None` when absent / not an integer.
    #[must_use]
    pub fn get_int(&self, key: &str) -> Option<i64> {
        self.provider.get_raw(key)?.inner.as_i64()
    }

    /// Boolean value for `key`, or `None` when absent / not a boolean.
    #[must_use]
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.provider.get_raw(key)?.inner.as_bool()
    }

    /// All non-sensitive configuration keys.
    #[must_use]
    pub fn keys(&self) -> Vec<String> {
        self.provider.keys()
    }

    /// Whether `key` exists in the provider.
    #[must_use]
    pub fn contains(&self, key: &str) -> bool {
        self.provider.has(key)
    }
}

// ─── ConfersConfigModule ────────────────────────────────────────────────────

/// Preset module exposing a confers [`ConfigProvider`] as a Kit capability.
///
/// The module itself is a zero-sized key: the provider instance is injected
/// once via [`register_confers_config`] (or [`Presets`]), stored in a private
/// config slot, and surfaced as the [`ConfersConfigHandle`] capability.
///
/// # Example
///
/// ```ignore
/// let mut kit = Kit::new();
/// trait_kit::kit::presets::register_confers_config(&mut kit, provider)?;
/// let kit = kit.build()?;
/// let cfg = kit.require::<ConfersConfigModule>()?;
/// let host = cfg.get_string("db.host");
/// ```
pub struct ConfersConfigModule;

impl ModuleMeta for ConfersConfigModule {
    const NAME: &'static str = "confers-config";
}

impl AutoBuilder for ConfersConfigModule {
    type Capability = ConfersConfigHandle;
    type Error = PresetError;

    fn build(kit: &Kit) -> Result<Self::Capability, Self::Error> {
        let slot = kit
            .config::<ConfersProviderSlot>()
            .map_err(|_| PresetError::ProviderNotInjected)?;
        Ok(ConfersConfigHandle {
            provider: Arc::clone(&slot.0),
        })
    }
}

/// Error type for preset module build failures.
#[derive(Debug)]
pub enum PresetError {
    /// `ConfersConfigModule` was registered without injecting a provider
    /// first — use [`register_confers_config`] instead of plain
    /// `kit.register::<ConfersConfigModule>()`.
    ProviderNotInjected,
    /// A remote source failed to load (T219).
    #[cfg(feature = "presets-remote")]
    RemoteLoad {
        /// Error message from the source.
        message: String,
    },
}

impl std::fmt::Display for PresetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProviderNotInjected => write!(
                f,
                "ConfersConfigModule built without a provider; call \
                 register_confers_config(kit, provider) first"
            ),
            #[cfg(feature = "presets-remote")]
            Self::RemoteLoad { message } => write!(f, "remote config source failed: {message}"),
        }
    }
}

impl std::error::Error for PresetError {}

// ─── Registration API ───────────────────────────────────────────────────────

/// Inject a confers provider and register [`ConfersConfigModule`] in one step.
///
/// # Errors
///
/// Returns `TraitKitError::AlreadyRegistered` if the module was already
/// registered in this Kit.
pub fn register_confers_config(
    kit: &mut Kit,
    provider: Arc<dyn ConfigProvider>,
) -> Result<(), TraitKitError> {
    kit.set_config(ConfersProviderSlot(provider));
    kit.register::<ConfersConfigModule>()
}

// ─── Presets builder ────────────────────────────────────────────────────────

/// Builder composing the common preset modules into a `Kit` in one call.
///
/// # Example
///
/// ```ignore
/// Presets::new()
///     .confers_config(provider)
///     .apply_to(&mut kit)?;
/// ```
#[derive(Default)]
pub struct Presets {
    provider: Option<Arc<dyn ConfigProvider>>,
}

impl Presets {
    /// Create an empty preset set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add the confers configuration hub preset.
    #[must_use]
    pub fn confers_config(mut self, provider: Arc<dyn ConfigProvider>) -> Self {
        self.provider = Some(provider);
        self
    }

    /// Apply every configured preset to `kit`.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::AlreadyRegistered` if a preset module was
    /// already registered in the Kit.
    pub fn apply_to(self, kit: &mut Kit) -> Result<(), TraitKitError> {
        if let Some(provider) = self.provider {
            register_confers_config(kit, provider)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use confers::{AnnotatedValue, ConfigValue, SourceId};
    use std::collections::HashMap;

    /// Minimal in-memory `ConfigProvider` mock: a plain map from dot-notation
    /// keys to owned annotated values (same shape as confers' own test
    /// provider). Send + Sync because the map owns its values.
    struct MapProvider(HashMap<String, AnnotatedValue>);

    impl MapProvider {
        fn from_pairs<const N: usize>(pairs: [(&str, ConfigValue); N]) -> Arc<Self> {
            let map = pairs
                .into_iter()
                .map(|(k, v)| (k.to_string(), AnnotatedValue::new(v, SourceId::default(), k)))
                .collect();
            Arc::new(Self(map))
        }
    }

    impl ConfigProvider for MapProvider {
        fn get_raw(&self, key: &str) -> Option<&AnnotatedValue> {
            self.0.get(key)
        }

        fn keys(&self) -> Vec<String> {
            self.0.keys().cloned().collect()
        }
    }

    #[test]
    fn confers_config_module_exposes_provider_as_kit_capability() {
        let provider = MapProvider::from_pairs([
            ("db.host", ConfigValue::String("localhost".into())),
            ("db.port", ConfigValue::I64(5432)),
            ("db.tls", ConfigValue::Bool(true)),
        ]);

        let mut kit = Kit::new();
        register_confers_config(&mut kit, provider).expect("register preset");
        let ready = kit.build().expect("build ok");

        let cfg = ready
            .require::<ConfersConfigModule>()
            .expect("config capability available through the Kit");
        assert_eq!(cfg.get_string("db.host").as_deref(), Some("localhost"));
        assert_eq!(cfg.get_int("db.port"), Some(5432));
        assert_eq!(cfg.get_bool("db.tls"), Some(true));
        assert_eq!(cfg.get_string("missing"), None);
        assert!(cfg.contains("db.host"));
        assert!(!cfg.contains("db.absent"));
        let mut keys = cfg.keys();
        keys.sort();
        assert_eq!(keys, vec!["db.host", "db.port", "db.tls"]);
    }

    #[test]
    fn confers_config_module_without_provider_fails_with_clear_error() {
        let mut kit = Kit::new();
        kit.register::<ConfersConfigModule>().expect("register");
        let err = kit.build().expect_err("build must fail");
        let msg = err.to_string();
        assert!(
            msg.contains("register_confers_config"),
            "error should guide users to the injection API: {msg}"
        );
    }

    #[test]
    fn handle_is_cloneable_and_shares_provider() {
        let provider = MapProvider::from_pairs([("a", ConfigValue::I64(1))]);
        let mut kit = Kit::new();
        register_confers_config(&mut kit, provider).expect("register");
        let ready = kit.build().expect("build ok");

        let h1 = ready.require::<ConfersConfigModule>().expect("require");
        let h2 = h1.clone();
        assert!(h1.contains("a") && h2.contains("a"), "clone shares provider");
    }

    #[test]
    fn presets_builder_applies_confers_config_preset() {
        let provider = MapProvider::from_pairs([("k", ConfigValue::String("v".into()))]);
        let mut kit = Kit::new();
        Presets::new()
            .confers_config(provider)
            .apply_to(&mut kit)
            .expect("apply presets");
        let ready = kit.build().expect("build ok");
        let cfg = ready.require::<ConfersConfigModule>().expect("require");
        assert_eq!(cfg.get_string("k").as_deref(), Some("v"));
    }
}

// ─── Remote source bridge (T219, `presets-remote` feature) ─────────────────

/// Bridge confers remote/`AsyncSource` configuration into an `AsyncKit` (T219).
///
/// The remote module loads the source once during `AsyncKit::build()`, wraps
/// the snapshot in a dot-path [`RemoteConfigProvider`], and publishes a
/// `ConfigChanged` audit event on the kit's event bus. Requires the
/// `presets-remote` feature (implies `presets` and confers `remote`).
#[cfg(feature = "presets-remote")]
pub mod remote {
    use super::{ConfersConfigHandle, PresetError};
    use confers::{AnnotatedValue, ConfigProvider, ConfigValue};
    use crate::core::{AsyncAutoBuilder, ModuleMeta};
    use crate::error::TraitKitError;
    use crate::kit::AsyncKit;
    use std::sync::Arc;

    /// Config slot carrying the remote source into the module build.
    struct RemoteSourceSlot(Arc<dyn confers::interface::AsyncSource>);

    impl Clone for RemoteSourceSlot {
        fn clone(&self) -> Self {
            Self(Arc::clone(&self.0))
        }
    }

    /// A [`ConfigProvider`] view over one loaded remote snapshot.
    ///
    /// Resolves dot-notation paths (`"db.host"`) by walking the snapshot's
    /// map structure; the root value itself is addressable via its own path.
    pub struct RemoteConfigProvider {
        root: Arc<AnnotatedValue>,
    }

    impl RemoteConfigProvider {
        /// Wrap a loaded snapshot.
        #[must_use]
        pub fn new(root: AnnotatedValue) -> Self {
            Self {
                root: Arc::new(root),
            }
        }

        fn resolve<'a>(&'a self, key: &str) -> Option<&'a AnnotatedValue> {
            if key == self.root.path.as_ref() {
                return Some(&self.root);
            }
            // Walk the map structure; every reference derives from
            // `&'a self.root`, so the final borrow is lifetime-safe without
            // any unsafe code.
            let mut node: &'a AnnotatedValue = &self.root;
            for segment in key.split('.') {
                let ConfigValue::Map(map) = &node.inner else {
                    return None;
                };
                node = map.get(segment)?;
            }
            Some(node)
        }

        fn collect_keys(node: &AnnotatedValue, prefix: &str, out: &mut Vec<String>) {
            if let ConfigValue::Map(map) = &node.inner {
                for (k, v) in map.iter() {
                    let path = if prefix.is_empty() {
                        k.to_string()
                    } else {
                        format!("{prefix}.{k}")
                    };
                    if let ConfigValue::Map(_) = &v.inner {
                        Self::collect_keys(v, &path, out);
                    } else {
                        out.push(path);
                    }
                }
            }
        }
    }

    impl ConfigProvider for RemoteConfigProvider {
        fn get_raw(&self, key: &str) -> Option<&AnnotatedValue> {
            self.resolve(key)
        }

        fn keys(&self) -> Vec<String> {
            let mut out = Vec::new();
            Self::collect_keys(&self.root, "", &mut out);
            out
        }
    }

    /// AsyncKit module exposing a remote configuration snapshot (T219).
    ///
    /// Capability: [`ConfersConfigHandle`](super::ConfersConfigHandle) — the
    /// same handle type as the local preset, so downstream modules are source
    /// agnostic.
    pub struct ConfersRemoteConfigModule;

    impl ModuleMeta for ConfersRemoteConfigModule {
        const NAME: &'static str = "confers-config-remote";
    }

    impl AsyncAutoBuilder for ConfersRemoteConfigModule {
        type Capability = ConfersConfigHandle;
        type Error = PresetError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
            Box::pin(async move {
                let slot = kit
                    .config::<RemoteSourceSlot>()
                    .map_err(|_| PresetError::ProviderNotInjected)?;
                let source_name = slot.0.name().to_string();
                let snapshot = slot.0.load().await.map_err(|e| PresetError::RemoteLoad {
                    message: e.to_string(),
                })?;
                let provider = Arc::new(RemoteConfigProvider::new(snapshot));
                // Audit: surface the remote load on the kit event bus (T208).
                kit.emit_event(crate::kit::events::KitEvent::ConfigChanged {
                    key: "confers-config-remote".to_string(),
                    summary: format!("remote load from {source_name}"),
                });
                Ok(ConfersConfigHandle { provider })
            })
        }
    }

    use std::pin::Pin;

    /// Inject a remote source and register [`ConfersRemoteConfigModule`] (T219).
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::AlreadyRegistered` if the module was already
    /// registered in this `AsyncKit`.
    pub fn register_confers_remote_config(
        kit: &mut AsyncKit,
        source: Arc<dyn confers::interface::AsyncSource>,
    ) -> Result<(), TraitKitError> {
        kit.set_config(RemoteSourceSlot(source));
        kit.register::<ConfersRemoteConfigModule>()
    }

    #[cfg(all(test, feature = "async"))]
    mod tests {
        use super::*;
        use crate::kit::events::{KitEvent, MemoryEventBus};
        use crate::test_helpers::block_on;
        use async_trait::async_trait;
        use std::sync::Mutex;

        struct MockRemoteSource {
            json: serde_json::Value,
            id: confers::SourceId,
        }

        #[async_trait]
        impl confers::interface::AsyncSource for MockRemoteSource {
            async fn load(&self) -> confers::ConfigResult<AnnotatedValue> {
                Ok(AnnotatedValue::new(
                    ConfigValue::from_json_value(self.json.clone()),
                    self.id.clone(),
                    "remote-root",
                ))
            }
            fn source_id(&self) -> &confers::SourceId {
                &self.id
            }
            fn name(&self) -> &str {
                "mock-remote"
            }
        }

        #[test]
        fn remote_module_loads_snapshot_and_publishes_audit() {
            let source = Arc::new(MockRemoteSource {
                json: serde_json::json!({"db": {"host": "remote-db", "port": 5432}}),
                id: confers::SourceId::default(),
            });

            let bus = Arc::new(MemoryEventBus::new());
            let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
            let sink = Arc::clone(&seen);
            bus.subscribe(move |event| {
                if let KitEvent::ConfigChanged { summary, .. } = event {
                    sink.lock().unwrap().push(summary.clone());
                }
            });

            let mut kit = AsyncKit::new();
            kit.with_event_bus(Some(Arc::clone(&bus) as Arc<dyn crate::kit::events::EventBus>));
            register_confers_remote_config(&mut kit, source).expect("register remote preset");
            let ready = block_on(kit.build()).expect("build ok");

            let cfg = ready
                .require::<ConfersRemoteConfigModule>()
                .expect("remote capability available");
            assert_eq!(cfg.get_string("db.host").as_deref(), Some("remote-db"));
            assert_eq!(cfg.get_int("db.port"), Some(5432));
            assert!(
                cfg.contains("db.host") && !cfg.contains("db.absent"),
                "dot-path resolution works"
            );
            let audits = seen.lock().unwrap();
            assert!(
                audits.iter().any(|s| s.contains("mock-remote")),
                "remote load published an audit event: {audits:?}"
            );
        }

        #[test]
        fn remote_source_failure_fails_build() {
            struct FailingSource;
            #[async_trait]
            impl confers::interface::AsyncSource for FailingSource {
                async fn load(&self) -> confers::ConfigResult<AnnotatedValue> {
                    Err(confers::ConfigError::FileNotFound {
                        filename: std::path::PathBuf::from("mock-remote-source"),
                        source: None,
                    })
                }
                fn source_id(&self) -> &confers::SourceId {
                    static ID: std::sync::OnceLock<confers::SourceId> = std::sync::OnceLock::new();
                    ID.get_or_init(confers::SourceId::default)
                }
                fn name(&self) -> &str {
                    "failing"
                }
            }

            let mut kit = AsyncKit::new();
            register_confers_remote_config(&mut kit, Arc::new(FailingSource))
                .expect("register");
            let err = block_on(kit.build()).expect_err("remote failure fails build");
            assert!(
                err.to_string().contains("mock-remote-source"),
                "source error surfaces: {err}"
            );
        }
    }
}
