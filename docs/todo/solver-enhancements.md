# Solver Enhancement Roadmap

**Created**: 2026-02-07
**Status**: Planning
**Validated by**: Gemini Pro (full codebase context, 8 parallel feasibility analyses)

This document captures all identified opportunities to push more TypeScript complexity into the solver, ordered by effort and impact. Every item was validated against the actual codebase by Gemini Pro with full solver context (~629K tokens).

---

## Priority Matrix

| # | Feature | Complexity | Risk | Solver Change? | Blocked By |
|---|---------|-----------|------|----------------|------------|
| 1 | [Const type parameters](#1-const-type-parameters-ts-50) | **Low** | Low | ~5 lines in `infer.rs` | Nothing |
| 2 | [`satisfies` operator](#2-satisfies-operator-ts-49) | **Low** | Low | None (checker only) | Nothing |
| 3 | [`NoInfer<T>`](#3-noinfer-ts-54) | **Low** | Low | New `TypeKey` variant | Nothing |
| 4 | [Assertion functions](#4-assertion-functions) | Low/Med | Low | Solver done; checker work | Nothing |
| 5 | [CFA closure invalidation](#5-cfa-closure-invalidation) | Low/Med | Low | None (checker fix) | Nothing |
| 6 | [Homomorphic mapped types](#6-homomorphic-mapped-type-preservationarraytuple) | Medium | Low | `evaluate_rules/mapped.rs` | Nothing |
| 7 | [Overload resolution](#7-overload-resolution) | Medium | Low | `operations.rs` | Nothing |
| 8 | [Recursive conditional tail-call](#8-recursive-conditional-type-tail-call-optimization-ts-45) | Medium | Low | `evaluate_rules/conditional.rs` | Nothing |
| 9 | [Template literal: mapped key remapping](#9-template-literal-mapped-type-key-remapping) | Medium | Medium | `evaluate_rules/mapped.rs` | Nothing |
| 10 | [Template literal: cross-product soundness](#10-template-literal-cross-product-soundness) | Medium | Low | New unexpanded state | Nothing |
| 11 | [Template literal: subtype structural equiv](#11-template-literal-subtype-structural-equivalence) | **High** | Medium | `subtype_rules/literals.rs` | Nothing |

---

## Tier 1: Low-Hanging Fruit (Low complexity, high confidence)

### 1. Const Type Parameters (TS 5.0)

**Effort**: ~5 lines changed | **Files**: `crates/tsz-solver/src/infer.rs`

The infrastructure is 90% complete. The `is_const: bool` field already exists on `TypeParamInfo` (types.rs), and `apply_const_assertion` already exists in `widening.rs` doing exactly the right transformation (preserve literals, convert arrays to readonly tuples, make object properties readonly).

**What to do:**

In `infer.rs` → `resolve_from_candidates` (around line 1068), the `is_const` branch currently only skips widening. Change it to call `apply_const_assertion`:

```rust
// Current (incomplete):
let widened = if is_const {
    filtered_no_never.iter().map(|c| c.type_id).collect()
} else {
    self.widen_candidate_types(&filtered_no_never)
};

// Fixed:
let widened = if is_const {
    filtered_no_never.iter()
        .map(|c| crate::widening::apply_const_assertion(self.interner, c.type_id))
        .collect()
} else {
    self.widen_candidate_types(&filtered_no_never)
};
```

**Edge cases to verify:**
- `const T extends string[]` — readonly inferred type violates mutable constraint → TS2344
- Defaults on const params may need `apply_const_assertion` in `instantiate.rs`
- Nested objects: `foo({ a: { b: 1 } })` should deeply readonly

**Tests to add** (in `infer_tests.rs`):
- `foo([1])` → `readonly [1]`
- `foo({a: 1})` → `{ readonly a: 1 }`
- `foo("a")` → `"a"` (not `string`)

---

### 2. `satisfies` Operator (TS 4.9)

**Effort**: Checker-only | **Files**: `crates/tsz-checker/src/expr.rs` (or equivalent)

No new solver API needed. `satisfies` is: contextual typing + assignability check + type preservation.

**What to do:**

```rust
fn check_satisfies_expression(&mut self, node: NodeIndex) -> TypeId {
    let target_type = self.resolve_type(type_node);           // RHS
    let expr_type = self.check_expression(expr, Some(target_type)); // LHS with context
    if !self.solver.is_assignable_to(expr_type, target_type) {
        self.report_error(node, /* TS1360 */);
    }
    expr_type  // Return original narrow type, NOT target_type
}
```

**Key behaviors:**
- Contextual type flows into the expression (lambdas, object literals benefit)
- Excess property checking applies (fresh object literals)
- Result type is the expression type, not the satisfies target

---

### 3. `NoInfer<T>` (TS 5.4)

**Effort**: Low (~10 files, trivial changes each) | **Files**: types.rs, intern.rs, visitor.rs, lower.rs, evaluate.rs, infer.rs, format.rs, instantiate.rs

**What to do:**

1. Add `TypeKey::NoInfer(TypeId)` variant to `types.rs`
2. In `lower.rs`: detect `NoInfer` name in `lower_type_reference`, emit new variant
3. In `evaluate.rs`: strip wrapper → return `evaluate(inner)`
4. In `infer.rs` → `infer_from_types`: if target is `NoInfer`, return `Ok(())` (block inference)
5. In `instantiate.rs`: preserve wrapper → `NoInfer(instantiated_inner)`
6. In `visitor.rs`: traverse inner type
7. In `format.rs`: print `NoInfer<...>`
8. Update interner boilerplate

**Critical design insight**: `infer_from_types` inspects unevaluated structure via `interner.lookup()` (sees `NoInfer` → blocks). Subtyping calls `evaluate_type` first (strips `NoInfer` → checks inner type). This duality makes it work correctly.

---

## Tier 2: Medium Effort (Existing infrastructure, moderate work)

### 4. Assertion Functions

**Effort**: Low/Medium | **Solver**: Done | **Checker**: Needs work

The solver already has everything:
- `TypePredicate` struct with `asserts: bool` field in `types.rs`
- `FunctionShape` has `type_predicate: Option<TypePredicate>`
- `lower.rs` correctly parses `asserts` from AST
- `narrowing.rs` handles `TypeGuard::Predicate` for assertion predicates

**Remaining checker work** (in call expression checking):
1. After resolving a call signature, check `signature.type_predicate`
2. If `predicate.asserts == true`:
   - Identify the target variable (argument matching `predicate.target`)
   - Call `solver.narrow_type(current_type, guard, true)`
   - Update current `FlowFacts` with the narrowed type
   - If narrowed to `never`, mark flow as unreachable

**Edge cases:**
- `asserts condition` (maps argument expression to TypeGuard)
- Generic assertions: `asserts val is T` with generic `T`
- `asserts this is T` — narrow `this` type in current context
- Assertion narrowing to `never` → subsequent code unreachable

**Binder**: No changes needed. CFG structure is unchanged; narrowing is data-flow.

---

### 5. CFA Closure Invalidation

**Effort**: Low/Medium | **Files**: `crates/tsz-checker/src/flow_analyzer.rs`

No solver changes. The bug is in the checker: `is_captured_variable` uses a stale `self.binder.current_scope_id` instead of computing the usage scope on-demand.

**What to fix:**

```rust
// Current (buggy):
let current_scope_id = self.binder.current_scope_id; // Stale after binding

// Fixed:
let decl_scope_id = self.binder.find_enclosing_scope(self.arena, decl_id);
let usage_scope_id = self.binder.find_enclosing_scope(self.arena, reference);
// If usage is nested inside a different function scope than declaration → captured
```

**Edge cases (acceptable to be pessimistic initially):**
- IIFEs: TypeScript preserves narrowing (synchronous execution). Can implement later.
- Synchronous callbacks (`array.map`): TypeScript preserves. Can implement later.
- `const` variables: Already handled — immutable variables preserve narrowing.

---

### 6. Homomorphic Mapped Type Preservation (Array/Tuple)

**Effort**: Medium | **Files**: `crates/tsz-solver/src/evaluate_rules/mapped.rs`

Currently, `Partial<[number, string]>` degrades to a plain object. TypeScript preserves the tuple structure: `[(number | undefined)?, (string | undefined)?]`.

**What to do:**

Add two new methods in `mapped.rs`, called from `evaluate_mapped` when the source is an array/tuple and `name_type` is `None`:

1. **`evaluate_mapped_array(mapped, elem_type)`**:
   - Substitute `K` → `TypeId::NUMBER`
   - Instantiate + evaluate template
   - Apply optional modifier (union with `undefined` if `+?`)
   - Apply readonly modifier (wrap in `ReadonlyType` if `+readonly`)
   - Return `TypeKey::Array(new_element_type)`

2. **`evaluate_mapped_tuple(mapped, tuple_list_id)`**:
   - Iterate tuple elements
   - For each element at index `I`: substitute `K` → `LiteralString("I")`
   - Instantiate + evaluate template for each
   - Rest elements: substitute `K` → `number`, preserve `rest: true`
   - Apply optional/readonly modifiers per-element
   - Return `TypeKey::Tuple(new_elements)`

**No new TypeKey variants needed.** Existing `Array`, `Tuple`, `ReadonlyType` are sufficient.

**Edge cases:**
- `as` clause present → fall back to object (keys change, structure not preserved)
- Empty tuples → map to empty tuple
- Rest elements → use array mapping for the rest portion
- Source is `readonly` → preserve readonly unless `-readonly`

---

### 7. Overload Resolution

**Effort**: Medium | **Files**: `crates/tsz-solver/src/operations.rs`

The solver already has `resolve_callable_call` (line 1280) that tries signatures. Two improvements needed:

**A. Contextual signature selection** (`get_contextual_signature`, line 226):
- Accept `arg_count: Option<usize>` parameter
- Iterate through `call_signatures`, find first where arity matches:
  ```
  min_args <= count && (has_rest || count <= params.len())
  ```

**B. Better error aggregation** (in `resolve_callable_call`):
- Track "best failure" (most arguments matched) for error reporting
- Return specific error for closest-matching overload instead of generic "NoOverloadMatch"

**Architecture note**: This is a pure solver operation. The checker passes argument types and count; the solver does set-theoretic matching. No AST access needed.

**Edge cases:**
- Rest parameters: match any count >= required
- Optional parameters: match range [required, total]
- Generic overloads: inference must be transactional (already is — fresh `InferenceContext` per attempt)
- Method bivariance: already handled via `func.is_method`

---

### 8. Recursive Conditional Type Tail-Call Optimization (TS 4.5)

**Effort**: Medium | **Files**: `crates/tsz-solver/src/evaluate_rules/conditional.rs`

**Already partially implemented.** The `evaluate_conditional` function has a `loop` with `tail_recursion_count` and `MAX_TAIL_RECURSION_DEPTH` (1000). But it only detects direct `TypeKey::Conditional` in tail position.

**What's missing**: Detection of `TypeKey::Application` in tail position (the common pattern):
```typescript
type TrimLeft<T extends string> = T extends ` ${infer R}` ? TrimLeft<R> : T;
//                                                           ^^^^^^^^^^^ Application, not Conditional
```

**What to do:**

Extend the tail check in `evaluate_conditional` (around line 228):

```rust
// Existing: direct Conditional detection
if let Some(TypeKey::Conditional(next_cond_id)) = self.interner().lookup(result_branch) {
    // ... loop
}

// Add: Application expanding to Conditional
if let Some(TypeKey::Application(app_id)) = self.interner().lookup(result_branch) {
    if let Some(expanded) = self.try_expand_application_structural(app_id) {
        if let Some(TypeKey::Conditional(next_cond_id)) = self.interner().lookup(expanded) {
            current_cond = (*self.interner().conditional_type(next_cond_id)).clone();
            tail_recursion_count += 1;
            continue;
        }
    }
}
```

**Performance impact**: Reduces stack depth from O(N) to O(1). Increases effective recursion limit from 50 (stack) to 1000 (iteration).

---

## Tier 3: Template Literal Improvements

### 9. Template Literal: Mapped Type Key Remapping

**Effort**: Medium | **Risk**: Medium | **Files**: `crates/tsz-solver/src/evaluate_rules/mapped.rs`

**Problem**: In `mapped.rs` around line 280, when a template literal in the `as` clause evaluates to a Union, `literal_string()` returns `None` and evaluation bails out.

```typescript
type T = { a: 1 };
type M = { [K in keyof T as `${K}1` | `${K}2`]: T[K] };
// Expected: { a1: 1, a2: 1 }
// Current:  deferred Mapped type (wrong)
```

**What to do:**
When `remapped` is a Union, iterate its members, extract each string literal, and generate a property for each:

```rust
let remapped_name = match crate::visitor::literal_string(self.interner(), remapped) {
    Some(name) => name,
    None => {
        // NEW: Handle Union of string literals
        if let Some(TypeKey::Union(list_id)) = self.interner().lookup(remapped) {
            let members = self.interner().type_list(list_id);
            for member in members {
                if let Some(name) = crate::visitor::literal_string(self.interner(), *member) {
                    // Add property with this name and the evaluated template value
                    props.push(/* ... */);
                }
            }
            continue; // Skip the single-property path below
        }
        return self.interner().mapped(mapped.clone()); // Fallback
    }
};
```

---

### 10. Template Literal: Cross-Product Soundness

**Effort**: Medium | **Risk**: Low | **Files**: `evaluate_rules/template_literal.rs`, `subtype_rules/literals.rs`

**Problem**: When expansion exceeds `TEMPLATE_LITERAL_EXPANSION_LIMIT` (100K), the template degrades to `string`. This is **unsound**: `"z" <: ExpandedTemplate` returns true even when `"z"` isn't in the set.

**What to do:**
1. Instead of returning `TypeId::STRING` on overflow, return the **unexpanded** template literal type (keep `TypeKey::TemplateLiteral` as-is, just don't expand)
2. In subtyping, use the existing backtracking matcher from `subtype_rules/literals.rs` (`match_template_literal_recursive`) to check `Literal <: TemplateLiteral` without expansion
3. `TemplateLiteral <: String` → always true (no expansion needed)

---

### 11. Template Literal: Subtype Structural Equivalence

**Effort**: High | **Risk**: Medium | **Files**: `crates/tsz-solver/src/subtype_rules/literals.rs`

**Problem**: Comparing unaligned template spans without expansion. E.g., `` `a${"b"}c` `` (normalizes to `"abc"`) vs `` `ab${"c"}` `` (normalizes to `"abc"`). If both sides have type interpolations (not just literals), structural comparison requires sophisticated matching.

**Recommendation**: Defer this. The current normalization during interning handles the literal-only cases. Complex type-vs-type span alignment is rare in practice and high risk for regressions.

---

## Implementation Order (Recommended)

### Phase 1: Quick Wins (1-2 days each)
1. **Const type parameters** — ~5 lines, infrastructure exists
2. **`satisfies` operator** — checker only, reuses existing solver APIs
3. **`NoInfer<T>`** — additive, isolated, low risk

### Phase 2: Checker Improvements (2-3 days each)
4. **Assertion functions** — solver done, wire up checker
5. **CFA closure invalidation** — fix stale scope bug

### Phase 3: Solver Core (3-5 days each)
6. **Homomorphic mapped types** — array/tuple preservation
7. **Overload resolution** — arity matching + error aggregation
8. **Recursive conditional tail-call** — detect Application in tail position

### Phase 4: Template Literals (3-5 days each)
9. **Mapped key remapping** — handle Union in `as` clause
10. **Cross-product soundness** — keep unexpanded form
11. **Subtype structural equivalence** — defer unless needed

---

## Validation Notes

All feasibility assessments were produced by Gemini Pro with full solver codebase context (62 files, ~629K tokens). Key findings:

- **No new TypeKey variants needed** for items 1-8 (except `NoInfer` in item 3)
- **Solver changes not needed** for items 2 and 5 (pure checker work)
- **Existing infrastructure leveraged** throughout:
  - `apply_const_assertion` in `widening.rs` (item 1)
  - `TypePredicate` with `asserts: bool` (item 4)
  - `FlowAnalyzer` + `find_enclosing_scope` (item 5)
  - `resolve_callable_call` + transactional `InferenceContext` (item 7)
  - `tail_recursion_count` loop in `evaluate_conditional` (item 8)
  - `match_template_literal_recursive` backtracking matcher (item 10)
