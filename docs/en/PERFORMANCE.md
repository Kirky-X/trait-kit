# ⚡ Trait-Kit Performance Benchmarks

> Delivers on the README's "zero runtime overhead" claim: establish a reproducible performance baseline with criterion benchmarks.
> This file is the **single source of truth** for baseline numbers, gate thresholds, and refresh rules. **CI gate policy**: the bench job in ci.yml uses an order-of-magnitude threshold ("local baseline ×100, rounded up to the nearest thousand") as a catastrophic-regression guard (the repo variable
> `BENCH_GATE_DISABLED=true` can disable the gate entirely) — shared runners are noisy, so this gate is **not** a strict regression detector; ±20%-level fine-grained regression judgment can only be done via the local baseline workflow on the same machine.

## 🏃 How to Run

```sh
# All benchmarks (the toggle benchmarks need the toggle feature; config/write_merge_config
# additionally needs confers; do NOT run benchmarks with --all-features — see the
# measurement note for the merge_config row below)
cargo bench --features toggle,confers

# Run a single benchmark
cargo bench --features toggle -- build/three_module_chain
```

Benchmark source: `benches/kit_bench.rs` (single target, `required-features = ["toggle"]`;
plain `cargo bench` (no feature) skips the target with a hint).

The benchmarks cover four hot-path axes:

| Axis | Benchmark | Description |
| --- | --- | --- |
| build | `build/three_module_chain` | 3-module chain (leaf→mid→top) register + `build()` (incl. graph validation / topological sort / per-module build) |
| require | `require/arc_capability_top` | `require::<M>()` returning an `Arc` capability (expected ≈ one `Arc::clone`) |
| config | `config/read_clone` | `config::<C>()` read (incl. `Clone` copy, a mid-sized 5-element Vec struct) |
| config | `config/write_set_config` | `set_config` overwrite (value swap in the `TypeMap`) |
| config | `config/write_merge_config` | `merge_config` read-modify-write (`confers` feature; available on a Ready Kit). **Measured only in combinations without `report`**: with `report` enabled the benchmark is compiled out — the report history is bounded (1024-entry ring semantics), but once at capacity every call triggers O(n) head removal + drop counting (see `take_config_overrides`), and an iterative benchmark would mix bookkeeping cost into the measurement and degrade into a memmove/cache benchmark (where the iteration count is a function of machine speed, making numbers incomparable) |
| toggle | `toggle/set` | `enable_toggle` (Ready Kit, HashMap backend) |
| toggle | `toggle/get` | `is_toggle_enabled` (both the miss and hit paths are of this magnitude) |

The `report` feature's recording overhead (one `Vec` push per `merge_config`, one accumulation per module on the build path) is for diagnostics and orders of magnitude below the hot paths above; it has no dedicated benchmark axis and is a **non-goal**; measured without `report`, `config/write_merge_config` is its pure-logic cost. The probe path (`probe` feature) has no dedicated benchmark axis either: `run_probes` adds a framework measurement overhead of one pair of `Instant::now()` calls per probe (nanosecond-level) plus one timer arm for the timeout variant, a non-goal relative to the millisecond-level latency of network probes themselves.

## 📊 Baseline (2026-09-29 quiet re-run, current)

- Machine: AMD Ryzen 9 9950X (16C/32T), WSL2 kernel 6.6.87
- Toolchain: rustc 1.97.1, `bench` profile (inherits `release`: `opt-level=3`, `lto=fat`, `codegen-units=1`)
- criterion config: `sample_size=20`, `warm_up_time=500ms`, `measurement_time=1s`
- Reporting basis: median (point estimate), unit ns/iter; feature set `toggle,confers`
- Data source: `median.point_estimate` from `target/criterion/<bench>/new/estimates.json`
- Measurement mode: short mode of 20 samples × 1s measurement and 500ms warm-up (`benches/kit_bench.rs::bench_config_short`); numbers are single-run measured values, not averages of multiple runs
- HEAD: `c04d9ab` (feature set toggle,confers, no report — `config/write_merge_config`
  is compiled out under report combinations, see the measurement note at the end)

| Benchmark | median |
| --- | --- |
| `build/three_module_chain` | ~625 ns |
| `require/arc_capability_top` | ~16 ns |
| `config/read_clone` | ~45 ns |
| `config/write_set_config` | ~36 ns |
| `config/write_merge_config` | ~39 ns |
| `toggle/set` | ~61 ns |
| `toggle/get` | ~1728 ns |

CI threshold derivation is based on this table (×100, rounded up to the nearest thousand, see "CI thresholds" below). This table is the gate derivation source: when refreshing, update `LIMITS_NS` in `scripts/bench_gate.py` in the same change (both places must change together).

### Reproducibility Notes

- `toggle/get` stayed at the ~1.7 µs magnitude across two independent full runs on this machine (1778 ns during a high-load period, 1728 ns on a re-run at load 1.12); one review round observed a single 454 ns value that was not reproducible on this machine and is not used as a baseline. If a stable lower value appears in the future, re-measure per the refresh rules below and update both places.
- The historical (2026-09-10) snapshot values toggle/set ≈21 ns and toggle/get ≈12 ns cannot be aligned with current HEAD measurements; the root cause is unidentified (its measurement conditions cannot be reproduced) and it is kept as a historical record in the next section only, excluded from gate derivation.
- The `report` feature's record/drain paths (the `push_config_override` recording cost of `merge_config` and the `mem::take` round-trip of `take_config_overrides`) **have no benchmark coverage and are an explicit non-goal**: `config/write_merge_config` is compiled out under report combinations (under an iterative benchmark, ring-eviction bookkeeping cost would pollute the measurement, see `benches/kit_bench.rs`), and CI bench also runs only `toggle,confers`. That path is for diagnostics and orders of magnitude below the hot paths; do not assume it is measured just because this table lacks the axis.

## 🗄 Historical Snapshot (2026-09-10, excluded from gate derivation)

- Early same-machine, same-criterion-config snapshot: build ~706 / require ~15 / read_clone ~39 /
  write_set_config ~34 / toggle/set ~21 / toggle/get ~12 (ns).
- The build/require/config rows agree in magnitude with current measurements; the two toggle rows
  differ from current measurements (61 ns / 1728 ns) by 3×–144×, root cause unidentified (the
  measurement conditions of that time cannot be reproduced; toggle.rs has only had its copyright
  header changed since the snapshot) — current re-measurements prevail.

### Conclusions

- **require ≈ 16 ns**: the same magnitude as a bare `Arc::clone` (≈1–2 ns) + `TypeId` HashMap lookup + downcast;
  the "zero-cost capability retrieval" goal holds; non-`Arc` capabilities are `Clone`d whole by `require`; for large structs use
  `require_ref` (borrowed read; mutually exclusive with the `RefCell` write path).
- **config read ≈ 45 ns**: includes the real copy cost of a mid-sized struct (String + Vec); the pure overhead part (lookup + downcast)
  is the same magnitude as require.
- **build ≈ 625 ns / 3 modules**: graph validation (missing deps + cycle detection) + Kahn sort + 3 build callbacks,
  a one-time startup cost of the expected magnitude.

## 🔁 Reproduce and Compare

1. Use a fixed machine plugged in, with power saving disabled;
2. `cargo bench --features toggle -- --save-baseline <name>` to save a baseline;
3. After changes, `cargo bench --features toggle -- --baseline <name>` to compare;
   criterion annotates regressions/improvements in the output (noise_threshold=5%).

## 🚦 CI Thresholds (enabled, order-of-magnitude guard)

- The ci.yml bench job asserts that the medians of the 7 benchmarks do not exceed the local baseline ×100 rounded up to the nearest thousand
  (`scripts/bench_gate.py`); the repo variable `BENCH_GATE_DISABLED=true` can disable the gate entirely.
- **Why ×100**: the gap between GitHub shared runners (ubuntu-latest) and local machines stacks three layers —
  single-core performance difference (≈3–10×), virtualization/neighbor noise (≈3–10×), and a conservative margin. The combined cap is set at two
  orders of magnitude. The gate is a catastrophic-regression guard (algorithmic complexity degradation, lock-contention explosion, accidental allocation storms and other
  ≥100× cliffs), not a strict regression detector; any tighter threshold on shared runners would be pierced by noise
  and lose its gate meaning.
- **Coverage warning**: each run diffs the set of benchmark directories under `target/criterion` that actually produced `new/estimates.json`
  against the `LIMITS_NS` key set and logs unmanaged benchmarks as WARN to stderr (not failing — criterion
  keeps stale directories for deleted/renamed benchmarks, so failing would produce false positives); new benchmarks should be added to the thresholds and re-baselined in sync.
- **Baseline refresh**: after upgrading rustc / criterion / machine / feature set, re-run the local bench and
  **change both places together**: the baseline table in this file + `LIMITS_NS` in `scripts/bench_gate.py` (note the trigger reason in the commit message).
- ±20%-level fine-grained regression judgment still uses the local `--save-baseline`/`--baseline` flow; once multi-machine variance
  data is stable, CI thresholds may be tightened (updating both places together per the refresh rules at that time).
