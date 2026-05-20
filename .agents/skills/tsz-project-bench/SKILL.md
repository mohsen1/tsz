---
name: tsz-project-bench
description: Work on TSZ benchmark and project-corpus compatibility. Use when investigating bench.yml failures, project compile guard rows, benchmark dashboard data, PGO bench setup, fixture metadata drift, or real-project blockers such as utility-types, ts-toolbelt, ts-essentials, rxjs, type-fest, zod, kysely, Next, or Vite.
---

# TSZ Project Bench

Use this skill for project-corpus and benchmark work. The benchmark dashboard is
only meaningful when compatibility metadata is trustworthy; a red project row is
a correctness blocker before it is a speed problem.

## Ground Rules

- Read `AGENTS.md` and `docs/plan/ROADMAP.md` before durable benchmark,
  performance, project-corpus, or website benchmark work.
- Inspect open PRs/issues for overlapping benchmark blockers.
- Do not treat timing as useful until the row is green or the failure is
  explicitly runtime, residency, timeout, or OOM.
- Keep fixture definitions shared between `scripts/bench/project-fixtures.sh`,
  `scripts/bench/bench-vs-tsgo.sh`, and
  `scripts/ci/project-compile-guard.sh`.
- Wrap long or memory-heavy local commands with `scripts/safe-run.sh`.

## Source Files

- `scripts/bench/project-fixtures.sh`: pinned repos, refs, and config writers.
- `scripts/bench/bench-vs-tsgo.sh`: benchmark runner, PGO, metadata emission.
- `scripts/ci/project-compile-guard.sh`: CI project compatibility smoke gate.
- `crates/tsz-website/src/_data/benchmark_data.js`: website row ingestion.
- `.github/workflows/bench.yml`: benchmark publication and daily latest runs.

## Debug Workflow

1. Identify the row, phase, and first failure class: prepare, tsc validation,
   tsz check, emit scope, crash, timeout, OOM, metadata, website ingestion, or
   timing.
2. Confirm whether the row is required or canary:
   `TSZ_PROJECT_COMPILE_SET=required`, `canary`, or `all`.
3. Reproduce with the narrowest project filter. Avoid full benchmark runs unless
   the task is specifically about benchmark harness behavior.
4. If the project fails to typecheck, reduce to the owning compiler operation
   before optimizing.
5. For speed work, identify the repeated operation and expected complexity
   change. Do not special-case fixture names.

## Useful Commands

Prepare benchmark dependencies without running measurements:

```bash
./scripts/bench/bench-vs-tsgo.sh --prepare-only
```

Run a narrow benchmark smoke:

```bash
./scripts/bench/bench-vs-tsgo.sh --quick --filter "utility-types" --json-file /tmp/bench.json
```

Run a filtered project compile guard after a dist-fast binary exists:

```bash
TSZ_PROJECT_COMPILE_FILTER="utility-types-project" \
TSZ_PROJECT_COMPILE_SET=required \
scripts/ci/project-compile-guard.sh
```

Continue through known failures only when collecting a matrix:

```bash
TSZ_PROJECT_COMPILE_ALLOW_FAILURES=1 \
TSZ_PROJECT_COMPILE_SET=all \
scripts/ci/project-compile-guard.sh
```

## PR Notes

Benchmark PRs should state:

- project row and before/after failure class,
- first broken phase and owner layer,
- fixture metadata changes, if any,
- whether timing is meaningful,
- targeted local commands or CI run links,
- `AgentName`.
