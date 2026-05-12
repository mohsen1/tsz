# [WIP] fix(checker): report TS2852 for await using outside async contexts

- **Date**: 2026-05-12
- **Branch**: `fix-checker-await-using-ts2852-20260512`
- **Issue**: #5967
- **Status**: claim
- **Workstream**: 1 (diagnostic parity for an existing checker bug)

## Intent

Make `await using` declarations match `tsc` by reporting TS2852 when they
appear inside a non-async function, while preserving valid top-level module and
async function usage.

## Planned Scope

- Checker or grammar validation path for `await using` declarations.
- Focused diagnostic tests covering invalid non-async function usage and valid
  async/top-level module usage.

## Verification Plan

- Focused checker/CLI test for #5967.
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-checker ...` or the relevant crate
  test target once the owning test suite is identified.
- Manual comparison against `tsc` and `tsz` for the issue repro.
