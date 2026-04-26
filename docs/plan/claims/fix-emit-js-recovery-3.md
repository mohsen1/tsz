# fix(parser): preserve duplicate `extends` clauses for emit parity

- **Date**: 2026-04-26
- **Time**: 2026-04-26 15:57:36
- **Branch**: `fix/emit-js-recovery-3`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 2 (Emit pass rate)

## Intent

Fix duplicate-`extends`-clause emit parity for the JS suite. Two TypeScript
conformance baselines drive this:

- `extendsClauseAlreadySeen.ts` (`class D extends C extends C { baz() {} }`)
- `extendsClauseAlreadySeen2.ts` (same shape)

tsc reports TS1172 for the duplicate `extends` keyword but still preserves
both heritage clauses in the AST so the JS emitter prints
`class D extends C extends C { ... }` verbatim. Our parser was discarding
the duplicate clause via `skip_heritage_type_references_for_recovery`,
yielding `class D extends C { ... }` and a 2-test deficit.

The duplicate `implements` case is unaffected: implements clauses never
appear in JS output, so the existing recovery is correct.

## Root Cause

`crates/tsz-parser/src/parser/state_statements_class.rs` ::
`parse_heritage_clause_extends` returned `None` for the duplicate clause
after reporting TS1172, throwing away the parsed type-references. The
emitter (which loops every heritage clause and prints ` extends T1, T2`)
never saw the duplicate.

## Fix

Drop the early `return None` for the duplicate `extends` clause. The
function now parses the type-reference(s) and creates a normal
`HeritageClause` AST node for the duplicate, exactly like the first
clause. Suppress the secondary `Classes can only extend a single class.`
TS1174 diagnostic for the duplicate clause path so we do not double up
errors with the TS1172 we already emit.

Existing checker iterators that consume heritage clauses already loop
with `for &clause in heritage_clauses.nodes` and either `break`/`return`
on the first `ExtendsKeyword` clause they find, so the second clause is
ignored by the type system but visible to the emitter.

The implements duplicate-keyword path keeps the existing
`skip_heritage_type_references_for_recovery` recovery — the JS emitter
strips implements anyway, so there is no parity gap there.

## Files Touched

- `crates/tsz-parser/src/parser/state_statements_class.rs`
  (drop early return for duplicate `extends`; gate TS1174 on
  `!is_duplicate`)
- `crates/tsz-parser/tests/parser_unit_tests.rs`
  (`class_duplicate_extends_recovery_*` updated to lock the new
  two-clause shape; the implements-side test is unchanged)
- `crates/tsz-emitter/tests/duplicate_extends_recovery_tests.rs` (new
  integration test, 3 cases — duplicate same base, duplicate distinct
  base, implements duplicate stays stripped)
- `crates/tsz-emitter/Cargo.toml` (register the new
  `duplicate_extends_recovery_tests` test target)

## Verification

- `cargo nextest run -p tsz-parser` (673/673 pass — including the
  rewritten `class_duplicate_extends_recovery_*` lock-in test)
- `cargo nextest run -p tsz-checker --lib` (2888/2888 pass — no checker
  regression from the new duplicate clauses)
- `cargo nextest run -p tsz-emitter` (1659/1659 pass — including the 3
  new `duplicate_extends_recovery_tests` cases)
- `bash scripts/emit/run.sh --filter=extendsClauseAlreadySeen --js-only`
  flips both `extendsClauseAlreadySeen` and `extendsClauseAlreadySeen2`
  from `+1/-1 lines` to PASS.
- `bash scripts/emit/run.sh --filter=parserClassDeclaration --js-only`
  (27/27 pass — broader heritage-clause regression check).

Net JS-emit movement: +2 tests (84.0 → 84.2 % at the time of writing,
within sampling noise on the full suite).
