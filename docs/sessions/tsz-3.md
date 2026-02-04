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

4. **Test and Verify**:
   - Once fixed, verify discriminant narrowing works end-to-end
   - Run conformance tests to ensure no regressions

## Test Case

```typescript
type D = { done: true, value: 1 } | { done: false, value: 2 };
function test(o: D) {
    if (o.done === true) {
        const y: 1 = o.value; // Should work - currently gets TS2322
    }
}
```

## Priority

**CRITICAL**: This is feature completeness work. The narrowing rewrite is "dead code" until it's triggered by the type checking pipeline. This aligns with the North Star goal of matching tsc behavior exactly.
