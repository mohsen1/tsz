# fix(checker, emitter): support defaulted JSDoc @template [T=Default]

- **Date**: 2026-05-07
- **Branch**: `fix/jsdoc-bracket-default-template-param`
- **PR**: TBD
- **Status**: claim
- **Workstream**: JSDoc parity (issue #4005)

## Intent

`@template [T=string]` declares a type parameter `T` with default
`string`. tsc accepts this form across both checking and declaration
emit. tsz dropped the bracket form on both surfaces:

1. **Checker** (`crates/tsz-checker/src/jsdoc/params.rs`): the identifier
   scanner saw the `[` byte as a non-identifier char and skipped the
   segment, leaving `T` unbound and producing spurious TS2304 at every
   reference.
2. **Emitter** (`crates/tsz-emitter/src/declaration_emitter/helpers/jsdoc.rs`):
   `parse_jsdoc_template_params` split on commas/whitespace and emitted
   each segment verbatim, so `[T=string]` ended up between `<` and `>`,
   producing invalid `.d.ts` output (`<[T=string]>` instead of
   `<T = string>`).

## Files Touched

- `crates/tsz-checker/src/jsdoc/params.rs` — unwrap leading `[` in
  identifier scan.
- `crates/tsz-checker/src/types/utilities/tests/jsdoc_params_tests.rs`
  — three new unit tests for the bracket-default form.
- `crates/tsz-emitter/src/declaration_emitter/helpers/jsdoc.rs` — add
  `normalize_jsdoc_template_bracket_default` to rewrite `[T=Default]`
  as TypeScript-shaped `T = Default` for declaration emit.

## Verification

- `cargo nextest run -p tsz-checker --lib -E 'test(jsdoc_template_bracket_default)'` — 3/3 new tests pass
- `cargo nextest run -p tsz-checker -E 'test(jsdoc) | test(template)'` — 662/662 pass
- `cargo nextest run -p tsz-emitter -E 'test(jsdoc) | test(template) | test(declaration)'` — 1031/1031 pass
- Manual repro from #4005:
  - `--noEmit --checkJs`: now exits 0 with no TS2304 (was: false-positive TS2304)
  - `--declaration --emitDeclarationOnly`: emits `export function id<T = string>(x: T): T;` matching tsc 6.0.3 (was: `<[T=string]>`)
