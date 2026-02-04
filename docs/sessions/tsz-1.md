# Session tsz-1: TS2318 Core Global Type Checking Fix

**Started**: 2026-02-04 (Continued after parser fixes completion)
**Goal**: Fix TS2318 "Cannot find global type" error reporting to match tsc behavior

## Problem Statement

Currently, tsz only checks for core global types (Array, Boolean, String, etc.) when no libs are loaded. However, tsc checks for these types even when libraries are loaded, emitting TS2318 if a loaded library is missing core types.

**Current Behavior**:
```rust
if !has_lib {
    // Only check when no libs loaded
    for &type_name in CORE_GLOBAL_TYPES { ... }
}
```

**Expected Behavior**: Always check core global types, even when libs are loaded

## Implementation

### Task: Fix check_missing_global_types in state_checking.rs

**File**: `src/checker/state_checking.rs`
**Function**: `check_missing_global_types`

**Changes Required**:
1. Remove `if !has_lib` guard
2. Check core types in all loaded lib contexts
3. Use `has_name_in_lib` or similar to check global availability

**Success Criteria**:
- TS2318 emitted for missing core types even when libs are loaded
- No false positive TS2318 when types exist in libs
- Matches tsc behavior exactly

## Progress

### 2026-02-04: Implementation Started
- Analyzed current implementation in `state_checking.rs`
- Identified `has_name_in_lib` method to check global symbols
- Ready to implement fix
