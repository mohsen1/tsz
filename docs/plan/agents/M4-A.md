# Agent Goal: M4-A

AgentName: M4-A
Computer: M4
Session: A
GitHub label: `agent:M4-A`

## Mission

Own advanced type evaluation bugs: recursive conditional types, mapped and
key-remapped types, template literal inference, `infer`, indexed access,
`keyof`, and key-space algebra. Keep evaluation solver-owned and avoid checker
symptom patches.

This goal is not complete when a branch exists. Keep going until the scoped
change lands in `main`, then pick the next M4-A release-gate item.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M4-A
scripts/agents/disk-preflight.sh M4-A
scripts/agents/list-owned-work.sh M4-A
```

## Current Assignment

- Primary gates: all tests pass, project rows blocked by advanced type
  evaluation turn green, accepted-regression strictness does not regress, and
  advanced-type bug issues are fixed or structurally owned.
- Bug or metric families: recursive conditionals, distributivity, mapped key
  remapping, template literal inference, `infer` binding, indexed access into
  deferred/mapped types, `keyof` over patterned or symbol keys, and evaluation
  fuel/TS2589 behavior.
- Architecture cleanup metric: deferred-to-`any` erasure, checker-local
  evaluation shortcuts, ambiguous evaluation cache keys, and oversized solver
  evaluation helpers should trend down.
- First live command: inspect owned PRs, then search open issues for `mapped`,
  `conditional`, `template`, `infer`, `keyof`, `indexed`, `recursive`, and
  `accepted-regression`.
- Next concrete step: cluster issues by one structural evaluation invariant and
  open/update a draft PR with solver tests plus renamed/aliased adjacent cases.

## Existing Work To Inspect First

- Live `agent:M4-A` PRs and recent merged advanced-evaluation PRs.
- Accepted-regression paths involving mapped, conditional, and recursive
  evaluation.
- Issues around ts-toolbelt, type-fest, ts-essentials, utility-types, Zod, and
  recursive depth.
- `docs/architecture/NORTH_STAR.md` and solver evaluation/cache docs.

## Non-Overlap Rules

- Do not add test-name, alias-name, fixture-name, or display-string special
  cases.
- Do not erase deferred conditionals to `any` or `error` to silence one
  diagnostic.
- If the issue is relation policy, inference/session state, or cache-key mode,
  coordinate with M4-B.
- If the issue needs a broad solver substrate rewrite, hand off or stack with
  M4-Opus.

## Verification

- Add solver or checker tests with renamed type parameters and alias/wrapper
  variants.
- Use narrow `cargo nextest run` filters.
- Use a narrow project-row reduction only after a focused unit invariant exists.
- Do not run full conformance locally.
