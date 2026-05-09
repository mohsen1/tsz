# fix(binder): drop lib type-alias preservation on shadow symbols

- **Date**: 2026-05-09
- **Branch**: `claude/nice-darwin-EZlRF`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance fixes)

## Intent

Fix #4687: `should_shadow_lib`'s `collect_preserved_lib_meaning` (#4634) carries
lib `type X<T> = ...` declarations onto module-local shadow symbols, polluting
their `declarations` vec. In TSchema-style code (`const Readonly: unique
symbol`, etc.), this leaks lib's mapped-type declarations into the property /
indexed-access traversal that computes `Static<...>`, conflating independently
defined types (e.g. `Input` rendered with `Output`'s shape).

Approach: skip `TYPE_ALIAS_DECLARATION` when classifying lib declarations to
preserve. Lib `INTERFACE_DECLARATION`s (e.g. `interface Array<T>`,
`interface Promise<T>`) and lib value declarations stay preserved, so the
existing `value_only_local_const_array_does_not_shadow_global_type_array`
regression tests still pass. The narrower carve-out is what we want anyway:
lib `type X<T>` aliases on a value shadow are exactly the case that taints
indexed-access traversal.

## Files Touched

- `crates/tsz-binder/src/nodes/binding.rs`
  (skip TYPE_ALIAS_DECLARATION in `collect_preserved_lib_meaning`)
- `crates/tsz-checker/tests/lib_global_namespace_shadowing_tests.rs`
  (regression test for the TSchema unique-symbol pattern)

## Verification

- `cargo nextest run -p tsz-binder -p tsz-checker`
- `./scripts/conformance/conformance.sh run --filter deeplyNestedMappedTypes`
- Refresh conformance snapshot once green
