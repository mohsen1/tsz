# Claim: Direct Actual-Lib Symbol Delegation

Date: 2026-05-13
PR: follow-up to #6286

## Scope

Reduce the remaining `DelegateCrossArenaSymbol` declaration-file residue left
by #6286 by handling more actual bundled lib symbols through the existing lib
resolver instead of constructing child checkers.

This slice is deliberately limited to:

- `symbol_arenas` delegations,
- target arenas that are non-DOM/non-webworker bundled lib declaration files,
- symbols proven to come from actual/cloned standard libs,
- non-value type symbols whose declarations are all in matching bundled lib
  arenas.

Type aliases, DOM/webworker lib surfaces, value-merged symbols, and symbols
with non-builtin declarations are intentionally excluded because broader
prototypes changed observable diagnostics or conformance behavior.

## Evidence

Attribution mode on `monorepo-006`:

| Counter | Before (#6286) | After | Delta |
| --- | ---: | ---: | ---: |
| `checker.with_parent_cache_constructed` | 55 | 40 | -15 |
| `with_parent_cache_by_reason.DelegateCrossArenaSymbol` | 40 | 31 | -9 |
| `delegate.misses` | 54 | 39 | -15 |
| `delegate_miss_classification.target_declaration_files` | 40 | 31 | -9 |
| `delegate_miss_classification.by_kind.type_alias` | 16 | 16 | 0 |
| `delegate_miss_classification.by_kind.interface` | 24 | 15 | -9 |

Diagnostics count remains unchanged at `10198`.

Raw artifacts:

- `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-direct-monorepo-006-diag.json`
- `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-direct-monorepo-006-pc.json`

## Residue

The remaining 31 declaration-file misses include the rejected type-alias slice
and interface residue that needs namespace-qualified, merged-lib, or
conformance-backed proof rather than broadening this global-lib resolver path.
