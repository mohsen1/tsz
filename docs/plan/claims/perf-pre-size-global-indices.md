Task: Workstream 5 checker residency - pre-size global index builders

Branch: `perf-pre-size-global-indices`

Status: claim

Claimed: 2026-05-01 23:00:46 UTC

Scope:
- Pre-size project-wide global index maps in `CheckerContext::build_global_indices`
  from known binder counts before inserting.
- Keep behavior unchanged: only allocation/hash-table growth patterns should move.

Rationale:
- Workstream 5 calls out eliminating per-file and program-wide map overhead that
  blocks large-repo completion. The global-index builder already walks all
  binders and knows enough cardinality to avoid repeated map growth during the
  same pass.

Verification Plan:
- `cargo fmt --check`
- `cargo check -p tsz-checker`
- focused checker/context tests if an existing target covers global indices
- `cargo clippy -p tsz-checker --all-targets -- -D warnings`
- `scripts/bench/perf-hotspots.sh --quick`
- guarded large-repo sample:
  `scripts/safe-run.sh --limit 75% --interval 2 --verbose -- .target-bench/dist/tsz --extendedDiagnostics --noEmit -p ~/code/large-ts-repo/tsconfig.flat.bench.json`
