# fix(emitter): emit TS2883 for symlinked-monorepo nested-package inferred types

- **Date**: 2026-04-30
- **Branch**: `worktree-fix-ts2883-symbol-link-decl-emit-module-names`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

When the inferred return type of a declaration references a symbol whose source
path is `<X>/node_modules/<P>/<sub>` and `<X>` is *not* an ancestor of the
consumer file's directory, the package was reached only through a symlinked or
otherwise nested `node_modules` chain outside the consumer's normal Node.js
resolution scope. tsc emits TS2883 ("inferred type cannot be named without a
reference to ... not portable") for these cases; tsz did not. This PR closes
that gap with a focused helper in the declaration emitter's portability path.

Flips `symbolLinkDeclarationEmitModuleNamesImportRef.ts` from all-missing to
PASS without regressing any of the four other `@link`-using conformance tests.

## Files Touched

- `crates/tsz-emitter/src/declaration_emitter/helpers/portability_resolve.rs`
  (+57 LOC: new `symlinked_nested_package_reference` helper + call site in
  `check_symbol_portability` between the bare-spec reachability check and the
  existing Case 1/2/3 logic)
- `crates/tsz-emitter/src/declaration_emitter/tests/enum_template_and_advanced.rs`
  (+122 LOC: 5 unit tests pinning the structural rule, including a test that
  exercises a different package/type-name pair to guard against name hardcoding)

## Verification

- Conformance: `symbolLinkDeclarationEmitModuleNamesImportRef.ts` PASS (was
  all-missing TS2883)
- Conformance: 4 currently-passing `@link` tests still PASS
  (`symlinkedWorkspaceDependenciesNoDirectLink*` × 4)
- Conformance: 3 sibling `symbolLinkDeclarationEmit*` tests still PASS
- Conformance: `declarationEmitMonorepoBaseUrl.ts` and
  `declarationEmitCommonSourceDirectoryDoesNotContainAllFiles.ts` still PASS
- Conformance: 5 of 6 corpus tests expecting TS2883 PASS (the remaining one,
  `declarationEmitObjectAssignedDefaultExport.ts`, was already a fingerprint-only
  failure on `main` before this change)
- Unit: `cargo nextest run -p tsz-emitter --lib` — 1641 tests pass
  (5 new tests included)
