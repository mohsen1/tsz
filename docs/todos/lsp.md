# LSP session notes

## 2026-02-22

Completed in this pass:
- Fixed node_modules package specifier normalization for extensionless `index.d.ts` roots (`bar/index.d.ts` -> `bar`) while preserving `.d.mts -> .mjs` and `.d.cts -> .cjs` behavior.
- Added package.json `main/module` entrypoint-aware module specifier fallback logic.
- Fixed `rootDirs` candidate selection to choose the shortest relative module specifier across root pair combinations.
- Added focused unit tests in `crates/tsz-lsp/src/project_operations.rs` for the above cases.
- Fixed quick-info hover parity for contextually-typed function-expression parameters in type-asserted callsites.
- Added focused hover unit test in `crates/tsz-lsp/tests/hover_tests.rs` for contextual parameter quick info (`(parameter) bb: number`).
- Fixed tsserver `completionInfo` member-entry selection to prefer provider member completions over project-level fallback entries, avoiding private class member leakage (`basicClassMembers`).
- Added focused completion regression tests in `crates/tsz-cli/src/bin/tsz_server/tests.rs` and `crates/tsz-lsp/tests/completions_tests.rs` for private constructor-parameter properties.
- Fixed `autoImportPaths.ts` import-fix parity by accepting JSONC-style `jsconfig.json` (including unquoted keys) in module-specifier config parsing.
- Fixed completion ordering parity for re-export/direct duplicate symbol names so barrel-style sources win when expected (`autoImportPathsAliasesAndBarrels.ts` `Thing2B`) without regressing same-directory direct imports (`Thing2A`).
- Added focused unit tests for the above in `crates/tsz-lsp/src/project_operations.rs`, `crates/tsz-lsp/tests/project_tests.rs`, and `crates/tsz-cli/src/bin/tsz_server/tests.rs`.
- Fixed `TypeScript/tests/cases/fourslash/autoImportJsDocImport1.ts` by reducing JSDoc missing-name synthetic diagnostics to unresolved type names only and rewriting matching import quick-fixes to merge into existing JSDoc `@import` tags.
- Added focused tsserver handler unit test in `crates/tsz-cli/src/bin/tsz_server/handlers_diagnostics.rs` covering the diagnostics->codefix path for JSDoc import-fix fan-out.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/autoImportCompletionAmbientMergedModule1.ts`: missing class member snippet completion `execActionWithCount`.
  Reason: requires implementing tsserver-style `includeCompletionsWithClassMemberSnippets` pipeline (including `ClassMemberSnippet/` completion source + code-action shaping), which is broader than this targeted module-specifier parity fix.
- `TypeScript/tests/cases/fourslash/autoImportModuleNone1.ts`: still returns unexpected completion `x` in `module:none` + `target:es5` scenario.
  Reason: auto-import gating paths were added for tsconfig/inferred/fourslash directive contexts, but this failure appears to involve completion entry surfacing outside current gate points and needs a deeper completion pipeline trace.
- `TypeScript/tests/cases/fourslash/autoImportCompletionExportListAugmentation{1,2,3,4}.ts`: missing `container` class member snippet completion.
  Reason: likely requires deeper augmentation/merged-export symbol indexing behavior beyond module specifier generation.
- `TypeScript/tests/cases/fourslash/arityErrorAfterStringCompletions.ts`: no completions offered inside string literal argument with contextual `keyof` generic constraint.
  Reason: needs a dedicated string-literal contextual completion pipeline (argument-context extraction + generic constraint/keyof evaluation) beyond current object/member completion paths.
- `TypeScript/tests/cases/fourslash/autoImportFileExcludePatterns{2,3}.ts` and `TypeScript/tests/cases/fourslash/autoImportSameNameDefaultExported.ts`: global completion list shape mismatch (`globalsPlus` parity, keyword/global lib surface ordering/content).
  Reason: requires deeper tsserver-parity work on completion global tables and lib-sensitive keyword/global population beyond this targeted `getCombinedCodeFix` import-merge fix.
- `TypeScript/tests/cases/fourslash/autoImportSpecifierExcludeRegexes3.ts`: import-fix module-specifier ordering remains reversed (`pkg/utils` before `pkg`).
  Reason: ordering appears to be finalized in a different post-processing layer than `CodeActionProvider` merge ordering and needs deeper trace through tsserver bridge code-fix result shaping.
- `TypeScript/tests/cases/fourslash/autoImportPathsNodeModules.ts`: import-fix module specifier mismatch persists for `@woltlab/wcf` path-mapped node_modules target.
  Reason: likely requires tracing interaction between node_modules package-specifier logic and `paths` wildcard resolution in this mixed config shape.
- `TypeScript/tests/cases/fourslash/autoImportCompletionExportEqualsWithDefault1.ts`: missing `parent` class-member snippet completion in export-equals/default-merged class hierarchy.
  Reason: requires ClassMemberSnippet parity work (inheritance-aware snippet generation + completion details/code-action shaping) beyond this targeted auto-import metadata fix.
- `TypeScript/tests/cases/fourslash/autoImportTypeOnlyPreferred1.ts`: still returns `Expected 'isNewIdentifierLocation' to be false, got true`.
  Reason: quick tsserver-side flag override caused regressions in other auto-import completion tests (`isNewIdentifierLocation` expected true), so the robust fix needs context-sensitive parity logic in completion source selection.
- `TypeScript/tests/cases/fourslash/autoImportVerbatimTypeOnly1.ts`: still fails on `verifyFileContent` after completion code action application in `/a.mts`.
  Reason: bridging parity gap between completion `source` (`./mod`) and emitted edit text (`import type` + `.js` specifier + mixed `type` named imports) requires a deeper fix in auto-import edit synthesis, not just tsserver response shaping.
- `TypeScript/tests/cases/fourslash/autoImportTypeOnlyPreferred1.ts`: now intermittently reproduces as missing completion `ts` when run in isolation.
  Reason: likely tied to broader completion candidate gating/ranking in type-only contexts under `verbatimModuleSyntax` + `moduleResolution: bundler`; needs end-to-end completion pipeline trace.
- `TypeScript/tests/cases/fourslash/alignmentAfterFormattingOnMultilineExpressionAndParametersList.ts`: still returns `Marker "1" has been invalidated by unrecoverable edits to the file`.
  Reason: requires a dedicated tsserver-parity range-format edit strategy for multiline alignment/marker stability; current session focused on paste-format normalization (`autoFormattingOnPasting`) with a minimal targeted fix.
- `TypeScript/tests/cases/fourslash/autoImportPnpm.ts`: still returns `No codefixes returned`.
  Reason: likely blocked on symlinked pnpm package topology resolution (`/node_modules/.pnpm/... -> /node_modules/...`) in missing-import candidate discovery; needs module-resolution parity work beyond this targeted CommonJS JS import-fix pass.
- `TypeScript/tests/cases/fourslash/autoImportSymlinkCaseSensitive.ts`: still returns `No codefixes returned`.
  Reason: appears to require case-sensitive symlink/node_modules symbol surfacing alignment for auto-import candidate collection; deferred due broader resolver/indexing impact.
- `TypeScript/tests/cases/fourslash/autoImportModuleNone1.ts`: still returns unexpected completion `x` under `module:none` + `target:es5`.
  Reason: inferred-project module/target gating appears to be missed in this fourslash path even after server-side inferred-option wiring; needs deeper request/bridge trace to identify where module/target options are dropped.
- `TypeScript/tests/cases/fourslash/autoImportFileExcludePatterns{2,3}.ts` and `TypeScript/tests/cases/fourslash/autoImportSameNameDefaultExported.ts`: still fail exact completion-list comparison after keyword/global ordering fix.
  Reason: ordering parity is now closer (`abstract, any, Array...`), but list cardinality/content still diverges (`globalsPlus` surface mismatch), which requires broader tsserver global-table parity work beyond this targeted completion ordering adjustment.
- `TypeScript/tests/cases/fourslash/autoImportModuleNone1.ts`: still returns unexpected completion `x` under `module:none` + `target:es5`.
  Reason: fixing this path robustly appears to require adapter-level inferred compiler option propagation for fourslash virtual files; quick bridge patches caused harness regressions and need a dedicated, isolated bridge follow-up.
- `TypeScript/tests/cases/fourslash/autoImportTypeOnlyPreferred1.ts`: after narrowing `isNewIdentifierLocation` heuristics, failure mode shifts to missing completion `ts` in this run.
  Reason: this now appears blocked on deeper auto-import candidate surfacing/ranking in type-only + `verbatimModuleSyntax` contexts (tsserver bridge/project completion integration), beyond a safe small heuristic-only patch.
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_{types,values}.ts`: still returns `Program objects are not serializable through the server protocol.`
  Reason: requires additional SessionClient/TestState adapter coverage for Program-returning harness paths (or native parity), which is broader than this JSDoc import-fix patch.
- `TypeScript/tests/cases/fourslash/autoImportVerbatimCJS1.ts`: still generates a new local baseline with missing auto-import completions/codefix snapshots.
  Reason: likely needs deeper auto-import candidate surfacing for `export =` ambient modules plus CommonJS object-literal exports under `verbatimModuleSyntax`, beyond this focused diagnostics/codefix rewrite.
- `TypeScript/tests/cases/fourslash/automaticConstructorToggling.ts`: still fails constructor quick-info parity for edited generic constructors (`Bsig`/`Dsig` in this run).
  Reason: robust parity appears to require deeper integration between edit-state-aware quick-info and constructor signature inference/instantiation (beyond a safe small fix in this pass).
- `TypeScript/tests/cases/fourslash/autoImportTypeOnlyPreferred3.ts`: still fails at marker `e` (`No codefixes returned`) after fixing earlier `codeFixAll` marker-`c` parity.
  Reason: remaining gap is namespace default/type-only import-fix candidate surfacing for `ns.A` (`import type ns from "./ns"`), which needs deeper import-candidate discovery logic beyond this targeted combined-fix patch.
- `TypeScript/tests/cases/fourslash/autoImportModuleNone1.ts`: still returns unexpected completion `x` under `module:none` + `target:es5`.
  Reason: completion still leaks through a non-trivial inferred-options/module-gating path that needs a dedicated completion pipeline trace; deferred to keep this run focused on combined missing-import fix-all correctness.
