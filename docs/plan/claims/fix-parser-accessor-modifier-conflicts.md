# fix(parser): emit TS1243 for accessor + readonly/declare modifier conflicts

- **Date**: 2026-05-02
- **Branch**: `fix/parser-accessor-modifier-conflicts`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Conformance — fingerprint match for
  `autoAccessorDisallowedModifiers.ts`)

## Intent

Auto-accessor properties cannot legally combine with `readonly` or
`declare` in any order. tsc emits TS1243 (`'X' modifier cannot be used
with 'Y' modifier`) on whichever keyword came second; tsz previously
emitted TS1029 (`must precede`) for `accessor readonly` and nothing
for the other three orderings, suggesting the combination was just
mis-ordered.

This change in
`crates/tsz-parser/src/parser/state_statements_class_members.rs`:

- `accessor` keyword: when `readonly` or `declare` was already seen,
  emit TS1243 on the accessor token (was: nothing).
- `readonly` keyword: when `accessor` was already seen, emit TS1243
  (was: TS1029 must-precede).
- `declare` keyword: when `accessor` was already seen, emit TS1243
  (was: nothing).

## Files Touched

- `crates/tsz-parser/src/parser/state_statements_class_members.rs`
  (+39/-3): three modifier branches updated.

## Verification

- `cargo test -p tsz-parser --lib` — 741 pass, 0 fail.
- `cargo test -p tsz-checker --lib` — 3146 pass, 0 fail.
- Targeted: `tsz-conformance --filter autoAccessorDisallowedModifiers
  --print-fingerprints` — 4 missing TS1243 fingerprints now emitted, 1
  spurious TS1029 removed:

  ```
  Before:
    missing: TS1243 ×4, TS1070 ×1, TS1275 ×17, TS1276 ×1
    extra:   TS1029 ×1
  After:
    missing: TS1070 ×1, TS1275 ×17, TS1276 ×1
    extra:   (none)
  ```

  The test still fails overall because the remaining missing
  fingerprints (TS1070 type-member-rejection, TS1275
  only-on-property-declaration, TS1276 cannot-be-optional) are larger
  semantic checks not covered by this PR.

- Full conformance: **12344/12582 (98.1%)** — unchanged from baseline.
  This PR is a fingerprint-match improvement, not a test-flip; it
  reduces the diff on `autoAccessorDisallowedModifiers.ts` so a later
  PR adding the remaining TS1070/TS1275/TS1276 emissions can flip the
  test cleanly.
- Pre-commit hook full sweep on the affected 7 crates (parser,
  binder, solver, checker, lowering, emitter, lsp): 21400 tests, all
  green.
