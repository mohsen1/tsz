**2026-04-27 04:50:00**
# fix(checker): walk full alias chain across binders for TS1361 vs TS1362 attribution

- **Date**: 2026-04-27
- **Branch**: `fix/ts1361-chain-attribution-cross-file-20260427-0449`
- **PR**: TBD
- **Status**: ready
- **Workstream**: Conformance — type-only import/export attribution parity

## Intent

Match `tsc`'s rule for picking between TS1361 (was imported using
`import type`) and TS1362 (was exported using `export type`) when a
value-position use of an aliased identifier crosses several files. The
nearest direct syntactic marker on any alias in the chain wins, even
when the chain crosses through plain re-exports that resolve into a
type-only binding upstream.

Reproduction (`conformance/externalModules/typeOnly/chained.ts`):

```ts
// /a.ts
class A { a!: string }
export type { A as B };
export type Z = A;

// /b.ts
import { Z as Y } from './a';
export { B as C } from './a';

// /c.ts
import type { C } from './b';
export { C as D };

// /d.ts
import { D } from './c';
new D();          // tsc: TS1361 (import type)
const d: D = {};  // tsc: TS2741
```

Pre-fix `tsz` walked the alias chain via
`get_type_only_import_export_kind`, but each iteration both checked the
local symbol for a direct `import type` / `export type` marker AND ran
`classify_cross_file_type_only_kind` on the module specifier. The walk
returned at the first cross-file inference hit. For the chain above it
hit `C in /b.ts` first (the alias from `export { B as C } from './a'`),
inferred TS1362 from `/a.ts`'s `export type { A as B }`, and returned
before reaching `C in /c.ts` whose `import type { C }` clause is the
authoritative direct marker for TS1361.

A second issue: `visited` from `resolve_alias_symbol` includes alias
`SymbolId`s from non-current binders (e.g. `C in /c.ts` while checking
`/d.ts`). The legacy walk used `binder.get_symbol_with_libs`, which only
sees the current binder + lib binders, so cross-file alias entries
fell through `continue` without ever being examined for direct markers.

## Fix

Two-pass walk in `get_type_only_import_export_kind`:

- **Pass 1**: walk every alias in `visited` using
  `get_symbol_from_any_binder` (cross-file aware), and return the first
  direct `import type` / `export type` syntactic marker found on any
  alias's declarations. Resolve the owning arena via
  `ctx.resolve_symbol_file_index` when the alias lives in another
  binder.
- **Pass 2**: identical to the legacy cross-file inference fallback —
  scan `import_module` for type-only re-exports and call
  `classify_cross_file_type_only_kind`. This preserves TS1362
  attribution for chains whose only direct marker is `export type` (and
  for inferred chains where no direct marker survives the walk).

Direct markers always win over inferred kinds, matching tsc.

## Files Touched

- `crates/tsz-checker/src/error_reporter/type_value.rs` — split single
  loop into two passes; pass 1 uses `get_symbol_from_any_binder` and
  resolves arena via `resolve_symbol_file_index`.
- `crates/tsz-cli/tests/driver_tests.rs` — new driver-level regression
  test mirroring the conformance fixture.

## Verification

- New unit test: `tsz-cli`
  `compile_chained_type_only_alias_attributes_to_import_type_marker`
  fails before fix (emits TS1362 only), passes after.
- Existing tests: `ts1361_chain_attribution_tests`,
  `ts2451_type_only_namespace_merge_tests`,
  `ts2451_plain_js_lib_anchor_tests`,
  `type_alias_namespace_merge_tests` continue to pass.
- Conformance: `typeOnly` slice 64/68 (was 63/68); `chained.ts` flips
  fail → pass with no regressions in the slice.
