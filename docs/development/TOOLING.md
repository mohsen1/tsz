# Developer Tooling Guide

This document describes the scripts and tools available for development, testing, and analysis in the tsz codebase.

## Architecture Boundary Checking

### `scripts/arch/check-checker-boundaries.sh`

Enforces the checker-solver boundary at commit time. Runs as part of the pre-commit hook.

Checks for:
- Forbidden imports of solver internals (`TypeKey`, raw interner) from checker code
- Direct `CompatChecker` access from TS2322-family paths (should route through `query_boundaries`)
- Cross-layer imports that violate the pipeline architecture
- Track 10 ratchet metrics for post-check fingerprint rewrites, checker and
  emitter `source_text.contains` decisions, file-name/path substring
  decisions, and rendered-type string decisions

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

## Quality And Performance Tooling

These tools are additive guardrails. Normal PR CI keeps the fast path focused on
formatting, lint, dependency policy, build, and unit tests; heavier exploratory
tools run from the scheduled/manual `Quality Tools` workflow or local focused
commands.

### `cargo-deny`

`cargo-deny` runs in PR CI through `deny.toml`. It enforces dependency-source
policy, rejects wildcard dependency versions, checks RustSec advisories, and
keeps license review explicit.

```bash
cargo install cargo-deny --version 0.19.6 --locked
cargo deny check
```

### `cargo-shear`

`cargo-shear` runs in PR CI to catch dependencies that remain declared in
`Cargo.toml` after the code stops using them. Treat findings as dependency
graph hygiene work; do not use `--fix` in an unrelated semantic PR.

```bash
cargo install cargo-shear --version 1.12.0 --locked
cargo shear
```

### Miri

Miri is useful for undefined-behavior checks in pure Rust library tests. Keep it
focused; do not run conformance, emit, fourslash, CLI process harnesses, or the
whole workspace under Miri.

```bash
rustup toolchain install nightly --component miri
cargo +nightly miri setup
scripts/quality/run-miri.sh
```

Override the default target list with `package:test-filter` entries when
investigating a crate:

```bash
TSZ_MIRI_TARGETS="tsz-common:interner::tests::test_interner_intern_and_resolve" \
  scripts/quality/run-miri.sh
```

### Coverage

`cargo-llvm-cov` produces source coverage for focused library unit tests. The
default script covers the common scanner/parser/common substrate and writes an
LCOV artifact.

```bash
cargo install cargo-llvm-cov --version 0.8.7 --locked
scripts/quality/run-coverage.sh
```

### Mutation Testing

`cargo-mutants` is intentionally scoped by default. Use it to audit whether
focused unit tests actually protect a rule before or after high-risk checker,
solver, scanner, or parser changes.

```bash
cargo install cargo-mutants --version 27.0.0 --locked
scripts/quality/run-mutants-smoke.sh
TSZ_MUTANTS_PACKAGE=tsz-scanner TSZ_MUTANTS_FILE='crates/tsz-scanner/src/**/*.rs' \
  scripts/quality/run-mutants-smoke.sh
```

The smoke script lists mutants only. Run a real mutation campaign deliberately
with a tight file glob and `--test-tool nextest` once the baseline command is
known to be fast enough.

### SemVer Checks

`cargo-semver-checks` audits public Rust API compatibility for the publishable
workspace crates. It is manual-only in the `Quality Tools` workflow because
TSZ is pre-1.0 and internal public APIs still move often; use it before
releases or when a PR changes a public crate boundary. It is not a substitute
for TypeScript conformance.

```bash
cargo install cargo-semver-checks --version 0.47.0 --locked
scripts/quality/run-semver-checks.sh
TSZ_SEMVER_BASELINE_REV=origin/main scripts/quality/run-semver-checks.sh
TSZ_SEMVER_BASELINE_REV=v0.1.9 scripts/quality/run-semver-checks.sh
TSZ_SEMVER_PACKAGES="tsz-core tsz-checker" scripts/quality/run-semver-checks.sh
```

### Sanitizers

Sanitizer smoke tests are Linux/nightly-only and target narrow native library
tests. They are for unsafe/FFI/native-dependency investigations, not routine
local pre-commit.

```bash
rustup toolchain install nightly --component rust-src
scripts/quality/run-sanitizer.sh
```

### Performance Probes

The repo already has Criterion benches and a benchmark workflow. The quality
workflow adds lightweight profile-build and binary-size attribution probes:

```bash
cargo install cargo-bloat --version 0.12.1 --locked
scripts/quality/run-perf-probes.sh
```

For CPU investigations, prefer the existing flame profile:

```bash
cargo build --profile flame --bin tsz
samply record --save-only -o /tmp/tsz-profile.json -- .target/flame/tsz check benches/
```

Lower-priority overlaps are deliberately kept out of the default quality
workflow: `cargo-audit` is covered by `cargo-deny` advisories, broad formal
verification is too expensive without a specific proof target, and extra
binary-size/profiling tools should stay tied to a measured performance question.

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
