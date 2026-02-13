# Control-Flow Narrowing Progress

**Status**: Work in Progress
**Pass Rate**: 51.1% (47/92 control flow tests)
**Date**: 2026-02-13

---

## Issue: Aliased Discriminant Narrowing with Let/Const

### Problem
TypeScript only narrows const-bound variables via aliased discriminants, not let-bound ones:

```typescript
let { data: d1, success: s1 } = getResult();    // LET
const { data: d2, success: s2 } = getResult();  // CONST
const areSuccess = s1 && s2;
if (areSuccess) {
    d1.toExponential();  // Should ERROR - d1 is let ❌
    d2.toExponential();  // Should OK - d2 is const ✅
}
```

**Expected**: TS18048 "possibly undefined" on d1
**Actual**: No error (we incorrectly narrow let-bound variables)

### Root Cause
When checking aliased discriminants, TypeScript:
1. Tracks which variables the alias depends on
2. Checks if those variables are const or let
3. Only narrows const variables (let can be reassigned)

We have `is_mutable_variable()` but don't use it in all narrowing paths.

---

## Work Completed

### Commit: 463e6045b
**Added**: Mutability check in one discriminant narrowing path
**File**: `crates/tsz-checker/src/control_flow.rs:2690`
**Change**: Added `is_mutable` check before applying discriminant narrowing

```rust
let is_mutable = self.is_mutable_variable(target);
if !is_property_access && !is_mutable {
    // Apply narrowing
}
```

**Result**: All unit tests pass (2394/2394) ✅
**Impact**: Partial - need to add check in other code paths

---

## Remaining Work

### Multiple Code Paths
Discriminant narrowing happens in several places:
1. ✅ `control_flow.rs:2695` - `narrow_by_discriminant_for_type` (DONE)
2. ❌ Type guard extraction and application
3. ❌ Flow node traversal
4. ❌ Antecedent tracking for aliases

### Next Steps
1. **Find all discriminant narrowing paths**
   - Search for `TypeGuard::Discriminant` usage
   - Search for `narrow_by_discriminant` calls
   - Trace through flow analysis for discriminant checks

2. **Add mutability checks to each path**
   - Before narrowing, call `is_mutable_variable(target)`
   - Skip narrowing if variable is mutable (let/var)

3. **Test each fix**
   - Run: `./scripts/conformance.sh run --filter controlFlowAliasedDiscriminants`
   - Expected: TS18048 errors on let-bound variables
   - Run: `cargo nextest run` to check for regressions

4. **Handle other CFA categories**
   - Assertion functions
   - Destructuring-aware flow
   - CFA edge cases

---

## Testing Commands

```bash
# Build
cargo build --profile dist-fast -p tsz-cli

# Test specific file
./.target/dist-fast/tsz tmp/test.ts --strict --noEmit

# Run control flow tests
./scripts/conformance.sh run --filter controlFlow

# Run specific test
./scripts/conformance.sh run --filter controlFlowAliasedDiscriminants

# Unit tests
cargo nextest run

# Commit and sync
git add -A && git commit -m "..."
git pull --rebase origin main && git push origin main
```

---

## Key Files

- `crates/tsz-checker/src/control_flow.rs` - Main flow analysis
- `crates/tsz-checker/src/control_flow_narrowing.rs` - Narrowing methods
- `crates/tsz-checker/src/flow_narrowing.rs` - Additional narrowing logic
- `crates/tsz-solver/src/narrowing.rs` - Solver narrowing operations

---

## Estimated Effort

**Aliased discriminants**: 2-3 sessions (ongoing)
**Full control-flow**: 5-7 sessions total

**Current**: 1 session invested
**Remaining**: 4-6 sessions

---

**Status**: Incremental progress made, multi-session effort required
