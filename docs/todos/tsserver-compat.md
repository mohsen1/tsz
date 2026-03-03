# tsserver Compatibility — Remaining Work

## Status
tsz-server implements all tsserver public commands. The fourslash test suite
passes 2,540/2,540. All commands return real results or protocol-compatible
responses.

## P0 — Stubbed commands with existing infrastructure to support them

- [x] `implementation` / `implementation-full` — already wired to `GoToImplementationProvider`.
- [x] `fileReferences` — wired to `Project::get_file_dependents()` via dependency graph.
- [x] `linkedEditingRange` — wired to `LinkedEditingProvider` for JSX tag pairs.
- [x] `getSmartSelectionRange` — wired to `SelectionRangeProvider` (same as `selectionRange`).
- [x] `projectInfo` — returns real tsconfig/jsconfig path via directory walk + open file list.
- [x] `compilerOptionsForInferredProjects` — stores options and applies them.
- [x] `getCompilerOptionsDiagnostics` — validates tsconfig JSON and reports parse errors.
- [x] `applyCodeActionCommand` — returns success acknowledgment (tsc uses this for deferred commands like "install types").

## P1 — Stubbed commands needing new infrastructure

- [x] `getApplicableRefactors` / `getEditsForRefactor` — wired to `CodeActionProvider::extract_variable()`.
- [x] `organizeImports` — wired to `CodeActionProvider::organize_imports()` with case sensitivity option.
- [x] `getEditsForFileRename` — scans open files for imports referencing renamed file and rewrites paths.
- [x] `emitOutput` — real JS output via `Printer` from tsz-emitter.
- [x] `getSyntacticClassifications` — scanner-based token classification (string, number, keyword, etc.).
- [x] `getSemanticClassifications` — decoded from `SemanticTokensProvider` delta encoding.

## P2 — Edge-case commands

- [x] `getMoveToRefactoringFileSuggestions` — scans open/project files filtered by extension, suggests new filename.
- [x] `preparePasteEdits` / `getPasteEdits` — paste-with-imports: detects exports from source file, generates import statements in target.
- [x] `configurePlugin` — stores plugin configuration in server for future plugin system use.
- [x] `mapCode` — inserts code snippets at focus location or end of file.

## Protocol commands (dispatched, minimal responses)

- [x] `reload` / `reloadProjects` — clears caches, re-reads open files from disk.
- [x] `status` — returns server version.
- [x] `compileOnSaveAffectedFileList` / `compileOnSaveEmitFile` — stub responses (no emit pipeline).
- [x] `saveto` — protocol-compatible no-op.
- [x] `watchChange` — protocol-compatible no-op.

## Not planned

- `-full` internal variants (20 commands) — editor adapters don't use these directly.
- `copilotRelated` — AI integration, not relevant.
