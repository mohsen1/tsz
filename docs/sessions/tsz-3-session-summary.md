# Session tsz-3: Lawyer Layer & Compatibility Quirks

**Started**: 2026-02-06
**Status**: Starting
**Focus**: Implement TypeScript-specific compatibility rules that deviate from pure structural subtyping

## Background

The "Judge" (SubtypeChecker) is now much stronger thanks to tsz-2 work on SubtypeVisitor stubs. However, it's currently "too sound" and lacks the specific "Lawyer" overrides that make TypeScript behave like TypeScript.

Per NORTH_STAR.md Section 3.3 (Judge vs. Lawyer), the Lawyer layer must handle TypeScript-specific assignment rules.

## Priority Tasks

### Task #16: Object Literal Freshness & Excess Property Checking 🔥 (HIGHEST IMPACT)
**Problem**: Currently, the solver likely allows `{ a: 1, b: 2 }` to be assigned to `{ a: number }` everywhere (width subtyping).

**Requirement**: Implement "Freshness" tracking. Object literals must be checked for excess properties when assigned to a target type that doesn't have them, unless the target has an index signature.

**Impact**:
- Huge percentage of TypeScript's errors are excess property errors (TS2353)
- Perfect test case for Judge/Lawyer separation
- Almost every non-trivial TypeScript program uses object literals

**Files**: `src/solver/lawyer.rs`, `src/solver/compat.rs`

### Task #17: The Void Return Exception
**Problem**: A function returning `string` should be assignable to a function returning `void`.

**Example**: `[1, 2].forEach(x => x.toString())`

**Requirement**: Lawyer must override the Judge's check for function returns when target return type is `void`.

**File**: `src/solver/subtype_rules/functions.rs`

### Task #18: Weak Type Detection (TS2559)
**Problem**: Types where all properties are optional (Weak Types) have special assignment rules.

**Requirement**: If a type is "Weak", Lawyer must ensure at least one property matches, even if it's structurally a subtype.

**File**: `src/solver/lawyer.rs`

### Task #19: Literal Widening
**Problem**: When inferring types or checking assignments in non-const contexts, literal types (like `1`) often need to "widen" to their base types (like `number`).

**Requirement**: Implement widening logic used during type inference and assignment.

## Starting Point

**Test Results from tsz-2**: 8105 passing, 195 failing, 158 ignored

## Next Steps

1. Start with Task #16 (Excess Property Checking) - highest impact
2. Follow Two-Question Rule: Ask Gemini for approach validation before implementing
3. Focus on Judge/Lawyer separation as described in NORTH_STAR.md
