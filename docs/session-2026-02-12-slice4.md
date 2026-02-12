# TypeScript Conformance Session - 2026-02-12 - Slice 4

## Session Overview

**Objective**: Improve conformance test pass rate for slice 4 of 4
- **Slice range**: Tests 9438-12582 (3145 tests)
- **Initial pass rate**: 54.0% (1687/3123 passed)
- **Tests analyzed**: 1438 failing tests

## Work Completed

### 1. TS1479 Implementation ✅
**Commit**: `0deae8f4b` - feat: implement TS1479 (CommonJS importing ES module)

**Impact**: 23 tests affected (7 single-code quick wins)

**Implementation**:
- Detects when CommonJS files import ES modules
- Checks current file is CommonJS (.cts extension or module option is not ESM)
- Checks target file is ESM (.mjs or .mts extension)
- Emits TS1479 with helpful error message
- Handles node16/nodenext module modes correctly

**Code Location**: `crates/tsz-checker/src/import_checker.rs:1648-1664`

**Limitations**:
- Full package.json "type" field detection requires module resolver integration
- Currently handles .cts files and explicit module system checks
- Does not yet detect .ts files in CommonJS packages via package.json

**Example Error**:
```
error TS1479: The current file is a CommonJS module whose imports will produce
'require' calls; however, the referenced file is an ECMAScript module and cannot
be imported with 'require'. Consider writing a dynamic 'import("./module.mjs")'
call instead.
```

## Analysis Results

### Error Code Priority Analysis

#### High-Impact Quick Wins (Not Implemented)
| Error Code | Total Tests | Single-Code Tests | Description |
|------------|-------------|-------------------|-------------|
| TS1479 | 23 | 7 | ✅ **IMPLEMENTED** |
| TS2585 | 10 | 7 | Unknown (needs research) |
| TS2343 | 6 | 6 | Unknown (needs research) |
| TS1100 | 12 | 6 | Reserved word issues |
| TS7026 | 17 | - | JSDoc/type issues |

#### High-Volume False Positives (Need Fixes)
| Error Code | False Positives | Description | Complexity |
|------------|-----------------|-------------|------------|
| TS2339 | 74 | Property does not exist | **HIGH** - Namespace/type-value distinction |
| TS2318 | 83 | Cannot find global type | **HIGH** - 19 emission sites |
| TS2345 | 54 | Argument type mismatch | Medium - Type inference |
| TS2322 | 46 | Type not assignable | Medium - Assignability checks |

#### Partially Implemented (Need Broader Coverage)
| Error Code | Missing | Extra | Gap Analysis |
|------------|---------|-------|--------------|
| TS2304 | 138 | 105 | Cannot find name - scope resolution gaps |
| TS2322 | 112 | 69 | Type assignability - edge cases |
| TS6053 | 103 | 0 | File not found - module resolution |
| TS2339 | 76 | 119 | Property access - see above |

### Co-Occurrence Patterns
These error pairs appear together frequently:
- TS2305 + TS2823 → 6 tests (module + augmentation issues)
- TS2322 + TS2345 → 4 tests (type + argument mismatches)
- TS2304 + TS2339 → 4 tests (name + property resolution)

## Deep Dive: TS2339 False Positives

**Problem**: 74 false positive tests where we emit TS2339 but TypeScript doesn't

**Root Cause** (from previous session + current investigation):
- Namespace/class merging with same name
- Type/value space distinction errors
- Example: `A.Point` can be both a namespace (containing properties) and a class

**Example Test**: `AmbientModuleAndNonAmbientClassWithSameNameAndCommonRoot.ts`
```typescript
declare namespace A {
    export namespace Point {  // Type space entity
        export var Origin: { x: number; y: number }
    }
}

namespace A {
    export class Point {  // Both type and value space
        constructor(public x: number, public y: number) { }
    }
}

// In value context, A.Point should resolve to:
// - class Point constructor (for new A.Point)
// - namespace Point (for A.Point.Origin)
// We incorrectly always resolve to class
```

**Fix Required**:
- Property access resolution must check namespace members
- Type/value context distinction in member access
- Likely in `crates/tsz-solver` property resolution code

**Complexity**: HIGH - Requires understanding TypeScript's complex namespace/class merging rules

## Deep Dive: TS2318 False Positives

**Problem**: 83 false positive tests where we emit "Cannot find global type"

**Root Cause Investigation**:
- `is_known_global_type_name()` has hardcoded list of 100+ global type names
- Emitted from 19 different code locations
- Over-eager checking: emits error even when type IS defined locally or imported

**Code Locations** (19 total):
- `state_type_resolution.rs`: 5 sites
- `type_literal_checker.rs`: 4 sites
- `type_computation_complex.rs`: 2 sites
- `state_checking.rs`: 1 site
- `state_type_analysis.rs`: 1 site
- `state_type_environment.rs`: 2 sites
- Others: 4 sites

**Fix Required**:
- Audit all 19 emission sites
- Ensure local/imported types are checked BEFORE global type check
- May require refactoring type resolution ordering

**Complexity**: HIGH - 19 emission sites, complex type resolution flow

## Session Statistics

**Time Spent**: ~2 hours
**Commits**: 1
**Tests Improved**: ~23 (estimated from TS1479 implementation)
**Pass Rate Change**: 54.0% → ~54.5% (estimated, needs full run to confirm)

## Unit Tests

All unit tests passing:
```bash
cargo nextest run --package tsz-checker
# Result: 359 tests run: 359 passed, 20 skipped
```

## Recommendations for Next Session

### Priority 1: Implement TS2585 (Quick Win)
- **Impact**: 10 tests (7 single-code)
- **Complexity**: Unknown - need to research what TS2585 checks
- **Effort**: 1-2 hours (if simple check)

### Priority 2: Implement TS2343 (Quick Win)
- **Impact**: 6 tests (6 single-code)
- **Complexity**: Unknown - need to research
- **Effort**: 1-2 hours (if simple check)

### Priority 3: TS2339 False Positives (High Impact)
- **Impact**: 74 tests
- **Complexity**: HIGH
- **Effort**: 4-6 hours
- **Approach**:
  1. Understand TypeScript namespace/class merging rules
  2. Trace property access resolution in solver
  3. Add type/value context to member access
  4. Test with namespace merging test cases

### Priority 4: TS2318 False Positives (High Impact)
- **Impact**: 83 tests
- **Complexity**: HIGH
- **Effort**: 6-8 hours
- **Approach**:
  1. Audit all 19 emission sites
  2. Create decision tree for when to emit TS2318
  3. Refactor emission logic to check local/imported first
  4. Add tests for each code path

## Files Modified

1. `crates/tsz-checker/src/import_checker.rs` - TS1479 implementation

## Git Commits

1. `0deae8f4b` - feat: implement TS1479 (CommonJS importing ES module)
   - Synced with main ✅

## Conformance Test Commands

```bash
# Run full slice 4
./scripts/conformance.sh run --offset 9438 --max 3145

# Run with specific error code
./scripts/conformance.sh run --offset 9438 --max 3145 --error-code 1479

# Analyze failures
./scripts/conformance.sh analyze --offset 9438 --max 3145

# Find quick wins (single-code tests)
./scripts/conformance.sh analyze --offset 9438 --max 3145 --category close
```

## Notes

- TS1479 implementation is partial but handles most common cases
- TS2339 and TS2318 false positives require architectural understanding
- Focus on quick wins (TS2585, TS2343) for next session
- Full conformance run needed to confirm exact improvement from TS1479
