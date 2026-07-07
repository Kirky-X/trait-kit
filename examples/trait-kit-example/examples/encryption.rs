// Copyright (c) 2026 Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! Level 4: `encryption` feature — set_encrypted + get_encrypted.
//!
//! Demonstrates the full three-tier inheritance:
//!
//! 1. `ModuleConfig::PATH` binds the config type to its module path.
//! 2. `dep:serde` + `dep:serde_json` are pulled in via the feature chain.
//! 3. The encryption key is HKDF-derived from `master_key` + `ModuleConfig::PATH`,
//!    so the same master key produces different field keys per module.
//!
//! Run: `cargo run -p trait-kit-example --example encryption --features encryption`

use trait_kit::kit::config::ModuleConfig;
use trait_kit::prelude::*;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct SecretConfig {
    api_key: String,
    port: u16,
}

impl ModuleConfig for SecretConfig {
    const PATH: &'static str = "config/secret.toml";
    fn default_value() -> Self {
        Self {
            api_key: "default_key".to_string(),
            port: 8080,
        }
    }
}

// 32-byte master key for XChaCha20-Poly1305 + HKDF.
// pragma: allowlist secret
const MASTER_KEY: [u8; 32] = *b"0123456789abcdef0123456789abcdef";

fn main() {
    let kit = Kit::new();
    let original = SecretConfig {
        api_key: "sk-12345".to_string(),
        port: 5432,
    };

    kit.set_encrypted(&original, &MASTER_KEY)
        .expect("set_encrypted should succeed");
    assert!(
        kit.contains_encrypted::<SecretConfig>(),
        "contains_encrypted should report true after set_encrypted"
    );

    let kit = kit.build().expect("build should succeed");

    let decrypted: SecretConfig = kit
        .get_encrypted(&MASTER_KEY)
        .expect("get_encrypted with correct key should succeed");
    assert_eq!(decrypted, original, "roundtrip should preserve value");
    println!(
        "Roundtrip OK: api_key={}, port={}",
        decrypted.api_key, decrypted.port
    );

    let wrong_key = *b"fedcba9876543210fedcba9876543210";
    let result: Result<SecretConfig, _> = kit.get_encrypted(&wrong_key);
    assert!(
        result.is_err(),
        "decryption with wrong key should fail (proves real encryption, not plaintext storage)"
    );
    println!("Wrong key correctly rejected");

    println!("encryption: OK");
}
