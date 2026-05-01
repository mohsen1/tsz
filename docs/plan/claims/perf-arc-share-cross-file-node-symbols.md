Status: claim
Branch: perf/arc-share-cross-file-node-symbols
Owner: Codex
Created: 2026-05-01T16:47:51Z

## Scope

Workstream 5 large-repo residency: Arc-share `MergedProgram.cross_file_node_symbols`
so the CLI driver can install the program-wide map into `ProjectEnv` with an
O(1) clone instead of deep-cloning the outer `FxHashMap` once per check.

## Files

- `crates/tsz-core/src/parallel/core.rs`
- `crates/tsz-cli/src/driver/check.rs`
- `docs/plan/claims/perf-arc-share-cross-file-node-symbols.md`

## Verification

- `cargo fmt --check`
- targeted `cargo check` for affected crates
- `scripts/bench/perf-hotspots.sh --quick` before/after if the build window permits
