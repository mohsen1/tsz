# Session tsz-2: Coinductive Subtyping (Recursive Types)

**Started**: 2026-02-05
**Status**: Active
**Goal**: Implement coinductive subtyping logic to handle recursive types without infinite loops

## Problem Statement

From NORTH_STAR.md Section 4.4:

> "TypeScript uses 'coinductive' subtyping for recursive types. This means we compute the Greatest Fixed Point (GFP) rather than Least Fixed Point (LFP). When comparing `type A = { self: A }` and `type B = { self: B }`, we assume they are subtypes and verify consistency."

Without coinductive subtyping, the compiler will crash or enter infinite loops when comparing recursive types.

**Impact**:
- Blocks complex recursive type definitions (linked lists, trees, Redux state)
- Causes stack overflow crashes
- Prevents proper type checking of self-referential generics

## Technical Details

**Files**:
- `src/solver/subtype.rs` - Core subtype checking logic
- `src/solver/mod.rs` - Solver state management
- `src/solver/visitor.rs` - Traversal of recursive structures

**Root Cause**:
When comparing `A` and `B` where both contain references to themselves, the naive approach leads to infinite recursion: `is_subtype_of(A, B)` → check properties → `is_subtype_of(A, B)` → ...

## Implementation Strategy

### Phase 1: Investigation (Pre-Implementation)
1. Read `docs/architecture/NORTH_STAR.md` Section 4.4 on Coinductive Subtyping
2. Ask Gemini: "What's the correct approach for cycle detection in subtype checking?"
3. Review existing `cycle_stack` or similar mechanisms in `subtype.rs`

### Phase 2: Implementation
1. Implement cycle tracking using `HashSet<(TypeId, TypeId)>` or similar
2. When entering a subtype check, add the pair to the set
3. If the pair is already in the set, return `true` (assume subtypes)
4. Remove the pair when exiting the check
5. Add depth limiting to prevent "type-system bombs"

### Phase 3: Validation
1. Write unit tests for recursive types
2. Test with complex recursive structures
3. Ask Gemini Pro to review implementation

## Success Criteria

- [ ] No stack overflows when comparing recursive types
- [ ] `type A = { self: A }` and `type B = { self: B }` are correctly identified as subtypes
- [ ] Depth limiting prevents infinite loops
- [ ] Unit tests cover simple and mutually recursive types
- [ ] Generic recursive types work (e.g., `List<number>` vs `List<string>`)

## Session History

*Created 2026-02-05 after completing Application type expansion.*
