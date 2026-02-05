# Session TSZ-10: Advanced Control Flow Analysis (CFA) & Narrowing

**Started**: 2026-02-05
**Status**: 🔄 ACTIVE
**Focus**: Implement robust control flow analysis and type narrowing

## Session Scope

### Problem Statement

TypeScript's type system becomes significantly more powerful with control flow analysis. The compiler can narrow types based on:
- Conditionals (`if`, `else if`, `switch`)
- Type guards (`typeof`, `instanceof`, user-defined)
- Truthiness checks
- Assertion functions
- Discriminant unions

While basic discriminant narrowing exists in tsz, a robust CFA implementation is needed to match `tsc` behavior in complex branching scenarios.

### Why This Matters

Without proper CFA and narrowing, tsz cannot correctly infer types in:
- Functions with conditional logic
- Type guards and user-defined predicates
- Assertion functions that affect flow
- Exhaustiveness checking for switch/if-else chains
- Unreachable code detection

This is a high-priority gap that affects almost every non-trivial TypeScript program.

## Architecture

Per `NORTH_STAR.md` and the existing codebase:

- **Binder**: Builds the FlowNode graph (CFG)
- **Checker**: Traverses AST, applies narrowing at each point
- **Solver**: Performs type intersections/subtractions via `narrow(type_id, narrower_id)`
- **Lawyer**: Consulted for special cases (any, unknown, private brands)

## Tasks (Priority Order)

### Task 1: Truthiness & Type-of Narrowing

**Status**: Pending

**Goal**: Implement narrowing for `typeof` and truthiness checks

**Test Cases**:
```typescript
// typeof narrowing
function foo(x: string | number) {
  if (typeof x === "string") {
    x; // should be string
  } else {
    x; // should be number
  }
}

// truthiness narrowing
function bar(x: string | null | undefined) {
  if (x) {
    x; // should be string (null | undefined removed)
  }
}
```

**Files to modify**:
- `src/solver/narrowing.rs` - narrowing logic
- `src/checker/flow_analysis.rs` - flow graph traversal

**Deliverables**:
- [ ] Implement `typeof` narrowing for string/number/boolean/object/etc.
- [ ] Implement truthiness narrowing (removes null/undefined/falsy)
- [ ] Add integration tests

### Task 2: Equality & Instanceof Narrowing

**Status**: Pending

**Goal**: Implement narrowing based on equality checks and instanceof

**Test Cases**:
```typescript
// equality narrowing
function foo(x: "a" | "b" | "c") {
  if (x === "a") {
    x; // should be "a"
  }
}

// instanceof narrowing
class A {}
class B {}
function bar(x: A | B) {
  if (x instanceof A) {
    x; // should be A
  }
}
```

**Files to modify**:
- `src/solver/narrowing.rs` - literal and instanceof narrowing
- `src/checker/expr.rs` - equality expression handling

**Deliverables**:
- [ ] Implement literal equality narrowing
- [ ] Implement instanceof narrowing
- [ ] Add integration tests

### Task 3: User-Defined Type Guards

**Status**: Pending

**Goal**: Implement support for `arg is T` type guards

**Test Cases**:
```typescript
function isString(x: unknown): x is string {
  return typeof x === "string";
}

function foo(x: string | number) {
  if (isString(x)) {
    x; // should be string
  }
}
```

**Files to modify**:
- `src/solver/lower.rs` - extract type predicates
- `src/solver/types.rs` - TypePredicate representation
- `src/checker/expr.rs` - apply type guard narrowing

**Deliverables**:
- [ ] Parse `is` type guards from function signatures
- [ ] Apply narrowing when type guard returns true
- [ ] Add integration tests

### Task 4: Assertion Functions

**Status**: Pending

**Goal**: Implement support for `asserts arg is T` assertion functions

**Test Cases**:
```typescript
function assertIsString(x: unknown): asserts x is string {
  if (typeof x !== "string") throw new Error();
}

function foo(x: unknown) {
  assertIsString(x);
  x; // should be string after assertion
}
```

**Files to modify**:
- `src/solver/narrowing.rs` - assertion function handling
- `src/solver/flow_analysis.rs` - flow graph updates after assertions

**Deliverables**:
- [ ] Detect `asserts` keyword in function signatures
- [ ] Narrow type after assertion function call
- [ ] Update flow graph to reflect assertion
- [ ] Add integration tests

### Task 5: Discriminant Union Refinement

**Status**: Pending (already partially implemented, needs verification)

**Goal**: Ensure discriminant property narrowing works correctly

**Test Cases**:
```typescript
type Shape = { kind: "circle", radius: number }
            | { kind: "square", side: number };

function area(shape: Shape) {
  if (shape.kind === "circle") {
    shape.radius; // should work
  }
}
```

**Files to verify**:
- `src/solver/narrowing.rs` - discriminant narrowing logic
- Known bugs from AGENTS.md: optional properties, Lazy types, Intersection types

**Deliverables**:
- [ ] Fix discriminant narrowing with optional properties
- [ ] Fix discriminant narrowing with Lazy types
- [ ] Fix discriminant narrowing with Intersection types
- [ ] Add comprehensive integration tests

### Task 6: Exhaustiveness Checking

**Status**: Pending

**Goal**: Implement exhaustiveness checking for if/else and switch statements

**Test Cases**:
```typescript
type Result = "success" | "error";

function handle(result: Result) {
  if (result === "success") {
    // ...
  } else {
    // result should be narrowed to "error" here
  }
}
```

**Files to modify**:
- `src/checker/statements.rs` - if/else statement checking
- `src/checker/expr.rs` - switch statement checking

**Deliverables**:
- [ ] Implement exhaustiveness checking for if/else chains
- [ ] Implement exhaustiveness checking for switch statements
- [ ] Report TS2366 when not exhaustive
- [ ] Add integration tests

### Task 7: Unreachable Code Detection

**Status**: Pending

**Goal**: Detect unreachable code when type narrows to never

**Test Cases**:
```typescript
function neverReturns(): never {
  throw new Error();
}

function foo() {
  neverReturns();
  console.log("unreachable"); // TS2307
}
```

**Files to modify**:
- `src/checker/flow_analysis.rs` - detect never types in flow
- `src/checker/statements.rs` - report TS2307

**Deliverables**:
- [ ] Detect unreachable code after never-returning expressions
- [ ] Report TS2307 with correct position
- [ ] Add integration tests

## Implementation Notes

### Mandatory Gemini Workflow

Per AGENTS.md, **must** ask Gemini TWO questions before implementing:

#### Question 1: Approach Validation (PRE-implementation)
```bash
./scripts/ask-gemini.mjs --include=src/solver --include=src/checker "I need to implement [FEATURE] for TSZ-10.
Here's my understanding: [PROBLEM DESCRIPTION].
Planned approach: [YOUR PLAN].

Questions:
1. Is this approach correct?
2. What files/functions should I modify?
3. What edge cases should I test?
4. How should I handle Lazy and Intersection types during narrowing?
5. Are there TypeScript behaviors I need to match?"
```

#### Question 2: Implementation Review (POST-implementation)
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver --include=src/checker "I implemented [FEATURE] for TSZ-10.
Changes: [PASTE CODE OR DIFF].

Please review:
1. Is this correct for TypeScript?
2. Did I miss any edge cases?
3. Are there type system bugs?
4. Does the flow graph update correctly?

Be brutal - tell me specifically what to fix."
```

### Known Issues from AGENTS.md

The discriminant narrowing implementation had **3 CRITICAL BUGS** that must be fixed:

1. **Reversed subtype check** - asked `is_subtype_of(property_type, literal)` instead of `is_subtype_of(literal, property_type)`
2. **Missing type resolution** - didn't handle `Lazy`/`Ref`/`Intersection` types
3. **Broken for optional properties** - failed on `{ prop?: "a" }` cases

These MUST be addressed in Task 5 (Discriminant Union Refinement).

## Related Sessions

- **TSZ-1**: Judge Layer (Core Type Relations) - Foundation
- **TSZ-4**: Lawyer Layer (Compatibility Rules) - Complete
- **TSZ-6**: Literal Type Widening - Complete

## Success Criteria

- [ ] Task 1: Truthiness & typeof narrowing implemented with tests
- [ ] Task 2: Equality & instanceof narrowing implemented with tests
- [ ] Task 3: User-defined type guards implemented with tests
- [ ] Task 4: Assertion functions implemented with tests
- [ ] Task 5: Discriminant union refinement fixed and tested
- [ ] Task 6: Exhaustiveness checking implemented with tests
- [ ] Task 7: Unreachable code detection implemented with tests
- [ ] All narrowing features handle Lazy/Intersection types correctly
- [ ] Conformance tests pass for CFA scenarios

## Work Log

### 2026-02-05: Session Initialized

**Context**: TSZ-4 complete (Lawyer Layer verification). Following AGENTS.md hook guidance, asked Gemini to recommend next session.

**Gemini Recommendation**: TSZ-10: Advanced Control Flow Analysis (CFA) & Narrowing because:
1. High priority for TypeScript compatibility
2. High architectural impact (Binder + Checker + Solver coordination)
3. Gaps exist in current discriminant narrowing (3 critical bugs)
4. Essential for almost every non-trivial TypeScript program

**Strategy**:
1. Start with Task 1 (Truthiness & typeof) - foundational narrowing
2. Progress through Tasks 2-4 (equality, type guards, assertions)
3. Task 5 fixes known discriminant narrowing bugs
4. Tasks 6-7 add exhaustiveness and unreachable code detection

**Next Task**: Task 1 - Truthiness & Type-of Narrowing

**Commit**: Session file created

### 2026-02-05: Task 1 Approach Validation (Mandatory Gemini Consultation)

**Context**: Following AGENTS.md mandatory workflow, asked Gemini for approach validation before implementing Task 1.

**Gemini Guidance Received**:

**Architecture Validation**:
- Architectural split is CORRECT
- Use existing `src/checker/control_flow_narrowing.rs` abstraction
- Modify `extract_type_guard` to recognize typeof/truthiness patterns
- Delegate to `solver.narrow_type(source_type, &guard, sense)`

**Critical Finding - Must Fix Task 5 Bugs First**:
- YES, address "Missing type resolution" bug immediately as part of Task 1
- The bugs affect ALL narrowing, not just discriminants
- Current narrowing engine is "blind" to Lazy and Intersection types
- Example: `type StringOrNumber = string | number; if (typeof x === "string")` fails without fix

**Handling Lazy/Intersection/Union Types**:
- **Lazy**: Use `resolve_type` helper which calls `db.evaluate_type(type_id)`
- **Intersection**: Narrow both A and B, return new intersection (NEVER if either is NEVER)
- **Union**: Filter members by recursively calling narrowing logic

**Specific Functions to Modify**:
- `src/checker/control_flow_narrowing.rs` - `extract_type_guard`
- `src/checker/flow_analysis.rs` - `narrow_type_by_condition_inner`
- `src/solver/narrowing.rs` - `narrow_type`, `narrow_by_typeof`, `narrow_by_truthiness`, `narrow_to_falsy`

**TypeScript Behaviors to Match**:

**typeof Narrowing:**
- `any` and `unknown` MUST be narrowed (unlike other types)
- `typeof null` is `"object"` - must handle null correctly
- `function` narrows to callable types
- Exclusion: `typeof x !== "string"` removes string and string literals

**Truthiness Narrowing:**
- Falsy values: null, undefined, false, 0, -0, 0n, "", NaN
- True branch: Removes null/undefined/void, narrows boolean to true, 0|1 to 1
- False branch: Narrows to falsy component (e.g., `"" | 0`)
- NaN: No `LiteralValue::NaN`, so number narrowing results in number

**Potential Pitfalls:**
1. Infinite recursion when resolving Lazy/traversing Intersections (use visited set)
2. Over-narrowing type parameters (use `db.intersection2` for type params)
3. Naked type parameters - handle `never` correctly in union filtering

**Next Steps**:
1. Fix Lazy/Intersection resolution in narrowing.rs FIRST
2. Implement `extract_type_guard` for typeof/truthiness
3. Implement `narrow_by_typeof` and `narrow_by_truthiness`
4. Ask Gemini for implementation review (Question 2) before committing

**Status**: Ready to implement with clear architectural guidance
