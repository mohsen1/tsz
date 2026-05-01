---
name: Fix checker strip js ext from exports-namespace display
description: Strip `.js` from `typeof import("…")` display when the receiver is the current file's `exports` namespace (preserve_js_extension=false), matching tsc.
type: project
branch: fix-checker-strip-js-ext-exports-namespace-display
status: ready
scope: checker (TS7053 / TS2339 / commonjs namespace display)

## Summary

Fix fingerprint mismatch where TS7053 / related diagnostics inside a `.js`
file's commonjs body printed `typeof import("foo.js")` instead of
`typeof import("foo")` for accesses through the implicit `exports` binding.

## Root Cause

`current_file_commonjs_module_name(preserve_js_extension=false)` looked up
`current_file_explicit_js_module_specifier()` (a specifier like
`./foo.js` recorded by the importer) and returned its basename verbatim
— `foo.js` — without stripping the JS extension. The branch was reached
for `exports[X]`/`exports.X` access (which sets `preserve_js_extension=
false`), so the rendered receiver kept the extension.

## Fix

After taking the basename of the explicit specifier, run it through
`tsz_common::file_extensions::strip_known_extension` so the JS extension
is dropped when `preserve_js_extension == false`. Variant 2
(`module.exports[X]`, `preserve_js_extension == true`) skips this branch
entirely and still preserves `.js`, matching tsc.

## Files Changed

- `crates/tsz-checker/src/state/type_analysis/computed_commonjs/exports_detection.rs`

## Verification

- Conformance: 12 improvements, 6 flaky regressions (all fail on main without
  the fix — verified by stash+rerun), net +6 (12298 → 12304).
  - `lateBoundAssignmentDeclarationSupport1.ts` now passes
- Unit tests: tsz-checker suite all green
- Fingerprint cover: variant 2 (`module.exports[X]`) still preserves `.js`
  intra-file (matches tsc baseline) — both variants pass 7/7 in the salsa
  late-bound suite.
