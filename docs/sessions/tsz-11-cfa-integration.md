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

### Implementation Approach (Per Gemini Pro - APPROVED)

### Step 1: Intercept Identifiers in compute_type_of_node_complex
**File**: `src/checker/state.rs` (lines 661-666)
**Function**: `compute_type_of_node_complex`

```rust
fn compute_type_of_node_complex(&mut self, idx: NodeIndex) -> TypeId {
    // 1. Intercept Identifiers BEFORE creating ExpressionDispatcher
    if let Some(node) = self.ctx.arena.get(idx) {
        if node.kind == crate::scanner::SyntaxKind::Identifier as u16 {
            return self.get_type_of_identifier_with_flow(idx);
        }
    }

    // 2. Fallback for other complex nodes
    use crate::checker::dispatch::ExpressionDispatcher;
    let mut dispatcher = ExpressionDispatcher::new(self);
    dispatcher.dispatch_type_computation(idx)
}
```

### Step 2: Implement get_type_of_identifier_with_flow
**File**: `src/checker/state.rs` (new function)
**Location**: Add near `get_type_of_identifier` or as method on `CheckerState`

```rust
fn get_type_of_identifier_with_flow(&mut self, idx: NodeIndex) -> TypeId {
    // 1. Resolve the symbol
    let sym_id = match self.get_symbol_at_node(idx) {
        Some(sym) => sym,
        None => return TypeId::UNKNOWN,
    };

    // 2. Get the declared type (flow-insensitive baseline)
    let initial_type = self.get_type_of_symbol(sym_id);

    // 3. Check if flow analysis applies
    let flow_node = match self.ctx.check_flow_usage(idx) {
        Some(node) => node,
        None => return initial_type, // No flow info, use declared type
    };

    // 4. Instantiate FlowAnalyzer with PERSISTENT cache
    use crate::checker::control_flow::FlowAnalyzer;
    let analyzer = FlowAnalyzer::new(
        self.ctx.binder.flow_graph.clone(),
        &self.ctx.binder.flow_node_types,
        &self.ctx.binder.flow_assignments,
        &self.ctx.binder.node_flow,
        self.ctx,
    )
    .with_flow_cache(&self.ctx.flow_analysis_cache); // CRITICAL: Reuse cache!

    // 5. Query for narrowed type
    analyzer.get_flow_type(idx, initial_type, flow_node)
}
```

### Step 3: Verify Cache Persistence
**File**: `src/checker/context.rs`
**Check**: Ensure `flow_analysis_cache` exists and is accessible

```rust
// Should already exist in CheckerContext:
flow_analysis_cache: RefCell<FxHashMap<(FlowNodeId, SymbolId, TypeId), TypeId>>
```

**Critical**: Do NOT create a new cache for each identifier lookup. Reuse the existing cache!

### Performance Considerations

1. ✅ **Cache Persistence**: Use existing `flow_analysis_cache` in `CheckerContext`
2. ✅ **RefCell Borrowing**: `FlowAnalyzer` drops borrow before recursive calls (safe)
3. ✅ **Type Parameters**: `FlowAnalyzer` has `initial_has_type_params` check (don't bypass)
4. ⚠️ **Cache Invalidation**: Currently no explicit invalidation - relies on correct flow node keys

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

### ✅ Question 1: Approach Validation (COMPLETED 2026-02-05)

**Initial Plan (REJECTED - Had Architectural Flaw)**:
- Modify `check_identifier` in `src/checker/expr.rs`
- Problem: `ExpressionChecker` holds mutable ref to `CheckerContext`
- `CheckerState` owns `CheckerContext` and has `get_type_of_symbol`
- **Cannot call back into `CheckerState` from `ExpressionChecker` due to borrowing rules**

**Corrected Approach (APPROVED by Gemini Pro)**:
1. ✅ Handle `SyntaxKind::Identifier` in `CheckerState::compute_type_of_node_complex`
2. ✅ Use existing `flow_analysis_cache` in `CheckerContext` (no new cache needed!)
3. ✅ Instantiate `FlowAnalyzer` with persistent cache and query `get_flow_type`

**Key Findings from Gemini Pro**:
- **Location**: `compute_type_of_node_complex` at state.rs:661-666
- **DELEGATE Pattern**: `compute_type_of_node` (state.rs:638-653) checks for `DELEGATE`, falls back to `compute_type_of_node_complex`
- **Cache**: `flow_analysis_cache` already exists in `CheckerContext` as `RefCell<FxHashMap<(FlowNodeId, SymbolId, TypeId), TypeId>`
- **Circular Dependencies**: Minimal risk if `RefCell` borrow is dropped before recursive calls

**Implementation Signature**:
```rust
fn compute_type_of_node_complex(&mut self, idx: NodeIndex) -> TypeId {
    // 1. Intercept Identifiers here
    if let Some(node) = self.ctx.arena.get(idx) {
        if node.kind == crate::scanner::SyntaxKind::Identifier as u16 {
            return self.get_type_of_identifier_with_flow(idx);
        }
    }

    // 2. Fallback for other complex nodes
    use crate::checker::dispatch::ExpressionDispatcher;
    let mut dispatcher = ExpressionDispatcher::new(self);
    dispatcher.dispatch_type_computation(idx)
}
```

### ⏳ Question 2: Implementation Review (PENDING)

After implementing, ask:
```bash
./scripts/ask-gemini.mjs --pro --include=src/checker/state.rs --include=src/checker/control_flow.rs \
  "I integrated FlowAnalyzer into CheckerState.

Changes: [PASTE CODE OR DIFF]

Please review:
1. Did I handle the flow cache correctly (persistence vs invalidation)?
2. Is the fallback to initial_type correct if no flow node exists?
3. Does this correctly handle the DELEGATE pattern from ExpressionChecker?"
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
