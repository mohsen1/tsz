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
- `TypeScript/tests/cases/fourslash/autoImportModuleNone{1,2}.ts`: now fails on `verify.getSemanticDiagnostics` payload parity after completion/edit flow.
  Reason: diagnostics path still diverges from tsserver/harness expectations for module:none test-state protocol handling (shape/content/source of diagnostics), requiring deeper TestState/SessionClient bridge tracing beyond this targeted LSP server fix.
- `TypeScript/tests/cases/fourslash/quickInfoCallProperty.ts`: still returns empty quick info at `x./**/m()` instead of property signature.
  Reason: requires deeper hover/member symbol resolution for property access nodes (current small boundary probing in tsserver quickinfo handler cannot recover `(property) I.m: () => void` without resolver-level member lookup parity).
- `TypeScript/tests/cases/fourslash/quickInfoCallProperty.ts` (related gap): declaration-site quick info on interface property signature (`m` in `interface I { m: ... }`) still returns empty via `HoverProvider`.
  Reason: this pass patched tsserver quickinfo parity for call-site marker behavior; declaration-node property hover needs a resolver/hover-layer symbol-resolution fix beyond this targeted handler fallback.

## 2026-02-22 (follow-up)

Investigated but punted:
- `TypeScript/tests/cases/fourslash/alignmentAfterFormattingOnMultilineExpressionAndParametersList.ts`: still returns `Marker "1" has been invalidated by unrecoverable edits to the file`.
  Reason: tsserver-format handler now narrows single-line edits to minimal diffs and has unit coverage for marker-stability simulation, but fourslash still invalidates marker positions in harness; remaining gap appears to be in request/option/edit-shaping parity specific to `format.document()` beyond the direct handler path tested.
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_{types,values}.ts`: still creates new local baselines with missing rename/definition/reference highlighting.
  Reason: current tsserver bridge resolves and binds one file per request (`parse_and_bind_file`), so cross-file alias symbol navigation for arbitrary module namespace identifiers is still absent and needs project-wide symbol resolution in handlers (or project-backed provider wiring), which is broader than this targeted formatting pass.

## 2026-02-22 (call hierarchy follow-up)

Completed in this pass:
- Fixed class-property arrow-function call hierarchy parity for `TypeScript/tests/cases/fourslash/callHierarchyClassPropertyArrowFunction.ts`.
- Added class-property initializer awareness in `crates/tsz-lsp/src/call_hierarchy.rs` so `prepare/incoming/outgoing` can treat `callee = () => {}` as a callable with:
  - correct callable range bounds (initializer/body),
  - precise property-name selection span,
  - `containerName` (`C`) propagation.
- Updated tsserver call-hierarchy response shaping in `crates/tsz-cli/src/bin/tsz_server/handlers_structure.rs` to emit `containerName` and avoid outgoing-call probe fallbacks that jumped to unrelated nearby call sites.
- Added focused unit coverage:
  - `crates/tsz-lsp/tests/call_hierarchy_tests.rs`
  - `crates/tsz-cli/src/bin/tsz_server/tests.rs`

Investigated but punted:
- None in this pass.

## 2026-02-22 (call hierarchy constructor follow-up)

Completed in this pass:
- Fixed outgoing call hierarchy constructor-target parity so `new Foo()` contributes an outgoing target instead of being dropped.
- Fixed call-hierarchy declaration item shaping for class constructor targets so they emit `kind: class` with class-name selection spans.
- Tightened callable range bounds using source brace matching for function-like nodes to avoid span bleed into following declarations in call hierarchy output.
- Added focused unit coverage:
  - `crates/tsz-lsp/tests/call_hierarchy_tests.rs`
  - `crates/tsz-cli/src/bin/tsz_server/tests.rs`

Investigated but punted:
- `TypeScript/tests/cases/fourslash/callHierarchyClassStaticBlock.ts`: still diverges from baseline call hierarchy (`static {}` incoming/outgoing shape).
  Reason: requires dedicated static-block callable/container modeling parity in call hierarchy, beyond this constructor-target fix.
- `TypeScript/tests/cases/fourslash/callHierarchyClassStaticBlock2.ts`: still diverges from baseline call hierarchy shape/spans.
  Reason: appears tied to the same static-block modeling gap and needs a broader static-block follow-up.

## 2026-02-22 (quick info contextual typing follow-up)

Completed in this pass:
- Improved `TypeScript/tests/cases/fourslash/quickInfoContextualTyping.ts` hover parity for:
  - class property declaration quick info (`C1T5.foo`) now using explicit function type annotation instead of `any`.
  - contextually typed function-expression parameter inside class property initializer (`function(i)` -> `(parameter) i: number`).
  - namespace-exported variable quick info container formatting (`var C2T5.foo: ...`).
- Added focused unit tests in `crates/tsz-lsp/tests/hover_tests.rs` for the above hover paths.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/quickInfoContextualTyping.ts` (remaining failure now at marker `25`).
  Reason: remaining gap is broader contextual typing for function parameters nested in array/object literal contextual signatures; needs additional contextual-type source discovery beyond property/variable declaration and type-assertion parents.

## 2026-02-22 (quick info contextual typing marker follow-up)

Completed in this pass:
- Improved quick info contextual-typing recovery for function-expression parameters in typed array-literal elements by recognizing callable contextual types from array element annotations (including type-literal call signatures).
- Added focused hover unit coverage in `crates/tsz-lsp/tests/hover_tests.rs` for contextually typed array-element function parameters.
- Added tsserver quickinfo marker probe handling for comment-based fourslash markers (`/*n*/`) in `crates/tsz-cli/src/bin/tsz_server/handlers_info.rs`.
- Added focused tsserver unit coverage in `crates/tsz-cli/src/bin/tsz_server/tests.rs` for marker-comment and identifier-position quickinfo contextual parameter typing.
- `TypeScript/tests/cases/fourslash/quickInfoContextualTyping.ts` improved from marker `25` failure (`(parameter) n: any`) to later marker `28`.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/quickInfoContextualTyping.ts` (current failure at marker `28`: expected `(property) IBar.foo: IFoo`, got empty quick info).
  Reason: remaining gap appears to be declaration-site/property quick info resolution in object-literal contextual typing (`foo` property under `IBar`) and needs a separate member/property quick-info resolver follow-up beyond this parameter-context fix.

## 2026-02-22 (call hierarchy containerName parity follow-up)

Completed in this pass:
- Fixed `TypeScript/tests/cases/fourslash/callHierarchyContainerName.ts` and `TypeScript/tests/cases/fourslash/server/callHierarchyContainerNameServer.ts` parity by improving call hierarchy callable discovery/shape for:
  - class-name markers resolving to constructors for hierarchy queries,
  - accessor response shaping (`getter`/`setter` kind + stripped name),
  - namespace/object container propagation (`containerName` for `Foo`/`Obj`),
  - constructor/class incoming caller modeling and range calculations.
- Added focused Rust unit coverage in `crates/tsz-lsp/tests/call_hierarchy_tests.rs` for:
  - same-name incoming disambiguation across classes/namespaces/object accessors,
  - precise method selection ranges.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/callHierarchyCallExpressionByConstNamedFunctionExpression.ts`: still emits `baz` quick-prepare baseline instead of `bar` at `bar()` marker in this run.
  Reason: appears to require deeper `find_node_at_offset`/prepare resolution parity for top-level call-site markers where parser node ranges over-approximate surrounding declarations; current targeted call-hierarchy container fix intentionally avoided broader node-selection/prepare resolver refactors.

## 2026-02-22 (call hierarchy const function expression follow-up)

Completed in this pass:
- Fixed call hierarchy callable resolution for const-assigned function expressions so declaration-name positions resolve to the initializer callable (`const bar = function () {}`), restoring incoming/outgoing call discovery parity for `TypeScript/tests/cases/fourslash/callHierarchyCallExpressionByConstNamedFunctionExpression.ts`.
- Added script-level incoming caller modeling in `crates/tsz-lsp/src/call_hierarchy.rs` and tsserver kind-shaping parity (`file` -> `script`) in `crates/tsz-cli/src/bin/tsz_server/handlers_structure.rs`.
- Suppressed incorrect `containerName` emission for top-level const function-expression call hierarchy items.
- Added focused unit coverage:
  - `crates/tsz-lsp/tests/call_hierarchy_tests.rs` (declaration-position incoming/outgoing + no containerName regression)
  - `crates/tsz-cli/src/bin/tsz_server/tests.rs` (tsserver incoming caller kind maps to `script`)

Investigated but punted:
- None in this pass.

## 2026-02-22 (auto-import paths node_modules follow-up)

Completed in this pass:
- Fixed `TypeScript/tests/cases/fourslash/autoImportPathsNodeModules.ts` module-specifier parity by allowing `paths` mappings to participate in auto-import module-specifier selection even when the target lives under `node_modules`, instead of short-circuiting to package-specifier only.
- Added focused unit coverage:
  - `crates/tsz-lsp/src/project_module_specifiers.rs`
  - `crates/tsz-cli/src/bin/tsz_server/handlers_code_fixes.rs`

Investigated but punted:
- `TypeScript/tests/cases/fourslash/autoImportVerbatimCJS1.ts`: still creates a new local baseline (missing CJS-style auto-import completions/codefixes like `path.normalize` and `coolName.explode`).
  Reason: requires import-candidate/model support for `export =`/CommonJS member-expression completions and codefix rewrite (`import x = require(...)` + identifier replacement), which is broader than this targeted module-specifier ordering fix.

## 2026-02-22 (completion globals surface follow-up)

Completed in this pass:
- Removed tsserver completion post-sort kind bias that incorrectly prioritized keyword entries over same-sort-text globals (`Array` vs `as`) in `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs`.
- Added focused unit test coverage for completion ordering across kinds with identical `sortText` in `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs`.
- Aligned LSP completion global keyword/global symbol tables closer to fourslash `globalsPlus` expectations in `crates/tsz-lsp/src/completions/symbols.rs`.
- Added explicit `globalThis`/`undefined` completion entries in `crates/tsz-lsp/src/completions.rs` to match fourslash global completion shape.
- Added focused unit coverage in `crates/tsz-lsp/tests/completions_tests.rs` for the global completion surface.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/autoImportFileExcludePatterns2.ts`: still fails exact global completion list equality by one entry in this run.
  Reason: residual completion-list parity gap appears tied to lib-sensitive global surface/content selection in the adapter+provider path (beyond static symbol-table alignment), requiring deeper end-to-end completion candidate tracing.
- `TypeScript/tests/cases/fourslash/autoImportFileExcludePatterns3.ts`: still fails exact global completion list equality by one entry in this run.
  Reason: same remaining lib/global completion-surface parity gap as `autoImportFileExcludePatterns2.ts`; needs deeper completion pipeline parity work.
- `TypeScript/tests/cases/fourslash/autoImportSameNameDefaultExported.ts`: still fails exact completion list equality despite ordering/surface improvements.
  Reason: remaining mismatch appears in the same global completion table/content parity layer and likely requires broader completion source harmonization rather than another small local sort/symbol-list tweak.

## 2026-02-22 (quick info contextual property-name follow-up)

Completed in this pass:
- Fixed quick-info fallback for unresolved object-literal property names under contextual typing so marker `28` in `TypeScript/tests/cases/fourslash/quickInfoContextualTyping.ts` now resolves `(property) IBar.foo: IFoo` instead of empty quick info.
- Added focused hover regression unit test in `crates/tsz-lsp/tests/hover_tests.rs` for contextual object-literal property-name hover synthesis.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/quickInfoContextualTyping.ts` (current failure moved to marker `30`: expected `(method) IFoo.f(i: number, s: string): string`, got empty quick info).
  Reason: requires declaration-site method/member quick-info resolution for contextually typed object-literal members beyond this targeted property-name fallback path.
- `TypeScript/tests/cases/fourslash/callHierarchyExportDefaultClass.ts`: still creates a local baseline with empty call hierarchy output at `export default class` marker.
  Reason: needs dedicated anonymous default-class call-hierarchy prepare/incoming/outgoing modeling (constructor/class alias handling) beyond this hover-focused patch.
- `TypeScript/tests/cases/fourslash/callHierarchyInterfaceMethod.ts`: still creates a local baseline with empty call hierarchy output at interface method declaration marker.
  Reason: requires interface method-signature call-hierarchy declaration modeling, which is broader than this quick-info fix.

## 2026-02-22 (module:none diagnostics span follow-up)

Completed in this pass:
- Fixed tsserver semantic diagnostics parity for `TypeScript/tests/cases/fourslash/autoImportModuleNone1.ts` by:
  - skipping synthetic `Cannot find name` diagnostics when TS1148 (`module:none`) is already present,
  - normalizing TS1148 diagnostic spans to stop at statement semicolons/newlines in `semanticDiagnosticsSync`.
- Added focused tsserver unit coverage in `crates/tsz-cli/src/bin/tsz_server/tests.rs` for module:none fourslash diagnostic payload behavior (no synthetic `Cannot find name`, correct TS1148 message/span).
- `./scripts/run-fourslash.sh --max=200` improved from `182/200` passing to `183/200` passing in this run, with `autoImportModuleNone1` no longer in failures.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_types.ts` and `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_values.ts`: still generate new local baselines in this run.
  Reason: remaining gaps are cross-file rename/definition/reference parity for arbitrary module namespace identifiers, which require broader project-wide symbol-resolution behavior beyond this targeted diagnostics fix.

## 2026-02-22 (completion globals parity follow-up)

Completed in this pass:
- Tightened tsserver completion sorting to use TypeScript-style case-sensitive UI numeric ordering in `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs`.
- Added focused tsserver completion sorting unit coverage for numeric segment ordering (`Int8Array` before `Int16Array`) in `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs`.
- Added tsserver-level unit coverage for ambient merged-module `autoImportFileExcludePatterns` semantics in `crates/tsz-cli/src/bin/tsz_server/tests.rs`.
- Added tsserver-level unit coverage to ensure synthetic CommonJS helper names (`exports`/`require`) are excluded from global completion lists in `crates/tsz-cli/src/bin/tsz_server/tests.rs`.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/autoImportFileExcludePatterns2.ts`: still fails exact globals-plus completion parity.
  Reason: local tsserver/unit scenarios now match expected ambient exclusion and sorting behavior, so the remaining mismatch appears to be in fourslash adapter/request-shaping parity (marker/options/context flow), not the core completion resolver.
- `TypeScript/tests/cases/fourslash/autoImportFileExcludePatterns3.ts`: still fails exact globals-plus completion parity.
  Reason: same remaining adapter-level parity gap as above; project/handler-level logic reproduces expected `x`/`y` behavior in focused tests.
- `TypeScript/tests/cases/fourslash/autoImportSameNameDefaultExported.ts`: still fails exact completion-list parity.
  Reason: likely tied to the same completion-list shaping/parity layer around exact globals-plus matching in fourslash, beyond the resolver/ordering fixes in this pass.

## 2026-02-22 (autoImportTypeOnlyPreferred1 completion follow-up)

Completed in this pass:
- Fixed `TypeScript/tests/cases/fourslash/autoImportTypeOnlyPreferred1.ts` completion/code-action parity by:
  - preserving tsserver-style completion entry `sourceDisplay` payload shape in `completionInfo`,
  - serializing LSP completion item `sourceDisplay` with tsserver-compatible camelCase key,
  - making completion auto-import edit synthesis usage-aware so type-position completions emit `import type` edits.
- Added focused unit coverage:
  - `crates/tsz-lsp/tests/completions_tests.rs` (`sourceDisplay` serialization key)
  - `crates/tsz-lsp/tests/project_tests.rs` (`export =` auto-import completion presence)
  - `crates/tsz-lsp/src/project_imports.rs` (export-assignment candidate collection)
  - `crates/tsz-cli/src/bin/tsz_server/tests.rs` (`completionInfo` + `completionEntryDetails` for type-only default auto-import from `export = ts`)
- `./scripts/run-fourslash.sh --max=200` improved from `185/200` passing to `186/200` passing in this run.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/autoImportFileExcludePatterns2.ts`: still fails exact `globalsPlus` completion-list equality (cardinality/content mismatch).
  Reason: remaining mismatch appears to be broader completion list shaping parity in the fourslash adapter/global-surface layer, beyond this targeted type-only completion fix.
- `TypeScript/tests/cases/fourslash/autoImportFileExcludePatterns3.ts`: still fails exact `globalsPlus` completion-list equality.
  Reason: same adapter/global completion-surface parity gap as `autoImportFileExcludePatterns2.ts`; needs deeper completion list source harmonization.
- `TypeScript/tests/cases/fourslash/autoImportSameNameDefaultExported.ts`: still fails exact completion-list equality.

## 2026-02-23 (quick info property-access follow-up)

Completed in this pass:
- Improved quick-info hover fallback for unresolved property-access member names (`obj.prop`) to derive member type from the left-expression type instead of returning empty hover.
- Improved contextual parameter hover typing for function expressions assigned through property access (`obj.prop = function(param) {}`) by reading callable parameter types from the assigned member’s type.
- Added focused unit coverage in `crates/tsz-lsp/tests/hover_tests.rs`:
  - `test_hover_property_access_member_name_uses_member_type`
  - `test_hover_property_assignment_function_parameter_uses_member_signature`

Investigated but punted:
- `TypeScript/tests/cases/fourslash/quickInfoContextualTyping.ts` (current failure moved to marker `61`: expected `(parameter) n: number`, got `(parameter) n: any`).
  Reason: remaining gap is contextual parameter typing for inline function expressions inside array-literal elements under property-assignment scenarios (`objc8.t11 = [function(n, s) ...]`), which needs broader array-element contextual typing parity for assignment paths beyond this targeted property-access fix.
  Reason: tied to the same unresolved global completion surface/content parity path as the `autoImportFileExcludePatterns{2,3}` failures.

## 2026-02-23 (fourslash 200 follow-up)

Investigated but punted:
- `TypeScript/tests/cases/fourslash/autoImportCompletionAmbientMergedModule1.ts`, `TypeScript/tests/cases/fourslash/autoImportCompletionExportListAugmentation{1,2,3,4}.ts`, and `TypeScript/tests/cases/fourslash/autoImportCompletionExportEqualsWithDefault1.ts`: still missing class-member snippet completions (`execActionWithCount`/`container`/`parent`).
  Reason: current tsserver bridge still lacks full fourslash parity for class-member snippet candidate surfacing in this adapter path (including request-shape/context propagation used by the harness), and a robust fix needs deeper handler+project completion integration.
- `TypeScript/tests/cases/fourslash/autoImportPnpm.ts` and `TypeScript/tests/cases/fourslash/autoImportSymlinkCaseSensitive.ts`: still return `No codefixes returned`.
  Reason: adding external-project file participation and pnpm-path fallback helpers was not sufficient in the fourslash harness path; remaining gap appears tied to symlink/link topology parity (`@link` handling + module resolution surface) beyond a small local code-fix candidate patch.

## 2026-02-22 (call hierarchy file-start incoming parity follow-up)

Completed in this pass:
- Fixed tsserver call-hierarchy incoming/outgoing bridge parity at file-start selection spans (`line:1`, `offset:1`) by treating those as source-file queries and skipping adjacent-offset probing in `crates/tsz-cli/src/bin/tsz_server/handlers_structure.rs`.
- Added focused tsserver unit coverage in `crates/tsz-cli/src/bin/tsz_server/tests.rs` to assert `provideCallHierarchyIncomingCalls` at file-start returns no calls.
- Verified targeted fourslash parity with `./scripts/run-fourslash.sh --skip-cargo-build --filter=callHierarchyFile --verbose` (now passing).
- Re-ran capped fourslash sample with `./scripts/run-fourslash.sh --skip-cargo-build --max=200`: stable `186/200` passing (same sampled pass count as before this change, with no new sampled regressions).

Investigated but punted:
- `TypeScript/tests/cases/fourslash/callHierarchyClassStaticBlock2.ts`: still creates a local baseline (empty call hierarchy payload in this run).
  Reason: requires dedicated call-hierarchy declaration/prepare support for class `static {}` blocks (constructor-like modeling) beyond this targeted file-start bridge fix.
- `TypeScript/tests/cases/fourslash/callHierarchyFunctionAmbiguity.{2,3}.ts`: still create local baselines in full-suite runs.
  Reason: appears to require overload/ambient declaration disambiguation parity in call-hierarchy declaration resolution and span shaping, which is broader than this focused incoming-query guard.

## 2026-02-22 (auto-import exclude-pattern completion/codefix follow-up)

Completed in this pass:
- Added tsserver completion-item post-processing in `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs` to prune deeper and `index`-penalized duplicate auto-import sources for the same symbol label.
- Added focused unit coverage in `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs` for the new pruning behavior.
- Added focused tsserver integration coverage in `crates/tsz-cli/src/bin/tsz_server/tests.rs` for `autoImportFileExcludePatterns` completion behavior (`Button` from `./lib/main` with no duplicate `Button` entries in completion responses).

Investigated but punted:
- `TypeScript/tests/cases/fourslash/autoImportFileExcludePatterns2.ts`: still fails on `verify.importFixModuleSpecifiers` after completion-path dedupe changes.
  Reason: completion parity gap narrowed (failure moved from `globalsPlus` list mismatch to import-fix module-specifier mismatch), but remaining issue appears in non-server import-fix request shaping/preferences propagation and requires deeper adapter-level tracing beyond this small handler-only fix.

## 2026-02-22 (auto-import exclude-pattern import-fix ordering follow-up)

Completed in this pass:
- Fixed tsserver missing-import candidate ordering for same-symbol relative module specifiers by preferring shallower paths in `crates/tsz-cli/src/bin/tsz_server/handlers_code_fixes.rs`.
- Added focused unit coverage in `crates/tsz-cli/src/bin/tsz_server/handlers_code_fixes.rs` for relative specifier ordering (`./lib/main` before `./lib/components/button/Button`).
- Verified `TypeScript/tests/cases/fourslash/autoImportFileExcludePatterns2.ts` now passes in targeted runs.
- Re-ran capped fourslash sample with `./scripts/run-fourslash.sh --max=200`: improved from `186/200` to `187/200` passing in this run.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/autoImportCompletionExportListAugmentation{1,2,3,4}.ts`: still missing `container` class member snippet completions.
  Reason: requires tsserver-parity class-member-snippet generation/source plumbing (`includeCompletionsWithClassMemberSnippets`) beyond this targeted import-fix ordering change.

## 2026-02-23 (arbitrary module namespace identifiers follow-up)

Investigated but punted:
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_values.ts`: still creates local baseline (`verify.baselineFindAllReferences` mismatch).
  Reason: rename/definition parity for quoted alias specifiers improved, but full `findReferences`/`references-full` parity still requires richer multi-symbol alias-chain modeling (defId/context graph shaping) in the SessionClient bridge path.
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_types.ts`: still creates local baseline (`verify.baselineFindAllReferences` mismatch).
  Reason: same remaining gap as values variant; current tsserver bridge output for quoted namespace alias chains does not yet match TypeScript’s referenced-symbol grouping/details contract.

## 2026-02-22 (quickInfo contextual function-valued property follow-up)

Completed in this pass:
- Improved contextual object-literal property hover recovery in `crates/tsz-lsp/src/hover.rs` for function-valued properties (`f: function(...)`) by:
  - allowing contextual-property fallback from nearest enclosing `PROPERTY_ASSIGNMENT` (not only direct identifier hits),
  - handling parenthesized wrappers when discovering object-literal contextual types (`<IFoo>({...})`),
  - preferring contextual container member type lookup over initializer-inferred `any`,
  - synthesizing method-style quick info signatures from contextual interface/type-literal member declarations when needed.
- Added focused hover unit coverage in `crates/tsz-lsp/tests/hover_tests.rs` for function-valued property quick info under contextual typing (`(method) IFoo.f(i: number, s: string): string`).
- Verified targeted improvement with `./scripts/run-fourslash.sh --filter=quickInfoContextualTyping --verbose`: failure moved from marker `30` to marker `31`.
- Re-ran capped fourslash command with `./scripts/run-fourslash.sh --max=200` (current harness still executes the full set in this environment): total passes improved from `880` to `886`.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/quickInfoContextualTyping.ts` (current failure at marker `31`: expected `(parameter) i: number`, got `(parameter) i: any`).
  Reason: remaining gap is tsserver quickinfo contextual parameter typing for function-expression parameters inside contextually typed object-literal property assignments, which needs a follow-up in quickinfo parameter-type extraction/bridge logic beyond this targeted property-name hover parity patch.

## 2026-02-22 (autoImportTypeOnlyPreferred3 TS2503 follow-up)

Completed in this pass:
- Fixed `TypeScript/tests/cases/fourslash/autoImportTypeOnlyPreferred3.ts` marker `e` by enabling missing-import quick-fix generation for `TS2503` (`Cannot find namespace`) in `crates/tsz-lsp/src/code_actions/code_action_imports.rs`.
- Extended diagnostics-driven import candidate collection to include `TS2503` and added targeted handling for `export * as default from "..."` namespace re-export defaults in `crates/tsz-lsp/src/project/imports.rs`.
- Prevented duplicate import quick-fixes caused by duplicate diagnostics by deduping diagnostics in tsserver `getCodeFixes` (`crates/tsz-cli/src/bin/tsz_server/handlers_code_fixes.rs`).
- Added focused unit tests:
  - `crates/tsz-lsp/src/project/imports.rs` (`diagnostics_import_candidates_include_default_from_export_star_as_default`)
  - `crates/tsz-cli/src/bin/tsz_server/handlers_code_fixes.rs` (`handle_get_code_fixes_missing_namespace_type_only_default_import`)
- Verified targeted fourslash parity with `./scripts/run-fourslash.sh --filter=autoImportTypeOnlyPreferred3 --verbose` (now passing).
- Re-ran capped sample with `./scripts/run-fourslash.sh --skip-build --max=200`: improved from `187/200` to `188/200` passing in this run.

Investigated but punted:
- None in this pass.

## 2026-02-22 (callHierarchyInterfaceMethod marker probe follow-up)

Completed in this pass:
- Fixed `TypeScript/tests/cases/fourslash/callHierarchyInterfaceMethod.ts` by adding interface `METHOD_SIGNATURE` support to LSP call hierarchy prepare/incoming resolution in `crates/tsz-lsp/src/call_hierarchy.rs`.
- Fixed tsserver call-hierarchy marker-comment probing (`/**/foo`) in `crates/tsz-cli/src/bin/tsz_server/handlers_structure.rs` so `prepareCallHierarchy`/incoming/outgoing requests can resolve marker-adjacent symbols.
- Added focused Rust unit tests:
  - `crates/tsz-lsp/tests/call_hierarchy_tests.rs` (`test_interface_method_signature_prepare_and_incoming_calls`)
  - `crates/tsz-cli/src/bin/tsz_server/tests.rs` (`test_prepare_call_hierarchy_marker_comment_before_interface_method`)
- Verified targeted parity improvement with:
  - `./scripts/run-fourslash.sh --filter=callHierarchyInterfaceMethod --max=20` (now passing)
- Re-ran capped sample: `./scripts/run-fourslash.sh --skip-build --max=200` stayed at `188/200` in this run.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/autoImportCompletionAmbientMergedModule1.ts`: still missing completion `execActionWithCount`.
  Reason: appears blocked on broader class-member snippet completion parity (`includeCompletionsWithClassMemberSnippets` completion source + entry detail shaping), which is larger than this targeted call-hierarchy fix.

## 2026-02-22 (quickinfo contextual parameter fallback follow-up)

Completed in this pass:
- Fixed quickinfo contextual-typing fallback for function-expression parameters under object-literal contextual members by refining tsserver quickinfo recovery in `crates/tsz-cli/src/bin/tsz_server/handlers_info.rs`.
- Added focused tsserver unit test coverage in `crates/tsz-cli/src/bin/tsz_server/tests.rs` for contextual object-literal function parameter quickinfo.
- Verified targeted fourslash progress with `./scripts/run-fourslash.sh --filter=quickInfoContextualTyping --verbose`: failure moved from marker `31` (`(parameter) i: any`) to marker `34`.
- Re-ran capped sample with `./scripts/run-fourslash.sh --skip-build --max=200`: remains `188/200` passing in this run (no sampled regressions).

Investigated but punted:
- `TypeScript/tests/cases/fourslash/quickInfoContextualTyping.ts`: still fails at marker `34` (`(property) IFoo.a: any[]` vs expected `number[]`).
  Reason: requires deeper contextual typing parity for declaration-site/object-literal array property quickinfo resolution in the hover/quickinfo provider path beyond this targeted parameter fallback patch.

## 2026-02-22 (autoImportVerbatimTypeOnly1 completion code-action merge follow-up)

Completed in this pass:
- Fixed `TypeScript/tests/cases/fourslash/autoImportVerbatimTypeOnly1.ts` code-action parity for the second completion application by aligning extensionless completion sources (`./mod`) with emitted JS specifiers (`./mod.js`) during named-import merge matching in `crates/tsz-lsp/src/code_actions/code_action_imports.rs`.
- Updated tsserver completion details shaping in `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs` so `.mts` merged auto-import edits replace an existing `import type { ... }` line (same module) instead of inserting a duplicate import line.
- Added focused unit tests:
  - `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs` (`normalize_mts_auto_import_edit_text_appends_existing_type_only_members`)
  - `crates/tsz-cli/src/bin/tsz_server/tests.rs` (`test_completion_entry_details_upgrades_type_only_named_import_for_value_usage` and `test_completion_entry_details_mts_type_position_adds_import_type_named_clause`)
- Verification:
  - `./scripts/run-fourslash.sh --filter=autoImportVerbatimTypeOnly1 --verbose` now passes.
  - `./scripts/run-fourslash.sh --max=200` improved from `188/200` to `189/200` passing in this run.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/autoImportCompletionExportListAugmentation{1,2,3,4}.ts` and `TypeScript/tests/cases/fourslash/autoImportCompletionAmbientMergedModule1.ts` remain failing in this sample.
  Reason: still blocked on broader tsserver class-member snippet completion parity (`includeCompletionsWithClassMemberSnippets` source generation + entry/detail shaping), beyond this targeted `.mts` auto-import merge fix.
- `TypeScript/tests/cases/fourslash/autoImportVerbatimCJS1.ts` remains failing in this sample.
  Reason: requires deeper CommonJS `export =` auto-import completion/code-fix modeling (`import x = require(...)`/member rewrite parity), which is broader than this targeted ESM verbatim type-only merge path.

## 2026-02-22 (signatureHelp callable interface incomplete-call follow-up)

Completed in this pass:
- Fixed `TypeScript/tests/cases/fourslash/callSignatureHelp.ts` by resolving lazy callee types before signature extraction in `crates/tsz-lsp/src/signature_help.rs`, so `declare const c: C; c(` can surface interface call signatures (`c(): number`) instead of empty signature help.
- Added focused unit coverage in `crates/tsz-lsp/tests/signature_help_tests.rs`:
  - `test_signature_help_incomplete_callable_interface_call`
- Verification:
  - `cargo nextest run -p tsz-lsp test_signature_help_incomplete_callable_interface_call` passes.
  - `./scripts/run-fourslash.sh --filter=callSignatureHelp --workers=1 --verbose` now passes.
  - `./scripts/run-fourslash.sh --max=200` remains `189/200` passing with the same 11 known failures (no regression from this change).
  - `cargo nextest run -p tsz-lsp` passes (`725` tests).

Investigated but punted:
- None in this pass.

## 2026-02-22 (quickinfo contextual array property follow-up)

Completed in this pass:
- Fixed quickinfo/hover contextual property typing fallback for object-literal members with non-function initializers by reading member annotations from contextual type declarations when solver property lookup is unavailable (`crates/tsz-lsp/src/hover.rs`).
- Added focused unit tests:
  - `crates/tsz-lsp/tests/hover_tests.rs` (`test_hover_contextual_object_literal_array_property_name`)
  - `crates/tsz-cli/src/bin/tsz_server/tests.rs` (`test_quickinfo_contextual_object_literal_array_property_name`)
- Verified targeted progress with `./scripts/run-fourslash.sh --filter=quickInfoContextualTyping --verbose`: failure moved from marker `34` (`(property) IFoo.a: any[]`) to marker `36`.
- Re-ran capped sample with `./scripts/run-fourslash.sh --skip-build --max=200`: remained `189/200` passing (no sampled regressions in this run).
- Broader safety check: `cargo nextest run -p tsz-lsp -p tsz-cli` passed (`1114` tests).

Investigated but punted:
- `TypeScript/tests/cases/fourslash/quickInfoContextualTyping.ts`: still fails at marker `36` (`(parameter) i: any` vs expected `number`).
  Reason: requires additional contextual parameter typing parity for function expressions assigned through class property declarations in tsserver quickinfo recovery beyond this targeted property-type fallback.

## 2026-02-23 (class-member snippet fallback scan follow-up)

Completed in this pass:
- Hardened class-member snippet fallback file discovery in `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs` by:
  - fixing external-project scan path collection to use tracked file paths (not project-name keys),
  - merging fallback snippet candidates even when provider returns partial member results,
  - adding import-specifier-based fallback module resolution for relative and `node_modules` paths used during snippet member discovery.
- Added focused handler unit coverage in `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs` for:
  - external project file path scan selection,
  - fallback import-specifier file resolution from open-file maps.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/autoImportCompletionAmbientMergedModule1.ts`, `TypeScript/tests/cases/fourslash/autoImportCompletionExportListAugmentation{1,2,3,4}.ts`, and `TypeScript/tests/cases/fourslash/autoImportCompletionExportEqualsWithDefault1.ts`: still missing class-member snippet completions (`execActionWithCount`/`container`/`parent`).
  Reason: remaining gap appears to be deeper tsserver/fourslash adapter parity in class-member snippet candidate surfacing context (request/project state hydration), beyond local fallback scan-path and module-specifier discovery fixes.
- `TypeScript/tests/cases/fourslash/autoImportVerbatimCJS1.ts`: still creates a local baseline with empty completion/codefix auto-import payloads.
  Reason: requires broader CommonJS auto-import candidate/edit parity for `module.exports = { ... }` and `export =`/`import = require(...)` completion + code-action shaping beyond this focused snippet fallback patch.

## 2026-02-23 (class-member snippet configure + accessor follow-up)

Completed in this pass:
- Wired tsserver `configure` preference persistence for `includeCompletionsWithClassMemberSnippets` so completion requests without inline `preferences` can still enable class-member snippet paths.
- Added getter accessor support to class-member snippet fallback candidate extraction (`GET_ACCESSOR` -> `get name(): T {}` shape).
- Added focused unit coverage in `crates/tsz-cli/src/bin/tsz_server/tests.rs` and `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs` for:
  - configure-driven snippet preference behavior,
  - augmented alias-chain inherited getter snippet candidate recovery,
  - snippet-priority/normalization behavior for `ClassMemberSnippet/` entries.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/autoImportCompletionAmbientMergedModule1.ts`, `TypeScript/tests/cases/fourslash/autoImportCompletionExportListAugmentation{1,2,3,4}.ts`, and `TypeScript/tests/cases/fourslash/autoImportCompletionExportEqualsWithDefault1.ts`: still fail in fourslash with snippet-shape mismatches (`isSnippet`/`insertText`) despite passing focused tsserver unit coverage.
  Reason: remaining divergence appears in the fourslash harness/session bridge completion-entry matching/selection path (not reproducible in direct `tsz-server` unit tests), and needs dedicated adapter-level trace instrumentation to locate where snippet entry metadata is lost or a non-snippet duplicate entry is selected first.

## 2026-02-23 (quickinfo class-property assignment parameter follow-up)

Completed in this pass:
- Fixed tsserver quickinfo contextual parameter fallback for function expressions assigned to class properties (`this.foo = function(i, s)`) by:
  - adding assignment-LHS property probe support when resolving enclosing callable hover context,
  - accepting property/variable function-type quickinfo display forms (`(property) C.foo: (i: number, s: string) => ...`) in parameter type extraction.
- Added focused tsserver unit coverage in `crates/tsz-cli/src/bin/tsz_server/tests.rs`:
  - `test_quickinfo_contextual_class_property_assignment_function_parameter`
- Verified crate safety:
  - `cargo nextest run -p tsz-cli`
  - `cargo nextest run -p tsz-lsp -p tsz-cli`
- Verified targeted fourslash progression:
  - `./scripts/run-fourslash.sh --filter=quickInfoContextualTyping --verbose` moved failure from marker `36` to marker `45`.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/quickInfoContextualTyping.ts`: still fails at marker `45` (`(property) t1: (s: string) => string` expected, got empty quick info).
  Reason: remaining gap is declaration-site/object-property assignment quickinfo fallback for `objc8.t1 = function(...)` in `handlers_info` member-access recovery; needs a dedicated property-assignment quickinfo resolver pass beyond this parameter-only fix.

## 2026-02-23 (class-member snippet parity follow-up)

Completed in this pass:
- Improved class-member snippet completion parity for `TypeScript/tests/cases/fourslash/autoImportCompletionExportListAugmentation1.ts` by:
  - accepting top-level completion/configure preference shapes in tsserver handlers (in addition to nested `preferences` payloads),
  - preserving class-member snippet response shape parity (`insertText`/`filterText` without `isSnippet` for `ClassMemberSnippet/` entries),
  - normalizing class-member snippet sort priority to tsserver location-priority ordering,
  - adding fallback code-action synthesis for class-member snippets when auto-import additional edits are missing (derive needed type imports from snippet type identifiers and enclosing `extends` import source).
- Added focused Rust unit tests in:
  - `crates/tsz-cli/src/bin/tsz_server/tests.rs`
  - `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs`
- Verification:
  - `./scripts/run-fourslash.sh --max=200` improved from `189/200` to `191/200` passing in this run.
  - `cargo nextest run -p tsz-cli -p tsz-lsp` passed (`1133` tests).

Investigated but punted:
- `TypeScript/tests/cases/fourslash/autoImportCompletionAmbientMergedModule1.ts`: still fails on `execActionWithCount` completion insert text mismatch.
  Reason: remaining gap appears to require richer method-signature/snippet-text synthesis from ambient merged-module symbol shape (beyond current class-member snippet fallback heuristics).
- `TypeScript/tests/cases/fourslash/autoImportCompletionExportEqualsWithDefault1.ts`: still fails on `parent` completion insert text mismatch.
  Reason: requires export-equals/default-merged inheritance-aware class-member snippet text generation, which is broader than this targeted completion-shape/code-action fallback patch.
- `TypeScript/tests/cases/fourslash/autoImportCompletionExportListAugmentation{2,4}.ts`: still fail `verifyFileContent` on completion-applied edits.
  Reason: remaining divergence is in exact import-edit text shaping/order for class-member snippet code actions under augmentation variants; needs a dedicated edit-synthesis parity pass.

## 2026-02-23 (call hierarchy decorator incoming follow-up)

Completed in this pass:
- Fixed `TypeScript/tests/cases/fourslash/callHierarchyDecorator.ts` incoming-call parity by treating decorator references (`@bar`, `@bar()`, `@ns.bar`) as call-hierarchy references in `crates/tsz-lsp/src/hierarchy/call_hierarchy.rs`.
- Added decorator caller recovery so top-level decorator usages contribute a declaration caller item (e.g. class `Foo`) instead of collapsing to script-level caller only.
- Added focused unit coverage in `crates/tsz-lsp/tests/call_hierarchy_tests.rs`:
  - `test_incoming_calls_include_decorator_references`.
- Verification:
  - `./scripts/run-fourslash.sh --filter=callHierarchyDecorator --verbose` now passes.
  - `cargo nextest run -p tsz-lsp` passes.
  - `./scripts/run-fourslash.sh --skip-build --max=200` remains `191/200` (no sampled regression).

Investigated but punted:
- `TypeScript/tests/cases/fourslash/callHierarchyFunctionAmbiguity.{1,2,3,4,5}.ts`: still fail with single-file declaration fallback behavior.
  Reason: requires project-wide/multi-file symbol resolution in tsserver call hierarchy requests (current `parse_and_bind_file` flow binds one file at a time), which is broader than this targeted decorator incoming fix.

## 2026-02-23 (class-member snippet ambient merged-module follow-up)

Completed in this pass:
- Fixed `TypeScript/tests/cases/fourslash/autoImportCompletionAmbientMergedModule1.ts` by improving class-member snippet method text/code-action parity in `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs`:
  - normalize fallback method parameter lists by trimming trailing commas,
  - normalize fallback method return types by stripping trailing `{`/`;` bleed from parser text spans,
  - prioritize synthesized class-member snippet import text changes over borrowed project auto-import edits for tsserver-like import placement.
- Added focused tsserver unit coverage in `crates/tsz-cli/src/bin/tsz_server/tests.rs`:
  - `test_completion_info_class_member_snippet_method_trims_trailing_param_comma` (completion entry `insertText` + `completionEntryDetails` code-action text change shape).
- Verification:
  - `cargo nextest run -p tsz-cli` passes (`408` tests).
  - `./scripts/run-fourslash.sh --skip-ts-build --max=200 --workers=4` improved from `191/200` to `192/200` passing.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_types.ts`
  Reason: still requires project-wide symbol grouping in tsserver `references-full`/rename/definition paths (current handlers remain single-file `parse_and_bind_file` based for these flows).
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_values.ts`
  Reason: same cross-file alias/re-export grouping gap as `_types` in tsserver navigation handlers.

## 2026-02-23 (quickinfo contextual call-signature spacing follow-up)

Completed in this pass:
- Fixed tsserver quickinfo display-string normalization spacing for call signatures in object types in `crates/tsz-cli/src/bin/tsz_server/handlers_info.rs` (`) :` -> `):`) so contextual overload-style type literals match fourslash quick info formatting expectations.
- Added focused unit coverage in `crates/tsz-cli/src/bin/tsz_server/handlers_info.rs`:
  - `normalize_quickinfo_display_string_normalizes_object_call_signature_spacing`
- Verification:
  - `cargo nextest run -p tsz-cli normalize_quickinfo_display_string_normalizes_object_call_signature_spacing` passes.
  - `./scripts/run-fourslash.sh --filter=quickInfoContextualTyping --verbose` improved from marker `18` spacing mismatch to marker `45`.
  - `./scripts/run-fourslash.sh --max=200` remains `192/200` passing in this run.
  - `cargo nextest run -p tsz-lsp -p tsz-cli` passed (`1141` tests).

Investigated but punted:
- `TypeScript/tests/cases/fourslash/quickInfoContextualTyping.ts`: still fails at marker `45` (`(property) t1: (s: string) => string` expected, got empty quick info).
  Reason: requires a dedicated quickinfo fallback for object-property assignment member access (`objc8.t1 = function(...)`) in tsserver `handlers_info` beyond this formatting-only patch.
- `TypeScript/tests/cases/fourslash/autoImportCompletionExportListAugmentation2.ts`
  Reason: remaining failure is exact completion-applied import-edit shaping/order for class-member snippets under augmentation variants.
- `TypeScript/tests/cases/fourslash/autoImportCompletionExportListAugmentation4.ts`
  Reason: same augmentation-variant import-edit shaping/order parity gap as `...ExportListAugmentation2.ts`.
- `TypeScript/tests/cases/fourslash/autoImportCompletionExportEqualsWithDefault1.ts`
  Reason: still needs export-equals/default-merged inheritance-aware class-member snippet insert-text synthesis (`parent` entry).
- `TypeScript/tests/cases/fourslash/autoImportPnpm.ts`
  Reason: still blocked on pnpm/symlinked package topology candidate surfacing for auto-import code fixes.
- `TypeScript/tests/cases/fourslash/autoImportSymlinkCaseSensitive.ts`
  Reason: still blocked on case-sensitive symlink/module-resolution parity in auto-import candidate discovery.
- `TypeScript/tests/cases/fourslash/autoImportVerbatimCJS1.ts`
  Reason: still needs deeper CommonJS/export-equals completion + code-fix modeling (`import = require(...)` style edits/candidates).

## 2026-02-23 (call hierarchy static block follow-up)

Completed in this pass:
- Fixed call hierarchy static-block parity for:
  - `TypeScript/tests/cases/fourslash/callHierarchyClassStaticBlock.ts`
  - `TypeScript/tests/cases/fourslash/callHierarchyClassStaticBlock2.ts`
- Added static-block callable modeling in `crates/tsz-lsp/src/hierarchy/call_hierarchy.rs` so `prepare/incoming/outgoing` treat `static {}` as a constructor-like callable (`name: static {}`) with correct selection span.
- Prevented class `containerName` leakage for functions nested inside class static blocks.
- Scoped outgoing-call collection to the current callable boundary (excluding nested callable bodies), and added static-block local function declaration fallback resolution for unresolved sibling calls.
- Added focused unit coverage in `crates/tsz-lsp/tests/call_hierarchy_tests.rs` for:
  - static block prepare shape,
  - no class container on nested static-block functions,
  - incoming caller mapping to static block,
  - direct static-block outgoing calls,
  - sibling function resolution inside static blocks.
- Validation:
  - `cargo nextest run -p tsz-lsp` => 732 passed.
  - `./scripts/run-fourslash.sh --skip-ts-build --filter=callHierarchyClassStaticBlock --sequential --verbose` => 2/2 passed.
  - `./scripts/run-fourslash.sh --skip-ts-build --max=200` => 192/200 passed.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_types.ts` and `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_values.ts`: still generate local baseline mismatches in this run.
  Reason: appears to require broader cross-file project-backed rename/definition/reference protocol parity in the tsserver bridge; deferred to keep this pass tightly scoped to static-block call hierarchy correctness.
- `TypeScript/tests/cases/fourslash/autoImportCompletionExportListAugmentation{2,4}.ts`: still fail `verifyFileContent` in this run.
  Reason: completion snippet/code-action shaping for export-list augmentation remains a larger completion pipeline parity task; out of scope for this targeted call-hierarchy fix.

## 2026-02-23 (class-member snippet export-equals details follow-up)

Completed in this pass:
- Fixed `TypeScript/tests/cases/fourslash/autoImportCompletionExportEqualsWithDefault1.ts` by improving class-member snippet handling in `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs`:
  - map explicit `./x.js` imports to sibling TS sources (`./x.ts`/`./x.d.ts`) for snippet fallback graph traversal,
  - normalize property snippet type names to underscored alias forms discovered from project/import graph (`Container` -> `Container_`, `Document` -> `Document_`),
  - allow snippet detail resolution when `completionEntryDetails` explicitly requests `source: "ClassMemberSnippet/"` even if snippet prefs are absent on the details request,
  - synthesize transitive default-import code actions for underscored alias members when direct auto-import edits are unavailable.
- Added focused unit coverage:
  - `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs`
    - `resolve_imported_module_files_maps_js_specifier_to_ts_source`
    - `class_member_snippet_additional_edits_rewrite_default_import_for_underscored_alias`
  - `crates/tsz-cli/src/bin/tsz_server/tests.rs`
    - `test_completion_info_class_member_snippet_export_equals_default_parent`
- Verification:
  - `cargo nextest run -p tsz-cli test_completion_info_class_member_snippet_export_equals_default_parent`
  - `./scripts/run-fourslash.sh --filter=autoImportCompletionExportEqualsWithDefault1 --verbose` now passes.
  - `./scripts/run-fourslash.sh --max=200` improved from `192/200` to `193/200` passing.
  - `cargo nextest run -p tsz-cli -p tsz-lsp` passed (`1146` tests).

Investigated but punted:
- None in this pass.

## 2026-02-23 (class-member snippet import source/order follow-up)

Completed in this pass:
- Fixed `TypeScript/tests/cases/fourslash/autoImportCompletionExportListAugmentation{2,4}.ts` code-action parity for class-member snippet completion application by improving import edit synthesis in `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs`:
  - prefer side-effect-imported module sources when synthesizing required type imports for `ClassMemberSnippet/` completion details,
  - insert synthesized named imports after the existing import block (instead of always at byte `0`) when a side-effect import for the module already exists.
- Added focused unit coverage:
  - `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs`
    - `class_member_snippet_synthesized_text_changes_inserts_after_import_block_for_side_effect_import`
  - `crates/tsz-cli/src/bin/tsz_server/tests.rs`
    - `test_completion_entry_details_class_member_snippet_export_list_augmentation_import_order`
- Verification:
  - `cargo nextest run -p tsz-cli` passed (`414` tests).
  - `./scripts/run-fourslash.sh --skip-ts-build --filter=autoImportCompletionExportListAugmentation2 --verbose` now passes.
  - `./scripts/run-fourslash.sh --skip-ts-build --skip-cargo-build --filter=autoImportCompletionExportListAugmentation4 --verbose` now passes.
  - `./scripts/run-fourslash.sh --skip-ts-build --skip-cargo-build --max=200` improved from `193/200` to `195/200` passing.
  - `cargo nextest run -p tsz-lsp -p tsz-cli` passed (`1148` tests).

Investigated but punted:
- None in this pass.

## 2026-02-23 (quickinfo contextual function-array parameter follow-up)

Completed in this pass:
- Improved tsserver quickinfo contextual parameter extraction for function-array property types by normalizing callable type text before parameter slicing in `crates/tsz-cli/src/bin/tsz_server/handlers_info.rs` (handles `((n: number, s: string) => string)[]` instead of collapsing to `number, s: string`).
- Extended assignment-LHS property probing for contextual quickinfo recovery to support wrapped RHS function expressions (`x.y = [function(...) {}]` / `x.y = (function(...) {})`).
- Added focused unit coverage in `crates/tsz-cli/src/bin/tsz_server/handlers_info.rs`:
  - `assignment_lhs_property_offset_before_function_supports_array_wrapped_rhs`
  - `contextual_parameter_type_from_text_extracts_function_array_parameter`
- Verification:
  - `cargo nextest run -p tsz-cli` passed (`416` tests).
  - `./scripts/run-fourslash.sh --filter=quickInfoContextualTyping --verbose` moved failure from marker `61` (`(parameter) n: any`) to marker `64`.
  - `./scripts/run-fourslash.sh --skip-ts-build --skip-cargo-build --max=200` remained `195/200` passing (no sampled regression).
  - `cargo nextest run -p tsz-lsp -p tsz-cli` passed (`1150` tests).

Investigated but punted:
- `TypeScript/tests/cases/fourslash/quickInfoContextualTyping.ts` (current failure at marker `64`: expected `(property) IBar.foo: IFoo`, got empty quick info).
  Reason: declaration-site/object-literal member quickinfo fallback for contextual `IBar` property names still misses this assignment path and needs a dedicated member-property resolver follow-up in tsserver `handlers_info`.

## 2026-02-23 (auto-import pnpm/symlink diagnostics+codefix follow-up)

Completed in this pass:
- Hardened tsserver missing-import fallback plumbing in `crates/tsz-cli/src/bin/tsz_server/handlers_code_fixes.rs` by:
  - scanning external project file content for synthetic missing-name viability,
  - adding side-effect-import fallback candidates (`import "pkg"`) for missing-name fixes,
  - adding external-project node_modules path fallback candidates (including pnpm path normalization) when file content is unavailable.
- Improved external-project path tracking in `crates/tsz-cli/src/bin/tsz_server/handlers_structure.rs` so `openExternalProject` keeps root file paths even when inline content is absent.
- Added focused unit coverage in `crates/tsz-cli/src/bin/tsz_server/handlers_code_fixes.rs` and `crates/tsz-cli/src/bin/tsz_server/tests.rs` for the above fallback paths and tracking behavior.
- Validation:
  - `cargo nextest run -p tsz-cli`
  - `cargo nextest run -p tsz-lsp -p tsz-cli`
  both pass in this run.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/autoImportPnpm.ts`: still returns `No codefixes returned`.
  Reason: despite handler-level fallback coverage, fourslash still does not surface an actionable missing-import codefix in this adapter path; remaining gap appears to be in SessionClient/adapter diagnostics→codefix request flow for virtual linked files, beyond a safe small server-side patch.
- `TypeScript/tests/cases/fourslash/autoImportSymlinkCaseSensitive.ts`: still returns `No codefixes returned`.
  Reason: appears to share the same adapter-level linked-file diagnostics/codefix request parity gap as `autoImportPnpm.ts`.

## 2026-02-23 (JSDoc annotate codefix ordering follow-up)

Completed in this pass:
- Fixed tsserver `getCodeFixes` placeholder ordering for JSDoc annotate parity in `crates/tsz-cli/src/bin/tsz_server/handlers_code_fixes.rs` by aligning synthetic `inferFromUsage` placeholder count to TypeScript-style `untypedParameterCount - 1` behavior.
- Added focused handler-level regression coverage in `crates/tsz-cli/src/bin/tsz_server/handlers_code_fixes.rs`:
  - `get_code_fixes_jsdoc_infer_placeholders_match_fourslash_order_24_26`
- Validation:
  - `cargo nextest run -p tsz-cli` passed (`424` tests).
  - `cargo nextest run -p tsz-lsp` passed (`734` tests).
  - `./scripts/run-fourslash.sh --max=200` improved from `193/200` to `196/200` passing.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/addFunctionInDuplicatedConstructorClassBody.ts`
  Reason: remaining mismatch is diagnostics cardinality/shape in constructor-body duplicate-member scenarios, which needs checker+diagnostics parity work beyond this targeted codefix-ordering patch.
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_types.ts` and `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_values.ts`
  Reason: still require broader cross-file rename/definition/reference bridge parity for arbitrary module namespace identifiers.
- `TypeScript/tests/cases/fourslash/autoImportVerbatimCJS1.ts`
  Reason: still needs deeper CommonJS/export-equals auto-import completion + codefix synthesis parity (`import = require(...)` style candidates/edits).

## 2026-02-23 (duplicate-constructor diagnostic parity follow-up)

Completed in this pass:
- Fixed `TypeScript/tests/cases/fourslash/addFunctionInDuplicatedConstructorClassBody.ts` by tightening synthetic missing-name detection in `crates/tsz-cli/src/bin/tsz_server/handlers_code_fixes.rs` so method-like declarations (`fn() {}` / `fn(): T {}`) are not misclassified as unresolved call expressions.
- Added focused unit coverage in `crates/tsz-cli/src/bin/tsz_server/handlers_code_fixes.rs`:
  - `semantic_diagnostics_sync_does_not_add_missing_name_for_class_method_declaration`
  - extended `parse_identifier_call_expression_ignores_keywords` with method-declaration regression assertions.
- Verification:
  - `cargo nextest run -p tsz-cli parse_identifier_call_expression_ignores_keywords semantic_diagnostics_sync_does_not_add_missing_name_for_class_method_declaration` passed.
  - `./scripts/run-fourslash.sh --skip-ts-build --max=200` improved from `196/200` to `197/200` passing.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_types.ts`
  Reason: still requires broader project-backed cross-file definition/references/rename parity in tsserver bridge paths (current single-file binding in relevant request flow is insufficient).
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_values.ts`
  Reason: same cross-file namespace alias/re-export navigation parity gap as `_types`.
- `TypeScript/tests/cases/fourslash/autoImportVerbatimCJS1.ts`
  Reason: remaining failure still needs deeper CommonJS `export =`/`import = require(...)` completion + codefix edit synthesis parity.

## 2026-02-23 (module-namespace resolver traversal follow-up)

Completed in this pass:
- Fixed resolver reference traversal in `crates/tsz-lsp/src/resolver/children.rs` to visit import/export clause/specifier nodes (`IMPORT_CLAUSE`, `NAMED_IMPORTS`, `NAMED_EXPORTS`, `NAMESPACE_IMPORT`, `IMPORT_SPECIFIER`, `EXPORT_SPECIFIER`) instead of skipping those subtrees.
- Extended module-namespace string-literal symbol lookup fallback in `crates/tsz-lsp/src/resolver/mod.rs` so quoted import/export specifier names can resolve through specifier name/property symbols when the parent node symbol is absent.
- Added focused resolver unit coverage in `crates/tsz-lsp/tests/resolver_tests.rs`:
  - `test_find_references_includes_module_namespace_string_literals`
- Verification:
  - `cargo nextest run -p tsz-lsp` passed (`735` tests).
  - `./scripts/run-fourslash.sh --skip-ts-build --skip-cargo-build --max=200` remained `197/200` (no sampled regressions).

Investigated but punted:
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_types.ts`
  Reason: remaining parity gap requires project-wide tsserver definition/references/rename behavior for quoted module namespace identifiers; current handlers still bind/query a single file per request.
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_values.ts`
  Reason: same cross-file tsserver request-shape/aggregation limitation as `_types` for string-literal module namespace identifiers.
- `TypeScript/tests/cases/fourslash/autoImportVerbatimCJS1.ts`
  Reason: still requires deeper CommonJS/`export =` auto-import completion candidate modeling and `import = require(...)` edit synthesis parity.

## 2026-02-23 (autoImportVerbatimCJS1 fallback follow-up)

Completed in this pass:
- Fixed `TypeScript/tests/cases/fourslash/autoImportVerbatimCJS1.ts` by adding tsserver-side fallback auto-import candidate synthesis for CommonJS `module.exports = { ... }` sources and ambient-module `export =` declarations in:
  - `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs` (completion entry + completion details code-action edits with `import x = require(...)` and member-access insert text),
  - `crates/tsz-cli/src/bin/tsz_server/handlers_code_fixes.rs` (`getCodeFixes` fallback rewrite for unresolved member names to `alias.member` plus `import = require` insertion).
- Added focused tsserver unit coverage in `crates/tsz-cli/src/bin/tsz_server/tests.rs`:
  - `test_completion_info_verbatim_commonjs_auto_imports_include_require_member_forms`
  - `test_get_code_fixes_verbatim_commonjs_fallback_rewrites_missing_member`
- Verification:
  - `cargo nextest run -p tsz-cli test_completion_info_verbatim_commonjs_auto_imports_include_require_member_forms test_get_code_fixes_verbatim_commonjs_fallback_rewrites_missing_member` passes.
  - `./scripts/run-fourslash.sh --skip-ts-build --filter=autoImportVerbatimCJS1 --verbose` now passes.
  - `./scripts/run-fourslash.sh --skip-ts-build --skip-cargo-build --max=200` improved from `197/200` to `198/200` passing in this run.
  - `cargo nextest run -p tsz-lsp` passes (`735` tests).

Investigated but punted:
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_types.ts`
  Reason: still requires project-wide cross-file definition/references/rename grouping parity in tsserver namespace-identifier request paths (current request handling remains single-file oriented in this sampled path).
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_values.ts`
  Reason: same cross-file tsserver namespace alias/re-export navigation parity gap as `_types`, needing broader project-backed request aggregation.

## 2026-02-23 (quick info contextual typing follow-up)

Completed in this pass:
- Fixed contextual object-literal property-name hover when the object literal appears on the RHS of a property-assignment binary expression (e.g. `holder.t12 = { foo: ... }`) by deriving container type from the assignment target in `crates/tsz-lsp/src/hover_contextual.rs`.
- Added focused hover unit coverage in `crates/tsz-lsp/tests/hover_tests.rs`:
  - contextual object-literal property hover in assignment context,
  - contextual parameter hover for function expressions passed as call arguments.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/quickInfoContextualTyping.ts`: still fails at marker `73` (`(parameter) n: any` vs `(parameter) n: number`) after fixing marker `64`.
  Reason: remaining gap is contextual typing for nested returned function expressions (`return function(n) { ... }`) and needs a broader contextual-call-signature propagation path than this targeted assignment/call-argument fix.
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_types.ts` and `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_values.ts`: still fail in `--max=200` run.
  Reason: requires project-wide go-to-definition/references/rename parity for quoted import/export names across files; current tsserver handler path remains file-local for these operations.

## 2026-02-23 (namespace alias definition follow-up)

Completed in this pass:
- Added a targeted tsserver alias-definition fallback in `crates/tsz-cli/src/bin/tsz_server/handlers_info.rs` that resolves quoted import/export alias markers through export alias chains to canonical declaration locations before rendering `definition`/`definitionAndBoundSpan` responses.
- Added focused tsserver unit coverage in `crates/tsz-cli/src/bin/tsz_server/tests.rs` (`test_alias_string_literal_navigation_uses_project_wide_resolution`) for canonical alias resolution and cross-file definition response expectations.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_types.ts`
  Reason: remaining mismatch is in `baselineRename`/`baselineFindAllReferences` fan-out for quoted alias chains; full parity needs alias-aware cross-file reference/rename symbol unification beyond this definition-targeted fix.
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_values.ts`
  Reason: same remaining cross-file alias reference/rename aggregation gap as `_types`; current run still reports a local baseline diff even after canonical definition resolution improvements.

## 2026-02-23 (quoted specifier project-name extraction follow-up)

Completed in this pass:
- Updated project import/export name extraction to treat `StringLiteral` specifier names the same as identifiers in:
  - `crates/tsz-lsp/src/project/mod.rs`
  - `crates/tsz-lsp/src/project/operations.rs`
- Added focused project-level unit coverage in `crates/tsz-lsp/tests/project_tests.rs`:
  - `test_project_cross_file_references_quoted_export_name`
- Added tsserver handler guardrails in `crates/tsz-cli/src/bin/tsz_server/handlers_info.rs` for quoted import/export specifier detection and rename-location filtering hooks.

Verification:
- `cargo nextest run -p tsz-lsp -E 'test(test_project_cross_file_references_quoted_export_name)'` passed.
- `cargo nextest run -p tsz-cli -E 'test(test_alias_string_literal_navigation_uses_project_wide_resolution)'` passed.
- `./scripts/run-fourslash.sh --max=200` remained `198/200` (no sampled improvement, no sampled regression).

Investigated but punted:
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_types.ts`
  Reason: remaining baseline gaps are still in tsserver `baselineRename`/`baselineFindAllReferences` semantics for quoted alias chains (`references-full`/rename symbol grouping) and require deeper cross-file alias-symbol unification in handler/protocol shaping.
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_values.ts`
  Reason: same unresolved tsserver cross-file alias reference/rename aggregation mismatch as `_types`; current project-level string-literal extraction improvements were necessary but not sufficient for full fourslash parity.

## 2026-02-23 (quoted alias definition span follow-up)

Completed in this pass:
- Added canonical-definition fallback for local quoted export aliases in `crates/tsz-cli/src/bin/tsz_server/handlers_info.rs` (`export { foo as "__<alias>" }` now canonicalizes through the local-side token when module specifier is absent).
- Fixed `definitionAndBoundSpan` canonical-alias fallback to return a non-empty token span for quoted import/export specifier queries instead of a zero-length cursor span.
- Added focused tsserver unit coverage in `crates/tsz-cli/src/bin/tsz_server/tests.rs`:
  - `test_definition_and_bound_span_quoted_local_export_alias_has_token_span`

Investigated but punted:
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_types.ts`
  Reason: still fails `baselineRename`/`baselineFindAllReferences`; requires alias-aware cross-file symbol grouping in `references-full`/rename response shaping beyond canonical definition/span fixes.
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_values.ts`
  Reason: same remaining tsserver alias-chain grouping gap as `_types`; current pass improved canonical definition/span behavior only.

## 2026-02-23 (quoted alias marker-offset probing follow-up)

Completed in this pass:
- Added quoted-specifier query offset probing in `crates/tsz-cli/src/bin/tsz_server/handlers_info.rs` so tsserver definition/references/rename handlers can shift from marker-adjacent offsets to nearby quoted import/export specifier tokens on the same line.
- Added quoted alias-chain aggregation helper logic in `crates/tsz-cli/src/bin/tsz_server/handlers_info.rs`:
  - canonical + direct project reference merge for quoted specifier queries,
  - quoted literal alias-closure discovery for string-literal re-export chains,
  - quoted import/export string-literal location collection.
- Added focused tsserver unit coverage in `crates/tsz-cli/src/bin/tsz_server/tests.rs`:
  - `test_quoted_alias_chain_references_and_rename_stay_on_quoted_specifiers`

Verification:
- `cargo nextest run -p tsz-cli test_alias_string_literal_navigation_uses_project_wide_resolution test_definition_and_bound_span_quoted_local_export_alias_has_token_span test_quoted_alias_chain_references_and_rename_stay_on_quoted_specifiers` passed.
- `./scripts/run-fourslash.sh --skip-build --max=200` remained `198/200` (no sampled regression, no sampled improvement).

Investigated but punted:
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_types.ts`
  Reason: still resolves rename/findReferences through local alias symbols (`foo`/`first`) instead of quoted export-name symbol identity under fourslash marker flows; full parity needs deeper symbol identity modeling for quoted import/export specifiers, not only handler-level offset probing and post-filtering.
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_values.ts`
  Reason: same unresolved quoted export-name symbol identity gap as `_types`; handler-level quoted-location aggregation improves targeted tsserver behavior but does not yet match fourslash baseline defId/context grouping semantics.

## 2026-02-23 (quoted alias chain references follow-up)

Completed in this pass:
- Hardened quoted import/export specifier detection in `crates/tsz-cli/src/bin/tsz_server/handlers_info.rs` to derive quoted names from enclosing import/export specifier nodes, not just the cursor token.
- Removed over-restrictive quoted-only filtering from quoted alias-chain reference aggregation so linked local alias usages can participate in `references`/`rename` results.
- Added focused unit coverage in `crates/tsz-cli/src/bin/tsz_server/tests.rs`:
  - `test_type_only_quoted_alias_references_work_from_type_keyword_offset`
  - updated `test_quoted_alias_chain_references_and_rename_stay_on_quoted_specifiers` to assert local alias usage participation.
- Validation:
  - `cargo nextest run -p tsz-cli test_type_only_quoted_alias_references_work_from_type_keyword_offset test_quoted_alias_chain_references_and_rename_stay_on_quoted_specifiers` passes.
  - `./scripts/run-fourslash.sh --skip-ts-build --skip-cargo-build --max=200` remains `198/200` (no sampled regression, no sampled improvement).

Investigated but punted:
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_types.ts`
  Reason: still diverges in baseline rename/go-to-definition/find-all-references shaping (quoted token span normalization + canonical symbol identity/defId grouping) and needs deeper tsserver/fourslash parity work beyond this offset/aggregation patch.
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_values.ts`
  Reason: same remaining parity gap as `_types`; current fixes improve handler robustness but do not yet align baseline symbol identity and range/rendering semantics.

## 2026-02-23 (quoted type-only alias definition probe follow-up)

Completed in this pass:
- Improved quoted alias target extraction in `crates/tsz-cli/src/bin/tsz_server/handlers_info.rs` by:
  - probing adjacent AST offsets for import/export specifier discovery,
  - accepting string-literal specifier names from either `name` or `property_name`,
  - adding a textual fallback parser for quoted import/export alias lines when AST node shape is incomplete.
- Tightened canonical alias-definition resolution for local export aliases to return the precise local token span (`foo` in `export { foo as "..." }`) instead of broad declaration spans that could re-resolve to the local import alias.
- Added focused tsserver unit coverage in `crates/tsz-cli/src/bin/tsz_server/tests.rs`:
  - `test_definition_type_only_quoted_import_alias_resolves_to_exported_symbol`.

Verification:
- `cargo nextest run -p tsz-cli test_definition_type_only_quoted_import_alias_resolves_to_exported_symbol` passes.
- `./scripts/run-fourslash.sh --skip-build --max=200` remains `198/200` (no sampled regression, no sampled improvement).
- `cargo nextest run -p tsz-lsp` passes (`738` tests).

Investigated but punted:
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_types.ts`
  Reason: remaining baseline gaps still require deeper tsserver parity for quoted alias-chain `findAllReferences`/rename symbol identity grouping (beyond this definition-targeted fix).
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_values.ts`
  Reason: same unresolved quoted alias-chain references/rename grouping parity as `_types`; current pass only fixed a definition-path subtype.

## 2026-02-23 (quoted alias location-filter follow-up)

Completed in this pass:
- Tightened quoted alias location filtering in `crates/tsz-cli/src/bin/tsz_server/handlers_info.rs` so quoted-alias rename/reference aggregation only keeps actual string-literal import/export specifier nodes (not arbitrary offsets within specifier clauses).
- Restored quoted-only filtering in quoted-alias reference merge paths for `references` fallback handling.
- Added focused tsserver unit coverage in `crates/tsz-cli/src/bin/tsz_server/tests.rs`:
  - `test_rename_from_export_quoted_alias_filters_non_specifier_locations`.

Verification:
- `cargo nextest run -p tsz-cli --bin tsz-server test_quoted_alias_chain_references_and_rename_stay_on_quoted_specifiers test_rename_from_export_quoted_alias_filters_non_specifier_locations test_type_only_quoted_alias_references_work_from_type_keyword_offset` passes.
- `./scripts/run-fourslash.sh --max=200` remains `198/200` (same two failures).

Investigated but punted:
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_types.ts`
  Reason: still needs deeper `references-full` quoted alias symbol-grouping/defId parity (multi-definition aggregation and detail shaping), beyond location filtering.
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_values.ts`
  Reason: same unresolved `references-full` quoted alias symbol-grouping parity as `_types`.

## 2026-02-23 (quoted alias references-full span normalization follow-up)

Completed in this pass:
- Tightened quoted import/export specifier location handling in `crates/tsz-cli/src/bin/tsz_server/handlers_info.rs` by normalizing string-literal spans to inner text ranges for quoted alias discovery helpers.
- Added alias-aware `references-full` fallback plumbing for quoted specifier queries and preserved quoted-only behavior for `references`/`rename` request paths.
- Added focused tsserver unit coverage in `crates/tsz-cli/src/bin/tsz_server/tests.rs`:
  - `test_references_full_quoted_alias_uses_inner_literal_span_and_cross_file_refs`.

Verification:
- `cargo nextest run -p tsz-cli test_quoted_alias_chain_references_and_rename_stay_on_quoted_specifiers test_references_full_quoted_alias_uses_inner_literal_span_and_cross_file_refs` passes.
- `./scripts/run-fourslash.sh --skip-ts-build --skip-cargo-build --max=200` remains `198/200` (no sampled regression, no sampled improvement).
- `cargo nextest run -p tsz-lsp` passes; `cargo nextest run -p tsz-cli -E 'not test(tsc_compat_)'` passes.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_types.ts`
  Reason: remaining baseline mismatch is still in tsserver `references-full` definition/detail shaping (defId/context grouping and canonical symbol identity for quoted alias chains), which needs a broader alias-symbol modeling pass than this targeted span/offset fix.
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_values.ts`
  Reason: same unresolved `references-full` alias-symbol identity/detail-shaping parity gap as `_types`.

## 2026-02-23 (call hierarchy tagged-template incoming follow-up)

Completed in this pass:
- Fixed call hierarchy incoming-call reference detection in `crates/tsz-lsp/src/hierarchy/call_hierarchy.rs` to treat `TaggedTemplateExpression` (`bar\`...\``) as a call-like reference context.
- Added focused unit coverage in `crates/tsz-lsp/tests/call_hierarchy_tests.rs`:
  - `test_incoming_calls_include_tagged_template_references`.

Verification:
- `cargo nextest run -p tsz-lsp` passes.
- `./scripts/run-fourslash.sh --filter=callHierarchyTaggedTemplate` now passes (`1/1`).
- `cargo nextest run -p tsz-cli` still has existing `tsc_compat_tests::*` failures in this environment; tsserver/unit suites remain passing.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/callHierarchyFunctionAmbiguity.2.ts`
  Reason: parity requires project-wide/multi-file call hierarchy symbol resolution for merged overload declarations (`a.d.ts` + `b.d.ts` + `main.ts`), while current tsserver call hierarchy path binds a single file per request.

## 2026-02-23 (quoted alias references-full fallback follow-up)

Completed in this pass:
- Tightened `references-full` quoted-alias fallback gating in `crates/tsz-cli/src/bin/tsz_server/handlers_info.rs` so quoted fallback only runs when symbol-based references are unavailable.
- Added alias-identifier discovery for quoted import/export chains and merged those identifier-based project references into quoted alias chain expansion.
- Added focused tsserver unit coverage in `crates/tsz-cli/src/bin/tsz_server/tests.rs`:
  - `test_references_full_quoted_alias_includes_symbol_alias_references_when_available`

Verification:
- `cargo nextest run -p tsz-cli test_references_full_quoted_alias_includes_symbol_alias_references_when_available test_references_full_quoted_alias_uses_inner_literal_span_and_cross_file_refs` passes.
- `./scripts/run-fourslash.sh --max=200 --skip-ts-build --skip-cargo-build` remains `198/200` (same two failures).
- `cargo nextest run -p tsz-lsp -p tsz-cli -E 'not test(tsc_compat_)'` passes (`1162` tests).

Investigated but punted:
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_types.ts`
  Reason: still needs tsserver `references-full` parity for quoted alias chains with multi-symbol definition grouping/detail rendering (`defId/contextId`-equivalent shape), beyond this fallback/identifier expansion patch.
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_values.ts`
  Reason: same remaining `references-full` multi-symbol grouping/detail-shaping gap as `_types`; current fix improves reference collection but not baseline-equivalent symbol detail aggregation.

## 2026-02-23 (arbitrary module namespace identifiers follow-up)

Completed in this pass:
- Improved resolver fallback for quoted import/export specifier string-literal symbol resolution in `crates/tsz-lsp/src/resolver/mod.rs` by allowing specifier-side identifier fallback lookup (`spec.name` then `spec.property_name`) with binder identifier resolution when node-symbol mapping is absent.
- Added focused resolver unit coverage in `crates/tsz-lsp/tests/resolver_tests.rs` for export-side quoted module-namespace alias resolution (`export { foo as "__<alias>" }`).
- Tightened tsserver quoted `references-full` alias fallback shaping in `crates/tsz-cli/src/bin/tsz_server/handlers_info.rs` and updated focused tsserver unit coverage in `crates/tsz-cli/src/bin/tsz_server/tests.rs` for multi-entry alias-reference payloads.

Investigated but punted:
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_types.ts`: still creates a new local baseline.
  Reason: remaining parity gap is in tsserver `references-full` payload semantics for arbitrary module namespace identifiers (definition grouping/metadata/detail rendering parity, not just reference discovery), and requires deeper alignment of referenced-symbol entry construction with TypeScript server behavior.
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_values.ts`: still creates a new local baseline.
  Reason: same remaining `references-full` grouping/detail parity gap as the types variant; current fixes improve internal resolver/test behavior but do not yet produce the exact fourslash baseline shape.

## 2026-02-23 (quoted alias rename context + references merge follow-up)

Completed in this pass:
- Tightened quoted-alias rename shaping in `crates/tsz-cli/src/bin/tsz_server/handlers_info.rs` to:
  - filter quoted rename locations to the queried literal token text,
  - include `contextStart/contextEnd` ranges for import/export statement wrapping.
- Tightened `references-full` fallback selection in `crates/tsz-cli/src/bin/tsz_server/handlers_info.rs` so symbol-based references are preferred when available, while still merging quoted alias-chain references into symbol-backed results.
- Extended quoted alias symbol-location harvesting in `crates/tsz-cli/src/bin/tsz_server/handlers_info_alias.rs` to include counterpart identifier spans (`bar`/`first`-style aliases) when matched quoted specifier literals are discovered.
- Added focused helper unit coverage in `crates/tsz-cli/src/bin/tsz_server/handlers_info_alias.rs`:
  - `import_statement_context_span_accepts_export_specifier_lines`.

Verification:
- `cargo nextest run -p tsz-cli test_rename_quoted_alias_marker_offset_uses_literal_only_locations test_references_full_quoted_alias_uses_inner_literal_span_and_cross_file_refs test_references_full_quoted_alias_includes_symbol_alias_references_when_available handlers_info_alias::tests::import_statement_context_span_accepts_export_specifier_lines` passes.
- `./scripts/run-fourslash.sh --max=200 --skip-build` remains `198/200` (same two failures).
- `cargo nextest run -p tsz-lsp -p tsz-cli -E 'not test(tsc_compat_)' --no-fail-fast` passes (`1164` tests).

Investigated but punted:
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_values.ts`
  Reason: remaining mismatch is still in `findAllReferences` symbol-group/detail serialization parity (`defId/contextId` grouping and definition payload shape), beyond this targeted rename/reference-location alignment.
- `TypeScript/tests/cases/fourslash/arbitraryModuleNamespaceIdentifiers_types.ts`
  Reason: same unresolved `references-full` definition/detail grouping parity as values variant; current changes improve span/alias coverage but do not yet match TypeScript’s referenced-symbol entry construction contract.
