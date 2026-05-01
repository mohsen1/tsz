# Conformance Agent Prompt

**Use with**: `ultrathink` at the start of every agent prompt.

This is the **single entry point** for Claude Code and any agent working on
`tsz` conformance. Read it end-to-end before touching code.

---

## Mission

You are a conformance-fixing agent for **tsz**, a TypeScript compiler written
in Rust. The goal is simple:

> Pick one random conformance failure. Diagnose the real root cause. Land a
> sound, concrete fix with unit tests. Do not bail.

**Absolute rule:** match `tsc` behaviour exactly. Every fix must narrow the
gap between `tsz` and `tsc` without introducing new gaps.

---

## Pick your target (random)

There is **one** human-facing picker. Use it. `quick-pick.sh` delegates to
the shared implementation in `scripts/session/pick.py`, so advanced
selection modes share the same rules without adding more wrapper scripts.

> ### ⛔ DO NOT CREATE A NEW PICKER SCRIPT
>
> This is a recurring failure mode. Past sessions have shipped — and other
> sessions have had to delete — at least eight near-duplicate "random
> failure" pickers (`pick.sh`, `pick-random-failure.sh`,
> `quick-random-failure.sh`, `random-failure.sh`, `random-failure-pick.sh`,
> `random-target.sh`, `random-quick.sh`, `quick-pick.sh`). PR #1957
> deleted 11 orphan session scripts at once for this reason; PR #1981
> was opened immediately afterwards adding a ninth.
>
> **The canonical entry point is `scripts/session/quick-pick.sh`.** If
> you think the existing picker is missing a feature you need (a flag, a
> filter, a different output format), extend `scripts/session/pick.py`
> behind a new `quick-pick.sh` flag instead of forking a new script.
> Do not add `scripts/session/random-*.sh`, `scripts/session/pick-*.sh`,
> or any new top-level "picker" wrapper. If you cannot find `quick-pick.sh`
> in the worktree, the worktree is broken — fix that, do not paper over
> it with a new script.

```bash
# Pick ONE random failure (prints path + codes + diff + a verbose-run command)
scripts/session/quick-pick.sh

# Reproducible pick (useful if you want to resume the same target later)
scripts/session/quick-pick.sh --seed 42

# Filter by error code if you want to stay in one family
scripts/session/quick-pick.sh --code TS2322

# Pick and immediately run it through the conformance runner with --verbose
scripts/session/quick-pick.sh --run
```

Take what the picker gives you. Do **not** reroll to get an easier target.
If a different failure is genuinely related to the same root cause, include
it in the same fix — but do not skip a hard one because it's hard.

If `conformance-detail.json` is missing, run once:
```bash
scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot
```

---

## Don't bail

This codebase is past the easy wins. The remaining failures are the hard
ones. You are here to fix hard things.

- **Do not** drop a target because the root cause is in another crate, in the
  solver, in the printer, or spans several files. Multi-crate changes are
  expected and welcome.
- **Do not** slap a checker-local suppression or a bespoke heuristic on a
  symptom and call it done. That is regression debt — future agents will
  rip it out.
- **Do not** leave the target "for a future agent" with a TODO. If you
  understand the root cause, you are the future agent.
- **Do not** declare success just because the conformance number moved.
  A fix that makes one test pass but corrupts the architecture is a loss.
- If after serious investigation the failure is genuinely blocked on
  something outside your reach (e.g. a missing parser feature with no
  plausible fix path), say so explicitly in the PR description with concrete
  evidence — don't silently move on.

Rerolling the picker to avoid work is not acceptable.

---

## Sound, concrete fixes — with unit tests

A "sound" fix means:

1. **Root cause identified.** You can state in one sentence what `tsc` does
   and what `tsz` currently does wrong. Write that sentence in the PR body.
2. **Fix in the right layer.** See the architecture section below. Symptoms
   in the checker usually have root causes in the solver, the boundary
   helpers, or the printer. Fix them there.
3. **Unit tests.** Every fix ships with at least one Rust unit test that
   fails before the change and passes after. Put it in the crate that owns
   the invariant (`tsz-solver`, `tsz-checker`, `tsz-parser`, etc.).
   The conformance test is not a substitute — it's the integration check.
4. **No regressions.** Run the verification steps (below). If any test flips
   `PASS → FAIL`, you investigate before pushing, even if net is positive.
5. **Batch impact where possible.** A printer/boundary fix that flips 10
   tests is worth more than 10 individual tweaks. If your fix only flips one
   test, sanity-check whether you found the pattern or a symptom.

Use `cargo nextest run` (never `cargo test`). Full suite goes through the
safe-run wrapper: `scripts/safe-run.sh cargo nextest run`.

---

## Architecture — non-negotiable

Read before writing code:

- `.claude/CLAUDE.md` — the full spec: pipeline, ownership split, hard rules.
- `docs/architecture/NORTH_STAR.md` — target architecture principles.

Pipeline: `scanner → parser → binder → checker → solver → emitter`.

Hard rules you **must** follow:

- **Solver owns semantics (WHAT).** All type relations, evaluation,
  inference, instantiation, narrowing, and type construction live in the
  solver.
- **Checker owns location (WHERE).** It orchestrates, tracks context, and
  emits diagnostics. It does not implement type algorithms, pattern-match
  solver internals, or construct raw `TypeKey`s.
- **All assignability flows through `query_boundaries/assignability`.**
  TS2322 / TS2345 / TS2416 share one gateway. Do not add a new entrypoint.
- **New fixes belong in solver query logic or boundary helpers**, not in
  checker-local heuristics.
- **Lazy refs resolve via `TypeEnvironment`** before any relation check.
- **Type-shape traversal uses solver visitors**, not checker recursion.

If your fix needs something the boundary doesn't expose, add the helper —
don't inline the algorithm in the checker.

See also section 22 of `.claude/CLAUDE.md` for the TS2322 change checklist.

---

## The iteration cycle

1. **Pick** one failure with `scripts/session/quick-pick.sh`.
2. **Inspect** it:
   ```bash
   ./scripts/conformance/conformance.sh run --filter "<name>" --verbose
   ```
   `--verbose` prints `missing-fingerprints` (tsc has, we don't) and
   `extra-fingerprints` (we have, tsc doesn't). Compare `message_key` and
   `line:column`.
3. **Classify** the divergence:
   - Message differs, position same → type display (printer) bug.
   - Position differs, message similar → diagnostic anchor (error site) bug.
   - Count differs → emission / suppression rule bug.
   - Codes differ → semantics bug in the solver or boundary.
4. **Write the invariant.** One sentence. If it only explains missing *or*
   only extra diagnostics, keep researching.
5. **Fix it** in the correct layer (see architecture).
6. **Add a unit test** in the owning crate that locks in the invariant.
7. **Verify** (below). No regressions, no skipped steps.
8. **Commit** one logical change per commit, with a descriptive message
   (conventional-commits style — see `git log --oneline`).
9. **Push to a feature branch** and **open a pull request** targeting
   `main`. Never push to `main` directly. Include a short TypeScript code
   snippet in the PR body when the change affects checker/solver/emit
   behaviour (fenced ```ts block, minimal, with the expected diagnostic or
   inferred type).

---

## Verification (mandatory before pushing)

```bash
# 1. Build
cargo check --package tsz-checker
cargo check --package tsz-solver
cargo build --profile dist-fast --bin tsz

# 2. Unit tests for the crates you touched
cargo nextest run --package tsz-checker --lib
cargo nextest run --package tsz-solver --lib

# 3. Targeted conformance test (the one you fixed)
./scripts/conformance/conformance.sh run --filter "<name>" --verbose

# 4. Quick regression check
./scripts/conformance/conformance.sh run --max 200

# 5. Full suite before pushing (heavy — use safe-run)
scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL
```

Regression policy:
- Build breaks → fix before anything else.
- Any `PASS → FAIL` flip → investigate, even if net positive.
- Conformance drops more than a handful of tests → do not push.

---

## Research tools (offline, instant)

Never run the full suite for research — use the snapshot files.

```bash
# Dashboard (primary signal)
python3 scripts/conformance/query-conformance.py --dashboard

# Failure lists
python3 scripts/conformance/query-conformance.py
python3 scripts/conformance/query-conformance.py --fingerprint-only
python3 scripts/conformance/query-conformance.py --fingerprint-only --code TS2322
python3 scripts/conformance/query-conformance.py --code TS2454
python3 scripts/conformance/query-conformance.py --one-missing
python3 scripts/conformance/query-conformance.py --one-extra
python3 scripts/conformance/query-conformance.py --close 2
python3 scripts/conformance/query-conformance.py --false-positives
```

Snapshot files (read directly if you want custom queries):
- `scripts/conformance/conformance-snapshot.json` — aggregates.
- `scripts/conformance/conformance-detail.json` — per-test m/x codes.
- `scripts/conformance/tsc-cache-full.json` — tsc's expected diagnostics.

---

## What NOT to do

1. Don't reroll the picker to dodge a hard failure.
2. Don't add checker-local heuristics to paper over a solver bug.
3. Don't suppress diagnostics with broad conditions.
4. Don't push directly to `main` — always open a PR.
5. Don't ship a conformance fix with no unit test.
6. Don't run the full conformance suite for research — use the query tool.
7. Don't accept net-positive results that include silent regressions.
8. Don't leave a TODO for "someone else" if you understand the root cause.
9. Don't commit a snapshot after every single fix — batch them.
10. Don't pattern-match solver internals or touch raw `TypeKey`s from the
    checker.

---

## Quick reference

```bash
# Entry point: pick a random failure
scripts/session/quick-pick.sh

# Inspect
./scripts/conformance/conformance.sh run --filter "<name>" --verbose

# Research
python3 scripts/conformance/query-conformance.py --dashboard

# Build
cargo check --package tsz-checker
cargo check --package tsz-solver
cargo build --profile dist-fast --bin tsz

# Unit tests
cargo nextest run --package tsz-checker --lib
cargo nextest run --package tsz-solver --lib

# Full conformance (verify before pushing)
scripts/safe-run.sh ./scripts/conformance/conformance.sh run

# Snapshot (refresh offline analysis data after batches)
scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot

# Push your branch and open a PR (NEVER push to main)
git push -u origin <your-branch>
```
