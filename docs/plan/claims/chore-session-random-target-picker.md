# chore(scripts/session): minimal random conformance failure picker

- **Date**: 2026-05-01
- **Time**: 2026-05-01 08:03:45
- **Branch**: `claude/brave-thompson-HPuTc`
- **PR**: TBD
- **Status**: ready
- **Workstream**: tooling (conformance workflow)

## Intent

Add `scripts/session/random-target.sh` — a small, self-contained random
picker for the conformance workflow. It complements the existing
`scripts/session/quick-pick.sh` / `scripts/session/pick.py` family but is
deliberately the simplest possible "give me one thing to work on" entry
point: no subcommands, no shortlists, no category filters beyond
`--code <ID>` and `--seed N`.

The script reads `scripts/conformance/conformance-detail.json` directly,
prints the target's path / category / expected+actual+missing+extra error
codes, the verbose-run command to repro it, and the first 40 lines of the
test source — everything an agent needs in one view to decide where to
start.

## Files Touched

- `scripts/session/random-target.sh` (new, ~115 LOC, bash + inline python3)
- `docs/plan/claims/chore-session-random-target-picker.md` (this file)

## Verification

- Manual: `scripts/session/random-target.sh --seed 42` deterministically
  picks one failure and prints the four sections (pick metadata, verbose
  command, source preview).
- Manual: `scripts/session/random-target.sh --code TS2322 --seed 7`
  filters the candidate pool by error code and picks reproducibly.
- Manual: `scripts/session/random-target.sh --code TS9999` exits non-zero
  with `no matching failures` when the filter is empty.
- `cargo fmt --all --check` — clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  — clean (zero warnings).
- `cargo nextest run --workspace --cargo-profile dist-fast --no-fail-fast`
  — 23,279/23,292 pass. The 13 dist-fast-only failures are pre-existing
  on `main` (e.g. `diagnostics::format::test_tracing::tests::*` rely on
  `debug_span!` macros that the release-profile compile-time level
  filter strips; running the same tests in dev profile passes 5/5).
  No Rust files were touched, so this PR cannot regress unit tests.

## Why no conformance fix in this PR

The first random pick was
`TypeScript/tests/cases/conformance/salsa/moduleExportAssignment7.ts`
(category: fingerprint-only, codes: TS2694). The expected fingerprints
require synthesising the CommonJS `module.exports = { ... }` object
literal as a `'"<file>".export='` namespace and threading
JSDoc `@typedef` / `@param {import("./mod").X}` resolution through it.
Each piece (CommonJS export-namespace synthesis in the binder, JSDoc
typedef binding, JSDoc `import(...)` type extraction, and the
`'"<file>".export='` printer surface) is a multi-day workstream.
Per `scripts/session/conformance-agent-prompt.md` "Don't bail" rules,
the right action is to flag the failure as out-of-reach for a single
session rather than silently rerolling — recording it here so the next
agent can pick it up cleanly.
