// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//
// 加密存储 E2E 缺口固化测试。
//
// 覆盖场景 ID（docs/TEST_SCENARIOS.md §2.7）：
// - ENC-11 字段密钥 HKDF 绑定 `C::PATH` + 版本标签 `v1`：不同 PATH 派生
//   不同密钥（隔离性）、同参数派生稳定（确定性）
// - ENC-12 confers `XChaCha20Crypto` / `derive_field_key` 再导出可用：
//   经 trait-kit 公开路径直接调用加解密原语完成 roundtrip
//
// 其余 ENC 域场景既有覆盖充分，此处仅声明引用、不重复固化：
// - ENC-01..09 → tests/basic.rs、tests/e2e_feature_combinations.rs
//   （short key 拒收）、tests/e2e_advanced.rs（e15/e17/c14/c15）
// - ENC-10 → src/kit/config.rs 内联测试（EncryptedBlob getters/Debug 脱敏）
// - ENC-13（明文 load_config 与密文 set_encrypted 共存）→
//   tests/e2e_feature_combinations.rs::e2e_confers_plus_encryption

#![cfg(feature = "encryption")]

use trait_kit::kit::config::{XChaCha20Crypto, derive_field_key};

/// ENC-11：HKDF info 绑定 `version:PATH`——不同 PATH 隔离、同参确定、
/// 版本标签参与派生。
#[test]
fn e2e_field_key_derivation_path_isolation() {
    // 32 字节样例主密钥（测试夹具，非真实凭据）。
    // pragma: allowlist secret
    let master = *b"0123456789abcdef0123456789abcdef";

    let db = derive_field_key(&master, "app/db", "v1").unwrap();
    let cache = derive_field_key(&master, "app/cache", "v1").unwrap();
    let db_v2 = derive_field_key(&master, "app/db", "v2").unwrap();
    let db_again = derive_field_key(&master, "app/db", "v1").unwrap();

    // 隔离：不同 PATH → 不同密钥（XChaCha20 字段密钥不得跨类型复用）。
    assert_ne!(
        db, cache,
        "不同 C::PATH 必须派生不同字段密钥（HKDF info 隔离）"
    );
    // 版本标签参与派生：轮换版本 → 新密钥。
    assert_ne!(db, db_v2, "不同 key_version 应派生不同字段密钥");
    // 确定性：同参重放得到同一密钥（解密依赖此性质）。
    assert_eq!(db, db_again, "同参派生应稳定");
    assert_eq!(db.len(), 32, "字段密钥应为 32 字节 XChaCha20 密钥");
}

/// ENC-12：再导出原语 roundtrip——`encrypt` 产出 (nonce, ciphertext)，
/// `decrypt(nonce, ciphertext, key)` 还原明文；错误密钥解密失败。
#[test]
fn e2e_reexported_crypto_roundtrip_via_trait_kit_path() {
    // 32 字节样例主密钥（测试夹具，非真实凭据）。
    // pragma: allowlist secret
    let master = *b"0123456789abcdef0123456789abcdef";

    // 派生字段密钥（与 Kit::set_encrypted 内部同源路径）。
    let field_key = derive_field_key(&master, "e2e/path", "v1").unwrap();

    let cipher = XChaCha20Crypto::new();
    let plaintext = b"trait-kit e2e secret payload".to_vec();
    let (nonce, ciphertext) = cipher
        .encrypt(&plaintext, &field_key)
        .expect("合法 32 字节密钥加密应成功");

    assert_eq!(
        nonce.len(),
        24,
        "XNonce 应为 24 字节（XChaCha20 nonce 长度）"
    );
    assert_ne!(
        ciphertext.as_slice(),
        plaintext.as_slice(),
        "密文不得等于明文"
    );

    let roundtrip = cipher
        .decrypt(&nonce, &ciphertext, &field_key)
        .expect("同密钥解密应成功");
    assert_eq!(roundtrip, plaintext, "roundtrip 应还原明文");

    // 错误密钥（不同 PATH 派生）解密 → CryptoError（Poly1305 校验失败）。
    let other_key = derive_field_key(&master, "other/path", "v1").unwrap();
    assert!(
        cipher.decrypt(&nonce, &ciphertext, &other_key).is_err(),
        "跨字段密钥解密必须失败（密钥隔离）"
    );
}
