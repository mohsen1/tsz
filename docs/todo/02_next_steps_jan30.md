# Conformance Improvement TODO List (Jan 30)

## Completed (Jan 30)

- TS2695: Eliminated false positives (tagged templates side effects)
- TS2362: Reduced 23% (enum arithmetic, treat Ref as number-like)
- TS2304: Reduced 92% (embedded lib fallback in tsz_server)
- TSC crashes: Fixed (0 crashes, rootFilePath resolution)
- DRY: Refactored conformance directory (extracted shared utilities)

## High Priority (Highest Impact)

### 1. OOM Errors (100 tests) - CRITICAL
- Problem: Memory limit exceeded when loading embedded libs
- Impact: 100 tests failing with OOM
- Investigation needed:
  - Profile memory usage per test
  - Check for memory leaks in lib loading
  - Consider lib preloading at server startup
  - May need to increase worker memory limit

### 2. Ownership Crashes (4 tests)
- Problem: "attempted to take ownership of Rust value while it was borrowed"
- Tests: classExtendsItself*.ts (circular class inheritance)
- Location: Rust binder/checker, not related to lib loading
- Action: Debug circular reference handling in binder

### 3. TS2339 "Property does not exist" (85x)
- Problem: Property lookup failing
- Likely causes:
  - Prototype chain walking issues
  - lib.d.ts integration incomplete
  - Namespace member resolution gaps

### 4. TS2336 "Property does not exist on type" (80x)
- Similar to TS2339, investigate together

### 5. TS2507 (47x)
- Unknown error code, needs investigation

### 6. TS2307 "Cannot find module" (28x)
- Module resolution issues
- Check @moduleResolution, @baseUrl, @paths handling

## Medium Priority

### 7. TS2322 "Type not assignable" (~402x in full set)
- Deep type system issues
- Requires: generic type parameter investigation, union distribution

### 8. TS1005 "',' expected" (~393x in full set)
- Parser cascades
- Import attributes, using declarations, decorators

### 9. TS2705 (59x in recent run)
- Needs investigation

### 10. TS1109 (30x)
- Needs investigation

## Low Priority / Cleanup

### 11. Extract compiler options builder
- Duplicate TARGET_MAP, MODULE_MAP in runners
- Consolidate to shared utility

### 12. TypeScript constants mappings
- ScriptTarget, ModuleKind mappings duplicated
- Extract to shared module

### 13. Lib loading utilities
- Further consolidation possible
- Lib manifest generation, resolution

### 14. Document conformance test architecture
- README for conformance directory
- Architecture diagram
- Contributing guide

## Code Quality

### 15. Investigate circular class inheritance crashes
- Add proper cycle detection
- Emit appropriate errors instead of crashing

### 16. Memory profiling for embedded lib loading
- Identify memory bottlenecks
- Optimize or increase limits

## Test Infrastructure

### 17. Improve test filtering
- Add --filter for specific error codes
- Add --filter for specific test files
- Support regex patterns

### 18. Better error reporting
- Show which tests have which errors
- Group by error category
- Trend tracking over time

## Next Session Priorities

1. Fix OOM errors (highest impact, blocks 100 tests)
2. Debug ownership crashes (blocks 4 tests)
3. Investigate TS2339/TS2336 (85x + 80x = 165 tests)
4. Profile and optimize memory usage

## Metrics to Track

- Pass rate: Current 32.2%, target 50%
- TSC crashes: Current 0, maintain 0
- tsz crashes: Current 4, target 0
- OOM errors: Current 100, target 0
- Top extra errors: Monitor trends

## Estimated Impact

If we fix top 5 issues:
- OOM (100) + Crashes (4) = 104 tests
- TS2339 (85) + TS2336 (80) = 165 tests
- TS2507 (47) + TS2307 (28) = 75 tests

Total potential: 344 tests flipped from FAIL to PASS
This would bring pass rate from 32% to approximately 50%
