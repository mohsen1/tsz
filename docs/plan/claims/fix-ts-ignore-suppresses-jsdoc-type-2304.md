# fix(cli): make @ts-ignore suppress JSDoc @type TS2304 in declaration emit

- **Date**: 2026-05-07
- **Branch**: `fix/ts-ignore-suppresses-jsdoc-type-2304`
- **PR**: TBD
- **Status**: claim
- **Workstream**: declaration-emit / directive parity (issue #3996)

## Intent

`apply_ts_directive_suppression` had a carve-out that kept TS2304 /
TS2552 alive when the diagnostic line text contained `@type {` and the
declaration-emit + checked-JS flag set was active. Issue #3996 confirms
tsc 6.0.3 suppresses these diagnostics — the carve-out was an
incorrect alignment hack and is the kind of "regex over printer-style
hint" pattern §25 of CLAUDE.md forbids. Drop it; tsc parity restored.

## Files Touched

- `crates/tsz-cli/src/driver/check_utils.rs` — remove the
  `line_text.contains("@type {")` branch; keep the public signature
  with the now-unused flag so callers stay stable.
- One regression unit test pinning the new behavior.

## Verification

- `cargo nextest run -p tsz-cli --lib -E 'test(check_utils)'` — 62/62 pass
- `cargo nextest run -p tsz-cli -E 'test(checked_js) | test(check_js) | test(jsdoc.*type) | test(ts2304)'` — 35/35 pass
- Manual repro from #3996 exits 0 with no TS2304, matching tsc 6.0.3.
