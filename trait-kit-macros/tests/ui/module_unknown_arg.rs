// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT

// Invalid `#[module(...)]` argument must produce a clear compile error.
use trait_kit_macros::Module;

#[derive(Module)]
#[module(nmae = "typo")]
struct BadArg;

fn main() {}
