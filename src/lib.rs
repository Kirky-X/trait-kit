// Copyright (c) 2026 Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! trait-kit — 模块标准接口与能力管理中心
//!
//! 提供模块定义标准接口和 Kit 能力管理中心的轻量实现。

#![deny(unsafe_code)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![doc = include_str!("../README.md")]

pub mod core;
pub mod kit;

#[cfg(feature = "i18n")]
pub mod i18n;

pub mod prelude;

#[cfg(feature = "async")]
pub use core::meta::AsyncAutoBuilder;
#[cfg(feature = "async")]
pub use kit::{AsyncKit, AsyncReady, AsyncUnbuilt};

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
    pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
        let waker = task::Waker::noop();
        // `Context::from_waker` takes `&Waker`; clippy::needless_borrow is a
        // false positive here (removing the `&` would be a type error).
        #[allow(clippy::needless_borrow)]
        let mut cx = task::Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(v) => return v,
                Poll::Pending => std::hint::spin_loop(),
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
}
