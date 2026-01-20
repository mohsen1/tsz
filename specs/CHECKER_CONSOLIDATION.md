## Checker Consolidation Plan (Thin → Primary)

**Objective:** Retire the legacy checker and make the thin checker (thin_checker + solver + thin_* stack) the single, canonical checker. Achieve full tsc parity, then remove legacy code, prefixes, and duplicated tests.

### Target State
- One checker pipeline: thin_checker using ThinNode arena, thin_binder, thin_parser, solver.
- All diagnostics, narrowing, and property-init behavior match tsc and are driven solely by `CompilerOptions`.
- No `thin_` prefixes in public/module names; canonical names are `checker`, `binder`, `parser`, etc.
- No legacy checker code or tests remain; CI and tooling run only the unified checker.

### Phases
1) **Parity Closure**
   - Thin checker: finish TS2564/TS2565 definite-assignment parity (class decl/expr, derived ctors, loops/try/switch, computed/private names, parameter properties, undefined unions, `!`, optional props).
   - Control flow: mirror legacy `checker::control_flow_tests` (closure capture, switch fallthrough, loop back-edges) in thin checker; align flow-graph use and narrowing.
   - Namespace/class/enum/function merging: ensure symbol flags and lookup match tsc (typeof namespace alias, merging order, value/type exports).
   - Solver: close subtype/inference gaps (optional vs required params, rest/tuple intersections, template literal patterns, invariant mutable props, index signature intersections, constrained inference `infer X extends C`, number index constraints, tuple rest inference, strict unknown fallback).
   - Emitter/transforms parity for ES5 spread/call/new/rest and decorator/tuple cases to eliminate downstream mismatches/timeouts.

2) **Dual-Run & Differential Guardrails**
   - Add a parity job that runs both legacy checker and thin checker on a representative corpus and diffs diagnostics; block merges on new divergences.
   - Move legacy checker suites into thin_checker equivalents as they go green; keep solver/thin tests as the source of truth.

3) **Cutover**
   - Switch CLI/LSP/test entry points to use thin checker by default (behind a short-lived flag if needed), with parity job still watching.
   - Remove legacy-only wiring paths once parity job is green for a sustained window.

4) **Rename & Cleanup**
   - Rename modules and crates to drop `thin_` prefixes (e.g., `thin_checker` → `checker`, `thin_binder` → `binder`, `thin_parser` → `parser`). Update imports, docs, CI scripts, and paths accordingly.
   - Delete legacy checker code and tests; remove duplicate fixtures; simplify docs to reference the unified checker only.
   - Update specs/README/PROJECT_DIRECTION to reflect the single-pipeline architecture.

5) **Post-Cutover Safeguards**
   - Keep the parity/differential harness (against recorded tsc baselines) to prevent regressions.
   - Maintain CompilerOptions-driven behavior only—no test-aware or path-aware logic.
   - Keep timeout/isolated runner defaults (nextest/harness) to avoid hangs during future suite growth.

### Success Criteria
- All suites (thin_checker, solver, emitter parity, parser, CLI, transforms, conformance slice) pass under the unified checker.
- No `thin_` prefixes in active code paths or public APIs; legacy checker code removed.
- Parity job shows zero diffs vs tsc baseline over a stabilization window.
