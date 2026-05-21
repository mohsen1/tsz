# Agent Goal: Studio-B

AgentName: Studio-B
Computer: Studio
Session: B
GitHub label: `agent:Studio-B`

## Mission

Own project-row performance and residency blockers, especially ts-toolbelt and
type-fest scale issues, without bypassing semantic parity.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh Studio-B
scripts/agents/disk-preflight.sh Studio-B
scripts/agents/list-owned-work.sh Studio-B
```

## Current Assignment

- Initial priority: land, close, or clearly hand off existing PRs in this lane
  before claiming issue backlog.
- `#9819` and `#9829` merged during the 2026-05-21 audit window and now serve
  as the three-point recursive-utility hotspot guard for this lane. `#8870` is
  closed.
- Issue context: `#8356`, `#7574`, `#7378`, `#7531`, and the
  closed-but-informative `#8857`/`#8858`/`#8868`/`#8869`/`#8870`.
- Related recent merges to inspect for benchmark-readiness guardrails:
  `#9866`, `#9813`, `#9794`, `#9789`, `#9626`, `#9587`, and
  `#9581`.
- Track: roadmap Tracks 2 and 10.
- Next concrete step: use the post-`#9866` attribution comments on `#8356` and
  `#7378` to choose a narrow source-file alias direct-lowering or solver
  evaluation slice. The remaining dominant shape is source-file type-alias
  `type_reference` bodies involving local type parameters and local alias
  symbols; do not re-run the same attribution unless a newer compiler change
  invalidates those comments.

## Existing Work To Inspect First

- Recent Studio-A merges tightened readiness and duplicate winner-row checks;
  do not bypass those artifacts when proving a performance win.
- `#9819`/`#9829` add recursive utility alias hotspot rows; keep them as
  benchmark guards rather than broad speed-tuning branches.
- `#9836` made the residual synthetic-row attribution commands visible in the
  tsgo-winner report. `#8857` and `#8858` are now closed; use them only as
  prior attribution and timing-mode context if either row regresses in a future
  complete dashboard snapshot.
- `#8868` is closed as the current-head `ts-toolbelt-project` attribution
  checkpoint. The matching-env counter probe showed `DelegateCrossArenaSymbol`
  dominates `with_parent_cache` construction, with source-file type-alias
  residues such as `Exclude`, `Naked`, `_Omit`, and `KeySet`; do not route that
  row as a diagnostic blocker unless a matching-env artifact turns red.
- `#9866` landed the source-file type-alias delegation cache slice suggested by
  that attribution. Do not duplicate it; first refresh the benchmark/counter
  evidence and identify any remaining structural residue.
- A post-`#9866` refresh is recorded on `#8356` and `#7378`: `ts-toolbelt`
  stayed green-compatible but `tsgo` was still 9.65x faster. The cache slice
  reduced `with_parent_cache` from 718 to 642 and `DelegateCrossArenaSymbol`
  from 647 to 571, but the leading residues stayed `Exclude`, `Naked`,
  `_Omit`, `KeySet`, and related source-file aliases.
- If a benchmark row is red because of diagnostics, hand it to the owning
  semantic lane before measuring runtime.

## Non-Overlap Rules

- Broad speed tuning waits until rows are green, unless the first blocker is
  runtime, OOM, timeout, or residency.
- Performance PRs state benchmark, before/after command, diagnostic status,
  RSS when relevant, and semantic invariant.
- Do not use fixture names as fast-path keys.

## Verification

- Use `scripts/bench/perf-hotspots.sh --quick` or narrow project filters.
- Wrap heavy runs with `scripts/safe-run.sh`.
- Do not run full conformance, full emit, or full fourslash locally.
