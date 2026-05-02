Task: Workstream 5 instantiate_type allocation - pre-size TypeSubstitution maps
Status: ready
Owner: Codex
Branch: perf-pre-size-type-substitution
Created: 2026-05-02T00:06:47Z

Scope:
- Pre-size `TypeSubstitution`'s internal `FxHashMap` in construction paths
  that know their eventual entry count.
- Keep behavior unchanged and avoid touching query-cache semantics or
  cross-file identity rules.

Verification:
- `cargo fmt --check` (pass)
- `cargo check -p tsz-solver` (pass)
- `cargo test -p tsz-solver instantiation_cache` (16 passed)
- `scripts/bench/perf-hotspots.sh --quick`
  (`artifacts/perf/hotspots-20260501-170810.json`): tsz beat tsgo on all five
  quick fixtures: 100 classes 2.15x, 50 generic functions 1.44x,
  DeepPartial optional-chain N=50 1.40x, Shallow optional-chain N=50 1.42x,
  Constraint conflicts N=30 1.75x.
- Guarded large-repo sample:
  `RUST_BACKTRACE=1 scripts/safe-run.sh --limit 75% --interval 2 --verbose -- .target-bench/dist/tsz --extendedDiagnostics --noEmit -p ~/code/large-ts-repo/tsconfig.flat.bench.json`;
  manual stop after a stable sample window (exit 143), peak sampled physical
  footprint 9624 MB / 12288 MB guard.
