# [WIP] fix(parser): align class reserved-word diagnostics

- **Date**: 2026-04-29
- **Branch**: `fix/parser-class-reserved-word-diagnostics`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the conformance mismatch for
`TypeScript/tests/cases/compiler/strictModeReservedWordInClassDeclaration.ts`.
The picked failure currently misses TS2702 and emits extra TS1139, TS2300, and
TS7051 diagnostics around strict-mode reserved words in class declarations.

## Files Touched

- `TBD` after diagnosis

## Verification

- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run --package <touched-crate> --lib`
- `./scripts/conformance/conformance.sh run --filter "strictModeReservedWordInClassDeclaration" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL`
