// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT

// AsyncKit<Ready> cannot call register() — typestate enforces Unbuilt-only.
//
// The fixture must produce exactly one error (the E0599 below). Explicit
// `ModuleMeta`/`AsyncAutoBuilder` impls instead of macros: the macros are
// not re-exported by the prelude, and a "cannot find macro" failure would
// cascade into E0277s whose suggestions depend on the current build. The
// Ready value comes from a stub (never executed) so the only diagnostic is
// the typestate E0599 itself.
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

fn ready_stub() -> AsyncKit<AsyncReady> {
    unreachable!("fixture never runs; only type-checked")
}

fn main() {
    let ready = ready_stub();
    // ERROR: register() is only available on AsyncKit<Unbuilt>
    ready.register::<MyModule>().unwrap();
}
