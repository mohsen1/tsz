# Session tsz-3: LSP Implementation - Directory Renames

**Started**: 2026-02-05
**Status**: 🔄 ACTIVE
**Previous Session**: CFA Stabilization (Blocked - architectural conflicts)

## Goal

Implement LSP features that improve developer experience without requiring deep Solver/Checker architecture expertise.

## Current Work: File Rename - Phase 4 (Directory Renames)

Extending `workspace/willRenameFiles` to handle directory renames.

**Challenge**: When a directory is renamed, must recursively update all imports
that reference files within that directory. Files inside the renamed directory
also need their relative imports updated.

**Implementation Plan**:
1. Detect if old_uri/new_uri are directories
2. Recursively find all files within the directory
3. For each file, find its dependents and update their imports
4. Handle relative imports inside the renamed directory

**Status**: Planning phase

## Completed Work

### File Rename Handling (2026-02-05)
**Status**: ✅ COMPLETE - Commit 460b19435

Implemented full `workspace/willRenameFiles` support:
- Phase 1: Path utilities (c0e1bec5a)
- Phase 2: FileRenameProvider (9041b49b5)
- Phase 3: Project orchestration (460b19435)

**Implementation**:
- Added `dependency_graph` field to Project struct
- `extract_imports()` handles imports AND re-exports
- `is_import_pointing_to_file()` filters imports correctly
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
