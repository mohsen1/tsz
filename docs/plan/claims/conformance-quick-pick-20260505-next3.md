# fix(checker): align intrinsic JSX ref callback diagnostic fingerprint

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next3`
- **PR**: #2753
- **Status**: claimed
- **Workstream**: 1 (Conformance fixes)

## Intent

This PR targets the fingerprint-only failure in
`TypeScript/tests/cases/conformance/jsx/tsxStatelessFunctionComponents2.tsx`.
Both `tsc` and `tsz` emit `TS2339`; the remaining gap is the exact diagnostic
fingerprint for the intrinsic `div` `ref` callback parameter access:

```tsx
let i = <div ref={x => x.propertyNotOnHtmlDivElement} />;
```

`tsc` reports that `propertyNotOnHtmlDivElement` does not exist on
`HTMLDivElement`. `tsz` currently emits the same diagnostic code set but misses
this fingerprint.

## Files Touched

- TBD after root-cause analysis.

## Verification

- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- Focused Rust unit tests for the owning crate
- `./scripts/conformance/conformance.sh run --filter "tsxStatelessFunctionComponents2" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL`
