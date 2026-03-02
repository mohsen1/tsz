# tsserver Compatibility — Remaining Work

## Status
tsz-server implements ~80% of tsserver's public commands. The fourslash test suite
passes 2,540/2,540, but many commands return empty stubs rather than real results.

## P0 — Stubbed commands with existing infrastructure to support them

These return empty today but tsz already has the underlying logic (or close to it).

- [ ] `implementation` / `implementation-full` — returns `[]`. Should use definition/reference infrastructure to find implementations of interfaces/abstract members.
- [ ] `fileReferences` — returns `{"refs":[],"symbolName":""}`. Should scan project imports to find files that reference a given file.
- [ ] `linkedEditingRange` — returns `None`. Should detect JSX tag pairs and return linked ranges for simultaneous rename.
- [ ] `getSmartSelectionRange` — returns `[]`. Should walk AST to expand selection semantically (expression → statement → block → function).
- [ ] `projectInfo` — returns `{"configFileName":"","fileNames":[]}`. Should return actual tsconfig path and file list.
- [ ] `compilerOptionsForInferredProjects` — returns `true`. Should store options and apply them to inferred projects.
- [ ] `getCompilerOptionsDiagnostics` — returns `[]`. Should validate tsconfig options and report errors.
- [ ] `applyCodeActionCommand` — returns `[]`. Should execute code action commands (e.g., add missing import).

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

## Missing commands (not dispatched at all)

- [ ] `reload` / `reloadProjects` — project reload.
- [ ] `compileOnSaveAffectedFileList` / `compileOnSaveEmitFile` — compile-on-save workflow.
- [ ] `saveto` — save file to location.
- [ ] `status` — server status.
- [ ] `applyChangedToOpenFiles` — batch file changes (older protocol).
- [ ] `watchChange` — file watch event notification.

## Not planned

- `-full` internal variants (20 commands) — editor adapters don't use these directly.
- `copilotRelated` — AI integration, not relevant.
