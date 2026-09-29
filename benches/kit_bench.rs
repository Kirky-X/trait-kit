// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Criterion benchmarks for the core Kit hot paths.
//!
//! Covers the four axes promised by the "runtime zero-overhead" claim:
//!
//! - `build/*`  — register + topological build of a three-module chain.
//! - `require/*` — capability retrieval (`Arc` clone semantics).
//! - `config/*` — typed config write (`set_config`) and read (`config`).
//! - `toggle/*` — runtime feature toggle set/get (`toggle` feature).
//!
//! Run with:
//!
//! ```text
//! cargo bench --features toggle,confers
//! ```
//!
//! The `toggle` feature is required because the toggle benchmarks exercise the
//! `toggle`-gated API surface; `confers` adds the
//! `config/write_merge_config` benchmark (it is skipped silently under
//! `--features toggle` alone, and under `--all-features` it is compiled out:
//! the `report`-gated record accumulation would grow unboundedly per
//! iteration and invalidate the measurement). Baseline numbers live in
//! `docs/PERFORMANCE.md`.

use std::sync::Arc;
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};

use trait_kit::TraitKitError;
use trait_kit::core::{AutoBuilder, ModuleMeta};
use trait_kit::kit::{Kit, Ready};

// ─── Fixtures ───────────────────────────────────────────────────────────────

/// Typed configuration payload used by the config benchmarks.
#[derive(Debug, Clone)]
#[allow(
    dead_code,
    reason = "fields exercise realistic clone cost in read bench"
)]
struct BenchConfig {
    url: String,
    retries: u32,
    timeouts: Vec<u64>,
}

/// Field-level override payload for [`BenchConfig`] (`confers` feature).
#[cfg(feature = "confers")]
#[derive(Clone, Default)]
struct BenchConfigOverride {
    url: Option<String>,
    retries: Option<u32>,
}

#[cfg(feature = "confers")]
impl trait_kit::kit::ConfigInherit for BenchConfig {
    type Override = BenchConfigOverride;
    fn apply_override(&mut self, ovr: &Self::Override) {
        if let Some(ref u) = ovr.url {
            self.url.clone_from(u);
        }
        if let Some(r) = ovr.retries {
            self.retries = r;
        }
    }
}

/// Capability handed out by every fixture module (cheap `Arc` clone).
#[derive(Debug, Clone)]
struct BenchCap(u32);

/// Fixture error type (never constructed — all builds succeed).
#[derive(Debug)]
struct BenchError;

impl std::fmt::Display for BenchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bench error")
    }
}

impl std::error::Error for BenchError {}

/// Leaf module: no dependencies.
struct BenchLeaf;
impl ModuleMeta for BenchLeaf {
    const NAME: &'static str = "bench-leaf";
}
impl AutoBuilder for BenchLeaf {
    type Capability = Arc<BenchCap>;
    type Error = BenchError;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(BenchCap(1)))
    }
}

/// Middle module: depends on `BenchLeaf`.
struct BenchMid;
impl ModuleMeta for BenchMid {
    const NAME: &'static str = "bench-mid";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        static DEPS: &[(&str, std::any::TypeId)] = &[(
            <BenchLeaf as ModuleMeta>::NAME,
            std::any::TypeId::of::<BenchLeaf>(),
        )];
        DEPS
    }
}
impl AutoBuilder for BenchMid {
    type Capability = Arc<BenchCap>;
    type Error = BenchError;
    fn build(kit: &Kit) -> Result<Self::Capability, Self::Error> {
        let leaf = kit.require::<BenchLeaf>().map_err(|_| BenchError)?;
        Ok(Arc::new(BenchCap(leaf.0 + 1)))
    }
}

/// Top module: depends on `BenchMid` (three-level chain leaf → mid → top).
struct BenchTop;
impl ModuleMeta for BenchTop {
    const NAME: &'static str = "bench-top";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        static DEPS: &[(&str, std::any::TypeId)] = &[(
            <BenchMid as ModuleMeta>::NAME,
            std::any::TypeId::of::<BenchMid>(),
        )];
        DEPS
    }
}
impl AutoBuilder for BenchTop {
    type Capability = Arc<BenchCap>;
    type Error = BenchError;
    fn build(kit: &Kit) -> Result<Self::Capability, Self::Error> {
        let mid = kit.require::<BenchMid>().map_err(|_| BenchError)?;
        Ok(Arc::new(BenchCap(mid.0 + 1)))
    }
}

fn make_config() -> BenchConfig {
    BenchConfig {
        url: "postgres://localhost:5432/bench".to_string(),
        retries: 3,
        timeouts: vec![100, 250, 500, 1_000, 2_500],
    }
}

/// Register the full fixture chain plus config; returns the unbuilt kit.
fn registered_kit() -> Kit {
    let mut kit = Kit::new();
    kit.register::<BenchLeaf>().expect("register leaf");
    kit.register::<BenchMid>().expect("register mid");
    kit.register::<BenchTop>().expect("register top");
    kit.set_config(make_config());
    kit
}

/// Register + build the fixture chain (the `build/*` benchmark body).
fn build_ready_kit() -> Result<Kit<Ready>, TraitKitError> {
    registered_kit().build()
}

// ─── Benchmarks ─────────────────────────────────────────────────────────────

fn bench_build(c: &mut Criterion) {
    c.bench_function("build/three_module_chain", |b| {
        b.iter(|| build_ready_kit().expect("build succeeds"))
    });
}

fn bench_require(c: &mut Criterion) {
    let kit = build_ready_kit().expect("build succeeds");
    c.bench_function("require/arc_capability_top", |b| {
        b.iter(|| {
            let cap = kit.require::<BenchTop>().expect("require succeeds");
            std::hint::black_box(&cap);
        })
    });
}

fn bench_config(c: &mut Criterion) {
    let kit = build_ready_kit().expect("build succeeds");
    c.bench_function("config/read_clone", |b| {
        b.iter(|| {
            let cfg = kit.config::<BenchConfig>().expect("config set");
            std::hint::black_box(&cfg);
        })
    });
    // `set_config` lives on `Kit<Unbuilt>` (configuration precedes build);
    // benchmark the overwrite-in-place path on a dedicated unbuilt kit.
    let unbuilt = registered_kit();
    c.bench_function("config/write_set_config", |b| {
        b.iter(|| unbuilt.set_config(make_config()))
    });
    // `merge_config` is a read-modify-write usable on every state marker
    // (benchmarked on a ready kit). Measured **without** the `report`
    // feature on purpose: the report record accumulates per call with no
    // cap (see `take_config_overrides`), so an iterated benchmark under
    // `report` would grow the Vec by GBs within one run and degrade into
    // a realloc/cache-miss benchmark whose numbers depend on iteration
    // count. The pure read-modify-write cost is what this measures.
    #[cfg(all(feature = "confers", not(feature = "report")))]
    c.bench_function("config/write_merge_config", |b| {
        b.iter(|| {
            kit.merge_config::<BenchConfig>(BenchConfigOverride {
                url: Some("postgres://localhost:5433/bench".to_string()),
                retries: None,
            })
        })
    });
}

fn bench_toggle(c: &mut Criterion) {
    let kit = build_ready_kit().expect("build succeeds");
    c.bench_function("toggle/set", |b| {
        b.iter(|| kit.enable_toggle("bench-toggle", true))
    });
    c.bench_function("toggle/get", |b| {
        b.iter(|| {
            let enabled = kit.is_toggle_enabled("bench-toggle");
            std::hint::black_box(enabled);
        })
    });
}

// Keep the default run under ~2 minutes on a laptop while still collecting
// stable medians (20 samples × ~1s measurement each). CI thresholds are
// intentionally not enabled yet — see docs/PERFORMANCE.md.
fn bench_config_short() -> Criterion {
    Criterion::default()
        .sample_size(20)
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(1))
        .noise_threshold(0.05)
}

criterion_group! {
    name = benches;
    config = bench_config_short();
    targets = bench_build, bench_require, bench_config, bench_toggle
}
criterion_main!(benches);
