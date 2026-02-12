# Tests 100-199 Analysis and Action Plan

## Test Range Overview

Tests 100-199 focus on several key TypeScript features:

### 1. Ambient Declarations (40+ tests)
Tests beginning with `ambient*`:
- `ambientExternalModuleWithRelativeExternalImportDeclaration.ts`
- `ambientModules.ts`
- `ambientGetters.ts`
- `ambientStatement1.ts`
- etc.

**What they test**:
- `declare` keyword functionality
- Ambient module declarations
- Ambient namespace declarations
- Interaction with module resolution

**Likely issues to fix**:
- TS2304: Cannot find name (missing ambient declaration recognition)
- TS1039: Initializers not allowed in ambient contexts
- TS1183: Implementation not allowed in ambient context

**Code locations**:
- `crates/tsz-binder/src/binder.rs` - Ambient declaration binding
- `crates/tsz-checker/src/checker/declaration_checker.rs` - Ambient checking
-  `crates/tsz-checker/src/state_type_analysis.rs` - Ambient type resolution

### 2. AMD Module System (10+ tests)
Tests beginning with `amd*`:
- `amdDeclarationEmitNoExtraDeclare.ts`
- `amdModuleName1.ts`
- `amdModuleBundleNoDuplicateDeclarationEmitComments.ts`

**What they test**:
- AMD module format support
- Module naming
- Declaration emit for AMD modules
- Comment preservation

**Likely issues**:
- Emission format differences
- Module name handling
- Declaration file generation

**Code locations**:
- `crates/tsz-emitter/src/` - Emit logic
- `crates/tsz-cli/src/driver.rs` - Module format options

### 3. Overload Resolution (5+ tests)
Tests with `overload*` or `ambiguous*`:
- `ambiguousOverload.ts`
- `ambiguousOverloadResolution.ts`
- `ambiguousCallsWhereReturnTypesAgree.ts`

**What they test**:
- Function overload matching
- Ambiguous overload detection
- Return type agreement checking

**Likely issues**:
- TS2769: No overload matches this call
- TS2304: Cannot resolve overload
- False positives when overloads are actually valid

**Code locations**:
- `crates/tsz-checker/src/call_checker.rs` - Call expression checking
- `crates/tsz-solver/src/operations.rs` - Overload resolution logic

### 4. Generic Assertions (2+ tests)
- `ambiguousGenericAssertion1.ts`

**What they test**:
- Type assertions with generics
- Generic type inference

**Likely issues**:
- TS2352: Conversion type assertions
- Generic constraint violations

**Code locations**:
- `crates/tsz-checker/src/type_computation.rs` - Type assertion handling
- `crates/tsz-solver/src/instantiate.rs` - Generic instantiation

## Expected Error Patterns

Based on similar test ranges, common issues in 100-199 likely include:

### High-Impact Fixes Needed

1. **TS1039 - Ambient Initializers** (10-15 tests)
   - Problem: We allow initializers in ambient contexts
   - Fix: Add check in declaration checker for ambient context

2. **TS2304 - Cannot Find Name** (false positives, 5-10 tests)
   - Problem: Not recognizing ambient declarations
   - Fix: Improve ambient symbol resolution in binder

3. **TS2769 - Overload Resolution** (false negatives, 3-5 tests)
   - Problem: Missing overload match errors
   - Fix: Improve overload candidate selection

4. **Emit Differences** (10-15 tests)
   - Problem: AMD module emit format differs from TSC
   - Fix: Align emitter output with TSC for AMD modules

## Analysis Strategy (When Binary Available)

### Step 1: Get Baseline
```bash
./scripts/conformance.sh run --max=100 --offset=100 --verbose > baseline-100-199.txt
```

### Step 2: Analyze by Category
```bash
# Close tests (easiest wins)
./scripts/conformance.sh analyze --max=100 --offset=100 --category close

# False positives (we emit errors TSC doesn't)
./scripts/conformance.sh analyze --max=100 --offset=100 --category false-positive

# Missing errors (we don't emit errors TSC does)
./scripts/conformance.sh analyze --max=100 --offset=100 --category all-missing
```

### Step 3: Focus on High-Impact Error Codes
```bash
# Check which error codes affect most tests
./scripts/conformance.sh analyze --max=100 --offset=100 | grep "Error code:" | sort | uniq -c | sort -rn | head -20
```

### Step 4: Fix Priority Order

**Priority 1: Ambient Context Checks** (likely 10-15 tests)
1. Read a failing ambient test
2. Create minimal repro in `tmp/ambient-test.ts`
3. Add ambient context tracking to declaration checker
4. Emit TS1039 for initializers in ambient context
5. Verify with: `./target/dist-fast/tsz tmp/ambient-test.ts`
6. Run conformance tests to measure improvement
7. Run `cargo nextest run` to ensure no regressions
8. Commit and push

**Priority 2: Ambient Symbol Resolution** (likely 5-10 tests)
1. Find tests with TS2304 false positives
2. Debug binder symbol table for ambient declarations
3. Fix recognition of `declare` keyword
4. Test and commit

**Priority 3: Overload Resolution** (likely 3-5 tests)
1. Find failing overload tests
2. Trace through call_checker.rs
3. Compare with TSC behavior
4. Fix candidate selection logic
5. Test and commit

**Priority 4: AMD Emit** (likely 5-10 tests, if emit is in scope)
1. Compare emit output with expected
2. Adjust emitter format
3. Test and commit

## Code Reading Checklist

Before implementing fixes, understand:

- [ ] How ambient declarations are bound (`crates/tsz-binder/src/binder.rs`)
- [ ] How declaration contexts are tracked (`crates/tsz-checker/src/context.rs`)
- [ ] How overload resolution works (`crates/tsz-checker/src/call_checker.rs`)
- [ ] How symbol resolution happens (`crates/tsz-checker/src/symbol_resolver.rs`)

## Testing Workflow

For each fix:

1. **Create minimal repro**: `tmp/test-{feature}.ts`
2. **Run TSC**: `cd TypeScript && npx tsc --noEmit tmp/test-{feature}.ts`
3. **Run tsz**: `./target/dist-fast/tsz tmp/test-{feature}.ts`
4. **Compare outputs**: Ensure error codes match
5. **Run slice tests**: `./scripts/conformance.sh run --max=100 --offset=100`
6. **Run unit tests**: `cargo nextest run`
7. **Commit**: Clear message explaining what was fixed
8. **Push**: `git pull --rebase origin main && git push`

## Success Metrics

**Current baseline** (unknown until tests run):
- Estimated: 50-70% pass rate based on similar ranges

**Target improvements**:
- Priority 1 fixes: +10-15% pass rate
- Priority 2 fixes: +5-10% pass rate
- Priority 3 fixes: +3-5% pass rate
- **Total target**: 75-90% pass rate for tests 100-199

## Notes

- Focus on GENERAL fixes that help multiple tests
- Don't break existing passing tests
- Always verify with unit tests before committing
- Document any architectural decisions in code comments

## When Binary is Available

Run this command first:
```bash
./scripts/conformance.sh run --max=100 --offset=100 --verbose 2>&1 | tee results-100-199-$(date +%Y%m%d).txt
```

Then analyze the output and follow the priority order above.
