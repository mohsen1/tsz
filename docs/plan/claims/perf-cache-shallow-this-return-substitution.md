Task: Workstream 5 instantiate_type cache - cache shallow this return substitution
Status: claim
Branch: `perf/cache-shallow-this-return-substitution`

Scope:
- Add a distinct instantiation-cache mode for `substitute_this_type_at_return_position`, whose shallow-this walk differs from deep `substitute_this_type`.
- Route existing checker/property-access callers that already hold a `QueryDatabase` through the cache-aware path.
- Keep leaf fast paths and empty-substitution cache constraints intact.

Verification:
- `cargo fmt --check`
- `cargo check -p tsz-solver -p tsz-checker`
- `cargo test -p tsz-solver instantiation_cache`
- Focused checker/property-access regression test if an existing target covers this path.
- `scripts/bench/perf-hotspots.sh --quick`
