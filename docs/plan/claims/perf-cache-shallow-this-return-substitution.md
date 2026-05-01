Task: Workstream 5 instantiate_type cache - cache shallow this return substitution
Status: ready
Branch: `perf/cache-shallow-this-return-substitution`

Scope:
- Add a distinct instantiation-cache mode for `substitute_this_type_at_return_position`, whose shallow-this walk differs from deep `substitute_this_type`.
- Route existing checker/property-access callers that already hold a `QueryDatabase` through the cache-aware path.
- Keep leaf fast paths and empty-substitution cache constraints intact.

Verification:
- `cargo fmt --check` (pass)
- `cargo check -p tsz-solver -p tsz-checker` (pass)
- `cargo test -p tsz-solver instantiation_cache` (16 passed)
- `cargo test -p tsz-core test_covariant_this_fluent_api` (pass)
- `scripts/conformance/conformance.sh run --filter "intersectionThisTypes"` (1/1 passed)
- `cargo clippy -p tsz-solver -p tsz-checker --all-targets -- -D warnings` (pass)
- `scripts/bench/perf-hotspots.sh --quick` (pass; tsz wins 5/5, artifact `artifacts/perf/hotspots-20260501-142711.json`)
- Guarded large-repo sample: `scripts/safe-run.sh --limit 75% --interval 2 --verbose -- .target-bench/dist/tsz --extendedDiagnostics --noEmit -p ~/code/large-ts-repo/tsconfig.flat.bench.json`; manual stop exit 143 after sample window, peak sampled physical footprint ~11.35 GB / 12.29 GB guard.
