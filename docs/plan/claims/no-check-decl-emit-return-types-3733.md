---
status: WIP
issue: 3733
agent: claude (auto-loop)
started: 2026-05-08 01:42:18 UTC
---

# `--noCheck --declaration` drops inferred return types (#3733)

## Problem
`tsz --declaration --emitDeclarationOnly --noCheck a.ts` emits
`function f(n: number);` (no return type). tsc emits
`function f(n: number): number;` because declaration emit needs the
checker's inferred type information regardless of `--noCheck`.

The CLI's `options.no_check` short-circuit returned early without
running the checker, leaving `type_caches` empty for the declaration
emit pass; the emitter then fell back to a bare `DeclarationEmitter`
that lacks inference data.

## Fix
Two CLI changes in `crates/tsz-cli/src/driver/check.rs`:

1. Gate the `if options.no_check { … return … }` short-circuit on
   `!options.emit_declarations`, so `--noCheck --declaration` falls
   through to the regular pipeline that runs the checker and produces
   `type_caches`.
2. In `check_file_for_parallel`, also run `check_source_file` when
   `no_check && emit_declarations`, but discard the resulting checker
   diagnostics so `--noCheck` still suppresses type errors.

The regular pipeline's existing emit machinery already consumes the
populated `type_caches` and prints inferred return types.

## Test plan
- [x] Manual repro from #3733: `function f(n: number): number;` now
  emits.
- [x] Negative manual repro: `export const x: string = 1;` under
  `--noCheck --declaration` no longer surfaces TS2322.
- [x] `cargo nextest run -p tsz-cli --lib -E 'test(no_check_with_declaration_emit_still_suppresses_type_errors)'` passes.
- [x] Existing `no_check_collect_diagnostics_keeps_parse_errors_and_skips_type_errors` still passes (no regression on the parse-only path).
- [x] Existing `no_check_path_emits_isolated_declarations_ts9007` (#3709) still passes.
