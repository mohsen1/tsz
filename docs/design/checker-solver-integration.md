# Design Document: Checker-Solver Integration Improvements

**Author:** Claude
**Date:** 2026-01-24
**Status:** RFC (Request for Comments)

## Executive Summary

This document critically evaluates potential improvements to the checker-solver integration in TSZ. After deep analysis of the existing architecture, **most initially proposed features already exist or would violate architectural principles**. This document identifies the few genuine opportunities for improvement and honestly rejects ideas that seemed good initially but don't hold up to scrutiny.

---

## 1. Architectural Context

### Current Design Philosophy

The TSZ type system follows a clean separation:

| Component | Responsibility | AST Awareness |
|-----------|----------------|---------------|
| **Solver** | Structural type logic, interning, subtyping | None (pure types) |
| **Checker** | Flow analysis, AST traversal, diagnostics | Full |

This separation is **intentional and correct**. The solver is reusable for LSP, linting, and other tools. The checker handles TypeScript-specific semantics tied to program structure.

### What the Solver Already Does Well

Before proposing new features, we must acknowledge existing capabilities:

1. **Recursive Types**: Coinductive checking with `SubtypeResult::Provisional` handles cycles elegantly
2. **Type Normalization**: Union/intersection simplification built into interning (`never` removal, literal absorption, object merging)
3. **Error Diagnostics**: 14-variant `SubtypeFailureReason` enum with `explain_failure()` function
4. **Apparent Types**: `apparent_primitive_shape_for_key()` synthesizes object shapes for primitives
5. **Variance**: Context-aware bivariance for methods, contravariance for strict mode
6. **Generic Inference**: Union-Find based constraint solving with `InferenceContext`
7. **Branded Types**: Private brand properties (`__private_brand_xxx`) for nominal class checking

---

## 2. Rejected Ideas (With Honest Reasoning)

### 2.1 Control Flow Narrowing in Solver

**Original Proposal**: Move type narrowing logic (typeof, instanceof, truthiness) to the solver.

**Why It's Wrong**:
- Narrowing is fundamentally about **program structure**, not type structure
- Requires understanding of scopes, blocks, control flow graphs
- Would make the solver AST-aware, breaking its core abstraction
- The checker is the correct location for flow-sensitive analysis

**Verdict**: ❌ Rejected. Violates separation of concerns.

### 2.2 Symbolic Constraint Solving

**Original Proposal**: Build a full constraint solver for queries like "under what constraints is this valid?"

**Why It's Wrong**:
- Union-Find inference with `ConstraintSet` already exists
- Full constraint solving is essentially SAT/SMT—massive complexity for marginal benefit
- TypeScript's type system isn't designed around constraint logic
- 99% of real code doesn't need this sophistication

**Verdict**: ❌ Rejected. Over-engineered; current inference is sufficient.

### 2.3 Type Simplification Engine

**Original Proposal**: Add a `simplify_type()` API for normalizing complex types.

**Why It's Wrong**:
- **Already exists**. `normalize_union()` and `normalize_intersection()` in intern.rs handle this
- Interning automatically deduplicates and simplifies
- `string | never` → `string` happens during union construction
- `{a: T} & {b: U}` → merged object happens during intersection construction

**Verdict**: ❌ Rejected. Feature already exists.

### 2.4 Variance Caching

**Original Proposal**: Pre-compute and cache variance of type parameters.

**Why It's Problematic**:
- Variance depends on **context**: `strictFunctionTypes`, `is_method` flag, position in type
- The same type parameter can be covariant in one position, contravariant in another
- Caching would require tracking (type, position, flags) tuples—complex invalidation
- Current on-demand checking is fast enough (single bit checks)

**Verdict**: ❌ Rejected. Complexity exceeds benefit; current approach is adequate.

### 2.5 Type Guards & Assertion Functions in Solver

**Original Proposal**: Solver understands type predicates like `x is string`.

**Why It's Wrong**:
- Same problem as narrowing: requires AST awareness
- Type guards affect **control flow**, not structural typing
- The checker correctly handles this by tracking narrowed types in scope

**Verdict**: ❌ Rejected. Wrong layer.

### 2.6 Apparent Type Resolution

**Original Proposal**: Centralize "apparent type" logic in solver.

**Why It's Wrong**:
- **Already exists**. `apparent_primitive_shape_for_key()` handles this
- Hardcoded method lists for String/Number/Boolean/BigInt primitives
- Synthesizes temporary object shapes for subtype checking

**Verdict**: ❌ Rejected. Feature already exists.

### 2.7 Recursive Type Detection

**Original Proposal**: Add `is_recursive_type()` and unfolding APIs.

**Why It's Wrong**:
- Coinductive checking already handles this transparently
- `SubtypeResult::Provisional` breaks cycles automatically
- Unfolding recursive types defeats interning (creates infinite structures)
- Users rarely need to know if a type is recursive

**Verdict**: ❌ Rejected. Current coinductive approach is superior.

### 2.8 Incremental Dependency Tracking

**Original Proposal**: Track which types depend on which symbols for cache invalidation.

**Why It's Wrong Layer**:
- Symbols are checker/binder concepts; solver works with interned TypeIds
- The solver is deliberately symbol-agnostic after type construction
- Dependency tracking belongs in the checker or a separate layer
- LSP already has file-based invalidation

**Verdict**: ❌ Rejected. Wrong architectural layer.

### 2.9 Branded/Nominal Type Support

**Original Proposal**: First-class brand support in solver.

**Why It's Wrong**:
- **Already exists**. Private brand checking via `__private_brand_xxx` properties
- Structural encoding `string & { __brand: 'UserId' }` works correctly
- Adding special support complicates the type system unnecessarily
- TypeScript is intentionally structural

**Verdict**: ❌ Rejected. Feature already exists via structural encoding.

### 2.10 Rich Error Explanation API

**Original Proposal**: Add detailed error tree generation.

**Why It's Partially Wrong**:
- **Already exists**. `SubtypeFailureReason` with 14 variants, `explain_failure()` function
- The real problem is the **checker not fully utilizing** these facilities
- This is an integration issue, not a missing solver feature

**Verdict**: ⚠️ Reframed. See Section 3.1.

---

## 3. Genuine Opportunities for Improvement

After rejecting 10 ideas, three genuine opportunities remain:

### 3.1 Better Utilization of Existing Error Facilities

**Problem**: The solver provides rich `SubtypeFailureReason` variants, but the checker doesn't fully convert these to user-facing diagnostics.

**Current State**:
```rust
// Solver provides (subtype.rs:704-794)
enum SubtypeFailureReason {
    MissingProperty { name, source_type, target_type },
    PropertyTypeMismatch { property, source_type, target_type, nested_reason },
    ReturnTypeMismatch { source_return, target_return, nested_reason },
    ParameterTypeMismatch { index, source_param, target_param, nested_reason },
    // ... 10 more variants
}
```

**What's Missing**: The checker often generates generic "Type X is not assignable to type Y" instead of leveraging nested reasons like "Property 'foo' is missing" or "Return type 'string' is not assignable to 'number'".

**Proposed Solution**:

```rust
// In checker: New diagnostic generation from solver failures
impl CheckerState {
    fn diagnose_assignment_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
        span: Span,
    ) -> Diagnostic {
        let compat = CompatChecker::new(self.ctx.types);
        match compat.explain_failure(source, target) {
            Some(reason) => self.render_failure_reason(reason, span, 0),
            None => self.generic_assignment_error(source, target, span),
        }
    }

    fn render_failure_reason(
        &self,
        reason: SubtypeFailureReason,
        span: Span,
        depth: usize,
    ) -> Diagnostic {
        match reason {
            SubtypeFailureReason::MissingProperty { name, .. } => {
                Diagnostic::error(span, format!(
                    "Property '{}' is missing in type '{}'",
                    name, self.format_type(source)
                ))
            }
            SubtypeFailureReason::PropertyTypeMismatch { property, nested_reason, .. } => {
                let mut diag = Diagnostic::error(span, format!(
                    "Types of property '{}' are incompatible",
                    property
                ));
                if depth < 3 {
                    diag.add_related(self.render_failure_reason(*nested_reason, span, depth + 1));
                }
                diag
            }
            // ... handle other variants
        }
    }
}
```

**Effort**: Low
**Impact**: High (much better error messages)

### 3.2 Constraint Conflict Detection

**Problem**: When generic inference produces conflicting constraints (e.g., `T <: number` AND `T <: string`), the failure is detected late with poor error messages.

**Current State**: `ConstraintSet` tracks bounds but doesn't eagerly detect conflicts:
```rust
pub struct ConstraintSet {
    lower_bounds: Vec<TypeId>,  // T :> L1, T :> L2, ...
    upper_bounds: Vec<TypeId>,  // T <: U1, T <: U2, ...
}
```

**Proposed Addition** (in solver/infer.rs):

```rust
impl ConstraintSet {
    /// Detect obviously conflicting constraints early
    pub fn detect_conflicts(&self, interner: &TypeInterner) -> Option<ConstraintConflict> {
        // Check if upper bounds are mutually exclusive
        for (i, &u1) in self.upper_bounds.iter().enumerate() {
            for &u2 in &self.upper_bounds[i+1..] {
                if are_disjoint(interner, u1, u2) {
                    return Some(ConstraintConflict::DisjointUpperBounds(u1, u2));
                }
            }
        }

        // Check if any lower bound exceeds all upper bounds
        for &lower in &self.lower_bounds {
            let exceeds_all = self.upper_bounds.iter().all(|&upper| {
                !is_subtype(interner, lower, upper)
            });
            if exceeds_all && !self.upper_bounds.is_empty() {
                return Some(ConstraintConflict::LowerExceedsUpper(lower));
            }
        }

        None
    }
}

fn are_disjoint(interner: &TypeInterner, a: TypeId, b: TypeId) -> bool {
    // string and number are disjoint
    // string and "hello" are NOT disjoint
    // {x: number} and {y: string} are NOT disjoint (can intersect)
    match (interner.lookup(a), interner.lookup(b)) {
        (Some(TypeKey::Intrinsic(k1)), Some(TypeKey::Intrinsic(k2))) => {
            matches!(
                (k1, k2),
                (IntrinsicKind::String, IntrinsicKind::Number) |
                (IntrinsicKind::Number, IntrinsicKind::String) |
                (IntrinsicKind::Boolean, IntrinsicKind::Number) |
                // ... other disjoint pairs
            )
        }
        _ => false  // Conservative: assume not disjoint
    }
}

pub enum ConstraintConflict {
    DisjointUpperBounds(TypeId, TypeId),
    LowerExceedsUpper(TypeId),
}
```

**Effort**: Medium
**Impact**: Medium (better inference errors for edge cases)

### 3.3 Type Formatting in Solver

**Problem**: The checker formats types for error messages, but duplicates logic that the solver understands better.

**Current State**: Type formatting is scattered across checker code, requiring repeated pattern matching on `TypeKey`.

**Proposed Addition** (new file: solver/format.rs):

```rust
/// Format a type as a human-readable string
pub struct TypeFormatter<'a> {
    interner: &'a TypeInterner,
    max_depth: usize,
    max_union_members: usize,
    truncate_objects: bool,
}

impl<'a> TypeFormatter<'a> {
    pub fn new(interner: &'a TypeInterner) -> Self {
        Self {
            interner,
            max_depth: 5,
            max_union_members: 5,
            truncate_objects: true,
        }
    }

    pub fn format(&self, type_id: TypeId) -> String {
        self.format_with_depth(type_id, 0)
    }

    fn format_with_depth(&self, type_id: TypeId, depth: usize) -> String {
        if depth > self.max_depth {
            return "...".to_string();
        }

        match self.interner.lookup(type_id) {
            None => "<unknown>".to_string(),
            Some(TypeKey::Intrinsic(kind)) => self.format_intrinsic(kind),
            Some(TypeKey::Literal(lit)) => self.format_literal(lit),
            Some(TypeKey::Union(list_id)) => {
                let members = self.interner.type_list(list_id);
                self.format_union(members, depth)
            }
            Some(TypeKey::Object(shape)) => self.format_object(shape, depth),
            Some(TypeKey::Function(shape)) => self.format_function(shape, depth),
            // ... other type keys
        }
    }

    fn format_union(&self, members: &[TypeId], depth: usize) -> String {
        if members.len() > self.max_union_members {
            let formatted: Vec<_> = members[..self.max_union_members]
                .iter()
                .map(|&m| self.format_with_depth(m, depth + 1))
                .collect();
            format!("{} | ... {} more", formatted.join(" | "), members.len() - self.max_union_members)
        } else {
            members
                .iter()
                .map(|&m| self.format_with_depth(m, depth + 1))
                .collect::<Vec<_>>()
                .join(" | ")
        }
    }

    // ... other format methods
}
```

**Effort**: Medium
**Impact**: Medium (cleaner code, consistent formatting)

---

## 4. Implementation Priority

| Feature | Effort | Impact | Priority |
|---------|--------|--------|----------|
| Error facility utilization (3.1) | Low | High | **P0** |
| Type formatting in solver (3.3) | Medium | Medium | P1 |
| Constraint conflict detection (3.2) | Medium | Medium | P2 |

### Recommended Approach

1. **Phase 1**: Implement better error message generation using existing `SubtypeFailureReason`
2. **Phase 2**: Add `TypeFormatter` to solver, migrate checker formatting code
3. **Phase 3**: Add constraint conflict detection if inference errors remain problematic

---

## 5. What NOT To Do

This section exists to prevent future proposals of already-rejected ideas:

| Don't Do This | Why |
|---------------|-----|
| Move narrowing to solver | Breaks AST-agnostic design |
| Add constraint logic solver | Over-engineered; Union-Find is sufficient |
| Add "simplify type" API | Already exists in interning |
| Cache variance decisions | Context-dependent; complexity > benefit |
| Add type guard support | Wrong layer; belongs in checker |
| Add branded type syntax | Structural encoding already works |
| Track symbol dependencies | Wrong layer; belongs in checker/binder |

---

## 6. Success Metrics

If we implement the recommended improvements:

1. **Error Message Quality**: Reduce generic "not assignable" errors by 50%+ in favor of specific property/parameter errors
2. **Code Deduplication**: Remove type formatting code from checker (consolidate in solver)
3. **Inference Errors**: Detect constraint conflicts during inference instead of at resolution

---

## 7. Conclusion

After critical analysis, **most ideas for "new solver features" were either already implemented or architecturally inappropriate**. The genuine opportunities lie in:

1. Better utilizing existing solver capabilities (error reasons)
2. Moving shared logic to the right layer (type formatting)
3. Targeted additions for specific pain points (constraint conflicts)

The solver's design is fundamentally sound. The improvements needed are integration and utilization, not new type system features.

---

## Appendix A: Existing Solver Capabilities Reference

### Normalization (intern.rs)
- `normalize_union()`: Flattens, deduplicates, removes `never`, absorbs into `any`/`unknown`
- `normalize_intersection()`: Flattens, removes `unknown`, detects disjoint primitives → `never`

### Error Diagnostics (subtype.rs)
- `SubtypeFailureReason`: 14 variants covering all failure modes
- `explain_failure()`: Two-pass (fast check, then deep explain)

### Variance (subtype_rules/functions.rs)
- Method bivariance: Configurable via `disable_method_bivariance`
- Strict function types: Contravariant parameters when enabled

### Recursion (subtype.rs)
- Coinductive semantics via `SubtypeResult::Provisional`
- `MAX_SUBTYPE_DEPTH = 100`, `MAX_TOTAL_SUBTYPE_CHECKS = 100,000`

### Generic Inference (infer.rs)
- Union-Find with `ena` crate
- `ConstraintSet` with lower/upper bounds
- `InferenceContext` manages fresh type variables
