# [WIP] fix(checker): align control-flow function-like diagnostics

- **Date**: 2026-05-03
- **Branch**: `fix/control-flow-function-like-fingerprint-05031915`
- **PR**: #2612
- **Status**: implemented
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Investigate and fix the fingerprint-only mismatch in
`TypeScript/tests/cases/compiler/controlFlowForFunctionLike1.ts`.
`tsc` and `tsz` agree on TS2345, but `tsz` anchors or renders an extra
fingerprint for `test.ts:22:12`:

```text
Argument of type 'string' is not assignable to parameter of type 'number'.
```

## Files Touched

- `crates/tsz-checker/src/state/type_analysis/core_type_query.rs`
- `crates/tsz-checker/src/state/type_analysis/core.rs`
- `crates/tsz-checker/src/types/mod.rs`
- `crates/tsz-checker/src/types/type_literal_checker.rs`
- `crates/tsz-checker/src/types/type_node_advanced.rs`
- `crates/tsz-checker/src/types/type_node_helpers.rs`
- `crates/tsz-checker/tests/conformance_issues/errors/private_members.rs`
- `crates/tsz-checker/tests/typeof_function_like_flow_tests.rs`
- `crates/tsz-checker/Cargo.toml`

## Verification

- `scripts/session/quick-pick.sh --run` selected and reproduced
  `TypeScript/tests/cases/compiler/controlFlowForFunctionLike1.ts`
  as a fingerprint-only failure with matching `[TS2345]`.
- `cargo nextest run -p tsz-checker --test typeof_function_like_flow_tests`
  passed.
- `cargo check -p tsz-checker` passed.
- `cargo build --profile dist-fast --bin tsz` passed.
- `./scripts/conformance/conformance.sh run --filter "controlFlowForFunctionLike1" --verbose`
  passed 1/1 with no fingerprint-only mismatch.
