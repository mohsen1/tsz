# fix(checker): align infer conditional type fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/checker-infer-types-fingerprints`
- **PR**: #3586
- **Status**: ready
- **Workstream**: 1 (conformance)

## Intent

Claiming `TypeScript/tests/cases/conformance/types/conditional/inferTypes1.ts`.

Current `origin/main` reports the expected TS1338, TS2304, TS2322, and TS2344
codes, but the diagnostic fingerprints are missing two TS2344 entries for
conditional `infer` constraint violations.

## Verification

- `CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-checker --lib ts2344_function_type_arg_with_extra_required_param_fails_single_param_constraint ts2344_single_constrained_infer_fails_incompatible_true_branch_constraint`
- `./scripts/conformance/conformance.sh run --filter "inferTypes1" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `git diff --check`
- `scripts/architecture-check.sh --quick`
- `cargo clippy -p tsz-checker --lib -- -D warnings`
