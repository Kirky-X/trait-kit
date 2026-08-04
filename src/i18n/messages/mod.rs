// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 嵌入 FTL 消息文件内容，供 `I18nManager` 在编译时加载。

/// 英文（fallback）消息文件。
pub const EN_FTL: &str = include_str!("en.ftl");

/// 简体中文消息文件。
pub const ZH_FTL: &str = include_str!("zh.ftl");
