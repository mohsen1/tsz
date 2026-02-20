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
use crate::types::{
    CallableShapeId, FunctionShapeId, InferencePriority, IntrinsicKind, LiteralValue, MappedTypeId,
    ObjectShapeId, TemplateLiteralId, TemplateSpan, TupleElement, TupleListId, TypeApplicationId,
    TypeData, TypeId, TypeListId,
};
use crate::visitor::is_literal_type;
use ena::unify::{InPlaceUnificationTable, NoError, UnifyKey, UnifyValue};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use tsz_common::interner::Atom;

/// Helper function to extend a vector with deduplicated items.
/// Uses a `HashSet` for O(1) lookups instead of O(n) contains checks.
fn extend_dedup<T>(target: &mut Vec<T>, items: &[T])
where
    T: Clone + Eq + std::hash::Hash,
{
    if items.is_empty() {
        return;
    }

    // Hot path for inference: most merges add a single item.
    // Avoid allocating/hash-building a set for that case.
    if items.len() == 1 {
        let item = &items[0];
        if !target.contains(item) {
            target.push(item.clone());
        }
        return;
    }

    let mut existing: FxHashSet<_> = target.iter().cloned().collect();
    for item in items {
        if existing.insert(item.clone()) {
            target.push(item.clone());
        }
    }
}

/// An inference variable representing an unknown type.
/// These are created when instantiating generic functions.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct InferenceVar(pub u32);

// Uses TypeScript-standard InferencePriority from types.rs

/// A candidate type for an inference variable.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InferenceCandidate {
    pub type_id: TypeId,
    pub priority: InferencePriority,
    pub is_fresh_literal: bool,
}

/// Value stored for each inference variable root.
#[derive(Clone, Debug, Default)]
pub struct InferenceInfo {
    pub candidates: Vec<InferenceCandidate>,
    pub upper_bounds: Vec<TypeId>,
    pub resolved: Option<TypeId>,
}

impl InferenceInfo {
    pub const fn is_empty(&self) -> bool {
        self.candidates.is_empty() && self.upper_bounds.is_empty()
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

    pub fn merge_from(&mut self, other: Self) {
        for ty in other.lower_bounds {
            self.add_lower_bound(ty);
        }
        for ty in other.upper_bounds {
            self.add_upper_bound(ty);
        }
    }

    /// Perform transitive reduction on upper bounds to remove redundant constraints.
    ///
    /// If we have constraints (T <: A) and (T <: B) and we know (A <: B),
    /// then (T <: B) is redundant and can be removed.
    ///
    /// This reduces N² pairwise checks in `detect_conflicts` to O(N * `reduced_N`).
    pub fn transitive_reduction(&mut self, interner: &dyn TypeDatabase) {
        if self.upper_bounds.len() < 2 {
            return;
        }

        let mut redundant = FxHashSet::default();
        let bounds = &self.upper_bounds;

        for (i, &u1) in bounds.iter().enumerate() {
            for (j, &u2) in bounds.iter().enumerate() {
                if i == j || redundant.contains(&u1) || redundant.contains(&u2) {
                    continue;
                }

                // If u1 <: u2, then u2 is redundant (u1 is a stricter constraint)
                if crate::subtype::is_subtype_of(interner, u1, u2) {
                    redundant.insert(u2);
                }
            }
        }

        if !redundant.is_empty() {
            self.upper_bounds.retain(|ty| !redundant.contains(ty));
        }
    }

    /// Detect early conflicts between collected constraints.
    /// This allows failing fast before full resolution.
    pub fn detect_conflicts(&self, interner: &dyn TypeDatabase) -> Option<ConstraintConflict> {
        // PERF: Transitive reduction of upper bounds to minimize N² checks.
        let mut reduced_upper = self.upper_bounds.clone();
        if reduced_upper.len() >= 2 {
            let mut redundant = FxHashSet::default();
            for (i, &u1) in reduced_upper.iter().enumerate() {
                for (j, &u2) in reduced_upper.iter().enumerate() {
                    if i == j || redundant.contains(&u1) || redundant.contains(&u2) {
                        continue;
                    }
                    if crate::subtype::is_subtype_of(interner, u1, u2) {
                        redundant.insert(u2);
                    }
                }
            }
            if !redundant.is_empty() {
                reduced_upper.retain(|ty| !redundant.contains(ty));
            }
        }

        // 1. Check for mutually exclusive upper bounds
        for (i, &u1) in reduced_upper.iter().enumerate() {
            for &u2 in &reduced_upper[i + 1..] {
                if are_disjoint(interner, u1, u2) {
                    return Some(ConstraintConflict::DisjointUpperBounds(u1, u2));
                }
            }
        }

        // 2. Check if any lower bound is incompatible with any upper bound
        for &lower in &self.lower_bounds {
            for &upper in &reduced_upper {
                // Ignore ERROR and ANY for conflict detection
                if lower == TypeId::ERROR
                    || upper == TypeId::ERROR
                    || lower == TypeId::ANY
                    || upper == TypeId::ANY
                {
                    continue;
                }
                if !crate::subtype::is_subtype_of(interner, lower, upper) {
                    return Some(ConstraintConflict::LowerExceedsUpper(lower, upper));
                }
            }
        }

        None
    }
}

/// Conflict detected between constraints on an inference variable.
#[derive(Clone, Debug)]
pub enum ConstraintConflict {
    /// Mutually exclusive upper bounds (e.g., string AND number)
    DisjointUpperBounds(TypeId, TypeId),
    /// A lower bound is not a subtype of an upper bound
    LowerExceedsUpper(TypeId, TypeId),
}

/// Helper to determine if two types are definitely disjoint (no common inhabitants).
fn are_disjoint(interner: &dyn TypeDatabase, a: TypeId, b: TypeId) -> bool {
    if a == b {
        return false;
    }
    if a.is_any_or_unknown() || b.is_any_or_unknown() {
        return false;
    }

    let key_a = interner.lookup(a);
    let key_b = interner.lookup(b);

    match (key_a, key_b) {
        (Some(TypeData::Intrinsic(k1)), Some(TypeData::Intrinsic(k2))) => {
            use IntrinsicKind::*;
            // Basic primitives are disjoint (ignoring object/Function which are more complex)
            k1 != k2 && !matches!((k1, k2), (Object | Function, _) | (_, Object | Function))
        }
        (Some(TypeData::Literal(l1)), Some(TypeData::Literal(l2))) => l1 != l2,
        (Some(TypeData::Literal(l1)), Some(TypeData::Intrinsic(k2))) => {
            !is_literal_compatible_with_intrinsic(&l1, k2)
        }
        (Some(TypeData::Intrinsic(k1)), Some(TypeData::Literal(l2))) => {
            !is_literal_compatible_with_intrinsic(&l2, k1)
        }
        _ => false,
    }
}

fn is_literal_compatible_with_intrinsic(lit: &LiteralValue, kind: IntrinsicKind) -> bool {
    match lit {
        LiteralValue::String(_) => kind == IntrinsicKind::String,
        LiteralValue::Number(_) => kind == IntrinsicKind::Number,
        LiteralValue::BigInt(_) => kind == IntrinsicKind::Bigint,
        LiteralValue::Boolean(_) => kind == IntrinsicKind::Boolean,
    }
}

/// Maximum iterations for constraint strengthening loops to prevent infinite loops.
pub const MAX_CONSTRAINT_ITERATIONS: usize = 100;

/// Maximum recursion depth for type containment checks.
pub const MAX_TYPE_RECURSION_DEPTH: usize = 100;

/// Type inference context for a single function call or expression.
pub struct InferenceContext<'a> {
    pub(crate) interner: &'a dyn TypeDatabase,
    /// Type resolver for semantic lookups (e.g., base class queries)
    pub(crate) resolver: Option<&'a dyn crate::TypeResolver>,
    /// Memoized subtype checks used by BCT and bound validation.
    pub(crate) subtype_cache: RefCell<FxHashMap<(TypeId, TypeId), bool>>,
    /// Unification table for inference variables
    pub(crate) table: InPlaceUnificationTable<InferenceVar>,
    /// Map from type parameter names to inference variables, with const flag
    pub(crate) type_params: Vec<(Atom, InferenceVar, bool)>,
}

impl<'a> InferenceContext<'a> {
    pub(crate) const UPPER_BOUND_INTERSECTION_FAST_PATH_LIMIT: usize = 8;
    pub(crate) const UPPER_BOUND_INTERSECTION_LARGE_SET_THRESHOLD: usize = 64;

    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        InferenceContext {
            interner,
            resolver: None,
            subtype_cache: RefCell::new(FxHashMap::default()),
            table: InPlaceUnificationTable::new(),
            type_params: Vec::new(),
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
                let cond = self.interner.conditional_type(cond_id);
                self.collect_type_params(cond.check_type, params, visited);
                self.collect_type_params(cond.extends_type, params, visited);
                self.collect_type_params(cond.true_type, params, visited);
                self.collect_type_params(cond.false_type, params, visited);
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(mapped_id);
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
                let cond = self.interner.conditional_type(cond_id);
                self.type_contains_param(cond.check_type, target, visited)
                    || self.type_contains_param(cond.extends_type, target, visited)
                    || self.type_contains_param(cond.true_type, target, visited)
                    || self.type_contains_param(cond.false_type, target, visited)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(mapped_id);
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
    pub fn interner(&self) -> &dyn TypeDatabase {
        self.interner
    }

    // =========================================================================
    // Constraint Collection
    // =========================================================================

    /// Add a lower bound constraint: ty <: var
    /// This is used when an argument type flows into a type parameter.
    /// Updated to use `NakedTypeVariable` (highest priority) for direct argument inference.
    pub fn add_lower_bound(&mut self, var: InferenceVar, ty: TypeId) {
        self.add_candidate(var, ty, InferencePriority::NakedTypeVariable);
    }

    /// Add an inference candidate for a variable.
    pub fn add_candidate(&mut self, var: InferenceVar, ty: TypeId, priority: InferencePriority) {
        let root = self.table.find(var);
        let candidate = InferenceCandidate {
            type_id: ty,
            priority,
            is_fresh_literal: is_literal_type(self.interner, ty),
        };
        self.table.union_value(
            root,
            InferenceInfo {
                candidates: vec![candidate],
                ..InferenceInfo::default()
            },
        );
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

    /// Collect a constraint from an assignment: source flows into target
    /// If target is an inference variable, source becomes a lower bound.
    /// If source is an inference variable, target becomes an upper bound.
    pub const fn collect_constraint(&mut self, _source: TypeId, _target: TypeId) {
        // Check if target is an inference variable (via TypeData lookup)
        // For now, we rely on the caller to call add_lower_bound/add_upper_bound directly
        // This is a placeholder for more sophisticated constraint collection
    }

    /// Perform structural type inference from a source type to a target type.
    ///
    /// This is the core algorithm for inferring type parameters from function arguments.
    /// It walks the structure of both types, collecting constraints for type parameters.
    ///
    /// # Arguments
    /// * `source` - The type from the value argument (e.g., `string` from `identity("hello")`)
    /// * `target` - The type from the parameter (e.g., `T` from `function identity<T>(x: T)`)
    /// * `priority` - The inference priority (e.g., `NakedTypeVariable` for direct arguments)
    ///
    /// # Type Inference Algorithm
    ///
    /// TypeScript uses structural type inference with the following rules:
    ///
    /// 1. **Direct Parameter Match**: If target is a type parameter `T` we're inferring,
    ///    add source as a lower bound candidate for `T`.
    ///
    /// 2. **Structural Recursion**: For complex types, recurse into the structure:
    ///    - Objects: Match properties recursively
    ///    - Arrays: Match element types
    ///    - Functions: Match parameters (contravariant) and return types (covariant)
    ///
    /// 3. **Variance Handling**:
    ///    - Covariant positions (properties, arrays, return types): `infer(source, target)`
    ///    - Contravariant positions (function parameters): `infer(target, source)` (swapped!)
    ///
    /// # Example
    /// ```ignore
    /// let mut ctx = InferenceContext::new(&interner);
    /// let t_var = ctx.fresh_type_param(interner.intern_string("T"), false);
    ///
    /// // Inference: identity("hello") should infer T = string
    /// ctx.infer_from_types(string_type, t_type, InferencePriority::NakedTypeVariable)?;
    /// ```
    pub fn infer_from_types(
        &mut self,
        source: TypeId,
        target: TypeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        // Resolve the types to their actual TypeDatas
        let source_key = self.interner.lookup(source);
        let target_key = self.interner.lookup(target);

        // Block inference if target is NoInfer<T> (TypeScript 5.4+)
        // NoInfer prevents inference from flowing through this type position
        if let Some(TypeData::NoInfer(_)) = target_key {
            return Ok(()); // Stop inference - don't descend into NoInfer
        }

        // Unwrap NoInfer from source if present (rare but possible)
        let source_key = if let Some(TypeData::NoInfer(inner)) = source_key {
            self.interner.lookup(inner)
        } else {
            source_key
        };

        // Case 1: Target is a TypeParameter we're inferring (Lower Bound: source <: T)
        if let Some(TypeData::TypeParameter(ref param_info)) = target_key
            && let Some(var) = self.find_type_param(param_info.name)
        {
            // Add source as a lower bound candidate for this type parameter
            self.add_candidate(var, source, priority);
            return Ok(());
        }

        // Case 2: Source is a TypeParameter we're inferring (Upper Bound: T <: target)
        // CRITICAL: This handles contravariance! When function parameters are swapped,
        // the TypeParameter moves to source position and becomes an upper bound.
        if let Some(TypeData::TypeParameter(ref param_info)) = source_key
            && let Some(var) = self.find_type_param(param_info.name)
        {
            // T <: target, so target is an UPPER bound
            self.add_upper_bound(var, target);
            return Ok(());
        }

        // Case 3: Structural recursion - match based on type structure
        match (source_key, target_key) {
            // Object types: recurse into properties
            (Some(TypeData::Object(source_shape)), Some(TypeData::Object(target_shape))) => {
                self.infer_objects(source_shape, target_shape, priority)?;
            }

            // Function types: handle variance (parameters are contravariant, return is covariant)
            (Some(TypeData::Function(source_func)), Some(TypeData::Function(target_func))) => {
                self.infer_functions(source_func, target_func, priority)?;
            }

            // Callable types: infer across signatures and properties
            (Some(TypeData::Callable(source_call)), Some(TypeData::Callable(target_call))) => {
                self.infer_callables(source_call, target_call, priority)?;
            }

            // Array types: recurse into element types
            (Some(TypeData::Array(source_elem)), Some(TypeData::Array(target_elem))) => {
                self.infer_from_types(source_elem, target_elem, priority)?;
            }

            // Tuple types: recurse into elements
            (Some(TypeData::Tuple(source_elems)), Some(TypeData::Tuple(target_elems))) => {
                self.infer_tuples(source_elems, target_elems, priority)?;
            }

            // Union types: try to infer against each member
            (Some(TypeData::Union(source_members)), Some(TypeData::Union(target_members))) => {
                self.infer_unions(source_members, target_members, priority)?;
            }

            // Intersection types
            (
                Some(TypeData::Intersection(source_members)),
                Some(TypeData::Intersection(target_members)),
            ) => {
                self.infer_intersections(source_members, target_members, priority)?;
            }

            // TypeApplication: recurse into instantiated type
            (Some(TypeData::Application(source_app)), Some(TypeData::Application(target_app))) => {
                self.infer_applications(source_app, target_app, priority)?;
            }

            // Index access types: infer both object and index types
            (
                Some(TypeData::IndexAccess(source_obj, source_idx)),
                Some(TypeData::IndexAccess(target_obj, target_idx)),
            ) => {
                self.infer_from_types(source_obj, target_obj, priority)?;
                self.infer_from_types(source_idx, target_idx, priority)?;
            }

            // ReadonlyType: unwrap if both are readonly (e.g. readonly [T] vs readonly [number])
            (
                Some(TypeData::ReadonlyType(source_inner)),
                Some(TypeData::ReadonlyType(target_inner)),
            ) => {
                self.infer_from_types(source_inner, target_inner, priority)?;
            }

            // Unwrap ReadonlyType when only target is readonly (mutable source is compatible)
            (_, Some(TypeData::ReadonlyType(target_inner))) => {
                self.infer_from_types(source, target_inner, priority)?;
            }

            // Task #40: Template literal deconstruction for infer patterns
            // Handles: source extends `prefix${infer T}suffix` ? true : false
            (Some(source_key), Some(TypeData::TemplateLiteral(target_id))) => {
                self.infer_from_template_literal(source, Some(&source_key), target_id, priority)?;
            }

            // Mapped type inference: infer from object properties against mapped type
            // Handles: source { a: string, b: number } against target { [P in K]: T }
            // Infers K from property names and T from property value types
            (Some(TypeData::Object(source_shape)), Some(TypeData::Mapped(mapped_id))) => {
                self.infer_from_mapped_type(source_shape, mapped_id, priority)?;
            }

            // If we can't match structurally, that's okay - it might mean the types are incompatible
            // The Checker will handle this with proper error reporting
            _ => {
                // No structural match possible
                // This is not an error - the Checker will verify assignability separately
            }
        }

        Ok(())
    }

    /// Infer from object types by matching properties
    fn infer_objects(
        &mut self,
        source_shape: ObjectShapeId,
        target_shape: ObjectShapeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source_shape = self.interner.object_shape(source_shape);
        let target_shape = self.interner.object_shape(target_shape);

        // For each property in the target, try to find a matching property in the source
        for target_prop in &target_shape.properties {
            if let Some(source_prop) = source_shape
                .properties
                .iter()
                .find(|p| p.name == target_prop.name)
            {
                self.infer_from_types(source_prop.type_id, target_prop.type_id, priority)?;
            }
        }

        // Also check index signatures for inference
        // If target has a string index signature, infer from source's string index
        if let (Some(target_string_idx), Some(source_string_idx)) =
            (&target_shape.string_index, &source_shape.string_index)
        {
            self.infer_from_types(
                source_string_idx.value_type,
                target_string_idx.value_type,
                priority,
            )?;
        }

        // If target has a number index signature, infer from source's number index
        if let (Some(target_number_idx), Some(source_number_idx)) =
            (&target_shape.number_index, &source_shape.number_index)
        {
            self.infer_from_types(
                source_number_idx.value_type,
                target_number_idx.value_type,
                priority,
            )?;
        }

        Ok(())
    }

    /// Infer type arguments from an object type matched against a mapped type.
    ///
    /// When source is `{ a: string, b: number }` and target is `{ [P in K]: T }`:
    /// - Infer K from the union of source property name literals ("a" | "b")
    /// - Infer T from each source property value type against the mapped template
    fn infer_from_mapped_type(
        &mut self,
        source_shape: ObjectShapeId,
        mapped_id: MappedTypeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let mapped = self.interner.mapped_type(mapped_id);
        let source = self.interner.object_shape(source_shape);

        if source.properties.is_empty() {
            return Ok(());
        }

        // Infer the constraint type (K) from the union of source property names
        // e.g., for { foo: string, bar: number }, K = "foo" | "bar"
        let name_literals: Vec<TypeId> = source
            .properties
            .iter()
            .map(|p| self.interner.literal_string_atom(p.name))
            .collect();
        let names_union = if name_literals.len() == 1 {
            name_literals[0]
        } else {
            self.interner.union(name_literals)
        };
        self.infer_from_types(names_union, mapped.constraint, priority)?;

        // Infer the template type (T) from each source property value type
        for prop in &source.properties {
            self.infer_from_types(prop.type_id, mapped.template, priority)?;
        }

        Ok(())
    }

    /// Infer from function types, handling variance correctly
    fn infer_functions(
        &mut self,
        source_func: FunctionShapeId,
        target_func: FunctionShapeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source_sig = self.interner.function_shape(source_func);
        let target_sig = self.interner.function_shape(target_func);

        tracing::trace!(
            source_params = source_sig.params.len(),
            target_params = target_sig.params.len(),
            "infer_functions called"
        );

        // Parameters are contravariant: swap source and target
        let mut source_params = source_sig.params.iter().peekable();
        let mut target_params = target_sig.params.iter().peekable();

        loop {
            let source_rest = source_params.peek().is_some_and(|p| p.rest);
            let target_rest = target_params.peek().is_some_and(|p| p.rest);

            tracing::trace!(
                source_rest,
                target_rest,
                "Checking rest params in loop iteration"
            );

            // If both have rest params, infer the rest element types
            if source_rest && target_rest {
                let source_param = source_params.next().unwrap();
                let target_param = target_params.next().unwrap();
                self.infer_from_types(target_param.type_id, source_param.type_id, priority)?;
                break;
            }

            // If source has rest param, infer all remaining target params into it
            if source_rest {
                let source_param = source_params.next().unwrap();
                for target_param in target_params.by_ref() {
                    self.infer_from_types(target_param.type_id, source_param.type_id, priority)?;
                }
                break;
            }

            // If target has rest param, infer all remaining source params into it
            if target_rest {
                let target_param = target_params.next().unwrap();

                // CRITICAL: Check if target rest param is a type parameter (like A extends any[])
                // If so, we need to infer it as a TUPLE of all remaining source params,
                // not as individual param types.
                //
                // Example: wrap<A extends any[], R>(fn: (...args: A) => R)
                //          with add(a: number, b: number): number
                //          should infer A = [number, number], not A = number
                let target_is_type_param = matches!(
                    self.interner.lookup(target_param.type_id),
                    Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
                );

                tracing::trace!(
                    target_is_type_param,
                    target_param_type = ?target_param.type_id,
                    "Rest parameter inference - target is type param check"
                );

                if target_is_type_param {
                    // Collect all remaining source params into a tuple
                    let mut tuple_elements = Vec::new();
                    for source_param in source_params.by_ref() {
                        tuple_elements.push(TupleElement {
                            type_id: source_param.type_id,
                            name: source_param.name,
                            optional: source_param.optional,
                            rest: source_param.rest,
                        });
                    }

                    tracing::trace!(
                        num_elements = tuple_elements.len(),
                        "Collected source params into tuple"
                    );

                    // Infer the tuple type against the type parameter
                    // Note: Parameters are contravariant, so target comes first
                    if !tuple_elements.is_empty() {
                        let tuple_type = self.interner.tuple(tuple_elements);
                        tracing::trace!(
                            tuple_type = ?tuple_type,
                            target_param = ?target_param.type_id,
                            "Inferring tuple against type parameter"
                        );
                        self.infer_from_types(target_param.type_id, tuple_type, priority)?;
                    }
                } else {
                    // Target rest param is not a type parameter (e.g., number[] or Array<string>)
                    // Infer each source param individually against the rest element type
                    for source_param in source_params.by_ref() {
                        self.infer_from_types(
                            target_param.type_id,
                            source_param.type_id,
                            priority,
                        )?;
                    }
                }
                break;
            }

            // Neither has rest param, do normal pairwise comparison
            match (source_params.next(), target_params.next()) {
                (Some(source_param), Some(target_param)) => {
                    // Note the swapped arguments! This is the key to handling contravariance.
                    self.infer_from_types(target_param.type_id, source_param.type_id, priority)?;
                }
                _ => break, // Mismatch in arity - stop here
            }
        }

        // Return type is covariant: normal order
        self.infer_from_types(source_sig.return_type, target_sig.return_type, priority)?;

        // This type is contravariant
        if let (Some(source_this), Some(target_this)) = (source_sig.this_type, target_sig.this_type)
        {
            self.infer_from_types(target_this, source_this, priority)?;
        }

        // Type predicates are covariant
        if let (Some(source_pred), Some(target_pred)) =
            (&source_sig.type_predicate, &target_sig.type_predicate)
        {
            // Compare targets by index if possible
            let targets_match = match (source_pred.parameter_index, target_pred.parameter_index) {
                (Some(s_idx), Some(t_idx)) => s_idx == t_idx,
                _ => source_pred.target == target_pred.target,
            };

            tracing::trace!(
                targets_match,
                ?source_pred.parameter_index,
                ?target_pred.parameter_index,
                "Inferring from type predicates"
            );

            if targets_match
                && source_pred.asserts == target_pred.asserts
                && let (Some(source_ty), Some(target_ty)) =
                    (source_pred.type_id, target_pred.type_id)
            {
                self.infer_from_types(source_ty, target_ty, priority)?;
            }
        }

        Ok(())
    }

    /// Infer from tuple types
    fn infer_tuples(
        &mut self,
        source_elems: TupleListId,
        target_elems: TupleListId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source_list = self.interner.tuple_list(source_elems);
        let target_list = self.interner.tuple_list(target_elems);

        for (source_elem, target_elem) in source_list.iter().zip(target_list.iter()) {
            self.infer_from_types(source_elem.type_id, target_elem.type_id, priority)?;
        }

        Ok(())
    }

    /// Infer from callable types, handling signatures and properties
    fn infer_callables(
        &mut self,
        source_id: CallableShapeId,
        target_id: CallableShapeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source = self.interner.callable_shape(source_id);
        let target = self.interner.callable_shape(target_id);

        // For each call signature in the target, try to find a compatible one in the source
        for target_sig in &target.call_signatures {
            for source_sig in &source.call_signatures {
                if source_sig.params.len() == target_sig.params.len() {
                    for (s_param, t_param) in source_sig.params.iter().zip(target_sig.params.iter())
                    {
                        self.infer_from_types(t_param.type_id, s_param.type_id, priority)?;
                    }
                    self.infer_from_types(
                        source_sig.return_type,
                        target_sig.return_type,
                        priority,
                    )?;
                    break;
                }
            }
        }

        // For each construct signature
        for target_sig in &target.construct_signatures {
            for source_sig in &source.construct_signatures {
                if source_sig.params.len() == target_sig.params.len() {
                    for (s_param, t_param) in source_sig.params.iter().zip(target_sig.params.iter())
                    {
                        self.infer_from_types(t_param.type_id, s_param.type_id, priority)?;
                    }
                    self.infer_from_types(
                        source_sig.return_type,
                        target_sig.return_type,
                        priority,
                    )?;
                    break;
                }
            }
        }

        // Properties
        for target_prop in &target.properties {
            if let Some(source_prop) = source
                .properties
                .iter()
                .find(|p| p.name == target_prop.name)
            {
                self.infer_from_types(source_prop.type_id, target_prop.type_id, priority)?;
            }
        }

        // String index
        if let (Some(target_idx), Some(source_idx)) = (&target.string_index, &source.string_index) {
            self.infer_from_types(source_idx.value_type, target_idx.value_type, priority)?;
        }

        // Number index
        if let (Some(target_idx), Some(source_idx)) = (&target.number_index, &source.number_index) {
            self.infer_from_types(source_idx.value_type, target_idx.value_type, priority)?;
        }

        Ok(())
    }

    /// Infer from union types
    fn infer_unions(
        &mut self,
        source_members: TypeListId,
        target_members: TypeListId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source_list = self.interner.type_list(source_members);
        let target_list = self.interner.type_list(target_members);

        // TypeScript inference filtering: when the target union contains both
        // type parameters and fixed types (e.g., `T | undefined`), strip source
        // members that match fixed target members before inferring against the
        // parameterized members. This prevents `undefined` in `number | undefined`
        // from being inferred as a candidate for `T` in `T | undefined`.
        let (parameterized, fixed): (Vec<TypeId>, Vec<TypeId>) = target_list
            .iter()
            .partition(|&&t| self.target_contains_inference_param(t));

        if !parameterized.is_empty() && !fixed.is_empty() {
            // Filter source: only infer members not already covered by fixed targets
            for &source_ty in source_list.iter() {
                let matches_fixed = fixed.contains(&source_ty);
                if !matches_fixed {
                    for &target_ty in &parameterized {
                        self.infer_from_types(source_ty, target_ty, priority)?;
                    }
                }
            }
        } else {
            // No filtering needed — fall back to exhaustive inference
            for source_ty in source_list.iter() {
                for target_ty in target_list.iter() {
                    self.infer_from_types(*source_ty, *target_ty, priority)?;
                }
            }
        }

        Ok(())
    }

    /// Check if a target type directly is or contains an inference type parameter.
    fn target_contains_inference_param(&self, target: TypeId) -> bool {
        let Some(key) = self.interner.lookup(target) else {
            return false;
        };
        match key {
            TypeData::TypeParameter(ref info) => self.find_type_param(info.name).is_some(),
            _ => false,
        }
    }

    /// Infer from intersection types
    fn infer_intersections(
        &mut self,
        source_members: TypeListId,
        target_members: TypeListId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source_list = self.interner.type_list(source_members);
        let target_list = self.interner.type_list(target_members);

        // For intersections, we can pick any member that matches
        for source_ty in source_list.iter() {
            for target_ty in target_list.iter() {
                // Don't fail if one member doesn't match
                let _ = self.infer_from_types(*source_ty, *target_ty, priority);
            }
        }

        Ok(())
    }

    /// Infer from `TypeApplication` (generic type instantiations)
    fn infer_applications(
        &mut self,
        source_app: TypeApplicationId,
        target_app: TypeApplicationId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source_info = self.interner.type_application(source_app);
        let target_info = self.interner.type_application(target_app);

        // The base types must match for inference to work
        if source_info.base != target_info.base {
            return Ok(());
        }

        // Recurse into the type arguments
        for (source_arg, target_arg) in source_info.args.iter().zip(target_info.args.iter()) {
            self.infer_from_types(*source_arg, *target_arg, priority)?;
        }

        Ok(())
    }

    // =========================================================================
    // Task #40: Template Literal Deconstruction
    // =========================================================================

    /// Infer from template literal patterns with `infer` placeholders.
    ///
    /// This implements the "Reverse String Matcher" for extracting type information
    /// from string literals that match template patterns like `user_${infer ID}`.
    ///
    /// # Example
    ///
    /// ```typescript
    /// type GetID<T> = T extends `user_${infer ID}` ? ID : never;
    /// // GetID<"user_123"> should infer ID = "123"
    /// ```
    ///
    /// # Algorithm
    ///
    /// The matching is **non-greedy** for all segments except the last:
    /// 1. Scan through template spans sequentially
    /// 2. For text spans: match literal text at current position
    /// 3. For infer type spans: capture text until next literal anchor (non-greedy)
    /// 4. For the last span: capture all remaining text (greedy)
    ///
    /// # Arguments
    ///
    /// * `source` - The source type being checked (e.g., `"user_123"`)
    /// * `source_key` - The `TypeData` of the source (cached for efficiency)
    /// * `target_template` - The template literal pattern to match against
    /// * `priority` - Inference priority for the extracted candidates
    fn infer_from_template_literal(
        &mut self,
        source: TypeId,
        source_key: Option<&TypeData>,
        target_template: TemplateLiteralId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let spans = self.interner.template_list(target_template);

        // Special case: if source is `any` or the intrinsic `string` type, all infer vars get that type
        if source == TypeId::ANY
            || matches!(source_key, Some(TypeData::Intrinsic(IntrinsicKind::String)))
        {
            for span in spans.iter() {
                if let TemplateSpan::Type(type_id) = span
                    && let Some(TypeData::Infer(param_info)) = self.interner.lookup(*type_id)
                    && let Some(var) = self.find_type_param(param_info.name)
                {
                    // Source is `any` or `string`, so infer that for all variables
                    self.add_candidate(var, source, priority);
                }
            }
            return Ok(());
        }

        // If source is a union, try to match each member against the template
        if let Some(TypeData::Union(source_members)) = source_key {
            let members = self.interner.type_list(*source_members);
            for &member in members.iter() {
                let member_key = self.interner.lookup(member);
                self.infer_from_template_literal(
                    member,
                    member_key.as_ref(),
                    target_template,
                    priority,
                )?;
            }
            return Ok(());
        }

        // For literal string types, perform the actual pattern matching
        if let Some(source_str) = self.extract_string_literal(source)
            && let Some(captures) = self.match_template_pattern(&source_str, &spans)
        {
            // Convert captured strings to literal types and add as candidates
            for (infer_var, captured_string) in captures {
                let literal_type = self.interner.literal_string(&captured_string);
                self.add_candidate(infer_var, literal_type, priority);
            }
        }

        Ok(())
    }

    /// Extract a string literal value from a `TypeId`.
    ///
    /// Returns None if the type is not a literal string.
    fn extract_string_literal(&self, type_id: TypeId) -> Option<String> {
        match self.interner.lookup(type_id) {
            Some(TypeData::Literal(LiteralValue::String(s))) => Some(self.interner.resolve_atom(s)),
            _ => None,
        }
    }

    /// Match a source string against a template pattern, extracting infer variable bindings.
    ///
    /// # Arguments
    ///
    /// * `source` - The source string to match (e.g., `"user_123"`)
    /// * `spans` - The template spans (e.g., `[Text("user_"), Type(ID), Text("_")]`)
    ///
    /// # Returns
    ///
    /// * `Some(bindings)` - Mapping from inference variables to captured strings
    /// * `None` - The source doesn't match the pattern
    fn match_template_pattern(
        &self,
        source: &str,
        spans: &[TemplateSpan],
    ) -> Option<Vec<(InferenceVar, String)>> {
        let mut bindings = Vec::new();
        let mut pos = 0;

        for (i, span) in spans.iter().enumerate() {
            let is_last = i == spans.len() - 1;

            match span {
                TemplateSpan::Text(text_atom) => {
                    // Match literal text at current position
                    let text = self.interner.resolve_atom(*text_atom).to_string();
                    if !source.get(pos..)?.starts_with(&text) {
                        return None; // Text doesn't match
                    }
                    pos += text.len();
                }

                TemplateSpan::Type(type_id) => {
                    // Check if this is an infer variable
                    if let Some(TypeData::Infer(param_info)) = self.interner.lookup(*type_id)
                        && let Some(var) = self.find_type_param(param_info.name)
                    {
                        if is_last {
                            // Last span: capture all remaining text (greedy)
                            let captured = source[pos..].to_string();
                            bindings.push((var, captured));
                            pos = source.len();
                        } else {
                            // Non-last span: capture until next literal anchor (non-greedy)
                            // Find the next text span to use as an anchor
                            if let Some(anchor_text) = self.find_next_text_anchor(spans, i) {
                                let anchor = self.interner.resolve_atom(anchor_text).to_string();
                                // Find the first occurrence of the anchor (non-greedy)
                                let capture_end = source[pos..].find(&anchor)? + pos;
                                let captured = source[pos..capture_end].to_string();
                                bindings.push((var, captured));
                                pos = capture_end;
                            } else {
                                // No text anchor found (e.g., `${infer A}${infer B}`)
                                // Capture empty string for non-greedy match and continue
                                bindings.push((var, String::new()));
                                // pos remains unchanged - next infer var starts here
                            }
                        }
                    }
                }
            }
        }

        // Must have consumed the entire source string
        (pos == source.len()).then_some(bindings)
    }

    /// Find the next text span after a given index to use as a matching anchor.
    fn find_next_text_anchor(&self, spans: &[TemplateSpan], start_idx: usize) -> Option<Atom> {
        spans.iter().skip(start_idx + 1).find_map(|span| {
            if let TemplateSpan::Text(text) = span {
                Some(*text)
            } else {
                None
            }
        })
    }
}

// DISABLED: Tests use deprecated add_candidate / resolve_with_constraints API
// The inference system has been refactored to use unification-based inference.
#[cfg(test)]
#[path = "../tests/infer_tests.rs"]
mod tests;
