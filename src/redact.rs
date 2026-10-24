// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 对外文本脱敏：抹掉凭据片段、绝对路径与内部类型路径。
//!
//! 健康/就绪载荷与错误文本都可能流向未鉴权的调用方，凭据、连接串与
//! 服务器绝对路径不应原样出现在其中。原始文本仍由调用方自己持有
//! （如 `HealthStatus::detail`、错误的 `Error::source()`）。

/// 类型名的对外短标签（拥有所有权）：只保留末段类型名。
///
/// `std::any::type_name` 会带出完整路径（如 `my_crate::config::DbConfig`），
/// 这些内部结构会随错误文本与事件负载流向调用方。对外只保留类型名本身。
pub(crate) fn short_type_name<T: ?Sized + 'static>() -> String {
    short_type_name_ref::<T>().to_string()
}

/// 类型名的对外短标签（`'static`）：只保留末段类型名。
pub(crate) fn short_type_name_ref<T: ?Sized + 'static>() -> &'static str {
    short_type_path(std::any::type_name::<T>())
}

/// 从完整类型路径中取出类型名（裁掉模块路径与泛型实参）。
///
/// 实现先裁掉顶层泛型实参，再取路径末段：裁剪顺序决定了取到的是拥有
/// 泛型参数的类型名本身，容器名因此得以保留（`core::option::Option<
/// my_crate::Db>` → `Option`，而非实参里的 `Db`）。对外标签必须指向调用
/// 方注册的 Config 类型，取实参里的值类型会把排查方向带偏。
pub(crate) fn short_type_path(full: &str) -> &str {
    let base = match full.find('<') {
        Some(open) => &full[..open],
        None => full,
    };
    let last = base.rsplit("::").next().unwrap_or(base);
    last.strip_suffix('>').unwrap_or(last)
}

/// 敏感键名（大小写不敏感）：文本里出现这些键时，其后的值被替换为
/// `[REDACTED]`。
const SENSITIVE_KEYS: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "secret",
    "client_secret",
    "token",
    "api_key",
    "apikey",
    "access_key",
    "secret_key",
    "credential",
    "credentials",
    "authorization",
    "passphrase",
    "private_key",
    "signing_key",
    "encryption_key",
];

/// 键名是否敏感（大小写不敏感）。
///
/// 凭据类键名（`password`、`api_key` 等）不应出现在对外键名列表与共享
/// 命名空间里：那些位置会把键名交给未鉴权的调用方。判定按分隔符切段：
/// 整段等于敏感名，或以 `_` / `.` / `-` 分隔的后缀段命中敏感名
/// （`db_password` 命中，`tokenizer` 不命中）。
#[cfg(feature = "confers")]
pub(crate) fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SENSITIVE_KEYS.iter().any(|sensitive| {
        lower == *sensitive
            || lower
                .strip_suffix(sensitive)
                .is_some_and(|rest| rest.ends_with(['_', '.', '-']))
    })
}

/// 认证方案词：方案词本身保留，其后的凭据片段被抹。
const AUTH_SCHEMES: &[&str] = &[
    "bearer",
    "basic",
    "digest",
    "ntlm",
    "negotiate",
    "kerberos",
    "oauth",
    "hmac",
];

/// 片段是否为认证方案词（大小写不敏感）。
fn is_auth_scheme(token: &str) -> bool {
    AUTH_SCHEMES
        .iter()
        .any(|scheme| token.eq_ignore_ascii_case(scheme))
}

/// 片段开头连续的 `=` / `:` 分隔符长度（字节数；分隔符是 ASCII，切分点
/// 仍落在字符边界上）。
fn leading_separator_len(token: &str) -> usize {
    token
        .as_bytes()
        .iter()
        .take_while(|byte| **byte == b'=' || **byte == b':')
        .count()
}

/// 片段是否开启 PEM 块：`-----BEGIN` 出现且块未在片段内闭合。
///
/// PEM 块按空白切分后横跨多个片段（`-----BEGIN` / `PRIVATE` / `KEY-----`
/// / 密钥材料 / `-----END`），块内的私钥材料必须逐段抹除。
fn opens_pem_block(token: &str) -> bool {
    token.contains("-----BEGIN") && !token.contains("-----END")
}

/// 片段之后紧跟的内容形态。
enum Next {
    /// 无特殊含义，后续片段按普通片段处理。
    None,
    /// 下一个片段就是值，须被抹。
    Value,
    /// 当前片段以敏感键名结尾：下一个片段可能是分隔符
    /// （`password` + `=` + `<value>`），值再往后一个片段。
    KeyOnly,
}

/// 对外文本脱敏：凭据片段必抹；`mask_paths` 为真时绝对路径也抹。
///
/// 文本按空白切分后逐片段处理，再以单个空格拼回：片段级判定需要这个
/// 规整化的边界（`token: <value>` 这类带空格的写法）。PEM 块按空白切分
/// 后横跨多个片段，块内材料自 `-----BEGIN` 起逐段抹除至 `-----END`。
pub(crate) fn redact_text(text: &str, mask_paths: bool) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut mask_next = false;
    let mut key_pending = false;
    let mut pem_block = false;
    for token in text.split_whitespace() {
        if pem_block {
            // PEM 块内的片段属于同一块凭据材料，逐段抹除直到 `-----END`。
            parts.push("[REDACTED]".to_string());
            if token.contains("-----END") {
                pem_block = false;
            }
            continue;
        }
        if mask_next {
            // `Authorization: Bearer <token>`：方案词本身保留，
            // 凭据在下一个片段里。
            // `token` 只在此处（已确定后面跟的是凭据）算方案词：它独立
            // 出现时是常见名词（`invalid token detected`），按方案词处理会
            // 误伤散文。
            if is_auth_scheme(token) || token.eq_ignore_ascii_case("token") {
                parts.push(token.to_string());
            } else {
                parts.push("[REDACTED]".to_string());
                mask_next = false;
                if opens_pem_block(token) {
                    pem_block = true;
                }
            }
            continue;
        }
        if key_pending {
            key_pending = false;
            let separator_len = leading_separator_len(token);
            if separator_len > 0 && separator_len == token.len() {
                // `password = <value>`：分隔符原样保留，值在下一个片段。
                parts.push(token.to_string());
                mask_next = true;
                continue;
            }
            if separator_len > 0 {
                // `password =<value>`：分隔符与值同处一个片段。
                parts.push(format!("{}[REDACTED]", &token[..separator_len]));
                if opens_pem_block(token) {
                    pem_block = true;
                }
                continue;
            }
            // 不是分隔符：不吞片段，避免散文误伤。
        }
        if is_auth_scheme(token) {
            parts.push(token.to_string());
            mask_next = true;
            continue;
        }
        let (rendered, next) = redact_token(token, mask_paths);
        let masked = rendered.contains("[REDACTED]");
        if !masked && token.contains("-----BEGIN") {
            // PEM 块不依赖敏感键名前缀：没有 `private_key=` 这样的前缀时，
            // 块头片段本身不会被键名规则命中，但块内材料同样是凭据。
            parts.push("[REDACTED]".to_string());
            pem_block = !token.contains("-----END");
        } else {
            parts.push(rendered);
            if masked && opens_pem_block(token) {
                pem_block = true;
            }
        }
        match next {
            Next::None => {}
            Next::Value => mask_next = true,
            Next::KeyOnly => key_pending = true,
        }
    }
    parts.join(" ")
}

/// 截断到 `max_len` 个字符，超出部分以 `[truncated]` 标记。
pub(crate) fn truncate(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_string();
    }
    format!(
        "{}[truncated]",
        text.chars().take(max_len).collect::<String>()
    )
}

/// 抹掉单个片段里的凭据（以及可选的绝对路径）。
///
/// 返回值第二项说明该片段之后的内容形态：[`Next::Value`] 表示值落在下一个
/// 片段里（`token: <value>` 这种带空格的写法），[`Next::KeyOnly`] 表示该
/// 片段以敏感键名结尾（`password` + `=` + `<value>`）。
fn redact_token(token: &str, mask_paths: bool) -> (String, Next) {
    // URL userinfo：`scheme://<userinfo>@host` → `scheme://[REDACTED]@host`
    if let Some(scheme_end) = token.find("://") {
        let rest = &token[scheme_end + 3..];
        if let Some(at) = rest.find('@') {
            let userinfo = &rest[..at];
            if !userinfo.is_empty() && !userinfo.contains('/') {
                return (
                    format!("{}://[REDACTED]@{}", &token[..scheme_end], &rest[at + 1..]),
                    Next::None,
                );
            }
        }
    }

    // `password=…` / `token: …` 形式的键值对
    let lower = token.to_ascii_lowercase();
    for key in SENSITIVE_KEYS {
        let Some(pos) = lower.find(key) else {
            continue;
        };
        let tail = &token[pos + key.len()..];
        let trimmed = tail.trim_start_matches([' ', '\t']);
        let leading = tail.len() - trimmed.len();
        let Some(after_sep) = trimmed.strip_prefix(|c| c == '=' || c == ':') else {
            // 键名前已有分隔符 → 该键名落在值部分（如 `api_key=mysecret` 里的
            // `secret`），不能就此认定片段以键名结尾。
            let in_value = token[..pos].contains(['=', ':']);
            if trimmed.is_empty() && !in_value {
                // 片段以键名结尾：值可能落在后一个片段（`password = <value>`）。
                if mask_paths && let Some(masked) = mask_absolute_path(token) {
                    return (masked, Next::None);
                }
                return (token.to_string(), Next::KeyOnly);
            }
            // 键名落在值部分：继续扫其余键名，让真正的键名（`api_key` 等）
            // 被命中并脱敏。
            continue;
        };
        let value = after_sep.trim_start_matches([' ', '\t']);
        if value.is_empty() {
            // `token:` 结尾：值在下一个片段里。
            return (token.to_string(), Next::Value);
        }
        let sep_pos = pos + key.len() + leading;
        let value_pos = sep_pos + 1 + (after_sep.len() - value.len());
        return (
            format!(
                "{}[REDACTED]{}",
                &token[..value_pos],
                &token[value_pos + value.len()..]
            ),
            Next::None,
        );
    }

    if mask_paths && let Some(masked) = mask_absolute_path(token) {
        return (masked, Next::None);
    }
    (token.to_string(), Next::None)
}

/// 绝对路径（Unix `/etc/app/x.toml`、Windows `C:\app\x.toml`）只保留最后
/// 一段文件名，其余目录结构替换为 `[REDACTED]`。
fn mask_absolute_path(token: &str) -> Option<String> {
    let bytes = token.as_bytes();
    let is_windows = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/');
    let is_unix = bytes.len() >= 2 && bytes[0] == b'/' && bytes[1] != b'/';
    if !is_windows && !is_unix {
        return None;
    }
    let separator = |ch: char| ch == '/' || ch == '\\';
    let file_name = token.rsplit(separator).next().unwrap_or(token);
    if file_name.is_empty() || file_name == token {
        // 没有可保留的文件名（路径以分隔符结尾或只有一个片段）。
        return Some("[REDACTED]".to_string());
    }
    let keep_from = token.len() - file_name.len();
    Some(format!("[REDACTED]{}", &token[keep_from..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_type_path_drops_module_path() {
        assert_eq!(short_type_path("my_crate::config::DbConfig"), "DbConfig");
        assert_eq!(short_type_path("DbConfig"), "DbConfig");
    }

    /// 泛型实参被裁掉，留下拥有泛型参数的类型名本身（容器名不丢）。
    #[test]
    fn short_type_path_drops_generic_arguments() {
        assert_eq!(
            short_type_path("core::option::Option<my_crate::Db>"),
            "Option"
        );
        assert_eq!(
            short_type_path("std::collections::hash::map::HashMap<String, my_crate::Db>"),
            "HashMap"
        );
        assert_eq!(short_type_path("my_crate::Generic<u8, u16>"), "Generic");
    }

    /// 对外标签指向调用方注册的 Config 类型，不得退化成实参里的值类型。
    #[test]
    fn short_type_path_keeps_container_name() {
        assert_eq!(short_type_path("core::option::Option<crate::Db>"), "Option");
        assert_eq!(short_type_path("alloc::vec::Vec<crate::T>"), "Vec");
        assert_eq!(
            short_type_path("std::collections::HashMap<String, crate::Db>"),
            "HashMap"
        );
        assert_eq!(short_type_path("my_crate::config::DbConfig"), "DbConfig");
    }

    #[cfg(feature = "confers")]
    #[test]
    fn sensitive_key_matching_follows_segments() {
        assert!(is_sensitive_key("password"));
        assert!(is_sensitive_key("db_password"));
        assert!(is_sensitive_key("API_KEY"));
        assert!(is_sensitive_key("db.password"));
        assert!(!is_sensitive_key("tokenizer"));
        assert!(!is_sensitive_key("host"));
    }

    /// 带空格分隔符的键值对：值落在分隔符之后的片段里，必须被抹掉。
    #[test]
    fn redact_text_masks_value_after_spaced_separator() {
        assert_eq!(
            redact_text("password = hunter2", false),
            "password = [REDACTED]"
        );
        assert_eq!(
            redact_text("password : hunter2", false),
            "password : [REDACTED]"
        );
        assert_eq!(
            redact_text("api_key = sk-123", false),
            "api_key = [REDACTED]"
        );
        assert_eq!(
            redact_text("password =hunter2", false),
            "password =[REDACTED]"
        );
    }

    /// 紧凑写法的键值对：值与键同处一个片段，必须被抹掉。
    #[test]
    fn redact_text_masks_value_after_compact_separator() {
        assert_eq!(
            redact_text("password=hunter2", false),
            "password=[REDACTED]"
        );
        assert_eq!(
            redact_text("password:hunter2", false),
            "password:[REDACTED]"
        );
    }

    /// 值里含敏感词时不得中止键名扫描：外层真正的键名仍要脱敏。
    ///
    /// `api_key=mysecret` 里的 `secret` 落在值部分，若就此认定片段以键名
    /// 结尾，`api_key` 就再也扫不到，凭据随之原样外流。
    #[test]
    fn redact_text_masks_key_when_sensitive_word_appears_in_value() {
        for (input, expected) in [
            ("api_key=mysecret", "api_key=[REDACTED]"),
            ("token=mysecret", "token=[REDACTED]"),
            ("token=abcsecret", "token=[REDACTED]"),
            ("secret_key=mytoken", "secret_key=[REDACTED]"),
            ("passphrase=mysecret", "passphrase=[REDACTED]"),
            ("private_key=mysecret", "private_key=[REDACTED]"),
            ("encryption_key=mytoken", "encryption_key=[REDACTED]"),
            ("signing_key=mysecret", "signing_key=[REDACTED]"),
        ] {
            assert_eq!(redact_text(input, false), expected, "input: {input}");
        }
    }

    /// 没有凭据的散文不能被误伤：键名后不是分隔符时不得吞掉片段。
    #[test]
    fn redact_text_keeps_prose_without_secrets() {
        assert_eq!(redact_text("password is wrong", false), "password is wrong");
        assert_eq!(
            redact_text("invalid token detected", false),
            "invalid token detected"
        );
        assert_eq!(
            redact_text("plain detail without secrets", false),
            "plain detail without secrets"
        );
    }

    /// 认证方案词本身保留，其后的凭据片段被抹掉（不限 bearer）。
    #[test]
    fn redact_text_masks_credentials_after_auth_scheme() {
        assert_eq!(
            redact_text("Authorization: Basic dXNlcjpwYXNz", false),
            "Authorization: Basic [REDACTED]"
        );
        assert_eq!(
            redact_text("Authorization: NTLM TLMTTNTUAAAAAAA", false),
            "Authorization: NTLM [REDACTED]"
        );
        assert_eq!(
            redact_text("Authorization: token ghp_ABC", false),
            "Authorization: token [REDACTED]"
        );
        assert_eq!(
            redact_text("Authorization: Bearer abc123", false),
            "Authorization: Bearer [REDACTED]"
        );
    }

    /// 凭据材料类键名：键名命中且其后的值被抹掉。
    #[cfg(feature = "confers")]
    #[test]
    fn sensitive_key_matching_covers_credential_material() {
        assert!(is_sensitive_key("passphrase"));
        assert!(is_sensitive_key("private_key"));
        assert!(is_sensitive_key("signing_key"));
        assert!(is_sensitive_key("encryption_key"));
        assert!(is_sensitive_key("client_secret"));
        assert!(!is_sensitive_key("public_key"));
        assert!(!is_sensitive_key("encryption_key_id"));
    }

    /// 凭据材料类键名的值不得原样出现在对外文本里。
    #[test]
    fn redact_text_masks_credential_material_values() {
        assert_eq!(
            redact_text("passphrase=correct-horse-battery", false),
            "passphrase=[REDACTED]"
        );
        assert_eq!(
            redact_text(
                "private_key=-----BEGINPRIVATEKEY-----MIIEvQ-----ENDPRIVATEKEY-----",
                false
            ),
            "private_key=[REDACTED]"
        );
        assert_eq!(
            redact_text("signing_key=mysecret", false),
            "signing_key=[REDACTED]"
        );
        assert_eq!(
            redact_text("encryption_key=mytoken", false),
            "encryption_key=[REDACTED]"
        );
        assert_eq!(
            redact_text("client_secret=s3cr3t", false),
            "client_secret=[REDACTED]"
        );
    }

    fn pem_head(kind: &str) -> String {
        format!("-----BEGIN {kind}-----")
    }

    fn pem_fixture(kind: &str, body: &str) -> String {
        format!("{} {body} -----END {kind}-----", pem_head(kind))
    }

    /// PEM 块按空白切分后横跨多个片段：块内私钥材料必须逐段抹除。
    #[test]
    fn redact_text_masks_pem_block_material() {
        let redacted = redact_text(
            &format!("private_key={}", pem_fixture("PRIVATE KEY", "MIIEvQIB3gk")), // pragma: allowlist secret
            false,
        );
        assert!(
            !redacted.contains("MIIEvQIB3gk"),
            "PEM 块内的密钥材料不得外泄: {redacted}"
        );
        assert_eq!(
            redacted,
            "private_key=[REDACTED] [REDACTED] [REDACTED] [REDACTED] [REDACTED] PRIVATE KEY-----"
        );
        // 块未闭合（文本被截断）时，块内余下片段同样不得外泄。
        let truncated = redact_text(
            &format!("private_key={} MIIEvQIB3gk", pem_head("PRIVATE KEY")), // pragma: allowlist secret
            false,
        );
        assert!(
            !truncated.contains("MIIEvQIB3gk"),
            "未闭合的 PEM 块材料不得外泄: {truncated}"
        );
        assert_eq!(
            truncated,
            "private_key=[REDACTED] [REDACTED] [REDACTED] [REDACTED]"
        );
    }

    /// 没有敏感键名前缀的独立 PEM 块同样不得外泄。
    #[test]
    fn redact_text_masks_standalone_pem_block() {
        let redacted = redact_text(&pem_fixture("PRIVATE KEY", "MIIEvQIB3gk"), false); // pragma: allowlist secret
        assert!(
            !redacted.contains("MIIEvQIB3gk"),
            "独立 PEM 块的密钥材料不得外泄: {redacted}"
        );
        assert!(
            !redacted.contains(&pem_head("PRIVATE KEY")),
            "独立 PEM 块的块头不得外泄: {redacted}"
        );
    }

    /// 以敏感键名结尾的绝对路径仍按路径脱敏：目录结构不应外泄。
    #[test]
    fn redact_text_masks_paths_ending_with_sensitive_key() {
        assert_eq!(
            redact_text("open /etc/secrets/token failed", true),
            "open [REDACTED]token failed"
        );
    }
}
