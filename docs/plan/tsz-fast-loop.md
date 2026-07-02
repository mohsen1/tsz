# tsz-fast-loop

A fast compile-feedback loop for editing Rust in tsz. `cargo check` after a
real edit costs seconds to minutes here, and `cargo nextest` compiles large
test binaries; tsz-fast-loop answers the same compile errors from a warm
rust-analyzer in ~1-2s and only lets cargo runs that will actually pass go
through.

Everything lives under `tools/tszd/` plus hook wiring in `.claude/settings.json`
and `.codex/hooks.json`. It is a thin, dependency-free layer (Python stdlib
only) over the `rust-analyzer` you already have from rustup.

---

## What it does

1. **A per-worktree native diagnostics daemon (`tszd`).** It launches
   `rust-analyzer` once, opens the crates you are changing, and answers
   diagnostic and navigation queries over a Unix socket. It runs
   rust-analyzer's **native** analysis (type inference, name resolution) with
   `checkOnSave` OFF, so a query never shells out to `cargo` — the errors come
   back in ~1-2s instead of after a full compile.

2. **A cargo gate (PreToolUse hook).** When a session runs
   `cargo check | clippy | nextest | test | build` (bare or wrapped in
   `scripts/safe-run.sh`) and the daemon already sees compile errors in
   workspace crates, the command is **denied and the numbered errors are
   returned in its place** — the same feedback cargo would print, delivered
   early, and the expensive cargo/nextest invocation is skipped. If
   rust-analyzer is clean (or the daemon is cold/slow), the command runs
   normally.

3. **Grep-free navigation.** The same warm daemon answers definition, hover,
   references, and workspace-symbol queries — precise cross-crate navigation
   without `rg` over 1900-file crates.

## Using it

The daemon starts automatically at session start and on the first gated cargo
command. You mostly just edit and run cargo as usual; the gate short-circuits
the runs that would fail. When you want direct feedback:

```
./tools/tszd/ra diag            # numbered compile errors (~1-2s warm)
./tools/tszd/ra explain <n>     # diagnostic n with surrounding code

./tools/tszd/ra def   <file> <line> <col>   # jump to definition (1-based)
./tools/tszd/ra hover <file> <line> <col>   # type / signature / docs
./tools/tszd/ra refs  <file> <line> <col>   # callers/users (capped at 30)
./tools/tszd/ra symbols <Name>              # find a type/fn repo-wide

./tools/tszd/ra scope [crate]   # show / extend the analyzed crate set
./tools/tszd/ra stats           # daemon + gate session stats
./tools/tszd/ra up | down       # start / stop the daemon
```

Coordinates are 1-based, matching `ra diag` and `grep -n`.

### The loop

1. Edit code.
2. `ra diag`, fix, repeat — instant, no cargo.
3. When `ra diag` is clean, run your real `cargo check` / narrow
   `cargo nextest run` filter **once** to confirm. rust-analyzer's native
   diagnostics catch the large majority of compile errors but not every class,
   so the confirming cargo run is required, not optional.

### Escape hatches

- **`RA_SKIP=1 cargo ...`** — bypass the gate for a single command (use if you
  believe a block is a false positive).
- **`TSZ_FAST_LOOP=0`** in the environment — disable the daemon warm-up and the
  gate entirely for the session.

## Scope

The daemon analyzes only the crates you have changed relative to `origin/main`
(the "dirty cone"), which keeps warm-up and memory bounded. If you start
editing across a crate boundary, add the other crate with
`./tools/tszd/ra scope <crate>` before you rely on its diagnostics;
`./tools/tszd/ra scope` lists the current set.

## Design choices worth knowing

- **`checkOnSave` is off and stays off.** Turning it on would make every
  diagnostic query run `cargo check` under the hood, which is far slower than
  the loop it is meant to replace. The whole value here is native, cargo-free
  feedback.
- **The gate never rations or blocks clean runs.** It only ever denies a run
  the daemon already knows will fail; a clean or unknown state always passes
  through. Capping how often you may run cargo is counterproductive on staged
  errors — you need to re-check as often as you edit — so there is no budget.
- **Fail-open.** If the daemon is cold, unreachable, or slow (>5s), the cargo
  command runs unchanged and the daemon warms in the background for next time.
  The gate can only save time, never wedge you.
- **Isolated startup cargo.** rust-analyzer's one-time metadata / build-script
  / proc-macro runs use a dedicated target dir (`.target/tszd`) so they never
  contend on the target lock with your own `cargo` or your editor's
  rust-analyzer.

## Operational notes

- rust-analyzer on a workspace this size is memory-heavy (multiple GB
  resident). The daemon **auto-stops after 30 minutes idle**, disables cache
  priming, and caps its query LRU; run `./tools/tszd/ra down` when you finish a
  worktree, and expect the warm-up cost again next session.
- State lives in `.tsz-ra/` (gitignored): the socket, a `daemon.json` address
  file, a log, and `events.jsonl` — one line per gate decision, summarized by
  `ra stats`.
- One daemon per worktree; `ra up` is idempotent and race-safe. Sessions on
  different worktrees are independent.

## Not covered

- Step-through / breakpoint debugging is the Debug Adapter Protocol, not LSP,
  and requires debug builds — outside the scope of this feature. Use `TSZ_LOG`
  / `TSZ_LOG_FORMAT` tracing for runtime debugging.
