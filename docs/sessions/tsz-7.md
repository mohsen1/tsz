# Session tsz-7 - Automatic Import Generation for Declaration Emit

## Date: 2026-02-04

## Status: ACTIVE

### Session Progress (2026-02-04)

**Phase 1: Track Foreign Symbols in UsageAnalyzer** ✅ COMPLETE

**Implementation Summary:**
1. Added `current_arena: Arc<NodeArena>` to UsageAnalyzer struct
2. Added `foreign_symbols: FxHashSet<SymbolId>` to track foreign symbols
3. Modified `mark_symbol_used()` to categorize symbols:
   - **Global/lib symbols**: Ignored (checked via `binder.lib_symbol_ids`)
   - **Local symbols**: Added to `used_symbols` only
   - **Foreign symbols**: Added to both `used_symbols` AND `foreign_symbols`
4. Added `get_foreign_symbols()` getter method
5. Updated DeclarationEmitter to:
   - Store `current_arena: Option<Arc<NodeArena>>`
   - Store `foreign_symbols: Option<FxHashSet<SymbolId>>`
   - Added `set_current_arena()` method
   - Pass `current_arena` to UsageAnalyzer in `emit()`
6. Updated driver to call `emitter.set_current_arena(file.arena.clone())`

**Architecture Decision:**
Uses `Arc<NodeArena>` comparison (`Arc::ptr_eq()`) instead of `file_idx` to determine local vs foreign symbols. This follows tsz's design where `NodeArena` is the source of truth for file identity.

**Testing:**
- Conformance: 269/639 (42.1%) - no regressions
- Compilation: Successful

**Commits:**
- (To be added after commit)

### Session Goal

Implement automatic import generation for .d.ts files. When TypeScript generates declaration files, it automatically adds imports for types that are used but not explicitly imported. This session implements the same behavior.

### Problem Statement

Currently, tsz's declaration emitter can elide unused imports (TSZ-5), but it cannot **add** missing imports. This causes "Type cannot be named" errors when exported functions/variables reference types from other modules.

**Example:**
```typescript
// utils.ts
export interface Helper { x: number; }

// main.ts
import { Helper } from './utils';
export function factory(): Helper {
    return { x: 42 };
}
```

**Expected .d.ts (what tsc generates):**
```typescript
// main.d.ts
import { Helper } from './utils';
export declare function factory(): Helper;
```

**Current tsz output (missing import):**
```typescript
// main.d.ts
export declare function factory(): Helper;  // ERROR: Cannot find name 'Helper'
```

### Architecture

Building on TSZ-5's UsageAnalyzer infrastructure:

1. **Identify Foreign Symbols**: ✅ COMPLETE - Detect used symbols that are:
   - Not from lib files (checked via `lib_symbol_ids`)
   - From different arenas (checked via `Arc::ptr_eq()`)

2. **Resolve Module Paths**: ⏭ NEXT - Map SymbolId → source module path

3. **Synthesize Imports**: ⏭ PENDING - Inject ImportDeclaration nodes before emitting file

4. **Handle Edge Cases**: ⏭ PENDING
   - Name collisions (aliasing: `import { X as X_1 }`)
   - Type-only vs value imports
   - Re-exports

### Implementation Plan

**Phase 1: Track Foreign Symbols in UsageAnalyzer** ✅ COMPLETE
- ✅ Add `foreign_symbols: FxHashSet<SymbolId>` field
- ✅ Add `current_arena: Arc<NodeArena>` field
- ✅ Modify `mark_symbol_used()` to categorize symbols (global/local/foreign)
- ✅ Add `get_foreign_symbols()` getter
- ✅ Update DeclarationEmitter to store and pass current_arena
- ✅ Update driver to call `set_current_arena()`

**Phase 2: Module Path Resolution** ⏭ NEXT
- Map SymbolId → source module path
- Leverage `module_exports` from MergedProgram
- Handle both direct imports and namespace imports

**Phase 3: Import Synthesis** ⏭ PENDING
- Add method to DeclarationEmitter to emit missing imports
- Insert before other declarations in .d.ts
- Format: `import { TypeName } from './module';`

**Phase 4: Testing & Refinement** ⏭ PENDING
- Conformance tests for multi-file scenarios
- Edge case handling (name collisions, type-only imports)

### Dependencies

- TSZ-5 (UsageAnalyzer) - ✅ Complete
- TSZ-4 (Declaration Emit) - ✅ Complete
- `MergedProgram.module_exports` - Available
- `BinderState` symbol resolution - Available

### Expected Impact

- Reduces "Type cannot be named" errors in .d.ts files
- Completes the module system story for declaration emit
- Major improvement to multi-file project support

### Conformance Baseline

Current: 42.1% (269/639)
Target: Significant increase by fixing missing import errors

### Notes

This is the "inverse" of TSZ-5:
- TSZ-5: Remove imports that aren't used (elision)
- TSZ-7: Add imports that are needed but missing (generation)

Together, they provide complete import management for valid .d.ts files.
