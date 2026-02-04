# Session tsz-4 - Declaration Emit (.d.ts file generation)

## Date: 2025-02-04

## Status: ACTIVE - Declaration Emit Implementation

### Executive Summary

Session tsz-4 is focused entirely on implementing declaration file generation (`tsc --declaration` or `-d`). This feature generates `.d.ts` definition files from TypeScript source files, enabling library authors to publish type definitions for their consumers.

### What is Declaration Emit?

Declaration emit generates `.d.ts` files containing only type information:

**Input** (`mylib.ts`):
```typescript
export function add(a: number, b: number): number {
    return a + b;
}
export class Calculator {
    private value: number;
    add(n: number): this { ... }
}
```

**Output** (`mylib.d.ts`):
```typescript
export declare function add(a: number, b: number): number;
export declare class Calculator {
    private value: number;
    add(n: number): this;
}
```

## Current State

### ✅ Already Implemented
- `src/declaration_emitter.rs` - Basic declaration emitter exists
- Handles: functions, classes, interfaces, type aliases, enums, imports, exports
- Modifiers: public, private, protected, static, readonly, abstract
- Type parameters and constraints
- Heritage clauses (extends, implements)

### ❌ Missing Features (from Gemini analysis)

1. **Type Reification (TypeId → AST)** - CRITICAL
   - Need to convert Solver's `TypeId` back into printable AST nodes
   - Required for inferred types in declarations
   - Example: `function add(a, b) { return a + b; }` → `declare function add(a: any, b: any): any;`
   - Implementation location: `src/emitter/types.rs` or new type reification module

2. **Export Filtering**
   - Use Binder's `is_exported` flag to filter output
   - Only emit exported symbols (not private/internal ones)
   - Binder already has this info in `src/binder/state_binding.rs`

3. **Visibility Stripping (TS4023)**
   - Detect when exported symbols reference non-exported types
   - Example: Exporting a function that returns a private type should error
   - Error code: TS4023 "Exported variable X has or is using name Y from external module..."

4. **Import Rewriting**
   - Generate correct `import` statements in `.d.ts` files
   - Handle type-only imports: `import type { Foo } from './foo'`
   - Rewrite relative paths for declaration context

5. **Alias Resolution**
   - Decide when to emit full type structure vs just alias name
   - Example: `type X = { a: string }` vs `type X = MyInterface`

6. **Shadowing Handling**
   - Ensure generated type parameter names don't conflict with global names

## Architecture

### Data Flow
```
Parser → Binder (marks is_exported) → Checker/Solver (infers TypeId)
                                                      ↓
DeclarationEmitter ← TypeReifier ← TypeId
    ↓
.d.ts output
```

### Key Components

**Binder** (`src/binder/`)
- Provides `is_exported` flag on symbols
- Already fully implemented and working

**Solver** (`src/solver/`)
- Provides type inference and `TypeId` for expressions
- Returns inferred types for functions/variables without annotations

**DeclarationEmitter** (`src/declaration_emitter.rs`)
- Current implementation: ~1800 lines
- Handles AST-based emission (when type annotations exist)
- Missing: integration with Solver for inferred types

**Type Reifier** (NEEDS IMPLEMENTATION)
- Convert `TypeId` → synthetic AST node
- Should handle primitives, arrays, unions, intersections, functions, classes, generics
- Location: Could extend `src/emitter/types.rs` or create new module

## Priority Tasks

### Phase 1: Type Reification (HIGH PRIORITY)
1. Implement basic type reification for primitives
   - `TypeId::STRING` → `"string"`
   - `TypeId::NUMBER` → `"number"`
   - `TypeId::BOOLEAN` → `"boolean"`

2. Implement array type reification
   - Detect `TypeKey::Array(elem_id)`
   - Recursively reify element type
   - Output: `string[]` or `Array<string>`

3. Implement function type reification
   - Extract parameters and return type
   - Output: `(a: T, b: U) => R`

4. Create unit tests for type reification
   - Mock solver with known TypeIds
   - Assert string output matches TypeScript syntax

### Phase 2: Solver Integration
1. Add Checker/Solver context to DeclarationEmitter
2. Query solver for inferred types when AST annotation missing
3. Test with functions/variables that have inferred types

### Phase 3: Export Filtering
1. Integrate with Binder's `is_exported` flag
2. Filter statement stream to only emit exported symbols
3. Handle re-exports correctly

### Phase 4: Import Rewriting
1. Detect when types reference imported symbols
2. Generate correct import statements in .d.ts output
3. Handle type-only imports

## Session Coordination

**Other Sessions** (no conflicts):
- **tsz-1**: Parse errors (TS1005, TS1109, etc.)
- **tsz-2**: Module resolution (TS2307, TS2664, TS2322)
- **tsz-3**: Const type parameters, type system issues

**Declaration emit is independent** - doesn't overlap with other session work

## Commits

*(Session starting - no commits yet)*

## Resources

- Gemini conversation 2026-02-04: Declaration emit architecture and requirements
- File: `src/declaration_emitter.rs` - Current implementation
- File: `src/emitter/types.rs` - Type emission helpers
- File: `docs/architecture/NORTH_STAR.md` - Architecture reference

## Next Steps

1. ✅ Reviewed existing DeclarationEmitter implementation
2. ✅ Identified gaps via Gemini analysis
3. **TODO**: Implement TypeReifier for basic types
4. **TODO**: Add solver integration to DeclarationEmitter
5. **TODO**: Create comprehensive test suite

---

## Notes

- Declaration emit depends on type inference from Solver
- Key challenge: Converting internal TypeId back to TypeScript syntax
- Should preserve all type information visible in the public API
- Must strip all implementation details (function bodies, initializers)
