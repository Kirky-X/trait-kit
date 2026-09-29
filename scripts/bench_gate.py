#!/usr/bin/env python3
# Copyright (c) 2026 Kirky.X🌠
# SPDX-License-Identifier: MIT
"""CI bench gate: assert criterion medians stay within order-of-magnitude limits.

Thresholds are the local baseline medians ×100, rounded up to the next whole
thousand (derivation and the baseline ledger live in
docs/bench-baseline.md). The gate is a disaster guard for shared-runner
noise, not a fine-grained regression detector. Run locally after
`cargo bench --features toggle,confers` with no arguments.
"""

import argparse
import json
import pathlib
import sys

LIMITS_NS = {
    "build_three_module_chain": 63_000,
    "require_arc_capability_top": 2_000,
    "config_read_clone": 5_000,
    "config_write_set_config": 4_000,
    "config_write_merge_config": 4_000,
    "toggle_set": 7_000,
    "toggle_get": 173_000,
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--criterion-dir",
        type=pathlib.Path,
        default=pathlib.Path("target/criterion"),
        help="criterion output directory (default: target/criterion)",
    )
    args = parser.parse_args()

    failures = []
    measured_dirs = set()
    for name, limit in LIMITS_NS.items():
        path = args.criterion_dir / name / "new" / "estimates.json"
        try:
            median = json.loads(path.read_text())["median"]["point_estimate"]
        except (FileNotFoundError, json.JSONDecodeError, KeyError):
            failures.append(
                f"{name}: estimates.json missing or corrupt ({path})"
            )
            continue
        measured_dirs.add(name)
        status = "OK" if median <= limit else "FAIL"
        print(f"{status} {name}: median {median:.0f} ns (limit {limit} ns)")
        if median > limit:
            failures.append(
                f"{name}: median {median:.0f} ns exceeds limit {limit} ns — "
                "order-of-magnitude regression; check docs/bench-baseline.md"
            )
    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1

    # Coverage warning: benchmarks that ran but have no entry in LIMITS_NS
    # silently escape the gate. Warn instead of failing — criterion keeps
    # stale directories for renamed/removed benchmarks, and failing on those
    # would produce false positives.
    unmanaged = {
        d.name
        for d in args.criterion_dir.iterdir()
        if (d / "new" / "estimates.json").is_file()
    } - set(LIMITS_NS)
    if unmanaged:
        print(
            "WARN unmanaged benchmarks have no gate threshold (add them to "
            f"LIMITS_NS and docs/bench-baseline.md): {sorted(unmanaged)}",
            file=sys.stderr,
        )
    print("bench gate passed: all medians within order-of-magnitude limits")
    return 0


if __name__ == "__main__":
    sys.exit(main())
