# chore(checker/speculation): rename `guard` callsite locals to `snap` + drop stale `SpeculationGuard` doc

- **Date**: 2026-04-26
- **Branch**: `chore/speculation-snapshot-callsite-rename`
- **PR**: #1379
- **Status**: ready
- **Workstream**: ROBUSTNESS_AUDIT_2026-04-26 item #5 (#E)

## Intent

Audit item #5 (Speculation guard documentation says rollback-on-drop, actually
implicit-commits) was partially addressed by PR #1213 (`DiagnosticSpeculationGuard`
→ `DiagnosticSpeculationSnapshot` rename) and PR #1364 (module preamble
alignment). Two remaining inconsistencies:

1. `crates/tsz-checker/src/context/speculation.rs:70` still references the
   removed `SpeculationGuard` type in a doc-comment.
2. Nine call-sites still bind the holder to a local named `guard`, perpetuating
   the implication of RAII semantics. Rename to `snap` to match the type and
   make the explicit-action discipline visible at the call-site.

Behavior preserving: variable rename + doc-comment update, no logic change.

## Files Touched

- `crates/tsz-checker/src/context/speculation.rs` (1-line doc fix)
- `crates/tsz-checker/src/checkers/jsx/overloads.rs` (4 callsites)
- `crates/tsz-checker/src/types/function_type.rs` (2 callsites)
- `crates/tsz-checker/src/types/computation/object_literal/computation.rs` (2 callsites)
- `crates/tsz-checker/src/types/class_type/js_class_properties.rs` (1 callsite)

## Verification

- `cargo check -p tsz-checker` (passes)
- `cargo nextest run -p tsz-checker --lib` (no behavior change)
