# Query-Boundary Refactor Tracker

Last updated: 2026-02-14
Branch: `refactor/checker-query-boundaries`
Goal: reduce checker complexity while preserving exact `tsc` behavior.

## Current Status (10-item list)

1. Centralize checker-side predicates into solver queries
Status: In progress
Notes: Significant migration done in `type_checking`, `type_computation`, `state_type_analysis`, `class_type`, `assignability_checker`, `constructor_checker`, `call_checker`, `iterable_checker`, `object_type`.

2. Remove direct `TypeKey` matching in checker hot paths
Status: In progress
Notes: Still heavy in `state_type_resolution`, `state_type_environment`, `dispatch`, and selected large flow paths.

3. Unify callable/function/constructor resolution behind one query surface
Status: In progress
Notes: Improved in class/constructor/call-related modules, but still fragmented across some checker subsystems.

4. Consolidate relation/assignability entry points
Status: In progress
Notes: Better in `assignability_checker`, but cross-module duplication still exists.

5. Normalize repeated diagnostic construction
Status: In progress
Notes: Some consolidation landed; many duplicated diagnostic assembly paths remain.

6. Collapse duplicated union/intersection traversal logic
Status: In progress
Notes: Some evaluator paths improved; broad dedup still pending in flow/control/env paths.

7. Introduce consistent query-boundary modules per checker subsystem
Status: In progress (advanced)
Notes: Boundaries added for multiple subsystems; still missing a few major modules.

8. Reduce option/plumbing duplication beyond `context.rs`
Status: In progress
Notes: Limited cleanup so far; more structural dedup needed.

9. Add focused parity tests per refactor seam
Status: Partially complete
Notes: Full suites run constantly; seam-targeted additions are still sparse.

10. Document canonical architecture/dependency directions
Status: In progress
Notes: `docs/architecture/NORTH_STAR.md` updated with DefId/Lazy architecture section.

## Completed Boundary Modules

- `query_boundaries/type_checking.rs`
- `query_boundaries/type_computation.rs`
- `query_boundaries/state_type_analysis.rs`
- `query_boundaries/class_type.rs`
- `query_boundaries/assignability.rs`
- `query_boundaries/constructor_checker.rs`
- `query_boundaries/call_checker.rs`
- `query_boundaries/iterable_checker.rs`
- `query_boundaries/object_type.rs`
- plus existing: `class.rs`, `diagnostics.rs`, `state.rs`

## Known Workspace Test Baseline

`cargo nextest run --workspace` currently has a stable failure set:

1. `tsz-cli driver_tests::compile_arrow_function_with_rest_params`
2. `tsz-cli driver_tests::compile_generic_utility_library_type_utilities`
3. `tsz-cli driver_tests::compile_resolves_node_modules_exports_subpath`
4. `tsz-cli driver_tests::compile_resolves_package_imports_wildcard`
5. `tsz-cli driver_tests::compile_resolves_package_imports_prefers_types_condition`
6. `tsz-cli driver_tests::compile_resolves_package_imports_prefers_require_condition_for_commonjs`

No additional failures should be introduced by refactor-only changes.

## Next Queue (high impact)

1. `flow_analysis.rs` boundary extraction for direct query calls.
2. `state_type_resolution.rs` targeted boundary for high-frequency query access.
3. `state_type_environment.rs` boundary for resolution/classification helpers.
4. `dispatch.rs` cleanup for remaining direct low-level type handling.
