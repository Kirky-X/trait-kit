// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Re-exports of the most commonly used types and traits.

pub use crate::core::{AutoBuilder, ModuleMeta};
pub use crate::error::TraitKitError;
pub use crate::i18n::{I18nError, I18nFormatter, I18nManager, tr};
pub use crate::kit::{Kit, Ready, Unbuilt};

#[cfg(feature = "async")]
pub use crate::core::AsyncAutoBuilder;
#[cfg(feature = "async")]
pub use crate::{AsyncKit, AsyncReady, AsyncUnbuilt};

#[cfg(feature = "confers")]
pub use crate::kit::Configurable;

#[cfg(feature = "confers-macros")]
pub use crate::kit::ModuleConfig;

#[cfg(all(feature = "lifecycle", feature = "async"))]
pub use crate::core::AsyncLifecycle;
#[cfg(feature = "lifecycle")]
pub use crate::core::Lifecycle;

#[cfg(all(feature = "health", feature = "async"))]
pub use crate::core::AsyncHealthCheck;
#[cfg(feature = "health")]
pub use crate::core::{HealthCheck, HealthStatus};

#[cfg(feature = "observability")]
pub use crate::core::BuildObserver;

#[cfg(all(feature = "scope", feature = "async"))]
pub use crate::kit::AsyncScope;
#[cfg(feature = "scope")]
pub use crate::kit::Scope;

#[cfg(all(feature = "shutdown", feature = "async"))]
pub use crate::kit::AsyncShutdownCoordinator;
#[cfg(feature = "shutdown")]
pub use crate::kit::{ShutdownCoordinator, ShutdownPhase, ShutdownPhaseResult, ShutdownResult};

#[cfg(all(test, feature = "async"))]
mod tests {
    //! Verify the async re-exports reachable through `prelude::*` compile
    //! against the expected concrete types (`async_kit::Ready` / `Unbuilt`), not
    //! the sync variants. This guards against a regression where lib.rs
    //! aliases the wrong `Ready`/`Unbuilt` markers.
    use crate::kit::async_kit::{Ready as AsyncReadyMarker, Unbuilt as AsyncUnbuiltMarker};
    use crate::prelude::*;

    #[test]
    fn prelude_async_kit_compiles() {
        let _ = AsyncKit::new();
    }

    #[test]
    fn prelude_async_markers_match_async_kit_markers() {
        fn assert_same_type<T: 'static, U: 'static>() {
            assert_eq!(
                std::any::TypeId::of::<T>(),
                std::any::TypeId::of::<U>(),
                "prelude marker diverged from async_kit marker"
            );
        }
        assert_same_type::<AsyncReady, AsyncReadyMarker>();
        assert_same_type::<AsyncUnbuilt, AsyncUnbuiltMarker>();
    }

    #[allow(dead_code, reason = "trait presence check only")]
    fn _async_auto_builder_is_in_prelude<M: AsyncAutoBuilder>() {}
}
