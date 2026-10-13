// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 嵌入 `locales/` 语言包内容，供 `I18nManager` 在编译时加载。
//!
//! 语言包按域拆分（与 base 工作区 confers/inklog 的 `locales/{en,zh}/` 布局一致）：
//!
//! - `errors`：错误 `Display` 模板（`TraitKitError`/`I18nError`）与诊断上下文标记；
//! - `logs`：`log` facade 输出（软构建降级、关闭桥接）与探针 readiness 载荷；
//! - `examples`：`examples/` crate 的演示输出。
//!
//! `include_str!` 使磁盘文件与内嵌内容同源（改磁盘即生效）；域清单守卫测试
//! `locales_dir_matches_embedded_list` 保证新增域文件不会漏挂。

/// 英文（fallback）错误消息。
pub const EN_ERRORS_FTL: &str = include_str!("../../../locales/en/errors.ftl");

/// 英文日志/降级/探针载荷消息。
pub const EN_LOGS_FTL: &str = include_str!("../../../locales/en/logs.ftl");

/// 英文示例演示输出。
pub const EN_EXAMPLES_FTL: &str = include_str!("../../../locales/en/examples.ftl");

/// 简体中文错误消息。
pub const ZH_ERRORS_FTL: &str = include_str!("../../../locales/zh/errors.ftl");

/// 简体中文日志/降级/探针载荷消息。
pub const ZH_LOGS_FTL: &str = include_str!("../../../locales/zh/logs.ftl");

/// 简体中文示例演示输出。
pub const ZH_EXAMPLES_FTL: &str = include_str!("../../../locales/zh/examples.ftl");

/// 英文语言包的 FTL 片段（按域），数组顺序即入束顺序。
pub const EN_FTLS: &[&str] = &[EN_ERRORS_FTL, EN_LOGS_FTL, EN_EXAMPLES_FTL];

/// 中文语言包的 FTL 片段（按域），数组顺序即入束顺序。
pub const ZH_FTLS: &[&str] = &[ZH_ERRORS_FTL, ZH_LOGS_FTL, ZH_EXAMPLES_FTL];

/// 内嵌语言包清单（`locales/` 相对路径 + 内容），仅供守卫测试比对磁盘目录。
#[cfg(test)]
pub const EMBEDDED_LOCALES: &[(&str, &str)] = &[
    ("locales/en/errors.ftl", EN_ERRORS_FTL),
    ("locales/en/logs.ftl", EN_LOGS_FTL),
    ("locales/en/examples.ftl", EN_EXAMPLES_FTL),
    ("locales/zh/errors.ftl", ZH_ERRORS_FTL),
    ("locales/zh/logs.ftl", ZH_LOGS_FTL),
    ("locales/zh/examples.ftl", ZH_EXAMPLES_FTL),
];
