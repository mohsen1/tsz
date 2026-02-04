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

### 2026-02-04: Static Block TDZ Investigation

**Implemented** (commits b549afdcd, 0e8d667a7):
- `is_variable_used_before_declaration_in_static_block` method
- TS2448 diagnostic code and message
- Forward reference TDZ check method `check_forward_reference_tdz`
- Checks if symbol is block-scoped (let, const, class, enum)
- Compares usage position vs declaration position in source
- Verifies usage is inside a static block using `find_enclosing_static_block`

**Working**:
- Detects TDZ when symbol IS resolved by binder
- Example: Module-level `const` used inside static block after being declared

**Critical Discovery**:
- Forward references are NOT being handled correctly
- When FOO is used before declaration in `classStaticBlockUseBeforeDef3.ts`:
  - `get_type_of_identifier` is NOT being called for the FOO identifier at all
  - This means the issue is deeper than TDZ checking - it's in AST traversal or identifier resolution
  - The forward reference exists in `file_locals` but `resolve_identifier_symbol` returns None
  - The code path that should check for forward TDZ violations is never reached

**Root Cause**:
- The binder runs completely before the checker, so all symbols SHOULD exist
- However, `resolve_identifier_symbol` uses scope chain walking, which doesn't find forward references
- The fallback path (checking `file_locals`) exists but `get_type_of_identifier` isn't being called for the problematic identifiers

**Next Steps**:
1. Investigate why `get_type_of_identifier` isn't called for forward-referenced identifiers in static blocks
2. May need to check AST construction or expression type checking for call arguments
3. Implement computed property TDZ
4. Implement heritage clause TDZ

## Success Criteria

- [x] Static block TDZ method implemented (partial - needs forward reference handling)
- [ ] Forward reference TDZ detection
- [ ] Computed property TDZ implemented
- [ ] Heritage clause TDZ implemented
- [ ] Tests pass for all TDZ cases
- [ ] No regressions in existing tests
- [ ] All work committed and pushed

## Notes

- This task is well-isolated and doesn't require broad architectural changes
- It directly addresses conformance gaps in TS2454 errors
- Reference: `docs/walkthrough/04-checker.md` documents this as a known gap
- Forward reference detection may require checking all top-level declarations in the file
