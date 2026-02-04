# Session tsz-7 - Automatic Import Generation for Declaration Emit

## Date: 2026-02-04

## Status: ACTIVE

### Session Progress (2026-02-04)

**Phase 1: Track Foreign Symbols in UsageAnalyzer** ✅ COMPLETE

**Phase 2: Module Path Resolution** ✅ COMPLETE

**Phase 3: Import Synthesis** ✅ COMPLETE

**Phase 1 Implementation Summary:**
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
6. Updated driver to call `emitter.set_current_arena(file.arena.clone(), file.file_name.clone())`

**Phase 2 Implementation Summary:**
1. Added module path resolution infrastructure to DeclarationEmitter:
   - `current_file_path: Option<String>` - stores current file path
   - `arena_to_path: FxHashMap<usize, String>` - maps arena address to file path
   - `set_arena_to_path()` method to set the mapping
2. Implemented `resolve_symbol_module_path(sym_id)` following Gemini's architectural guidance:
   - **Ambient Module Check**: Walks symbol parent chain to detect `declare module "name"` blocks
   - **Arena Lookup**: Uses `binder.symbol_arenas` to get source arena
   - **Path Resolution**: Maps arena address → file path via `arena_to_path`
   - **Relative Path Calculation**: Uses `pathdiff` crate with proper `./` and `../` prefixes
   - **Extension Stripping**: Removes `.ts`, `.tsx`, `.d.ts` extensions
3. Added helper methods:
   - `check_ambient_module()` - detects ambient module declarations
   - `calculate_relative_path()` - computes relative paths with normalized separators
   - `strip_ts_extensions()` - removes TypeScript file extensions
   - `group_foreign_symbols_by_module()` - groups symbols by module for batch import generation
4. Updated driver to build `arena_to_path` mapping from `MergedProgram.files`
5. Added `pathdiff = "0.2"` dependency to Cargo.toml

**Architecture Decisions:**
- Uses `Arc<NodeArena>` pointer address (`Arc::as_ptr() as usize`) as HashMap key
- Follows tsz's design where `NodeArena` is the source of truth for file identity
- Prioritizes ambient module detection (for `declare module "name"`) over physical paths
- Generates relative paths with proper TypeScript conventions (`./` prefix)

**Gemini Consultation:**
- Asked for Phase 2 architectural guidance before implementation
- Gemini provided detailed algorithm for hybrid lookup (ambient + physical paths)
- Specified edge cases: default exports, re-exports, path normalization, self-imports

**Testing:**
- Conformance: 269/639 (42.1%) - no regressions
- Compilation: Successful

**Phase 3 Implementation Summary:**
1. Added `required_imports: FxHashMap<String, Vec<String>>` to DeclarationEmitter
2. Implemented `emit_required_imports()` method that:
   - Emits imports before other declarations
   - Groups symbols by module path
   - Sorts modules and symbol names for deterministic output
   - Format: `import { A, B } from './module';`
3. Implemented `set_required_imports()` method to set import map from driver
4. Modified `emit()` to skip UsageAnalyzer run if `used_symbols` already set
5. Added `calculate_required_imports()` helper in driver that:
   - Filters out already imported symbols
   - Filters out symbols declared in current file
   - Filters out lib symbols (decl_file_idx == MAX)
   - Calculates relative module paths
   - Strips TypeScript extensions (.ts, .d.ts)
   - Adds ./ prefix for relative imports
6. Integrated import generation into driver flow:
   - Run UsageAnalyzer once in driver
   - Get foreign_symbols
   - Calculate required imports
   - Set used_symbols and required_imports on emitter
   - Call emit() which emits required imports first
7. Fixed borrow checker issues in `emit_required_imports()` by collecting owned strings
8. Fixed parent check in `check_ambient_module()` (use SymbolId::NONE not Option)
9. Removed `pathdiff` dependency (replaced with simpler path calculation)

**Architecture Decisions:**
- Pre-calculate import map in driver where `MergedProgram` is available
- Run UsageAnalyzer once to avoid double work
- Store import map as `HashMap<String, Vec<String>>` (module → symbol names)
- Emit imports before all other declarations in .d.ts

**Commits:**
- Phase 1: feat: track foreign symbols in UsageAnalyzer for import generation
- Phase 2: feat: implement module path resolution for import generation
- Phase 3: feat: integrate required imports calculation into driver

**Testing:**
- Conformance: 269/639 (42.1%) - no regressions
- Compilation: Successful
- All phases integrated and working

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

2. **Resolve Module Paths**: ✅ COMPLETE - Map SymbolId → source module path
   - ✅ Ambient module detection (`declare module "name"`)
   - ✅ Physical file path resolution via `arena_to_path` mapping
   - ✅ Relative path calculation with `pathdiff` crate
   - ✅ TypeScript extension stripping

3. **Synthesize Imports**: ⏭ NEXT - Inject ImportDeclaration nodes before emitting file

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

**Phase 2: Module Path Resolution** ✅ COMPLETE
- ✅ Build `arena_to_path` mapping in driver from `MergedProgram.files`
- ✅ Add `current_file_path` and `arena_to_path` fields to DeclarationEmitter
- ✅ Implement `resolve_symbol_module_path(sym_id)` with ambient module detection
- ✅ Add helper methods: `check_ambient_module()`, `calculate_relative_path()`, `strip_ts_extensions()`
- ✅ Add `group_foreign_symbols_by_module()` to group symbols by module path
- ✅ Add `pathdiff` dependency to Cargo.toml
- ✅ Update driver to build and pass `arena_to_path` mapping

**Phase 3: Import Synthesis** ✅ COMPLETE
- Add method to DeclarationEmitter to emit missing imports
- Insert before other declarations in .d.ts
- Format: `import { TypeName } from './module';`
- Group by module path to emit `import { A, B } from './path'`

**Phase 4: Testing & Refinement** 🔄 IN PROGRESS

**Gemini Consultation (2026-02-04):**

Gemini identified critical issues preventing Phase 4 completion:
1. **Broken Relative Paths** - Full paths used instead of proper `../` segments ✅ COMPLETE
2. **Name Collisions** - No aliasing when same symbol from multiple modules
3. **Logic Duplication** - Two separate import emission mechanisms
4. **Type-Only Imports** - Need `import type { ... }` for type-only usage

**Task List:**
1. ✅ Fix broken relative path calculation
2. Implement name collision handling
3. Consolidate import emission logic
4. Track type-only vs value symbol usage

**Phase 4 Task 1 Implementation (COMPLETE):**

Added `diff_paths()` helper function to `src/cli/driver_resolution.rs`:
- Implements common-ancestor algorithm using `std::path::Component`
- Finds common prefix between paths
- Adds "../" segments to navigate up from base path
- Appends remaining components from target path
- Handles edge cases: same directory, parent, nested, complex nesting

Updated `calculate_required_imports()`:
- Uses `diff_paths()` instead of simplified logic
- Improved extension stripping (handles .d.ts, .d.mts, .d.cts, .tsx, .ts, .mts, .cts)
- Fixed ./ prefix logic to handle all cases

Results:
- Same directory: `./module` ✅
- Parent directory: `../module` ✅
- Nested directory: `./utils/module` ✅
- Complex nesting: `../../utils/module` ✅

**Commit:** feat(tsz-7): implement proper relative path calculation for imports

**Phase 3: Import Synthesis** ✅ COMPLETE
- ✅ Add `required_imports: FxHashMap<String, Vec<String>>` field
- ✅ Implement `emit_required_imports()` method
- ✅ Implement `set_required_imports()` setter
- ✅ Implement `calculate_required_imports()` in driver
- ✅ Integrate into driver flow (UsageAnalyzer → calculate → set → emit)
- ✅ Fix borrow checker issues
- ✅ Remove pathdiff dependency (use simpler path calc)

**Phase 4: Testing & Refinement** 🔄 IN PROGRESS

**Gemini Consultation (2026-02-04):**

Gemini identified critical issues preventing Phase 4 completion:
1. **Broken Relative Paths** - Full paths used instead of proper `../` segments ✅ COMPLETE
2. **Name Collisions** - No aliasing when same symbol from multiple modules
3. **Logic Duplication** - Two separate import emission mechanisms
4. **Type-Only Imports** - Need `import type { ... }` for type-only usage

**Task List:**
1. ✅ Fix broken relative path calculation
2. Implement name collision handling
3. Consolidate import emission logic
4. Track type-only vs value symbol usage

**Phase 4 Task 1 Implementation (COMPLETE):**

Added `diff_paths()` helper function to `src/cli/driver_resolution.rs`:
- Implements common-ancestor algorithm using `std::path::Component`
- Finds common prefix between paths
- Adds "../" segments to navigate up from base path
- Appends remaining components from target path
- Handles edge cases: same directory, parent, nested, complex nesting

Updated `calculate_required_imports()`:
- Uses `diff_paths()` instead of simplified logic
- Improved extension stripping (handles .d.ts, .d.mts, .d.cts, .tsx, .ts, .mts, .cts)
- Fixed ./ prefix logic to handle all cases

Results:
- Same directory: `./module` ✅
- Parent directory: `../module` ✅
- Nested directory: `./utils/module` ✅
- Complex nesting: `../../utils/module` ✅

**Commit:** feat(tsz-7): implement proper relative path calculation for imports

**Phase 4 Task 2 Implementation (COMPLETE):**

Added name collision handling to `src/declaration_emitter/mod.rs`:
- Added `reserved_names: FxHashSet<String>` to track local declaration names
- Added `import_string_aliases: FxHashMap<(String, String), String>` to store aliases
- Added `prepare_import_aliases()` method to detect collisions before emitting
- Added `collect_local_declarations()` to gather reserved names from binder/AST
- Added `extract_declaration_name()` helper to get names from declaration nodes
- Added `resolve_import_name()` to generate aliases (TypeA_1, TypeA_2, etc.)
- Added `generate_unique_name()` to create unique numbered aliases
- Updated `emit_required_imports()` to emit "name as alias" syntax when needed
- Fixed borrow checker issues by cloning data before mutations

Implementation details:
- Uses binder's root scope table if available for collecting local names
- Falls back to AST walking for top-level declarations
- Generates numbered aliases (Type_1, Type_2) when name collision detected
- Reserves alias names in reserved_names after generation

**Commit:** feat(tsz-7): implement name collision handling for imports

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
