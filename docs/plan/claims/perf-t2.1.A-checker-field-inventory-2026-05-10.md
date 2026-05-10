# perf(checker): T2.1.A.1 — checker field-lifetime inventory + CI guard

- **Date**: 2026-05-10
- **Branch**: `perf/t2.1.A-checker-field-inventory-2026-05-10`
- **PR**: TBD
- **Status**: claim
- **Workstream**: T2.1.A (PERFORMANCE_PLAN.md §6 lifetime split)

## Intent

Implements the **inventory-and-guard half** of T2.1.A from
`docs/plan/PERFORMANCE_PLAN.md`:

> | T2.1.A | Add field inventory, manifest, `ProgramContext`/`WorkerContext`/`FileSession` shells. Move only obvious `ProgramStable` fields. | CI fails on unknown fields; no behavior change. |

T2.1.A is too large to ship in one PR (227 fields × judgment-time per
field, plus skeleton struct introduction, plus actual field movement).
We split it:

- **T2.1.A.1 (this PR)**: inventory script + manifest covering all
  227 fields + CI guard. **No struct changes, no field moves.** This
  is purely the audit surface that the rest of T2.1 builds on.
- **T2.1.A.2 (next PR)**: introduce empty `ProgramContext` /
  `WorkerContext` / `FileSession` shells.
- **T2.1.A.3 (PR after)**: move only obvious `ProgramStable` borrowed
  refs (`arena`, `binder`, `types`) into `ProgramContext` with
  delegating accessors on `CheckerContext`. No semantic change.

## Files Touched

- `scripts/arch/checker_field_inventory.py` — parser + CI guard
  (~280 LOC, new). Parses `pub struct CheckerContext<'a>` from
  `crates/tsz-checker/src/context/mod.rs`, loads the manifest TOML,
  and exits non-zero on missing fields, stale entries, `Unknown`
  classifications, or invalid lifetime classes.
- `crates/tsz-checker/src/context/checker_context_lifetimes.toml` —
  manifest covering all 227 `CheckerContext` fields (~750 LOC, new).
- `scripts/arch/check-checker-boundaries.sh` — wire the inventory
  script into the existing pre-commit checker-boundary script.

## Classification distribution (227 fields)

| Class | Count | Examples |
| --- | --- | --- |
| `ProgramStable` | 46 | `arena`, `binder`, `types`, `lib_contexts`, `definition_store`, all `Arc<...>` ProjectEnv-installed indices |
| `FileLocalReset` | 119 | `request_node_types`, `flow_*` caches, `class_*_type_cache`, `node_resolution_stack`, symbol caches keyed by `SymbolId` (conservative pending QueryCache audit) |
| `SpeculationScoped` | 41 | depth counters, `contextual_type`, `return_type_stack`, nesting flags that overload checking saves/restores |
| `DiagnosticsOnly` | 21 | `diagnostics`, `emitted_diagnostics`, deferred error accumulators, parse-error position lists |

`WorkerReusable` and `LspPersistent` are intentionally empty in this
initial pass; the plan classes them as advanced-stage reclassifications
that depend on shells being introduced.

## Classification policy (encoded in the manifest's `reason` fields)

1. Borrowed `&'a` refs and `Arc<...>` shared structures consumed by
   `ProjectEnv` → `ProgramStable`.
2. Caches keyed by `NodeIndex` or `(u32, ...)` source positions →
   `FileLocalReset` because raw indices collide across files.
3. Diagnostic accumulators (`Vec<Diagnostic>`, position dedupe sets,
   error tracking) → `DiagnosticsOnly`.
4. Resolution stacks/sets that exist only to detect cycles within
   one file check → `FileLocalReset`.
5. Depth counters and overload-save/restore flags → `SpeculationScoped`.
6. Caches keyed by `SymbolId` could be safe across files but kept
   `FileLocalReset` in this pass; promotion to `ProgramStable`
   requires the QueryCache audit listed in PERFORMANCE_PLAN.md §6.

## Verification

- `python3 scripts/arch/checker_field_inventory.py` passes:
  "Checker field-lifetime inventory passed: 227 field(s) all classified."
- `python3 scripts/arch/checker_field_inventory.py --render`
  produces a markdown table grouped by lifetime class for review.
- Pre-commit hook now runs the inventory check via
  `scripts/arch/check-checker-boundaries.sh`.
- No Rust source changes; no runtime impact; no conformance impact.

## Conformance

Snapshots unchanged. This PR adds tooling and a manifest only.
