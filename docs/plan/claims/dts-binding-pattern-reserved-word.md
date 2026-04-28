# codex/dts-binding-pattern-reserved-word

Date: 2026-04-28
Branch: `codex/dts-binding-pattern-reserved-word`
PR: TBD
Status: verified locally; publish pending GitHub auth/DNS availability

## Workstream

Workstream 2: declaration emit parity.

## Intent

Fix the `declarationEmitBindingPatternWithReservedWord` DTS failure where a function-valued exported const with source type parameters printed the generic constraint through solver-expanded structure (`{ [x: string]: never }`) instead of preserving the source alias (`LocaleData`).

## Files Touched

- `crates/tsz-emitter/src/declaration_emitter/helpers/variable_decl.rs`
- `crates/tsz-emitter/src/declaration_emitter/tests/comprehensive_parity.rs`

## Verification

- `cargo nextest run -p tsz-emitter declaration_emitter::tests::comprehensive_parity::test_arrow_initializer_generic_constraint_preserves_alias` passed.
- `CARGO_TARGET_DIR=/private/tmp/tsz-dts-binding-pattern-reserved-word/.target ./scripts/emit/run.sh --filter=declarationEmitBindingPatternWithReservedWord --dts-only --verbose --json-out=/tmp/dts-binding-pattern-reserved-word.rebased-direct.json` passed.
- `./scripts/emit/run.sh --filter=declarationEmitBindingPattern --dts-only --json-out=/tmp/dts-binding-pattern-suite.after.json` still has two pre-existing DTS failures: `declarationEmitBindingPatterns` and `declarationEmitBindingPatternsUnused`.
