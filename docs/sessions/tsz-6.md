# Session TSZ-6: Literal Type Widening & Const Assertions

**Started**: 2026-02-05
**Status**: 🔄 Active
**Focus**: Implement TypeScript's type widening rules for literal types in the Inference Engine

## Session Scope

### Problem Statement
TypeScript widens literal types to primitive types in certain contexts for usability:
- `let x = "a"` infers `x` as `string`, not `"a"`
- `const x = "a"` infers `x` as `"a"` (no widening)
- Object literal properties are widened even when assigned to `const`
- `as const` suppresses all widening and adds `readonly`

Currently, TSZ likely infers all literals as their literal type, causing false positives when values are reassigned.

### Why This Matters
Without proper widening:
- `let x = "a"; x = "b";` incorrectly fails (TS2322)
- Object literals infer overly narrow types
- `as const` doesn't work as expected
- Hundreds of conformance test failures

### Tasks (Priority Order)

#### Priority 1: Implement `widen_type` in the Solver
**File**: `src/solver/operations.rs` (or new `src/solver/widening.rs`)

**Goal**: Create a visitor-based `widen_type(TypeId) -> TypeId` function

**Logic**:
```rust
pub fn widen_type(type_id: TypeId) -> TypeId {
    match type_id {
        // String/Number/Boolean literals → Intrinsic primitives
        TypeId::Literal(Literal::String(_)) => TypeId::STRING,
        TypeId::Literal(Literal::Number(_)) => TypeId::NUMBER,
        TypeId::Literal(Literal::Boolean(_)) => TypeId::BOOLEAN,

        // Unions: widen all members
        TypeId::Union(members) => {
            TypeId::Union(members.iter().map(widen_type).collect())
        }

        // Objects: widen property types (unless readonly)
        TypeId::Object(shape) => {
            // Widen each property type
        }

        // Other types: no change
        _ => type_id
    }
}
```

**Edge Cases**:
- `null` and `undefined` behavior under `strictNullChecks`
- Nested object literals (recursive widening)
- Generic type parameters
- Intersection types

#### Priority 2: Integrate Widening into Variable Declarations
**File**: `src/checker/declarations.rs`

**Goal**: Apply widening in "widening contexts" (let/var), not in "non-widening" contexts (const)

**Logic**:
```rust
// For let/var declarations (widening context)
let inferred = self.get_type_of_node(initializer);
let widened = self.type_env.widen_type(inferred);

// For const declarations (non-widening context)
let inferred = self.get_type_of_node(initializer);
// Don't widen
```

**Test Cases**:
```typescript
// Should PASS - let widens literal to primitive
let x = 1;
x = 2;

// Should FAIL - const preserves literal
const y = 1;
y = 2; // TS2588: Cannot assign to '1' because it is a constant
```

#### Priority 3: Object Literal Property Widening
**File**: `src/checker/expr.rs` (object literal type inference)

**Goal**: Properties are widened even when object is assigned to const

**TSC Rule**: Properties are mutable by default, so they must widen

**Test Cases**:
```typescript
// Should infer { x: number } even though assigned to const
const obj = { x: 1 };
obj.x = 2; // Should be allowed
```

#### Priority 4: Const Assertions (`as const`)
**Files**:
- `src/parser/` - Parse `as const` syntax
- `src/solver/lower.rs` - Lower const assertion node
- `src/solver/operations.rs` - Suppress widening recursively

**Goal**: Implement `as const` to preserve literals and add readonly

**Logic**:
- Suppress widening for entire expression tree
- Recursively add `readonly` modifier to all properties
- Recursively add `readonly` to array elements

**Test Cases**:
```typescript
const arr = [1, 2, 3] as const;
// Type: readonly [1, 2, 3]

const obj = { x: 1, y: [2, 3] } as const;
// Type: { readonly x: 1, readonly y: readonly [2, 3] }
```

#### Priority 5: Literal Type Stripping (Freshness Removal)
**File**: `src/solver/freshness.rs`

**Goal**: When fresh object literal is assigned to widened variable, strip freshness

**Logic**:
- Fresh literals have excess property checking
- After assignment to widened type, freshness is removed
- Subsequent assignments don't trigger EPC

## Implementation Notes

### Architecture
Following NORTH_STAR.md principles:
- **Solver (WHAT)**: Implements `widen_type` operation
- **Checker (WHERE)**: Determines when to apply widening
- Clear separation of concerns

### Mandatory Gemini Workflow
Per AGENTS.md, **must** ask Gemini before implementing:

**Question 1 (Pre-implementation)**:
```bash
./scripts/ask-gemini.mjs --include=src/solver --include=src/checker "
I am implementing Literal Type Widening for TSZ-6.

Plan:
1. Create widen_type visitor in src/solver/operations.rs
2. Call from src/checker/declarations.rs for let/var bindings
3. Handle readonly properties in objects (don't widen)

Questions:
1. How should I handle widening vs non-widening contexts recursively?
2. Should Solver handle context or Checker pass a flag?
3. What about generic type parameters - widen or preserve?
4. Edge cases with intersections and unions?
"
```

**Question 2 (Post-implementation)**:
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver --include=src/checker "
I implemented Literal Type Widening in [FILES].

Changes: [PASTE CODE OR DIFF]

Please review:
1. Is this correct for TypeScript?
2. Did I miss any edge cases?
3. Are there type system bugs?

Be brutal - if wrong, tell me exactly what to fix.
"
```

## Related Sessions
- **TSZ-4**: Nominality & Accessibility (Lawyer Layer) - ✅ COMPLETE
- **TSZ-5**: Multi-Pass Generic Inference (ACTIVE)
- **TSZ-6**: Literal Type Widening & Const Assertions (THIS SESSION)

## Success Criteria
- [ ] Priority 1: widen_type function implemented
- [ ] Priority 2: let/var use widening, const doesn't
- [ ] Priority 3: Object properties widened correctly
- [ ] Priority 4: as const syntax implemented
- [ ] Priority 5: Freshness stripped after assignment
- [ ] All tests passing
- [ ] Conformance improvement (fewer TS2322 errors)
