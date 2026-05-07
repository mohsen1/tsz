# fix(server): emit UTF-16 navtree-full spans

- **Date**: 2026-05-07
- **Branch**: `fix/server-navtree-utf16-spans`
- **PR**: TBD
- **Status**: claim
- **Workstream**: server protocol parity (issue #3912)

## Intent

`navtree-full` `TextSpan` numbers were Rust byte offsets, computed via
`LineMap::position_to_offset` and `source_text.len()`. tsserver returns
UTF-16 code-unit positions. Any non-ASCII char before a navigation
item shifted byte offsets but not UTF-16 offsets, so a file like
`const s = "é"; function f() {}` came out with `nameSpan.start: 25`
where tsserver returns 24.

Both byte-offset paths in `handle_navtree` are converted to UTF-16:

- `range_to_text_span` walks `source_text` once accumulating
  `ch.len_utf16()` until `(line, character)` matches the requested
  position, then sums end−start for the length.
- The root span's full length sums `len_utf16()` for every char
  instead of returning `source_text.len()`.

## Files Touched

- `crates/tsz-cli/src/bin/tsz_server/handlers_info.rs` — add
  `position_to_utf16_offset`, swap both byte-offset call sites in
  `handle_navtree`.
- `crates/tsz-cli/src/bin/tsz_server/tests.rs` — regression test
  asserting the issue's exact `é` source produces UTF-16 spans.

## Verification

- `cargo nextest run -p tsz-cli -E 'test(navtree) | test(navbar) | test(navigation)'` — 33/33 pass (1 new regression test)
- `cargo check -p tsz-cli` — clean
