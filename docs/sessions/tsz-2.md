# Session TSZ-5: Discriminant Narrowing Robustness

**Started**: 2026-02-05
**Status**: 🔄 IN PROGRESS
**Focus**: Fix critical bugs in discriminant narrowing implementation

## Problem Statement

Recent implementation of discriminant narrowing (commit f2d4ae5d5) had **3 critical bugs** identified by Gemini Pro:
1. **Reversed subtype check** - Asked `is_subtype_of(property_type, literal)` instead of `is_subtype_of(literal, property_type)`
2. **Missing type resolution** - Didn't handle `Lazy`/`Ref`/`Intersection` types
3. **Broken for optional properties** - Failed on `{ prop?: "a" | "b" }` cases

## Goal

Harden discriminant narrowing in `src/solver/narrowing.rs` to handle full complexity of `tsc` narrowing behavior.

## Focus Areas

### 1. Optional Property Discriminants
- Narrowing on unions where discriminant is an optional property
- Example: `{ prop?: "a" | "b" }`
- Must handle `undefined` in the union correctly

### 2. Type Resolution
- Properly resolve `Lazy`/`Ref`/`Intersection` types before narrowing
- Ensure subtype checks work on resolved types, not wrappers

### 3. In Operator Narrowing
- Discriminant narrowing via `in` operator
- Example: `"a" in obj` where obj has optional property `a`

### 4. Instanceof Narrowing
- Discriminant narrowing via `instanceof` operator
- Class constructor discriminants

### 5. Intersection Type Discriminants
- Handle `Intersection` types as discriminants
- Resolve all members of intersection for checking

## Files to Modify

- `src/solver/narrowing.rs` - Main narrowing logic
- `src/solver/subtype.rs` - Relation foundation (may need fixes)
- Test files in `src/solver/tests/` or `src/checker/tests/`

## Mandatory Pre-Implementation Steps

Per AGENTS.md, MUST ask Gemini TWO questions:

### Question 1 (Approach Validation)
```bash
./scripts/ask-gemini.mjs --include=src/solver/narrowing --include=src/solver/subtype \
  "I need to harden discriminant narrowing to handle Lazy/Ref/Intersection types and optional properties.

  Current issues:
  1. Reversed subtype check in literal comparison
  2. Missing type resolution for wrapper types
  3. Optional properties not handled correctly

  What's the right approach? Should I:
  - Add resolution step before narrowing checks?
  - Modify the subtype check logic?
  - Add special handling for optional properties?

  Please provide: 1) File paths, 2) Function names, 3) Edge cases"
```

### Question 2 (Implementation Review)
After implementing, MUST ask:
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver/narrowing \
  "I implemented discriminant narrowing fixes for optional properties and type resolution.

  Changes: [PASTE CODE OR DIFF]

  Please review: 1) Is this correct for TypeScript? 2) Did I miss edge cases?
  Be specific if it's wrong - tell me exactly what to fix."
```

## Debugging Approach

Use `tsz-tracing` skill to understand current behavior:
```bash
TSZ_LOG="wasm::solver::narrowing=trace" TSZ_LOG_FORMAT=tree \
  cargo test test_name -- --nocapture 2>&1 | head -200
```

## Dependencies

- Session tsz-1: Core type relations (may need coordination)
- Session tsz-2: Complete (circular inference)
- Session tsz-3: Narrowing (different domain - control flow)
- Session tsz-4: Emitter (different domain)

## Why This Is Priority

Per Gemini Pro and AGENTS.md:
- **High Impact**: Core TypeScript feature used daily
- **Recent Bugs**: 3 critical bugs found in recent implementation
- **Conformance**: Essential for matching `tsc` behavior exactly
- **User Experience**: Breaks common patterns with optional properties
