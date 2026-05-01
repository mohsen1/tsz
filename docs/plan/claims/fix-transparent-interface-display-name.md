# Fix transparent interface display name in TS2322/TS2741 diagnostics

- **Date**: 2026-05-01
- **Branch**: `fix/transparent-interface-display-name`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance (fingerprint parity)

## Intent

When an empty interface transparently extends a base class/interface
(`interface A2 extends A1 {}`), the structural type is the base's instance
type. Fix three places so diagnostics match tsc: (1) the source position
shows the base class name, not the interface name; (2) the target position
preserves the interface annotation name; (3) the type→def registration
uses the correct `DefKind::Class` (not `ClassConstructor`) to avoid
spurious `typeof` prefixes.

Fixes all 5 `privateNamesUnique` conformance tests (was fingerprint-only).

## Files Touched

- `crates/tsz-checker/src/types/interface_type.rs` (+38 detection helper)
- `crates/tsz-checker/src/state/type_resolution/symbol_types.rs` (~30 refactor)
- `crates/tsz-checker/src/state/type_environment/lazy.rs` (~15 boundary fix)
- `crates/tsz-checker/src/error_reporter/core/diagnostic_source/assignment_formatting.rs` (~25 display routing)
- `crates/tsz-solver/src/def/core.rs` (+7 `force_register_type_to_def`)
- `crates/tsz-checker/tests/conformance_issues/core/helpers.rs` (+65 unit test)

## Verification

- `./scripts/conformance/conformance.sh run --filter "privateNamesUnique"` — 5/5 passed
- `cargo nextest run -p tsz-checker -E 'test(transparent_interface_displays_base_name_in_source_and_annotation_in_target)'` — PASS
- `cargo nextest run -p tsz-checker -p tsz-solver` — 11354 passed (3 pre-existing failures in JSX/generator)
