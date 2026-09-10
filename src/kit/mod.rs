// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Kit — the capability and configuration management center.

pub mod graph;
#[allow(clippy::module_inception)]
pub mod kit;
pub mod ports;
#[cfg(feature = "report")]
pub mod report;
pub(crate) mod typemap;

pub mod events;

#[cfg(feature = "async")]
pub mod async_kit;
#[cfg(feature = "async")]
pub mod async_typemap;

#[cfg(feature = "confers")]
pub mod config;

#[cfg(feature = "presets")]
pub mod presets;

#[cfg(feature = "compose")]
pub mod sub_kit;

pub use graph::{DependencyGraph, GraphError, ModuleEntry};
pub use kit::{Kit, Ready, Unbuilt};
pub(crate) use typemap::TypeMap;

#[cfg(feature = "scope")]
pub mod scope;
#[cfg(all(feature = "scope", feature = "async"))]
pub use scope::AsyncScope;
#[cfg(feature = "scope")]
pub use scope::Scope;

#[cfg(feature = "toggle")]
pub mod toggle;

#[cfg(feature = "shutdown")]
pub mod shutdown;
#[cfg(all(feature = "shutdown", feature = "async"))]
pub use shutdown::AsyncShutdownCoordinator;
#[cfg(feature = "shutdown")]
pub use shutdown::{ShutdownCoordinator, ShutdownPhase, ShutdownPhaseResult, ShutdownResult};

#[cfg(feature = "async")]
pub use async_kit::{AsyncKit, Ready as AsyncReady, Unbuilt as AsyncUnbuilt};
#[cfg(feature = "async")]
pub use async_typemap::AsyncTypeMap;

#[cfg(feature = "confers")]
pub use config::Config;
#[cfg(feature = "confers")]
pub use config::ConfigInherit;
#[cfg(feature = "confers")]
pub use config::Configurable;
#[cfg(feature = "confers")]
pub use config::ModuleConfig;
#[cfg(feature = "confers")]
pub use config::SharedConfig;
#[cfg(feature = "confers")]
pub use config::Validatable;
#[cfg(feature = "confers")]
pub use config::ValidationError;
#[cfg(feature = "confers")]
pub use config::interpolate_json_value;
#[cfg(feature = "confers")]
pub use config::merge_json_deep;

#[cfg(feature = "presets")]
pub use presets::{
    register_confers_config, ConfersConfigHandle, ConfersConfigModule, PresetError, Presets,
};

#[cfg(feature = "presets-remote")]
pub use presets::remote::{
    register_confers_remote_config, ConfersRemoteConfigModule, RemoteConfigProvider,
};

#[cfg(feature = "compose")]
pub use sub_kit::{SubKitHandle, SubKitModule, SubKitSpec};

// NOTE: derive macros (ConfigInherit, SharedConfig) live in `trait-kit-derive`.
// Users add `trait-kit-derive` as a dependency to use #[derive(ConfigInherit)]
// and #[derive(SharedConfig)]. The traits are re-exported above.

#[cfg(feature = "encryption")]
pub use config::{ConfersKeyProvider, EncryptedBlob, KeyBytes, KeyProvider};
#[cfg(feature = "encryption")]
pub(crate) use config::XChaCha20Crypto;

pub use ports::{
    LogPort, LogLevel, MetricsPort, NoOpLogPort, NoOpMetricsPort, OptionalLogPort,
    OptionalMetricsPort,
};

pub use events::{EventBus, KitEvent, MemoryEventBus, NoOpEventBus, OptionalEventBus};

#[cfg(feature = "toggle")]
pub use toggle::{MemoryToggle, ToggleBackend, ToggleBackendType, ToggleHandle, ToggleKey, ToggleValue};
#[cfg(all(feature = "toggle", feature = "confers"))]
pub use toggle::ConfersToggle;

#[cfg(feature = "report")]
pub use report::{
    BuildReport, ContractEntry, ContractManifest, ModuleBuildState, ModuleReportEntry,
    OverrideRecord,
};
