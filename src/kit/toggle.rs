// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Feature toggle system for runtime module enable/disable.
//!
//! The toggle system extends the `conditional` feature's compile-time
//! predicate-based registration (`register_if`) to runtime: modules can be
//! conditionally registered based on string-keyed feature flags that can be
//! toggled on/off at any point during the application lifecycle.
//!
//! Requires the `toggle` feature (which implies `conditional`).
