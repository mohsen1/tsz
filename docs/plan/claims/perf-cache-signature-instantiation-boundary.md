Task: Workstream 5 binder residency - pre-size core parallel file_locals merges
Status: ready
Owner: Codex
Branch: perf/cache-signature-instantiation-boundary
Created: 2026-05-01T22:05:21Z

Scope:
- Mirror the CLI binder reconstruction pre-sizing in the core parallel binder
  paths that still merge per-file locals and program globals from an empty
  `SymbolTable`.
- Keep the slice narrow and avoid overlapping the open solver intrinsic
  fast-path PR or the already-merged cross-file lookup binder Arc-clone work.

Verification:
- `cargo fmt --check`
- `cargo check -p tsz-core`
- `cargo test -p tsz-core create_binder_from_bound_file_composes_per_file_and_global`
- `cargo clippy -p tsz-core --all-targets -- -D warnings`
- `scripts/bench/perf-hotspots.sh --quick`
  (`artifacts/perf/hotspots-20260501-150807.json`): tsz beat tsgo on all five
  quick fixtures: 100 classes 2.13x, 50 generic functions 1.29x,
  DeepPartial optional-chain N=50 1.35x, Shallow optional-chain N=50 1.38x,
  Constraint conflicts N=30 1.64x.
- Guarded large-repo sample:
  `RUST_BACKTRACE=1 scripts/safe-run.sh --limit 75% --interval 2 --verbose -- .target-bench/dist/tsz --extendedDiagnostics --noEmit -p ~/code/large-ts-repo/tsconfig.flat.bench.json`;
  manual stop after a stable sample window (exit 130 from Ctrl-C), peak sampled
  physical footprint 11804 MB / 12288 MB guard.
