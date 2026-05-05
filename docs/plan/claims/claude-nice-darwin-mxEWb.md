# cli: handle CR/CRLF/LS/PS line endings in @ts-ignore directive scanning

- **Date**: 2026-05-05
- **Branch**: `claude/nice-darwin-mxEWb`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance — CLI directive parity

## Intent

Fix #3305: `@ts-ignore` and `@ts-expect-error` suppression in
`crates/tsz-cli/src/driver/check_utils.rs` only treats `\n` as a line
break, so files with CR-only or LS/PS line endings either suppress the
wrong line or treat the entire file as one comment. Match TS by
recognizing `\r`, `\n`, `\r\n`, ` `, and ` ` as line breaks in
both the line-start table and the single-line comment span scanner.

## Files Touched

- `crates/tsz-cli/src/driver/check_utils.rs`
- `crates/tsz-cli/src/driver/check_utils_tests.rs` (new tests, if a
  test module exists; otherwise inline tests in the same file)

## Verification

- `cargo nextest run -p tsz-cli`
- Targeted CR/CRLF repro: directive line successfully suppresses the
  next line.
