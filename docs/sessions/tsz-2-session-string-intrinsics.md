# Session tsz-2: String Mapping Intrinsics

**Date:** 2026-02-06  
**Focus:** String Intrinsic Types (Uppercase, Lowercase, Capitalize, Uncapitalize)

## Overview

Working on TypeScript string manipulation type transformations:
- `Uppercase<T>` - Convert string literal type to uppercase
- `Lowercase<T>` - Convert string literal type to lowercase
- `Capitalize<T>` - Capitalize first character
- `Uncapitalize<T>` - Uncapitalize first character

## Why This Task?

Per Gemini recommendation:
- **Independent**: Non-recursive "leaf" operations
- **No cycle detection needed**: Avoids current architectural blockers
- **Incremental**: Can implement one-by-one
- **Architecture-aligned**: Uses Solver for WHAT, TypeInterner for deduplication

## Implementation Plan

1. **Locate evaluation logic**: `src/solver/evaluate.rs` - `evaluate_string_intrinsic`
2. **Handle type_arg**:
   - If `Literal(String)`: Perform Rust string transformation
   - If `Union`: Distribute operation
   - If `TemplateLiteral`: Evaluate parts
3. **Use Interner**: Transform string back to TypeId

## Files to Touch

- `src/solver/types.rs` - StringIntrinsicKind enum
- `src/solver/evaluate.rs` - Core logic
- `src/solver/visitor.rs` - Traversal

## Previous Context

Task #22 (interface readonly) and Task #18 (index signatures) are blocked
on cycle-aware type resolution. Moving to string intrinsics as independent work.
