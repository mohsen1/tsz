---
name: rust-debugger
description: Break on a repro and read tsz's runtime state — a TypeId, a diagnostic's operand, checker/solver control flow, a variable's value — with the rdbg debugger, instead of adding tracing/println and rebuilding. Use when a checker or solver result is wrong and you would otherwise instrument-and-rebuild, when you need to see what type/flags reach a decision site, when a test panics or asserts, or to pinpoint which branch emits a diagnostic. Complements tsz-tracing (no code edits, no rebuild between looks).
---

# rust-debugger (rdbg)

Reach for this before the instrument-and-rebuild loop. tsz builds are slow; a
single `rdbg` session breaks on a repro, then reads real values as many times as
you like — no source edits, no rebuild between looks. This is the sanctioned way
to inspect runtime state (AGENTS.md bans `dbg!`/`println!` instrumentation).

Requires `rdbg` on PATH: `curl -fsSL https://azimi.me/rust-debugger-skill/install.sh | sh`
(also needs `rust-analyzer` and `lldb-dap`, both already present in this repo's
toolchain). Build with debug info (the default `cargo build`).

## The loop

```bash
# 1. a minimal repro
printf 'for (const x of 42) {}\n' > /tmp/r.ts

# 2. find where to break (rust-analyzer, no grep)
rdbg where emit_ts2488_not_iterable

# 3. build once and stop there (--cargo handles the .target dir)
rdbg launch --cargo . --bin tsz \
  --break crates/tsz-checker/src/checkers/iterable_checker.rs:1077 \
  -- /tmp/r.ts --noEmit --strict

# 4. read state — repeat freely, no rebuild
rdbg vars            # locals: TypeId { 0: 9 }, literal_display_type: None, …
rdbg eval expr_type  # a variable path
rdbg bt              # who called this emit
rdbg step over       # walk the branch
rdbg continue
```

## Debugging a failing test

```bash
rdbg launch --cargo . --test iterability_error_literal_display_tests \
  --break crates/tsz-checker/src/checkers/iterable_checker.rs:1737 \
  -- for_of_number_literal_keeps_literal
rdbg break --panic          # stop where an assert/panic fires; bt to your frame
```

## What it's good for in tsz

- **A wrong type or flag at a decision site.** Break at the check, `vars`/`eval`
  the `TypeId`, `is_*` flags, and options reaching it — instead of guessing and
  rebuilding.
- **Which branch emitted a diagnostic.** Break in the emitter, `bt` to see the
  caller, read the operand type actually passed.
- **A panic or assertion.** `--panic` (or `--test`), then `bt` + `up` to your
  frame with its arguments.
- **Checker → solver flow.** `step in`/`out` across the boundary, `where`/`def`
  to jump to the definition of what you're standing on.

## Notes

- `eval`/`set`/breakpoint conditions take variable **paths** and simple
  primitive comparisons, not arbitrary Rust expressions (`codelldb` on PATH lifts
  this). Interned handles show as e.g. `TypeId { 0: 9 }`.
- One paused process per checkout; `rdbg stop` ends a session, `rdbg down`
  releases the daemon (also auto-stops after 30 min idle).
- Still use `tsz-tracing` (`TSZ_LOG`) for flow across a whole run; use `rdbg`
  when you want to stop at one point and inspect exact values.
