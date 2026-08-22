# TSZ PR Body Checklist

Use this checklist before creating or materially updating a TSZ PR.

## Required Sections

- `Goal: <green|fast|grow|hold>`: the one roadmap goal the PR serves: `green`
  (project results match TypeScript 7), `fast` (green rows >=3x faster than
  `tsgo`), `grow` (new real, dependency-complete corpus projects), or `hold`
  (do not regress a declared rewrite capability).
- `## Verification`: targeted local commands and CI expectation.
- `## Provenance`: actual runtime values, with exactly these line formats:
  - `Machine: <m1|m4|studio|cloud|hostname>`
  - `Assistant: <claude-code|codex|bot>`
  - `Model: <model id, e.g. claude-opus-4-8>`
  - `Effort: <low|medium|high|max>`

Report which computer, which assistant, which model, and what effort level you
actually ran with; do not invent nicknames.

## Body Validation

Run:

```bash
gh pr view <number> --json body
```

Confirm the remote body includes all required sections after GitHub stores it.
No CI job validates this prose; fix omissions or stale evidence before asking
for review or queueing the PR.

## Architecture-Affecting PRs

When a PR changes capability/nonclaim ownership, whole-program containment,
forcing/recursion, caches, or checker side tables, include:

- the single owner before and after, plus mirrors deleted;
- architecture metric before/after and the ratchet check command;
- local versus program-global completion behavior;
- the dependent/independent producer-consumer matrix for capability changes;
- stable-key status matrices plus exact added/removed diagnostics, with every
  addition reconciled against the pinned oracle;
- cache/recursion identity and cleanup behavior;
- for every temporary nonclaim, its typed reason and deletion condition.

Do not raise the architecture ratchet in an ordinary behavior campaign.

## Good Verification Lines

- `cargo nextest run -p tsz-core -E 'test(<filter>)'`
- `cargo nextest run --workspace`
- `./scripts/conformance/conformance.sh run --filter "<name>" --verbose`
- `docs-only; CI Summary and the relevant documentation lint suffice`
- `CI script only; no compiler behavior path`

## Bad Verification Lines

- blank `## Verification` section
- stale commands from a previous head
- broad claims such as `tests pass` without naming the command or CI gate
