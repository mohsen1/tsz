# Session TSZ-11: Truthiness & Equality Narrowing

**Started**: 2026-02-05
**Status**: Active
**Goal**: Implement narrowing for truthiness checks (`if (x)`) and equality/identity checks (`if (x === null)`)

## Problem Statement

From NORTH_STAR.md and the TypeScript type system:

TypeScript supports narrowing through:
1. **Truthiness checks**: `if (x)` narrows `string | null` to `string` in the true branch
2. **Strict equality**: `if (x === null)` narrows to exclude `null`
3. **Loose equality**: `if (x == null)` narrows to exclude both `null` AND `undefined` (Lawyer rule)

Currently, tsz does not implement these narrowing operations, which limits the effectiveness of control flow analysis.

**Impact:**
- Blocks proper handling of nullable types
- Reduces type safety in conditional branches
- Incomplete control flow analysis

## Technical Details

**Files**:
- `src/solver/narrowing.rs` - Add `narrow_by_truthiness` and `narrow_by_equality`
- `src/checker/control_flow_narrowing.rs` - Detect equality expressions in guards
- `src/solver/visitor.rs` - Use visitor to identify falsy types

**Root Causes**:
- No implementation of truthiness narrowing (falsy type filtering)
- No implementation of equality narrowing (literal value exclusion)
- Loose equality (`==`) has special TypeScript behavior (Lawyer rule)

## Implementation Strategy

### Phase 1: Investigation (Current Phase)
1. Read existing narrowing code in `src/solver/narrowing.rs`
2. Ask Gemini Question 1: Approach validation
3. Understand falsy component detection

### Phase 2: Truthiness Narrowing
1. Implement `narrow_by_truthiness(type_id)` in Solver
2. Filter union members to exclude falsy types:
   - `null`
   - `undefined`
   - `false` (boolean)
   - `0` and `NaN` (number)
   - `""` (empty string)
   - `0n` (bigint)
3. Handle special cases: `any`, `unknown`, `never`
4. Add tests for truthiness narrowing

### Phase 3: Equality Narrowing (Strict)
1. Implement `narrow_by_equality(type_id, literal_value, is_exclusive)`
2. For `=== literal`: exclude the literal value
3. For `!== literal`: keep only the literal value
4. Handle `null` and `undefined` specially
5. Add tests for strict equality

### Phase 4: Equality Narrowing (Loose - Lawyer Rule)
1. Implement loose equality semantics
2. `== null` narrows to exclude both `null` AND `undefined`
3. `!= null` keeps only `null | undefined`
4. This is a TypeScript-specific "Lawyer" rule
5. Add tests for loose equality

### Phase 5: Checker Integration
1. Update `extract_type_guard` to detect equality expressions
2. Pass guard to Solver for narrowing calculation
3. Ensure proper integration with control flow analysis
4. Test with complex control flow

## Success Criteria

- [ ] `if (x)` narrows `string | null` to `string` in true branch
- [ ] `if (x === null)` narrows `string | null` to `null` in true branch
- [ ] `if (x == null)` narrows to exclude both `null` AND `undefined`
- [ ] Falsy types are correctly identified and excluded
- [ ] Works with union types, intersections, and lazy types
- [ ] No regressions in existing narrowing

## Session History

*Created 2026-02-05 after completing TSZ-10.*
*Recommended by Gemini as next high-value task.*
*Completes the Control Flow Analysis story alongside discriminant narrowing.*

---

## Investigation Results (2026-02-05)

### Existing Infrastructure

**Falsy Component Detection** (already implemented):
- `falsy_component(type_id)` - returns the falsy part of a type
- `narrow_to_falsy(type_id)` - narrows to only the falsy part
- Located in `src/checker/control_flow_narrowing.rs`

**Truthy Narrowing** (already implemented!):
- `narrow_excluding_type` - excludes a specific type from a union
- Used in conjunction with `falsy_component`
- Already working for basic cases

**Status**: Need to verify if truthiness narrowing is already working or needs implementation.

