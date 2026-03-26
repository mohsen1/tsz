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

use crate::TypeDatabase;
#[cfg(test)]
use crate::types::*;
use crate::types::{InferencePriority, TemplateSpan, TypeData, TypeId};
use crate::visitor::is_literal_type;
use ena::unify::{InPlaceUnificationTable, NoError, UnifyKey, UnifyValue};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use tsz_common::interner::Atom;

/// Helper function to extend a vector with deduplicated items.
/// Uses a `HashSet` for O(1) lookups instead of O(n) contains checks.
fn extend_dedup<T>(target: &mut Vec<T>, items: &[T])
where
    T: Copy + Eq + std::hash::Hash,
{
    if items.is_empty() {
        return;
    }

    // Hot path for inference: most merges add a single item.
    // Avoid allocating/hash-building a set for that case.
    if items.len() == 1 {
        let item = &items[0];
        if !target.contains(item) {
            target.push(*item);
        }
        return;
    }

    let mut existing: FxHashSet<_> = target.iter().copied().collect();
    for item in items {
        if existing.insert(*item) {
            target.push(*item);
        }
    }
}

/// An inference variable representing an unknown type.
/// These are created when instantiating generic functions.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct InferenceVar(pub(crate) u32);

// Uses TypeScript-standard InferencePriority from types.rs

/// A candidate type for an inference variable.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct InferenceCandidate {
    pub(crate) type_id: TypeId,
    pub(crate) priority: InferencePriority,
    pub(crate) is_fresh_literal: bool,
    pub(crate) from_object_property: bool,
    pub from_index_signature: bool,
    pub object_property_index: Option<u32>,
    pub object_property_name: Option<Atom>,
}

#[derive(Copy, Clone, Debug, Default)]
struct CandidateContext {
    from_object_property: bool,
    from_index_signature: bool,
    object_property_index: Option<u32>,
    object_property_name: Option<Atom>,
    source_is_fresh: bool,
}

/// Value stored for each inference variable root.
#[derive(Clone, Debug, Default)]
pub(crate) struct InferenceInfo {
    pub(crate) candidates: Vec<InferenceCandidate>,
    /// Candidates from contravariant positions (e.g., function parameters).
    /// When only `contra_candidates` exist (no covariant candidates), the
    /// resolution uses intersection instead of union, matching tsc behavior.
    pub(crate) contra_candidates: Vec<InferenceCandidate>,
    pub(crate) upper_bounds: Vec<TypeId>,
    pub(crate) resolved: Option<TypeId>,
}

impl InferenceInfo {
    pub(crate) const fn is_empty(&self) -> bool {
        self.candidates.is_empty()
            && self.contra_candidates.is_empty()
            && self.upper_bounds.is_empty()
    }
}

impl UnifyKey for InferenceVar {
    type Value = InferenceInfo;

    fn index(&self) -> u32 {
        self.0
    }

    fn from_index(u: u32) -> Self {
        Self(u)
    }

    fn tag() -> &'static str {
        "InferenceVar"
    }
}

impl UnifyValue for InferenceInfo {
    type Error = NoError;

    fn unify_values(a: &Self, b: &Self) -> Result<Self, Self::Error> {
        let mut merged = a.clone();

        // Deduplicate candidates using helper
        extend_dedup(&mut merged.candidates, &b.candidates);
        extend_dedup(&mut merged.contra_candidates, &b.contra_candidates);

        // Deduplicate upper bounds using helper
        extend_dedup(&mut merged.upper_bounds, &b.upper_bounds);

        if merged.resolved.is_none() {
            merged.resolved = b.resolved;
        }
        Ok(merged)
    }
}

/// Inference error
#[derive(Clone, Debug)]
#[allow(dead_code)] // Variants/fields reserved for full inference error reporting
pub(crate) enum InferenceError {
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
#[allow(dead_code)] // Methods reserved for constraint-based inference resolution
pub(crate) struct ConstraintSet {
    /// Lower bounds: types that must be subtypes of this variable
    /// e.g., from argument types being assigned to a parameter
    pub(crate) lower_bounds: Vec<TypeId>,
    /// Upper bounds: types that this variable must be a subtype of
    /// e.g., from `extends` constraints on type parameters
    pub(crate) upper_bounds: Vec<TypeId>,
}

#[allow(dead_code)] // Methods reserved for constraint-based inference resolution
impl ConstraintSet {
    pub const fn new() -> Self {
        Self {
            lower_bounds: Vec::new(),
            upper_bounds: Vec::new(),
        }
    }

    pub fn from_info(info: &InferenceInfo) -> Self {
        let mut lower_bounds = Vec::new();
        let mut upper_bounds = Vec::new();
        let mut seen_lower = FxHashSet::default();
        let mut seen_upper = FxHashSet::default();

        for candidate in &info.candidates {
            if seen_lower.insert(candidate.type_id) {
                lower_bounds.push(candidate.type_id);
            }
        }

        for &upper in &info.upper_bounds {
            if seen_upper.insert(upper) {
                upper_bounds.push(upper);
            }
        }

        Self {
            lower_bounds,
            upper_bounds,
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
    pub const fn is_empty(&self) -> bool {
        self.lower_bounds.is_empty() && self.upper_bounds.is_empty()
    }
}

/// Maximum iterations for constraint strengthening loops to prevent infinite loops.
pub(crate) const MAX_CONSTRAINT_ITERATIONS: usize = 100;

/// Maximum recursion depth for type containment checks.
#[allow(dead_code)] // Used by conditional type inference (not yet wired up)
pub(crate) const MAX_TYPE_RECURSION_DEPTH: usize = 100;

/// Type inference context for a single function call or expression.
pub(crate) struct InferenceContext<'a> {
    pub(crate) interner: &'a dyn TypeDatabase,
    /// Type resolver for semantic lookups (e.g., base class queries)
    pub(crate) resolver: Option<&'a dyn crate::TypeResolver>,
    /// Memoized subtype checks used by BCT and bound validation.
    pub(crate) subtype_cache: RefCell<FxHashMap<(TypeId, TypeId), bool>>,
    /// Unification table for inference variables
    pub(crate) table: InPlaceUnificationTable<InferenceVar>,
    /// Map from type parameter names to inference variables, with const flag
    pub(crate) type_params: Vec<(Atom, InferenceVar, bool)>,
    /// Declared `extends` constraints per inference variable (from type parameter declarations).
    /// Separate from `upper_bounds` which also includes contextual type bounds.
    /// Used to decide literal type preservation: `T extends string` preserves `"z"`,
    /// but contextual `Box<boolean>` should NOT preserve `false`.
    pub(crate) declared_constraints: FxHashMap<InferenceVar, TypeId>,
    /// Depth counter for `TypeApplication` expansion during inference.
    /// Prevents infinite recursion when inferring through recursive type aliases
    /// like `type Spec<T> = { [P in keyof T]: Spec<T[P]> }`.
    pub(crate) app_expansion_depth: u32,
    /// When true, candidates are routed to `contra_candidates` instead of
    /// regular `candidates`. This is set during the forward inference pass
    /// of callback parameters (contravariant context) so that structural
    /// decomposition produces contra-candidates matching tsc's behavior:
    /// contra-candidates are resolved via intersection and are only used
    /// when no covariant candidates exist.
    pub(crate) in_contra_mode: bool,
}

impl<'a> InferenceContext<'a> {
    pub(crate) const UPPER_BOUND_INTERSECTION_FAST_PATH_LIMIT: usize = 8;
    pub(crate) const UPPER_BOUND_INTERSECTION_LARGE_SET_THRESHOLD: usize = 64;
    /// Maximum depth for expanding `TypeApplication` targets during inference.
    /// Prevents infinite recursion for recursive type aliases.
    pub(crate) const MAX_APP_EXPANSION_DEPTH: u32 = 5;

    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        InferenceContext {
            interner,
            resolver: None,
            subtype_cache: RefCell::new(FxHashMap::default()),
            table: InPlaceUnificationTable::new(),
            type_params: Vec::new(),
            declared_constraints: FxHashMap::default(),
            app_expansion_depth: 0,
            in_contra_mode: false,
        }
    }

    pub fn with_resolver(
        interner: &'a dyn TypeDatabase,
        resolver: &'a dyn crate::TypeResolver,
    ) -> Self {
        InferenceContext {
            interner,
            resolver: Some(resolver),
            subtype_cache: RefCell::new(FxHashMap::default()),
            table: InPlaceUnificationTable::new(),
            type_params: Vec::new(),
            declared_constraints: FxHashMap::default(),
            app_expansion_depth: 0,
            in_contra_mode: false,
        }
    }

    /// Create a fresh inference variable
    pub fn fresh_var(&mut self) -> InferenceVar {
        self.table.new_key(InferenceInfo::default())
    }

    /// Create an inference variable for a type parameter
    pub fn fresh_type_param(&mut self, name: Atom, is_const: bool) -> InferenceVar {
        let var = self.fresh_var();
        self.type_params.push((name, var, is_const));
        var
    }

    /// Register an existing inference variable as representing a type parameter.
    ///
    /// This is useful when the caller needs to compute a unique placeholder name
    /// (and corresponding placeholder `TypeId`) after allocating the inference variable.
    pub fn register_type_param(&mut self, name: Atom, var: InferenceVar, is_const: bool) {
        self.type_params.push((name, var, is_const));
    }

    /// Look up an inference variable by type parameter name
    pub fn find_type_param(&self, name: Atom) -> Option<InferenceVar> {
        self.type_params
            .iter()
            .find(|(n, _, _)| *n == name)
            .map(|(_, v, _)| *v)
    }

    /// Record the declared `extends` constraint for an inference variable.
    pub fn set_declared_constraint(&mut self, var: InferenceVar, constraint: TypeId) {
        self.declared_constraints.insert(var, constraint);
    }

    /// Get the declared `extends` constraint for an inference variable.
    #[allow(dead_code)] // Reserved for full constraint-based inference
    pub fn get_declared_constraint(&mut self, var: InferenceVar) -> Option<TypeId> {
        let root = self.table.find(var);
        self.declared_constraints.get(&root).copied()
    }

    /// Check if an inference variable is a const type parameter
    pub fn is_var_const(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        self.type_params
            .iter()
            .any(|(_, v, is_const)| self.table.find(*v) == root && *is_const)
    }

    /// Probe the current value of an inference variable
    pub fn probe(&mut self, var: InferenceVar) -> Option<TypeId> {
        self.table.probe_value(var).resolved
    }

    /// Unify an inference variable with a concrete type
    #[allow(dead_code)] // Reserved for full constraint-based inference
    pub fn unify_var_type(&mut self, var: InferenceVar, ty: TypeId) -> Result<(), InferenceError> {
        // Get the root variable
        let root = self.table.find(var);

        if self.occurs_in(root, ty) {
            return Err(InferenceError::OccursCheck { var: root, ty });
        }

        // Check current value
        match self.table.probe_value(root).resolved {
            None => {
                self.table.union_value(
                    root,
                    InferenceInfo {
                        resolved: Some(ty),
                        ..InferenceInfo::default()
                    },
                );
                Ok(())
            }
            Some(existing) => {
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

        let value_a = self.table.probe_value(root_a).resolved;
        let value_b = self.table.probe_value(root_b).resolved;
        if let (Some(a_ty), Some(b_ty)) = (value_a, value_b)
            && !self.types_compatible(a_ty, b_ty)
        {
            return Err(InferenceError::Conflict(a_ty, b_ty));
        }

        self.table
            .unify_var_var(root_a, root_b)
            .map_err(|_| InferenceError::Conflict(TypeId::ERROR, TypeId::ERROR))?;
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

    pub(crate) fn occurs_in(&mut self, var: InferenceVar, ty: TypeId) -> bool {
        let root = self.table.find(var);
        if self.type_params.is_empty() {
            return false;
        }

        let mut visited = FxHashSet::default();
        for &(atom, param_var, _) in &self.type_params {
            if self.table.find(param_var) == root
                && self.type_contains_param(ty, atom, &mut visited)
            {
                return true;
            }
        }
        false
    }

    pub(crate) fn type_param_names_for_root(&mut self, root: InferenceVar) -> Vec<Atom> {
        self.type_params
            .iter()
            .filter(|&(_name, var, _)| self.table.find(*var) == root)
            .map(|(name, _var, _)| *name)
            .collect()
    }

    pub(crate) fn upper_bound_cycles_param(&mut self, bound: TypeId, targets: &[Atom]) -> bool {
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

    pub(crate) fn expand_cyclic_upper_bound(
        &mut self,
        root: InferenceVar,
        bound: TypeId,
        target_names: &[Atom],
        candidates: &mut Vec<InferenceCandidate>,
        upper_bounds: &mut Vec<TypeId>,
    ) {
        let name = match self.interner.lookup(bound) {
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info.name,
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
        let info = self.table.probe_value(bound_root);

        for candidate in info.candidates {
            if self.occurs_in(root, candidate.type_id) {
                continue;
            }
            candidates.push(InferenceCandidate {
                type_id: candidate.type_id,
                priority: InferencePriority::Circular,
                is_fresh_literal: candidate.is_fresh_literal,
                from_object_property: candidate.from_object_property,
                from_index_signature: candidate.from_index_signature,
                object_property_index: candidate.object_property_index,
                object_property_name: candidate.object_property_name,
            });
        }

        for ty in info.upper_bounds {
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
            TypeData::TypeParameter(info) | TypeData::Infer(info) => {
                params.insert(info.name);
            }
            TypeData::Array(elem) => {
                self.collect_type_params(elem, params, visited);
            }
            TypeData::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                for element in elements.iter() {
                    self.collect_type_params(element.type_id, params, visited);
                }
            }
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let members = self.interner.type_list(members);
                for &member in members.iter() {
                    self.collect_type_params(member, params, visited);
                }
            }
            TypeData::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    self.collect_type_params(prop.type_id, params, visited);
                }
            }
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
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
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.collect_type_params(app.base, params, visited);
                for &arg in &app.args {
                    self.collect_type_params(arg, params, visited);
                }
            }
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                for param in &shape.params {
                    self.collect_type_params(param.type_id, params, visited);
                }
                if let Some(this_type) = shape.this_type {
                    self.collect_type_params(this_type, params, visited);
                }
                self.collect_type_params(shape.return_type, params, visited);
            }
            TypeData::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                for sig in &shape.call_signatures {
                    for param in &sig.params {
                        self.collect_type_params(param.type_id, params, visited);
                    }
                    if let Some(this_type) = sig.this_type {
                        self.collect_type_params(this_type, params, visited);
                    }
                    self.collect_type_params(sig.return_type, params, visited);
                }
                for sig in &shape.construct_signatures {
                    for param in &sig.params {
                        self.collect_type_params(param.type_id, params, visited);
                    }
                    if let Some(this_type) = sig.this_type {
                        self.collect_type_params(this_type, params, visited);
                    }
                    self.collect_type_params(sig.return_type, params, visited);
                }
                for prop in &shape.properties {
                    self.collect_type_params(prop.type_id, params, visited);
                }
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.get_conditional(cond_id);
                self.collect_type_params(cond.check_type, params, visited);
                self.collect_type_params(cond.extends_type, params, visited);
                self.collect_type_params(cond.true_type, params, visited);
                self.collect_type_params(cond.false_type, params, visited);
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.get_mapped(mapped_id);
                self.collect_type_params(mapped.constraint, params, visited);
                if let Some(name_type) = mapped.name_type {
                    self.collect_type_params(name_type, params, visited);
                }
                self.collect_type_params(mapped.template, params, visited);
            }
            TypeData::IndexAccess(obj, idx) => {
                self.collect_type_params(obj, params, visited);
                self.collect_type_params(idx, params, visited);
            }
            TypeData::KeyOf(operand) | TypeData::ReadonlyType(operand) => {
                self.collect_type_params(operand, params, visited);
            }
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                for span in spans.iter() {
                    if let TemplateSpan::Type(inner) = span {
                        self.collect_type_params(*inner, params, visited);
                    }
                }
            }
            TypeData::StringIntrinsic { type_arg, .. } => {
                self.collect_type_params(type_arg, params, visited);
            }
            TypeData::Enum(_def_id, member_type) => {
                // Recurse into the structural member type
                self.collect_type_params(member_type, params, visited);
            }
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::BoundParameter(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ThisType
            | TypeData::ModuleNamespace(_)
            | TypeData::Error => {}
            TypeData::NoInfer(inner) => {
                self.collect_type_params(inner, params, visited);
            }
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
        let upper_bounds = self.table.probe_value(root).upper_bounds;

        for bound in upper_bounds {
            for target in targets {
                let mut seen = FxHashSet::default();
                if self.type_contains_param(bound, *target, &mut seen) {
                    return true;
                }
            }
            if let Some(TypeData::TypeParameter(info)) = self.interner.lookup(bound)
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
            TypeData::TypeParameter(info) | TypeData::Infer(info) => info.name == target,
            TypeData::Array(elem) => self.type_contains_param(elem, target, visited),
            TypeData::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                elements
                    .iter()
                    .any(|e| self.type_contains_param(e.type_id, target, visited))
            }
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|&member| self.type_contains_param(member, target, visited))
            }
            TypeData::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|p| self.type_contains_param(p.type_id, target, visited))
            }
            TypeData::ObjectWithIndex(shape_id) => {
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
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.type_contains_param(app.base, target, visited)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.type_contains_param(arg, target, visited))
            }
            TypeData::Function(shape_id) => {
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
            TypeData::Callable(shape_id) => {
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
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.get_conditional(cond_id);
                self.type_contains_param(cond.check_type, target, visited)
                    || self.type_contains_param(cond.extends_type, target, visited)
                    || self.type_contains_param(cond.true_type, target, visited)
                    || self.type_contains_param(cond.false_type, target, visited)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.get_mapped(mapped_id);
                if mapped.type_param.name == target {
                    return false;
                }
                self.type_contains_param(mapped.constraint, target, visited)
                    || self.type_contains_param(mapped.template, target, visited)
            }
            TypeData::IndexAccess(obj, idx) => {
                self.type_contains_param(obj, target, visited)
                    || self.type_contains_param(idx, target, visited)
            }
            TypeData::KeyOf(operand) | TypeData::ReadonlyType(operand) => {
                self.type_contains_param(operand, target, visited)
            }
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => self.type_contains_param(*inner, target, visited),
                })
            }
            TypeData::StringIntrinsic { type_arg, .. } => {
                self.type_contains_param(type_arg, target, visited)
            }
            TypeData::Enum(_def_id, member_type) => {
                // Recurse into the structural member type
                self.type_contains_param(member_type, target, visited)
            }
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::BoundParameter(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ThisType
            | TypeData::ModuleNamespace(_)
            | TypeData::Error => false,
            TypeData::NoInfer(inner) => self.type_contains_param(inner, target, visited),
        }
    }

    /// Resolve all type parameters to concrete types
    #[allow(dead_code)] // Reserved for full constraint-based inference
    pub fn resolve_all(&mut self) -> Result<Vec<(Atom, TypeId)>, InferenceError> {
        // Clone type_params to avoid borrow conflict
        let type_params: Vec<_> = self.type_params.clone();
        let mut results = Vec::new();
        for (name, var, _) in type_params {
            match self.probe(var) {
                Some(ty) => results.push((name, ty)),
                None => return Err(InferenceError::Unresolved(var)),
            }
        }
        Ok(results)
    }

    /// Get the interner reference
    #[allow(dead_code)] // Reserved for full constraint-based inference
    pub fn interner(&self) -> &dyn TypeDatabase {
        self.interner
    }

    /// Substitute source inference variable placeholders in the candidates
    /// and upper bounds of a set of target variables.
    ///
    /// When a generic function is passed as an argument to another generic function,
    /// the constraint collector creates "source" inference variables for the inner
    /// function's type parameters. These may leak into the outer variables' candidates
    /// as raw `TypeParameter` placeholders (e.g., `Array<__infer_src_3>`).
    ///
    /// This method resolves those source variables and substitutes their resolved
    /// types back into the outer variables' candidates, so the resolution phase
    /// sees concrete types instead of opaque placeholders.
    pub fn substitute_source_vars_in_targets(
        &mut self,
        target_vars: &[InferenceVar],
        source_subst: &crate::instantiation::instantiate::TypeSubstitution,
        interner: &dyn TypeDatabase,
    ) {
        use crate::instantiation::instantiate::instantiate_type;
        let target_set: FxHashSet<InferenceVar> =
            target_vars.iter().map(|v| self.table.find(*v)).collect();
        for &var in target_vars {
            let root = self.table.find(var);
            let info = self.table.probe_value(root);
            let mut changed = false;
            let mut new_candidates: Vec<InferenceCandidate> = info
                .candidates
                .iter()
                .map(|c| {
                    let subst_ty = instantiate_type(interner, c.type_id, source_subst);
                    if subst_ty != c.type_id {
                        changed = true;
                    }
                    InferenceCandidate {
                        type_id: subst_ty,
                        ..*c
                    }
                })
                .collect();
            let mut new_contra: Vec<InferenceCandidate> = info
                .contra_candidates
                .iter()
                .map(|c| {
                    let subst_ty = instantiate_type(interner, c.type_id, source_subst);
                    if subst_ty != c.type_id {
                        changed = true;
                    }
                    InferenceCandidate {
                        type_id: subst_ty,
                        ..*c
                    }
                })
                .collect();
            let new_upper: Vec<TypeId> = info
                .upper_bounds
                .iter()
                .map(|&ub| {
                    let subst_ty = instantiate_type(interner, ub, source_subst);
                    if subst_ty != ub {
                        changed = true;
                    }
                    subst_ty
                })
                .collect();
            if changed {
                // Filter out candidates that are themselves target inference variables.
                // After substitution, a candidate like `__infer_src_Y` might resolve to
                // `__infer_1`, which is another outer var. Remove such self-references
                // to prevent circular resolution.
                new_candidates.retain(|c| {
                    if let Some(TypeData::TypeParameter(_)) = interner.lookup(c.type_id) {
                        // Check if this type parameter is one of our target inference variables
                        !target_set
                            .iter()
                            .any(|&tv| self.table.probe_value(tv).resolved == Some(c.type_id))
                    } else {
                        true
                    }
                });
                new_contra.retain(|c| {
                    if let Some(TypeData::TypeParameter(_)) = interner.lookup(c.type_id) {
                        !target_set
                            .iter()
                            .any(|&tv| self.table.probe_value(tv).resolved == Some(c.type_id))
                    } else {
                        true
                    }
                });
                self.table.union_value(
                    root,
                    InferenceInfo {
                        candidates: new_candidates,
                        contra_candidates: new_contra,
                        upper_bounds: new_upper,
                        resolved: info.resolved,
                    },
                );
            }
        }
    }

    // =========================================================================
    // Constraint Collection
    // =========================================================================

    /// Add a lower bound constraint: ty <: var
    /// This is used when an argument type flows into a type parameter.
    /// Updated to use `NakedTypeVariable` (highest priority) for direct argument inference.
    #[allow(dead_code)] // Reserved for full constraint-based inference
    pub fn add_lower_bound(&mut self, var: InferenceVar, ty: TypeId) {
        self.add_candidate(var, ty, InferencePriority::NakedTypeVariable);
    }

    /// Add an inference candidate for a variable.
    pub fn add_candidate(&mut self, var: InferenceVar, ty: TypeId, priority: InferencePriority) {
        self.add_candidate_with_context(var, ty, priority, CandidateContext::default());
    }

    /// Add a contravariant inference candidate for a variable.
    /// Used when the type parameter appears in a contravariant position
    /// (e.g., function parameter types). When only `contra_candidates` exist
    /// (no covariant candidates), resolution uses intersection instead of
    /// union, matching tsc's `contraCandidates` behavior.
    pub fn add_contra_candidate(
        &mut self,
        var: InferenceVar,
        ty: TypeId,
        priority: InferencePriority,
    ) {
        let root = self.table.find(var);
        let candidate = InferenceCandidate {
            type_id: ty,
            priority,
            is_fresh_literal: is_literal_type(self.interner, ty),
            from_object_property: false,
            from_index_signature: false,
            object_property_index: None,
            object_property_name: None,
        };
        self.table.union_value(
            root,
            InferenceInfo {
                contra_candidates: vec![candidate],
                ..InferenceInfo::default()
            },
        );
    }

    /// Add an inference candidate for a variable that originates from an object property.
    /// `object_property_index` captures the source property order and enables deterministic
    /// tie-breaking when repeated property candidates collapse to a union.
    /// `source_is_fresh` indicates whether the source object is a fresh literal (from an
    /// object literal expression). When true, literal property types will be widened during
    /// inference resolution (matching TSC's `RequiresWidening` behavior).
    pub fn add_property_candidate_with_index(
        &mut self,
        var: InferenceVar,
        ty: TypeId,
        priority: InferencePriority,
        object_property_index: u32,
        object_property_name: Option<Atom>,
        source_is_fresh: bool,
    ) {
        self.add_candidate_with_context(
            var,
            ty,
            priority,
            CandidateContext {
                from_object_property: true,
                object_property_index: Some(object_property_index),
                object_property_name,
                source_is_fresh,
                ..CandidateContext::default()
            },
        );
    }

    pub fn add_index_signature_candidate_with_index(
        &mut self,
        var: InferenceVar,
        ty: TypeId,
        priority: InferencePriority,
        object_property_index: u32,
        source_is_fresh: bool,
    ) {
        self.add_candidate_with_context(
            var,
            ty,
            priority,
            CandidateContext {
                from_object_property: true,
                from_index_signature: true,
                object_property_index: Some(object_property_index),
                source_is_fresh,
                ..CandidateContext::default()
            },
        );
    }

    fn add_candidate_with_context(
        &mut self,
        var: InferenceVar,
        ty: TypeId,
        priority: InferencePriority,
        context: CandidateContext,
    ) {
        let root = self.table.find(var);
        // A candidate is a "fresh literal" (eligible for widening) when:
        // - It's a literal type AND
        // - Either it's NOT from an object property (direct arg like identity("hello")),
        //   OR the source object is a fresh literal (from object literal expression).
        // This matches TSC's RequiresWidening flag: literals from type annotations
        // (non-fresh sources) are NOT widened, but literals from object literal
        // expressions ARE widened.
        let candidate = InferenceCandidate {
            type_id: ty,
            priority,
            is_fresh_literal: (!context.from_object_property || context.source_is_fresh)
                && is_literal_type(self.interner, ty),
            from_object_property: context.from_object_property,
            from_index_signature: context.from_index_signature,
            object_property_index: context.object_property_index,
            object_property_name: context.object_property_name,
        };
        if self.in_contra_mode {
            // In contravariant context (e.g., callback parameter structural
            // decomposition), route to contra_candidates so they are resolved
            // via intersection and only used when no covariant candidates exist.
            self.table.union_value(
                root,
                InferenceInfo {
                    contra_candidates: vec![candidate],
                    ..InferenceInfo::default()
                },
            );
        } else {
            self.table.union_value(
                root,
                InferenceInfo {
                    candidates: vec![candidate],
                    ..InferenceInfo::default()
                },
            );
        }
    }

    /// Add an upper bound constraint: var <: ty
    /// This is used for `extends` constraints on type parameters.
    pub fn add_upper_bound(&mut self, var: InferenceVar, ty: TypeId) {
        let root = self.table.find(var);
        self.table.union_value(
            root,
            InferenceInfo {
                upper_bounds: vec![ty],
                ..InferenceInfo::default()
            },
        );
    }

    /// Get the constraints for a variable
    pub fn get_constraints(&mut self, var: InferenceVar) -> Option<ConstraintSet> {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        if info.is_empty() {
            None
        } else {
            Some(ConstraintSet::from_info(&info))
        }
    }

    /// Check whether an inference variable has any candidates (covariant or contravariant).
    pub fn var_has_candidates(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        !info.candidates.is_empty() || !info.contra_candidates.is_empty()
    }

    /// Check whether an inference variable has `contra_candidates` with at least one
    /// concrete (non-TypeParameter) type. `TypeParameter` types in `contra_candidates`
    /// are typically unresolved source inference placeholders from generic function
    /// arguments and should not drive the resolution gate.
    pub fn has_concrete_contra_candidates(
        &mut self,
        var: InferenceVar,
        db: &dyn crate::caches::db::TypeDatabase,
    ) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.contra_candidates
            .iter()
            .any(|c| !matches!(db.lookup(c.type_id), Some(TypeData::TypeParameter(_))))
    }

    /// Check whether an inference variable has any contravariant candidates that are
    /// usable for resolution. Synthetic inference placeholders like `__infer_*` and
    /// `__infer_src_*` are excluded, but real outer type parameters (for example `T`
    /// from `function g<T>(...)`) are preserved because they carry meaningful
    /// cross-generic evidence.
    pub fn has_usable_contra_candidates(
        &mut self,
        var: InferenceVar,
        db: &dyn crate::caches::db::TypeDatabase,
    ) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.contra_candidates.iter().any(|c| {
            !crate::type_queries::data::is_bare_infer_placeholder_db(db, c.type_id)
        })
    }

    /// Check whether a variable's inference came exclusively from contravariant positions.
    pub fn has_only_contra_candidates(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.candidates.is_empty() && !info.contra_candidates.is_empty()
    }

    pub fn has_index_signature_candidates(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.candidates
            .iter()
            .any(|candidate| candidate.from_index_signature)
    }

    /// Check if all inference candidates for a variable have `ReturnType` priority.
    /// This indicates the type was inferred from callback return types (Round 2),
    /// not from direct arguments (Round 1).
    pub fn all_candidates_are_return_type(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        !info.candidates.is_empty()
            && info
                .candidates
                .iter()
                .all(|c| c.priority == InferencePriority::ReturnType)
    }

    /// Get the original un-widened literal candidate types for an inference variable.
    pub fn get_literal_candidates(&mut self, var: InferenceVar) -> Vec<TypeId> {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.candidates
            .iter()
            .filter(|c| c.is_fresh_literal)
            .map(|c| c.type_id)
            .collect()
    }

    /// Check if all covariant candidates for a variable are fresh literals.
    /// When false, the resolved type should NOT be widened by `widen_literal_type`
    /// (matches tsc's `getWidenedLiteralType` which only widens fresh literals).
    pub fn all_candidates_are_fresh_literals(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        !info.candidates.is_empty() && info.candidates.iter().all(|c| c.is_fresh_literal)
    }
}

// DISABLED: Tests use deprecated add_candidate / resolve_with_constraints API
// The inference system has been refactored to use unification-based inference.
#[cfg(test)]
#[path = "../../tests/infer_tests.rs"]
mod tests;
