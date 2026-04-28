# Detect Git merge conflict markers in JSX child content

- **Date**: 2026-04-28
- **Branch**: `fix/scanner-jsx-detect-conflict-marker`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

`crates/tsz-scanner/src/scanner_impl.rs:scan_jsx_token` did not check for
Git merge conflict markers (`<<<<<<<`, `|||||||`, `=======`, `>>>>>>>`)
when scanning JSX child content. The regular-mode handler already runs
`is_conflict_marker_trivia()` first for `<`/`=`/`>`/`|` (lines 585–608,
705–714, 773–800, etc.), but the JSX path went straight to angle-bracket
tokenization and treated the leading `<` of `<<<<<<< HEAD` as the start
of a nested JSX tag.

Fix: add the same `is_conflict_marker_trivia()` early check at the top
of `scan_jsx_token`. When detected, scan the marker via
`scan_conflict_marker_trivia()` (which emits TS1185) and return a
`ConflictMarkerTrivia` token (or recurse under skip-trivia mode).

## Files Touched

- `crates/tsz-scanner/src/scanner_impl.rs` (~14 LOC added in `scan_jsx_token`)

## Verification

- `cargo nextest run -p tsz-scanner` → 354 pass.
- `./scripts/conformance/conformance.sh run --filter "conflictMarkerTrivia3"`:
  pre-fix `missing=[TS1185], extra=[TS1003,TS1139,TS17008]`. Post-fix
  `missing=[TS1005], extra=[TS17008,TS1005]` — TS1185 now emits correctly,
  three spurious diagnostics (TS1003, TS1139, one TS1005) are eliminated.
  Test does not flip to PASS yet (parser-side TS17008 over-emit and TS1005
  position drift are separate issues), but the scanner-level mismatch is
  resolved.
- `./scripts/conformance/conformance.sh run --filter "jsx"`: 267/298 pass,
  unchanged vs main (no regressions).
