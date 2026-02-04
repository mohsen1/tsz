# Session tsz-3 - Critical Bug Review (Last 400 Commits)

**Started**: 2026-02-04
**Status**: IN_PROGRESS
**Focus**: Review and fix critical bugs from the last 400 commits

## Context

Systematic review of the last 400 commits to find and fix type system bugs before continuing new feature work.

## Findings

### 1. Discriminant Narrowing (COMMIT: f2d4ae5d5) ✅ FIXED

**Issue**: The `narrow_by_discriminant` rewrite had 3 critical bugs:
1. **Reversed subtype check**: Asked `is_subtype_of(property_type, literal)` instead of `is_subtype_of(literal, property_type)`
2. **Missing type resolution**: Didn't handle `Lazy`/`Ref`/`Intersection` types
3. **Broken for optional properties**: Failed on `{ prop?: "a" }` cases

**Resolution**: REVERTED commit f2d4ae5d5

**Correct Approach** (from Gemini):
- Use **filtering approach**, not pre-discovery
- For each union member, use `resolve_property_access` to handle Lazy/Intersection/Apparent
- Check `is_subtype_of(literal, property_type)` - NOT reversed
- Handle edge cases: optional properties, shared discriminant values, non-object members

**Status**: ✅ Reverted, ready for re-implementation

---

### 2. instanceof Narrowing (FILE: src/solver/narrowing.rs)

**Issue**: `narrow_by_instanceof` has 1 critical bug:

#### Bug: Interface vs Class Narrowing
- **Current**: Returns `NEVER` for interface vs class (uses `narrow_to_type` which checks assignability)
- **Expected**: Should narrow to `I & C` (intersection)
- **Example**:
  ```typescript
  interface I {}
  class C implements I {}
  function test(x: I) {
      if (x instanceof C) {
          // Should narrow to I & C
          // Currently returns NEVER (wrong!)
      }
  }
  ```

**Fix**: Use `interner.intersection2(source, target)` instead of `narrow_to_type` when not assignable

**Status**: ⚠️ BUG FOUND, NOT FIXED

---

### 3. `in` Operator Narrowing (FILE: src/solver/narrowing.rs)

**Issue**: `narrow_by_property_presence` has 4+ critical bugs:

#### Bug A: unknown Handling
- **Current**: Returns `unknown` unchanged
- **Expected**: Should narrow to `object & { prop: unknown }`

#### Bug B: Optional Property Promotion
- **Current**: Property stays optional after `in` check
- **Expected**: Should become required
- **Example**:
  ```typescript
  function test(x: { a?: string }) {
      if ("a" in x) {
          x.a; // Should be string (not string | undefined)
      }
  }
  ```

#### Bug C: Missing Type Resolution
- **Current**: Uses `object_shape_id` which doesn't resolve `Lazy`/`Ref`
- **Expected**: Must call `resolve_ref_type` before shape lookup
- **Impact**: Fails for named interfaces and classes

#### Bug D: No Intersection Support
- **Current**: Returns `false` for intersection types
- **Expected**: Should return `true` if ANY member has the property

#### Missing Features:
- Prototype property checks (should use `apparent_object_member_kind`)
- Private field handling

**Status**: ⚠️ BUGS FOUND, NOT FIXED

---

## Summary

| Feature | Bugs Found | Status |
|---------|-----------|--------|
| Discriminant narrowing | 3 (reversed check, no resolution, optional props) | ✅ Reverted |
| instanceof narrowing | 1 (interface vs class) | ⚠️ Not fixed |
| `in` operator narrowing | 4+ (unknown, optional, resolution, intersection) | ⚠️ Not fixed |

**Total Critical Bugs**: 8+

## Next Steps

TODO: Ask Gemini to prioritize:
1. Fix instanceof and `in` operator narrowing bugs
2. Continue review of remaining commits
3. Re-implement discriminant narrowing correctly

## Session History

- 2026-02-04: Started review of last 400 commits
- 2026-02-04: Reverted discriminant narrowing commit
- 2026-02-04: Found 5+ additional bugs in narrowing
