---
name: tsz-project-bench
description: Work on TSZ benchmark and project-corpus compatibility. Use when investigating bench.yml failures, project compile guard rows, benchmark dashboard data, PGO bench setup, fixture metadata drift, or real-project blockers such as utility-types, ts-toolbelt, ts-essentials, rxjs, type-fest, zod, kysely, Next, or Vite.
---

# TSZ Project Bench

Use for project-corpus rows, benchmark harness, fixture metadata, PGO bench
setup, and website benchmark data.

## Rules

- Read `AGENTS.md` and `docs/plan/ROADMAP.md` for durable benchmark/project
  work.
- Inspect overlapping PRs/issues.
- Red row = correctness blocker before speed problem unless failure is runtime,
  residency, timeout, or OOM.
- Keep fixture metadata shared across `project-fixtures.sh`,
  `bench-vs-tsgo.sh`, and `project-compile-guard.sh`.
- Wrap long/heavy local commands with `scripts/safe-run.sh`.

## Sources

- `scripts/bench/project-fixtures.sh`
- `scripts/bench/bench-vs-tsgo.sh`
- `scripts/ci/project-compile-guard.sh`
- `crates/tsz-website/src/_data/benchmark_data.js`
- `.github/workflows/bench.yml`

## Workflow

1. Identify row, phase, and first failure: prepare, `tsc` validation, tsz check,
   emit scope, crash, timeout, OOM, metadata, website ingestion, timing.
2. Required/canary/all set: `TSZ_PROJECT_COMPILE_SET=<required|canary|all>`.
3. Reproduce with the narrowest filter.
4. If typecheck fails, reduce to owning compiler operation before optimizing.
5. For speed work, name repeated operation and complexity change. No
   fixture-name special cases.

## Commands

```bash
./scripts/bench/bench-vs-tsgo.sh --prepare-only
./scripts/bench/bench-vs-tsgo.sh --quick --filter "<row>" --json-file /tmp/bench.json
TSZ_PROJECT_COMPILE_FILTER="<row>" TSZ_PROJECT_COMPILE_SET=required scripts/ci/project-compile-guard.sh
TSZ_PROJECT_COMPILE_ALLOW_FAILURES=1 TSZ_PROJECT_COMPILE_SET=all scripts/ci/project-compile-guard.sh
```

PR notes: goal (`green`, `fast`, or `grow`), row, before/after failure class,
first broken phase, owner layer, fixture metadata changes, whether timing is
meaningful, commands/CI links.
