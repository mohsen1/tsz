# Session tsz-12: Bidirectional Narrowing & Advanced CFA Features

**Started**: 2026-02-05
**Status**: 🟡 READY TO START
**Priority**: HIGH (follow-up to completed tsz-10)

## Goal

Implement advanced CFA features to achieve 100% parity with TypeScript's control flow analysis:
1. **Bidirectional narrowing** for equality checks with references
2. **Assertion functions** integration with flow analysis
3. **Deeply nested discriminants** (e.g., `action.payload.kind`)
4. Edge case fixes (zombie freshness, `any` narrowing)

## Context

Session tsz-10 completed the core CFA & narrowing infrastructure:
- ✅ Type guards (typeof, instanceof, discriminants, truthiness)
- ✅ Property access & assignment narrowing
- ✅ Exhaustiveness checking (fixed discriminant comparison bug)

However, Gemini's analysis revealed several missing features that are critical for real-world TypeScript code.

---

## Phase 1: Bidirectional Narrowing (HIGH PRIORITY)

**Problem**: Current narrowing handles `x === "literal"` but not `x === y` where both are references.

**TypeScript Behavior**:
```typescript
function foo(x: string | number, y: string) {
    if (x === y) {
        // x is narrowed to string (from y's type)
        // y is narrowed to string | number (from x's type)
    }
}
```

**Implementation Location**:
- File: `src/checker/control_flow_narrowing.rs`
- Function: `narrow_by_binary_expr` (line ~2270)

**Current Code** (line ~2362):
```rust
// Bidirectional narrowing: x === y where both are references
if is_strict {
    // Check if target is on the left side
    if self.is_matching_reference(bin.left, target) {
        if let Some(node_types) = self.node_types {
            if let Some(&right_type) = node_types.get(&bin.right.0) {
                // For === (effective_truth = true): always narrow
                // For !== (effective_truth = false): only narrow if right_type is a unit type
                if effective_truth {
                    return narrowing.narrow_to_type(type_id, right_type);
                }
            }
        }
    }
    // ... symmetric check for right side
}
```

**Task**: Verify this code works correctly. If not, fix it.

**Test Cases**:
```typescript
// Test 1: Basic bidirectional narrowing
function test1(x: string | number, y: string) {
    if (x === y) {
        x.toLowerCase(); // x should be string
    }
}

// Test 2: Both sides are unions
function test2(x: string | number, y: string | boolean) {
    if (x === y) {
        x.toLowerCase(); // x should be string (intersection of types)
        y.toFixed();    // y should be never (no overlap)
    }
}

// Test 3: Negation
function test3(x: string | number, y: string) {
    if (x !== y) {
        // x should be... complex (x minus y if y is subset of x)
    }
}
```

---

## Phase 2: Assertion Functions Integration (HIGH PRIORITY)

**Problem**: Assertion functions (`asserts x is T`) currently only work in conditionals, but should narrow the entire subsequent flow.

**TypeScript Behavior**:
```typescript
function assertIsString(x: unknown): asserts x is string {
    if (typeof x !== "string") throw new Error();
}

function foo(x: unknown) {
    assertIsString(x);
    x.toLowerCase(); // x is string for ALL subsequent code
}
```

**Implementation Location**:
- File: `src/checker/control_flow_narrowing.rs`
- Function: `handle_call_iterative`

**Current Behavior**: Only handles assertions in condition contexts.

**Required Behavior**: Detect assertion calls and update `current_flow` for all subsequent statements in the block.

**Task**: Modify `handle_call_iterative` to:
1. Detect if call is an assertion function
2. Extract the type guard from the assertion
3. Apply the narrowing to the flow node that dominates all subsequent statements
4. Update the flow graph to reflect the narrowed type

---

## Phase 3: Deeply Nested Discriminants (MEDIUM PRIORITY)

**Problem**: Current discriminant narrowing only handles immediate property access. Real-world code (Redux, Flux) uses nested discriminants.

**TypeScript Behavior**:
```typescript
type Action =
    | { type: 'UPDATE', payload: { kind: 'user', data: User } }
    | { type: 'UPDATE', payload: { kind: 'product', data: Product } };

function reducer(action: Action) {
    switch (action.payload.kind) {
        case 'user':
            // action.payload should be { kind: 'user', data: User }
            return action.payload.data.name;
        case 'product':
            return action.payload.data.price;
    }
}
```

**Implementation Location**:
- File: `src/checker/control_flow_narrowing.rs`
- Function: `discriminant_property_info`

**Current Limitation**: Only returns immediate parent property.

**Required Enhancement**: Recursively walk `PropertyAccessExpression` to build `property_path: Vec<Atom>` (e.g., `["payload", "kind"]`).

**Task**:
1. Modify `discriminant_property_info` to build full property path
2. Update `narrow_by_discriminant` to handle paths of any length
3. Test with nested patterns up to 3-4 levels deep

---

## Phase 4: Edge Case Fixes (MEDIUM PRIORITY)

### 4.1: Zombie Freshness
**Issue**: Fresh object literals might lose freshness after narrowing.

**Investigation**: Check `narrow_by_discriminant` in `src/solver/narrowing.rs`. Ensure narrowed types preserve `ObjectFlags::FRESH_LITERAL`.

### 4.2: Truthiness of 0 and ""
**Issue**: `if (x)` where `x: string | number` should narrow to `string (excluding "") | number (excluding 0)`.

**Investigation**: Verify `narrow_by_truthiness` handles `0` and `""` correctly.

### 4.3: Narrowing `any`
**Issue**: `typeof x === "string"` where `x: any` should narrow to `string` within the block.

**Investigation**: Check `narrow_by_typeof` in `src/solver/narrowing.rs`. Currently returns `ANY` immediately (line ~402). Should narrow to the specific type.

---

## Complexity Assessment

**Overall Complexity**: **HIGH**

**Risk Areas**:
- Bidirectional narrowing requires careful intersection logic
- Assertion functions need flow graph updates (could affect other analyses)
- Nested discriminants require recursive algorithms
- Edge cases are subtle and easily broken

**Mitigation**:
- Follow Two-Question Rule strictly for ALL changes
- Test with real TypeScript codebases
- Incremental implementation with thorough testing

---

## Coordination Notes

**tsz-10**: Complete (basic CFA & narrowing)
**tsz-11**: Truthiness & Equality Narrowing (may overlap with Phase 1)

Check other sessions to avoid duplicate work.

---

## Gemini Consultation Plan

### Phase 1: Bidirectional Narrowing
**Question 1** (Pre-implementation):
```bash
./scripts/ask-gemini.mjs --include=src/checker/control_flow_narrowing.rs "
I need to implement bidirectional narrowing for x === y where both are references.

Current code in narrow_by_binary_expr (line ~2362) has some bidirectional logic.
Question: Is this logic correct? If not, what's the right approach?

Test case: function foo(x: string | number, y: string) {
    if (x === y) { x.toLowerCase(); } // x should narrow to string
}"
```

**Question 2** (Post-implementation): Code review with Pro model

### Phase 2: Assertion Functions
**Question 1**:
```bash
./scripts/ask-gemini.mjs --include=src/checker/control_flow.rs "
I need to integrate assertion functions with flow analysis.

assertIsString(x) should narrow x for ALL subsequent code, not just in conditionals.
How do I update the flow graph to reflect this?

Current: handle_call_iterative only handles assertions in condition contexts.
Required: Detect assertions and apply narrowing to subsequent flow nodes."
```

### Phase 3: Nested Discriminants
**Question 1**:
```bash
./scripts/ask-gemini.mjs --include=src/checker/control_flow_narrowing.rs "
I need to support nested discriminants like action.payload.kind.

Current discriminant_property_info only returns immediate parent.
Question: How do I recursively walk PropertyAccessExpression to build property_path?
What's the algorithm for handling paths of arbitrary length?"
```

---

## Success Criteria

- [ ] Bidirectional narrowing works for reference equality
- [ ] Assertion functions narrow subsequent code
- [ ] Nested discriminants (2-3 levels) work correctly
- [ ] Edge cases handled (freshness, 0/"", any)
- [ ] All changes validated by Gemini (pre and post)
- [ ] Conformance tests pass
