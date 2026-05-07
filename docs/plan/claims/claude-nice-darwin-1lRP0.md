# fix(parser): import defer falls into import-equals recovery

- **Date**: 2026-05-06
- **Branch**: `claude/nice-darwin-1lRP0`
- **PR**: TBD
- **Status**: claim
- **Workstream**: parser parity (#4119)

## Intent

`tsz` currently routes `import defer from = require("m")` through the
import-equals lookahead because `look_ahead_is_import_equals` treats `defer`
the same as `type` when `from` follows. `defer` has no import-equals form in
TypeScript, so the lookahead must only return `true` for `TypeKeyword` in
that branch. After the fix, `import defer from = require("m")` is parsed as
an import declaration with `defer` as the modifier and `from` as the
default-import binding, matching `tsc`'s single TS1005 (`'from' expected`)
diagnostic at the `=`.

## Files Touched

- `crates/tsz-parser/src/parser/parse_rules/utils.rs`
  (narrow the `next2 == FromKeyword` and identifier-follow branches so they
  no longer route `defer` to import-equals)
- `crates/tsz-parser/src/parser/parse_rules/utils.rs` tests
  (add lookahead unit tests for `import defer from = require("m")` and the
  preserved `import type from = require("m")` case)

## Verification

- `cargo nextest run -p tsz-parser`
- targeted CLI repro for `import defer from = require("m")`
