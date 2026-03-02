# tsserver Compatibility — Remaining Work

## Status
tsz-server implements ~90% of tsserver's public commands. The fourslash test suite
passes 2,540/2,540, but some commands return empty stubs rather than real results.

## P0 — Stubbed commands with existing infrastructure to support them

- [x] `implementation` / `implementation-full` — already wired to `GoToImplementationProvider`.
- [x] `fileReferences` — wired to `Project::get_file_dependents()` via dependency graph.
- [x] `linkedEditingRange` — wired to `LinkedEditingProvider` for JSX tag pairs.
- [x] `getSmartSelectionRange` — wired to `SelectionRangeProvider` (same as `selectionRange`).
- [x] `projectInfo` — returns real tsconfig/jsconfig path via directory walk + open file list.
- [x] `compilerOptionsForInferredProjects` — stores options and applies them.
- [ ] `getCompilerOptionsDiagnostics` — returns `[]`. Should validate tsconfig options and report errors.
- [ ] `applyCodeActionCommand` — minimal stub. Should execute code action commands (e.g., add missing import).

## P1 — Stubbed commands needing new infrastructure

- [ ] `getApplicableRefactors` / `getEditsForRefactor` — returns `[]`. Needs refactoring engine (extract function/variable/type, convert, move).
- [ ] `organizeImports` — returns `[]`. Needs import analysis (sort, remove unused, group).
- [ ] `getEditsForFileRename` — returns `[]`. Needs cross-file import path rewriting.
- [ ] `emitOutput` — returns `{"outputFiles":[],"emitSkipped":true}`. Needs emit pipeline.
- [ ] `getSyntacticClassifications` — returns `[]`. Needs token-level classification.
- [ ] `getSemanticClassifications` — returns `[]`. Needs type-aware classification.

## P2 — Low priority / edge-case commands

- [ ] `getMoveToRefactoringFileSuggestions` — returns empty. Needs move-to-file refactoring.
- [ ] `preparePasteEdits` / `getPasteEdits` — returns empty. VS Code paste-with-imports feature.
- [ ] `configurePlugin` — no-op. No plugin system.
- [ ] `mapCode` — returns `[]`. Code mapping / source maps.

## Protocol commands (dispatched, minimal responses)

- [x] `reload` / `reloadProjects` — clears caches, re-reads open files from disk.
- [x] `status` — returns server version.
- [x] `compileOnSaveAffectedFileList` / `compileOnSaveEmitFile` — stub responses (no emit pipeline).
- [x] `saveto` — protocol-compatible no-op.
- [x] `watchChange` — protocol-compatible no-op.

## Not planned

- `-full` internal variants (20 commands) — editor adapters don't use these directly.
- `copilotRelated` — AI integration, not relevant.
