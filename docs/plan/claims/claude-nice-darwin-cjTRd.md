# fix(emitter): preserve string-literal enum initializers in ES5 emit

- **Date**: 2026-05-06
- **Branch**: `claude/nice-darwin-cjTRd`
- **PR**: TBD
- **Status**: ready
- **Workstream**: emitter parity

## Intent

`EnumES5Emitter::emit_enum` constructed `IRPrinter::new()`, which has no
arena and no source text. The transformer marks string-literal enum
initializers as `IRNode::ASTRef` to preserve quote style; without the
arena/source the printer fell through to the placeholder `undefined`,
producing `E[E["A"] = undefined.length] = "A";` instead of tsc's
`E[E["A"] = "".length] = "A";`. Switch to `IRPrinter::with_arena_and_source`
(or `with_arena` when the emitter has no source text) so ASTRef nodes
resolve against the original text.

Closes #4165.

## Files Touched

- `crates/tsz-emitter/src/transforms/enum_es5.rs` — pass arena (and
  source text when present) into the `IRPrinter` used by
  `EnumES5Emitter::emit_enum`.
- `crates/tsz-emitter/tests/enum_es5.rs` — add two regression tests
  (double- and single-quoted variants) plus a `with_source` helper that
  mirrors the production `transform_dispatch` setup.

## Verification

- `cargo test -p tsz-emitter --lib` (1989 lib tests pass)
- `cargo test -p tsz-emitter` (full crate suite green)
- `cargo clippy -p tsz-emitter --lib --tests -- -D warnings` (clean)
- Manual repro from the issue: `tsz` now emits `"".length` (matches tsc)
  instead of `undefined.length`.
