# CFA Bugs - Implementation Status

## Completed ✅

### Bug #1.1: const variable detection (Fixed)
**File**: `src/checker/flow_analysis.rs`  
**Issue**: `is_mutable_binding()` checked wrong node for CONST flag  
**Fix**: Now correctly checks VariableDeclarationList parent node  
**Commit**: `8bea11319`

### Bug #1.2: Captured vs local variables in closures (Fixed)
**File**: `src/checker/flow_analysis.rs`  
**Issue**: ALL mutable variables in closures had narrowing reset  
**Fix**: Added `is_captured_variable()` to distinguish captured from local  
**Commit**: `98f7d6b7f`

### Bug #2.1: Assignment tracking in conditions (Fixed)
**File**: `src/checker/flow_graph_builder.rs`  
**Issue**: Assignments in if/while/for conditions not tracked  
**Fix**: Added `handle_expression_for_assignments()` to 5 control flow methods  
**Commit**: `a7ca92250`

### Bug #3.1: Unreachable code resurrection (Fixed)
**File**: `src/checker/flow_graph_builder.rs`  
**Issue**: Code after return/throw incorrectly marked reachable at merge points  
**Fix**: Added unreachable check before creating merge labels  
**Commit**: `a7ca92250`

### Bug #9: for-await-of async check (Fixed)
**File**: `src/checker/statements.rs`  
**Issue**: No check for await outside async function in for-await-of  
**Fix**: Added `check_await_expression()` call when await_modifier is true  
**Commit**: `a7ca92250`

## Remaining Bugs ⚠️

### Bug #4.1: Flow node antecedent traversal through closure START nodes
**Complexity**: High (architectural change required)  
**Files**: 
- `src/checker/flow_graph_builder.rs` (build_function_body)
- `src/checker/control_flow.rs` (check_flow)

**Issue**:
When analyzing control flow through closures, the flow analyzer must traverse through closure START node antecedents to properly track variable state across closure boundaries.

Currently, each function gets an isolated flow graph with a START node that has no antecedents:

```rust
// src/checker/flow_graph_builder.rs:197-199
self.graph = FlowGraph::new();
self.current_flow = self.graph.nodes.alloc(flow_flags::START);
// START node has no antecedents → can't traverse to outer scope
```

**Root Cause**:
- `build_function_body()` creates isolated FlowGraph per function
- START nodes have empty `antecedent` array
- Flow analysis stops at START, can't continue to outer scope

**Required Changes**:
1. **Store function node in START flow node**:
   ```rust
   pub fn build_function_body(&mut self, func_node: NodeIndex, body: &BlockData) -> &FlowGraph {
       // ... existing code ...
       let start_id = self.graph.nodes.alloc(flow_flags::START);
       if let Some(node) = self.graph.nodes.get_mut(start_id) {
           node.node = func_node; // Link to AST node
       }
   }
   ```

2. **Store reference to outer flow graph**:
   ```rust
   // In FlowGraphBuilder or FlowAnalyzer
   pub outer_flow_graph: Option<FlowGraph>,
   ```

3. **Handle START node in flow traversal** (in `check_flow`):
   ```rust
   // When encountering START node with no antecedents
   if flow.has_any_flags(flow_flags::START) && flow.antecedent.is_empty() {
       // Get function node from flow.node
       let func_node = flow.node;
       // Find flow node where function was declared in outer graph
       let outer_flow = self.get_outer_flow_node(func_node);
       // Continue traversal from outer scope
       return self.check_flow(reference, initial_type, outer_flow, visited);
   }
   ```

**Test**: `test_nested_closure_capture` (currently ignored)

---

### Bug #4.2: Loop label unions back edge types correctly
**Complexity**: Medium  
**File**: `src/checker/control_flow.rs` (LOOP_LABEL handling)

**Issue**:
When a loop has a label, the flow analyzer should union the types from all back edges (continue statements and loop end) to create the correct type for the loop body.

Current behavior: LOOP_LABEL union doesn't properly handle:
1. Back edges from continue statements
2. Type changes within loop body
3. Proper widening at loop entry

**Required Changes**:
1. Track all back edges in LOOP_LABEL
2. Union types from:
   - Loop entry point
   - All continue statement targets
   - Loop body completion
3. Apply widening at loop boundaries

**Test**: `test_loop_label_unions_back_edge_types` (currently ignored)

---

## Lower Priority Bugs

### Bug #5.1: const assertion narrowing
**Complexity**: Low  
**Description**: const assertions should narrow types in subsequent code

### Bug #6.1: readonly property checking
**Complexity**: Low  
**Description**: readonly properties should prevent reassignment

### Bug #7.1: delete operand type checking
**Complexity**: Low  
**Description**: Delete operator should verify operand is deletable

## Testing

Unit tests added in `src/checker/tests/control_flow_tests.rs`:
- 61 tests passing
- 9 tests ignored (awaiting Bug #4.1, #4.2 fixes)
- 0 tests failing

## Conformance Test Baseline

- **Baseline**: 7834 tests passing
- **Status**: Maintained through all CFA fixes
