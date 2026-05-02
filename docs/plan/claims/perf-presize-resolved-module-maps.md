# perf(cli): pre-size resolved module maps

- **Date**: 2026-05-02
- **Branch**: `perf/presize-resolved-module-maps`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 5 (large-repo residency/runtime)

## Intent

Pre-size the driver's resolved-module maps from the cached module specifier
lists already collected for the compilation. This avoids repeated hash-map and
hash-set growth during module resolution on large repositories while preserving
the existing resolution behavior and checker-facing data shape.

## Planned Scope

- `crates/tsz-cli/src/driver/check.rs`
- `docs/plan/claims/perf-presize-resolved-module-maps.md`

## Verification

- Pending
