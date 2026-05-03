// Copyright © 2026 Kirky.X. All rights reserved.

//! TC-DOC-003: Configuration read/update with shared ConfigHandle.

use trait_kit::prelude::*;

#[derive(Debug, Clone, PartialEq)]
struct AppConfig {
    version: String,
    debug: bool,
}

struct AppConfigKey;

impl ConfigKey for AppConfigKey {
    type Config = AppConfig;
    const NAME: &'static str = "app_config";
}

fn main() {
    let kit = Kit::new();

    kit.set_config::<AppConfigKey>(AppConfig {
        version: "1.0.0".to_string(),
        debug: false,
    });

    let handle_1 = kit.config::<AppConfigKey>().unwrap();
    let handle_2 = kit.config::<AppConfigKey>().unwrap();

    println!("Initial config: {:?}", handle_1.load());

    handle_1.set(AppConfig {
        version: "2.0.0".to_string(),
        debug: true,
    });

    println!("After update via handle_1:");
    println!("  handle_1 sees: {:?}", handle_1.load());
    println!("  handle_2 sees: {:?}", handle_2.load());

    let old_snapshot = handle_2.load();
    handle_2.set(AppConfig {
        version: "3.0.0".to_string(),
        debug: false,
    });

    println!("Old snapshot still valid: {:?}", old_snapshot);
    println!("New config: {:?}", handle_1.load());
}
