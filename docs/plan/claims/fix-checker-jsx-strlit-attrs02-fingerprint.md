# fix(checker): include JSX spread props in TS2322 source-type display for excess-prop diagnostics

- **Date**: 2026-04-29
- **Branch**: `fix/checker-jsx-strlit-attrs02-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance and Fingerprints)

## Intent

`contextuallyTypedStringLiteralsInJsxAttributes02.tsx` is a fingerprint-only
failure: error codes (TS2322, TS2683, TS2769) match tsc, but the printed
source-type in two TS2322 diagnostics drops JSX spread properties.

For elements like `<NoOverload {...{onClick: (k) => ...}} extra />` (single
overload, has both a JSX spread attribute and an excess named attribute),
tsc anchors TS2322 at the excess named attribute (`extra`) with source
type `{ extra: true; onClick: (k: "left" | "right") => void; }`. tsz emits
at the same anchor but with source type `{ extra: true; }` — only the
excess attribute, dropping the spread props.

This PR fixes the per-attribute excess-property emission path in
`check_jsx_attributes_against_props` to construct the source-type display
from ALL provided JSX attributes (explicit + spread-derived), ordered
explicit-first (matching tsc's display).

Scope is intentionally narrow: only the source-type *display* in the
TS2322 first-line message changes. The diagnostic anchor, code, and chain
text remain unchanged.

The b4 anchor mismatch (`<MainButton goTo="home" extra />` — tsc anchors
TS2769 at the JSX tag name, tsz at the first attribute) is a separate root
cause in the JSX overload `jsx_overload_explicit_failure_attr` heuristic
and is out of scope for this PR.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/props/resolution.rs` — new helper
  `format_jsx_attrs_synthesized_source_for_excess` and call-site change
  in the per-attribute excess-property emission path (~50 LOC change).
- `crates/tsz-checker/tests/jsx_excess_attr_with_spread_source_display_tests.rs`
  — new unit-test lock for the synthesized source-type string.

## Verification

- `cargo nextest run -p tsz-checker -- jsx_excess_attr_with_spread`
- `./scripts/conformance/conformance.sh run --filter "contextuallyTypedStringLiteralsInJsxAttributes02" --verbose`
  — confirms two of three fingerprint mismatches resolved (c1 + d1); the
  remaining b4 anchor mismatch is the separate scope above.
- Targeted JSX conformance smoke:
  `./scripts/conformance/conformance.sh run --filter "contextuallyTyped.*Jsx"`
