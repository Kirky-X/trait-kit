// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Build observability: observer callbacks during module construction.

use std::time::Duration;

/// Observer for build pipeline events.
///
/// Implement this trait and register via `Kit::with_observer()` or
/// `AsyncKit::with_observer()` to receive callbacks during `build()`.
///
/// Requires the `observability` feature.
#[cfg(feature = "observability")]
pub trait BuildObserver: Send + Sync + 'static {
    /// Called before a module's build function is invoked.
    fn on_module_start(&self, _module_name: &'static str) {}

    /// Called after a module is successfully built.
    fn on_module_built(&self, _module_name: &'static str, _elapsed: Duration) {}

    /// Called when a module's build function returns an error.
    fn on_build_error(&self, _module_name: &'static str, _error: &crate::error::TraitKitError) {}
}

#[cfg(all(test, feature = "observability"))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct CountingObserver {
        start_count: Arc<AtomicUsize>,
        built_count: Arc<AtomicUsize>,
        error_count: Arc<AtomicUsize>,
    }

    impl BuildObserver for CountingObserver {
        fn on_module_start(&self, _module_name: &'static str) {
            self.start_count.fetch_add(1, Ordering::SeqCst);
        }

        fn on_module_built(&self, _module_name: &'static str, _elapsed: Duration) {
            self.built_count.fetch_add(1, Ordering::SeqCst);
        }

        fn on_build_error(&self, _module_name: &'static str, _error: &crate::error::TraitKitError) {
            self.error_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn observer_trait_is_object_safe() {
        let start = Arc::new(AtomicUsize::new(0));
        let built = Arc::new(AtomicUsize::new(0));
        let error = Arc::new(AtomicUsize::new(0));

        let observer: Box<dyn BuildObserver> = Box::new(CountingObserver {
            start_count: Arc::clone(&start),
            built_count: Arc::clone(&built),
            error_count: Arc::clone(&error),
        });

        observer.on_module_start("test");
        assert_eq!(start.load(Ordering::SeqCst), 1);

        observer.on_module_built("test", Duration::from_millis(10));
        assert_eq!(built.load(Ordering::SeqCst), 1);

        assert_eq!(error.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn observer_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CountingObserver>();
    }
}
