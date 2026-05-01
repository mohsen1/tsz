# perf(cli): avoid semantic def clone before shared store

Status: claim
Owner: Codex
Branch: `perf/definition-store-semantic-def-overlay`
Created: 2026-05-01 19:03:27 UTC

## Intent

Workstream 5 large-repo residency: build the CLI shared `DefinitionStore`
from program-wide semantic defs plus per-file overlays without deep-cloning the
entire `program.semantic_defs` map first.

## Planned Scope

- `crates/tsz-solver/src/def/core.rs`
- `crates/tsz-cli/src/driver/check.rs`
- Focused tests for overlay precedence and shared-store construction behavior.

## Verification Plan

- `cargo fmt --check`
- `cargo check -p tsz-solver -p tsz-cli`
- `cargo test -p tsz-solver from_semantic_defs`
- `cargo test -p tsz-cli driver`
- `scripts/bench/perf-hotspots.sh --quick`
