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

**Challenge**: Requires integrating multiple components (SymbolIndex, ProjectOperations, CodeActions, Completions) and managing complex LSP protocol details.

**Gemini's Implementation Plan**:

1. Update `CompletionItem` struct in `completions.rs`:
   - Add `additional_text_edits: Option<Vec<TextEdit>>` field
   - Add `with_additional_edits()` builder method

2. Refactor `CodeActionProvider::build_import_edit` to be public utility:
   - Extract import generation logic from `code_actions.rs`
   - Make it accessible from `Project`

3. Update `Project::collect_import_candidates_for_name`:
   - Use `SymbolIndex` instead of iterating all files (O(Files) → O(Files_with_symbol))

4. Update `Project::get_completions`:
   - Generate `additionalTextEdits` for auto-import candidates
   - Attach edits to `CompletionItem` using new builder method

**Edge Cases**:
- Don't suggest if already imported (check for aliases)
- Merge into existing import blocks if module already imported
- Use relative paths (`calculate_new_relative_path`)
- Lower sort priority for auto-imports (avoid jumping to top)

**Status**: In Progress - Steps 1, 2, 4 complete; Step 3 deferred

**Progress**:
- ✅ Step 1: Updated `CompletionItem` struct
  - Added `additional_text_edits: Option<Vec<TextEdit>>` field
  - Added `with_additional_edits()` builder method
  - Added custom deserializer (completion items are server→client only)
- ✅ Step 2: Refactored edit generation
  - Added `build_auto_import_edit()` public method to `CodeActionProvider`
  - Wraps existing `build_import_edit()` logic
- ⏸️ Step 3: SymbolIndex optimization (DEFERRED)
  - Would require adding `symbol_index` field to `Project` struct
  - Can be added as separate performance improvement later
  - Current implementation still works (iterates all files)
- ✅ Step 4: Wired up auto-import edits in `get_completions`
  - Creates `CodeActionProvider` for the file
  - Calls `build_auto_import_edit()` to generate `Vec<TextEdit>`
  - Attaches edits to completion item via `with_additional_edits()`

**Next Steps** (per Gemini consultation, in priority order):

### Immediate: Integration Tests for Auto-Import
Write test cases in `src/lsp/tests/project_tests.rs` to verify:
1. Named Export: `export const foo = 1` results in auto-import
2. Default Export: `export default function foo() {}` works correctly
3. Re-exports: `export { x } from './mod'` is discoverable
4. Existing Import Check: Already-imported symbols don't reappear
5. Type-only Imports: Type positions generate `import type`

### Later: Performance Optimization
Integrate `SymbolIndex` into `Project` for O(1) candidate lookup:
- Add `symbol_index: SymbolIndex` field to Project struct
- Hook into file lifecycle (set_file, update_file, remove_file)
- Update `collect_import_candidates_for_name` to use index

### Future: Additional Enhancements
- Workspace Symbols: Use existing SymbolIndex + WorkspaceSymbolsProvider
- Prefix Matching: Suggest completions for partial matches (e.g., `Lis` → `List`)

## Session Status

Per Gemini consultation, recommended path is:
1. ✅ Write tests for File Rename (COMPLETE)
2. ✅ Add dynamic import support (COMPLETE)
3. 🔄 Implement Auto-Import Completions (IN PROGRESS)

**Current Focus**: Auto-Import Completions. This involves:
- Integrating SymbolIndex into completion flow
- Adding `additionalTextEdits` to insert import statements
- Providing import suggestions when typing unresolved names
2. Add dynamic import support (`import("./module")` and `require()`)
3. Implement Auto-Import Completions (most requested feature)

**Current Focus**: Considering next LSP feature to implement. Options include:
- Dynamic import support in FileRenameProvider
- Auto-import completions
- Code action improvements
- `handle_will_rename_files()` orchestrates full flow
- Fixed two critical bugs per Gemini Pro review

**Value**: When files are renamed, all import statements across the project automatically update.

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
