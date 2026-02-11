# TS1117 Duplicate Object Literal Properties Investigation

## Summary
5 tests fail due to missing TS1117 "An object literal cannot have multiple properties with the same name" errors.

## Current Status
We DO emit TS1117 in some cases (ES5 target) but miss it in others:
- ✅ ES5 target: duplicate named properties → TS1117
- ❌ ES2015+ with "use strict" directive: duplicate named properties → should emit TS1117 but don't
- ❌ Computed property names with constant values: duplicates → should emit TS1117 but don't
- ❌ Class with numeric properties in different formats (0b11 vs 3) → should emit TS1117 but don't

## Failing Tests
1. `duplicatePropertiesInStrictMode.ts` - "use strict" directive, ES2015 target
2. `duplicateObjectLiteralProperty_computedName1/2/3.ts` - computed names like `[n]` where n is const
3. `duplicateIdentifierDifferentSpelling.ts` - numeric literals 0b11 and 3 (both evaluate to 3)

## Root Causes

### 1. Strict Mode Detection
**Location**: `crates/tsz-checker/src/type_computation.rs:1688`

```rust
if !skip_duplicate_check
    && self.ctx.compiler_options.target.is_es5()  // ❌ Only checks ES5
    && properties.contains_key(&name_atom)
```

**Problem**:
- We only check ES5 target
- We should also check for ECMAScript "use strict" directive
- `self.ctx.is_strict_mode()` checks TypeScript --strict flag, NOT "use strict" directive
- These are different things:
  - TypeScript `--strict`: enables strict type checking (noImplicitAny, etc.)
  - ECMAScript `"use strict"`: runtime semantics, affects duplicate property rules

**Fix Required**:
- Detect "use strict" directive at file/function level
- Parser may need to track this during parsing
- OR checker needs to scan for strict directives

### 2. Computed Property Names
**Location**: `crates/tsz-checker/src/type_computation.rs:1716-1720`

```rust
} else {
    // Computed property name that can't be statically resolved
    self.check_computed_property_name(prop.name);
    self.get_type_of_node(prop.initializer);
}
```

**Problem**:
- We skip ALL computed properties, even constant ones
- `[n]` where `n = 1` should be resolvable to constant value
- Constant computed properties should be checked for duplicates

**Example**:
```typescript
const n = 1;
const obj = {
    [n]: 1,    // OK
    [n]: 2     // ❌ Should emit TS1117
};
```

**Fix Required**:
- Try to evaluate computed property expression as constant
- If constant, convert to string and check for duplicates
- Need constant folding/evaluation for simple cases

### 3. Numeric Literal Normalization
**Location**: Same as #1

**Problem**:
- `0b11` (binary) and `3` (decimal) both represent the number 3
- Should be treated as duplicates
- Currently treated as different strings

**Example**:
```typescript
var x = {
    0b11: 'a',  // binary 3
    3: 'b'      // decimal 3 - ❌ Should emit TS1117
};
```

**Fix Required**:
- Normalize numeric property names to a canonical form
- Parse numeric literals and compare values, not strings
- May need to handle: binary (0b), octal (0o), hex (0x), decimal

## Implementation Plan

### Phase 1: Numeric Normalization (Easiest)
1. In `get_property_name_from_object_literal()`, detect numeric literals
2. Parse binary/octal/hex to decimal value
3. Use decimal value as canonical name for duplicate checking

### Phase 2: Constant Computed Properties (Medium)
1. When encountering computed property `[expr]`, try to evaluate as constant
2. If expr is:
   - Literal: use literal value
   - Reference to const variable: lookup value
   - Symbol/enum: use unique identifier
3. Add to properties map and check for duplicates

### Phase 3: Strict Mode Detection (Harder)
1. Parser needs to track "use strict" directive
2. Add `has_use_strict_directive` to node flags or separate field
3. Checker checks both ES5 target OR use strict directive

## Code Locations
- Duplicate checking: `crates/tsz-checker/src/type_computation.rs` lines 1683-1700, 1768-1783, 1817-1832, 1929-1947
- Property name extraction: Same file, around line 1660
- Computed property checking: `crates/tsz-checker/src/property_checker.rs:127`

## Note
Attempted fix adding `|| self.ctx.is_strict_mode()` but this checks TypeScript --strict flag, not "use strict" directive. This caused pass rate to decrease from 985 to 981, so was reverted.

## Impact
- 5 tests affected in slice 2
- Quick win if we implement phases 1-2
- Phase 3 requires parser changes
