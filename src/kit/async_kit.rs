// Copyright (c) 2026 Kirky.X
//
// Licensed under the MIT License
// See LICENSE file in the project root for full license information.

//! Stub for `AsyncKit<S>` — Phase 1a ships only the type definition so that
//! `AsyncAutoBuilder` (which references `AsyncKit` in its `build` signature)
//! can compile and be tested. Phase 1b replaces this file with a full
//! typestate implementation (`new` / `register` / `set_config` / `build` /
//! `require` / `optional` / `contains` / `contains_config`).
//!
//! Keeping the struct shape aligned with `Kit` lets Phase 1b swap in the real
//! `AsyncBuildFn` [`HashMap`] without touching downstream trait signatures.

use std::any::TypeId;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::{Arc, RwLock};

use super::async_typemap::AsyncTypeMap;
use super::graph::DependencyGraph;

/// Marker type for the unbuilt state. Phase 1b mirrors Kit's typestate.
pub struct Unbuilt;

/// Marker type for the ready (built) state. Phase 1b mirrors Kit's typestate.
pub struct Ready;

/// The async capability and configuration management center.
///
/// Phase 1a stub: only the struct layout exists. All methods are added in
/// Phase 1b. The `builders` map uses `()` as a placeholder; Phase 1b swaps in
/// the real `AsyncBuildFn` type-erased builder closure.
#[allow(dead_code, reason = "Phase 1b adds methods that read every field")]
#[allow(
    clippy::zero_sized_map_values,
    reason = "placeholder; Phase 1b replaces () with AsyncBuildFn"
)]
pub struct AsyncKit<S = Unbuilt> {
    /// Phase 1b replaces `()` with `AsyncBuildFn`.
    pub(crate) builders: Arc<RwLock<HashMap<TypeId, ()>>>,
    pub(crate) graph: DependencyGraph,
    pub(crate) configs: AsyncTypeMap,
    pub(crate) capabilities: AsyncTypeMap,
    pub(crate) _state: PhantomData<S>,
}
