// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Decorator feature — post-build capability wrapping/enhancement.
//!
//! Demonstrates:
//! - `Kit::decorate::<M>(decorator)` — register a decorator that transforms
//!   a module's capability after build
//! - Multiple decorators can be registered; applied in registration order
//!
//! Run: `cargo run -p trait-kit-example --example decorator --features decorator`

use std::sync::Arc;
use trait_kit::prelude::*;

#[derive(Debug, Clone)]
struct MessageCap {
    content: String,
}

struct MessageModule;

impl ModuleMeta for MessageModule {
    const NAME: &'static str = "message";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        &[]
    }
}

impl AutoBuilder for MessageModule {
    type Capability = Arc<MessageCap>;
    type Error = TraitKitError;

    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(MessageCap {
            content: "hello".to_string(),
        }))
    }
}

fn main() {
    let mut kit = Kit::new();

    // Register a decorator that uppercases the message content
    kit.decorate::<MessageModule>(|cap| {
        println!(
            "  [decorator] transforming: '{}' -> '{}'",
            cap.content,
            cap.content.to_uppercase()
        );
        Arc::new(MessageCap {
            content: cap.content.to_uppercase(),
        })
    });

    kit.register::<MessageModule>()
        .expect("register MessageModule");
    let kit = kit.build().expect("build should succeed");

    let msg = kit
        .require::<MessageModule>()
        .expect("require MessageModule");
    println!("Final content: {}", msg.content);

    println!("decorator: OK");
}
