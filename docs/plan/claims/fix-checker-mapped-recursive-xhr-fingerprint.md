# fix(checker): align mapped recursive XMLHttpRequest fingerprint

- **Date**: 2026-05-05
- **Branch**: `fix/checker-mapped-recursive-xhr-fingerprint`
- **PR**: #2771
- **Status**: ready
- **Workstream**: conformance / fingerprint parity

## Intent

Random conformance pick selected
`TypeScript/tests/cases/compiler/mappedTypeRecursiveInference.ts`.
The test is fingerprint-only: `tsc` and `tsz` both emit `TS2345`, but the
displayed target for `Deep<XMLHttpRequest>` orders and expands properties
differently. This PR will root-cause the display/inference surface needed to
match `tsc` without hardcoding the conformance file.

Observed verbose mismatch on `origin/main`:

- Missing fingerprint: `Deep<{ onreadystatechange: unknown; readonly readyState: { toString: ...; ... }; readonly response: unknown; readonly responseText: { toString: ...; ... 39 more ...; [Symbol.iterator]: ...; }; ... 29 more ...; dispatchEvent: unknown; }>`
- Extra fingerprint: `Deep<{ dispatchEvent: unknown; onerror: unknown; addEventListener: unknown; onload: unknown; readonly status: unknown; open: unknown; onabort: unknown; removeEventListener: unknown; responseType: unknown; readonly responseURL: unknown; ... 23 more ...; readonly readyState: unknown; }>`

## Outcome

`mappedTypeRecursiveInference.ts` now matches the expected `TS2345`
fingerprint. Recursive reverse mapped inference materializes apparent primitive
member objects for `Deep<number>` / `Deep<string>` sources, keeps nullable
callback properties uninformative as `unknown`, and preserves declaration order
for nested reverse-mapped objects. Diagnostic formatting now truncates apparent
string member lists like `tsc`, keeping `toString` plus the symbol tail.

## Files Touched

- `crates/tsz-solver/src/operations/constraints/reverse_mapped.rs`
- `crates/tsz-solver/src/diagnostics/format/compound.rs`
- `crates/tsz-checker/tests/generic_call_inference_tests.rs`
- `crates/tsz-checker/tests/reverse_mapped_inference_tests.rs`

## Verification

- `cargo fmt --all --check`
- `cargo nextest run -p tsz-checker recursive_homomorphic_mapped_materializes_primitive_apparent_members recursive_homomorphic_mapped_with_nullable_property_lets_outer_check_reject_null recursive_homomorphic_mapped_against_self_referential_interface_no_unknown_property recursive_homomorphic_mapped_against_index_signature_interface_no_unknown_property`
- pre-commit hook through clippy, wasm rustc warnings, architecture guardrails, and affected-crate tests (rerun after updating the changed reverse-mapped regression)
- `./scripts/conformance/conformance.sh run --filter "mappedTypeRecursiveInference" --verbose` → `2/2 passed (100.0%)`, `Fingerprint-only: 0`
