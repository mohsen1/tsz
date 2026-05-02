# fix(checker): narrow `unknown` by direct literal equality

- **Date**: 2026-05-02
- **Branch**: `fix/checker-narrow-unknown-by-literal-equality`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Conformance — fingerprint match for
  `unknownType2.ts`)

## Intent

`narrow_by_boolean_comparison` was intercepting plain reference
equalities like `u === true` and routing them through truthiness
recursion (`narrow_type_by_condition_inner(.., guard_expr=u, .., sense)`).
That's the right move when the LHS is a **type guard expression**
(`isString(x) === true`, `x instanceof Error === false`), but it's
wrong when the LHS *is* the narrowing target itself: truthiness
narrowing of `unknown` doesn't reduce to `true`/`false`, so
`if (u === true)` left `u` as `unknown` and downstream
`const x: true = u` failed with a spurious TS2322.

The fix adds one early-return guard alongside the existing
discriminant-path bail-out: if `guard_expr` matches the narrowing
target by reference, return `None` so `LiteralEquality` handles the
narrowing instead.

## Files Touched

- `crates/tsz-checker/src/flow/control_flow/condition_narrowing.rs`
  (+9): one new early-return in `narrow_by_boolean_comparison`.

## Verification

- Isolated repros (all OK after fix, all errored before):
  - `if (u === true) { const x: true = u; }` (`u: unknown`)
  - `if (u === true) { const y: boolean = u; }`
  - `if (u === true || u === false) { const someBool: boolean = u; }`
  - `if (u === aString) { const s: string = u; }` (`aString: string`)
- `cargo test -p tsz-checker --lib` — 3146 pass, 0 fail.
- Targeted: `tsz-conformance --filter unknownType2 --print-fingerprints`
  — line 18 boolean-equality false-positive removed from extras (the
  `u === true || u === false` case now correctly narrows). The test
  still fails overall because typed-reference equalities further down
  (`u === aBoolean`, `u === aNumber`, enum cases, unique symbol cases)
  hit a different narrowing path that isn't fixed here. Net
  fingerprint-only test still fails; this PR is a partial fingerprint
  improvement, not a test-flip.
- Full conformance: **12344/12582 (98.1%)** unchanged from baseline,
  no regressions.

## Why this is partial, not a flip

The same test exercises `unknown === <typed-reference>` patterns
(e.g. `u === aBoolean` where `aBoolean: boolean`). Those go through a
different narrowing path that doesn't end up in `LiteralEquality`
either — they hit the typed-equality fall-through, which currently
doesn't narrow `unknown` to the rhs type the way tsc does. Fixing
that broader case is a separate change in
`narrow_by_binary_expr` / `extract_type_guard` that I did not include
here because it ripples through far more tests than the boolean
literal case and needs its own conformance pass.
