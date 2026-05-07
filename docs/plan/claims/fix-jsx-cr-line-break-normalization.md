# fix(emitter): normalize CR-only line breaks in JSX text

- **Date**: 2026-05-07
- **Branch**: `fix/jsx-cr-line-break-normalization`
- **PR**: TBD
- **Status**: claim
- **Workstream**: emitter parity (issue #3903)

## Intent

JSX text with a CR-only line break (e.g. `<div>a\rb</div>`) was emitted
unchanged by `process_jsx_text` because the multiline path only fired on
`'\n'`. tsc treats `\r\n`, `\n`, and bare `\r` as line terminators (see
`isLineBreak` in `compiler/scanner.ts`) and coalesces them through the
same trim+join pipeline. This PR normalizes CR/CRLF to LF before the
existing split-and-trim logic runs, fixing the case in issue #3903.

## Files Touched

- `crates/tsz-emitter/src/emitter/jsx/mod.rs` (~12 LOC change + 4 unit tests)

## Verification

- `cargo nextest run -p tsz-emitter -E 'test(process_jsx_text)'` — 4 new tests pass
- `cargo nextest run -p tsz-emitter -E 'test(jsx)'` — 38/38 jsx tests pass
- Manual repro from issue #3903: `tsz -p tsconfig.json` now emits
  `React.createElement("div", null, "a b")` instead of `"a\rb"`.
