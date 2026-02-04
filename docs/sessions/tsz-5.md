# Session tsz-5: Advanced CFA - Type Predicates & Exhaustiveness

**Started**: 2026-02-04
**Status**: ACTIVE
**Previous**: tsz-3 (CFA Infrastructure - COMPLETED)

## Context

Session tsz-3 completed the architectural foundation for CFA:
- **Type Environment Unification**: Enabled `Lazy` type resolution (type aliases) during narrowing by sharing `Rc<RefCell<TypeEnvironment>>` across the Checker and Solver.
- **Loop Narrowing**: Implemented conservative widening for mutable variables and preservation for constants.
- **Cache Validation**: Verified that the triple-keyed flow cache (Node, Symbol, InitialType) correctly handles widened types without poisoning.

This session builds on that foundation to implement advanced TypeScript features that require deep integration between the `FlowAnalyzer` (Checker) and `NarrowingContext` (Solver).

## Session Goals

1. **User-Defined Type Guards**: Support `is` and `asserts` predicates.
2. **Property Path Narrowing**: Narrow nested properties (e.g., `if (user.address) ...`).
3. **Switch Statement Exhaustiveness**: Narrow to `never` when all union constituents are handled.
4. **Reachability Analysis**: Detect unreachable code and missing return statements.

---

## Priority 1: User-Defined Type Guards ✅ COMPLETE

### Problem
Currently, calls to functions returning `arg is T` or `asserts arg is T` do not trigger narrowing in the `FlowAnalyzer`.

### Status
**Feature Already Implemented!** The infrastructure exists in:
- `src/checker/control_flow_narrowing.rs` - `narrow_by_call_predicate` implementation
- `src/checker/control_flow.rs` - Called from `narrow_type_by_condition_inner`
- Tests: `test_user_defined_type_predicate_narrows_branches`, `test_user_defined_type_predicate_alias_narrows`

### Tasks
- [x] **FlowAnalyzer Update**: Recognize `TypePredicate` nodes in function return types during call expression checking in `src/checker/flow_analysis.rs`.
- [x] **NarrowingContext Integration**: Update `NarrowingContext` in `src/solver/narrowing.rs` to apply the predicate type to the target symbol.
- [x] **Asserts Support**: Implement "assertion" narrowing where the flow following the call is narrowed regardless of a conditional check.

---

## Priority 2: Property Path Narrowing

### Problem
Narrowing a property (e.g., `if (x.kind === "a")`) should narrow the parent object `x` if it is a union of types with a `kind` discriminant.

### Tasks
- [ ] **Path Tracking**: Enhance `NarrowingContext` to track property access paths (e.g., `['user', 'address', 'zip']`).
- [ ] **Union Refinement**: Implement logic in `src/solver/operations.rs` or `narrowing.rs` to refine a union based on a narrowed property path.
- [ ] **Edge Case**: Handle optional chaining and potential `null`/`undefined` in the path.

---

## Priority 3: Switch Exhaustiveness

### Problem
After a `switch` on a discriminated union, the `default` block or the code immediately following the switch should narrow the variable to `never` if all possible constituents have been handled in `case` branches.

### Tasks
- [ ] **Exhaustion Logic**: In `src/checker/flow_analysis.rs`, track which constituents of a union have been "consumed" by `case` labels.
- [ ] **Union Subtraction**: Use the Solver to subtract the handled types from the initial union type.
- [ ] **Diagnostic Reporting**: Report `TS2534` if a `default` case is reached but the type is not `never` (when exhaustiveness is expected).

---

## Priority 4: Reachability Analysis

### Problem
The compiler needs to report errors for unreachable code (TS2534) and functions that don't return a value on all code paths (TS2366).

### Tasks
- [ ] **CFG Traversal**: Use the `FlowNode` graph built by the Binder to identify nodes with no incoming active flow.
- [ ] **Return Path Validation**: Implement a check in `src/checker/statements.rs` that ensures all paths in a non-void function end in a `return` or `throw`.

---

## Critical Reminders

- **The Two-Question Rule**: Before implementing any logic in `src/solver/` or `src/checker/`, you **MUST** consult Gemini for approach validation and implementation review.
- **Lazy Resolution**: Always use `ctx.type_environment` when resolving types to ensure type aliases are correctly handled, as established in `tsz-3`.
- **Visitor Pattern**: Use the visitor pattern from `src/solver/visitor.rs` for any complex type traversals.

## Complexity: HIGH

**Why High**: These features involve complex interactions between the AST (Checker), the Control Flow Graph (Binder), and Type Relations (Solver). Property path narrowing in particular requires careful management of symbol identities across different scopes.
