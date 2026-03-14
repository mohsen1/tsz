# Session Management System — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a coordination system for 10 concurrent Claude Code agents across 2-3 machines, using git branches as the shared state and `/loop` for periodic coordination.

**Architecture:** Git branches are the coordination medium (visible from all machines via `git fetch`). Each agent owns a campaign (mechanism area), works on a `campaign/<name>` branch in its own worktree, and pushes verified changes. An integrator agent merges validated branches to main. No coordination state is committed to the repo — only campaign definitions and scripts.

**Tech Stack:** Bash scripts, YAML campaign definitions, git branching, `/loop` for periodic checks.

---

## Design Decisions

### Why git branches for coordination (not Beads, not file-based, not GitHub Issues)

- **Cross-machine**: All machines see branches via `git fetch origin`
- **Zero infrastructure**: No databases, no servers, no extra dependencies
- **No repo pollution**: Branches are ephemeral, deleted after merge
- **Self-documenting**: Branch commits = progress report
- **Claim semantics**: Branch exists on origin = campaign claimed

### Why mechanism-based campaigns (not diagnostic-code-based)

TS2322 appears in generic inference, property resolution, narrowing, AND contextual typing. Assigning "fix TS2322" to an agent means they touch every file. Assigning "fix generic inference" means they touch `solver/infer.rs` and `solver/operations/generic_call.rs` — a contained set. The TS2322 fixes come as a natural side effect.

### Why an integrator agent

Direct-to-main caused 6 regressions in 29 snapshots. The integrator validates before merging, guaranteeing green main. It's one agent's full-time job — the other 9 produce code.

### Disk management strategy

Each worktree creates ~5GB in `target/`. With 5 worktrees per machine = 25GB. Solution:
1. Share `CARGO_TARGET_DIR` across worktrees per machine (cargo handles file locks)
2. Periodic cleanup of old/merged worktrees
3. `setup-machine.sh` configures this automatically

---

## Task 1: Campaign Definitions

**Files:**
- Create: `scripts/session/campaigns.yaml`

The source of truth for what each agent works on. Machine-readable, human-understandable.

---

## Task 2: Session Launcher Script

**Files:**
- Create: `scripts/session/start-campaign.sh`

Creates a worktree on `campaign/<name>` branch off `origin/main`. Checks if campaign is already claimed (branch exists on remote). Prints agent protocol instructions. Sets up shared `CARGO_TARGET_DIR`.

---

## Task 3: Campaign Status Checker

**Files:**
- Create: `scripts/session/check-status.sh`

Shows all campaign branches, their last commit, age, and conformance delta. Designed to be run via `/loop` for periodic awareness. Also shows disk usage.

---

## Task 4: Integration Script

**Files:**
- Create: `scripts/session/integrate.sh`

The integrator agent's main tool. For each campaign branch with new commits:
1. Creates a temporary merge onto latest main
2. Runs targeted conformance tests
3. If no regression: fast-forward merges to main, pushes
4. If regression: reports failure, skips branch

Can be run manually or via `/loop 30m`.

---

## Task 5: Disk Cleanup Script

**Files:**
- Create: `scripts/session/cleanup.sh`

Removes:
- `target/` dirs in worktrees with no recent commits (>24h)
- Worktrees for merged campaign branches
- Stale remote-tracking branches
- Old conformance artifacts

---

## Task 6: Machine Setup Script

**Files:**
- Create: `scripts/session/setup-machine.sh`

Turnkey setup for a new machine (including the 3rd overnight machine):
1. Runs project setup
2. Configures shared `CARGO_TARGET_DIR`
3. Ensures `.worktrees` is gitignored
4. Shows available campaigns
5. Prints instructions for starting agents

---

## Task 7: Agent Protocol Document

**Files:**
- Create: `scripts/session/AGENT_PROTOCOL.md`

The discipline protocol: research → plan → implement → verify → commit → push.
Rules for file ownership, coordination, and when to push.
Template `/loop` commands for workers and integrator.

---

## Task 8: Update CLAUDE.md

**Files:**
- Modify: `.claude/CLAUDE.md` (section 20.25)

Add pointer to session management scripts and campaign-based workflow.

---

## Task 9: Update Settings Hooks

**Files:**
- Modify: `.claude/settings.json`

Update SessionStart hook to read campaign context when working in a campaign worktree.

---

## Task 10: End-to-End Verification

- Create a test campaign branch
- Run the full workflow: start → check-status → integrate → cleanup
- Verify no regressions to existing workflow
- Delete test branch
