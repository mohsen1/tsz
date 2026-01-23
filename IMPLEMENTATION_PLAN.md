# Implementation Plan: Fixing Top Priority Issues

**Created:** 2026-01-23
**Target:** Reduce TS2322 false positives from 12,122 to <5,000, improve pass rate from 36.3% to 50%+

---

## Executive Summary

This plan addresses the top sources of false positives in TSZ. The primary issue is **double weak type checking** causing 12,122 extra TS2322 errors. The fix is straightforward: remove the redundant check in SubtypeChecker.

---

## Phase 1: TS2322 False Positives (12,122 extra errors) - TOP PRIORITY

### Root Cause: Double Weak Type Checking

The solver checks weak types **redundantly in two places**:

| Layer | File | Lines | What It Does |
|-------|------|-------|--------------|
| CompatChecker | `src/solver/compat.rs` | 167-169 | Checks `violates_weak_type()` - **KEEP** |
| SubtypeChecker | `src/solver/subtype.rs` | 375 | Checks `violates_weak_type()` - **REMOVE** |

When CompatChecker calls SubtypeChecker, borderline cases get rejected twice.

### Step 1.1: Remove Redundant Weak Type Check (Quick Win)

**File:** `src/solver/subtype.rs`

**Change 1:** Lines 375-377 - Remove in `check_subtype_inner()`
```rust
// REMOVE these lines:
if self.enforce_weak_types && self.violates_weak_type(source, target) {
    return SubtypeResult::False;
}
```

**Change 2:** Lines 3724-3729 - Remove in `explain_failure()`
```rust
// REMOVE these lines:
if self.enforce_weak_types && self.violates_weak_type(source, target) {
    return Some(SubtypeFailureReason::NoCommonProperties {
        source_type: source,
        target_type: target,
    });
}
```

**Expected Impact:** 30-50% reduction in TS2322 false positives (~4,000-6,000 fewer errors)

**Risk:** LOW - CompatChecker still enforces weak type rules

### Step 1.2: Alternative - Disable Flag

If removing code is concerning, just flip the flag:

**File:** `src/solver/subtype.rs` - Lines 205 and 227
```rust
// Change from:
enforce_weak_types: true,
// To:
enforce_weak_types: false,
```

### Step 1.3: Fix Union Weak Type Logic (Medium Risk)

**File:** `src/solver/compat.rs` - Lines 388-393

**Problem:** Using `.any()` means if ANY union member violates weak type, the whole assignment fails. Should use `.all()` - only fail if ALL members violate.

```rust
// BEFORE:
TypeKey::Union(members) => {
    members.iter().any(|member| self.violates_weak_type_with_target_props(*member, target_props))
}

// AFTER:
TypeKey::Union(members) => {
    members.iter().all(|member| self.violates_weak_type_with_target_props(*member, target_props))
}
```

**Risk:** MEDIUM - Needs verification against TypeScript behavior first

---

## Phase 2: TS2304/TS2694 False Positives (6,902 combined)

### Root Cause

Symbol resolution marking valid names as "not found":
- **TS2304 (3,798 extra):** Cannot find name - global symbols not resolved
- **TS2694 (3,104 extra):** Namespace has no exported member - exports not populated

### Investigation Files

| File | Purpose |
|------|---------|
| `src/binder/state.rs:3701-3714` | `get_symbol()` - core symbol lookup |
| `src/binder/state.rs:827-831` | Lib binder integration |
| `src/checker/state.rs:2621-2722` | `resolve_qualified_name()` - emits TS2694 |

### Action Items

1. **Verify lib binders checked everywhere:** Ensure `get_symbol()` checks lib binders in all code paths
2. **Audit qualified name resolution:** Check if exports table is fully populated
3. **Review declaration merging:** Ensure merged declarations combine exports

### Debug Approach
```bash
# Find tests with TS2304 false positives
./conformance/run-conformance.sh --native --max=50 --verbose 2>&1 | grep -B5 "Extra: TS2304"

# Compare specific test
npx tsc --noEmit path/to/test.ts  # Should have NO errors
cargo run -- --check path/to/test.ts  # Incorrectly emits TS2304
```

---

## Phase 3: TS1005 Parser Errors (2,706 extra)

### Root Cause

Parser error recovery in `parse_expected()` emitting errors where TSC doesn't.

### Investigation Files

| File | Purpose |
|------|---------|
| `src/parser/state.rs:300-365` | `parse_expected()` - error suppression logic |

### Action Items

1. **Review error suppression list:** May need to add more token types
2. **Compare parser output:** Find patterns in which constructs trigger extra TS1005

---

## Implementation Sequence

### Commit 1: Remove redundant weak type check
```bash
# Apply changes to src/solver/subtype.rs
# Remove lines 375-377 and 3724-3729

# Test
cargo test --lib solver::
./conformance/run-conformance.sh --native --max=100

# Commit
git add src/solver/subtype.rs
git commit -m "fix(solver): remove duplicate weak type check in SubtypeChecker

CompatChecker already performs weak type checking at lines 167-170.
SubtypeChecker was checking again at line 375, causing borderline
cases to fail twice. This was causing ~12,122 extra TS2322 errors.

Removed redundant checks:
- subtype.rs:375-377 - check_subtype_inner weak type check
- subtype.rs:3724-3729 - explain_failure weak type check"
```

### Commit 2: Fix union weak type logic (after testing)
```bash
# Change .any() to .all() in src/solver/compat.rs:388-393
```

### Commit 3: Symbol resolution fixes (after investigation)

### Commit 4: Parser error recovery fixes (after investigation)

---

## Testing Strategy

### Before Each Change
```bash
# Run unit tests
cargo test --lib solver::subtype_tests
cargo test --lib solver::compat_tests

# Quick conformance check
./conformance/run-conformance.sh --native --max=100
```

### After All Changes
```bash
# Full conformance
./conformance/run-conformance.sh --native --all --workers=8
```

---

## Success Metrics

| Phase | Current | Target |
|-------|---------|--------|
| TS2322 Extra | 12,122 | < 5,000 |
| TS2304/TS2694 Extra | 6,902 | < 3,000 |
| TS1005 Extra | 2,706 | < 1,500 |
| **Pass Rate** | **36.3%** | **50%+** |

---

## Key Files Reference

### Phase 1 (TS2322)
- `src/solver/subtype.rs:375-377` - **REMOVE** redundant check
- `src/solver/subtype.rs:3724-3729` - **REMOVE** redundant check in explain_failure
- `src/solver/compat.rs:167-170` - **KEEP** authoritative weak type check
- `src/solver/compat.rs:388-393` - **REVIEW** .any() vs .all() for unions

### Phase 2 (TS2304/TS2694)
- `src/binder/state.rs:3701-3714` - Symbol lookup
- `src/checker/state.rs:2621-2722` - Qualified name resolution

### Phase 3 (TS1005)
- `src/parser/state.rs:300-365` - Error suppression logic
