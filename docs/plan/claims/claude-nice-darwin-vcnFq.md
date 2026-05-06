# fix(emitter): preserve escaped `\${` in template literal type d.ts emit

- **Date**: 2026-05-06
- **Branch**: `claude/nice-darwin-vcnFq`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 2 (Declaration Emit)

## Intent

Closes #3412. The declaration emitter writes template-literal-type spans
from the parser's *cooked* literal text, but the scanner already collapsed
the source's `\${` to `${`. As a result, an escaped `${` was re-emitted as
a real template substitution, producing invalid d.ts (`tsc` reports
TS1110 on the round trip). Re-escape `$` to `\$` in
`escape_template_literal_text` whenever the next character is `{`, so
literal `${` in cooked text is preserved in the .d.ts output. Bare `$`
(not followed by `{`) stays unescaped to match `tsc`.

## Files Touched

- `crates/tsz-emitter/src/declaration_emitter/type_emission.rs` (+9/−5 LOC)
- `crates/tsz-emitter/src/declaration_emitter/tests/fix_verification.rs`
  (+33 LOC regression test)

## Verification

- `CARGO_TARGET_DIR=/tmp/tsz-3412-target cargo test -p tsz-emitter
  fix_template_literal` (8/8 pass, including new
  `fix_template_literal_escaped_dollar_brace_preserved`)
- `CARGO_TARGET_DIR=/tmp/tsz-3412-target cargo test -p tsz-emitter --lib`
  (1921 pass, 5 ignored)
- `CARGO_TARGET_DIR=/tmp/tsz-3412-target cargo check --workspace`
- `cargo fmt --package tsz-emitter -- --check`
- `git diff --check`
