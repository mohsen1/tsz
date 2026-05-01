# perf(binder): Arc-share binder cross-file node symbols

Status: claim
Owner: Codex
Branch: `perf/binder-cross-file-node-symbols-arc`
Created: 2026-05-01 18:35:04 UTC

## Intent

Workstream 5 large-repo residency: keep `BinderState.cross_file_node_symbols`
behind `Arc<CrossFileNodeSymbols>` so legacy/core binder reconstruction can
share the merged arena-pointer index instead of deep-cloning the outer map.

## Planned Scope

- `crates/tsz-binder/src/state/`
- `crates/tsz-core/src/parallel/core.rs`
- Direct tests/fixtures constructing `BinderStateScopeInputs`.

## Verification Plan

- `cargo fmt --check`
- `cargo check -p tsz-binder -p tsz-core`
- `cargo test -p tsz-core parallel`
- `scripts/bench/perf-hotspots.sh --quick`
