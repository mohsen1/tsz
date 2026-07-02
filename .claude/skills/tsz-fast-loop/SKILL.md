---
name: tsz-fast-loop
description: Fast compile-feedback loop for editing Rust code in tsz. Use in EVERY session that edits crates/* and would run cargo check, cargo build, or cargo nextest — get rust-analyzer diagnostics in ~1-2s instead of waiting seconds-to-minutes for cargo, and only pay for real cargo runs that will pass. Also use when a cargo command was denied by the tsz-fast-loop gate, when iterating on compile errors, or when you need to find a definition, a symbol's type, or all callers (ra def/hover/refs/symbols) instead of grepping huge crates.
---

# tsz-fast-loop

tsz is 2.16M lines of Rust; `cargo check` after a real edit costs seconds to
minutes and `cargo nextest` compiles huge test binaries. A per-worktree
rust-analyzer daemon (`tszd`) answers the same compile errors in ~1-2s without
running cargo at all, so most of your edit-iterate cycles never wait on cargo.

## The loop

1. Edit code.
2. `./tools/tszd/ra diag` — numbered compile errors, ~1-2s. Fix, repeat.
3. When `ra diag` is clean, run your real `cargo check` / narrow
   `cargo nextest run` filter ONCE to confirm (rust-analyzer misses ~6% of
   error classes, so the confirming cargo run is mandatory — never skip it).

`ra explain <n>` shows diagnostic `<n>` with surrounding code.

## The gate (automatic)

A PreToolUse hook watches cargo commands. If rust-analyzer already sees
compile errors, your cargo call is denied and the errors are returned to you
directly — that denial IS your cargo feedback, delivered early. Trust it:
fix the listed errors and re-run. The gate NEVER blocks a clean run.

- Denied but you believe rust-analyzer is wrong? Re-run prefixed with
  `RA_SKIP=1` — it bypasses the gate unconditionally.
- Daemon cold or slow? Cargo runs normally (the gate fails open) and the
  daemon warms in the background for your next check.

## Navigate without grep

The warm daemon also answers navigation — much more precise than `rg` in
1900-file crates (coordinates are 1-based, same as `ra diag` / `grep -n`):

- `./tools/tszd/ra def <file> <line> <col>` — jump to a symbol's definition.
- `./tools/tszd/ra hover <file> <line> <col>` — its type/signature/docs
  (fastest way to learn what a method returns without opening its crate).
- `./tools/tszd/ra refs <file> <line> <col>` — every caller/user (capped 30);
  use before changing a signature to see the blast radius.
- `./tools/tszd/ra symbols <Name>` — find a type/function by name repo-wide.

Prefer these over rg when chasing types across crates/tsz-checker or
crates/tsz-solver; fall back to rg for string/comment searches.

## Scope

The daemon analyzes the crates you have changed (vs origin/main). Working
across crate boundaries before editing the second crate?
`./tools/tszd/ra scope <crate>` adds it. `./tools/tszd/ra scope` lists.

## Lifecycle

- `./tools/tszd/ra up` — start (idempotent; auto-started at session start and
  by the gate). Warmup ~10-60s, hidden in the background.
- `./tools/tszd/ra stats` — queries, cache hits, checks intercepted/denied.
- `./tools/tszd/ra down` — stop. The daemon auto-stops after 30min idle
  because rust-analyzer on tsz is ~10GB resident: run `ra down` when you
  finish a worktree, and expect the warmup cost again next session.

## Hard rules

- NEVER enable rust-analyzer checkOnSave/flycheck here: that makes every
  diagnostic query run `cargo check` under the hood — far slower than the loop
  it replaces. The value is native, cargo-free feedback.
- NEVER ration your own cargo checks to "save time" — on staged errors you
  need to re-check after each fix or you can miss a second error hidden behind
  the first. Check as often as you need; the gate makes it cheap.
- A clean `ra diag` is necessary but not sufficient — it can miss a small
  fraction of error classes, so always finish with the real cargo confirm and
  the narrow nextest filter per AGENTS.md.
