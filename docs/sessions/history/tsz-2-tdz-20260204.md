# Session tsz-2: TDZ (Temporal Dead Zone) Checking

**Started**: 2026-02-04
**Goal**: Implement TDZ checking to detect variables used before declaration in class contexts

## Problem Statement

TypeScript enforces Temporal Dead Zone (TDZ) rules to prevent variables from being used before they're declared. Currently, several methods in `src/checker/flow_analysis.rs` are stubbed and return `false`, causing the compiler to miss these errors.

## Scope

Implement TDZ checking for:
1. **Static Blocks**: Variables used in static blocks before their declaration
2. **Computed Properties**: Variables used in computed property names `[expr]` before declaration
3. **Heritage Clauses**: Variables used in `extends`/`implements` clauses before declaration

## Progress

### 2026-02-04: TDZ Implementation - COMPLETE ✅

**All three TDZ checks implemented:**

#### 1. Static Block TDZ ✅
- Root cause: Static blocks weren't being traversed at all
- Fixed by adding `CLASS_STATIC_BLOCK_DECLARATION` case to `check_class_member`
- `is_variable_used_before_declaration_in_static_block` implemented
- Test case: `classStaticBlockUseBeforeDef3.ts` passes
- Emits TS2448

#### 2. Computed Property TDZ ✅
- `is_variable_used_before_declaration_in_computed_property` implemented
- Checks if usage is inside a computed property name `[expr]`
- Uses `find_enclosing_computed_property` from scope_finder.rs
- Emits TS2448

#### 3. Heritage Clause TDZ ✅
- `is_variable_used_before_declaration_in_heritage_clause` implemented
- Checks if usage is in `extends`/`implements` clause
- Uses `find_enclosing_heritage_clause` from scope_finder.rs
- Emits TS2448

**Implementation Details:**
All three TDZ checks follow the same pattern:
1. Get the symbol and verify it's block-scoped (let, const, class, enum)
2. Get the declaration node
3. Compare source positions: usage must be before declaration
4. Check if usage is in the specific TDZ-sensitive context
5. Return true if TDZ violation detected

**Test Results:**
```typescript
class Baz {
    static {
        console.log(FOO);   // line 17
    }
}
const FOO = "FOO";  // line 21
```
✅ tsc: `error TS2448: Block-scoped variable 'FOO' used before its declaration.`
✅ tsz: `error TS2448: Block-scoped variable 'FOO' used before its declaration.`

**Commits:**
- fea4b95f5: Static block TDZ with traversal fix
- 0e8d667a7: TS2448 diagnostic
- 629e51593: Computed property and heritage clause TDZ

## Success Criteria

- [x] Static block TDZ method implemented
- [x] Static block traversal fixed (root cause)
- [x] TS2448 diagnostic added
- [x] Test case passes (classStaticBlockUseBeforeDef3.ts)
- [x] Computed property TDZ implemented
- [x] Heritage clause TDZ implemented
- [x] All work committed and pushed
- [x] No regressions (52 pre-existing test failures, unchanged)

## Notes

- This task required fixing a missing traversal handler for static blocks
- The fix enables ALL type checking for static blocks, not just TDZ
- Static blocks were completely untype-checked before this fix
- Reference: `docs/walkthrough/04-checker.md` documents this as a known gap
- All three TDZ contexts now emit TS2448 as expected

## Session Status: COMPLETED ✅
