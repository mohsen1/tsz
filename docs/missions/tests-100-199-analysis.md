# Tests 100-199 Analysis

**Test Range**: Second 100 tests (offset 100, max 100)
**Date**: 2026-02-12
**Status**: Pre-execution analysis (build environment blocked)

## Test Categories in Range 100-199

Based on file naming patterns, tests 100-199 focus on:

### Ambient Declarations (~40% of range)

**File prefix**: `ambient*`

**Test areas**:
- `ambientExternalModule*` - External module declarations
- `ambientFundule` - Function module combinations
- `ambientGetters` - Ambient getter declarations
- `ambientModuleExports` - Module.exports patterns
- `ambientModules` - General ambient module tests
- `ambientNameRestrictions` - Naming rules for ambient context
- `ambientPropertyDeclaration*` - Property declarations
- `ambientRequireFunction` - require() in ambient context
- `ambientStatement*` - Statement validation
- `ambientWith*` - with statements in ambient context

**Key checker areas to focus on**:
- `crates/tsz-checker/src/declaration_checker.rs` - Ambient declaration validation
- `crates/tsz-binder/src/binder.rs` - Ambient context handling
- Ambient declarations have special rules (no initializers, etc.)

**Common issues expected**:
- Missing validation errors for invalid ambient syntax
- Incorrect handling of ambient namespaces
- Module resolution in ambient context

### Ambiguous Types (~10% of range)

**File prefix**: `ambiguous*`

**Test areas**:
- `ambiguousCalls*` - Overload resolution where multiple signatures match
- `ambiguousGeneric*` - Type assertions with generics
- `ambiguousOverload*` - Ambiguous function overloads

**Key solver areas**:
- `crates/tsz-solver/src/subtype.rs` - Overload resolution logic
- `crates/tsz-checker/src/function_type.rs` - Function signature checking
- Overload resolution should pick "best" match, not fail on ambiguity

**Common issues expected**:
- Overload resolution picking wrong signature
- False positive errors when multiple signatures are valid
- Type assertion resolution with generics

### AMD Module System (~20% of range)

**File prefix**: `amd*`

**Test areas**:
- `amdDeclaration*` - AMD declaration file generation
- `amdDependency*` - Dependency comment handling
- AMD (Asynchronous Module Definition) is a module system

**Key areas**:
- `crates/tsz-emitter/src/` - AMD module emission
- `crates/tsz-checker/src/` - AMD module checking
- May involve `define()` function patterns

**Common issues expected**:
- Incorrect AMD module emission
- Missing checks for AMD-specific patterns
- Comment preservation in AMD context

### Mixed Category (~30% of range)

Tests starting with `a*` but not in above categories, including:
- Accessibility modifiers
- Abstract classes
- Accessors (getters/setters)
- Aliasing
- Array types

## Expected Error Patterns

### Pattern 1: Ambient Context Validation

**Likely failing**:
- Missing TS1039: "Initializers are not allowed in ambient contexts"
- Missing TS1046: "Top-level declarations in .d.ts files must start with 'declare' or 'export'"
- Missing TS1183: "'implements' clause already seen"

**Fix location**: `crates/tsz-checker/src/declaration_checker.rs`

**Strategy**: Add validation when `is_ambient` flag is set

### Pattern 2: Overload Resolution

**Likely failing**:
- Extra TS2394: "Overload signature is not compatible with function implementation"
- Missing overload-related errors
- Wrong overload picked in ambiguous cases

**Fix location**: `crates/tsz-checker/src/function_type.rs`

**Strategy**: Improve overload resolution algorithm

### Pattern 3: Module System Checks

**Likely failing**:
- AMD-specific errors
- require() validation
- Module.exports checking

**Fix location**: `crates/tsz-checker/src/import_checker.rs`, emitter

**Strategy**: Add AMD module system support

## Strategic Priorities for This Range

### Priority 1: Ambient Declaration Validation (HIGH ROI)

**Why**:
- ~40% of tests in this range
- Often simple missing checks
- Clear rules from TypeScript spec

**Approach**:
1. Identify which ambient validation errors we're missing
2. Add checks in declaration_checker.rs
3. Guard with `is_ambient` or similar flag
4. Verify doesn't break non-ambient code

**Expected gain**: +10-20 tests

### Priority 2: Overload Resolution Improvements (MEDIUM ROI)

**Why**:
- ~10% of tests
- Core type system feature
- May have cascading benefits

**Approach**:
1. Review overload resolution algorithm
2. Compare with TSC behavior on ambiguous cases
3. Fix resolution logic
4. Add tests for edge cases

**Expected gain**: +5-10 tests

### Priority 3: AMD Module Support (LOWER ROI)

**Why**:
- ~20% of tests but may be complex
- Emitter changes risky
- May not be highest priority

**Approach**:
- Assess current AMD support level
- Add missing checks
- Fix emission if needed
- May defer to later

**Expected gain**: +5-15 tests (but higher risk)

## Test Examples to Investigate First

### 1. ambientStatement1.ts
**Likely issue**: Missing syntax validation in ambient context
**Expected fix**: Add check that certain statements aren't allowed in ambient

### 2. ambiguousOverload.ts
**Likely issue**: Overload resolution picking wrong signature or failing
**Expected fix**: Improve overload resolution tie-breaking

### 3. amdDeclarationEmitNoExtraDeclare.ts
**Likely issue**: Emitting unnecessary 'declare' keywords in AMD context
**Expected fix**: Update emitter logic for AMD modules

## Comparison with Other Slices

### Slice 1 (tests 0-99)
- More basic language features
- Likely higher pass rate

### Slice 2 (tests 100-199) - THIS RANGE
- Ambient declarations (specialist feature)
- Module systems (AMD)
- Likely moderate pass rate

### Slice 3 (tests 6292+)
- Later alphabet (u-z range)
- Advanced features
- Known to be ~62% pass rate

**Hypothesis**: This range may have:
- Lower pass rate than Slice 1 (more specialist features)
- Higher pass rate than Slice 3 (earlier alphabet = more common features)
- **Estimated baseline**: 50-70% pass rate

## Pre-Execution Checklist

Before running tests:
- [ ] Build succeeds: `cargo build --profile dist-fast -p tsz-cli`
- [ ] Unit tests pass: `cargo nextest run`
- [ ] Git state clean: `git status`

After getting baseline:
- [ ] Document actual pass rate
- [ ] Identify top 5 failing error codes
- [ ] List "close" tests (1-2 errors away)
- [ ] Update this analysis with actual findings

## Technical Resources

### TypeScript Spec Sections
- Ambient Declarations: https://github.com/microsoft/TypeScript/blob/main/doc/spec-ARCHIVED.md#11-ambient-declarations
- Module Systems: https://github.com/microsoft/TypeScript/blob/main/doc/spec-ARCHIVED.md#12-modules

### Relevant Code Paths
```
Ambient checking:
  crates/tsz-checker/src/declaration_checker.rs
  crates/tsz-binder/src/binder.rs (is_ambient flag)

Overload resolution:
  crates/tsz-checker/src/function_type.rs
  crates/tsz-solver/src/subtype.rs

Module systems:
  crates/tsz-checker/src/import_checker.rs
  crates/tsz-emitter/src/modules.rs (if exists)
```

### Unit Test Locations
```
Ambient:
  crates/tsz-checker/src/tests/*ambient*.rs

Functions:
  crates/tsz-checker/src/tests/*function*.rs
  crates/tsz-checker/src/tests/*overload*.rs
```

## Success Metrics

### Minimum Success
- **Baseline established**: Know current pass rate
- **Top issues identified**: Know what to fix
- **1-2 fixes made**: Some improvement demonstrated

### Target Success
- **+15-25 tests passing** (15-25% improvement)
- **Ambient validation**: Most ambient errors working
- **No regressions**: Unit tests still passing

### Excellent Success
- **+30-40 tests passing** (30-40% improvement)
- **All "close" tests fixed**: Every test within 1-2 errors now passes
- **Documented learnings**: Clear notes for future work

---

**Status**: Ready for execution once build environment functional
**Next step**: Run `./scripts/conformance.sh run --max=100 --offset=100`
