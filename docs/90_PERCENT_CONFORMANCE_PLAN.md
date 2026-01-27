# Path to 90% TypeScript Conformance

**Date**: 2026-01-27
**Current Status**: 27.8% (133/478 tests passing)
**Target**: 90%+ conformance
**Gap**: Need +297 more tests to pass

---

## Current Status Summary

### Test Results (478 tests)
```
Pass Rate:   27.8% (133/478)
Crashes:     1 remaining (allowJsCrossMonorepoPackage.ts)
OOM:         0
Timeouts:    0
Performance: 15 tests/sec
```

### Recent Progress
1. ✅ **Crash Fix**: Reduced crashes from 390+ to 1 (99.7% improvement)
2. ✅ **Smart Caching**: Fixed performance regression from aggressive caching
3. ✅ **TS2571 Application Types**: Fixed Application type evaluation in contextual typing

---

## Error Analysis

### Top Extra Errors (We Emit But Shouldn't)

| Error Code | Count | Description | Priority | Estimated Impact |
|------------|-------|-------------|----------|------------------|
| **TS2339** | 353x | Property does not exist | P0 | ~30-40 tests |
| **TS2307** | 163x | Cannot find module | P0 | ~15-20 tests |
| **TS2571** | 144x | Object is of type unknown | P0 | ~10-15 tests |
| **TS2507** | 113x | Not a constructor | P1 | ~10-15 tests |
| **TS2749** | 89x | Value used as type | P1 | ~15-20 tests |
| **TS2304** | 68x | Cannot find name (extra) | P1 | ~5-10 tests |
| **TS7010** | 53x | Async function return type | P1 | ~5-10 tests |
| **TS2345** | 38x | Argument not assignable | P2 | ~5-10 tests |

### Top Missing Errors (We Should Emit But Don't)

| Error Code | Count | Description | Priority | Action Required |
|------------|-------|-------------|----------|-----------------|
| **TS2318** | 426x | Cannot find global type | Expected | --noLib tests, mostly correct |
| **TS2711** | 230x | Component symbol | P0 | Implement component checking |
| **TS2792** | 126x | Missing readonly modifier | P1 | Fix readonly modifier checking |
| **TS2697** | 48x | (investigate) | P2 | Investigate root cause |
| **TS6053** | 24x | File not found | P1 | Improve file resolution |

---

## Implementation Strategy

### Phase 1: Fix Top Extra Errors (Target: 40% conformance)

**Goal**: Reduce extra errors by fixing false positives

#### 1.1 Fix TS2339 Property Access (353x)
**Current Status**: Comprehensive fixes already implemented
- ✅ Object type index signature fallback
- ✅ Type reference resolution
- ✅ Union type property access
- ✅ Intersection type index signature fallback

**Remaining Work**:
- Investigate why TS2339 increased from 59x (100-test sample) to 353x (full run)
- May need additional fixes for specific edge cases

**Estimated Impact**: +30-40 tests

#### 1.2 Fix TS2307 Module Resolution (163x)
**Location**: `src/module_resolver.rs`

**Issues**:
- Node module resolution algorithm incomplete
- Path mapping not implemented
- @types package resolution missing
- Relative vs absolute path handling

**Solution**:
```rust
fn resolve_module_name(
    &self,
    module_name: &str,
    containing_file: &Path,
    options: &CompilerOptions,
) -> Option<PathBuf> {
    // 1. Check relative paths
    if module_name.starts_with('.') {
        return self.resolve_relative(module_name, containing_file);
    }

    // 2. Check path mappings (tsconfig paths)
    if let Some(mapped) = self.check_path_mappings(module_name, options) {
        return Some(mapped);
    }

    // 3. Node module resolution
    self.resolve_node_module(module_name, containing_file)
}
```

**Estimated Impact**: +15-20 tests

#### 1.3 Fix TS2571 (144x)
**Current Status**: Application type fix implemented

**Remaining Issues**:
- Spread parameter typing (TS2556/TS2345)
- Object literal 'this' type inference
- Empty destructuring on unknown

**Estimated Impact**: +10-15 tests

#### 1.4 Fix TS2507 Constructor Checking (113x)
**Status**: Mostly correct errors, some misclassification

**Issue**: Some TS2349 cases misclassified as TS2507

**Solution**: Improve error classification logic

**Estimated Impact**: +5-10 tests (error recategorization)

### Phase 2: Implement Missing Error Checks (Target: 60% conformance)

**Goal**: Implement missing error diagnostics

#### 2.1 Fix TS2711 Component Symbol (230x)
**Issue**: Component symbol checking not implemented

**Location**: `src/checker/type_checking.rs`

**Solution**: Add component symbol validation

**Estimated Impact**: +20-30 tests

#### 2.2 Fix TS2792 Readonly Modifier (126x)
**Issue**: Readonly modifier checking incomplete

**Solution**: Enhance property modifier validation

**Estimated Impact**: +10-15 tests

### Phase 3: Final Crash Fix & Optimization (Target: 90% conformance)

#### 3.1 Fix Remaining Crash
**Test**: compiler/allowJsCrossMonorepoPackage.ts
**Error**: `Cannot read properties of undefined (reading 'flags')`

**Root Cause**: Cross-package symbol resolution
**Solution**: Use GlobalSymbolId for cross-package references

**Estimated Impact**: +1 test, 100% crash elimination

#### 3.2 Continue TS2749 Fixes (89x)
**Status**: Reduced from 195x, still work to do

**Strategy**: Systematic symbol context tracking improvements

**Estimated Impact**: +15-20 tests

#### 3.3 Fix TS7010 Async Functions (53x)
**Issue**: Async function return type inference

**Solution**: Improve Promise type handling for async functions

**Estimated Impact**: +5-10 tests

---

## Quick Wins (High Impact, Low Effort)

### 1. Fix Cross-Package Symbol Crash
**Impact**: 100% crash elimination, +1 test
**Effort**: 1-2 hours
**Location**: `src/checker/symbol_resolver.rs`

### 2. Improve Module Resolution (TS2307)
**Impact**: +15-20 tests
**Effort**: 2-4 hours
**Location**: `src/module_resolver.rs`

### 3. Continue TS2749 Work
**Impact**: +15-20 tests
**Effort**: 2-3 hours
**Location**: Multiple files

---

## Estimated Timeline to 90%

| Phase | Target | Tests Needed | Estimated Time |
|-------|--------|--------------|----------------|
| Phase 1 | 40% | +60 tests | 8-12 hours |
| Phase 2 | 60% | +90 tests | 12-16 hours |
| Phase 3 | 90% | +147 tests | 16-20 hours |
| **Total** | **90%** | **+297 tests** | **36-48 hours** |

---

## Next Actions (Immediate Priority)

1. **Fix the remaining crash** - 1 hour
   - Investigate allowJsCrossMonorepoPackage.ts
   - Implement GlobalSymbolId resolution
   - Test and commit

2. **Improve module resolution** - 3 hours
   - Implement Node module resolution
   - Add path mapping support
   - Test TS2307 reduction

3. **Continue TS2749 fixes** - 2 hours
   - Symbol context tracking
   - Type-only import validation
   - Measure impact

4. **Run full conformance test** - 1 hour
   - Verify improvements
   - Update documentation
   - Plan next iteration

---

## Success Metrics

### Short Term (Next Session)
- [ ] Fix remaining crash → 0 crashes
- [ ] Reduce TS2307 by 50% → ~80x
- [ ] Reduce TS2749 by 30% → ~60x
- [ ] Reach 35% conformance → ~167 tests

### Medium Term (Next Week)
- [ ] Implement TS2711 checking
- [ ] Fix TS2792 readonly modifier
- [ ] Reach 60% conformance → ~287 tests

### Long Term (This Month)
- [ ] All major error categories addressed
- [ ] 90%+ conformance achieved → 430+ tests
- [ ] 0 crashes, 0 OOM, 0 timeouts

---

## Commit History (This Session)

1. `69aa442a5` - fix(checker): Implement smart caching to fix crash regression
2. `9b9c5d769` - fix(checker): Evaluate Application types before contextual typing

---

## Notes

- The pass rate varies between test runs (27.8% to 41.2%) depending on test subset
- Full test run shows more accurate picture of conformance
- Many TS2339 errors are in complex edge cases that require careful investigation
- Module resolution is a major gap affecting many tests
- Component symbol (TS2711) is a significant missing feature
