# Developer Tooling Guide

This document describes the scripts and tools available for development, testing, and analysis in the tsz codebase.

## Architecture Boundary Checking

### `scripts/arch/check-checker-boundaries.sh`

Enforces the checker-solver boundary at commit time. Runs as part of the pre-commit hook.

Checks for:
- Forbidden imports of solver internals (`TypeKey`, raw interner) from checker code
- Direct `CompatChecker` access from TS2322-family paths (should route through `query_boundaries`)
- Cross-layer imports that violate the pipeline architecture

```bash
# Run manually
./scripts/arch/check-checker-boundaries.sh
```

### `scripts/arch/arch_guard.py`

Python-based architecture guard that validates import patterns across the entire workspace.

```bash
# Run the guard
python3 scripts/arch/arch_guard.py

# Run its test suite
python3 scripts/arch/test_arch_guard.py
```

### `scripts/arch/render_architecture_report.py`

Generates an HTML/text report of crate dependencies and boundary compliance.

```bash
python3 scripts/arch/render_architecture_report.py
```

## Conformance Testing

### Quick Reference

```bash
# Build the conformance runner (fast profile)
cargo build --profile dist-fast -p tsz-conformance

# Run all tests
.target/dist-fast/tsz-conformance --cache-file scripts/conformance/tsc-cache-full.json

# Run filtered tests (fast iteration)
.target/dist-fast/tsz-conformance --filter "controlFlow" \
  --cache-file scripts/conformance/tsc-cache-full.json

# Verbose output (see expected vs actual diagnostics)
.target/dist-fast/tsz-conformance --filter "testName" --verbose \
  --cache-file scripts/conformance/tsc-cache-full.json

# Limit test count for quick smoke tests
.target/dist-fast/tsz-conformance --filter "pattern" --max 50 \
  --cache-file scripts/conformance/tsc-cache-full.json
```

### Offline Analysis Tools

These work from pre-computed snapshot files — zero CPU cost, instant results.

#### `scripts/conformance/query-conformance.py`

The primary analysis tool. Queries the conformance snapshot without running any tests.

```bash
# Overview: what to work on next
python3 scripts/conformance/query-conformance.py

# Root-cause campaign recommendations
python3 scripts/conformance/query-conformance.py --campaigns

# Deep-dive one campaign
python3 scripts/conformance/query-conformance.py --campaign big3

# Tests fixable by adding 1 missing diagnostic
python3 scripts/conformance/query-conformance.py --one-missing

# Tests fixable by removing 1 extra diagnostic (false positives)
python3 scripts/conformance/query-conformance.py --one-extra

# False positive breakdown
python3 scripts/conformance/query-conformance.py --false-positives

# Deep-dive a specific error code
python3 scripts/conformance/query-conformance.py --code TS2322

# List tests where a code is falsely emitted
python3 scripts/conformance/query-conformance.py --extra-code TS7053

# Tests closest to passing (diff <= N)
python3 scripts/conformance/query-conformance.py --close 2

# Export paths for piping
python3 scripts/conformance/query-conformance.py --code TS2454 --paths-only
```

#### `scripts/conformance/conformance.sh`

The main conformance test orchestrator. Handles running tests, generating snapshots, and analysis.

```bash
# Run all conformance tests
./scripts/conformance/conformance.sh run

# Run + analyze + save snapshots
./scripts/conformance/conformance.sh snapshot

# Analyze from existing snapshots (no CPU cost)
./scripts/conformance/conformance.sh analyze --campaigns
./scripts/conformance/conformance.sh analyze --one-missing
./scripts/conformance/conformance.sh analyze --close 2
```

### Snapshot Files

| File | Description |
|------|-------------|
| `scripts/conformance/conformance-snapshot.json` | High-level aggregates (summary, areas, top failures) |
| `scripts/conformance/conformance-detail.json` | Per-test failure data (expected/actual/missing/extra codes) |
| `scripts/conformance/conformance-baseline.txt` | One-line-per-test pass/fail with code diff |
| `scripts/conformance/tsc-cache-full.json` | tsc expected diagnostics for every test |

### Reading Snapshots Directly

```python
import json

# High-level summary
with open('scripts/conformance/conformance-snapshot.json') as f:
    snap = json.load(f)
# Keys: summary, areas_by_pass_rate, top_failures, partial_codes,
#        one_missing_zero_extra, one_extra_zero_missing, false_positive_codes

# Per-test detail
with open('scripts/conformance/conformance-detail.json') as f:
    detail = json.load(f)
# detail["failures"][test_path] = {"e": [...], "a": [...], "m": [...], "x": [...]}
# e=expected, a=actual, m=missing, x=extra

# tsc expected diagnostics
with open('scripts/conformance/tsc-cache-full.json') as f:
    cache = json.load(f)
# cache[test_key] = {"error_codes": [...], "diagnostic_fingerprints": [...]}
```

## Setup and Maintenance

### `scripts/setup/setup.sh`

One-time setup: installs git hooks, initializes TypeScript submodule.

```bash
./scripts/setup/setup.sh
```

### `scripts/safe-run.sh`

Memory-guarded command execution. Monitors RSS and kills the process if it exceeds the limit.

```bash
# Default limit (75% of system RAM)
scripts/safe-run.sh cargo test

# Custom limit
scripts/safe-run.sh --limit 8192 -- cargo build --release

# Verbose (show memory usage)
scripts/safe-run.sh --verbose -- cargo build
```

Use for: full conformance runs, `cargo test` (full suite), `cargo build --release`, and any multi-worker test runner.

### `scripts/setup/reset-ts-submodule.sh`

Resets the TypeScript submodule to the pinned commit SHA. Called automatically by the pre-commit hook.

## Pre-commit Hook Details

The pre-commit hook (`scripts/githooks/pre-commit`) keeps local commits cheap.
It blocks TypeScript submodule changes and formats staged Rust files. Build,
lint, unit, conformance, emit, fourslash, and WASM verification run in CI.

In fast mode it runs these checks in order:

1. **Submodule block** — prevents committing TypeScript submodule changes
2. **Formatting** — `cargo fmt` with auto-fix and re-stage

Environment variables to control behavior:
- `TSZ_SKIP_HOOKS=1` — skip all checks
- `TSZ_SKIP_BENCH=1` — skip microbenchmark check
- `TSZ_PRECOMMIT_FULL=1` — run strict legacy-style checks: cleanup, clippy fix, CI parity lint, wasm check, and transitive tests
- `TSZ_PRECOMMIT_CLEAN=1` — run target cleanup in fast mode
- `TSZ_PRECOMMIT_CLIPPY_FIX=1` — run `cargo clippy --fix` before clippy verification
- `TSZ_PRECOMMIT_CI_PARITY=1` — run full CI parity lint in fast mode
- `TSZ_PRECOMMIT_WASM=1` — run the wasm32 check in fast mode
- `TSZ_PRECOMMIT_RESET_TYPESCRIPT=1` — reset/init the TypeScript submodule before checking
- `TSZ_PRECOMMIT_TEST_SCOPE=affected` — test changed crates plus transitive dependents
- `TSZ_PRECOMMIT_TEST_SCOPE=all` or `TSZ_TEST_ALL=1` — force testing all workspace crates
- `TSZ_GIT_HOOK_RESET_TYPESCRIPT=1` — reset/init TypeScript from post-merge and post-rewrite hooks

The TypeScript submodule is intentionally on-demand in git hooks. Conformance,
emit, and fourslash runners initialize or validate `TypeScript/` when those
suites need the corpus or baselines.
