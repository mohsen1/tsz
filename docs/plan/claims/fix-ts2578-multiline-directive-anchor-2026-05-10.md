# fix(cli): anchor multiline TS2578 at directive line

- **Date**: 2026-05-10
- **Branch**: `fix/ts2578-multiline-directive-anchor-2026-05-10`
- **PR**: #4965
- **Status**: ready
- **Workstream**: diagnostic-conformance

## Intent

Unused `@ts-expect-error` diagnostics already anchored at the comment range
for same-line comments, but multiline block comments still used the block
opener. TypeScript anchors unused directives inside multiline comments at
the directive line, which affected both regular block comments and JSX
comment trivia.

This change keeps suppression behavior unchanged while separating the
comment span from the unused-directive diagnostic span.

## Files Touched

- `crates/tsz-cli/src/driver/check_utils.rs`

## Verification

- `cargo fmt --all --check`
- `cargo test -p tsz-cli unused_expect_error -- --nocapture`
- `cargo test -p tsz-cli ts_directive_scan_handles_cr -- --nocapture`
- `cargo build --profile dist-fast -p tsz-cli --bin tsz -p tsz-conformance`
- `tsz-conformance --filter multiline` (1/1 passed)
- `tsz-conformance --filter ts-expect-error` (4/4 passed)
