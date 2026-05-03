// Copyright © 2026 Kirky.X. All rights reserved.

//! Concurrency and shared semantics tests (TC-CONCURRENT-001~004).

mod common;

use std::sync::Arc;
use std::thread;

use trait_kit::prelude::*;

use common::*;

#[test]
fn tc_concurrent_001_kit_clone_shares_state() {
    let kit = Kit::new();
    let kit2 = kit.clone();
    let logger: Arc<dyn Logger + Send + Sync> = Arc::new(TestLogger {
        prefix: "[test] ".to_string(),
    });

    kit.provide::<MainLogger>(logger.clone()).unwrap();

    assert!(kit2.contains::<MainLogger>());
    assert!(Arc::ptr_eq(&kit2.require::<MainLogger>().unwrap(), &logger));
}

#[test]
fn tc_concurrent_002_multi_thread_read_capability() {
    let kit = Kit::new();
    let logger: Arc<dyn Logger + Send + Sync> = Arc::new(TestLogger {
        prefix: "[test] ".to_string(),
    });
    kit.provide::<MainLogger>(logger.clone()).unwrap();

    let kit_1 = kit.clone();
    let kit_2 = kit.clone();
    let logger_1 = logger.clone();
    let logger_2 = logger.clone();

    let h1 = thread::spawn(move || {
        let cap = kit_1.require::<MainLogger>().unwrap();
        Arc::ptr_eq(&cap, &logger_1)
    });
    let h2 = thread::spawn(move || {
        let cap = kit_2.require::<MainLogger>().unwrap();
        Arc::ptr_eq(&cap, &logger_2)
    });

    assert!(h1.join().unwrap());
    assert!(h2.join().unwrap());
}

#[test]
fn tc_concurrent_003_concurrent_provide_only_one_succeeds() {
    let kit = Kit::new();
    let kit_1 = kit.clone();
    let kit_2 = kit.clone();

    let logger_1: Arc<dyn Logger + Send + Sync> = Arc::new(TestLogger {
        prefix: "[one] ".to_string(),
    });
    let logger_2: Arc<dyn Logger + Send + Sync> = Arc::new(TestLogger {
        prefix: "[two] ".to_string(),
    });

    let l1 = logger_1.clone();
    let l2 = logger_2.clone();

    let h1 = thread::spawn(move || kit_1.provide::<MainLogger>(l1));
    let h2 = thread::spawn(move || kit_2.provide::<MainLogger>(l2));

    let r1 = h1.join().unwrap();
    let r2 = h2.join().unwrap();

    let successes = [&r1, &r2].iter().filter(|r| r.is_ok()).count();
    assert_eq!(successes, 1);

    let final_logger = kit.require::<MainLogger>().unwrap();
    assert!(Arc::ptr_eq(&final_logger, &logger_1) || Arc::ptr_eq(&final_logger, &logger_2));
}

#[test]
fn tc_concurrent_004_config_update_visible_across_threads() {
    let kit = Kit::new();
    kit.set_config::<LoggerConfigKey>(LoggerConfig {
        prefix: "[v1] ".to_string(),
    });

    let handle = kit.config::<LoggerConfigKey>().unwrap();
    let handle_1 = handle.clone();
    let handle_2 = handle.clone();

    let h = thread::spawn(move || {
        for _ in 0..100 {
            let v = handle_1.load();
            if v.prefix == "[v2] " {
                return true;
            }
            std::hint::spin_loop();
        }
        false
    });

    handle_2.set(LoggerConfig {
        prefix: "[v2] ".to_string(),
    });

    assert!(h.join().unwrap());
}
