# fix(checker): align contextual function object property intersection fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-contextual-function-object-property-intersection-fingerprint`
- **PR**: #2967
- **Status**: ready
- **Workstream**: 1 (Conformance — fingerprint-only `contextualTypeFunctionObjectPropertyIntersection.ts`)

## Intent

Realign the `TS2353` target display for
`TypeScript/tests/cases/compiler/contextualTypeFunctionObjectPropertyIntersection.ts`.
The checker already emits the correct diagnostic codes and positions, but the
excess-property message kept the optional `undefined | ...` contextual wrapper
and displayed a broad callback parameter type.

The tsc display strips the optional wrapper, materializes the remapped mapped
member for `FOO`, and keeps the wildcard callback parameter as the full event
union:

```ts
{ FOO?: Action<{ type: "FOO"; }> | undefined; }
  & { "*"?: Action<{ type: "FOO"; } | { type: "bar"; }> | undefined; }
```

## Files Touched

- `crates/tsz-checker/src/error_reporter/core/type_display.rs`
- `crates/tsz-checker/src/error_reporter/core/excess_display.rs`
- `crates/tsz-checker/tests/mapped_type_errors_conformance_tests.rs`

## Verification

- `cargo fmt --check` — pass.
- `cargo check --package tsz-checker` — pass.
- `cargo check --package tsz-solver` — pass.
- `cargo nextest run -p tsz-checker --test mapped_type_errors_conformance_tests` — pass.
- `cargo nextest run --package tsz-checker --lib architecture_contract_tests_src::test_checker_file_size_ceiling` — pass.
- `cargo nextest run --package tsz-checker --lib` — pass.
- `./scripts/conformance/conformance.sh run --filter "contextualTypeFunctionObjectPropertyIntersection" --verbose` — pass.
- `./scripts/conformance/conformance.sh run --max 200` — pass.
