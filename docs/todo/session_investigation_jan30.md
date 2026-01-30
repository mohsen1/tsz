# Session Investigation - January 30, 2026 (Late)

## Current Conformance State

**Pass Rate: 43.3% (5,361/12,379)** - Excellent progress!

### Top Extra Errors (tsz emits, tsc doesn't):
1. **TS2322: 10,991x** - Type 'X' is not assignable to type 'Y'
2. TS1005: 1,442x - Parser errors
3. TS2339: 1,278x - Property does not exist
4. TS7010: 1,260x - Export errors
5. TS2304: 909x - Cannot find name

## Investigation Summary

### Completed During This Session:

1. **using/await using declarations** - Already fixed earlier
   - Parser support for ES2022 `using` keyword
   - Parser support for `await using` combined syntax
   - Fixed TS1359 for async class methods

2. **Error Category Investigation** - Created investigation tasks for:
   - TS2507: Type not a constructor (20x in 500-sample)
   - TS1109: Expression expected parser errors (18x)
   - TS2322: Type assignment regression (CRITICAL)

### Key Findings:

#### TS2322 Regression (Highest Priority)
- **Impact**: 10,991x errors, blocking ~1,500-2,000 tests
- **Status**: Known regression from earlier work
- **Files**: Likely in src/checker/type_checking.rs or solver
- **Action**: Needs dedicated investigation session

#### TS2507 Analysis
- Emitted from `type_computation_complex.rs` for `new` expressions
- Three call sites:
  - Line 204: Intersection types without construct signatures
  - Line 244: Type parameter intersection constraints
  - Line 292: NotConstructable classification
- Has proper error suppression for ANY/ERROR/UNKNOWN
- Root cause: tsz doesn't recognize some types as having construct signatures

#### TS1109 Parser Errors
- "Expression expected" - emitted from 20+ locations throughout parser
- Appears in both extra and missing error lists
- Indicates error recovery/suppression logic misalignment with TypeScript

#### TS2445 Protected Properties
- Known issue documented in code (state_type_environment.rs:907)
- False positives when `enclosing_class` context not set
- Already has workaround for variable/parameter symbols

## Priority for Next Session

### Immediate (Highest Impact):
1. **TS2322 Investigation** - 10,991x errors
   - Check git log for commits around regression
   - Find concrete test case demonstrating regression
   - Identify root cause in type assignment logic
   - Estimated impact: +1,500-2,000 tests if fixed

### Medium Priority:
2. **Timeout Tests** - 82 tests timing out
   - 4 circular inheritance tests (classExtendsItself*)
   - Other complex flow tests
   - May need cycle detection or timeout improvements

3. **OOM Tests** - 14 tests
   - dependentDestructuredVariables.ts
   - controlFlowOptionalChain.ts
   - Memory profiling needed

### Lower Priority:
4. **TS2507** - Complex type system issue
5. **TS1109** - Parser error recovery complexity

## Files Modified This Session

None - investigation only session

## Recommendations

1. **Start next session with TS2322 investigation** - this is the clear #1 priority
2. Use `git bisect` to find the commit that caused the regression
3. Focus on solver or type checking changes
4. Create minimal test cases to isolate the issue
5. Consider reverting the problematic commit if fix is too complex
