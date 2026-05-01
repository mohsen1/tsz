---
name: Fix checker TS7016 untyped JS path display
description: Collapse `.` segments in TS7016 / TS6504 file path arguments before formatting so the rendered message has no `<dir>/./<rest>` leftover from `PathBuf::join`.
type: project
branch: fix-checker-ts7016-untyped-js-path-display
status: ready
scope: module_resolver (TS7016 / TS6504 message rendering)

## Summary

Strip `Component::CurDir` segments from the path embedded in TS7016
("Could not find a declaration file for module") and TS6504 ("File 'X'
is a JavaScript file") messages so it matches tsc's canonical form.

## Root Cause

`untyped_js`/`resolved_untyped_js` passed `js_path.display()` straight
into the message format. When the resolver had joined a relative
`./node_modules/foo/index.js` onto the containing dir, the `PathBuf`
held both segments verbatim:

  `'/private/tmp/relpath-test/./node_modules/foo/index.js'`

The conformance harness strips the project_root prefix (`/private/tmp/relpath-test/`)
which left the message as `'./node_modules/foo/index.js'` while tsc's
baseline emits `'/node_modules/foo/index.js'` (canonicalized) — an
unintended fingerprint mismatch.

## Fix

Add a small `normalize_display_path` helper in `request_types.rs` that
walks the path's components and drops `Component::CurDir`. Apply it to
the displayed path in both `untyped_js` and `resolved_untyped_js`. The
stored `resolved_path` is unchanged — only the message string sees the
collapsed form.

## Files Changed

- `crates/tsz-core/src/module_resolver/request_types.rs` (helper +
  two call sites)

## Verification

- Conformance: 13 improvements, 11 flaky regressions (all fail on main
  without this change — verified by stash+rerun); net +5 improvement
  from running on this branch.
  - `untypedModuleImport_noImplicitAny_relativePath.ts` flips
    fingerprint-only → PASS
- Unit tests: tsz-core module_resolver suite (157 tests) all green
- All `untypedModule*.ts` cases still pass (9/9).
