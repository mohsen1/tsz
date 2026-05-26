# TSZ PR Body Checklist

Use this checklist before creating or materially updating a TSZ PR.

## Required Sections

- `AgentName`: stable identity, usually a canonical lane name when assigned.
- `Track`: roadmap track and PR type, such as refactor-only, semantic campaign,
  emit/DTS parity, benchmark blocker, or tooling/guardrail.
- `Invariant`: structural rule for behavior work; iteration bottleneck and
  affected surface for process work.
- `Scope`: concrete files or systems touched.
- `Project Corpus Impact`: always present. Use `Row: n/a`, `Bug family: n/a`,
  and a concrete evidence line for docs, skills, or harness-only work.
- `Verification`: targeted local commands and CI expectation.
- `Coordination Notes`: overlap checks, dependency branches, WIP state,
  follow-ups, and signed handoff facts.

## Body Validation

Run:

```bash
gh pr view <number> --json body
```

Confirm the remote body includes all required sections after GitHub stores it.
If `project-corpus-pr-body` fails, fix the body before rerunning jobs.

## Good Evidence Lines

- `docs-only`
- `agent-skill-only`
- `test-harness-only`
- `CI script only; no compiler behavior path`
- `project row: utility-types; diagnostic false-positive family`

## Bad Evidence Lines

- blank `Evidence:`
- `n/a` with no explanation
- stale commands from a previous head
- broad claims such as `tests pass` without naming the command or CI gate
