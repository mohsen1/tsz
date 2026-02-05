# Session TSZ-10: Fix Discriminant Narrowing Regressions

**Started**: 2026-02-05
**Status**: Active
**Goal**: Fix 3 critical bugs in discriminant narrowing identified in AGENTS.md

## Problem Statement

From AGENTS.md evidence (2026-02-04 investigation):

Recent implementation of discriminant narrowing (commit `f2d4ae5d5`) introduced **3 critical bugs**:

1. **Reversed subtype check** - Asking `is_subtype_of(property_type, literal)` instead of `is_subtype_of(literal, property_type)`
2. **Missing type resolution** - Not handling `Lazy`, `Ref`, `Intersection` types within narrowing logic
3. **Optional property failures** - Breaking on `{ prop?: "a" }` cases

**Impact**:
- Breaks type narrowing for discriminant properties
- Causes incorrect type inference in conditional branches
- Blocks valid TypeScript code from working correctly

## Technical Details

**Files**:
- `src/solver/narrowing.rs` - Discriminant narrowing implementation
- `src/solver/visitor.rs` - Type visitor infrastructure
- `src/solver/types.rs` - Type structures (Lazy, Ref, Intersection)

**Root Causes**:
- Subtype check arguments were reversed
- Type resolution not called before subtype checks
- Optional properties not handled in discriminant matching

## Implementation Strategy

### Phase 1: Test Cases (Pre-Implementation)
1. Create failing test cases demonstrating each bug
2. Add to `src/checker/tests/` or manual test file
3. Verify tests fail with current code

### Phase 2: Fix Bug #1 - Reversed Subtype Check
1. Locate the reversed subtype check in `narrowing.rs`
2. Reverse arguments: `is_subtype_of(literal, property_type)`
3. Add test to verify fix

### Phase 3: Fix Bug #2 - Missing Type Resolution
1. Add type resolution calls before subtype checks
2. Handle `TypeKey::Lazy(DefId)` - resolve to structural type
3. Handle `TypeKey::Ref(SymbolRef)` - resolve to definition
4. Handle `TypeKey::Intersection` - resolve all members
5. Add test to verify fix

### Phase 4: Fix Bug #3 - Optional Properties
1. Add optional property handling in discriminant matching
2. Test case: `{ type?: "stop", speed: number }`
3. Verify optional discriminants work correctly

### Phase 5: Validation
1. Run all tests to verify no regressions
2. Ask Gemini Pro to review implementation
3. Document fixes in session file

## Success Criteria

- [ ] Discriminant narrowing works for literal properties
- [ ] Type resolution handles Lazy/Ref/Intersection types
- [ ] Optional properties in discriminants work correctly
- [ ] All existing tests still pass
- [ ] No regressions introduced

## Session History

*Created 2026-02-05 after TSZ-9 encountered implementation complexity.*
*Recommended by Gemini as high-value, tractable task.*
*Focuses on fixing known regressions in localized code area.*

---

## Investigation Results (2026-02-05)

### Code Review: narrowing.rs

**Good News**: Bug #1 (Reversed Subtype Check) is **ALREADY FIXED**! ✅

Location: `src/solver/narrowing.rs`, line 437
```rust
let matches = is_subtype_of(self.db, literal_value, prop_type);
```

Comment on lines 435-436 explicitly states:
```rust
// CRITICAL: Use is_subtype_of(literal_value, property_type)
// NOT the reverse! This was the bug in the reverted commit.
```

**Existing Implementation Also Has**:
- ✅ Lazy type resolution (line 306, 411)
- ✅ Union type property handling (lines 309-324)
- ✅ Intersection type handling (lines 414-421)
- ✅ Function `get_type_at_path` for property path traversal

### Remaining Issue: Bug #3 - Optional Properties

**Location**: `src/solver/narrowing.rs`, line 330

**Current Code**:
```rust
let prop = shape.properties.iter().find(|p| p.name == prop_name)?;
```

**Problem**: This only finds properties that exist. For optional properties 
(`prop?.type`), we need to handle them differently:
- If property is optional AND doesn't exist on object, still match
- Use the optional property's type for the subtype check

**Example**:
```typescript
type Opt = { type?: "stop", speed: number } | { type: "go", speed: number };
function test(o: Opt) {
    if (o.type === "stop") {
        // Should narrow to { type?: "stop", speed: number }
        // even though 'type' is optional
    }
}
```

### Updated Assessment

**Bugs Status**:
1. ✅ Reversed subtype check - ALREADY FIXED
2. ✅ Missing type resolution - ALREADY IMPLEMENTED
3. ⚠️ Optional properties - NEEDS FIX

**Action Plan Update**:
- Focus on fixing optional property handling in `get_type_at_path`
- Single, focused fix in one location
- Much more tractable than originally thought

