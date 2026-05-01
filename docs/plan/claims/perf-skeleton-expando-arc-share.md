# perf(skeleton): Arc-share expando index

Status: claim
Owner: Codex
Branch: `perf/skeleton-expando-arc-share`
Created: 2026-05-01 17:53:33 UTC

## Intent

Workstream 5 large-repo residency: keep the `SkeletonIndex` merged expando
property index behind `Arc` so driver and server `ProjectEnv` installation can
reuse the skeleton allocation instead of deep-cloning the program-wide map.

## Planned Scope

- `crates/tsz-core/src/parallel/skeleton.rs`
- `crates/tsz-cli/src/driver/check.rs`
- `crates/tsz-cli/src/bin/tsz_server/check.rs`
- Targeted tests/fixtures if the public field type change requires updates.

## Verification Plan

- `cargo fmt --check`
- `cargo check -p tsz-core -p tsz-cli`
- `cargo test -p tsz-core skeleton`
- `scripts/bench/perf-hotspots.sh --quick`
