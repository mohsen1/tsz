# tsz-fast-loop: bringing the harness's proven levers into real tsz sessions

Goal: make real Claude Code / Codex coding sessions on `tsz` fast, given that
cargo check/build/test-compile dominates the edit loop.

Every claim below is tied to a measured result from `rust-lsp-agent-env`
(v0-v9, docs/prototype_notes.md) or to a verified fact about the tsz repo.
This is REVISION 2: an adversarial 3-agent verification pass corrected 16
issues in revision 1 (5 major); corrections are integrated and marked [V].

---

## 1. What the evidence says transfers (and what does not)

Measured on tsz itself (2.16M LOC, 5 dependency-staged tasks x 4 modes, real
latency — v9):

| lever | verdict | numbers |
|---|---|---|
| NATIVE rust-analyzer preflight (no cargo) | **the winner** | 5/5 solved, real cargo checks 4 -> 1, preflight cost 5.6s/run, zero per-query cargo; agent turn 80s vs 89s no_lsp (suggestive at n=5 [V]) |
| Flycheck-backed preflight (cargo under LSP) | **actively harmful at scale** | 569s turns (6.4x), 82.7s waiting on flychecks, 2.1x compute |
| Hard cargo budgets | **lost a solve** (one trial, clear mechanism [V]) | 4/5; the failed agent fixed error #1, exhausted budget, declared victory without discovering error #2 |
| Voluntary LSP tool adoption | weak | v0-v7: agents rarely call `ra` unprompted; the working mechanism is environment-level substitution (intercepting cargo) |
| Dirty-generation caching | irrelevant for competent agents | v5: agents edit between checks; zero same-state repeats |

**Evidence caveats, carried in full [V]:** n=5 tasks, one crate (tsz-parser),
one error family (synthetic staged compile repair), single model (sonnet).
Real sessions are semantic checker/solver work; the check-count / solve-rate /
compute results are the robust ones — turn-time deltas are suggestive only.
The Codex layer additionally extrapolates across models.

Non-obvious integration constraints the harness caught (each cost a debugging
cycle; all must ship in the product):

1. Native diagnostics are **per-open-document** — the daemon must open the
   relevant sources or errors in untouched files are invisible.
2. **didOpen during rust-analyzer workspace loading is silently dropped** —
   open sources only after load settles.
3. rust-analyzer **only re-publishes native diagnostics for changed
   documents** — unchanged-but-affected files need a version-bump re-sync per
   query.
4. A file **reverted to HEAD** keeps a stale editor overlay — previously
   synced files need one final re-sync.
5. `unresolved-method` etc. are gated behind `diagnostics.experimental.enable`
   (off by default; must be on).
6. Blocking-error classification must cover `crates/*/src`.
7. Native detection has a measured miss rate (~6% on fd) — a clean preflight
   must ALWAYS allow the real check (confirm-check soundness). Never refuse a
   clean check; never budget.

## 2. What tsz sessions actually look like (recon, verification-corrected)

- PRs are overwhelmingly agent-authored (`claude/*`, `codex/*`, `green/*`):
  fix(checker)/fix(solver)/fix(emit)/perf. Hot crates by change frequency:
  **tsz-checker (456 file-changes/200 commits, 1905 .rs files — also the
  largest), tsz-solver (245), tsz-cli (93), tsz-emitter (89)**.
- Session conventions (AGENTS.md): never run full conformance/emit/fourslash;
  narrow filters; **`cargo nextest run`, not `cargo test`**; **wrap long or
  memory-heavy commands with `scripts/safe-run.sh`** [V] — interception must
  match safe-run-wrapped cargo, or it misses exactly the expensive calls.
- Existing infra [V-corrected]:
  - Skills: **`.agents/skills/` is the primary convention (11 skills),
    `.claude/skills/` holds symlinks** (tsz-emit pattern). A .claude-only
    skill would be invisible to Codex sessions.
  - Hooks: `.claude/settings.json` (SessionStart intake echo; PostToolUse
    Edit|Write -> cargo fmt) AND **`.codex/hooks.json` mirroring them, with
    `codex_hooks = true` in `.codex/config.toml`** — Codex sessions DO have
    hooks in this repo.
  - **`rust-analyzer-lsp@claude-plugins-official` is already enabled** in
    .claude/settings.json — Claude sessions already run an RA integration.
    M4 must measure coexistence (tszd + plugin RA + possibly editor RA);
    default decision: disable the plugin when tszd is active, because the
    plugin exposes voluntary LSP tools (the weak lever) while tszd provides
    environment-level substitution (the proven lever) — re-evaluate if the
    plugin exposes a diagnostics-push or hook surface.
  - `.cargo/config.toml` sets **`[build] target-dir = ".target"`** per
    worktree, jobs=14, and `profile.dev.package."*" opt-level = 2` [V].
  - Hygiene: **`scripts/agents/llm-context-audit.py` must be run after any
    edit to `.claude/`, `.codex/`, or startup hooks** [V].
- Where "really slow" actually bites (phase-0 M2 documents it): cold checks in
  fresh `.claude/worktrees/*` (41.5s+ measured on the clone), nextest
  compiling test binaries of huge crates per iteration, and incremental checks
  after real semantic edits in checker/solver (my 1-5s touch timings are a
  lower bound).

## 3. Architecture

```
tools/tszd/                          # layer 1: daemon + CLI (vendored harness code)
  tszd.py                            # native RA daemon (v8/v9 fixes baked in)
  ra                                 # CLI: up/diag/explain/context/touched/scope/stats/down
  shims/cargo                        # layer 3b: optional PATH shim (humans/direnv)
.agents/skills/tsz-fast-loop/SKILL.md   # layer 2a (symlinked from .claude/skills/) [V]
.claude/settings.json                # layer 2b: SessionStart warm + PreToolUse gate
.codex/hooks.json                    # layer 3a: same hooks for Codex [V]
docs/plan/tsz-fast-loop.md           # this plan + predeclared metrics
```

### Layer 1 — `tszd`: the native diagnostics daemon

- rust-analyzer via stdio; **checkOnSave OFF** (never flycheck — the 6.4x
  disaster); `diagnostics.experimental.enable = true`;
  `cargo.buildScripts.enable = true`.
- **Dedicated target dir for the daemon's own cargo** [V]:
  `rust-analyzer.cargo.targetDir = ".target/tszd"` — build-script/proc-macro
  runs at startup would otherwise contend on the repo-pinned `.target` with
  the agent's cargo. Claim precisely: *no cargo after startup; startup cargo
  is isolated*. Startup estimate: 9-26s measured on the clone; the repo's
  opt-level-2 dep profile may raise the first-ever build-script compile [V].
- **Scope = dirty cone**: sources of crates containing files changed vs
  merge-base with main, plus `ra scope add <crate>`. Opened post-load;
  per-query version-bump re-sync; revert-overlay handling (constraints 1-4).
- Per-worktree Unix socket + pidfile under `.tsz-ra/` (gitignored);
  `ra up` idempotent and **fully detached** (setsid + /dev/null stdio — the
  harness's detach requirement, now explicit [V]); idle auto-shutdown; `ra down`.
- Output: compact one-line-per-diagnostic (`file:line: code: message`),
  **hard 8K character budget with an "N more — run `ra diag`" tail** [V]
  (hook feedback strings are capped at 10K chars).

### Layer 2 — Claude Code integration (the proven substitution mechanism)

- **SessionStart hook** with **`"async": true`** (or the fully-detached
  `ra up`) so warmup never blocks the first prompt [V].
- **PreToolUse hook**: matcher is exactly `"Bash"` (matchers match TOOL NAMES
  only [V]); the hook script reads stdin JSON and applies the command regex to
  `.tool_input.command` itself, exiting 0 silently for non-matching commands.
  The regex matches BOTH bare and safe-run-wrapped invocations [V]:
  `^(scripts/)?safe-run\.sh(\s+\S+)*?\s+(--\s+)?cargo\s+(check|clippy|nextest|test|build)\b|^cargo\s+(check|clippy|nextest|test|build)\b`
  1. Query `ra diag --json` (~1-2s at the 127-file tsz-parser scope, v9;
     ~2.2s on fd, v8 [V]; timeout 5s).
  2. Blocking errors in workspace crates -> **deny, reason = the numbered
     diagnostics** (within the 8K budget). The agent gets instant precise
     feedback; the whole cargo/nextest invocation is saved.
  3. Clean, daemon unreachable, or timeout -> **allow** (fail-open +
     confirm-check soundness; no budgets, ever).
  4. `RA_SKIP=1` anywhere in the command -> allow unconditionally (the
     false-positive escape hatch; the deny message teaches it).
- **Skill `.agents/skills/tsz-fast-loop/SKILL.md`** (symlink into
  `.claude/skills/`): teaches the loop ("edit -> `ra diag` -> fix -> real
  `cargo check` only when ra is clean -> ONE narrow nextest filter"),
  documents RA_SKIP / `ra scope` / lifecycle, and forbids enabling checkOnSave
  (with the 569s number).
- **Telemetry**: every interception appends one JSONL event
  (allow/deny, diag count, query ms, argv) to `.tsz-ra/events.jsonl`;
  `ra stats` summarizes checks-saved/wall-saved. Benefit gets PROVEN in
  production, not assumed.
- **Hygiene**: run `scripts/agents/llm-context-audit.py` in the same PR that
  touches `.claude/` or `.codex/` [V].

### Layer 3 — Codex + humans

- **Codex: same hooks** — mirror the SessionStart warm + PreToolUse gate into
  `.codex/hooks.json` (the repo already runs codex hooks) [V]. Verify the
  codex_hooks PreToolUse deny semantics in M6; fall back to the PATH shim if
  the gate isn't supported.
- Humans / hook-less tools: optional direnv PATH shim
  (`tools/tszd/shims/cargo`), preflight-then-exec with identical fail-open +
  RA_SKIP semantics.
- MCP server: deprioritized — voluntary tool adoption is the measured weakest
  lever; revisit only if the shim/hook layers underperform.

## 4. Phase 0 — de-risk measurements (gates, throwaway worktree)

| # | measurement | gate |
|---|---|---|
| M1 | native scope cost on **tsz-checker** (1905 files): warmup, per-query wall, RSS | p50 query < 5s, RSS acceptable; else module-level dirty-cone subsets the crate |
| M2 | real edit-loop costs: incremental check after a semantic checker/solver edit; narrow nextest compile+run | documents the savings ceiling |
| M3 | staged red/green detection probe on checker/solver/emitter error classes (port `native_diag_sensitivity.py`) | red-detect >= 90%; else enumerate blind classes in SKILL.md, keep hook allow-biased |
| M4 | coexistence: tszd + **enabled rust-analyzer-lsp plugin** + editor RA [V] | no interference; decide plugin-off-when-tszd |
| M5 | `ra up` inside a fresh `.claude/worktrees/*` worktree (cold .target/tszd, opt-level-2 build scripts [V]) | warmup < 60s; else pre-warm at worktree-intake |
| M6 | codex_hooks PreToolUse deny semantics [V] | works like Claude Code's; else shim for Codex |

## 5. Rollout + predeclared success criteria

1. One PR: `tools/tszd` + skill (`.agents/skills/` + symlink) + both hook
   files + llm-context-audit run; everything behind `TSZ_FAST_LOOP=1`
   (hooks no-op without it).
2. ~1 week of mixed sessions with the flag on.
3. **Predeclared criteria** (machine-checkable from `.tsz-ra/events.jsonl`):
   - median real cargo invocations per session down >= 40% vs baseline;
   - >= 30% of denials followed by an edit and then a PASSING real check
     (denials were actionable feedback, not noise);
   - false-block rate (denial followed by RA_SKIP + passing cargo) < 5%;
   - no regression in session solve behavior / PR merge rate.
4. Criteria hold -> default-on + writeup. Criteria fail -> the telemetry names
   the failing lever; iterate or revert. No shipping on vibes.

## 6. Risks -> mitigations (evidence-anchored)

| risk | mitigation | anchor |
|---|---|---|
| checker-scope native cost blows up | M1 gate; module-level dirty cone; allow-on-timeout | v9 measured only the 127-file scope |
| native false positives wrongly block | RA_SKIP in every deny message; false-block telemetry | fd sensitivity 94%; misses = missing_import family |
| stale diagnostics mislead the agent | all four v8 sync fixes ported | v8: three real bugs found only via agent trials |
| daemon lifecycle jank | idempotent detached `ra up`, pidfile+socket in `.tsz-ra/`, idle shutdown; robustness re-proven in M5 (the harness's daemon-lifecycle count is not in the notes — treat as unproven [V]) | code-level detach fix |
| target-dir lock contention at startup | dedicated `.target/tszd` for the daemon's cargo [V] | repo `.cargo/config.toml` pins `.target` |
| flycheck temptation | forbidden in SKILL.md with the 569s number | v9 flycheck arm |
| budget temptation | hook never refuses clean checks; no counters | v9 budget arm: 4/5 |
| triple-RA memory pressure | M4 measurement; plugin-off-when-tszd default | plugin already enabled [V] |

## 7. Effort

Layer 1 port ~1 day; layer 2+3 hooks/skill ~0.5 day; phase 0 ~0.5 day (mostly
machine time); rollout 1 week elapsed, ~0.5 day analysis.
