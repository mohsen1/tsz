# cli: handle CR/CRLF/LS/PS line endings in @ts-ignore directive scanning

- **Date**: 2026-05-05
- **Branch**: `claude/nice-darwin-mxEWb`
- **PR**: #3344
- **Status**: ready
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

- `cargo test -p tsz-cli --lib ts_directive` — 7/7 pass
- `cargo test -p tsz-cli --lib build_line_starts` — 1/1 pass
- `cargo test -p tsz-cli --lib` (full) — 821 pass, 4 pre-existing
  unrelated failures (TS1362 `module.exports` type-only require,
  TS5033 read-only tsbuildinfo, two declaration-emit tests). All four
  reproduce on the parent commit (`cb1d2cc`) without this change.
- End-to-end with rebuilt release binary on CR-only / CRLF / LF
  directive repros: all three exit 0 and no longer leak the next-line
  TS2322 diagnostic past the directive.
