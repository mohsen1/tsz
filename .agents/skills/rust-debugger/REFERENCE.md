# rdbg full reference

Expanded command reference for the `rust-debugger` skill (upstream:
<https://github.com/mohsen1/rust-debugger-skill>). Read on demand; the triage
and tsz-specific recipe live in `SKILL.md`.

## Start a session

```
rdbg where parse_config                            # find where to break
rdbg launch --cargo . --bin app --break src/config.rs:88 -- --threads 4
rdbg launch --cargo . --lib --break src/lib.rs:42 -- my_test        # a #[test] in the library
rdbg launch --cargo . --test mytest --break tests/mytest.rs:12 -- some_case  # tests/mytest.rs
rdbg launch --bin-path target/debug/app --break src/main.rs:11   # skip the build
```

Pick the target by where the test lives: `--lib` for a `#[test]` inside the
library (`#[cfg(test)] mod tests` in `src/` — the common case), `--test <name>`
only for an integration test file `tests/<name>.rs`. In both, the words after
`--` are the test-name filter, so exactly the test you name runs.

Add `--panic` to also stop where any panic is raised, or `--break-fn <name>`.

To watch a value evolve without stepping, `trace` instead of `launch` — it runs
through every hit and returns a table in one call:

```
rdbg trace --cargo . --bin app --break src/x.rs:42 --capture i,sum --max 30
rdbg trace --cargo . --lib --break src/lib.rs:42 --capture a,b -- my_test
```

## Breakpoints

Set or change these any time, including while paused.

```
rdbg break src/x.rs:42                # line
rdbg break src/x.rs:42 --if "i == 5"  # conditional (simple comparisons)
rdbg break src/x.rs:42 --hit 3        # on the 3rd hit
rdbg break src/x.rs:42 --log "i={i}"  # logpoint (print, don't stop)
rdbg break --fn my_crate::do_thing    # entering a function
rdbg break --panic                    # where a Rust panic is raised
rdbg watch cfg.threads                # when a value changes
rdbg breaks                           # list with ids; break-rm/break-on/break-off <id>
```

## Run and step

```
rdbg continue
rdbg continue --until 'sum >= 100'    # keep resuming until a condition holds
rdbg step over | in | out | insn
rdbg until src/x.rs:99                # run to a line
rdbg pause                            # interrupt a running program
rdbg restart
```

`continue --until '<path> <op> <value>'` (ops `== != < <= > >=`) re-checks the
condition at each breakpoint stop itself — one call instead of a continue/eval
loop per iteration, and it works where lldb conditional breakpoints don't fire.
Needs an active breakpoint to stop at; ends at the first stop where the
condition holds, or reports that the program exited.

## Read and change state

```
rdbg vars                             # locals with real Rust values
rdbg eval items[0].qty sum            # one or more variable paths (not method calls)
rdbg set cfg.threads = 8 --then continue   # change a value and resume
rdbg set cfg.threads = 8              # change a value
rdbg watch-expr add total             # re-shown at every stop
rdbg bt                               # backtrace
rdbg list                             # source around the current line
rdbg state                            # stop + locals + watches together
```

## Threads and frames

```
rdbg threads
rdbg thread <id>
rdbg frame <n> | up | down            # vars/eval follow the selected frame
```

## Navigate

```
rdbg where <Name>
rdbg def | hover | refs <file> <line> <col>
```

`rdbg stop` ends the session; `rdbg down` stops the daemon.

## Batching and machine-readable output

- `rdbg do '<cmd>; <cmd>; ...'` runs several subcommands in one call; the batch
  stops at the first error or program exit.
- Stops list only the top-frame locals that changed since the previous stop
  (`~ sum: u32 = 6 (was 3)`); `rdbg vars --full` forces the complete deep dump.
- Pass `--json` anywhere in the args for one compact JSON line per command with
  a `status` field (`ok | user_error | target_error | build_error |
  debug_adapter_error | timeout | no_session`).

## Common loops

- **Wrong value.** Break where it is computed, `vars` and `eval` to see the
  real inputs, `step` to watch it go wrong, `set` to test a fix without
  recompiling.
- **Value goes wrong at some iteration.** Break in the loop, then
  `continue --until 'sum > 100'` to jump straight to the first stop where the
  condition holds instead of continue/eval-ing by hand.
- **Panic.** `rdbg debug --cargo . --lib --panic -- <test>` (or
  `--bin`/`--test`) runs to the panic and returns the message, the first *user*
  frame with its arguments and locals, and a backtrace in one call. (Or
  `launch … --panic`, then `bt`/`up` to your frame, to keep poking around.)
- **Unexpected mutation.** `watch <var>`, then `continue` to stop the moment it
  changes.
- **Failing test.** `--lib … -- <test_name>` for a `#[test]` in the library,
  `--test <name> … -- <test_name>` for `tests/<name>.rs`; break at the
  assertion or inside the code under test.

## When breakpoints don't bind

A `launch` that reports `no stop/exit event` and runs to completion without ever
pausing — even on a function you know is on the path — is a **binding failure**,
not a wrong line or symbol. `rdbg breaks` shows the breakpoint `NOT BOUND` (or
bound with `0 hits`) and there was no `>>> STOP`. The `tsz-checker --lib`
unit-test binary is the known offender (#15653): it carries hundreds of MB of
debug info and rdbg/lldb can fail to attach to or resolve symbols on it
(reported on macOS with rdbg 0.4.0). Route around it:

- **Break in a small `--test <stem>` integration target, not `--lib`.** The
  `SKILL.md` tsz recipe already uses `--test crates/tsz-checker/tests/<stem>.rs`
  for this reason — the giant `--lib` binary is the one that fails to bind, and
  most checker behavior reproduces from an integration test.
- **Last-resort fallback when nothing binds.** Build once with `cargo test -p
  tsz-checker --lib --no-run`, then run the compiled binary directly
  (`.target/debug/deps/tsz_checker-<hash> <test_name> --nocapture
  --test-threads=1`), adding a temporary sentinel `return` at the top of the
  suspect function to surface its inputs. Revert the sentinel afterward — it is
  throwaway scaffolding, and `dbg!`/`println!` remain forbidden.
- **If `cargo nextest` hangs at 0% CPU on a lock** (disk pressure), run the
  compiled test binary directly as above instead of through the runner.

## Notes

- `eval`, `set`, and conditions take variable paths and simple comparisons, not
  arbitrary Rust expressions. `codelldb` on `PATH` lifts this (the installer
  sets it up); Rust *method* calls still can't be evaluated — break inside
  instead.
- Debug the debug build; a `--release` binary has little to inspect.
- One paused process per project; `rdbg down` (or 30 minutes idle) releases it.
