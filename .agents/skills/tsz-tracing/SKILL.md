---
name: tsz-tracing
description: Debug tsz compiler issues using the built-in tracing infrastructure. Use when investigating type inference bugs, conformance failures, or understanding runtime behavior. Provides hierarchical trace output of solver, checker, and binder operations.
---

# TSZ Tracing

Use tracing before adding debug prints. It is for conformance failures,
inference/narrowing/assignability surprises, and runtime path discovery.

## Commands

```bash
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -p tsz-cli -- file.ts
TSZ_LOG=debug TSZ_LOG_FORMAT=json cargo run -p tsz-cli -- file.ts
TSZ_LOG="tsz_solver=trace,tsz_checker=debug" TSZ_LOG_FORMAT=tree cargo run -p tsz-cli -- file.ts
TSZ_LOG="tsz_solver::narrowing=trace" TSZ_LOG_FORMAT=tree cargo run -p tsz-cli -- file.ts
```

Use `RUST_LOG` only as fallback. `TSZ_LOG_FORMAT` values: `tree`, `json`,
`text`.

## Workflow

1. Reduce to the smallest `.ts` witness.
2. Compare expected behavior with `npx tsc --noEmit file.ts`.
3. Start broad (`debug`, tree), then narrow by module.
4. Capture large traces with `2> trace.log`, `head`, `tail`, `rg`, or
   `grep "type_id=<id>"`.
5. Use structured trace fields and spans; never add ad-hoc stdout/stderr
   debugging.

## Useful Filters

- Narrowing: `TSZ_LOG="tsz_solver::narrowing=trace"`.
- Relations/inference: `TSZ_LOG="tsz_solver=debug"`.
- Checker lookup/cache: `TSZ_LOG="tsz_checker=debug"`.
- Query events: `TSZ_LOG=tsz::query_json=trace TSZ_LOG_FORMAT=json`.

## Read Output

Tree output nests spans, shows elapsed ms, level, module path, event, and
fields. Look for the first structural divergence: wrong narrowed member,
unexpected relation result, missing lazy ref resolution, cache hit/miss, or
diagnostic source-span decision.

Tracing goes to stderr and is initialized only when `TSZ_LOG`/`RUST_LOG` is set.
For trace examples and quick reminders, read `QUICKREF.md`.
