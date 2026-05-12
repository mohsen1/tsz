# fix(checker): report JS @type new-expression subclass assignment mismatch

- **Date**: 2026-05-12
- **Branch**: `fix/jsdoc-newexpr-raw-relation-subclass-20260512`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Conformance / diagnostic fingerprints)

## Intent

Target the remaining `subclassThisTypeAssignable01` fingerprint-only gap after
the earlier JSDoc generic-display fix. Current `main` emits the TypeScript-file
`TS2322` for `const test8: ClassComponent<any> = new C();` but misses the
matching JavaScript-file diagnostic for:

```js
/** @type {ClassComponent<any>} */
const test9 = new C();
```

The working hypothesis is that the JS `@type` variable path contextually checks
the initializer and then reuses that contextual result as the relation source,
which can hide the raw `new C()` incompatibility. This slice will preserve
contextual typing where it matters while making the assignment relation compare
the real initializer type for this JSDoc path.

## Files Touched

- `crates/tsz-checker/src/state/variable_checking/core.rs`
- `crates/tsz-checker/tests/jsdoc_cross_file_typedef_tests.rs`
- `crates/tsz-checker/src/jsdoc/resolution/type_construction.rs`

## Verification

- `cargo test -p tsz-checker --lib jsdoc_type_assignment_new_expression_reports_subclass_mismatch -- --nocapture` (1/1 pass)
- `cargo build --profile dist-fast -p tsz-cli -p tsz-conformance` (pass)
- `.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --filter subclassThisTypeAssignable01 --print-fingerprints --verbose` (1/1 pass, fingerprint-only 0)
- `.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --filter subclassThisTypeAssignable02 --print-fingerprints --verbose` (1/1 pass, fingerprint-only 0)
- `cargo fmt --all` (pass)
- `git diff --check` (pass)
