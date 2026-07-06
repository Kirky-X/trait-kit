// Copyright © 2026 Kirky.X

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
