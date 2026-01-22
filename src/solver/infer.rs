//! Type inference engine using Union-Find.
//!
//! This module implements type inference for generic functions using
//! the `ena` crate's Union-Find data structure.
//!
//! Key features:
//! - Inference variables for generic type parameters
//! - Constraint collection during type checking
//! - Bounds checking (L <: α <: U)
//! - Best common type calculation
//! - Efficient unification with path compression

use crate::interner::Atom;
use crate::solver::TypeDatabase;
use crate::solver::types::*;
use ena::unify::{InPlaceUnificationTable, NoError, UnifyKey, UnifyValue};
use rustc_hash::FxHashSet;

#[cfg(test)]
use crate::solver::TypeInterner;

/// An inference variable representing an unknown type.
/// These are created when instantiating generic functions.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct InferenceVar(pub u32);

/// Wrapper for TypeId to implement UnifyValue (avoiding orphan rule)
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct InferenceValue(pub Option<TypeId>);

impl UnifyKey for InferenceVar {
    type Value = InferenceValue;

    fn index(&self) -> u32 {
        self.0
    }

    fn from_index(u: u32) -> Self {
        InferenceVar(u)
    }

    fn tag() -> &'static str {
        "InferenceVar"
    }
}

impl UnifyValue for InferenceValue {
    type Error = NoError;

    fn unify_values(a: &Self, b: &Self) -> Result<Self, Self::Error> {
        match (a.0, b.0) {
            (None, None) => Ok(InferenceValue(None)),
            (Some(t), None) | (None, Some(t)) => Ok(InferenceValue(Some(t))),
            (Some(a), Some(b)) if a == b => Ok(InferenceValue(Some(a))),
            // When types conflict, prefer the first (or could return error)
            (Some(a), Some(_)) => Ok(InferenceValue(Some(a))),
        }
    }
}

/// Inference error
#[derive(Clone, Debug)]
pub enum InferenceError {
    /// Two incompatible types were unified
    Conflict(TypeId, TypeId),
    /// Inference variable was not resolved
    Unresolved(InferenceVar),
    /// Circular unification detected (occurs-check)
    OccursCheck { var: InferenceVar, ty: TypeId },
    /// Lower bound is not subtype of upper bound
    BoundsViolation {
        var: InferenceVar,
        lower: TypeId,
        upper: TypeId,
    },
    /// Variance violation detected
    VarianceViolation {
        var: InferenceVar,
        expected_variance: &'static str,
        position: TypeId,
    },
}

/// Constraint set for an inference variable.
/// Tracks both lower bounds (L <: α) and upper bounds (α <: U).
#[derive(Clone, Debug, Default)]
pub struct ConstraintSet {
    /// Lower bounds: types that must be subtypes of this variable
    /// e.g., from argument types being assigned to a parameter
    pub lower_bounds: Vec<TypeId>,
    /// Upper bounds: types that this variable must be a subtype of
    /// e.g., from `extends` constraints on type parameters
    pub upper_bounds: Vec<TypeId>,
}

impl ConstraintSet {
    pub fn new() -> Self {
        ConstraintSet {
            lower_bounds: Vec::new(),
            upper_bounds: Vec::new(),
        }
    }

    /// Add a lower bound constraint: L <: α
    pub fn add_lower_bound(&mut self, ty: TypeId) {
        if !self.lower_bounds.contains(&ty) {
            self.lower_bounds.push(ty);
        }
    }

    /// Add an upper bound constraint: α <: U
    pub fn add_upper_bound(&mut self, ty: TypeId) {
        if !self.upper_bounds.contains(&ty) {
            self.upper_bounds.push(ty);
        }
    }

    /// Check if there are any constraints
    pub fn is_empty(&self) -> bool {
        self.lower_bounds.is_empty() && self.upper_bounds.is_empty()
    }

    pub fn merge_from(&mut self, other: ConstraintSet) {
        for ty in other.lower_bounds {
            self.add_lower_bound(ty);
        }
        for ty in other.upper_bounds {
            self.add_upper_bound(ty);
        }
    }
}

struct TupleRestExpansion {
    fixed: Vec<TupleElement>,
    variadic: Option<TypeId>,
}

/// Type inference context for a single function call or expression.
pub struct InferenceContext<'a> {
    interner: &'a dyn TypeDatabase,
    /// Unification table for inference variables
    table: InPlaceUnificationTable<InferenceVar>,
    /// Map from type parameter names to inference variables
    type_params: Vec<(Atom, InferenceVar)>,
    /// Constraints for each inference variable
    constraints: Vec<ConstraintSet>,
}

impl<'a> InferenceContext<'a> {
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        InferenceContext {
            interner,
            table: InPlaceUnificationTable::new(),
            type_params: Vec::new(),
            constraints: Vec::new(),
        }
    }

    /// Create a fresh inference variable
    pub fn fresh_var(&mut self) -> InferenceVar {
        let var = self.table.new_key(InferenceValue(None));
        let idx = var.0 as usize;
        debug_assert_eq!(idx, self.constraints.len());
        self.constraints.push(ConstraintSet::new());
        var
    }

    /// Create an inference variable for a type parameter
    pub fn fresh_type_param(&mut self, name: Atom) -> InferenceVar {
        let var = self.fresh_var();
        self.type_params.push((name, var));
        var
    }

    /// Register an existing inference variable as representing a type parameter.
    ///
    /// This is useful when the caller needs to compute a unique placeholder name
    /// (and corresponding placeholder TypeId) after allocating the inference variable.
    pub fn register_type_param(&mut self, name: Atom, var: InferenceVar) {
        self.type_params.push((name, var));
    }

    /// Look up an inference variable by type parameter name
    pub fn find_type_param(&self, name: Atom) -> Option<InferenceVar> {
        self.type_params
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| *v)
    }

    /// Probe the current value of an inference variable
    pub fn probe(&mut self, var: InferenceVar) -> Option<TypeId> {
        self.table.probe_value(var).0
    }

    /// Unify an inference variable with a concrete type
    pub fn unify_var_type(&mut self, var: InferenceVar, ty: TypeId) -> Result<(), InferenceError> {
        // Get the root variable
        let root = self.table.find(var);

        if self.occurs_in(root, ty) {
            return Err(InferenceError::OccursCheck { var: root, ty });
        }

        // Check current value
        match self.table.probe_value(root).0 {
            None => {
                // No value yet, assign it
                self.table.union_value(root, InferenceValue(Some(ty)));
                Ok(())
            }
            Some(existing) => {
                // Check compatibility
                if self.types_compatible(existing, ty) {
                    Ok(())
                } else {
                    Err(InferenceError::Conflict(existing, ty))
                }
            }
        }
    }

    /// Unify two inference variables
    pub fn unify_vars(&mut self, a: InferenceVar, b: InferenceVar) -> Result<(), InferenceError> {
        let root_a = self.table.find(a);
        let root_b = self.table.find(b);

        if root_a == root_b {
            return Ok(());
        }

        let value_a = self.table.probe_value(root_a).0;
        let value_b = self.table.probe_value(root_b).0;
        if let (Some(a_ty), Some(b_ty)) = (value_a, value_b)
            && !self.types_compatible(a_ty, b_ty)
        {
            return Err(InferenceError::Conflict(a_ty, b_ty));
        }

        self.table
            .unify_var_var(root_a, root_b)
            .map_err(|_| InferenceError::Conflict(TypeId::ERROR, TypeId::ERROR))?;

        let new_root = self.table.find(root_a);
        let root_a_idx = root_a.0 as usize;
        let root_b_idx = root_b.0 as usize;
        let new_root_idx = new_root.0 as usize;
        debug_assert!(new_root_idx == root_a_idx || new_root_idx == root_b_idx);

        let mut merged = ConstraintSet::new();
        merged.merge_from(std::mem::take(&mut self.constraints[root_a_idx]));
        if root_b_idx != root_a_idx {
            merged.merge_from(std::mem::take(&mut self.constraints[root_b_idx]));
        }
        self.constraints[new_root_idx] = merged;
        Ok(())
    }

    /// Check if two types are compatible for unification
    fn types_compatible(&self, a: TypeId, b: TypeId) -> bool {
        if a == b {
            return true;
        }

        // Any is compatible with everything
        if a == TypeId::ANY || b == TypeId::ANY {
            return true;
        }

        // Unknown is compatible with everything
        if a == TypeId::UNKNOWN || b == TypeId::UNKNOWN {
            return true;
        }

        // Never is compatible with everything
        if a == TypeId::NEVER || b == TypeId::NEVER {
            return true;
        }

        false
    }

    fn occurs_in(&mut self, var: InferenceVar, ty: TypeId) -> bool {
        let root = self.table.find(var);
        if self.type_params.is_empty() {
            return false;
        }

        let mut visited = FxHashSet::default();
        for &(atom, param_var) in &self.type_params {
            if self.table.find(param_var) == root
                && self.type_contains_param(ty, atom, &mut visited)
            {
                return true;
            }
        }
        false
    }

    fn type_param_names_for_root(&mut self, root: InferenceVar) -> Vec<Atom> {
        self.type_params
            .iter()
            .filter_map(|(name, var)| {
                if self.table.find(*var) == root {
                    Some(*name)
                } else {
                    None
                }
            })
            .collect()
    }

    fn upper_bound_cycles_param(&mut self, bound: TypeId, targets: &[Atom]) -> bool {
        let mut params = FxHashSet::default();
        let mut visited = FxHashSet::default();
        self.collect_type_params(bound, &mut params, &mut visited);

        for name in params {
            let mut seen = FxHashSet::default();
            if self.param_depends_on_targets(name, targets, &mut seen) {
                return true;
            }
        }

        false
    }

    fn expand_cyclic_upper_bound(
        &mut self,
        root: InferenceVar,
        bound: TypeId,
        target_names: &[Atom],
        lower_bounds: &mut Vec<TypeId>,
        upper_bounds: &mut Vec<TypeId>,
    ) {
        let name = match self.interner.lookup(bound) {
            Some(TypeKey::TypeParameter(info)) | Some(TypeKey::Infer(info)) => info.name,
            _ => return,
        };

        let Some(var) = self.find_type_param(name) else {
            return;
        };

        if let Some(resolved) = self.probe(var) {
            if !upper_bounds.contains(&resolved) {
                upper_bounds.push(resolved);
            }
            return;
        }

        let bound_root = self.table.find(var);
        let constraints = self.constraints[bound_root.0 as usize].clone();

        for ty in constraints.lower_bounds {
            if self.occurs_in(root, ty) {
                continue;
            }
            if !lower_bounds.contains(&ty) {
                lower_bounds.push(ty);
            }
        }

        for ty in constraints.upper_bounds {
            if self.occurs_in(root, ty) {
                continue;
            }
            if !target_names.is_empty() && self.upper_bound_cycles_param(ty, target_names) {
                continue;
            }
            if !upper_bounds.contains(&ty) {
                upper_bounds.push(ty);
            }
        }
    }

    fn collect_type_params(
        &self,
        ty: TypeId,
        params: &mut FxHashSet<Atom>,
        visited: &mut FxHashSet<TypeId>,
    ) {
        if !visited.insert(ty) {
            return;
        }
        let Some(key) = self.interner.lookup(ty) else {
            return;
        };

        match key {
            TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
                params.insert(info.name);
            }
            TypeKey::Array(elem) => {
                self.collect_type_params(elem, params, visited);
            }
            TypeKey::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                for element in elements.iter() {
                    self.collect_type_params(element.type_id, params, visited);
                }
            }
            TypeKey::Union(members) | TypeKey::Intersection(members) => {
                let members = self.interner.type_list(members);
                for &member in members.iter() {
                    self.collect_type_params(member, params, visited);
                }
            }
            TypeKey::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in shape.properties.iter() {
                    self.collect_type_params(prop.type_id, params, visited);
                }
            }
            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in shape.properties.iter() {
                    self.collect_type_params(prop.type_id, params, visited);
                }
                if let Some(index) = shape.string_index.as_ref() {
                    self.collect_type_params(index.key_type, params, visited);
                    self.collect_type_params(index.value_type, params, visited);
                }
                if let Some(index) = shape.number_index.as_ref() {
                    self.collect_type_params(index.key_type, params, visited);
                    self.collect_type_params(index.value_type, params, visited);
                }
            }
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.collect_type_params(app.base, params, visited);
                for &arg in app.args.iter() {
                    self.collect_type_params(arg, params, visited);
                }
            }
            TypeKey::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                for param in shape.params.iter() {
                    self.collect_type_params(param.type_id, params, visited);
                }
                if let Some(this_type) = shape.this_type {
                    self.collect_type_params(this_type, params, visited);
                }
                self.collect_type_params(shape.return_type, params, visited);
            }
            TypeKey::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                for sig in shape.call_signatures.iter() {
                    for param in sig.params.iter() {
                        self.collect_type_params(param.type_id, params, visited);
                    }
                    if let Some(this_type) = sig.this_type {
                        self.collect_type_params(this_type, params, visited);
                    }
                    self.collect_type_params(sig.return_type, params, visited);
                }
                for sig in shape.construct_signatures.iter() {
                    for param in sig.params.iter() {
                        self.collect_type_params(param.type_id, params, visited);
                    }
                    if let Some(this_type) = sig.this_type {
                        self.collect_type_params(this_type, params, visited);
                    }
                    self.collect_type_params(sig.return_type, params, visited);
                }
                for prop in shape.properties.iter() {
                    self.collect_type_params(prop.type_id, params, visited);
                }
            }
            TypeKey::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(cond_id);
                self.collect_type_params(cond.check_type, params, visited);
                self.collect_type_params(cond.extends_type, params, visited);
                self.collect_type_params(cond.true_type, params, visited);
                self.collect_type_params(cond.false_type, params, visited);
            }
            TypeKey::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(mapped_id);
                self.collect_type_params(mapped.constraint, params, visited);
                if let Some(name_type) = mapped.name_type {
                    self.collect_type_params(name_type, params, visited);
                }
                self.collect_type_params(mapped.template, params, visited);
            }
            TypeKey::IndexAccess(obj, idx) => {
                self.collect_type_params(obj, params, visited);
                self.collect_type_params(idx, params, visited);
            }
            TypeKey::KeyOf(operand) | TypeKey::ReadonlyType(operand) => {
                self.collect_type_params(operand, params, visited);
            }
            TypeKey::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                for span in spans.iter() {
                    if let TemplateSpan::Type(inner) = span {
                        self.collect_type_params(*inner, params, visited);
                    }
                }
            }
            TypeKey::StringIntrinsic { type_arg, .. } => {
                self.collect_type_params(type_arg, params, visited);
            }
            TypeKey::Intrinsic(_)
            | TypeKey::Literal(_)
            | TypeKey::Ref(_)
            | TypeKey::TypeQuery(_)
            | TypeKey::UniqueSymbol(_)
            | TypeKey::ThisType
            | TypeKey::Error => {}
        }
    }

    fn param_depends_on_targets(
        &mut self,
        name: Atom,
        targets: &[Atom],
        visited: &mut FxHashSet<Atom>,
    ) -> bool {
        if targets.contains(&name) {
            return true;
        }
        if !visited.insert(name) {
            return false;
        }
        let Some(var) = self.find_type_param(name) else {
            return false;
        };
        let root = self.table.find(var);
        let upper_bounds = self.constraints[root.0 as usize].upper_bounds.clone();

        for bound in upper_bounds {
            for target in targets {
                let mut seen = FxHashSet::default();
                if self.type_contains_param(bound, *target, &mut seen) {
                    return true;
                }
            }
            if let Some(TypeKey::TypeParameter(info)) = self.interner.lookup(bound)
                && self.param_depends_on_targets(info.name, targets, visited)
            {
                return true;
            }
        }

        false
    }

    fn type_contains_param(
        &self,
        ty: TypeId,
        target: Atom,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if !visited.insert(ty) {
            return false;
        }

        let key = match self.interner.lookup(ty) {
            Some(key) => key,
            None => return false,
        };

        match key {
            TypeKey::TypeParameter(info) => info.name == target,
            TypeKey::Array(elem) => self.type_contains_param(elem, target, visited),
            TypeKey::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                elements
                    .iter()
                    .any(|e| self.type_contains_param(e.type_id, target, visited))
            }
            TypeKey::Union(members) | TypeKey::Intersection(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|&member| self.type_contains_param(member, target, visited))
            }
            TypeKey::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|p| self.type_contains_param(p.type_id, target, visited))
            }
            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|p| self.type_contains_param(p.type_id, target, visited))
                    || shape.string_index.as_ref().is_some_and(|idx| {
                        self.type_contains_param(idx.key_type, target, visited)
                            || self.type_contains_param(idx.value_type, target, visited)
                    })
                    || shape.number_index.as_ref().is_some_and(|idx| {
                        self.type_contains_param(idx.key_type, target, visited)
                            || self.type_contains_param(idx.value_type, target, visited)
                    })
            }
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.type_contains_param(app.base, target, visited)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.type_contains_param(arg, target, visited))
            }
            TypeKey::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                if shape.type_params.iter().any(|tp| tp.name == target) {
                    return false;
                }
                shape
                    .this_type
                    .is_some_and(|this_type| self.type_contains_param(this_type, target, visited))
                    || shape
                        .params
                        .iter()
                        .any(|p| self.type_contains_param(p.type_id, target, visited))
                    || self.type_contains_param(shape.return_type, target, visited)
            }
            TypeKey::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                let in_call = shape.call_signatures.iter().any(|sig| {
                    if sig.type_params.iter().any(|tp| tp.name == target) {
                        false
                    } else {
                        sig.this_type.is_some_and(|this_type| {
                            self.type_contains_param(this_type, target, visited)
                        }) || sig
                            .params
                            .iter()
                            .any(|p| self.type_contains_param(p.type_id, target, visited))
                            || self.type_contains_param(sig.return_type, target, visited)
                    }
                });
                if in_call {
                    return true;
                }
                let in_construct = shape.construct_signatures.iter().any(|sig| {
                    if sig.type_params.iter().any(|tp| tp.name == target) {
                        false
                    } else {
                        sig.this_type.is_some_and(|this_type| {
                            self.type_contains_param(this_type, target, visited)
                        }) || sig
                            .params
                            .iter()
                            .any(|p| self.type_contains_param(p.type_id, target, visited))
                            || self.type_contains_param(sig.return_type, target, visited)
                    }
                });
                if in_construct {
                    return true;
                }
                shape
                    .properties
                    .iter()
                    .any(|p| self.type_contains_param(p.type_id, target, visited))
            }
            TypeKey::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(cond_id);
                self.type_contains_param(cond.check_type, target, visited)
                    || self.type_contains_param(cond.extends_type, target, visited)
                    || self.type_contains_param(cond.true_type, target, visited)
                    || self.type_contains_param(cond.false_type, target, visited)
            }
            TypeKey::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(mapped_id);
                if mapped.type_param.name == target {
                    return false;
                }
                self.type_contains_param(mapped.constraint, target, visited)
                    || self.type_contains_param(mapped.template, target, visited)
            }
            TypeKey::IndexAccess(obj, idx) => {
                self.type_contains_param(obj, target, visited)
                    || self.type_contains_param(idx, target, visited)
            }
            TypeKey::KeyOf(operand) | TypeKey::ReadonlyType(operand) => {
                self.type_contains_param(operand, target, visited)
            }
            TypeKey::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => self.type_contains_param(*inner, target, visited),
                })
            }
            TypeKey::StringIntrinsic { type_arg, .. } => {
                self.type_contains_param(type_arg, target, visited)
            }
            TypeKey::Infer(info) => info.name == target,
            TypeKey::Intrinsic(_)
            | TypeKey::Literal(_)
            | TypeKey::Ref(_)
            | TypeKey::TypeQuery(_)
            | TypeKey::UniqueSymbol(_)
            | TypeKey::ThisType
            | TypeKey::Error => false,
        }
    }

    /// Resolve all type parameters to concrete types
    pub fn resolve_all(&mut self) -> Result<Vec<(Atom, TypeId)>, InferenceError> {
        // Clone type_params to avoid borrow conflict
        let type_params: Vec<_> = self.type_params.clone();
        let mut results = Vec::new();
        for (name, var) in type_params {
            match self.probe(var) {
                Some(ty) => results.push((name, ty)),
                None => return Err(InferenceError::Unresolved(var)),
            }
        }
        Ok(results)
    }

    /// Get the interner reference
    #[allow(dead_code)]
    pub fn interner(&self) -> &dyn TypeDatabase {
        self.interner
    }

    // =========================================================================
    // Constraint Collection
    // =========================================================================

    /// Add a lower bound constraint: ty <: var
    /// This is used when an argument type flows into a type parameter.
    pub fn add_lower_bound(&mut self, var: InferenceVar, ty: TypeId) {
        let root = self.table.find(var);
        self.constraints[root.0 as usize].add_lower_bound(ty);
    }

    /// Add an upper bound constraint: var <: ty
    /// This is used for `extends` constraints on type parameters.
    pub fn add_upper_bound(&mut self, var: InferenceVar, ty: TypeId) {
        let root = self.table.find(var);
        self.constraints[root.0 as usize].add_upper_bound(ty);
    }

    /// Get the constraints for a variable
    pub fn get_constraints(&mut self, var: InferenceVar) -> Option<&ConstraintSet> {
        let root = self.table.find(var);
        let constraints = &self.constraints[root.0 as usize];
        if constraints.is_empty() {
            None
        } else {
            Some(constraints)
        }
    }

    /// Collect a constraint from an assignment: source flows into target
    /// If target is an inference variable, source becomes a lower bound.
    /// If source is an inference variable, target becomes an upper bound.
    pub fn collect_constraint(&mut self, _source: TypeId, _target: TypeId) {
        // Check if target is an inference variable (via TypeKey lookup)
        // For now, we rely on the caller to call add_lower_bound/add_upper_bound directly
        // This is a placeholder for more sophisticated constraint collection
    }

    // =========================================================================
    // Bounds Checking and Resolution
    // =========================================================================

    /// Resolve an inference variable using its collected constraints.
    ///
    /// Algorithm:
    /// 1. If already unified to a concrete type, return that
    /// 2. Otherwise, compute the best common type from lower bounds
    /// 3. Validate against upper bounds
    /// 4. If no lower bounds, use the constraint (upper bound) or default
    pub fn resolve_with_constraints(
        &mut self,
        var: InferenceVar,
    ) -> Result<TypeId, InferenceError> {
        // Check if already resolved
        if let Some(ty) = self.probe(var) {
            return Ok(ty);
        }

        let (root, result, upper_bounds) = self.compute_constraint_result(var);

        // Validate against upper bounds
        for &upper in &upper_bounds {
            if !self.is_subtype(result, upper) {
                return Err(InferenceError::BoundsViolation {
                    var,
                    lower: result,
                    upper,
                });
            }
        }

        if self.occurs_in(root, result) {
            return Err(InferenceError::OccursCheck {
                var: root,
                ty: result,
            });
        }

        // Store the result
        self.table.union_value(root, InferenceValue(Some(result)));

        Ok(result)
    }

    /// Resolve an inference variable using its collected constraints and a custom
    /// assignability check for upper-bound validation.
    pub fn resolve_with_constraints_by<F>(
        &mut self,
        var: InferenceVar,
        mut is_subtype: F,
    ) -> Result<TypeId, InferenceError>
    where
        F: FnMut(TypeId, TypeId) -> bool,
    {
        // Check if already resolved
        if let Some(ty) = self.probe(var) {
            return Ok(ty);
        }

        let (root, result, upper_bounds) = self.compute_constraint_result(var);

        for &upper in &upper_bounds {
            if !is_subtype(result, upper) {
                return Err(InferenceError::BoundsViolation {
                    var,
                    lower: result,
                    upper,
                });
            }
        }

        if self.occurs_in(root, result) {
            return Err(InferenceError::OccursCheck {
                var: root,
                ty: result,
            });
        }

        self.table.union_value(root, InferenceValue(Some(result)));

        Ok(result)
    }

    fn compute_constraint_result(
        &mut self,
        var: InferenceVar,
    ) -> (InferenceVar, TypeId, Vec<TypeId>) {
        let root = self.table.find(var);
        let constraints = self.constraints[root.0 as usize].clone();
        let target_names = self.type_param_names_for_root(root);
        let mut upper_bounds = Vec::new();
        let mut lower_bounds = constraints.lower_bounds;
        for bound in constraints.upper_bounds {
            if self.occurs_in(root, bound) {
                continue;
            }
            if !target_names.is_empty() && self.upper_bound_cycles_param(bound, &target_names) {
                self.expand_cyclic_upper_bound(
                    root,
                    bound,
                    &target_names,
                    &mut lower_bounds,
                    &mut upper_bounds,
                );
                continue;
            }
            if !upper_bounds.contains(&bound) {
                upper_bounds.push(bound);
            }
        }

        if !upper_bounds.is_empty() {
            lower_bounds.retain(|ty| !matches!(*ty, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR));
        }

        let result = if !lower_bounds.is_empty() {
            // Best common type: union of all lower bounds
            self.best_common_type(&lower_bounds)
        } else if !upper_bounds.is_empty() {
            // No lower bounds, use intersection of upper bounds
            if upper_bounds.len() == 1 {
                upper_bounds[0]
            } else {
                self.interner.intersection(upper_bounds.clone())
            }
        } else {
            // No constraints at all - return unknown
            TypeId::UNKNOWN
        };

        (root, result, upper_bounds)
    }

    /// Resolve all type parameters using constraints.
    pub fn resolve_all_with_constraints(&mut self) -> Result<Vec<(Atom, TypeId)>, InferenceError> {
        let type_params: Vec<_> = self.type_params.clone();
        let mut results = Vec::new();

        for (name, var) in type_params {
            let ty = self.resolve_with_constraints(var)?;
            results.push((name, ty));
        }

        Ok(results)
    }

    // =========================================================================
    // Best Common Type
    // =========================================================================

    /// Calculate the best common type from a set of types.
    /// This is the union of all types (widening).
    pub fn best_common_type(&self, types: &[TypeId]) -> TypeId {
        if types.is_empty() {
            return TypeId::UNKNOWN;
        }
        if types.len() == 1 {
            return types[0];
        }

        // Filter out duplicates and special types
        let mut seen = FxHashSet::default();
        let mut unique: Vec<TypeId> = Vec::new();
        for &ty in types {
            if ty == TypeId::NEVER {
                continue; // never doesn't contribute to union
            }
            if seen.insert(ty) {
                unique.push(ty);
            }
        }

        if unique.is_empty() {
            return TypeId::NEVER;
        }
        if unique.len() == 1 {
            return unique[0];
        }

        // Try to find a more specific common type
        // For example, if we have [string, "hello"], the result should be string
        // If we have ["hello", "world"], the result should be the union of both literals,
        // which widens to string

        // First, check if all types are literals of the same primitive type
        let common_base = self.find_common_base_type(&unique);
        if let Some(base) = common_base {
            // All types share a common base type
            // Check if using the base type would be more specific than a union
            if self.all_types_are_narrower_than_base(&unique, base) {
                return base;
            }
        }

        // Try to find the best single type that satisfies all candidates
        // Check if one type is a supertype of all others
        for &candidate in &unique {
            if self.is_suitable_common_type(candidate, &unique) {
                return candidate;
            }
        }

        // Create union of all types
        self.interner.union(unique)
    }

    /// Find a common base type for a set of types.
    /// For example, [string, "hello"] -> Some(string)
    fn find_common_base_type(&self, types: &[TypeId]) -> Option<TypeId> {
        if types.is_empty() {
            return None;
        }

        // Get the base type of the first element
        let first_base = self.get_base_type(types[0])?;

        // Check if all other types have the same base
        for &ty in types.iter().skip(1) {
            let base = self.get_base_type(ty)?;
            if base != first_base {
                return None;
            }
        }

        Some(first_base)
    }

    /// Get the base type of a type (stripping literals, etc.)
    fn get_base_type(&self, ty: TypeId) -> Option<TypeId> {
        match self.interner.lookup(ty) {
            Some(TypeKey::Literal(_)) => {
                // Get the intrinsic type of the literal
                match ty {
                    TypeId::STRING | TypeId::NUMBER | TypeId::BOOLEAN | TypeId::BIGINT => Some(ty),
                    _ => {
                        // For literal values, extract their base type
                        if let Some(TypeKey::Literal(lit)) = self.interner.lookup(ty) {
                            match lit {
                                LiteralValue::String(_) => Some(TypeId::STRING),
                                LiteralValue::Number(_) => Some(TypeId::NUMBER),
                                LiteralValue::Boolean(_) => Some(TypeId::BOOLEAN),
                                LiteralValue::BigInt(_) => Some(TypeId::BIGINT),
                            }
                        } else {
                            Some(ty)
                        }
                    }
                }
            }
            _ => Some(ty),
        }
    }

    /// Check if all types are narrower than (subtypes of) the given base type.
    fn all_types_are_narrower_than_base(&self, types: &[TypeId], base: TypeId) -> bool {
        types.iter().all(|&ty| self.is_subtype(ty, base))
    }

    /// Check if a candidate type is a suitable common type for all types.
    /// A suitable common type must be a supertype of all types in the list.
    fn is_suitable_common_type(&self, candidate: TypeId, types: &[TypeId]) -> bool {
        types.iter().all(|&ty| self.is_subtype(ty, candidate))
    }

    /// Simple subtype check for bounds validation.
    /// Uses a simplified check - for full checking, use SubtypeChecker.
    fn is_subtype(&self, source: TypeId, target: TypeId) -> bool {
        // Same type
        if source == target {
            return true;
        }

        // never <: T for all T
        if source == TypeId::NEVER {
            return true;
        }

        // T <: unknown for all T
        if target == TypeId::UNKNOWN {
            return true;
        }

        // any <: T and T <: any
        if source == TypeId::ANY || target == TypeId::ANY {
            return true;
        }

        // object keyword accepts any non-primitive type
        if target == TypeId::OBJECT {
            return self.is_object_keyword_type(source);
        }

        let source_key = self.interner.lookup(source);
        let target_key = self.interner.lookup(target);

        // Check if source is literal of target intrinsic
        if let Some(TypeKey::Literal(lit)) = source_key.as_ref() {
            match (lit, target) {
                (LiteralValue::String(_), t) if t == TypeId::STRING => return true,
                (LiteralValue::Number(_), t) if t == TypeId::NUMBER => return true,
                (LiteralValue::Boolean(_), t) if t == TypeId::BOOLEAN => return true,
                (LiteralValue::BigInt(_), t) if t == TypeId::BIGINT => return true,
                _ => {}
            }
        }

        // Array and tuple structural checks
        if let (Some(TypeKey::Array(s_elem)), Some(TypeKey::Array(t_elem))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            return self.is_subtype(*s_elem, *t_elem);
        }

        if let (Some(TypeKey::Tuple(s_elems)), Some(TypeKey::Tuple(t_elems))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            let s_elems = self.interner.tuple_list(*s_elems);
            let t_elems = self.interner.tuple_list(*t_elems);
            return self.tuple_subtype_of(&s_elems, &t_elems);
        }

        if let (Some(TypeKey::Tuple(s_elems)), Some(TypeKey::Array(t_elem))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            let s_elems = self.interner.tuple_list(*s_elems);
            return self.tuple_subtype_array(&s_elems, *t_elem);
        }

        if let (Some(TypeKey::Object(s_props)), Some(TypeKey::Object(t_props))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            let s_shape = self.interner.object_shape(*s_props);
            let t_shape = self.interner.object_shape(*t_props);
            return self.object_subtype_of(
                &s_shape.properties,
                Some(*s_props),
                &t_shape.properties,
            );
        }

        if let (
            Some(TypeKey::ObjectWithIndex(s_shape_id)),
            Some(TypeKey::ObjectWithIndex(t_shape_id)),
        ) = (source_key.as_ref(), target_key.as_ref())
        {
            let s_shape = self.interner.object_shape(*s_shape_id);
            let t_shape = self.interner.object_shape(*t_shape_id);
            return self.object_with_index_subtype_of(&s_shape, Some(*s_shape_id), &t_shape);
        }

        if let (Some(TypeKey::Object(s_props)), Some(TypeKey::ObjectWithIndex(t_shape))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            let s_shape = self.interner.object_shape(*s_props);
            let t_shape = self.interner.object_shape(*t_shape);
            return self.object_props_subtype_index(&s_shape.properties, Some(*s_props), &t_shape);
        }

        if let (Some(TypeKey::ObjectWithIndex(s_shape_id)), Some(TypeKey::Object(t_props))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            let s_shape = self.interner.object_shape(*s_shape_id);
            let t_shape = self.interner.object_shape(*t_props);
            return self.object_subtype_of(
                &s_shape.properties,
                Some(*s_shape_id),
                &t_shape.properties,
            );
        }

        if let (Some(TypeKey::Function(s_fn)), Some(TypeKey::Function(t_fn))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            let s_fn = self.interner.function_shape(*s_fn);
            let t_fn = self.interner.function_shape(*t_fn);
            return self.function_subtype_of(&s_fn, &t_fn);
        }

        if let (Some(TypeKey::Callable(s_callable)), Some(TypeKey::Callable(t_callable))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            let s_callable = self.interner.callable_shape(*s_callable);
            let t_callable = self.interner.callable_shape(*t_callable);
            return self.callable_subtype_of(&s_callable, &t_callable);
        }

        if let (Some(TypeKey::Function(s_fn)), Some(TypeKey::Callable(t_callable))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            let s_fn = self.interner.function_shape(*s_fn);
            let t_callable = self.interner.callable_shape(*t_callable);
            return self.function_subtype_callable(&s_fn, &t_callable);
        }

        if let (Some(TypeKey::Callable(s_callable)), Some(TypeKey::Function(t_fn))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            let s_callable = self.interner.callable_shape(*s_callable);
            let t_fn = self.interner.function_shape(*t_fn);
            return self.callable_subtype_function(&s_callable, &t_fn);
        }

        if let (Some(TypeKey::Application(s_app)), Some(TypeKey::Application(t_app))) =
            (source_key.as_ref(), target_key.as_ref())
        {
            let s_app = self.interner.type_application(*s_app);
            let t_app = self.interner.type_application(*t_app);
            if s_app.args.len() != t_app.args.len() {
                return false;
            }
            if !self.is_subtype(s_app.base, t_app.base) {
                return false;
            }
            for (s_arg, t_arg) in s_app.args.iter().zip(t_app.args.iter()) {
                if !self.is_subtype(*s_arg, *t_arg) {
                    return false;
                }
            }
            return true;
        }

        // Intersection: A & B <: T if either member is a subtype of T
        if let Some(TypeKey::Intersection(members)) = source_key.as_ref() {
            let members = self.interner.type_list(*members);
            return members
                .iter()
                .any(|&member| self.is_subtype(member, target));
        }

        // Union: A | B <: T if both A <: T and B <: T
        if let Some(TypeKey::Union(members)) = source_key.as_ref() {
            let members = self.interner.type_list(*members);
            return members
                .iter()
                .all(|&member| self.is_subtype(member, target));
        }

        // Target intersection: S <: (A & B) if S <: A and S <: B
        if let Some(TypeKey::Intersection(members)) = target_key.as_ref() {
            let members = self.interner.type_list(*members);
            return members
                .iter()
                .all(|&member| self.is_subtype(source, member));
        }

        // Target union: S <: (A | B) if S <: A or S <: B
        if let Some(TypeKey::Union(members)) = target_key.as_ref() {
            let members = self.interner.type_list(*members);
            return members
                .iter()
                .any(|&member| self.is_subtype(source, member));
        }

        false
    }

    fn is_object_keyword_type(&self, source: TypeId) -> bool {
        match source {
            TypeId::ANY | TypeId::NEVER | TypeId::ERROR | TypeId::OBJECT => return true,
            TypeId::UNKNOWN
            | TypeId::VOID
            | TypeId::NULL
            | TypeId::UNDEFINED
            | TypeId::BOOLEAN
            | TypeId::NUMBER
            | TypeId::STRING
            | TypeId::BIGINT
            | TypeId::SYMBOL => return false,
            _ => {}
        }

        let key = match self.interner.lookup(source) {
            Some(key) => key,
            None => return false,
        };

        match key {
            TypeKey::Object(_)
            | TypeKey::ObjectWithIndex(_)
            | TypeKey::Array(_)
            | TypeKey::Tuple(_)
            | TypeKey::Function(_)
            | TypeKey::Callable(_)
            | TypeKey::Mapped(_)
            | TypeKey::Application(_)
            | TypeKey::ThisType => true,
            TypeKey::ReadonlyType(inner) => self.is_subtype(inner, TypeId::OBJECT),
            TypeKey::TypeParameter(info) | TypeKey::Infer(info) => info
                .constraint
                .is_some_and(|constraint| self.is_subtype(constraint, TypeId::OBJECT)),
            _ => false,
        }
    }

    fn optional_property_type(&self, prop: &PropertyInfo) -> TypeId {
        if prop.optional {
            self.interner.union2(prop.type_id, TypeId::UNDEFINED)
        } else {
            prop.type_id
        }
    }

    fn optional_property_write_type(&self, prop: &PropertyInfo) -> TypeId {
        if prop.optional {
            self.interner.union2(prop.write_type, TypeId::UNDEFINED)
        } else {
            prop.write_type
        }
    }

    fn is_subtype_with_method_variance(
        &self,
        source: TypeId,
        target: TypeId,
        allow_bivariant: bool,
    ) -> bool {
        if !allow_bivariant {
            return self.is_subtype(source, target);
        }

        let source_key = self.interner.lookup(source);
        let target_key = self.interner.lookup(target);

        match (source_key.as_ref(), target_key.as_ref()) {
            (Some(TypeKey::Function(s_fn)), Some(TypeKey::Function(t_fn))) => {
                let s_fn = self.interner.function_shape(*s_fn);
                let t_fn = self.interner.function_shape(*t_fn);
                return self.function_like_subtype_of_with_variance(
                    &s_fn.params,
                    s_fn.return_type,
                    &t_fn.params,
                    t_fn.return_type,
                    true,
                );
            }
            (Some(TypeKey::Callable(s_callable)), Some(TypeKey::Callable(t_callable))) => {
                let s_callable = self.interner.callable_shape(*s_callable);
                let t_callable = self.interner.callable_shape(*t_callable);
                return self.callable_subtype_of_with_variance(&s_callable, &t_callable, true);
            }
            (Some(TypeKey::Function(s_fn)), Some(TypeKey::Callable(t_callable))) => {
                let s_fn = self.interner.function_shape(*s_fn);
                let t_callable = self.interner.callable_shape(*t_callable);
                return self.function_subtype_callable_with_variance(&s_fn, &t_callable, true);
            }
            (Some(TypeKey::Callable(s_callable)), Some(TypeKey::Function(t_fn))) => {
                let s_callable = self.interner.callable_shape(*s_callable);
                let t_fn = self.interner.function_shape(*t_fn);
                return self.callable_subtype_function_with_variance(&s_callable, &t_fn, true);
            }
            _ => {}
        }

        self.is_subtype(source, target)
    }

    fn lookup_property<'props>(
        &self,
        props: &'props [PropertyInfo],
        shape_id: Option<ObjectShapeId>,
        name: Atom,
    ) -> Option<&'props PropertyInfo> {
        if let Some(shape_id) = shape_id {
            match self.interner.object_property_index(shape_id, name) {
                PropertyLookup::Found(idx) => return props.get(idx),
                PropertyLookup::NotFound => return None,
                PropertyLookup::Uncached => {}
            }
        }
        props.iter().find(|p| p.name == name)
    }

    fn object_subtype_of(
        &self,
        source: &[PropertyInfo],
        source_shape_id: Option<ObjectShapeId>,
        target: &[PropertyInfo],
    ) -> bool {
        for t_prop in target {
            let s_prop = self.lookup_property(source, source_shape_id, t_prop.name);
            match s_prop {
                Some(sp) => {
                    if sp.optional && !t_prop.optional {
                        return false;
                    }
                    if sp.readonly && !t_prop.readonly {
                        return false;
                    }
                    let source_type = self.optional_property_type(sp);
                    let target_type = self.optional_property_type(t_prop);
                    if !self.is_subtype_with_method_variance(
                        source_type,
                        target_type,
                        t_prop.is_method,
                    ) {
                        return false;
                    }
                    if !t_prop.readonly
                        && (sp.write_type != sp.type_id || t_prop.write_type != t_prop.type_id)
                    {
                        let source_write = self.optional_property_write_type(sp);
                        let target_write = self.optional_property_write_type(t_prop);
                        if !self.is_subtype_with_method_variance(
                            target_write,
                            source_write,
                            t_prop.is_method,
                        ) {
                            return false;
                        }
                    }
                }
                None => {
                    if !t_prop.optional {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn object_props_subtype_index(
        &self,
        source: &[PropertyInfo],
        source_shape_id: Option<ObjectShapeId>,
        target: &ObjectShape,
    ) -> bool {
        if !self.object_subtype_of(source, source_shape_id, &target.properties) {
            return false;
        }
        self.check_properties_against_index_signatures(source, target)
    }

    fn object_with_index_subtype_of(
        &self,
        source: &ObjectShape,
        source_shape_id: Option<ObjectShapeId>,
        target: &ObjectShape,
    ) -> bool {
        if !self.object_subtype_of(&source.properties, source_shape_id, &target.properties) {
            return false;
        }

        if let Some(t_string_idx) = &target.string_index
            && let Some(s_string_idx) = &source.string_index
        {
            if s_string_idx.readonly && !t_string_idx.readonly {
                return false;
            }
            if !self.is_subtype(s_string_idx.value_type, t_string_idx.value_type) {
                return false;
            }
        }

        if let Some(t_number_idx) = &target.number_index
            && let Some(s_number_idx) = &source.number_index
        {
            if s_number_idx.readonly && !t_number_idx.readonly {
                return false;
            }
            if !self.is_subtype(s_number_idx.value_type, t_number_idx.value_type) {
                return false;
            }
        }

        if let (Some(s_string_idx), Some(s_number_idx)) =
            (&source.string_index, &source.number_index)
            && !self.is_subtype(s_number_idx.value_type, s_string_idx.value_type)
        {
            return false;
        }

        self.check_properties_against_index_signatures(&source.properties, target)
    }

    fn check_properties_against_index_signatures(
        &self,
        source: &[PropertyInfo],
        target: &ObjectShape,
    ) -> bool {
        let string_index = target.string_index.as_ref();
        let number_index = target.number_index.as_ref();

        if string_index.is_none() && number_index.is_none() {
            return true;
        }

        for prop in source {
            let prop_type = self.optional_property_type(prop);

            if let Some(number_idx) = number_index
                && self.is_numeric_property_name(prop.name)
            {
                if !number_idx.readonly && prop.readonly {
                    return false;
                }
                if !self.is_subtype(prop_type, number_idx.value_type) {
                    return false;
                }
            }

            if let Some(string_idx) = string_index {
                if !string_idx.readonly && prop.readonly {
                    return false;
                }
                if !self.is_subtype(prop_type, string_idx.value_type) {
                    return false;
                }
            }
        }

        true
    }

    fn rest_element_type(&self, type_id: TypeId) -> TypeId {
        if type_id == TypeId::ANY {
            return TypeId::ANY;
        }
        match self.interner.lookup(type_id) {
            Some(TypeKey::Array(elem)) => elem,
            _ => type_id,
        }
    }

    fn are_parameters_compatible(&self, source: TypeId, target: TypeId, bivariant: bool) -> bool {
        if bivariant {
            self.is_subtype(target, source) || self.is_subtype(source, target)
        } else {
            self.is_subtype(target, source)
        }
    }

    fn are_this_parameters_compatible(
        &self,
        source: Option<TypeId>,
        target: Option<TypeId>,
        bivariant: bool,
    ) -> bool {
        if source.is_none() && target.is_none() {
            return true;
        }
        // Use Unknown instead of Any for stricter type checking
        // When this parameter type is not specified, we should not allow any value
        let source = source.unwrap_or(TypeId::UNKNOWN);
        let target = target.unwrap_or(TypeId::UNKNOWN);
        self.are_parameters_compatible(source, target, bivariant)
    }

    fn is_numeric_property_name(&self, name: Atom) -> bool {
        let prop_name = self.interner.resolve_atom_ref(name);
        Self::is_numeric_literal_name(prop_name.as_ref())
    }

    pub(crate) fn is_numeric_literal_name(name: &str) -> bool {
        if name == "NaN" || name == "Infinity" || name == "-Infinity" {
            return true;
        }

        let value: f64 = match name.parse() {
            Ok(value) => value,
            Err(_) => return false,
        };
        if !value.is_finite() {
            return false;
        }

        Self::js_number_to_string(value) == name
    }

    fn js_number_to_string(value: f64) -> String {
        if value.is_nan() {
            return "NaN".to_string();
        }
        if value == 0.0 {
            return "0".to_string();
        }
        if value.is_infinite() {
            return if value.is_sign_negative() {
                "-Infinity".to_string()
            } else {
                "Infinity".to_string()
            };
        }

        let abs = value.abs();
        if !(1e-6..1e21).contains(&abs) {
            let mut formatted = format!("{:e}", value);
            if let Some(split) = formatted.find('e') {
                let (mantissa, exp) = formatted.split_at(split);
                let exp_digits = &exp[1..];
                let (sign, digits) = if exp_digits.starts_with('-') {
                    ('-', &exp_digits[1..])
                } else {
                    ('+', exp_digits)
                };
                let trimmed = digits.trim_start_matches('0');
                let digits = if trimmed.is_empty() { "0" } else { trimmed };
                formatted = format!("{mantissa}e{sign}{digits}");
            }
            return formatted;
        }

        let formatted = value.to_string();
        if formatted == "-0" {
            "0".to_string()
        } else {
            formatted
        }
    }

    fn function_like_subtype_of(
        &self,
        source_params: &[ParamInfo],
        source_return: TypeId,
        target_params: &[ParamInfo],
        target_return: TypeId,
    ) -> bool {
        self.function_like_subtype_of_with_variance(
            source_params,
            source_return,
            target_params,
            target_return,
            false,
        )
    }

    fn function_like_subtype_of_with_variance(
        &self,
        source_params: &[ParamInfo],
        source_return: TypeId,
        target_params: &[ParamInfo],
        target_return: TypeId,
        bivariant: bool,
    ) -> bool {
        if !self.is_subtype(source_return, target_return) {
            return false;
        }

        let target_has_rest = target_params.last().is_some_and(|p| p.rest);
        let source_has_rest = source_params.last().is_some_and(|p| p.rest);
        let target_fixed = if target_has_rest {
            target_params.len().saturating_sub(1)
        } else {
            target_params.len()
        };
        let source_fixed = if source_has_rest {
            source_params.len().saturating_sub(1)
        } else {
            source_params.len()
        };

        if !target_has_rest && source_params.len() > target_params.len() {
            return false;
        }

        let fixed_compare = std::cmp::min(source_fixed, target_fixed);
        for i in 0..fixed_compare {
            let s_param = &source_params[i];
            let t_param = &target_params[i];
            if !self.are_parameters_compatible(s_param.type_id, t_param.type_id, bivariant) {
                return false;
            }
        }

        if target_has_rest {
            let rest_param = match target_params.last() {
                Some(param) => param,
                None => return false,
            };
            let rest_elem = self.rest_element_type(rest_param.type_id);

            for i in target_fixed..source_fixed {
                let s_param = &source_params[i];
                if !self.are_parameters_compatible(s_param.type_id, rest_elem, bivariant) {
                    return false;
                }
            }

            if source_has_rest {
                let s_rest = match source_params.last() {
                    Some(param) => param,
                    None => return false,
                };
                let s_rest_elem = self.rest_element_type(s_rest.type_id);
                if !self.are_parameters_compatible(s_rest_elem, rest_elem, bivariant) {
                    return false;
                }
            }
        }

        true
    }

    fn function_subtype_of(&self, source: &FunctionShape, target: &FunctionShape) -> bool {
        if source.is_constructor != target.is_constructor {
            return false;
        }
        if !self.are_this_parameters_compatible(source.this_type, target.this_type, false) {
            return false;
        }

        self.function_like_subtype_of(
            &source.params,
            source.return_type,
            &target.params,
            target.return_type,
        )
    }

    fn call_signature_subtype_of(
        &self,
        source: &CallSignature,
        target: &CallSignature,
        bivariant: bool,
    ) -> bool {
        if !self.are_this_parameters_compatible(source.this_type, target.this_type, bivariant) {
            return false;
        }
        self.function_like_subtype_of_with_variance(
            &source.params,
            source.return_type,
            &target.params,
            target.return_type,
            bivariant,
        )
    }

    fn callable_subtype_of(&self, source: &CallableShape, target: &CallableShape) -> bool {
        self.callable_subtype_of_with_variance(source, target, false)
    }

    fn callable_subtype_of_with_variance(
        &self,
        source: &CallableShape,
        target: &CallableShape,
        bivariant: bool,
    ) -> bool {
        for t_sig in &target.call_signatures {
            let mut found = false;
            for s_sig in &source.call_signatures {
                if self.call_signature_subtype_of(s_sig, t_sig, bivariant) {
                    found = true;
                    break;
                }
            }
            if !found {
                return false;
            }
        }

        for t_sig in &target.construct_signatures {
            let mut found = false;
            for s_sig in &source.construct_signatures {
                if self.call_signature_subtype_of(s_sig, t_sig, bivariant) {
                    found = true;
                    break;
                }
            }
            if !found {
                return false;
            }
        }

        self.object_subtype_of(&source.properties, None, &target.properties)
    }

    fn function_subtype_callable(&self, source: &FunctionShape, target: &CallableShape) -> bool {
        self.function_subtype_callable_with_variance(source, target, false)
    }

    fn function_subtype_callable_with_variance(
        &self,
        source: &FunctionShape,
        target: &CallableShape,
        bivariant: bool,
    ) -> bool {
        for t_sig in &target.call_signatures {
            if !self.function_like_subtype_of_with_variance(
                &source.params,
                source.return_type,
                &t_sig.params,
                t_sig.return_type,
                bivariant,
            ) {
                return false;
            }
        }
        true
    }

    fn callable_subtype_function(&self, source: &CallableShape, target: &FunctionShape) -> bool {
        self.callable_subtype_function_with_variance(source, target, false)
    }

    fn callable_subtype_function_with_variance(
        &self,
        source: &CallableShape,
        target: &FunctionShape,
        bivariant: bool,
    ) -> bool {
        for s_sig in &source.call_signatures {
            if self.function_like_subtype_of_with_variance(
                &s_sig.params,
                s_sig.return_type,
                &target.params,
                target.return_type,
                bivariant,
            ) {
                return true;
            }
        }
        false
    }

    fn tuple_subtype_array(&self, source: &[TupleElement], target_elem: TypeId) -> bool {
        for elem in source {
            if elem.rest {
                let expansion = self.expand_tuple_rest(elem.type_id);
                for fixed in expansion.fixed {
                    if !self.is_subtype(fixed.type_id, target_elem) {
                        return false;
                    }
                }
                if let Some(variadic) = expansion.variadic
                    && !self.is_subtype(variadic, target_elem)
                {
                    return false;
                }
            } else if !self.is_subtype(elem.type_id, target_elem) {
                return false;
            }
        }
        true
    }

    fn tuple_subtype_of(&self, source: &[TupleElement], target: &[TupleElement]) -> bool {
        let source_required = source.iter().filter(|e| !e.optional && !e.rest).count();
        let target_required = target.iter().filter(|e| !e.optional && !e.rest).count();

        if source_required < target_required {
            return false;
        }

        for (i, t_elem) in target.iter().enumerate() {
            if t_elem.rest {
                let expansion = self.expand_tuple_rest(t_elem.type_id);
                let mut source_iter = source.iter().skip(i);

                for t_fixed in &expansion.fixed {
                    match source_iter.next() {
                        Some(s_elem) => {
                            if s_elem.rest {
                                return false;
                            }
                            if !self.is_subtype(s_elem.type_id, t_fixed.type_id) {
                                return false;
                            }
                        }
                        None => {
                            if !t_fixed.optional {
                                return false;
                            }
                        }
                    }
                }

                if let Some(variadic) = expansion.variadic {
                    let variadic_array = self.interner.array(variadic);
                    for s_elem in source_iter {
                        if s_elem.rest {
                            if !self.is_subtype(s_elem.type_id, variadic_array) {
                                return false;
                            }
                        } else if !self.is_subtype(s_elem.type_id, variadic) {
                            return false;
                        }
                    }
                    return true;
                }

                if source_iter.next().is_some() {
                    return false;
                }
                return true;
            }

            if let Some(s_elem) = source.get(i) {
                if s_elem.rest {
                    return false;
                }
                if !self.is_subtype(s_elem.type_id, t_elem.type_id) {
                    return false;
                }
            } else if !t_elem.optional {
                return false;
            }
        }

        if source.len() > target.len() {
            return false;
        }

        if source.iter().any(|elem| elem.rest) {
            return false;
        }

        true
    }

    fn expand_tuple_rest(&self, type_id: TypeId) -> TupleRestExpansion {
        match self.interner.lookup(type_id) {
            Some(TypeKey::Array(elem)) => TupleRestExpansion {
                fixed: Vec::new(),
                variadic: Some(elem),
            },
            Some(TypeKey::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                let mut fixed = Vec::new();
                for elem in elements.iter() {
                    if elem.rest {
                        let inner = self.expand_tuple_rest(elem.type_id);
                        fixed.extend(inner.fixed);
                        return TupleRestExpansion {
                            fixed,
                            variadic: inner.variadic,
                        };
                    }
                    fixed.push(elem.clone());
                }
                TupleRestExpansion {
                    fixed,
                    variadic: None,
                }
            }
            _ => TupleRestExpansion {
                fixed: Vec::new(),
                variadic: Some(type_id),
            },
        }
    }

    // =========================================================================
    // Conditional Type Inference
    // =========================================================================

    /// Infer type parameters from a conditional type.
    /// When a type parameter appears in a conditional type, we can sometimes
    /// infer its value from the check and extends clauses.
    pub fn infer_from_conditional(
        &mut self,
        var: InferenceVar,
        check_type: TypeId,
        extends_type: TypeId,
        true_type: TypeId,
        false_type: TypeId,
    ) {
        // If check_type is an inference variable, try to infer from extends_type
        if let Some(TypeKey::TypeParameter(info)) = self.interner.lookup(check_type)
            && let Some(check_var) = self.find_type_param(info.name)
            && check_var == self.table.find(var)
        {
            // check_type is this variable
            // Try to infer from extends_type as an upper bound
            self.add_upper_bound(var, extends_type);
        }

        // Recursively infer from true/false branches
        self.infer_from_type(var, true_type);
        self.infer_from_type(var, false_type);
    }

    /// Infer type parameters from a type by traversing its structure.
    fn infer_from_type(&mut self, var: InferenceVar, ty: TypeId) {
        let root = self.table.find(var);

        // Check if this type contains the inference variable
        if !self.contains_inference_var(ty, root) {
            return;
        }

        match self.interner.lookup(ty) {
            Some(TypeKey::TypeParameter(info)) => {
                if let Some(param_var) = self.find_type_param(info.name)
                    && self.table.find(param_var) == root
                {
                    // This type is the inference variable itself
                    // Extract bounds from constraint if present
                    if let Some(constraint) = info.constraint {
                        self.add_upper_bound(var, constraint);
                    }
                }
            }
            Some(TypeKey::Array(elem)) => {
                self.infer_from_type(var, elem);
            }
            Some(TypeKey::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                for elem in elements.iter() {
                    self.infer_from_type(var, elem.type_id);
                }
            }
            Some(TypeKey::Union(members)) | Some(TypeKey::Intersection(members)) => {
                let members = self.interner.type_list(members);
                for &member in members.iter() {
                    self.infer_from_type(var, member);
                }
            }
            Some(TypeKey::Object(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in shape.properties.iter() {
                    self.infer_from_type(var, prop.type_id);
                }
            }
            Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in shape.properties.iter() {
                    self.infer_from_type(var, prop.type_id);
                }
                if let Some(index) = shape.string_index.as_ref() {
                    self.infer_from_type(var, index.key_type);
                    self.infer_from_type(var, index.value_type);
                }
                if let Some(index) = shape.number_index.as_ref() {
                    self.infer_from_type(var, index.key_type);
                    self.infer_from_type(var, index.value_type);
                }
            }
            Some(TypeKey::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                self.infer_from_type(var, app.base);
                for &arg in app.args.iter() {
                    self.infer_from_type(var, arg);
                }
            }
            Some(TypeKey::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                for param in shape.params.iter() {
                    self.infer_from_type(var, param.type_id);
                }
                if let Some(this_type) = shape.this_type {
                    self.infer_from_type(var, this_type);
                }
                self.infer_from_type(var, shape.return_type);
            }
            Some(TypeKey::Conditional(cond_id)) => {
                let cond = self.interner.conditional_type(cond_id);
                self.infer_from_conditional(
                    var,
                    cond.check_type,
                    cond.extends_type,
                    cond.true_type,
                    cond.false_type,
                );
            }
            _ => {}
        }
    }

    /// Check if a type contains an inference variable.
    pub(crate) fn contains_inference_var(&mut self, ty: TypeId, var: InferenceVar) -> bool {
        let root = self.table.find(var);

        match self.interner.lookup(ty) {
            Some(TypeKey::TypeParameter(info)) => {
                if let Some(param_var) = self.find_type_param(info.name) {
                    self.table.find(param_var) == root
                } else {
                    false
                }
            }
            Some(TypeKey::Array(elem)) => self.contains_inference_var(elem, var),
            Some(TypeKey::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                elements
                    .iter()
                    .any(|e| self.contains_inference_var(e.type_id, var))
            }
            Some(TypeKey::Union(members)) | Some(TypeKey::Intersection(members)) => {
                let members = self.interner.type_list(members);
                members.iter().any(|&m| self.contains_inference_var(m, var))
            }
            Some(TypeKey::Object(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|p| self.contains_inference_var(p.type_id, var))
            }
            Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|p| self.contains_inference_var(p.type_id, var))
                    || shape.string_index.as_ref().is_some_and(|idx| {
                        self.contains_inference_var(idx.key_type, var)
                            || self.contains_inference_var(idx.value_type, var)
                    })
                    || shape.number_index.as_ref().is_some_and(|idx| {
                        self.contains_inference_var(idx.key_type, var)
                            || self.contains_inference_var(idx.value_type, var)
                    })
            }
            Some(TypeKey::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                self.contains_inference_var(app.base, var)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.contains_inference_var(arg, var))
            }
            Some(TypeKey::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                shape
                    .params
                    .iter()
                    .any(|p| self.contains_inference_var(p.type_id, var))
                    || shape
                        .this_type
                        .is_some_and(|t| self.contains_inference_var(t, var))
                    || self.contains_inference_var(shape.return_type, var)
            }
            Some(TypeKey::Conditional(cond_id)) => {
                let cond = self.interner.conditional_type(cond_id);
                self.contains_inference_var(cond.check_type, var)
                    || self.contains_inference_var(cond.extends_type, var)
                    || self.contains_inference_var(cond.true_type, var)
                    || self.contains_inference_var(cond.false_type, var)
            }
            _ => false,
        }
    }

    // =========================================================================
    // Variance Inference
    // =========================================================================

    /// Compute the variance of a type parameter within a type.
    /// Returns (covariant_count, contravariant_count, invariant_count, bivariant_count)
    pub fn compute_variance(&self, ty: TypeId, target_param: Atom) -> (u32, u32, u32, u32) {
        let mut covariant = 0u32;
        let mut contravariant = 0u32;
        let mut invariant = 0u32;
        let mut bivariant = 0u32;

        self.compute_variance_helper(
            ty,
            target_param,
            true,
            &mut covariant,
            &mut contravariant,
            &mut invariant,
            &mut bivariant,
        );

        (covariant, contravariant, invariant, bivariant)
    }

    fn compute_variance_helper(
        &self,
        ty: TypeId,
        target_param: Atom,
        polarity: bool, // true = covariant, false = contravariant
        covariant: &mut u32,
        contravariant: &mut u32,
        invariant: &mut u32,
        bivariant: &mut u32,
    ) {
        match self.interner.lookup(ty) {
            Some(TypeKey::TypeParameter(info)) if info.name == target_param => {
                if polarity {
                    *covariant += 1;
                } else {
                    *contravariant += 1;
                }
            }
            Some(TypeKey::Array(elem)) => {
                self.compute_variance_helper(
                    elem,
                    target_param,
                    polarity,
                    covariant,
                    contravariant,
                    invariant,
                    bivariant,
                );
            }
            Some(TypeKey::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                for elem in elements.iter() {
                    self.compute_variance_helper(
                        elem.type_id,
                        target_param,
                        polarity,
                        covariant,
                        contravariant,
                        invariant,
                        bivariant,
                    );
                }
            }
            Some(TypeKey::Union(members)) | Some(TypeKey::Intersection(members)) => {
                let members = self.interner.type_list(members);
                for &member in members.iter() {
                    self.compute_variance_helper(
                        member,
                        target_param,
                        polarity,
                        covariant,
                        contravariant,
                        invariant,
                        bivariant,
                    );
                }
            }
            Some(TypeKey::Object(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in shape.properties.iter() {
                    // Properties are covariant in their type (read position)
                    self.compute_variance_helper(
                        prop.type_id,
                        target_param,
                        polarity,
                        covariant,
                        contravariant,
                        invariant,
                        bivariant,
                    );
                    // Properties are contravariant in their write type (write position)
                    if prop.write_type != prop.type_id && !prop.readonly {
                        self.compute_variance_helper(
                            prop.write_type,
                            target_param,
                            !polarity,
                            covariant,
                            contravariant,
                            invariant,
                            bivariant,
                        );
                    }
                }
            }
            Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in shape.properties.iter() {
                    self.compute_variance_helper(
                        prop.type_id,
                        target_param,
                        polarity,
                        covariant,
                        contravariant,
                        invariant,
                        bivariant,
                    );
                    if prop.write_type != prop.type_id && !prop.readonly {
                        self.compute_variance_helper(
                            prop.write_type,
                            target_param,
                            !polarity,
                            covariant,
                            contravariant,
                            invariant,
                            bivariant,
                        );
                    }
                }
                if let Some(index) = shape.string_index.as_ref() {
                    self.compute_variance_helper(
                        index.value_type,
                        target_param,
                        polarity,
                        covariant,
                        contravariant,
                        invariant,
                        bivariant,
                    );
                }
                if let Some(index) = shape.number_index.as_ref() {
                    self.compute_variance_helper(
                        index.value_type,
                        target_param,
                        polarity,
                        covariant,
                        contravariant,
                        invariant,
                        bivariant,
                    );
                }
            }
            Some(TypeKey::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                // Variance depends on the generic type definition
                // For now, assume covariant for all type arguments
                for &arg in app.args.iter() {
                    self.compute_variance_helper(
                        arg,
                        target_param,
                        polarity,
                        covariant,
                        contravariant,
                        invariant,
                        bivariant,
                    );
                }
            }
            Some(TypeKey::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                // Parameters are contravariant
                for param in shape.params.iter() {
                    self.compute_variance_helper(
                        param.type_id,
                        target_param,
                        !polarity,
                        covariant,
                        contravariant,
                        invariant,
                        bivariant,
                    );
                }
                // Return type is covariant
                self.compute_variance_helper(
                    shape.return_type,
                    target_param,
                    polarity,
                    covariant,
                    contravariant,
                    invariant,
                    bivariant,
                );
            }
            Some(TypeKey::Conditional(cond_id)) => {
                let cond = self.interner.conditional_type(cond_id);
                // Conditional types are invariant in their type parameters
                self.compute_variance_helper(
                    cond.check_type,
                    target_param,
                    false,
                    covariant,
                    contravariant,
                    invariant,
                    bivariant,
                );
                self.compute_variance_helper(
                    cond.extends_type,
                    target_param,
                    false,
                    covariant,
                    contravariant,
                    invariant,
                    bivariant,
                );
                // But can be either in the result
                self.compute_variance_helper(
                    cond.true_type,
                    target_param,
                    polarity,
                    covariant,
                    contravariant,
                    invariant,
                    bivariant,
                );
                self.compute_variance_helper(
                    cond.false_type,
                    target_param,
                    polarity,
                    covariant,
                    contravariant,
                    invariant,
                    bivariant,
                );
            }
            _ => {}
        }
    }

    /// Check if a type parameter is invariant at a given position.
    pub fn is_invariant_position(&self, ty: TypeId, target_param: Atom) -> bool {
        let (_, _, invariant, _) = self.compute_variance(ty, target_param);
        invariant > 0
    }

    /// Check if a type parameter is bivariant at a given position.
    pub fn is_bivariant_position(&self, ty: TypeId, target_param: Atom) -> bool {
        let (_, _, _, bivariant) = self.compute_variance(ty, target_param);
        bivariant > 0
    }

    /// Get the variance of a type parameter as a string.
    pub fn get_variance(&self, ty: TypeId, target_param: Atom) -> &'static str {
        let (covariant, contravariant, invariant, bivariant) =
            self.compute_variance(ty, target_param);

        if invariant > 0 {
            "invariant"
        } else if bivariant > 0 {
            "bivariant"
        } else if covariant > 0 && contravariant > 0 {
            "invariant" // Both covariant and contravariant means invariant
        } else if covariant > 0 {
            "covariant"
        } else if contravariant > 0 {
            "contravariant"
        } else {
            "unused"
        }
    }

    // =========================================================================
    // Enhanced Constraint Resolution
    // =========================================================================

    /// Try to infer a type parameter from its usage context.
    /// This implements bidirectional type inference where the context
    /// (e.g., return type, variable declaration) provides constraints.
    pub fn infer_from_context(
        &mut self,
        var: InferenceVar,
        context_type: TypeId,
    ) -> Result<(), InferenceError> {
        // Add context as an upper bound
        self.add_upper_bound(var, context_type);

        // If the context type contains this inference variable,
        // we need to solve more carefully
        let root = self.table.find(var);
        if self.contains_inference_var(context_type, root) {
            // Context contains the inference variable itself
            // This is a recursive type - we need to handle it specially
            return Err(InferenceError::OccursCheck {
                var: root,
                ty: context_type,
            });
        }

        Ok(())
    }

    /// Strengthen constraints by analyzing relationships between type parameters.
    /// For example, if T <: U and we know T = string, then U must be at least string.
    pub fn strengthen_constraints(&mut self) -> Result<(), InferenceError> {
        let type_params: Vec<_> = self.type_params.clone();

        // Iterate multiple times to propagate constraints
        for _ in 0..type_params.len() {
            for (name, var) in type_params.iter() {
                let root = self.table.find(*var);
                let constraints = self.constraints[root.0 as usize].clone();

                // Propagate lower bounds to other type parameters
                for &lower in &constraints.lower_bounds {
                    self.propagate_lower_bound(root, lower, *name);
                }

                // Propagate upper bounds to other type parameters
                for &upper in &constraints.upper_bounds {
                    self.propagate_upper_bound(root, upper, *name);
                }
            }
        }

        Ok(())
    }

    fn propagate_lower_bound(&mut self, var: InferenceVar, lower: TypeId, exclude_param: Atom) {
        if let Some(TypeKey::TypeParameter(info)) = self.interner.lookup(lower)
            && info.name != exclude_param
            && let Some(lower_var) = self.find_type_param(info.name)
        {
            let lower_root = self.table.find(lower_var);
            let lower_constraints = self.constraints[lower_root.0 as usize].clone();

            // Add all upper bounds of the lower param as our upper bounds
            for &upper in &lower_constraints.upper_bounds {
                self.add_upper_bound(var, upper);
            }
        }
    }

    fn propagate_upper_bound(&mut self, var: InferenceVar, upper: TypeId, exclude_param: Atom) {
        if let Some(TypeKey::TypeParameter(info)) = self.interner.lookup(upper)
            && info.name != exclude_param
            && let Some(upper_var) = self.find_type_param(info.name)
        {
            let upper_root = self.table.find(upper_var);
            let upper_constraints = self.constraints[upper_root.0 as usize].clone();

            // Add all lower bounds of the upper param as our lower bounds
            for &lower in &upper_constraints.lower_bounds {
                self.add_lower_bound(var, lower);
            }
        }
    }

    /// Validate that resolved types respect variance constraints.
    pub fn validate_variance(&mut self) -> Result<(), InferenceError> {
        let type_params: Vec<_> = self.type_params.clone();
        for (_name, var) in type_params.iter() {
            let resolved = match self.probe(*var) {
                Some(ty) => ty,
                None => continue,
            };

            // Check if this type parameter appears in its own resolved type
            // We use the occurs_in method which already exists and handles this
            if self.occurs_in(*var, resolved) {
                let root = self.table.find(*var);
                // This would be a circular reference
                return Err(InferenceError::OccursCheck {
                    var: root,
                    ty: resolved,
                });
            }

            // For more advanced variance checking, we would need to know
            // the declared variance of each type parameter in its generic type
            // This is a placeholder for future enhancement
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "infer_tests.rs"]
mod tests;
