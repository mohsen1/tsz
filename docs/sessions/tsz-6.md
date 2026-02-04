# Session tsz-6 - Advanced Type Nodes for Declaration Emit

## Date: 2026-02-04

## Status: INITIALIZING

### Session Goal

Implement advanced type node emission (MappedType, ConditionalType, TypeQuery, IndexedAccessType) to fix TS1005 syntax errors and unblock accurate declaration emit.

### Problem Statement

Current declaration emit has 75+ TS1005 syntax errors (29 missing, 46 extra) because `emit_type` falls back to `emit_node` for unsupported type nodes, producing empty type annotations like `: ;`.

**Root Cause:** Missing support in `src/declaration_emitter.rs` emit_type() for:
- MappedType: `{ [K in T]: U }`
- ConditionalType: `T extends U ? X : Y`
- TypeQuery: `typeof X`
- IndexedAccessType: `T[K]`
- TemplateLiteralType: `` `a${B}c` ``

### Why This Session First (Strategic Rationale)

Per Gemini consultation (2026-02-04):

> **Import/Export Elision (TSZ-5) has a hard functional dependency on the Solver's ability to understand how symbols are referenced.**
> 
> If a .d.ts contains `export const x: typeof InternalVar;` and TypeQuery isn't implemented, the usage analyzer will fail to see that InternalVar is "used", leading to incorrect import elision.
> 
> **Solver-First Principle:** Implementing type nodes first ensures the "Lawyer" (usage analyzer) has all the facts from the "Judge" (Solver).

### Implementation Plan

#### Phase 1: TypeQuery (typeof)
- [ ] Add TYPE_QUERY handling in emit_type()
- [ ] Test: `export const x: typeof SomeSymbol;`
- [ ] Emit: `typeof SomeSymbol` (with proper qualification)

#### Phase 2: IndexedAccessType
- [ ] Add INDEXED_ACCESS_TYPE handling
- [ ] Test: `export type T = Array<string>[0];`
- [ ] Emit: `Array<string>[0]`

#### Phase 3: MappedType
- [ ] Add MAPPED_TYPE handling
- [ ] Test: `export type Readonly<T> = { readonly [P in keyof T]: T[P]; }`
- [ ] Emit: `{ readonly [P in keyof T]: T[P]; }`

#### Phase 4: ConditionalType
- [ ] Add CONDITIONAL_TYPE handling
- [ ] Test: `export type NonNullable<T> = T extends null ? never : T;`
- [ ] Emit: `T extends null ? never : T`

### Success Criteria

- [ ] TS1005 errors reduced by 50%+
- [ ] Conformance pass rate increases significantly
- [ ] All test cases for advanced types emit correctly
- [ ] Output matches TypeScript exactly

### Dependencies

- Requires: Parser/Binder support for these type nodes (verify first)
- Blocks: TSZ-5 (Import/Export Elision) - paused until this completes

### Notes

- **MANDATORY**: Follow Two-Question Rule for any solver/checker changes
- All changes to src/solver/ or src/checker/ require Gemini consultation first
- Focus on emit_type() in src/declaration_emitter.rs primarily
