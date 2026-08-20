---
name: tsz-conformance
description: Triage TSZ diagnostic parity against the pinned TypeScript 7 oracle. Use for seed-capability regressions, focused conformance witnesses, fingerprint mismatches, or full-corpus observations during the clean-slate rewrite.
---

# TSZ Conformance

Conformance has two roles during the rewrite: declared seed capabilities are a
strict monotonic gate, while the unsupported full corpus is an honest
observation. Prefer checked-in artifacts and narrow filters; CI owns broad runs.

## Rules

- Read `AGENTS.md` and `docs/plan/ROADMAP.md` for conformance-affecting work.
- Do not run full conformance locally.
- Treat a failing test as a witness for a structural rule.
- Do not hide unsupported behavior or capability regressions with
  snapshot/allowlist churn.

## Offline First

```bash
python3 scripts/conformance/query-conformance.py --dashboard
python3 scripts/conformance/query-conformance.py --campaigns
python3 scripts/conformance/query-conformance.py --fingerprint-only
python3 scripts/conformance/query-conformance.py --code TS2322
python3 scripts/conformance/query-conformance.py --code TS2322 --paths-only
```

Artifacts: `conformance-detail.json`, `conformance-snapshot.json`,
`conformance-accepted-regressions.txt`, `conformance-shard-weights.json`.
The accepted-regression file describes the frozen legacy checkpoint; it is not
the rewrite's R0 floor and must not be refreshed to manufacture green results.

## Oracle

Semantics come from the **pinned** compiler, not from any other copy on the box.
Run manual spot-checks through the wrapper so they match what the gate scores:

```bash
scripts/conformance/oracle.sh case.ts --strict --lib es2022 --target es2022
node scripts/node_modules/typescript/lib/tsc.js --version   # Version 7.0.2
```

- **Use `oracle.sh`, not a bare `tsc.js` invocation.** It runs the pinned
  `typescript@7.0.2` with the same `--singleThreaded --stableTypeOrdering true`
  flags `generate-tsc-cache.rs` uses for TypeScript 7+. This is not just about
  ordering: `typescript@7.0.2` (typescript-go) reports a *different diagnostic
  set* under `--singleThreaded`. A position-invalid import in a bare `{ }` block
  (or `if`/loop/`try` body) gets `TS2307`/`TS2305` only single-threaded, not
  under the default concurrent scheduler; the same import in a function, class
  `static { }`, or namespace body gets neither, in both modes (#16413).
  `compare-to-parent.sh`/`conformance.sh` score the single-threaded cache, so a
  plain `tsc.js case.ts` silently disagrees with the gate — hand-oracling
  without the flag reads a fix as passing that the gate then fails (this
  mis-scoped #16409/#16411).
- `TypeScript/` (submodule) and any container-global
  `/opt/**/node_modules/typescript` are the **6.0** line. They are corpus and
  test *cases* only — never the source of a semantic rule.
- Reading a rule out of 6.0 source and pinning it with tests lands the wrong
  behaviour: #16215 encoded `ignoreDeprecations !== "6.0"` from a 6.0.2
  `typescript.js`, but 7.0 removed that grace window entirely (#16217).
- 7.0 traps when hand-oracling: **`--target es5` was removed** — it answers
  `error TS5108` and emits nothing else, and that line carries no `file.ts(l,c):`
  prefix, so a row filter drops it and every row reads clean. `strict` also
  defaults to true.

Check raw output once before trusting a filtered sweep: an invalid invocation and
a clean compile are indistinguishable after filtering.

## Focused Run

```bash
./scripts/conformance/conformance.sh run --filter "<name>" --verbose
```

Keep filters precise. Let harnesses rebuild stale binaries when possible.

## Triage

Classify: new/accepted/resolved/fingerprint-only/wrong-code/missing-code/
extra-code/crash/timeout/OOM. Then identify owner inside the replacement:
syntax recovery, program/config/resolution, binding, relation, inference,
narrowing, indexed/keyof/mapped/conditional/template semantics, diagnostic
display, or emit-only.

Before coding, state the structural rule and adjacent cases. Behavior changes
need tests at the public service boundary plus focused owner-module tests.

For a full-corpus observation, preserve every unsupported, crash, timeout, and
mismatch result in the artifact. When a semantic family graduates, add exact
public-boundary tests and establish its new monotonic floor; do not inherit or
edit the retired implementation's accepted-failure policy.
