# perf(checker): reuse cached source-file comments in is_symbol_used_in_jsdoc

- **Date**: 2026-05-08
- **Branch**: `perf/checker-jsdoc-use-cached-comments`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T3.4 (small-fixture polish — comment lookup)

## Intent

`is_symbol_used_in_jsdoc` at
`crates/tsz-checker/src/types/type_checking/unused.rs:1731` called
`tsz_common::comments::get_comment_ranges(text)` on the **entire source
text** for every symbol the unused-checker considered. The parser already
populates `SourceFileData.comments` once at parse time
(`crates/tsz-parser/src/parser/node.rs:962`,
`crates/tsz-parser/src/parser/state_statements.rs:81`) using the same
helper, and other call-sites already use it
(`crates/tsz-checker/src/types/type_checking/declarations.rs:1055,1073`).

Replace the per-symbol rescan with `&sf.comments`. O(file_size) per
symbol → O(comments) per symbol; identical data.

## Files Touched

- `crates/tsz-checker/src/types/type_checking/unused.rs` (-2/+1 LOC)

## Verification

- `cargo check -p tsz-checker` (clean)
- `cargo nextest run -p tsz-checker --lib -E 'test(/jsdoc|unused|TS6133|TS6196/)'`
  (399 passed, 3374 skipped)
- Behavior is byte-identical: `sf.comments` is populated by the same
  `get_comment_ranges` helper at parse time on the same source text.
