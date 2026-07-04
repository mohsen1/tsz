---
name: rust-debugger
description: Debug a Rust program or failing test with rdbg — set breakpoints (line, function, conditional, hit-count, panic, or watchpoint), run and step, read locals as real Rust values, change a variable mid-run, and jump to definition/hover/references via rust-analyzer. In tsz, reach for it on a runtime question reading hasn't localized — above all a wrong or extra (false-positive) diagnostic, where you break the diagnostic sinks and backtrace to the deciding frame instead of grepping emit sites. Skip it for missing-diagnostic bugs (nothing fires to break on) and small localized bugs a quick read already pins down.
---

# rust-debugger

`rdbg` debugs from the command line: one paused process per project; breakpoints
and state carry across calls. Needs a debug build (the default `cargo build`).
Run `rdbg` with no arguments for the full command list; `REFERENCE.md` beside
this file adds examples. Install:
`curl -fsSL https://azimi.me/rust-debugger-skill/install.sh | sh` (also sets up
codelldb), plus `rust-analyzer` on `PATH`.

## When to reach for it (and when not)

Read first. Every `launch` rebuilds the target, so a wasted debugging detour is
expensive. Launch only for a **runtime question at a place you can name**:
- a **wrong** value where you need the real inputs/flow that produced it;
- an unexpected branch, type, or state that reading can't pin down;
- a **panic** — `rdbg debug --panic` lands on the culprit frame in one call;
- testing a fix live with `set --then continue` before editing + rebuilding.

Don't launch when a quick read already points at the fix; when the output is
**missing** (nothing fires to break on — finding the absent check is a reading
task); or to iterate on a candidate fix — re-run the narrowed test instead.

Stay cheap: one session with several breakpoints, or one `trace`, beats
re-launching. If 2–3 probes haven't localized it, go back to reading.

Fix once, don't churn: after ~2–3 edit→test cycles you're guessing — break at
the failing assertion and compare actual vs expected values, or `set` the
suspect value and `continue` to validate the hypothesis without editing.

## tsz: wrong or extra diagnostics (the main use here)

tsz diagnostics funnel through a few sinks: `CheckerState::error`,
`CheckerContext::push_diagnostic` (`context/diagnostic_push.rs`), and
`emit_render_request`. For a **wrong** or **extra** (false-positive)
diagnostic, break the sinks and trace the emit *backward* to the deciding
code instead of grepping emit sites:

```
rdbg launch --cargo crates/tsz-checker --test <stem> --break-fn error --break-fn push_diagnostic -- <test_fn>
rdbg eval code                         # `code` at an error stop; `diag.code` at push_diagnostic
rdbg continue --until 'code == <code>' # ONLY if the current stop isn't already the emit
rdbg bt                                # walk back to the deciding frame
rdbg frame <n> ; rdbg vars             # the types/flags that produced it
```

`<stem>` is `crates/tsz-checker/tests/<stem>.rs`; words after `--` filter to
the failing `#[test]` name. Check the current stop's `code` before
`continue --until` — it resumes first and tests only later stops.

- Unhit sinks are reported at exit (`bound, 0 hits` = wrong path, try the
  next sink; `NOT BOUND` = bad name). Qualified names (`Type::method`) don't
  bind — use the base name, or `--break <file>:<line>` inside the function.
- A **missing** diagnostic has nothing to trace. Read to find where the check
  should fire; only then break there to see why its condition is false.
- The same pattern works a layer down: break a `tsz-solver` relation or
  evaluation function to see the actual `TypeId`s and flags at the decision.
- vs `tsz-tracing`: `TSZ_LOG` traces show the breadth of what solver/checker
  did; rdbg answers a pointed question with concrete values and lets you
  mutate state to test a hypothesis.
- `rdbg down` when finished; `.rdbg/` is gitignored; use rdbg instead of
  temporary `dbg!`/`println!` (forbidden here anyway).

## Targets and sessions

```
rdbg launch --cargo <dir> --bin app --break src/x.rs:88 -- --threads 4
rdbg launch --cargo <dir> --lib --break src/lib.rs:42 -- my_test     # #[test] in src/
rdbg launch --cargo <dir> --test t --break tests/t.rs:12 -- case     # tests/t.rs
rdbg trace --cargo <dir> --lib --break src/lib.rs:42 --capture a,b -- my_test
```

`--lib` for a `#[test]` inside the library (the common case); `--test <name>`
only for an integration test file `tests/<name>.rs`. Add `--panic` to also stop
where a panic is raised, or `--break-fn <name>` for a function breakpoint.
`trace` runs through every hit and returns a table in one call — use it to
watch a value evolve without stepping.

## Command crib

```
rdbg break src/x.rs:42 [--if "i == 5" | --hit 3 | --log "i={i}"]
rdbg break --fn crate::f | --panic ; rdbg watch var ; rdbg breaks
rdbg continue [--until 'sum >= 100']  # condition re-checked at each stop
rdbg step over|in|out ; rdbg until src/x.rs:99 ; rdbg restart
rdbg vars ; rdbg eval a.b c ; rdbg set a.b = 8 [--then continue]
rdbg bt ; rdbg frame <n>|up|down ; rdbg list ; rdbg state
rdbg where <Name> ; rdbg def|hover|refs <file> <line> <col>
rdbg stop ; rdbg down                 # end session; stop the daemon
```

## Notes

- `eval`/`set`/conditions take variable paths and simple comparisons; codelldb
  adds comparison/arithmetic/field eval. Rust method calls can't run — break
  inside instead.
- Panicking test, one call: `rdbg debug --cargo <dir> --lib --panic -- <test>`
  returns the message, first user frame with args and locals, and a backtrace.
- Debug the debug build; `--release` has little to inspect. `rdbg down` (or 30
  minutes idle) releases the paused process.
