# fix(solver,checker): emit TS2344 for JSDoc @extends incompat property type

- **Date**: 2026-04-26
- **Branch**: `fix/checker-jsdoc-extends-incompat-prop-ts2344`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

`conformance/jsdoc/extendsTag5.ts` was wrong-code: tsc emits TS2344 when a JSDoc
`@extends {A<{...}>}` type-argument violates the target's `@template {Foo} T`
constraint via an incompatible property type (e.g. `b: string` vs constraint
`b: boolean | string[]`). tsz missed TS2344 entirely and instead emitted
spurious TS2322/TS2409 on the constructor body.

## Root Cause

Two stacked bugs:

1. **Solver — String/iterable shortcut leak**: `is_boxed_primitive_subtype`
   (`crates/tsz-solver/src/relations/subtype/rules/intrinsics.rs`) had a
   String/iterable shortcut that returned true when the target was iterable
   yielding string AND `target_has_non_iterable_properties` reported "no
   extras". The helper only inspected `ObjectShape` / `ObjectWithIndex`, so it
   missed `TypeData::Array(T)` (no lib loaded) and `Application(Array, [T])`
   (lib loaded). Net: `string <: string[]` returned true, which dragged
   `string <: boolean | string[]` to true, and the JSDoc constraint check
   silently passed.
2. **Checker — Constraint display lost typedef alias**: TS2344 message
   formatted the constraint by structurally expanding the resolved typedef,
   producing `'{ a: ...; b: ...; }'` instead of tsc's `'Foo'`.

## Fix

- **Solver**: `target_has_non_iterable_properties` now returns `true` when
  the target (raw, readonly-unwrapped, or `evaluate_type`-resolved) is
  `TypeData::Array` or `TypeData::Tuple` — catching all three shapes
  `Array<T>` can take depending on lib state. Helper now takes `&mut self`
  to allow `evaluate_type`.
- **Checker**: `check_jsdoc_extends_tag_type_argument_constraints` now uses
  the original `constraint_expr` text as the display name when the source
  is a single identifier (no `<`, `|`, `&`, `(`, `[`, `{`, `?`), falling
  back to `format_type_diagnostic` only for structural constraints.

## Files Touched

- `crates/tsz-solver/src/relations/subtype/rules/intrinsics.rs`
- `crates/tsz-solver/tests/subtype_cache_tests.rs` (1 new lock)
- `crates/tsz-checker/src/classes/class_implements_checker/jsdoc_heritage.rs`
- `crates/tsz-checker/tests/jsdoc_extends_constraint_tests.rs` (4 new locks)

## Verification

- `cargo nextest run -p tsz-solver subtype_cache_tests` — 31/31 pass
- `cargo nextest run -p tsz-solver --tests` — 5522/5522 pass
- `cargo nextest run -p tsz-checker --test jsdoc_extends_constraint_tests`
  — 7/7 pass (3 pre-existing + 4 new)
- `cargo nextest run -p tsz-checker --lib` — 2918/2918 pass
- `./scripts/conformance/conformance.sh run --filter "extendsTag5" --verbose`:
  before: missing TS2344 ×2 + extra TS2322/TS2409 (wrong-code)
  after: TS2344 ×2 emit at correct positions with `'Foo'` display; only
  spurious TS2322/TS2409 from the constructor body remain (separate bug,
  tracked in memory).

## Remaining Gap (Separate)

`extendsTag5.ts` still fails as fingerprint-only on the constructor-body
`return a` pattern: `class A` (no own props) with `constructor(a: T)
{ return a }` produces tsz TS2322 + TS2409 ("Type 'T' is not assignable to
type 'A<T>'"). tsc accepts because A's instance type has no required
members. Saved as a separate memory item for follow-up.
