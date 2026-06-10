---
name: tsz-architecture
description: Preserve TSZ architecture boundaries while changing checker, solver, binder, emitter, LSP, WASM, or CLI code. Use when planning semantic fixes, moving logic across query boundaries, handling architecture guard failures, ratcheting boundary debt, or reviewing changes for solver-first and no hardcoding rules.
---

# TSZ Architecture

Use for ownership-boundary work. Default direction: solver-first semantics, thin
checker, syntax parser, symbol/flow binder, output-only emitter.

## Rules

- Read `AGENTS.md` and `docs/plan/ROADMAP.md` for conformance, emit,
  performance, architecture, LSP/WASM, Sound Mode, or DRY cleanup work.
- Inspect overlapping PRs/issues.
- State: `When <structural condition>, tsc does X; tsz does X through <owner>.`
- No fixture paths, user names, source snippets, rendered strings, or single
  test names as decision inputs.
- Prefer one query/boundary helper over local checker branches.

## Owners

- Scanner: tokens and interning.
- Parser: syntax-only AST.
- Binder: symbols, scopes, hoisting, flow graph.
- Solver: relations, evaluation, inference, instantiation, operations,
  narrowing, semantic caches.
- Checker: orchestration, diagnostics, source spans.
- Emitter: JS/DTS output and transform scheduling.
- LSP/WASM/CLI: API consumers.

## Boundary Workflow

1. Classify as `WHAT` (solver/query boundary) or `WHERE` (checker location/
   diagnostic orchestration).
2. Search existing boundaries:
   `rg "struct .*Request|RelationRequest|query_boundaries|TypeData" crates/tsz-checker/src crates/tsz-solver/src`
3. Put checker-facing semantic adapters under
   `crates/tsz-checker/src/query_boundaries/`.
4. Avoid new checker `TypeKey` matching, direct `CompatChecker`, raw interning,
   or type-shape recursion.
5. Add owning-crate tests and adjacent cases varying names/wrappers.

## Guards

```bash
python3 scripts/arch/arch_guard.py
scripts/arch/check-checker-boundaries.sh
python3 scripts/arch/test_arch_guard.py
```

Use targeted `cargo nextest run`; wrap heavy runs with `scripts/safe-run.sh`.

PR notes: structural rule, owner layer, adjacent matrix, guard/tests, temporary
debt/removal condition.
