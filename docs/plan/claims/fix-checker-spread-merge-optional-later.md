# fix(checker): preserve earlier contributions when later object-spread member is optional

- **Date**: 2026-05-01
- **Branch**: `fix/checker-spread-merge-optional-later`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance — wrong-code)

## Intent

For object-literal spread `{ ...A, ...B }`, tsc's merge rule is asymmetric on
B's optionality:
- If B's property `p` is **required**, it fully overrides A's `p` (the runtime
  always sees B's value).
- If B's property `p` is **optional**, the runtime may skip it, so A's
  contribution still applies. The merged read type is the union of both
  read types, the merged write type is the union of both write types,
  and the merged property is required iff *any* contributor was required;
  `readonly` is intersected.

tsz historically used unconditional override (`properties.insert(prop.name, prop.clone())`),
which broke `compiler/conformance/types/spread/objectSpreadStrictNull.ts`:
`{ ...definiteString, ...optionalNumber }` should produce `{ sn: string | number }`,
not `{ sn?: number }`.

## Files Touched

- `crates/tsz-checker/src/types/computation/object_literal/computation.rs`
  (helper `merge_spread_property` + 2 call sites — main `properties` map
  and the per-branch `union_spread_branches` map).
- `crates/tsz-checker/src/tests/object_spread_optional_merge_tests.rs`
  (4 unit tests: 1 positive, 1 name-renamed positive cover, 2 negative
  covers locking required-override behavior).
- `crates/tsz-checker/src/lib.rs` (test module wiring).

## Verification

- Targeted conformance: `objectSpreadStrictNull.ts` → **1/1 PASS** (was
  fingerprint-only with 4 mismatches).
- `cargo nextest run -p tsz-checker -p tsz-solver --lib` → 8649/8649 pass.
- Smoke conformance:
  - `--filter spread` → 69/71 PASS (2 pre-existing).
  - `--filter objectLiteral` → 51/52 PASS (1 pre-existing).
