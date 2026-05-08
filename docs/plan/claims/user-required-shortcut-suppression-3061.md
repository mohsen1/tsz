---
status: WIP
issue: 3061
agent: claude (auto-loop)
started: 2026-05-08 03:20:05 UTC
---

# Local `Required<T>` aliases suppress TS2344 (#3061)

## Problem
`required_mapped_constraint_source` (in
`crates/tsz-checker/src/checkers/generic_checker/mapped_constraint_helpers.rs`)
shortcuts `T extends Required<Source>` to "is satisfied by `Source`",
keyed only on the symbol's escaped name. A user-defined
`type Required<T> = { marker: string };` therefore silently passes the
constraint check even though it has nothing to do with the lib's
mapped utility.

## Fix
Gate the shortcut on `self.ctx.symbol_is_from_lib(sym_id)`. The lib's
`Required<T>` continues to fast-path; user redeclarations fall through
to the regular constraint check, which correctly emits TS2344.

The existing user-`Required<T>`-style mapped tests
(`test_required_mapped_constraint_accepts_required_source_and_defaults`
and `test_required_mapped_constraint_still_rejects_missing_default_property`)
still pass: their bodies are the homomorphic mapped form
`{ [K in keyof T]-?: T[K] }`, which is caught by the existing
`mapped.optional_modifier == Some(MappedModifier::Remove)` branch
above.

## Files
- `crates/tsz-checker/src/checkers/generic_checker/mapped_constraint_helpers.rs` —
  add `symbol_is_from_lib` gate.
- `crates/tsz-checker/tests/generic_tests.rs` —
  `test_user_defined_required_with_unrelated_shape_does_not_skip_constraint`.
