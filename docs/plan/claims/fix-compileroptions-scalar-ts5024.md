# fix(config): emit TS5024 for scalar compilerOptions

- **Date**: 2026-05-07
- **Branch**: `fix/compileroptions-scalar-ts5024`
- **PR**: #4387
- **Status**: ready
- **Workstream**: config validation parity (issue #3882)

## Intent

A top-level scalar `compilerOptions` value (`{"compilerOptions":"bad",…}`)
bypassed every nested option validator and surfaced as a generic serde
`invalid type: string "bad", expected struct CompilerOptions` parse
failure. tsc emits TS5024 at column 20 with the standard
"requires a value of type object" message. This PR adds the same
top-level object-type validation that already exists for `include` /
`exclude` / `files` / `references`, then replaces the offending scalar
with an empty object so the rest of the config still deserializes.

## Files Touched

- `crates/tsz-core/src/config/mod.rs` — call `validate_top_level_object_option`
  for `compilerOptions` and add the helper itself; two unit tests.

## Verification

- `cargo nextest run -p tsz-core --lib -E 'test(config::tests)'` — 150/150 pass
- Manual repro from #3882 emits identically to tsc 6.0.3:
  `tsconfig.json(1,20): error TS5024: Compiler option 'compilerOptions' requires a value of type object.`
