# Checker-Solver Integration Improvements

**Status:** RFC
**Date:** 2026-01-24

## Summary

This document proposes three improvements to how the checker utilizes the solver's existing capabilities. The solver's architecture is sound; the opportunities lie in better integration, not new type system features.

## Background

The checker and solver have a clean separation:

| Component | Responsibility |
|-----------|----------------|
| **Solver** | Structural type logic, interning, subtyping (AST-agnostic) |
| **Checker** | Flow analysis, AST traversal, diagnostics |

The solver already provides: type normalization, rich error diagnostics (`SubtypeFailureReason`), coinductive recursion handling, variance checking, and Union-Find based generic inference.

## Proposed Changes

### 1. Utilize Existing Error Facilities

**Priority:** P0 (Low effort, high impact)

**Problem:** The checker generates generic "Type X is not assignable to type Y" errors instead of leveraging the solver's detailed `SubtypeFailureReason` variants.

**Current solver API** (subtype.rs:704-794):
```rust
enum SubtypeFailureReason {
    MissingProperty { name, source_type, target_type },
    PropertyTypeMismatch { property, nested_reason, ... },
    ReturnTypeMismatch { source_return, target_return, nested_reason },
    ParameterTypeMismatch { index, source_param, target_param, nested_reason },
    TupleElementMismatch { index, ... },
    IndexSignatureMismatch { ... },
    NoUnionMemberMatches { source, union_members },
    // ... 7 more variants
}
```

**Proposed checker addition:**

```rust
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
                    "Types of property '{}' are incompatible", property
                ));
                if depth < 3 {
                    diag.add_related(self.render_failure_reason(*nested_reason, span, depth + 1));
                }
                diag
            }
            // Handle remaining variants...
        }
    }
}
```

**Expected outcome:** Specific error messages like "Property 'foo' is missing" instead of generic assignability errors.

---

### 2. Centralize Type Formatting

**Priority:** P1 (Medium effort, medium impact)

**Problem:** Type-to-string formatting is duplicated across checker code with repeated `TypeKey` pattern matching.

**Proposed addition** (solver/format.rs):

```rust
pub struct TypeFormatter<'a> {
    interner: &'a TypeInterner,
    max_depth: usize,
    max_union_members: usize,
}

impl<'a> TypeFormatter<'a> {
    pub fn new(interner: &'a TypeInterner) -> Self {
        Self { interner, max_depth: 5, max_union_members: 5 }
    }

    pub fn format(&self, type_id: TypeId) -> String {
        self.format_impl(type_id, 0)
    }

    fn format_impl(&self, type_id: TypeId, depth: usize) -> String {
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
            Some(TypeKey::Tuple(elements)) => self.format_tuple(elements, depth),
            // ... remaining type keys
        }
    }
}
```

**Expected outcome:** Single source of truth for type formatting; checker delegates to solver.

---

### 3. Early Constraint Conflict Detection

**Priority:** P2 (Medium effort, medium impact)

**Problem:** Generic inference with conflicting constraints (e.g., `T <: number` AND `T <: string`) fails late with poor error messages.

**Proposed addition** (solver/infer.rs):

```rust
impl ConstraintSet {
    pub fn detect_conflicts(&self, interner: &TypeInterner) -> Option<ConstraintConflict> {
        // Check for mutually exclusive upper bounds
        for (i, &u1) in self.upper_bounds.iter().enumerate() {
            for &u2 in &self.upper_bounds[i+1..] {
                if are_disjoint_primitives(interner, u1, u2) {
                    return Some(ConstraintConflict::DisjointUpperBounds(u1, u2));
                }
            }
        }
        // Check if lower bound exceeds all upper bounds
        for &lower in &self.lower_bounds {
            if !self.upper_bounds.is_empty()
                && self.upper_bounds.iter().all(|&u| !is_subtype(interner, lower, u))
            {
                return Some(ConstraintConflict::LowerExceedsUpper(lower));
            }
        }
        None
    }
}

pub enum ConstraintConflict {
    DisjointUpperBounds(TypeId, TypeId),
    LowerExceedsUpper(TypeId),
}

fn are_disjoint_primitives(interner: &TypeInterner, a: TypeId, b: TypeId) -> bool {
    match (interner.lookup(a), interner.lookup(b)) {
        (Some(TypeKey::Intrinsic(k1)), Some(TypeKey::Intrinsic(k2))) => {
            use IntrinsicKind::*;
            matches!((k1, k2),
                (String, Number) | (Number, String) |
                (String, Boolean) | (Boolean, String) |
                (Number, Boolean) | (Boolean, Number)
            )
        }
        _ => false
    }
}
```

**Expected outcome:** Early detection of unsatisfiable constraints with clear error messages.

---

## Alternatives Considered

| Alternative | Reason Not Pursued |
|-------------|-------------------|
| Control flow narrowing in solver | Would break AST-agnostic design |
| Type simplification API | Already exists (`normalize_union`, `normalize_intersection`) |
| Variance caching | Context-dependent; complexity exceeds benefit |
| Branded type syntax | Structural encoding via intersection already works |
| Symbol dependency tracking | Wrong layer; belongs in checker/binder |

---

## Implementation Plan

| Phase | Change | Files |
|-------|--------|-------|
| 1 | Error facility utilization | `checker/state.rs`, `checker/diagnostics.rs` |
| 2 | Type formatter | New `solver/format.rs`, update `solver/mod.rs` |
| 3 | Constraint conflict detection | `solver/infer.rs` |

---

## Success Criteria

1. Reduce generic "not assignable" errors by 50%+ (measure via test suite error messages)
2. Remove duplicated type formatting code from checker
3. Constraint conflicts detected during inference, not at resolution
