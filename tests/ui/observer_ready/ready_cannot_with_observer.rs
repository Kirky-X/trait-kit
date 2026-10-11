// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT

// Kit<Ready> cannot call with_observer() — typestate enforces Unbuilt-only.
//
// Runs only when the `observer` feature is enabled (driver gate in
// tests/compile_fail.rs): `BuildObserver` does not exist otherwise and the
// missing-import error would violate the exactly-one-error rule.
//
// The fixture must produce exactly one error (the E0599 below). Explicit
// `ModuleMeta` impl instead of `impl_module_meta!`: the macro is not
// re-exported by the prelude, and a "cannot find macro" failure would
// cascade into E0277s whose "other types implement this trait" suggestions
// depend on which feature-gated impls exist in the current build.
use std::sync::Arc;
use trait_kit::core::BuildObserver;
use trait_kit::prelude::*;

struct MyModule;
impl ModuleMeta for MyModule {
    const NAME: &'static str = "my-module";
}
impl AutoBuilder for MyModule {
    type Capability = Arc<u32>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(1))
    }
}

struct MyObserver;
impl BuildObserver for MyObserver {}

fn main() {
    let mut kit = Kit::new();
    kit.register::<MyModule>().unwrap();
    let mut ready = kit.build().unwrap();
    let _ = &mut ready;
    // ERROR: with_observer() is only available on Kit<Unbuilt>
    ready.with_observer(Arc::new(MyObserver));
}
