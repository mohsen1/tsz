//! Flow Analysis Integration with Solver
//!
//! This module integrates flow analysis with the solver's type system to provide:
//! - Type narrowing based on control flow
//! - Definite assignment checking
//! - TDZ (Temporal Dead Zone) validation
//!
//! The key insight is that flow analysis needs to track:
//! 1. **Type Narrowings**: How types change based on control flow guards
//! 2. **Definite Assignments**: Which variables are definitely assigned at a point
//! 3. **TDZ Violations**: Variables used before their declaration

use crate::interner::Atom;
use crate::solver::narrowing::NarrowingContext;
use crate::solver::{TypeDatabase, TypeId};
use rustc_hash::{FxHashMap, FxHashSet};

/// Flow facts that represent the state of variables at a specific program point.
///
/// This structure bridges the checker's flow analysis and the solver's type system
/// by tracking what we know about variables at a given control flow point.
#[derive(Clone, Debug, Default)]
pub struct FlowFacts {
    /// Type narrowings: maps variable name to its narrowed type
    pub type_narrowings: FxHashMap<String, TypeId>,

    /// Variables that are definitely assigned at this point
    pub definite_assignments: FxHashSet<String>,

    /// Variables that violate TDZ (used before declaration)
    pub tdz_violations: FxHashSet<String>,
}

impl FlowFacts {
    /// Create empty flow facts
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a type narrowing for a variable
    pub fn add_narrowing(&mut self, variable: String, narrowed_type: TypeId) {
        self.type_narrowings.insert(variable, narrowed_type);
    }

    /// Mark a variable as definitely assigned
    pub fn mark_definitely_assigned(&mut self, variable: String) {
        self.definite_assignments.insert(variable);
    }

    /// Mark a variable as having a TDZ violation
    pub fn mark_tdz_violation(&mut self, variable: String) {
        self.tdz_violations.insert(variable);
    }

    /// Check if a variable is definitely assigned
    pub fn is_definitely_assigned(&self, variable: &str) -> bool {
        self.definite_assignments.contains(variable)
    }

    /// Check if a variable has a TDZ violation
    pub fn has_tdz_violation(&self, variable: &str) -> bool {
        self.tdz_violations.contains(variable)
    }

    /// Get the narrowed type for a variable (if any)
    pub fn get_narrowed_type(&self, variable: &str) -> Option<TypeId> {
        self.type_narrowings.get(variable).copied()
    }

    /// Merge two flow fact sets (for join points in control flow)
    ///
    /// At control flow join points (e.g., after if/else), we:
    /// - Keep only narrowings that are present in both branches (intersection)
    /// - Keep only definite assignments that are present in both branches
    /// - Union the TDZ violations (if any path has a TDZ violation, it's a violation)
    pub fn merge(&self, other: &FlowFacts) -> FlowFacts {
        let mut result = FlowFacts::new();

        // Intersection for type narrowings (must be narrowed in all paths)
        for (var, ty) in &self.type_narrowings {
            if let Some(other_ty) = other.type_narrowings.get(var) {
                if ty == other_ty {
                    result.type_narrowings.insert(var.clone(), *ty);
                }
            }
        }

        // Intersection for definite assignments (must be assigned in all paths)
        for var in &self.definite_assignments {
            if other.definite_assignments.contains(var) {
                result.definite_assignments.insert(var.clone());
            }
        }

        // Union for TDZ violations (any path with violation is a violation)
        result.tdz_violations = self
            .tdz_violations
            .union(&other.tdz_violations)
            .cloned()
            .collect();

        result
    }
}

/// Flow type evaluator that integrates flow analysis with the solver.
///
/// This evaluator uses the solver's type operations to compute narrowed types
/// based on flow facts gathered during control flow analysis.
pub struct FlowTypeEvaluator<'a> {
    db: &'a dyn TypeDatabase,
    narrowing_context: NarrowingContext<'a>,
}

impl<'a> FlowTypeEvaluator<'a> {
    /// Create a new flow type evaluator
    pub fn new(db: &'a dyn TypeDatabase) -> Self {
        let narrowing_context = NarrowingContext::new(db);
        Self {
            db,
            narrowing_context,
        }
    }

    /// Compute the narrowed type for a variable based on flow facts.
    ///
    /// This integrates with the solver's narrowing logic to apply type guards
    /// (typeof checks, discriminant checks, null checks) to produce a refined type.
    ///
    /// # Arguments
    /// - `original_type`: The declared type of the variable
    /// - `flow_facts`: Flow facts gathered from control flow analysis
    /// - `variable_name`: The name of the variable being checked
    ///
    /// # Returns
    /// The narrowed type, or the original type if no narrowing applies
    pub fn compute_narrowed_type(
        &self,
        original_type: TypeId,
        flow_facts: &FlowFacts,
        variable_name: &str,
    ) -> TypeId {
        // First check if we have a narrowed type from flow facts
        if let Some(narrowed) = flow_facts.get_narrowed_type(variable_name) {
            return narrowed;
        }

        // No narrowing information - return original type
        original_type
    }

    /// Narrow a type based on a typeof guard.
    ///
    /// This is used when flow analysis encounters a typeof check:
    /// ```typescript
    /// if (typeof x === "string") {
    ///     // x is narrowed to string
    /// }
    /// ```
    pub fn narrow_by_typeof(&self, source_type: TypeId, typeof_result: &str) -> TypeId {
        self.narrowing_context
            .narrow_by_typeof(source_type, typeof_result)
    }

    /// Narrow a type based on a discriminant check.
    ///
    /// This is used for discriminated unions:
    /// ```typescript
    /// if (action.type === "add") {
    ///     // action is narrowed to the "add" variant
    /// }
    /// ```
    pub fn narrow_by_discriminant(
        &self,
        union_type: TypeId,
        property_name: Atom,
        literal_value: TypeId,
    ) -> TypeId {
        self.narrowing_context
            .narrow_by_discriminant(union_type, property_name, literal_value)
    }

    /// Narrow a type by excluding a specific type.
    ///
    /// This is used for negative type guards:
    /// ```typescript
    /// if (x !== null) {
    ///     // x is narrowed to non-null
    /// }
    /// ```
    pub fn narrow_excluding_type(&self, source_type: TypeId, excluded_type: TypeId) -> TypeId {
        self.narrowing_context
            .narrow_excluding_type(source_type, excluded_type)
    }

    /// Check if a variable is definitely assigned at this point.
    ///
    /// This integrates with the flow analysis to determine if a variable
    /// has been assigned on all control flow paths leading to this point.
    ///
    /// # Arguments
    /// - `variable`: The name of the variable to check
    /// - `flow_facts`: Flow facts gathered from control flow analysis
    ///
    /// # Returns
    /// true if the variable is definitely assigned, false otherwise
    pub fn is_definitely_assigned(&self, variable: &str, flow_facts: &FlowFacts) -> bool {
        flow_facts.is_definitely_assigned(variable)
    }

    /// Check if a variable has a TDZ (Temporal Dead Zone) violation.
    ///
    /// TDZ violations occur when a variable is used before its declaration:
    /// ```typescript
    /// console.log(x); // TDZ violation!
    /// let x;
    /// ```
    ///
    /// # Arguments
    /// - `variable`: The name of the variable to check
    /// - `flow_facts`: Flow facts gathered from control flow analysis
    ///
    /// # Returns
    /// true if the variable has a TDZ violation, false otherwise
    pub fn has_tdz_violation(&self, variable: &str, flow_facts: &FlowFacts) -> bool {
        flow_facts.has_tdz_violation(variable)
    }

    /// Create flow facts from a set of definite assignments.
    ///
    /// This is a convenience method for creating FlowFacts when you only
    /// have definite assignment information.
    pub fn facts_from_assignments(&self, assignments: FxHashSet<String>) -> FlowFacts {
        FlowFacts {
            definite_assignments: assignments,
            ..Default::default()
        }
    }

    /// Create flow facts from a set of type narrowings.
    ///
    /// This is a convenience method for creating FlowFacts when you only
    /// have type narrowing information.
    pub fn facts_from_narrowings(&self, narrowings: FxHashMap<String, TypeId>) -> FlowFacts {
        FlowFacts {
            type_narrowings: narrowings,
            ..Default::default()
        }
    }
}

#[cfg(test)]
#[path = "flow_analysis_tests.rs"]
mod tests;
