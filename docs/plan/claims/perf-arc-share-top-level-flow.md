Status: ready
Branch: perf/arc-share-top-level-flow
Owner: Codex
Created: 2026-05-01T17:19:15Z

## Scope

Workstream 5 large-repo residency: Arc-share `BinderState.top_level_flow` so
`BinderState::clone()` does not deep-clone the per-file top-level flow map.

## Files

- `crates/tsz-binder/src/state/mod.rs`
- `crates/tsz-binder/src/state/core.rs`
- `docs/plan/claims/perf-arc-share-top-level-flow.md`

## Verification

- `cargo fmt --check`
- `cargo check -p tsz-binder`
- `cargo test -p tsz-binder rebind`
- `scripts/bench/perf-hotspots.sh --quick`
