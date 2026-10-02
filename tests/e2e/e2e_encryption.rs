// Copyright (c) 2026 Kirky.X🌠
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

/// HKDF info 绑定 `version:PATH`——不同 PATH 隔离、同参确定、
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

/// 再导出原语 roundtrip——`encrypt` 产出 (nonce, ciphertext)，
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
        .encrypt(&plaintext, &*field_key)
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
        .decrypt(&nonce, &ciphertext, &*field_key)
        .expect("同密钥解密应成功");
    assert_eq!(*roundtrip, plaintext, "roundtrip 应还原明文");

    // 错误密钥（不同 PATH 派生）解密 → CryptoError（Poly1305 校验失败）。
    let other_key = derive_field_key(&master, "other/path", "v1").unwrap();
    assert!(
        cipher.decrypt(&nonce, &ciphertext, &*other_key).is_err(),
        "跨字段密钥解密必须失败（密钥隔离）"
    );
}

// ─── ConfersKeyProvider 桥接 confers SecretKeyProvider（e2e 层） ──────

/// confers 生态桥接：ConfersKeyProvider 包裹任意 confers
/// SecretKeyProvider → roundtrip 成功、provider_type 透传、get_key 失败
/// 时 fail-closed 为 BuildFailed{context="key provider (<type>)"}。
/// （src 内部单测 key_provider_tests 为同型锚点，本用例固化 e2e 面。）
#[cfg(feature = "confers")]
mod confers_key_provider_bridge_e2e {
    use trait_kit::kit::config::{ConfersKeyProvider, KeyProvider};
    use trait_kit::prelude::*;

    #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct BridgeSecret {
        token: String,
    }
    impl ModuleConfig for BridgeSecret {
        const PATH: &'static str = "e2e/key-provider-bridge";
        fn default_value() -> Self {
            Self {
                token: String::new(),
            }
        }
    }

    /// 正常 32 字节 confers provider mock。
    struct WorkingConfersProvider;
    impl confers::secret::SecretKeyProvider for WorkingConfersProvider {
        fn get_key(&self) -> Result<confers::SecretBytes, confers::CryptoError> {
            Ok(confers::SecretBytes::new(vec![7u8; 32]))
        }
        fn provider_type(&self) -> &'static str {
            "e2e-bridge"
        }
    }

    /// get_key 返回 Err 的 confers provider（fail-closed 路径）。
    struct FailingConfersProvider;
    impl confers::secret::SecretKeyProvider for FailingConfersProvider {
        fn get_key(&self) -> Result<confers::SecretBytes, confers::CryptoError> {
            Err(confers::CryptoError::InvalidKeyLength(4))
        }
        fn provider_type(&self) -> &'static str {
            "e2e-broken"
        }
    }

    /// 短钥 confers provider（<16 字节，get 侧下限校验）。
    struct ShortKeyConfersProvider;
    impl confers::secret::SecretKeyProvider for ShortKeyConfersProvider {
        fn get_key(&self) -> Result<confers::SecretBytes, confers::CryptoError> {
            Ok(confers::SecretBytes::new(vec![1u8; 8]))
        }
        fn provider_type(&self) -> &'static str {
            "e2e-short"
        }
    }

    #[test]
    fn e2e_confers_key_provider_roundtrip_and_fail_closed() {
        // 成功路径：经 provider set → get_encrypted(同钥) roundtrip。
        let kit = Kit::new();
        let provider = ConfersKeyProvider::new(WorkingConfersProvider);
        kit.set_encrypted_with_key_provider(
            &BridgeSecret {
                token: "bridge-secret".into(),
            },
            &provider,
        )
        .expect("经 confers provider 加密应成功");
        let ready = kit.build().expect("build 应成功");
        let plain: BridgeSecret = ready
            .get_encrypted(provider.master_key().expect("key").expose())
            .expect("同钥解密应成功");
        assert_eq!(plain.token, "bridge-secret");
        // provider_type 透传内层类型名。
        assert_eq!(provider.provider_type(), "e2e-bridge");

        // fail-closed：get_key 返回 Err → BuildFailed，context 指明
        // key provider 与内层类型名（不落空钥路径）。
        let broken = Kit::new();
        let err = broken
            .set_encrypted_with_key_provider(
                &BridgeSecret { token: "x".into() },
                &ConfersKeyProvider::new(FailingConfersProvider),
            )
            .expect_err("get_key 失败必须 fail-closed");
        match &err {
            TraitKitError::BuildFailed { context, .. } => {
                assert!(
                    context.contains("key provider") && context.contains("e2e-broken"),
                    "context 应含 key provider 与内层类型名：got '{context}'"
                );
            }
            other => panic!("expected BuildFailed, got: {other:?}"),
        }
        assert_eq!(
            err.kind(),
            trait_kit::ErrorKind::InitFailed,
            "fail-closed 应归类 InitFailed"
        );

        // fail-closed：短钥同样立即拒绝（不落空钥/弱钥路径）。
        let short = Kit::new();
        assert!(
            short
                .set_encrypted_with_key_provider(
                    &BridgeSecret { token: "x".into() },
                    &ConfersKeyProvider::new(ShortKeyConfersProvider),
                )
                .is_err(),
            "短钥必须立即拒绝"
        );
    }
}
