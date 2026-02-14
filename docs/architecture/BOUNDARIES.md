# TSZ Boundary Contract

This document defines mandatory ownership boundaries for semantic work.

## WHAT vs WHERE

- `WHAT` (type algorithms): belongs in `tsz-solver`.
- `WHERE` (diagnostic span/orchestration): belongs in `tsz-checker`.

## Layer ownership

- `scanner`: lexing/tokenization and atom interning.
- `parser`: syntax-only AST construction.
- `binder`: symbols/scopes/flow graph; no semantic typing.
- `checker`: AST walk and diagnostics; no ad-hoc type algorithms.
- `solver`: relations/evaluation/inference/instantiation/narrowing.
- `emitter`: output transform/print; no semantic validation.

## Hard rules

- Checker must not construct semantic types with raw interner APIs.
- Checker must not depend on solver internals (`TypeKey` internals).
- Binder must not import solver for semantic decisions.
- Emitter must not import checker internals for semantic checks.

## Relation diagnostics

- TS2322/TS2345/TS2416-family diagnostics must route through checker relation gateways.
- Mismatch policy/suppression decisions must remain centralized.

## DefId and type environment

- Semantic refs are `Lazy(DefId)` in solver.
- Checker is responsible for DefId stabilization + env orchestration only.
- Traversal/discovery for type-shape dependencies must live in solver visitors.

## Cache ownership

- Checker cache scope:
  - `node -> TypeId`
  - `symbol -> TypeId`
  - flow/CFG caches and diagnostics
- Solver cache scope:
  - relation/evaluation/inference/instantiation memoization

## Enforcement

- Run `scripts/check-checker-boundaries.sh`.
- CI should fail on boundary violations.
- Architecture report artifact is generated at:
  - `artifacts/architecture/arch_guard_report.json`
