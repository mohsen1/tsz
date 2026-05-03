# fix(checker): suppress nested-default TS2322 cascade through optional-parent destructuring

- **Date**: 2026-05-03
- **Branch**: `fix/destructuring-suppress-nested-default-cascade`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance — diagnostic-count cascade suppression for destructuring patterns)

## Intent

When a binding pattern destructures through an *optional* property without
supplying a parent default, accessing nested properties produces TS2339 /
TS2532 because the parent value may be `undefined`. tsc reports only the
upstream property-access error and **suppresses cascading default-value
TS2322s** inside the nested pattern.

```ts
function test({
    method = "z",
    nested: { p = "c" }              // ← `nested?` is optional, no parent default
}: {
    method?: "x" | "y",
    nested?: { p: "a" | "b" }
}) {}

// tsc:
//   line 5:5  TS2322 'z' → '"x" | "y"'
//   line 6:15 TS2339 .p does not exist on '{ p: ... } | undefined'
//   (no TS2322 for `p = "c"` — cascade suppressed)
//
// tsz before:
//   line 5:5  TS2322 'z' → '"x" | "y"'
//   line 6:15 TS2322 'c' → '"a" | "b"'   ← extra cascade
//   line 6:15 TS2339 .p does not exist on '{ p: ... } | undefined'
//
// tsz after: matches tsc (cascade dropped).
```

The fix is in `check_binding_element_with_request`
(`crates/tsz-checker/src/types/type_checking/core.rs`). When recursing
into a nested binding pattern, the existing logic strips `| undefined`
from `nested_type` only when the binding element supplies its own
default. After the strip, if the `nested_type` still includes
`undefined` (i.e. parent had no default), pass
`check_default_assignability = false` to the recursion — the upstream
TS2339 is the meaningful error; secondary default mismatches are noise.

## Files Touched

- `crates/tsz-checker/src/types/type_checking/core.rs` (+18 / -1) —
  compute `nested_check_default_assignability` and pass it to the
  recursive `check_binding_pattern_with_request`.
- `crates/tsz-checker/tests/destructuring_default_target_narrow_tests.rs`
  (+109 / 0) — three new unit tests pinning the structural rules.

## Verification

- `cargo nextest run -p tsz-checker --test destructuring_default_target_narrow_tests` — 4/4 pass (1 existing + 3 new).
- `cargo nextest run -p tsz-checker -E 'test(destructur)'` — 135 destructuring tests pass.
- `./scripts/conformance/conformance.sh run --filter "destructuringParameterDeclaration8"` — extra TS2322 at line 5:15 (`p = "c"` cascade) is **gone**. Test stays fingerprint-only because of the separate `| undefined` target-display issue (tracked in open PR #2426); once #2426 lands, this test should flip to PASS.
- Full conformance: identical net delta (-1 / 0 improvements / 1 regression — `arrowExpressionBodyJSDoc.ts`) with and without the patch on the same `main` HEAD, confirming the fix is conformance-neutral and the single regression is **pre-existing snapshot drift**.

## Notes

The three new unit tests use two distinct property/literal name choices
(`method`/`nested`/`p` and `flag`/`config`/`mode`) for the suppression
case, plus an inverse-direction lock (when the parent supplies `= {}`,
the inner default IS checked). This keeps the rule structural per
CLAUDE.md §25 review checklist item 3.
