# Session TSZ-3: Void Return Compatibility Implementation

**Started**: 2026-02-06
**Status**: 🔄 READY TO START
**Predecessor**: Object Literal Freshness (Completed)

## Task

Implement the **"Void Exception"** - a critical TypeScript compatibility rule in the Lawyer layer.

## Problem Statement

TypeScript allows a function returning **any type** to be assignable to a function type returning `void`. This is a special exception to standard structural typing.

### Example that should work but might fail:

```typescript
const list = [1, 2, 3];
// This should work: forEach accepts (x: number) => void
// Arrow returns string, but that's OK because target expects void
list.forEach(x => x.toString());
```

### The Void Exception Rule

When checking if a function type is assignable to another function type:
- **Normal rule**: Source return type must be subtype of target return type
- **Void exception**: If target return type is `void`, ANY source return type is acceptable

## Architecture

From NORTH_STAR.md Section 3.3:
- **Judge** (`src/solver/subtype.rs`): Pure structural subtyping (would reject `string => void`)
- **Lawyer** (`src/solver/compat.rs`): TypeScript-specific rules (must implement void exception here)

## Implementation Plan (To be validated with Gemini)

### Step 1: Investigation
Ask Gemini:
```bash
./scripts/ask-gemini.mjs --include=src/solver "I need to implement void return compatibility.
Context: When target function returns void, any source return type should be acceptable.
Where should I implement this? In compat.rs or subtype.rs?
Show me the exact function and line number."
```

### Step 2: Find the Code
Look for:
- `src/solver/compat.rs` - Function compatibility checking
- `src/solver/subtype_rules/functions.rs` - Return type checking
- Find where return type assignability is checked

### Step 3: Implement the Void Exception
Add logic like:
```rust
// When checking function return types
if target_return == TypeId::VOID {
    return true;  // Void exception - any return is OK
}
// Otherwise check normally
return self.is_subtype_of(source_return, target_return);
```

### Step 4: Test
- Find tests for `forEach`, `map`, etc.
- Look for `assignmentToVoid.ts`
- Run conformance tests

## Files to Investigate

- `src/solver/compat.rs` - Main Lawyer layer
- `src/solver/subtype_rules/functions.rs` - Function subtype rules
- `src/checker/tests/` - Look for void-related tests

## Expected Impact

This is the **third pillar** of the Lawyer layer (after Bivariance ✅ and Freshness ✅). Fixing this will:
- Resolve false positives in `forEach`, `map`, and other callback patterns
- Fix conformance tests involving ignored return values
- Unblock common JavaScript/TypeScript patterns

## Next Step

Ask Gemini the mandatory Question 1 to validate the approach before implementing.
