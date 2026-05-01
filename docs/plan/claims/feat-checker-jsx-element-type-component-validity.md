# feat(checker): use JSX.ElementType as the component validity constraint when defined

- **Date**: 2026-05-01
- **Branch**: `feat/checker-jsx-element-type-component-validity`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance — feature implementation)

## Intent

When a user defines `JSX.ElementType` (TS 4.8+ feature for customising
the set of types valid as JSX components), tsc validates the
component's *type* directly against `JSX.ElementType` and skips the
older `JSX.Element`-return-type check entirely. This is the
authoritative path for React 18 / Server Component patterns where a
function may return `string | number | array | Promise` as long as
`JSX.ElementType` admits the return shape.

tsz historically only ran the older `JSX.Element` check, so it
emitted spurious TS2786 ("cannot be used as a JSX component") for
every function component returning anything other than a JSX element
— even when `JSX.ElementType` was explicitly defined to admit other
return shapes.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/extraction.rs`
  (extend `check_jsx_component_return_type`: try `JSX.ElementType`
  first; only fall through to the legacy `JSX.Element` check when
  `JSX.ElementType` is not defined).
- `crates/tsz-checker/src/tests/jsx_element_type_constraint_tests.rs`
  (2 locking unit tests: positive + name-renamed positive cover).
- `crates/tsz-checker/src/lib.rs` (test wiring).

## Verification

- Targeted: `compiler/jsxElementType.tsx` — TS2786 false-positive count
  reduced from ~12 to 0; remaining mismatches are unrelated target-display
  issues (`IntrinsicAttributes &` prefix) that are scoped to follow-up.
- `cargo nextest run -p tsz-checker -p tsz-solver --lib` → 8673/8673 pass.
- Smoke conformance: `--filter jsx --max 200` → 179/200 PASS, all 14
  failing tests are pre-existing (verified against snapshot detail JSON).
