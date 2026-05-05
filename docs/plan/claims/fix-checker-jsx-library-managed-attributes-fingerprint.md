# fix(checker): align JSX LibraryManagedAttributes fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-jsx-library-managed-attributes-fingerprint`
- **PR**: #3132
- **Status**: draft-progress
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the remaining fingerprint-only TS2322 drift in
`TypeScript/tests/cases/conformance/jsx/tsxLibraryManagedAttributes.tsx`.
The random pick shows matching error codes and positions, but `tsz` still
prints less tsc-like target types for JSX `LibraryManagedAttributes` paths:
expanded `ReactNode` aliases, indexed-access prop-type aliases, and
`Defaultize` application arguments differ from `tsc`. This follows the prior
mapped-infer PR that left the parent fixture fingerprint-only with unrelated
display drift.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/extraction.rs`
- `crates/tsz-checker/src/checkers/jsx/props/resolution.rs`
- `crates/tsz-checker/src/checkers/jsx/props/validation.rs`
- `crates/tsz-solver/src/diagnostics/format/mod.rs`
- `crates/tsz-solver/src/evaluation/evaluate_rules/index_access.rs`

## Current Result

Partial fingerprint progress only. The target still fails fingerprint-only:

- Fixed concrete `PropTypeChecker<number, false>[typeof checkedType]` display
  to `number` in JSX attribute mismatch output.
- Fixed the `string & PropTypeChecker<string, false>[typeof checkedType]`
  display drift for specified-generic `foo` attributes.
- Fixed the first `Defaultize<InferredPropTypes<...>, { ...; }>` drift where
  display previously retained an empty `{}` intersection member.
- Remaining drift is alias-preservation for expanded `ReactNode`, `FooProps`
  inside `Defaultize`, and several whole-object `Defaultize` displays.

## Verification

- `cargo fmt --all --check`
- `cargo nextest run -p tsz-checker jsx_library_managed_attributes --hide-progress-bar`
- `./scripts/conformance/conformance.sh run --filter "tsxLibraryManagedAttributes" --verbose` (still fingerprint-only; improved missing/extra set)
