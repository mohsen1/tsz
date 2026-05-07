# [WIP] fix(checker): align union property existence diagnostics

- **Date**: 2026-05-07
- **Branch**: `fix/conformance-next-20260507-044234`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

The canonical picker selected
`TypeScript/tests/cases/compiler/unionPropertyExistence.ts`. The current
snapshot reports a fingerprint-only mismatch with matching `TS2339` and
`TS2551` codes.

This slice will root-cause the remaining diagnostic fingerprint drift for
property existence checks across unions, preserving the checker/solver
boundary ownership described in the architecture docs.

## Planned Scope

- Property access diagnostics, receiver display, or property suggestion logic
  as the root cause requires.
- A focused Rust regression test in the owning crate.
- Targeted conformance verification for `unionPropertyExistence`.

## Verification Plan

- `cargo fmt --all`
- Focused Rust regression test(s)
- `cargo nextest run` for touched crates or focused test filters
- `./scripts/conformance/conformance.sh run --filter "unionPropertyExistence" --verbose`
- Architecture guardrails if checker boundary code changes
- Pre-commit hook before publishing ready PR
