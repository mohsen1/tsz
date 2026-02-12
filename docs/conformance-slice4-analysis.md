# Conformance Test Analysis - Slice 4/4

## Current Status
- Slice: 4/4 (offset 9411, max 3134)
- Pass rate: ~53.6% (1678/3133 passing)
- Total failing in slice: 1440 tests

## Completed Work
- ✅ TS2428: Interface type parameter validation (disabled due to binder scope bug)

## High-Impact Fixes (Priority Order)

### 1. Quick Wins - NOT IMPLEMENTED Codes
These codes are never emitted and would give instant test passes:

- **TS1479** (CommonJS/ES module) → 23 tests, 7 single-code wins
- **TS2585** → 7 single-code tests  
- **TS1100** → 6 single-code tests, 12 total
- **TS2343** → 6 single-code tests
- **TS2630** → 12 total tests
- **TS7026** → 17 total tests

### 2. False Positives to Fix
Codes we emit when we shouldn't (reduces noise):

- **TS2339** → 74 false positives
- **TS2345** → 54 false positives
- **TS2322** → 44 false positives
- **TS2304** → 25 false positives
- **TS2318** → 81 false positives (wrong-code category)

### 3. Partial Implementations (Need Broader Coverage)
Codes we emit sometimes but miss in many cases:

- **TS2304** → missing in 138 tests
- **TS2322** → missing in 112 tests
- **TS6053** → missing in 103 tests (File is not a module)
- **TS2339** → missing in 78 tests

### 4. Co-Occurrence Opportunities
Implementing these code pairs passes multiple tests:

- TS2305 + TS2823 → 6 tests
- TS2322 + TS2345 → 4 tests
- TS2304 + TS2339 → 4 tests
- TS1100 + TS2630 → 4 tests

## Known Issues

### TS2428 Binder Bug
The interface type parameter validation (TS2428) is currently disabled because the binder incorrectly merges symbols from different scopes:

```rust
// crates/tsz-checker/src/state_checking.rs:160
// TODO: Re-enable after fixing binder bug where symbols from different scopes
// (e.g. file-scope and namespace-scope) get incorrectly merged into one symbol
// self.check_interface_type_parameters();
```

**Example Issue:**
```typescript
namespace M {
    interface A<T> { x: T; }
}
namespace M2 {
    interface A<T> { x: T; }  // Different scope, should NOT trigger TS2428
}
```

Currently, the binder merges both `A` symbols into one, making validation impossible without fixing the binder first.

## Recommendations for Next Session

1. **Implement TS1479** (CommonJS/ES module import checking) - 23 test impact
2. **Fix TS6053** (File is not a module) - 103 missing cases
3. **Debug TS2318 false positives** (81 tests) - likely over-eager global type checking
4. **Implement TS2585, TS1100, TS2343** - small codes with 6-7 test impact each

## Test Categories

- False Positives: 283 tests (we emit errors when TSC doesn't)
- All Missing: 463 tests (TSC emits errors, we don't)  
- Wrong Codes: 694 tests (both emit errors but different codes)
- Close to Passing: 414 tests (differ by 1-2 error codes)
