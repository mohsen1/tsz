# fix(solver): preserve source type when instanceof target is `any`

**2026-04-27 00:00:01** Branch: `fix/solver-index-sig-any-target-20260426-2329` (originally claimed as `fix/narrow-instanceof-any-target-...`; renamed by harness)

**2026-04-27 00:00:02** Scope: `narrow_by_instance_type` true-branch was filtering union members against instance type `any` and dropping primitive members (e.g., `string`), so `obj: F | string` followed by `obj instanceof F` (where F has `new(): any`) wrongly narrowed to `F`.

**2026-04-27 00:00:03** Fix: when the resolved instance type is `any`, return `source_type` unchanged in `narrow_by_instance_type` (matches existing false-branch handling in `narrow_by_instanceof_false`).

**2026-04-27 00:00:04** Verification: `typeGuardsWithInstanceOfByConstructorSignature.ts` lines 134/135 (TS2339 on `string | F`); unit test in `tsz-solver`; full conformance.
