# fix(checker): report JS @type new-expression subclass assignment mismatch

- **Date**: 2026-05-12
- **Branch**: `fix/jsdoc-newexpr-raw-relation-subclass-20260512`
- **PR**: TBD
- **Status**: claim
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

## Verification

- Planned: targeted checker regression for JS `@type` new-expression assignment.
- Planned: targeted conformance `subclassThisTypeAssignable01`.
