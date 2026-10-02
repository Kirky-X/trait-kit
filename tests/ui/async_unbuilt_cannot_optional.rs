// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT

// AsyncKit<Unbuilt> cannot call optional() — typestate enforces Ready-only.
// (`require` intentionally IS available on Unbuilt: async build callbacks
// pull dependencies through it; `optional` is a Ready-state read API.)
//
// The fixture must produce exactly one error (the E0599 below). Explicit
// trait impls to avoid macro-resolution cascade errors (see
// async_ready_cannot_register.rs).
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use trait_kit::prelude::*;

struct MyModule;
impl ModuleMeta for MyModule {
    const NAME: &'static str = "my-module";
}
impl AsyncAutoBuilder for MyModule {
    type Capability = Arc<u32>;
    type Error = TraitKitError;
    fn build<'a>(
        _kit: &'a AsyncKit,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>> {
        Box::pin(async { Ok(Arc::new(1)) })
    }
}

fn main() {
    let kit = AsyncKit::new();
    // ERROR: optional() is only available on AsyncKit<Ready>
    assert!(kit.optional::<MyModule>().is_none());
}
