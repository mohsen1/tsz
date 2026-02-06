# Session TSZ-3: Method Bivariance Implementation

**Started**: 2026-02-06
**Status**: 🔄 READY TO START
**Predecessor**: Object Literal Freshness (Completed)

## Task

Implement method bivariance - a critical TypeScript compatibility rule in the "Lawyer" layer.

## Problem Statement

TypeScript treats function properties **strictly** (contravariant parameters), but methods **bivariantly** (both co- and contravariant). This is essential for common patterns like:

```typescript
// Array<string | number> should be assignable to Array<string>
// because methods are bivariant
const arr1: Array<string | number> = ["a", 1];
const arr2: Array<string> = arr1;  // Should work!

// Event handlers use this pattern
class EventEmitter {
    on(fn: (data: string) => void): void;
}

// This should work because methods are bivariant
const emitter: EventEmitter = {
    on(fn: (data: string | number) => void) {  // Wider parameter type
        fn("hello");
    }
};
```

Without method bivariance, simple DOM operations and Array manipulations will fail type checking.

## Current Architecture

From NORTH_STAR.md Section 3.3:
- **Judge** (`src/solver/subtype.rs`): Pure structural subtyping
- **Lawyer** (`src/solver/compat.rs`): TypeScript-specific compatibility rules (includes Freshness ✅, needs Method Bivariance)

## Key Question

How do we distinguish between **methods** and **function properties** in the current type system?

According to NORTH_STAR.md:
- Methods: Properties with `FunctionType` where the function is marked as a method
- Function properties: Properties with `FunctionType` where the function is NOT marked as a method

## Implementation Plan (To be validated with Gemini)

1. **Investigation Phase**:
   - Ask Gemini: "How does the current TypeKey/TypeId structure distinguish methods from function properties?"
   - Find where function types are created and marked
   - Understand the current subtype checking logic for functions

2. **Implementation Phase** (after validation):
   - Modify `src/solver/subtype.rs` to check if a function is a method
   - Apply bivariant parameter checking for methods
   - Apply contravariant parameter checking for function properties
   - Add tests

## Files to Investigate

- `src/solver/subtype.rs` - Subtype checking logic
- `src/solver/compat.rs` - Lawyer compatibility layer
- `src/solver/intern.rs` - Type interning (how are functions created?)
- `src/checker/tests/` - Look for existing bivariance tests

## Next Step

Ask Gemini Question 1 (Approach Validation) before writing any code:

```bash
./scripts/ask-gemini.mjs --include=src/solver "I need to implement method bivariance.
Problem: TypeScript treats methods bivariantly but function properties strictly.
How do I distinguish methods from function properties in the current TypeKey structure?
What functions should I modify? Are there edge cases I'm missing?"
```
