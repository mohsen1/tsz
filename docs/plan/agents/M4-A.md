# Agent Goal: M4-A

AgentName: M4-A
Computer: M4
Session: A
GitHub label: `agent:M4-A`

## Mission

Own recursive conditional, mapped, template-literal, `infer`, and indexed-access
evaluation bugs that block project rows, accepted-regression strictness, emit
type facts, or open bug closure. Keep evaluation solver-owned and avoid checker
symptom patches.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M4-A
scripts/agents/disk-preflight.sh M4-A
scripts/agents/list-owned-work.sh M4-A
```

## Current Assignment

- Primary gate: semantic correctness for advanced type evaluation.
- Bug families: recursive conditionals, mapped/key-remapped types, template
  literal inference, `infer`, indexed access into deferred/mapped types,
  `keyof` over patterned or symbol keys, and evaluation fuel/TS2589 behavior.
- Architecture cleanup metric: checker-local evaluation shortcuts,
  deferred-to-`any` erasure, broad type-query quarantine wrappers, and
  evaluation cache ambiguity should trend down.
- First live command: inspect owned PRs, then search open issues for labels
  `solver`, `accepted-regression`, `false-positive`, `false-negative`, and
  terms `mapped`, `conditional`, `template`, `infer`, `keyof`, `indexed`.
- Next concrete step: cluster open issues into one structural invariant, then
  open/update a draft PR with solver tests and adjacent renamed/aliased cases.

## Existing Work To Inspect First

- Open and recent M4-A PRs.
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
- If the issue is relation policy or cache-key mode, coordinate with M4-B.
- If the issue is contextual inference session state, coordinate with M4-C.

## Verification

- Add solver or checker tests with renamed type parameters and alias/wrapper
  variants.
- Use narrow `cargo nextest run` filters.
- Use a narrow project-row reduction only after a focused unit invariant exists.
- Do not run full conformance locally.
