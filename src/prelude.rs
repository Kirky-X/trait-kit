// Copyright (c) 2026 Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! Re-exports of the most commonly used types and traits.

pub use crate::core::error::KitError;
pub use crate::core::meta::{AutoBuilder, ModuleMeta};
pub use crate::kit::{Kit, Ready, Unbuilt};

#[cfg(feature = "async")]
pub use crate::core::meta::AsyncAutoBuilder;
#[cfg(feature = "async")]
pub use crate::{AsyncKit, AsyncReady, AsyncUnbuilt};

#[cfg(feature = "confers")]
pub use crate::kit::config::Configurable;

#[cfg(feature = "confers-macros")]
pub use crate::kit::config::ModuleConfig;

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
        fn assert_same_type<T, U>()
        where
            T: std::any::Any + 'static,
            U: std::any::Any + 'static,
        {
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
