# Claim: Direct Actual-Lib Symbol Delegation

Date: 2026-05-13
PR: #6260

## Scope

Reduce the remaining `DelegateCrossArenaSymbol` declaration-file residue from
#6243 by handling actual bundled lib symbols through the existing lib resolver
instead of constructing child checkers.

This slice is deliberately limited to:

- `symbol_arenas` delegations,
- target arenas that are bundled lib declaration files, and
- symbols proven to come from actual/cloned standard libs,
- allowlisted option/registry interface symbols whose declarations are all in
  builtin lib arenas.

Type aliases, core lib identity interfaces, and symbols with non-builtin
declarations are intentionally excluded because broader prototypes changed
observable diagnostics or conformance behavior.

## Evidence

Attribution mode on `monorepo-006`:

| Counter | Before (#6243) | After | Delta |
| --- | ---: | ---: | ---: |
| `checker.with_parent_cache_constructed` | 56 | 55 | -1 |
| `with_parent_cache_by_reason.DelegateCrossArenaSymbol` | 41 | 40 | -1 |
| `delegate.misses` | 55 | 54 | -1 |
| `delegate_miss_classification.target_declaration_files` | 41 | 40 | -1 |
| `delegate_miss_classification.by_kind.type_alias` | 16 | 16 | 0 |
| `delegate_miss_classification.by_kind.interface` | 25 | 24 | -1 |

Diagnostics count remains unchanged at `10198`.

Raw artifacts:

- `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-direct-monorepo-006-diag.json`
- `docs/plan/perf-runs/raw/2026-05-13-delegate-actual-lib-direct-monorepo-006-pc.json`

## Residue

The remaining 40 declaration-file misses include the rejected type-alias slice
and interface residue that needs namespace-qualified, merged-lib, or
conformance-backed proof rather than broadening this global-lib resolver path.
