// Copyright (c) 2026 Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! trait-kit — 模块标准接口与能力管理中心
//!
//! 提供模块定义标准接口和 Kit 能力管理中心的轻量实现。

#![deny(unsafe_code)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![doc = include_str!("../README.md")]

pub mod core;
pub mod kit;

pub mod prelude;

#[cfg(feature = "async")]
pub use core::meta::AsyncAutoBuilder;
#[cfg(feature = "async")]
pub use kit::{AsyncKit, AsyncReady, AsyncUnbuilt};
