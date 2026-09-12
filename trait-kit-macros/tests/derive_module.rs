// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Behavior tests for `#[derive(Module)]`: the derived `ModuleMeta`
//! must be indistinguishable from the hand-written impl.

use std::any::TypeId;

use trait_kit::core::{AutoBuilder, ModuleMeta};
use trait_kit::kit::Kit;
use trait_kit_macros::Module;

// ─── Fixtures ───────────────────────────────────────────────────────────────

#[derive(Debug)]
struct DerivedError;

impl std::fmt::Display for DerivedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "derived test error")
    }
}
impl std::error::Error for DerivedError {}

#[derive(Debug, Clone)]
struct Cap(u32);

/// Hand-written reference module (no deps).
struct ManualLeaf;
impl ModuleMeta for ManualLeaf {
    const NAME: &'static str = "manual-leaf";
}
impl AutoBuilder for ManualLeaf {
    type Capability = Cap;
    type Error = DerivedError;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Cap(1))
    }
}

/// Derived module mirroring `ManualLeaf` — default name (struct ident).
#[derive(Module)]
struct ManualLeafDerived;
impl AutoBuilder for ManualLeafDerived {
    type Capability = Cap;
    type Error = DerivedError;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Cap(1))
    }
}

/// Derived module with explicit `name` and `deps(...)`.
#[derive(Module)]
#[module(name = "derived-top", deps(ManualLeaf))]
struct DerivedTop;
impl AutoBuilder for DerivedTop {
    type Capability = Cap;
    type Error = DerivedError;
    fn build(kit: &Kit) -> Result<Self::Capability, Self::Error> {
        let leaf = kit.require::<ManualLeaf>().map_err(|_| DerivedError)?;
        Ok(Cap(leaf.0 + 1))
    }
}

/// Derived module using the `deps = [...]` list form.
#[derive(Module)]
#[module(deps = [ManualLeaf])]
struct DerivedTopBracketForm;

// ─── Equivalence: derived vs hand-written ───────────────────────────────────

#[test]
fn derived_name_defaults_to_struct_ident() {
    assert_eq!(<ManualLeafDerived as ModuleMeta>::NAME, "ManualLeafDerived");
}

#[test]
fn derived_name_override_matches_hand_written_convention() {
    assert_eq!(<DerivedTop as ModuleMeta>::NAME, "derived-top");
}

#[test]
fn derived_dependencies_match_hand_written_type_ids() {
    // Hand-written side: ManualLeaf declares no deps. A hand-written top
    // module would declare `[(ManualLeaf::NAME, TypeId::of::<ManualLeaf>())]`.
    let expected: &[(&'static str, TypeId)] = &[(
        <ManualLeaf as ModuleMeta>::NAME,
        TypeId::of::<ManualLeaf>(),
    )];
    assert_eq!(
        <DerivedTop as ModuleMeta>::dependencies(),
        expected,
        "deps(...) form must equal the hand-written declaration"
    );
    assert_eq!(
        <DerivedTopBracketForm as ModuleMeta>::dependencies(),
        expected,
        "deps = [...] form must equal the hand-written declaration"
    );
    assert!(
        <ManualLeafDerived as ModuleMeta>::dependencies().is_empty(),
        "no deps attribute → empty dependency list"
    );
}

#[test]
fn derived_module_behaves_identically_in_kit() {
    // Hand-written flow
    let mut kit = Kit::new();
    kit.register::<ManualLeaf>().expect("register manual");
    kit.register::<DerivedTop>().expect("register derived top");
    let ready = kit.build().expect("build ok");
    let cap = ready.require::<DerivedTop>().expect("require derived");
    assert_eq!(cap.0, 2, "dependency injection through derived module works");

    // The graph validates the derived module's declared deps: requiring the
    // leaf capability resolves the same singleton the derived module saw.
    let leaf = ready.require::<ManualLeaf>().expect("require manual");
    assert_eq!(leaf.0, 1);
}

#[test]
fn derived_module_duplicate_registration_is_rejected() {
    // Same duplicate-name rule applies to derived modules as to hand-written.
    let mut kit = Kit::new();
    kit.register::<ManualLeaf>().expect("first");
    kit.register::<ManualLeafDerived>().expect("derived ok");
    let dup = kit.register::<ManualLeafDerived>().unwrap_err();
    assert!(
        matches!(dup, trait_kit::TraitKitError::AlreadyRegistered { .. }),
        "duplicate derived registration rejected: {dup:?}"
    );
}
