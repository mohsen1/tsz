# TSZ PR Body Checklist

Use this checklist before creating or materially updating a TSZ PR.

## Required Sections

- `Goal: <green|fast|grow|hold>`: the one roadmap goal the PR serves: `green`
  (benchmark rows compile like `tsc`), `fast` (green rows >=2x faster than
  `tsgo`), `grow` (new corpus projects), or `hold` (conformance/JS emit/DTS/
  fourslash parity floor).
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
If `pr-body-gate` fails, fix the body before rerunning jobs.

## Good Verification Lines

- `cargo nextest run -p tsz-checker <filter>`
- `./scripts/conformance/conformance.sh run --filter "<name>" --verbose`
- `docs-only; lint and pr-body-gate suffice`
- `CI script only; no compiler behavior path`

## Bad Verification Lines

- blank `## Verification` section
- stale commands from a previous head
- broad claims such as `tests pass` without naming the command or CI gate
