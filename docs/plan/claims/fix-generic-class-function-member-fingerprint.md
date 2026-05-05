# [WIP] fix(checker): align generic class function member fingerprint

- **Date**: 2026-05-05
- **Branch**: `fix/generic-class-function-member-fingerprint`
- **PR**: #3128
- **Status**: claim
- **Workstream**: conformance / contextual signature instantiation

## Intent

Random conformance pick selected
`TypeScript/tests/cases/conformance/types/typeRelationships/typeInference/genericClassWithFunctionTypedMemberArguments.ts`.
The current divergence is fingerprint-only: both tsc and tsz report `TS2345`,
but tsz formats at least one diagnostic with a widened callback return type
where tsc preserves the literal inference target.

This claim takes over the stale investigation in
`docs/plan/claims/claude-brave-thompson-cj2vT.md`; there is no open PR for
that branch. The implementation will fix the root cause rather than a
display-only suppression and will add owning-crate regression coverage.

## Files Touched

- TBD after root-cause analysis.

## Verification

- Baseline target command:
  `./scripts/conformance/conformance.sh run --filter "genericClassWithFunctionTypedMemberArguments" --verbose`
- Planned: focused Rust regression test in the owning crate.
- Planned: targeted conformance rerun for `genericClassWithFunctionTypedMemberArguments`.
- Planned: `cargo nextest run -p tsz-checker -p tsz-solver`.
