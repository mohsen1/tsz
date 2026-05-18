---
name: tsz-architecture
description: Preserve TSZ architecture boundaries while changing checker, solver, binder, emitter, LSP, WASM, or CLI code. Use when planning semantic fixes, moving logic across query boundaries, handling architecture guard failures, ratcheting boundary debt, or reviewing changes for solver-first and no hardcoding rules.
---

# TSZ Architecture

Use this skill when a change touches ownership boundaries. The default direction
is solver-first semantics, thin checker orchestration, syntax-only parser,
symbol/flow-only binder, and output-only emitter.

## Ground Rules

- Read `AGENTS.md` and `docs/plan/ROADMAP.md` before conformance, emit,
  performance, architecture, LSP/WASM, Sound Mode, or DRY cleanup work.
- Inspect open PRs/issues before starting overlapping architecture work.
- State the structural rule before editing:
  `When <structural condition>, tsc does X; tsz should do X through <owner>.`
- Do not add decisions based on fixture paths, user-chosen names, source text
  snippets, rendered type strings, or a single test name.
- Prefer one shared query or boundary helper over local checker branches.

## Ownership Map

- Scanner: tokenization and string interning.
- Parser: syntax-only AST.
- Binder: symbols, scopes, hoisting, and flow graph; no type computation.
- Solver: relations, evaluation, inference, instantiation, operations,
  narrowing, and semantic caches.
- Checker: AST walk, contextual orchestration, diagnostics, and source spans.
- Emitter: JS/DTS output and transform scheduling; no semantic validation.
- LSP/WASM/CLI: consumers of checker/solver/project APIs, not owners of type
  algorithms.

## Boundary Workflow

1. Classify the change as `WHAT` or `WHERE`.
   - `WHAT`: type semantics belong in solver or a solver-backed boundary.
   - `WHERE`: diagnostics and source locations belong in checker.
2. Search for an existing boundary before adding one:

   ```bash
   rg "struct .*Request|RelationRequest|query_boundaries|TypeData" crates/tsz-checker/src crates/tsz-solver/src
   ```

3. Put new checker-facing semantic facts under a narrow
   `crates/tsz-checker/src/query_boundaries/` module when the checker needs an
   orchestration adapter.
4. Keep raw solver internals crate-private where possible. Do not introduce new
   checker `TypeKey` matching, direct `CompatChecker` construction, direct raw
   interning, or type-shape recursion when a query can own it.
5. Add focused unit tests in the owning crate and adjacent cases that vary names
   and wrappers.

## Guard Commands

Run targeted architecture checks when the touched area can trip them:

```bash
python3 scripts/arch/arch_guard.py
scripts/arch/check-checker-boundaries.sh
python3 scripts/arch/test_arch_guard.py
```

Use `cargo nextest run` for Rust tests; avoid `cargo test`. Wrap long,
multi-worker, or memory-heavy commands with `scripts/safe-run.sh`.

## Common Failure Classes

- Checker reaches into solver internals instead of a query boundary.
- Checker computes a semantic type fact locally.
- Emitter patches already emitted output to encode semantic policy.
- LSP or WASM reimplements checker/solver behavior.
- A ratchet is updated to allow new debt without a matching owner or removal
  condition.
- A diagnostic fix depends on formatted type text instead of structural facts.

## PR Notes

Architecture-sensitive PRs should include:

- structural rule,
- owner layer and boundary chosen,
- adjacent-case matrix,
- guard/test commands,
- known temporary debt and removal condition, if any,
- `AgentName`.
