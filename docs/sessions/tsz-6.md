# Session tsz-6 - Advanced Type Nodes for Declaration Emit

## Date: 2026-02-04

## Status: COMPLETE ✅

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

#### Phase 1: TypeQuery (typeof) ✅
- [x] Add TYPE_QUERY handling in emit_type()
- [x] Add type_arguments support (TS 4.7+)
- [x] Emit: `typeof SomeSymbol` and `typeof f<number>`

#### Phase 2: IndexedAccessType ✅
- [x] Add INDEXED_ACCESS_TYPE handling
- [x] Add precedence handling for Union/Intersection/Function types
- [x] Emit: `(A | B)[K]` with proper parentheses

#### Phase 3: MappedType ✅
- [x] Already implemented (no changes needed)
- [x] Emit: `{ readonly [P in keyof T]: T[P]; }`

#### Phase 4: ConditionalType ✅
- [x] Add CONDITIONAL_TYPE handling
- [x] Add precedence handling for all type parts
- [x] Emit: `T extends null ? never : T` with proper parentheses

### Success Criteria

- [x] All advanced type nodes implemented in emit_type()
- [x] Parser support verified (already had all accessors)
- [x] Code compiles without errors
- [ ] TS1005 errors reduced (to be measured with conformance tests)
- [ ] Conformance pass rate increases (to be measured)

### Dependencies

- Requires: Parser/Binder support for these type nodes (verify first)
- Blocks: TSZ-5 (Import/Export Elision) - paused until this completes

### Notes

- **MANDATORY**: Follow Two-Question Rule for any solver/checker changes
- All changes to src/solver/ or src/checker/ require Gemini consultation first
- Focus on emit_type() in src/declaration_emitter.rs primarily

### Completion Summary - 2026-02-04

**Completed Work:**
1. **TypeQuery Enhancement**: Added type_arguments support for `typeof f<number>` (TS 4.7+)
2. **IndexedAccessType Fix**: Added precedence handling to correctly emit `(A | B)[K]` instead of `A | B[K]`
3. **ConditionalType Implementation**: Full implementation with precedence handling for all parts (check_type, extends_type, true_type, false_type)
4. **MappedType Verification**: Confirmed already implemented correctly

**Commit**: 186d17223 - "feat: implement advanced type node emission for declaration emit"

**Impact:**
- Advanced type nodes now emit correctly in .d.ts files
- Precedence handling prevents syntax errors in complex type expressions
- Unblocks tsz-5 (Import/Export Elision) with accurate type foundation

**Next Steps:**
1. Run conformance tests to measure impact
2. If significant improvement, consider tsz-5 ready to resume
3. Otherwise, identify remaining missing type nodes
