# Session tsz-3: LSP Implementation - Dynamic Import Support

**Started**: 2026-02-05
**Status**: 🔄 ACTIVE
**Previous Session**: CFA Stabilization (Blocked - architectural conflicts)

## Goal

Implement LSP features that improve developer experience without requiring deep Solver/Checker architecture expertise.

## Completed Work

### File Rename: Dynamic Import Support (2026-02-05)
**Status**: ✅ COMPLETE

Added support for dynamic imports (`import()`) and `require()` calls to File Rename functionality.

**Implementation** (`src/lsp/file_rename.rs`):
- Added `try_add_call_expression()` to detect `CallExpression` nodes
- Added `is_import_keyword()` to detect `import()` expressions (uses `ImportKeyword`)
- Added `is_require_identifier()` to detect `require()` calls (identifier with text "require")
- Updated `find_import_specifier_nodes()` to scan for `CALL_EXPRESSION` nodes
- Reuses logic from `document_links.rs` for consistency

**Implementation** (`src/lsp/project.rs`):
- Updated `extract_imports()` to handle dynamic imports for DependencyGraph
- Added `try_extract_dynamic_import()` helper to extract specifiers from call expressions
- Added `is_import_keyword()` and `is_require_identifier()` helpers (non-Option returning versions)

**Tests Added (3 new tests)**:
1. `test_dynamic_import_updates` - Verifies `import("./module")` is updated on rename
2. `test_require_call_updates` - Verifies `require("./module")` is updated on rename
3. `test_mixed_imports_and_dynamic` - Verifies all import types work together

**Total Tests**: 11 passing, 1 ignored

**Value**: File Rename now handles static imports, re-exports, dynamic imports, and require calls.

### File Rename Testing (2026-02-05)
**Status**: ✅ COMPLETE - Commit 52e9da990

Wrote comprehensive test suite for File Rename functionality (`src/lsp/tests/file_rename_tests.rs`).

**Tests Implemented (8 passing, 1 ignored)**:
1. `test_single_file_rename_updates_imports` - Basic file rename
2. `test_directory_rename_updates_imports` - Directory rename (recursive)
3. `test_nested_directory_rename` - Nested directory structure
4. `test_sibling_directory_rename` - Sibling directory with `../` imports
5. `test_reexport_updates_on_rename` - Re-export chains
6. `test_extensionless_import_updates` - Extensionless imports
7. `test_no_edit_for_unrelated_imports` - Filtering works correctly
8. `test_dot_slash_prefix_preserved` - Import style preservation
9. `test_directory_with_index_file` - IGNORED (requires directory-to-index module resolution)

**Bugs Fixed During Testing**:
1. **AST Walk Bug**: `find_import_specifier_nodes()` was only checking the root node instead of iterating all nodes in the arena
2. **Directory Rename Bug**: Was passing directory path instead of actual file paths to `process_file_rename()`
3. **Path Normalization**: Added `normalize_path()` helper to properly resolve `.` and `..` components in paths

**Known Limitations**:
- Directory-to-index resolution (e.g., `./utils` → `./utils/index.ts`) requires full module resolution logic
- This is a complex edge case that depends on TypeScript's module resolution algorithm

### File Rename Handling (2026-02-05)
**Status**: ✅ COMPLETE - Commits c0e1bec5a, 9041b49b5, 460b19435, cdf4c3b78

Implemented full `workspace/willRenameFiles` support:
- Phase 1: Path utilities (c0e1bec5a)
- Phase 2: FileRenameProvider (9041b49b5)
- Phase 3: Project orchestration (460b19435)
- Phase 4: Directory renames (cdf4c3b78)

**Implementation**:
- Added `dependency_graph` field to Project struct
- `extract_imports()` handles imports AND re-exports
- `is_import_pointing_to_file()` filters imports correctly with path normalization
- `normalize_path()` helper resolves `.` and `..` components
- `process_file_rename()` iterates through all files to find dependents
- Directory renames expand to individual file renames with proper path computation

**Value**: Renaming files or directories now correctly updates all imports across the project.

## Current Work: Auto-Import Completions

**Goal**: Implement Auto-Import Completions - the most requested LSP feature. When a user types an unresolved name, suggest completions that automatically add the import statement.

**Status**: ✅ COMPLETE

**Implementation** (Steps 1, 2, 4 complete; Step 3 deferred for performance optimization):

### Step 1: Updated `CompletionItem` struct (`src/lsp/completions.rs`)
- Added `additional_text_edits: Option<Vec<TextEdit>>` field
- Added `with_additional_edits()` builder method
- Added custom deserializer (completion items are server→client only, TextEdit doesn't implement Deserialize)

### Step 2: Refactored edit generation (`src/lsp/code_actions.rs`)
- Added `build_auto_import_edit()` public method to `CodeActionProvider`
- Wraps existing `build_import_edit()` logic

### Step 3: SymbolIndex optimization (COMPLETE)
- Added `symbol_index: SymbolIndex` field to `Project` struct
- Integrated into file lifecycle: `set_file`, `update_file`, `remove_file`
- Optimized `collect_import_candidates_for_name` to use symbol_index for named exports
- Added smart fallback for default/namespace exports (where import name can differ from export name)
- Performance: O(N) → O(1) for named exports (where N = number of files)
- All 65 project tests passing

### Step 4: Wired up auto-import edits in `Project::get_completions` (`src/lsp/project.rs`)
- Creates `CodeActionProvider` for the file
- Calls `build_auto_import_edit()` to generate `Vec<TextEdit>`
- Attaches edits to completion item via `with_additional_edits()`

**Testing**:
- ✅ Existing test `test_project_completions_auto_import_named` verifies auto-import functionality
- ✅ Updated test to verify `additionalTextEdits` are present on completion items
- ✅ All 65 project tests passing

**Value**: When users type an undefined name, completions now include auto-import suggestions that automatically insert the import statement when accepted. Performance is optimized for the common case (named exports).

## Completed Work Summary

### SymbolIndex Integration (2026-02-05)
**Status**: ✅ COMPLETE - Commit 1dc6d3f38

Integrated `SymbolIndex` into `Project` for O(1) auto-import candidate lookup.

**Implementation** (`src/lsp/project.rs`):
- Added `symbol_index: SymbolIndex` field to `Project` struct
- Updated `set_file()` to call `symbol_index.index_file()`
- Updated `update_file()` to call `symbol_index.update_file()`
- Updated `remove_file()` to call `symbol_index.remove_file()`

**Implementation** (`src/lsp/project_operations.rs`):
- Optimized `collect_import_candidates_for_name()` to use symbol_index for named exports
- Added smart fallback for default/namespace exports (where import name can differ from export name)
- For named exports: O(N) → O(1) lookup via `symbol_index.get_files_with_symbol()`
- For default/namespace exports: Falls back to checking all files (necessary for correctness)

**Testing**:
- ✅ All 65 project tests passing
- ✅ Auto-import tests verify `additionalTextEdits` are generated
- ✅ Default export and re-export tests work correctly

**Value**: Auto-import completions now scale efficiently to large projects for the common case (named exports).

### Workspace Symbols (2026-02-05)
**Status**: ✅ COMPLETE - Commit 93e197ef8

Added `Project::get_workspace_symbols()` method to enable the `workspace/symbol` LSP request (Cmd+T / Ctrl+T in editors).

**Implementation** (`src/lsp/project.rs`):
- Added `get_workspace_symbols(query: &str) -> Vec<SymbolInformation>` method
- Uses existing `WorkspaceSymbolsProvider` and `SymbolIndex`
- O(S) search where S = number of unique symbols across project
- Results sorted by relevance: exact > prefix > substring (case-insensitive)
- Maximum 100 results returned

**Value**: Users can now instantly search across all symbols in their project (functions, classes, interfaces, constants, etc.) using the SymbolIndex that was already integrated.

## Session Status

**Status**: 🔄 ACTIVE - Ready for next feature

**Completed LSP Features** (all working with SymbolIndex optimization):
- ✅ File Rename (with directory support, dynamic imports, and require calls)
- ✅ Auto-Import Completions (with additionalTextEdits and O(1) lookup for named exports)
- ✅ JSX Linked Editing
- ✅ SymbolIndex integration (O(1) auto-import candidate lookup)
- ✅ Workspace Symbols (project-wide symbol search via Cmd+T / Ctrl+T)

**Next Options** (per Gemini consultation):
1. **Add Prefix Matching** - Suggest completions for partial matches (e.g., `Lis` → `List`)
2. **Index Transitive Re-exports** - Enhance SymbolIndex to track wildcard reexports (`export * from './mod'`)
3. **Deep Indexing** - Enhance SymbolIndex to track nested symbols (class members, interface members)
4. **Move to different session** - Solver/Checker work, coordination work, or other LSP features

**Previous Blocked Work** (CFA):
- Assertion functions narrowing - COMPLETE (safe to keep)
- Truthiness narrowing fix - REVERTED (breaks circular extends)
- Requires deep Solver architecture expertise to resolve circular dependency issue

### JSX Linked Editing (2026-02-05)
**Status**: ✅ COMPLETE - Commit e5f6bcee7

Implemented `textDocument/linkedEditingRange` for JSX/TSX files.
When editing an opening JSX tag (e.g., `<div>`), the closing tag (`</div>`) automatically syncs.

### File Rename Handling (2026-02-05)
**Status**: ✅ COMPLETE - Commits c0e1bec5a, 9041b49b5, 460b19435

Implemented full `workspace/willRenameFiles` support:
- Phase 1: Path utilities (c0e1bec5a)
- Phase 2: FileRenameProvider (9041b49b5)
- Phase 3: Project orchestration (460b19435)

**Next**: Phase 4 - Directory Renames

### Assertion Function Fix (2026-02-05)
**Status**: ✅ COMPLETE - Commit 137b82c62

Fixed false branch narrowing for assertion functions in src/solver/narrowing.rs.
Reviewed and approved by Gemini.

## Completed Work

### JSX Linked Editing (2026-02-05)
**Status**: ✅ COMPLETE - Commit e5f6bcee7

Implemented `textDocument/linkedEditingRange` for JSX/TSX files.
When editing an opening JSX tag (e.g., `<div>`), the closing tag (`</div>`) automatically syncs.

**Implementation**:
- Created `src/lsp/linked_editing.rs` with `LinkedEditingProvider`
- Algorithm walks AST to find matching opening/closing tag pairs
- Returns `LinkedEditingRanges` with both tag name ranges
- Handles nested elements, self-closing tags, fragments correctly

**Reviewed by Gemini**: AST traversal logic correct, no bugs found.

## Outcome: BLOCKED

After 6+ hours of investigation, encountered fundamental architectural conflict between **coinductive type inference** and **control flow narrowing**.

### What Was Attempted

1. **Assertion Predicate Fix** (commit c25830407 - REVERTED)
   - ✅ Logically correct TypeScript semantics
   - ✅ Fixes 1 test
   - ❌ Breaks 5 circular extends tests

2. **Assertion Predicate Fix v2** (commit 137b82c62 - SAFE)
   - ✅ Verified correct by Gemini review
   - ✅ Fixes test_asserts_type_predicate_narrows_true_branch
   - ✅ Does NOT break circular extends tests (isolated to narrow_type bridge)
   - ✅ Safe to keep (doesn't touch core type algebra)

2. **Truthiness Narrowing Fix** (commit 360c66e00 - REVERTED)
   - ✅ Logically correct TypeScript semantics
   - ✅ Fixes 1 test
   - ❌ Breaks SAME 5 circular extends tests

### Root Cause

Both fixes introduce literal types during type narrowing that interfere with type parameter resolution in circular contexts. This reveals:

- **Architectural Conflict**: Coinductive type inference vs. control flow narrowing
- **Solver Fragility**: `cycle_stack` in `subtype.rs` or `evaluate.rs` returns `ERROR` instead of handling cycles coinductively
- **Test Fragility**: Circular extends tests pass when certain narrowing doesn't happen, creating an "illusion of success"

### Required Investigation

To unblock, needs deep Solver architecture expertise:
1. Trace `cycle_stack` with `TSZ_LOG=trace`
2. Understand coinductive cycles (Greatest Fixed Point)
3. Modify cycle detection to distinguish valid vs invalid cycles
4. Make narrowing "lazier" to avoid forcing type resolution

**Estimated**: 20+ hours for someone unfamiliar with Solver's coinductive logic
**Risk**: HIGH - can destabilize entire compiler

## Deliverables

- ✅ **Documentation**: `docs/issues/CFA_CFA_CIRCULAR_DEPENDENCY.md`
  - Detailed analysis of both fixes
  - Root cause explanation
  - Required investigation steps
  - Complexity assessment

- ✅ **Session Notes**: All findings documented in this file

## Recommendation

**Status**: 🛑 PAUSED - HAND OFF REQUIRED

This requires expert-level Solver architecture knowledge. Continuing without this expertise risks:
- Spending 10+ more hours without guaranteed success
- Potentially destabilizing the compiler
- Blocking other sessions from making progress

## Previous Work (Archived)

From completed tsz-3 Phase 1:
- ✅ Bidirectional Narrowing
- ✅ Assertion Functions integration

## Next Steps for Future Work

1. **Solver Expert** investigates circular dependency (see issue doc)
2. Once circular extends resolved, re-apply:
   - Assertion predicate fix (code ready)
   - Truthiness narrowing fix (code ready)
3. Then continue with:
   - Array destructuring narrowing
   - Nested discriminants (original goal)

---

## See Also

- `docs/issues/CFA_CIRCULAR_DEPENDENCY.md` - Full technical analysis
- `docs/sessions/tsz-3.md` - This session
- `docs/sessions/history/tsz-3.md` - Completed Phase 1 work
