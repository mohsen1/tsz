# fix(emitter): stop erased declarations and ambient module recovery from leaking type-only text

- **Date**: 2026-05-05
- **Branch**: `claude/fix-erased-declarations-leak-bZ1qk`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 2 (emit pass rate)

## Intent

Closes #3411. Two related emitter bugs leak TypeScript-only source text into
JS output:

1. Recovered interface members (e.g. `return (value: string): void;`) parse
   as a sibling top-level statement of the interface and get emitted into JS.
2. Ambient `declare module "outer" { ... }` triggers a recovery scan over the
   raw source range, which mistakes comment text like `// module \`fake\` {`
   for an unnamed template module and writes `declare; module ; {}` into JS.

The fix is a structural rule, not a name-/text-pattern hack: erased
declarations must not contribute source text or comment text to JS, and the
ambient-module recovery scanners must not run inside an ambient (`declare`)
module's body or treat comment-stripped windows as live source.

## Files Touched

- `crates/tsz-emitter/src/emitter/declarations/namespace.rs` — guard the
  template-module and anonymous-module recovery scanners against being
  invoked for ambient modules and against scanning across comment text.
- `crates/tsz-emitter/src/emitter/statements/core.rs` — when a recovered
  sibling statement is itself trailing material parsed inside an erased
  declaration's range, drop it instead of emitting raw TS text.
- `crates/tsz-emitter/src/emitter/helpers.rs` — small helper additions if
  needed to classify "inside an erased range" without re-walking the AST.

## Verification

- `cargo nextest run -p tsz-emitter`
- Targeted reproductions for both repros from #3411 produce no leaked
  TypeScript text in JS output.
