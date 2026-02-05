# Session TSZ-11: Control Flow Analysis (CFA) Integration

**Started**: 2026-02-05
**Status**: 🔄 READY TO START
**Focus**: Integrate FlowAnalyzer into main Checker loop for flow-sensitive type narrowing

## Problem Statement

**The "Blind Spot"**: While the **Solver** now knows *how* to narrow types correctly (all TSZ-10 bugs fixed), the **Checker** is failing to ask the Solver for narrowed types at the right time.

### Root Cause

`get_type_of_symbol` in `src/checker/state_type_analysis.rs` uses a **flow-insensitive cache**. When the checker encounters an identifier like `action` in `action.type`, it looks up the symbol's **declared type** rather than querying the `FlowAnalyzer` for the **narrowed type** at that specific flow node.

### Impact

All the hard work done in `src/solver/narrowing.rs` remains "dark matter"—the logic exists, but the compiler doesn't use it to suppress errors in user code.

**Example**:
```typescript
function handle(action: Action) {
    if (action.type === "add") {
        // action should be narrowed to { type: "add", value: number }
        // But Checker might still think it's the full Action union
        const value: number = action.value; // Could fail with wrong type
    }
}
```

## Solution Architecture

### Goal

Integrate `FlowAnalyzer` into the main expression checking path so that identifiers return narrowed types based on control flow.

### Files to Modify

1. **`src/checker/expr.rs`**
   - `check_identifier()` function - Query FlowAnalyzer for narrowed types
   - Expression checking loop - Update flow context after each sub-expression

2. **`src/checker/state_type_analysis.rs`**
   - `get_type_of_symbol()` - Add flow-sensitive variant
   - Implement cache keyed by `(SymbolId, FlowNodeId)` instead of just `SymbolId`

3. **`src/checker/control_flow.rs`**
   - Ensure `FlowAnalyzer::get_flow_type_of_node` is exposed and performant
   - Handle cache invalidation correctly

### Implementation Approach (Per Gemini Pro)

#### Phase 1: Flow-Sensitive Symbol Resolution
1. Modify `check_identifier` in `src/checker/expr.rs` to query `FlowAnalyzer`
2. Implement flow-sensitive cache for symbol types
3. Ensure logical sub-expressions (`&&`, `||`) update the flow context

#### Phase 2: Performance Optimization
1. Profile the performance impact of flow-sensitive lookups
2. Implement efficient cache keyed by `(SymbolId, FlowNodeId)`
3. Consider lazy evaluation for expensive narrowing operations

### Key Challenges

1. **Performance**: Flow-sensitive lookups are more expensive than flow-insensitive
2. **Cache Invalidation**: Must ensure cache is correctly invalidated or keyed by flow node
3. **Compound Conditions**: `if (typeof x === "string" && x.length > 0)` - Right side must see narrowing from left side

## Test Cases to Verify

### Instanceof in Methods
```typescript
class Animal { }
class Dog extends Animal {
    bark() { console.log("woof"); }
}

function pet(x: Animal) {
    if (x instanceof Dog) {
        x.bark(); // Should work - x is narrowed to Dog
    }
}
```

### Compound Conditions
```typescript
function process(x: unknown) {
    if (typeof x === "string" && x.length > 0) {
        // Right side of && should see x narrowed to string
        console.log(x.toUpperCase()); // Should work
    }
}
```

### Discriminant Unions
```typescript
type Action =
    | { type: "add", value: number }
    | { type: "remove", id: string };

function handle(action: Action) {
    if (action.type === "add") {
        const v: number = action.value; // Should work
    }
}
```

## Known Issues from TSZ-10

From `docs/sessions/history/tsz-10.md`:

1. **Exhaustiveness (TS2366)**: Reverted due to complexity
   - Requires checking if `undefined` is assignable to return type at end of function
   - Needs switch statement exhaustiveness detection
   - This is a "North Star" requirement for matching `tsc` diagnostics

2. **Flow State Propagation**: Not yet implemented for:
   - Mid-expression narrowing (compound conditions)
   - Loop body narrowing
   - Callback parameter narrowing

## Dependencies

- ✅ Session TSZ-10: Discriminant narrowing robustness (COMPLETE)
- ✅ Session TSZ-2: Circular type parameter inference (COMPLETE)
- ✅ Session TSZ-3: Control flow narrowing infrastructure (COMPLETE)
- ✅ Session TSZ-5: Multi-pass generic inference (COMPLETE)

## Why This Is Priority

Per Gemini Pro (2026-02-05):
> "Without this integration, all the hard work done in `src/solver/narrowing.rs` remains 'dark matter'—the logic exists, but the compiler doesn't use it to suppress errors in the user's code."

**High Impact**:
- Core TypeScript feature users expect
- Enables all discriminant narrowing fixes to actually work
- Required for instanceof narrowing to be useful
- Foundation for exhaustiveness checking

## Mandatory Gemini Workflow

### Question 1: Approach Validation (REQUIRED BEFORE IMPLEMENTATION)

```bash
./scripts/ask-gemini.mjs --pro --include=src/checker --include=src/solver \
  "I need to integrate FlowAnalyzer into the main Checker loop so that identifiers return narrowed types.

Problem: get_type_of_symbol is flow-insensitive.
Plan:
1) Modify check_identifier in src/checker/expr.rs to query FlowAnalyzer.
2) Implement a flow-sensitive cache for symbol types.
3) Ensure logical sub-expressions (&&, ||) update the flow context.

Is this the right approach? Which specific functions in state_type_analysis.rs should I modify?
Are there performance pitfalls with flow-sensitive caching?"
```

### Question 2: Implementation Review (REQUIRED AFTER IMPLEMENTATION)

```bash
./scripts/ask-gemini.mjs --pro --include=src/checker/expr.rs --include=src/checker/state_type_analysis.rs \
  "I implemented FlowAnalyzer integration in the Checker.

Changes: [PASTE CODE OR DIFF]

Please review: 1) Is this correct for TypeScript? 2) Did I handle compound conditions correctly?
3) Are there performance issues? Be specific if it's wrong."
```

## Next Steps

1. ✅ Read this session file thoroughly
2. ⏳ **Ask Gemini Question 1** for approach validation (MANDATORY)
3. ⏳ Implement based on Gemini's guidance
4. ⏳ **Ask Gemini Question 2** for implementation review (MANDATORY)
5. ⏳ Fix any issues Gemini identifies
6. ⏳ Test with provided test cases
7. ⏳ Commit and push

## Success Criteria

- [ ] Instanceof narrowing works in method calls
- [ ] Compound conditions (`&&`, `||`) propagate narrowing correctly
- [ ] Discriminant unions narrow correctly in if statements
- [ ] Performance impact is acceptable (no significant slowdown)
- [ ] All narrowing tests pass
- [ ] New test cases for CFA integration pass

## References

- TSZ-10 completion: `docs/sessions/history/tsz-10.md`
- FlowAnalyzer: `src/checker/control_flow.rs`
- Narrowing logic: `src/solver/narrowing.rs`
- Type checking: `src/checker/expr.rs`
- Symbol resolution: `src/checker/state_type_analysis.rs`
