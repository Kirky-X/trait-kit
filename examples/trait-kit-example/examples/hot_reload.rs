// Copyright (c) 2026 Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! Level 3: `hot-reload` feature — subscribe + reload_config.
//!
//! Registers a callback that fires when `reload_config::<C>()` re-loads the
//! value. Uses `Rc<Cell<u32>>` to count invocations across the `Kit<Unbuilt>`
//! → `Kit<Ready>` boundary (callbacks survive `build()`).
//! Run: `cargo run -p trait-kit-example --example hot_reload --features hot-reload`

use std::cell::Cell;
use std::error::Error;
use std::rc::Rc;
use trait_kit::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
struct AppConfig {
    value: u32,
}

impl Configurable for AppConfig {
    fn load() -> Result<Self, Box<dyn Error>> {
        Ok(Self { value: 42 })
    }
}

fn main() {
    let kit = Kit::new();
    let counter = Rc::new(Cell::new(0u32));
    let counter_clone = Rc::clone(&counter);
    kit.subscribe::<AppConfig>(move || {
        counter_clone.set(counter_clone.get() + 1);
    });

    // reload_config is available on Kit<Unbuilt> (matches test pattern).
    kit.reload_config::<AppConfig>()
        .expect("reload should succeed");
    assert!(counter.get() >= 1, "callback should fire on reload");

    let kit = kit.build().expect("build should succeed");
    let config: AppConfig = kit.config().expect("config should be retrievable");
    assert_eq!(
        config.value, 42,
        "reload should have stored the loaded value"
    );

    println!(
        "hot_reload: callbacks={}, value={}",
        counter.get(),
        config.value
    );
}
