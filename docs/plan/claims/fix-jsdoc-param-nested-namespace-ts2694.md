# fix-jsdoc-param-nested-namespace-ts2694

- **Status**: ready
- **Branch**: `fix/conformance-next-20260505-1`
- **Claimed**: 2026-05-05T04:59:33Z

## Scope

Fix the conformance failure `prototypePropertyAssignmentMergeWithInterfaceMethod.ts` by validating JSDoc `@param` nested namespace member references such as `lf.schema.Table`, while preserving existing JS prototype assignment TS2708 suppression. The same pass also validates simple `@return` type names so `!IThenable` keeps its TS2552 fingerprint.

## Verification

- `cargo test -p tsz-checker --lib jsdoc_param_nested_namespace_missing_member_emits_ts2694 -- --nocapture`
- `cargo test -p tsz-checker --lib js_expando_assignment_to_type_only_namespace_member_does_not_emit_ts2708 -- --nocapture`
- `cargo test -p tsz-checker --lib architecture_contract_tests_src::test_checker_file_size_ceiling -- --nocapture`
- `./scripts/conformance/conformance.sh run --filter "prototypePropertyAssignmentMergeWithInterfaceMethod" --verbose`
