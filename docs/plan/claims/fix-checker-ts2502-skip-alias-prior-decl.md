# fix(checker): keep TS2502 self-reference for alias-prior redeclarations

- **Date**: 2026-04-26
- **Branch**: `fix/checker-ts2502-skip-alias-prior-decl`
- **PR**: #1363
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix the missing TS2502 in
`conformance/compiler/crashDeclareGlobalTypeofExport.ts`. tsc emits both
TS2451 (redeclare) and TS2502 ("'foo' is referenced directly or
indirectly in its own type annotation") for:

```ts
import * as foo from './foo'
export as namespace foo
declare global { const foo: typeof foo; }
```

tsz emits only TS2451. Root cause: `has_prior_value_declaration_for_symbol`
treats *any* non-block-scoped prior declaration as a prior-value-decl
that suppresses the TS2502 self-reference check (the canonical
`var p: T1; var p: typeof p;` case). It currently filters only
block-scoped vars (let/const/using), so import aliases and UMD
namespace exports satisfy the suppression even though they bind to
another module's surface and never establish a value-typed binding in
the redeclaring scope.

Add `NAMESPACE_IMPORT`, `IMPORT_CLAUSE`, `IMPORT_SPECIFIER`,
`IMPORT_EQUALS_DECLARATION`, `NAMESPACE_EXPORT_DECLARATION`, and
`EXPORT_SPECIFIER` to the filter (plus the identifier-with-alias-parent
shape the UMD path records as the declaration node) so alias-style
prior decls no longer suppress TS2502.

## Files Touched

- `crates/tsz-checker/src/state/variable_checking/core.rs`
  (~33 LOC: extend `has_prior_value_declaration_for_symbol` to also
   exclude alias/import/UMD-namespace declarations)
- `crates/tsz-checker/src/state/variable_checking/core_tests.rs`
  (+63 LOC, 3 new tests: UMD case (positive), `var/var typeof`
   regression guard, lone-const self-ref regression guard)

## Verification

- `cargo nextest run -p tsz-checker --lib ts2502_alias_prior_decl`
  (3 PASS)
- `cargo nextest run -p tsz-checker --lib` (2865 PASS, 0 regressions)
- `./scripts/conformance/conformance.sh run --filter "crashDeclareGlobalTypeofExport"`
  (1/1 PASS)
- Full conformance: `crashDeclareGlobalTypeofExport.ts` flips PASS.
  Two listed regressions (`valueOfTypedArray.ts`,
  `jsExportMemberMergedWithModuleAugmentation2.ts`) are pre-existing
  drift from already-merged PRs on `main` — they fail identically
  with the change reverted (verified via `git stash`).
