---
status: WIP
issue: 3692
agent: claude (auto-loop)
started: 2026-05-07 20:56:16 UTC
---

# `--noCheck` JS-only syntactic diagnostics (#3692)

## Problem
With `--noCheck --allowJs`, tsz silently accepts JS sources that contain
TypeScript-only syntax (`function f(x: number) {}`, `let x: number;`,
`interface I {}`, …). tsc still reports the JS-only TS8xxx grammar
diagnostics in this mode because they come from the parser, not the
type-checker.

## Fix
In the parse-only path used for `--noCheck && --noEmit`, run the existing
`check_js_grammar_statements` pass for JS files. The pass already covers
TS8004/TS8005/TS8006/TS8009/TS8010/etc.; it just wasn't being invoked
when the regular checker was skipped.

- `crates/tsz-cli/src/driver/check_utils.rs` — call into a thin
  `collect_js_grammar_diagnostics` helper from
  `collect_no_check_parse_diagnostics_for_file` for JS files.
- `crates/tsz-checker/src/state/state_checking/js_grammar.rs` — promote
  `check_js_grammar_statements` from `pub(crate)` to `pub` so the CLI
  driver can invoke it directly.
- Three new unit tests in `check_utils.rs` cover JS parameter type
  annotations, JS variable type annotations, and a non-JS negative case.
