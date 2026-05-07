# scanner: U+2028/U+2029 line separators terminate `//` comments and shift line maps

- **Date**: 2026-05-07 21:40:00
- **Branch**: `claude/nice-darwin-q1oyq`
- **PR**: TBD
- **Status**: ready
- **Workstream**: scanner / common / position parity (issue #3331)

## Intent

`tsz` did not recognize U+2028 (LINE SEPARATOR) and U+2029 (PARAGRAPH
SEPARATOR) as line terminators inside `//` comments or in the LSP
`LineMap` used to render diagnostic line/column. tsc treats both as line
breaks, so `// @ts-expect-error const x = 1;` correctly terminates
the directive and starts a new logical line. Without the fix, `tsz` was
swallowing the next source line into the comment and reporting later
errors on the wrong line.

## Files Touched

- `crates/tsz-scanner/src/scanner_impl.rs` — single-line and multi-line
  comment scanners now break on U+2028/U+2029 via the existing
  `is_line_break` helper (was hard-coded to LF/CR).
- `crates/tsz-common/src/position/mod.rs` — `LineMap::build` and
  `position_to_offset` now treat U+2028/U+2029 as line boundaries.
- `crates/tsz-common/tests/position_tests.rs` — adds matrix tests for
  U+2028 / U+2029 / mixed terminators.
- `crates/tsz-scanner/src/scanner_impl.rs` (tests) — adds three scanner
  tests that lock single-line comment termination at U+2028 and U+2029.

## Verification

- `cargo test -p tsz-scanner --lib --tests` (95 tests pass)
- `cargo test -p tsz-common --lib --tests` (457 tests pass)
- `cargo test -p tsz-parser --lib` (819 tests pass)
- Reproductions A and B from issue #3331 now match `tsc 6.0.3`:
  - `// @ts-expect-error const ok = ...` → TS2578 + TS2322 on the
    correct line.
  - `// @ts-check let x = 1; x = "s"; ...` → both TS2322 are
    reported at columns matching tsc.
- Pre-existing failures in `tsz-checker` (3) and `tsz-cli` (4) reproduce
  on `main` without these changes; not introduced by this PR.
