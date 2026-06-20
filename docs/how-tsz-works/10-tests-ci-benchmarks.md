# Tests, CI, And Benchmarks

`tsz` matches `tsc` by combining narrow local checks with broad CI-owned parity
gates. The repo contract is explicit: do not run full conformance, emit, or
fourslash locally. Use focused filters; CI owns broad suites.

## Local Test Style

Use `cargo nextest run`, not `cargo test`, unless a known local issue requires a
documented workaround. Wrap long or memory-heavy commands with
`scripts/safe-run.sh`.

Local tests usually target one crate or one family:

```bash
cargo nextest run -p tsz-checker <filter>
cargo nextest run -p tsz-solver <filter>
cargo nextest run -p tsz-emitter <filter>
```

For docs-only changes, local verification is usually the generated docs coverage
checker plus line-count and link/format sanity checks.

## Conformance

Diagnostic conformance compares `tsz` with TypeScript's own tests:

- Rust harness: `crates/conformance/`
- Scripts: `scripts/conformance/`
- Snapshots/cache/detail files: `scripts/conformance/*.json`,
  `scripts/conformance/*.txt`
- Skill workflow: `.agents/skills/tsz-conformance/SKILL.md`

The roadmap currently treats exact diagnostic conformance as a `hold` gate.
Accepted regressions must stay empty or carry fresh exact-head CI evidence.

## Emit

Emit parity is split between JavaScript emit and declaration emit:

- `scripts/emit/run.sh`
- `scripts/emit/query-emit.py`
- `scripts/emit/src/`
- `scripts/emit/emit-snapshot.json`
- `scripts/emit/emit-detail.json`
- `scripts/emit/audit-output-surgery.py`

Emitter code lives in `crates/tsz-emitter/`, but broad emit baseline comparison
is CI-owned.

## Fourslash

Fourslash tests exercise language-service behavior:

- `scripts/fourslash/run-fourslash.sh`
- `scripts/fourslash/runner.cjs`
- `scripts/fourslash/tsz-adapter.cjs`
- `scripts/fourslash/tsz-worker.cjs`
- `scripts/fourslash/fourslash-snapshot.json`
- LSP helpers under `crates/tsz-lsp/src/fourslash*`

The roadmap tracks fourslash as a `hold` gate.

## Benchmarks And Project Rows

Benchmark/project compatibility lives under `scripts/bench/` and `scripts/ci/`.
Important files:

- `scripts/bench/project-rows.mjs`
- `scripts/bench/project-row-summary.mjs`
- `scripts/bench/validate-project-metadata.mjs`
- `scripts/bench/bench-vs-tsgo.sh`
- `scripts/bench/measure-tsz.sh`
- `scripts/ci/project-compile-guard.sh`
- `scripts/ci/project-compatibility.mjs`
- `.github/workflows/bench.yml`

Required rows are documented in `docs/plan/ROADMAP.md`. A project row can be
Green, Yellow, Red, or Gray; stale artifacts are triage input, not status.

## Architecture And Quality Guards

Architecture checks prevent convenient boundary erosion:

- `scripts/architecture-check.sh`
- `scripts/arch/arch_guard.py`
- `scripts/arch/arch_guard_policy.toml`
- `scripts/arch/check-checker-boundaries.sh`
- `scripts/check-crate-root-files.sh`

Quality and setup helpers include `scripts/quality/`, `scripts/setup/`,
`scripts/agents/disk-preflight.sh`, and `.config/nextest.toml`.
