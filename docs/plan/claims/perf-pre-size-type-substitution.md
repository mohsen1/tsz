Task: Workstream 5 instantiate_type allocation - pre-size TypeSubstitution maps
Status: claim
Owner: Codex
Branch: perf-pre-size-type-substitution
Created: 2026-05-02T00:06:47Z

Scope:
- Pre-size `TypeSubstitution`'s internal `FxHashMap` in construction paths
  that know their eventual entry count.
- Keep behavior unchanged and avoid touching query-cache semantics or
  cross-file identity rules.

Verification:
- `cargo fmt --check`
- `cargo check -p tsz-solver`
- `cargo test -p tsz-solver instantiation_cache`
- `scripts/bench/perf-hotspots.sh --quick`
