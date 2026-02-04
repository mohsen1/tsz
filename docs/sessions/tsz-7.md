# Session tsz-7 - Automatic Import Generation for Declaration Emit

## Date: 2026-02-04

## Status: ACTIVE

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

1. **Identify Foreign Symbols**: Detect used symbols that are:
   - Not declared in current file
   - Not present in existing import statements

2. **Resolve Module Paths**: Use Binder/Symbol info to find source module

3. **Synthesize Imports**: Inject ImportDeclaration nodes before emitting file

4. **Handle Edge Cases**:
   - Name collisions (aliasing: `import { X as X_1 }`)
   - Type-only vs value imports
   - Re-exports

### Implementation Plan

**Phase 1: Track Foreign Symbols in UsageAnalyzer**
- Add `foreign_symbols: FxHashSet<SymbolId>` field
- During analysis, identify symbols from other modules
- Use `BinderState::get_symbol_declaration()` to find origin

**Phase 2: Module Path Resolution**
- Map SymbolId → source module path
- Leverage `module_exports` from MergedProgram
- Handle both direct imports and namespace imports

**Phase 3: Import Synthesis**
- Add method to DeclarationEmitter to emit missing imports
- Insert before other declarations in .d.ts
- Format: `import { TypeName } from './module';`

**Phase 4: Testing & Refinement**
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

Current: 42.3% (270/639)
Target: Significant increase by fixing missing import errors

### Notes

This is the "inverse" of TSZ-5:
- TSZ-5: Remove imports that aren't used (elision)
- TSZ-7: Add imports that are needed but missing (generation)

Together, they provide complete import management for valid .d.ts files.
