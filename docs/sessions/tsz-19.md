# Session TSZ-19: Template Literal Number Formatting

**Started**: 2026-02-06
**Status**: 🔄 IN PROGRESS
**Focus**: Fix number to string conversion in template literals to match JavaScript spec

## Problem Statement

Template literals containing numbers should follow ECMAScript `ToString` specification for scientific notation thresholds:
- Use scientific notation if absolute value is `< 10^-6` or `≥ 10^21`
- Otherwise use fixed-point notation

**Current Behavior**: tsz incorrectly formats some numbers in template literals

**Expected Behavior**: Match tsc exactly

## Test Case

```typescript
type T2 = `${1e-7}`;   // Should be "1e-7"
type T4 = `${1e21}`;   // Should be "1e+21"

let x2: T2 = "1e-7";     // tsc: ok, tsz: error
let x4: T4 = "1e+21";    // tsc: ok, tsz: error
```

## Files to Modify

- **Primary**: `src/solver/evaluate_rules/template_literal.rs`

## Success Criteria

- [ ] Test case passes with tsz
- [ ] All edge cases handled: `1e-6`, `1e-7`, `1e20`, `1e21`
- [ ] Conformance tests pass for template literal number formatting

## Progress

### 2026-02-06: Investigation In Progress

**Bug Confirmed**: Template literals with numbers like `${1e-7}` and `${1e21}` are not being evaluated to literal strings.

**Test Case**:
```typescript
type T2 = `${1e-7}`;   // tsc: "1e-7", tsz: widened to `string`
type T4 = `${1e21}`;   // tsc: "1e+21", tsz: widened to `string`
```

**Investigation**:
1. Updated `extract_literal_strings` to handle JS scientific notation:
   - Threshold: `< 10^-6` or `≥ 10^21` → scientific notation
   - Fixed point: between thresholds
   - Added "+" sign for positive exponents in scientific notation

2. **Issue**: Template literals are being widened to `string` instead of evaluated to literal strings
   - Traced to `evaluate_template_literal` in template_literal.rs
   - The evaluation path may not be triggered, or the number literal isn't being stored as a literal

**Next Steps**: Need deeper investigation to find why:
- Template literals with only literals aren't being evaluated to literal strings
- The evaluation path isn't being triggered during type checking
- The numeric literal is being stored/prased incorrectly

