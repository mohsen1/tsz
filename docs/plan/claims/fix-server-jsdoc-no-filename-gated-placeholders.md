# fix(server): drop filename-gated inferFromUsage placeholders

- **Date**: 2026-05-07
- **Branch**: `fix/server-jsdoc-no-filename-gated-placeholders`
- **PR**: #4424
- **Status**: ready
- **Workstream**: server protocol parity (issue #3848)

## Intent

`getCodeFixes` had a production path that injected empty-changes
`inferFromUsage` actions whenever the request file path ended with one
of nine hardcoded conformance fixture filenames
(`annotateWithTypeFromJSDoc4.ts`, `15.ts`, `16.ts`, `19.ts`, `22.ts`,
`23.ts`, `24.ts`, `25.ts`, `26.ts`). The same source under any other
file name returned just the `Annotate with type from JSDoc` fix.

This is exactly the §25 anti-hardcoding pattern (filename list
driving a behavior decision), and tsserver does NOT emit those
placeholders. Removing the branch and its two helpers
(`should_emit_jsdoc_infer_placeholders`,
`estimate_jsdoc_infer_action_count`,
`estimate_jsdoc_infer_action_labels`) brings tsz-server in line with
tsserver and removes the file-name-vs-content protocol divergence.

## Files Touched

- `crates/tsz-cli/src/bin/tsz_server/handlers_code_fixes.rs` — delete
  the placeholder-injection branch and the unused
  `estimate_jsdoc_infer_action_labels` helper; drop the now-unused
  `request_start_line` local.
- `crates/tsz-cli/src/bin/tsz_server/handlers_code_fixes_jsdoc.rs` —
  delete `should_emit_jsdoc_infer_placeholders` and
  `estimate_jsdoc_infer_action_count`.
- `crates/tsz-cli/src/bin/tsz_server/handlers_code_fixes_tests.rs` —
  rewrite the previous lock-in test as a regression test that
  asserts the response contains the annotate fix and no
  `inferFromUsage` placeholders.

## Verification

- `cargo nextest run -p tsz-cli -E 'test(get_code_fixes_jsdoc_does_not_emit_filename_gated_placeholders)'` — passes
- `cargo nextest run -p tsz-cli -E 'test(get_code_fixes) | test(jsdoc) | test(annotate)'` — 56/56 pass
- `cargo check -p tsz-cli` — clean
