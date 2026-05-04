# fix(checker): preserve unresolved type names in interface property type displays

- **Date**: 2026-05-03
- **Branch**: `claude/brave-thompson-vff5r`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance — fingerprint parity (display)

## Intent

Conformance test `compiler/jsxCallElaborationCheckNoCrash1.tsx` was failing
fingerprint-only (right code, wrong message): tsz rendered the JSX
intrinsic-element type as `DetailedHTMLProps<HTMLAttributes<error>, error>`
where tsc shows `DetailedHTMLProps<HTMLAttributes<HTMLDivElement>,
HTMLDivElement>`. The `HTMLDivElement` reference comes from the React
intrinsic-elements interface in `react16.d.ts`; it is undeclared in the
test's lib chain, so the checker collapsed the type to `TypeId::ERROR`,
which the printer renders as the bare `error` token.

The fix routes the `type_literal_checker.rs` unresolved-name fallback
through `unresolved_type_name(name)` instead of `TypeId::ERROR`. The new
`UnresolvedTypeName(name)` variant is treated structurally as `Error`
everywhere (`visitor.rs`, `is_error_type`, etc.), so this is structurally
neutral — the only observable change is that the printer now renders the
original identifier in subsequent TS2322/TS2345 messages.

Scope is intentionally narrow: only the type-literal/interface-body path is
changed, because that is where react16's `IntrinsicElements` member type
declarations are lowered. The bare-variable-annotation fallback in
`reference_helpers.rs` is left alone — changing it caused regressions in
the JSX spread-child checker (`Error | T[]` collapses to `Error` in
`normalize_union`, while `UnresolvedTypeName | T[]` is simplified to
`T[]` and silently drops TS2609).

## Files Touched

- `crates/tsz-checker/src/types/type_literal_checker.rs` (~24 LOC change)
- `crates/tsz-checker/tests/missing_global_type_diagnostics_tests.rs`
  (+148 LOC, 2 new regression tests)

## Verification

- `cargo test --package tsz-checker --test missing_global_type_diagnostics_tests` (13/13 pass)
- `cargo test --package tsz-checker --lib` (3247/3247 pass)
- `cargo test --package tsz-solver --lib` (5593/5593 pass)
- `cargo fmt --all --check` (clean)
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` (clean)
- `./scripts/conformance/conformance.sh run --filter jsxCallElaborationCheckNoCrash1 --verbose` → `1/1 passed`
- `scripts/session/verify-all.sh --quick` (in progress)
