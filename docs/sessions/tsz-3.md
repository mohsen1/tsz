# Session tsz-3 - Flow Integration for Discriminant Narrowing

**Started**: 2026-02-04
**Status**: ACTIVE
**Focus**: Investigate why discriminant narrowing isn't triggered during end-to-end type checking

## Context

Previous session completed the `narrow_by_discriminant` rewrite. All narrowing unit tests pass, but discriminant narrowing is NOT triggered during end-to-end type checking of source files.

**Problem**: The narrowing logic works correctly in isolation (unit tests pass), but when checking actual TypeScript files, `narrow_by_discriminant` is never called. The flow analysis infrastructure exists but doesn't trigger for if statements.

## Root Cause Investigation

Gemini's analysis suggests the issue is likely in **Checker Integration**:
- Flow graph builder correctly creates `TRUE_CONDITION` / `FALSE_CONDITION` nodes for if statements
- The Checker needs to query `flow_graph.get_flow_at_node(node)` when analyzing expressions
- Checker might be missing the call to `solver.narrow_type(type_id, flow_node)` or passing `FlowNodeId::NONE`

## Investigation Plan

1. **Verify Flow Graph Construction**:
   - Check that `record_node_flow` is called for identifiers in discriminant checks
   - Verify flow nodes are correctly associated with AST nodes

2. **Checker Integration Audit**:
   - Find where Checker queries flow nodes during type checking
   - Verify `apply_flow_narrowing` is called with correct flow nodes
   - Check if flow nodes are being passed correctly through the type checking pipeline

3. **Debug the Missing Link**:
   - Add targeted tracing to understand the flow from AST node → flow node → narrowing
   - Identify where the connection breaks

**Location**: `src/solver/narrowing.rs` lines ~268-297 (narrow_by_discriminant function)

### Test Cases

```typescript
// Case 1: Shared discriminant values
type A = { kind: "group1", value: number };
type B = { kind: "group1", name: string };
type C = { kind: "group2", active: boolean };
type U1 = A | B | C;

function f1(x: U1) {
    if (x.kind === "group1") {
        // Should narrow to A | B
    }
}

// Case 2: Mixed with null
type U2 = { type: "ok", data: string } | { type: "error", code: number } | null;

function f2(x: U2) {
    if (x && x.type === "ok") {
        // Should narrow to { type: "ok", data: string }
    }
}
```

### Implementation Plan

1. Read current `narrow_by_discriminant` implementation
2. Rewrite to filter union members based on property value matching
3. Test with simple cases
4. Run conformance tests to verify improvement
