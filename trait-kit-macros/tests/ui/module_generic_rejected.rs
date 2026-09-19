// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT

// Generic structs cannot be module types (TypeId keying requires concrete
// types) — must fail with a clear error, not an unresolved-type cascade.
use trait_kit_macros::Module;

#[derive(Module)]
struct GenericModule<T> {
    value: T,
}

fn main() {}
