// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! trait-kit — 模块标准接口与能力管理中心
//!
//! 提供模块定义标准接口和 Kit 能力管理中心的轻量实现。

#![deny(unsafe_code)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![doc = include_str!("../README.md")]

pub mod core;
mod error;
pub mod kit;

pub mod i18n;

pub mod prelude;

pub use error::TraitKitError;
pub use error::TraitKitResult;

#[cfg(feature = "async")]
pub use core::AsyncAutoBuilder;
#[cfg(feature = "async")]
pub use kit::{AsyncKit, AsyncReady, AsyncUnbuilt};

#[cfg(all(feature = "lifecycle", feature = "async"))]
pub use core::AsyncLifecycle;
#[cfg(feature = "lifecycle")]
pub use core::Lifecycle;

#[cfg(all(feature = "health", feature = "async"))]
pub use core::AsyncHealthCheck;
#[cfg(feature = "health")]
pub use core::{HealthCheck, HealthStatus};

#[cfg(feature = "observability")]
pub use core::BuildObserver;

#[cfg(all(feature = "scope", feature = "async"))]
pub use kit::AsyncScope;
#[cfg(feature = "scope")]
pub use kit::Scope;

#[cfg(all(feature = "shutdown", feature = "async"))]
pub use kit::AsyncShutdownCoordinator;
#[cfg(feature = "shutdown")]
pub use kit::{ShutdownCoordinator, ShutdownPhase, ShutdownPhaseResult, ShutdownResult};

/// Shared test helpers for async test modules (`block_on` executor + `MockError`).
///
/// Extracted to deduplicate between `core::meta::async_tests` and
/// `kit::async_kit::tests` (audit LOW-003). Gated on `async` feature because
/// both consumer test mods are `#[cfg(all(test, feature = "async"))]`.
#[cfg(all(test, feature = "async"))]
pub(crate) mod test_helpers {
    use std::future::Future;
    use std::task::{self, Poll};

    /// Minimal single-threaded `Future` executor for tests (no extra deps).
    ///
    /// Uses `Waker::noop()` (stable since 1.85) because the `async` feature
    /// deliberately stays dep-free (no `tokio` / `futures` test runtime).
    ///
    /// # Panics
    ///
    /// Panics if the future does not complete within `MAX_POLLS` iterations,
    /// preventing infinite loops from hanging the test suite.
    pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
        /// Maximum number of poll iterations before panicking.
        /// Generous enough for any reasonable test future.
        const MAX_POLLS: u32 = 1_000_000;

        let waker = task::Waker::noop();
        // `Context::from_waker` takes `&Waker`; the borrow is required by the
        // API signature (not a clippy false positive).
        #[allow(clippy::needless_borrow)]
        let mut cx = task::Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        let mut polls = 0u32;
        loop {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(v) => return v,
                Poll::Pending => {
                    polls += 1;
                    assert!(
                        polls < MAX_POLLS,
                        "block_on: future did not complete within \
                         {MAX_POLLS} poll iterations (possible infinite loop)"
                    );
                    std::hint::spin_loop();
                }
            }
        }
    }

    /// Mock error type for tests verifying `AsyncAutoBuilder` trait signatures.
    #[derive(Debug, thiserror::Error)]
    #[allow(dead_code, reason = "mock error type verifies trait signature only")]
    pub(crate) enum MockError {
        #[error("mock build failed: {0}")]
        Failed(String),
    }

    #[cfg(test)]
    mod block_on_tests {
        use super::*;
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::task::Poll;

        #[test]
        fn block_on_handles_pending() {
            static POLL_COUNT: AtomicU32 = AtomicU32::new(0);
            POLL_COUNT.store(0, Ordering::SeqCst);

            let result = block_on(std::future::poll_fn(|_cx| {
                let n = POLL_COUNT.fetch_add(1, Ordering::SeqCst);
                if n < 1 {
                    Poll::Pending
                } else {
                    Poll::Ready(42)
                }
            }));
            assert_eq!(result, 42);
            assert!(POLL_COUNT.load(Ordering::SeqCst) >= 2);
        }
    }
}
