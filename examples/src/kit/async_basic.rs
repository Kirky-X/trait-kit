// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Async feature — AsyncKit<Unbuilt> → AsyncKit<Ready> typestate flow.
//!
//! Demonstrates `AsyncAutoBuilder` + `AsyncKit::register` / `build` /
//! `require` / `optional` / `contains` / `contains_config` / `set_config`.
//! Uses a minimal single-threaded `block_on` executor (no tokio required
//! because the futures resolve immediately).
//!
//! Run: `cargo run -p trait-kit-example --example async_basic --features async`

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{self, Poll};
use trait_kit::prelude::*;

// ─── Minimal block_on executor (mirrors trait-kit's internal test helper) ───

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = task::Waker::noop();
    let mut cx = task::Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::hint::spin_loop(),
        }
    }
}

// ─── Module definition ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct CacheCap {
    max_size: usize,
}

struct CacheModule;

impl ModuleMeta for CacheModule {
    const NAME: &'static str = "cache";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        &[]
    }
}

impl AsyncAutoBuilder for CacheModule {
    type Capability = Arc<CacheCap>;
    type Error = TraitKitError;

    fn build<'a>(
        kit: &'a AsyncKit,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>> {
        Box::pin(async move {
            let max_size: usize = kit.config().unwrap_or(100);
            Ok(Arc::new(CacheCap { max_size }))
        })
    }
}

fn main() {
    let mut kit = AsyncKit::new();
    kit.set_config(200usize);
    kit.register::<CacheModule>().expect("register CacheModule");

    let built = block_on(kit.build()).expect("build should succeed");

    let cache = built.require::<CacheModule>().expect("require CacheModule");
    assert_eq!(cache.max_size, 200);
    assert!(built.contains::<CacheModule>());
    assert!(built.optional::<CacheModule>().is_some());
    assert!(built.contains_config::<usize>());

    println!("async_basic: OK (cache max_size={})", cache.max_size);
}
