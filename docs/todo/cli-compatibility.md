# CLI Compatibility: tsz vs tsc, tsz-server vs tsserver

This document tracks the CLI compatibility status between tsz/tsz-server and tsc/tsserver.

## tsz vs tsc

### Implemented (matching tsc)

| Feature | Status | Notes |
|---------|--------|-------|
| `--help` / `-h` | Done | Via clap |
| `--version` / `-V` | Done | Via clap |
| `-v` (lowercase) | Done | Preprocessed to -V for tsc compat |
| `--all` | Done | Shows all compiler options |
| `--init` | Done | Creates tsconfig.json |
| `--showConfig` | Done | Prints resolved configuration |
| `--listFilesOnly` | Done | Lists files and exits |
| `--build` / `-b` | Done | Basic build mode |
| `--watch` / `-w` | Done | File watching with recompilation |
| `--project` / `-p` | Done | Project path |
| `--locale` | Done | Messaging locale |
| `@file` response files | Done | Read args from file |
| Exit code 0 | Done | Success |
| Exit code 1 | Done | Errors, outputs skipped |
| Exit code 2 | Done | Errors, outputs generated |
| All compiler options | Done | 100+ flags via clap (see args.rs) |
| All strict checks | Done | --strict, --strictNullChecks, etc. |
| All emit options | Done | --outDir, --declaration, --sourceMap, etc. |
| All module options | Done | --module, --moduleResolution, --baseUrl, etc. |
| Build mode flags | Done | --verbose, --dry, --force, --clean, --stopBuildOnErrors |
| Watch mode flags | Done | --watchFile, --watchDirectory, --fallbackPolling, etc. |
| Deprecated flags | Done | --charset, --out, --keyofStringsOnly, etc. (hidden) |
| `--reactNamespace` | Done | Deprecated, hidden |
| `--disableSizeLimit` | Done | Editor support flag |
| tsconfig.json discovery | Done | Walk-up search, --project flag |
| `--pretty` | Done | Color output control |
| `--diagnostics` / `--extendedDiagnostics` | Done | Performance info |
| `--listFiles` / `--listEmittedFiles` | Done | File listing |
| `--explainFiles` | Partial | Lists files but not full inclusion reasons |
| `--traceResolution` | Done | Module resolution tracing |

### Not Yet Implemented (tsz)

| Feature | Priority | Notes |
|---------|----------|-------|
| `--generateTrace <dir>` | Medium | Event trace for performance analysis; prints warning but doesn't generate |
| `--generateCpuProfile <path>` | Low | V8 CPU profile; not applicable to Rust (would need Rust profiling equivalent) |
| `--explainFiles` full reasons | Medium | Currently just lists files; needs inclusion reason tracking (e.g., "Matched by include pattern", "Imported via...") |
| `--paths` on CLI | Low | Path mappings; usually set in tsconfig.json only; tsc technically accepts it |
| `--plugins` on CLI | Low | Language service plugins; usually tsconfig-only |
| Full `--build` project references | High | Build mode currently does basic compilation; full multi-project build graph, dependency ordering, and incremental builds across references are not yet implemented |
| `--noCheck` full integration | Medium | Flag is accepted but may not fully skip type checking in all code paths |
| `--diagnostics` format match | Low | Output format differs slightly from tsc (timing categories, memory stats) |
| `--showConfig` full output | Medium | Currently outputs a subset of options; should match tsc's full JSON output |
| `--init` template variations | Low | tsc's --init can be customized; tsz uses a fixed template |
| Error message format | Medium | Diagnostic messages may differ in wording/formatting from tsc |
| `--locale` i18n message loading | Low | Flag is accepted but messages are always in English |
| Watch mode file watcher strategy | Medium | Some watcher strategies may not be fully implemented |

## tsz-server vs tsserver

### Implemented (matching tsserver)

| Feature | Status | Notes |
|---------|--------|-------|
| **CLI Flags** | | |
| `--syntaxOnly` | Done | Accepted, maps to ServerMode::Syntactic |
| `--useSingleInferredProject` | Done | Accepted (not yet functionally wired) |
| `--useInferredProjectPerProjectRoot` | Done | Accepted (not yet functionally wired) |
| `--suppressDiagnosticEvents` | Done | Accepted |
| `--noGetErrOnBackgroundUpdate` | Done | Accepted |
| `--allowLocalPluginLoads` | Done | Accepted |
| `--canUseWatchEvents` | Done | Accepted |
| `--disableAutomaticTypingAcquisition` | Done | Accepted |
| `--enableTelemetry` | Done | Accepted |
| `--validateDefaultNpmLocation` | Done | Accepted |
| `--cancellationPipeName` | Done | Accepted (not yet functionally wired) |
| `--serverMode` | Done | semantic/partialSemantic/syntactic |
| `--eventPort` | Done | Accepted (not yet functionally wired) |
| `--locale` | Done | Accepted |
| `--globalPlugins` | Done | Accepted (comma-separated) |
| `--pluginProbeLocations` | Done | Accepted (comma-separated) |
| `--logVerbosity` | Done | off/terse/normal/requestTime/verbose |
| `--logFile` | Done | Log file path |
| `--traceDirectory` | Done | Accepted |
| `--npmLocation` | Done | Accepted |
| `--enableProjectWideIntelliSenseOnWeb` | Done | Accepted |
| `--useNodeIpc` | Done | Accepted (not yet functionally wired) |
| **Environment Variables** | | |
| `TSS_LOG` | Done | Parsed: -level, -file, -traceToConsole |
| `TSS_DEBUG` | Done | Detected and logged |
| `TSS_DEBUG_BRK` | Done | Detected and logged |
| `TSZ_LIB_DIR` | Done | Custom lib directory override |
| **Wire Protocol** | | |
| Content-Length framing | Done | Default tsserver protocol mode |
| JSON-per-line (legacy) | Done | Via `--protocol legacy` |
| Request/Response/Event types | Done | Proper seq, type, command fields |
| **Protocol Commands** | | |
| `open` | Done | Opens file, stores content |
| `close` | Done | Removes file from open set |
| `configure` | Done | Accepted (config not yet applied) |
| `updateOpen` | Done | Batch open/change/close |
| `semanticDiagnosticsSync` | Done | Returns type errors for open files |
| `syntacticDiagnosticsSync` | Done | Returns parse errors for open files |
| `exit` | Done | Clean shutdown |
| **Legacy Protocol** | | |
| `check` command | Done | Full multi-file type checking |
| `status` command | Done | Memory, check count, lib cache |
| `recycle` command | Done | Clear caches |
| `shutdown` command | Done | Graceful shutdown |
| **CheckOptions** | Done | All tsc compiler options accepted |

### Not Yet Implemented (tsz-server)

#### High Priority

| Feature | Notes |
|---------|-------|
| `geterr` async diagnostics | tsserver fires `syntaxDiag`, `semanticDiag`, `suggestionDiag` events asynchronously; tsz-server acknowledges but doesn't fire events |
| `geterrForProject` | Same as above, project-wide |
| Full project management | tsserver manages Configured, External, and Inferred projects; tsz-server has no project concept |
| tsconfig.json discovery | tsserver walks up directories to find tsconfig.json for open files |
| File change tracking | tsserver tracks incremental edits (insertions, deletions); tsz-server requires full file content |
| `definition` / `typeDefinition` | Go-to-definition with actual position resolution |
| `references` | Find-all-references with actual position resolution |
| `quickinfo` | Hover information with type and documentation |
| `completions` / `completionInfo` | Auto-complete suggestions |
| `completionEntryDetails` | Detailed completion item info |

#### Medium Priority

| Feature | Notes |
|---------|-------|
| `signatureHelp` | Function signature information |
| `documentHighlights` | Symbol occurrence highlighting |
| `rename` | Symbol rename across files |
| `navtree` / `navbar` | Navigation tree/bar structures |
| `getCodeFixes` | Quick-fix suggestions |
| `getCombinedCodeFix` | Fix-all for an error kind |
| `getApplicableRefactors` | Available refactorings |
| `getEditsForRefactor` | Refactoring edit computation |
| `organizeImports` | Auto-organize imports |
| `format` / `formatonkey` | Code formatting |
| `inlayHints` | Inline type annotations |
| `selectionRange` | Smart selection ranges |
| `prepareCallHierarchy` / incoming/outgoing calls | Call hierarchy |
| `suggestionDiagnosticsSync` | Code suggestion diagnostics |
| Request cancellation via named pipes | `--cancellationPipeName` semaphore mechanism |
| Event port delivery | `--eventPort` TCP event delivery |
| Node IPC communication | `--useNodeIpc` channel |

#### Low Priority

| Feature | Notes |
|---------|-------|
| `projectInfo` | Return actual project configuration |
| `compilerOptionsForInferredProjects` | Set options for inferred projects |
| `openExternalProject` / `closeExternalProject` | External project management |
| `linkedEditingRange` | Synchronized editing ranges |
| `mapCode` | Code mapping/insertion |
| `fileReferences` | Cross-file reference search |
| `implementation` | Find implementations |
| `getOutliningSpans` | Code folding regions |
| `getEditsForFileRename` | File rename edit computation |
| `getMoveToRefactoringFileSuggestions` | Move-to-file refactor suggestions |
| Automatic Type Acquisition (ATA) | Auto-download @types packages; `--disableAutomaticTypingAcquisition` accepted but ATA itself not implemented |
| Plugin system | `--globalPlugins`, `--pluginProbeLocations`, `--allowLocalPluginLoads` flags accepted but plugin loading not implemented |
| Telemetry events | `--enableTelemetry` accepted but no events emitted |
| `TSS_DEBUG` / `TSS_DEBUG_BRK` attach | Env vars detected but no debugger support (Rust doesn't use V8 debugger) |
| Full TSS_LOG file writing | TSS_LOG parsed and logged to stderr but not written to the specified file |
| Multi-server architecture | VS Code spawns multiple tsserver processes (Main, Syntax, Semantic, Diagnostics); tsz-server is a single process |
| `requestCompleted` events | Sent after async request completion in tsserver |
| `projectsUpdatedInBackground` events | Background project update notifications |

## Architecture Differences

### tsz vs tsc
- **Language**: tsz is Rust; tsc is TypeScript/JavaScript running on Node.js
- **Performance**: tsz aims for native speed advantage
- **Conformance**: Currently at 39.2% of TypeScript conformance tests
- **Emit**: tsz has a full emitter; some edge cases may differ from tsc

### tsz-server vs tsserver
- **Protocol modes**: tsz-server supports both tsserver Content-Length protocol (default) and a legacy JSON-per-line mode for conformance testing
- **Project management**: tsserver maintains a full project graph with Configured, External, and Inferred projects; tsz-server does per-request type checking
- **Incremental**: tsserver caches parsed/bound files and does incremental checking; tsz-server caches lib files but re-parses user files per request
- **Language service**: tsserver provides the full IDE experience (completions, go-to-definition, refactoring, etc.); tsz-server currently focuses on diagnostics
- **Multi-process**: VS Code runs multiple tsserver instances; tsz-server is single-process
