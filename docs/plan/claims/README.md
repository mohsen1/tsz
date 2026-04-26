## Implementation Claims

Each in-flight or recently shipped roadmap claim lives in its own file under
this directory. One file per PR avoids the ROADMAP.md merge-conflict storm
that happens when many parallel agents append new entries to a single
`Active Implementation Claims` section.

### File naming

`<branch-slug>.md` — use the same slug as the PR branch, with `/` replaced
by `-`. Example: branch `chore/scripts-emit-cache-sha256` →
`chore-scripts-emit-cache-sha256.md`.

### File format

```markdown
# <one-line title — same as PR title>

- **Date**: 2026-04-26
- **Branch**: `chore/example-helper`
- **PR**: #1234 (or "TBD" while drafting)
- **Status**: claim · ready · shipped · abandoned
- **Workstream**: 8.4 (DRY emitter helpers)

## Intent

<2-4 sentences: what this PR changes, why, what it enables.>

## Files Touched

- `crates/foo/src/bar.rs` (~40 LOC change)

## Verification

- `cargo nextest run -p foo` (123 tests pass)
- `scripts/session/verify-all.sh` (when relevant)
```

### Workflow

1. Pull latest `main` and inspect open PRs.
2. Create a branch.
3. **Add a claim file** under `docs/plan/claims/<branch-slug>.md` with the
   `claim` status. Do not touch `docs/plan/ROADMAP.md` itself.
4. Commit just the claim file. Open a draft PR with the `WIP` label and a
   `[WIP] <scope>: <intent>` title.
5. Implement.
6. Update the claim file's `Status: ready` and add the verification line.
7. Remove `WIP` label, drop `[WIP]` from the title, mark the PR ready.

### Why one file per PR

When ten agents each add a new entry to a single `Active Implementation
Claims` list in `ROADMAP.md`, every PR after the first one rebases into a
mechanical merge conflict at the same lines. With one file per PR, the
conflict surface drops to zero — git's per-file merge picks up each new
file independently.

### Existing inline claims in `ROADMAP.md`

Existing claim entries in the `Active Implementation Claims` section of
`docs/plan/ROADMAP.md` are not migrated automatically. Once an entry is
shipped or abandoned, prune it from `ROADMAP.md` rather than rewriting it
in place — that further reduces ROADMAP churn.
