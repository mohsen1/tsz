use crate::contextual::extractors::extract_param_type_at_for_call;
use crate::diagnostics::PendingDiagnostic;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type_cached};
use crate::types::{
    CallSignature, CallableShapeId, FunctionShape, FunctionShapeId, IntrinsicKind, LiteralValue,
    ParamInfo, TypeData, TypeId, TypeListId, TypePredicate,
};
use crate::visitor::TypeVisitor;
use crate::{QueryDatabase, TypeDatabase};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::{Cell, RefCell};
use tracing::debug;

/// Result of `resolve_call_with_checker`: the call result, an optional
/// instantiated type predicate (for narrowing), and optional instantiated
/// parameter types (for post-inference excess property checking).
pub type CallWithCheckerResult = (
    CallResult,
    Option<(TypePredicate, Vec<ParamInfo>)>,
    Option<Vec<ParamInfo>>,
);

/// Maximum recursion depth for type constraint collection to prevent infinite loops.
pub const MAX_CONSTRAINT_RECURSION_DEPTH: usize = 100;
/// Maximum number of constrain-types steps per call evaluator pass.
/// This caps pathological recursive inference explosions while preserving
/// normal inference behavior on real-world calls.
pub(crate) const MAX_CONSTRAINT_STEPS: usize = 20_000;

pub trait AssignabilityChecker {
    fn is_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool;

    fn is_assignable_to_strict(&mut self, source: TypeId, target: TypeId) -> bool {
        self.is_assignable_to(source, target)
    }

    /// Assignability check for bivariant callback parameters.
    ///
    /// This is used for method parameter positions where TypeScript allows
    /// bivariant checking for function-typed callbacks.
    fn is_assignable_to_bivariant_callback(&mut self, source: TypeId, target: TypeId) -> bool {
        self.is_assignable_to(source, target)
    }

    /// Evaluate/expand a type using the checker's resolver context.
    /// This is needed during inference constraint collection, where Application types
    /// like `Func<T>` must be expanded to their structural form (e.g., a Callable).
    /// The default implementation returns the type unchanged (no resolver available).
    fn evaluate_type(&mut self, type_id: TypeId) -> TypeId {
        type_id
    }

    /// Expand a type alias Application to its body type with the given type arguments.
    ///
    /// For `Application(Lazy(DefId), [args...])`, resolves the DefId to its body type
    /// and instantiates it with the provided args. Returns `None` if the type is not
    /// an expandable Application or the base can't be resolved.
    ///
    /// This is used during inference to expand mapped type aliases (e.g., `TupleMapper<T>`)
    /// without evaluating the result (which would resolve inference variables to their
    /// constraints). The default returns `None` (no resolver available).
    fn expand_type_alias_application(&mut self, _type_id: TypeId) -> Option<TypeId> {
        None
    }

    /// Extract the element type from a promise-like application when the checker
    /// has enough semantic context to do so.
    ///
    /// This is used during generic inference for return-position relationships
    /// like `Promise<number> <: Promise<U>`, especially when the two applications
    /// are represented with different bases (`Lazy` vs `TypeQuery`) and structural
    /// expansion alone loses the type argument connection.
    fn promise_like_type_argument(&mut self, _type_id: TypeId) -> Option<TypeId> {
        None
    }

    /// Get a type resolver for variance computation and type parameter lookup.
    ///
    /// This is used during inference constraint collection to compute the variance
    /// of type parameters in type alias Applications. The checker implements this
    /// to provide its full resolver context.
    fn type_resolver(&self) -> Option<&dyn crate::TypeResolver> {
        None
    }

    /// Check if two types are structurally identical (tsc's `compareTypesIdentical`).
    ///
    /// Used for union signature compatibility where `this` types must be identical
    /// across union members. The default uses mutual assignability, but checkers
    /// should override to use their full type environment for Lazy type resolution.
    fn are_types_identical(&mut self, a: TypeId, b: TypeId) -> bool {
        if a == b {
            return true;
        }
        self.is_assignable_to(a, b) && self.is_assignable_to(b, a)
    }
}

// =============================================================================
// Function Call Resolution
// =============================================================================

/// Result of attempting to call a function type.
#[derive(Clone, Debug)]
pub enum CallResult {
    /// Call succeeded, returns the result type
    Success(TypeId),

    /// Not a callable type
    NotCallable { type_id: TypeId },

    /// `this` type mismatch
    ThisTypeMismatch {
        expected_this: TypeId,
        actual_this: TypeId,
        emit_not_callable: bool,
    },

    /// Argument count mismatch
    ArgumentCountMismatch {
        expected_min: usize,
        expected_max: Option<usize>,
        actual: usize,
    },

    /// Overloaded call with arity "gap": no overload matches this exact arity,
    /// but overloads exist for two surrounding fixed arities (TS2575).
    OverloadArgumentCountMismatch {
        actual: usize,
        expected_low: usize,
        expected_high: usize,
    },

    /// Argument type mismatch at specific position
    ArgumentTypeMismatch {
        index: usize,
        expected: TypeId,
        actual: TypeId,
        // Return type to continue checking after a mismatch in type-position checks.
        // Keeps downstream diagnostics (e.g. TS2339 on call results) available even
        // when the call itself is invalid with TS2345.
        fallback_return: TypeId,
    },

    /// TS2350: Only a void function can be called with the 'new' keyword.
    NonVoidFunctionCalledWithNew,

    /// A void-returning function was called with `new`. The result is `any`.
    /// Distinct from `Success(ANY)` so the checker can emit TS7009 (noImplicitAny)
    /// only for functions lacking construct signatures, not for types that
    /// legitimately return `any` from construct signatures.
    VoidFunctionCalledWithNew,

    /// Type parameter constraint violation (TS2322, not TS2345).
    /// Used when inference from callback return types produces a type that
    /// violates the type parameter's constraint. tsc reports TS2322 on the
    /// return expression, not TS2345 on the whole callback argument.
    TypeParameterConstraintViolation {
        /// The inferred type that violated the constraint
        inferred_type: TypeId,
        /// The constraint type that was violated
        constraint_type: TypeId,
        /// The return type of the call (for type computation to continue)
        return_type: TypeId,
    },

    /// No overload matched (for overloaded functions)
    NoOverloadMatch {
        func_type: TypeId,
        arg_types: Vec<TypeId>,
        failures: Vec<PendingDiagnostic>,
        fallback_return: TypeId,
    },
}

/// Evaluates function calls.
pub struct CallEvaluator<'a, C: AssignabilityChecker> {
    pub(crate) interner: &'a dyn QueryDatabase,
    pub(crate) checker: &'a mut C,
    pub(crate) defaulted_placeholders: FxHashSet<TypeId>,
    pub(in crate::operations) force_bivariant_callbacks: bool,
    /// Contextual type for the call expression's expected result
    /// Used for contextual type inference in generic functions
    pub(crate) contextual_type: Option<TypeId>,
    /// The `this` type provided by the caller (e.g. `obj` in `obj.method()`)
    pub(crate) actual_this_type: Option<TypeId>,
    /// Current recursion depth for `constrain_types` to prevent infinite loops
    pub(crate) constraint_recursion_depth: Cell<usize>,
    /// Total constrain-types steps for the current inference pass.
    pub(crate) constraint_step_count: Cell<usize>,
    /// Visited (source, target) pairs during constraint collection.
    pub(crate) constraint_pairs: RefCell<FxHashSet<(TypeId, TypeId)>>,
    /// Memoized fixed members for target union types during one inference pass.
    /// Keyed by the union `TypeId` used as the target in constrain-types.
    pub(crate) constraint_fixed_union_members: RefCell<FxHashMap<TypeId, FxHashSet<TypeId>>>,
    /// After a generic call resolves, holds the instantiated type predicate (if any).
    /// This lets the checker retrieve the predicate with inferred type arguments applied.
    pub last_instantiated_predicate: Option<(TypePredicate, Vec<ParamInfo>)>,
    /// After a generic call resolves, holds the instantiated parameter types.
    /// This lets the checker perform post-inference excess property checking on
    /// the concrete parameter types rather than the raw (pre-inference) types.
    pub last_instantiated_params: Option<Vec<ParamInfo>>,
    /// Memoization cache for `is_contextually_sensitive` to avoid exponential
    /// re-traversal on deeply nested type structures (e.g., long instantiation chains
    /// where each Application type references the previous one multiple times).
    pub(crate) contextual_sensitivity_cache: RefCell<FxHashMap<TypeId, bool>>,
    /// Recursion depth for reverse mapped type inference through mapped type templates.
    /// Used as a hard cap to prevent runaway recursion (in addition to the
    /// `reverse_mapped_visited` set which short-circuits true recursive patterns).
    pub(crate) reverse_mapped_depth: Cell<u32>,
    /// Set of `(mapped_template_id, source_value_id)` pairs currently being reverse-inferred.
    /// When we re-enter `reverse_infer_through_template` Case 6 with a pair we've already seen
    /// in the current chain, we've hit a true recursive type pattern (e.g., `Deep<T>` against
    /// a self-referential interface like `interface A { a: A }`). In that case we short-circuit
    /// to the source value itself, matching tsc's lazy `ReverseMappedType` convergence.
    /// Distinct pairs (different source sub-objects) are still allowed to recurse so that
    /// finite sources like `{Test: {Test1: {Test2: leaf}}}` reverse-map through every level.
    pub(crate) reverse_mapped_visited: RefCell<FxHashSet<(TypeId, TypeId)>>,
}

#[derive(Clone, Copy)]
pub(super) enum UnionCallSignatureCompatibility {
    Compatible {
        min_required: usize,
        max_allowed: Option<usize>,
    },
    Incompatible,
    Unknown,
}

/// Combined call signature computed from a union of callable types.
///
/// TypeScript computes a combined signature for unions where each member has
/// exactly one call signature (non-generic). The combined signature intersects
/// parameter types (contravariant) and unions return types:
///
/// ```text
/// { (a: number): string } | { (a: boolean): Date }
///   → combined: (a: number & boolean): string | Date
///              = (a: never): string | Date
/// ```
pub(crate) struct CombinedUnionSignature {
    /// Intersected parameter types at each position
    pub(crate) param_types: Vec<TypeId>,
    /// Minimum required arguments (max of all members' required counts)
    pub(crate) min_required: usize,
    /// Maximum allowed arguments (None if unbounded / has rest)
    pub(crate) max_allowed: Option<usize>,
    /// Unioned return type from all members
    pub(crate) return_type: TypeId,
}

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    pub fn new(interner: &'a dyn QueryDatabase, checker: &'a mut C) -> Self {
        CallEvaluator {
            interner,
            checker,
            defaulted_placeholders: FxHashSet::default(),
            force_bivariant_callbacks: false,
            contextual_type: None,
            actual_this_type: None,
            constraint_recursion_depth: Cell::new(0),
            constraint_step_count: Cell::new(0),
            constraint_pairs: RefCell::new(FxHashSet::default()),
            constraint_fixed_union_members: RefCell::new(FxHashMap::default()),
            last_instantiated_predicate: None,
            last_instantiated_params: None,
            contextual_sensitivity_cache: RefCell::new(FxHashMap::default()),
            reverse_mapped_depth: Cell::new(0),
            reverse_mapped_visited: RefCell::new(FxHashSet::default()),
        }
    }

    /// Set the actual `this` type for the call evaluation.
    pub const fn set_actual_this_type(&mut self, type_id: Option<TypeId>) {
        self.actual_this_type = type_id;
    }

    /// Set the contextual type for this call evaluation.
    /// This is used for contextual type inference when the expected return type
    /// can help constrain generic type parameters.
    /// Example: `let x: string = id(42)` should infer `T = string` from the context.
    pub const fn set_contextual_type(&mut self, ctx_type: Option<TypeId>) {
        self.contextual_type = ctx_type;
    }

    pub const fn set_force_bivariant_callbacks(&mut self, enabled: bool) {
        self.force_bivariant_callbacks = enabled;
    }

    pub(crate) fn is_function_union_compat(
        &mut self,
        arg_type: TypeId,
        mut target_type: TypeId,
    ) -> bool {
        if let Some(TypeData::Lazy(def_id)) = self.interner.lookup(target_type)
            && let Some(resolved) = self.interner.resolve_lazy(def_id, self.interner)
        {
            target_type = resolved;
            debug!(
                target_type = target_type.0,
                target_key = ?self.interner.lookup(target_type),
                "is_function_union_compat: resolved lazy target"
            );
        }
        if !matches!(self.interner.lookup(target_type), Some(TypeData::Union(_))) {
            let evaluated = self.interner.evaluate_type(target_type);
            if evaluated != target_type {
                target_type = evaluated;
                debug!(
                    target_type = target_type.0,
                    target_key = ?self.interner.lookup(target_type),
                    "is_function_union_compat: evaluated target"
                );
            }
            if let Some(TypeData::Lazy(def_id)) = self.interner.lookup(target_type)
                && let Some(resolved) = self.interner.resolve_lazy(def_id, self.interner)
            {
                target_type = resolved;
                debug!(
                    target_type = target_type.0,
                    target_key = ?self.interner.lookup(target_type),
                    "is_function_union_compat: resolved lazy target after eval"
                );
            }
        }
        let Some(TypeData::Union(members_id)) = self.interner.lookup(target_type) else {
            return false;
        };
        if !crate::type_queries::is_callable_type(self.interner, arg_type) {
            return false;
        }
        let members = self.interner.type_list(members_id);
        if members
            .iter()
            .any(|&member| self.checker.is_assignable_to(arg_type, member))
        {
            return true;
        }
        let synthetic_any_fn = self.interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            return_type: TypeId::ANY,
            this_type: None,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        if members
            .iter()
            .any(|&member| self.checker.is_assignable_to(synthetic_any_fn, member))
        {
            return true;
        }
        members
            .iter()
            .any(|&member| self.is_function_like_union_member(member))
    }

    pub(super) fn normalize_union_member(&self, mut member: TypeId) -> TypeId {
        for _ in 0..8 {
            let next = match self.interner.lookup(member) {
                Some(TypeData::Lazy(def_id)) => self
                    .interner
                    .resolve_lazy(def_id, self.interner)
                    .unwrap_or(member),
                Some(TypeData::Application(_) | TypeData::Mapped(_)) => {
                    self.interner.evaluate_type(member)
                }
                _ => member,
            };
            if next == member {
                break;
            }
            member = next;
        }
        member
    }

    /// Collect per-member call signature lists for a union.
    ///
    /// Returns a vec of (`member_index`, `call_signatures`) for each callable union member.
    /// Non-callable members are skipped.
    pub(super) fn collect_union_call_signature_lists(
        &self,
        members: &[TypeId],
    ) -> Vec<(usize, Vec<CallSignature>)> {
        let mut result = Vec::new();
        for (i, &member) in members.iter().enumerate() {
            let member = self.normalize_union_member(member);
            match self.interner.lookup(member) {
                Some(TypeData::Function(func_id)) => {
                    let func = self.interner.function_shape(func_id);
                    let sig = CallSignature {
                        type_params: func.type_params.clone(),
                        params: func.params.clone(),
                        this_type: func.this_type,
                        return_type: func.return_type,
                        type_predicate: func.type_predicate,
                        is_method: func.is_method,
                    };
                    result.push((i, vec![sig]));
                }
                Some(TypeData::Callable(callable_id)) => {
                    let callable = self.interner.callable_shape(callable_id);
                    if !callable.call_signatures.is_empty() {
                        result.push((i, callable.call_signatures.clone()));
                    }
                }
                _ => {}
            }
        }
        result
    }

    /// Check if two non-generic call signatures are structurally compatible for
    /// union signature combination (tsc's `compareSignaturesIdentical` with
    /// `partialMatch=true`, `ignoreReturnTypes=true`).
    ///
    /// Two signatures are compatible when they have the same number of required
    /// parameters (allowing extra optional params) and their type positions are
    /// identical under the checker-backed type identity hook.
    pub(super) fn are_signatures_compatible_for_union(
        &mut self,
        a: &CallSignature,
        b: &CallSignature,
    ) -> bool {
        if !self.are_signature_params_compatible_for_union(a, b) {
            return false;
        }

        // Check this types match. A missing `this` type does not constrain the
        // merged union signature, matching tsc's compareSignaturesIdentical path.
        match (a.this_type, b.this_type) {
            (Some(a_this), Some(b_this)) => self.checker.are_types_identical(a_this, b_this),
            _ => true,
        }
    }

    pub(super) fn are_signature_params_compatible_for_union(
        &mut self,
        a: &CallSignature,
        b: &CallSignature,
    ) -> bool {
        // Generic signatures require exact match — skip them for now
        if !a.type_params.is_empty() || !b.type_params.is_empty() {
            return false;
        }

        // Compare required parameter count
        let a_required = a.params.iter().filter(|p| p.is_required()).count();
        let b_required = b.params.iter().filter(|p| p.is_required()).count();
        if a_required != b_required {
            return false;
        }

        // Total parameter count must be compatible (partial match allows extra optional)
        // Use the minimum total — both must have at least that many params
        let min_total = a.params.len().min(b.params.len());

        // Check parameter types are identical for overlapping positions. Use the
        // checker hook instead of raw TypeId equality so aliases/lazy refs that
        // resolve to the same semantic type can still participate in tsc's
        // union-signature merging.
        for i in 0..min_total {
            if !self
                .checker
                .are_types_identical(a.params[i].type_id, b.params[i].type_id)
            {
                return false;
            }
        }

        true
    }

    /// Find compatible signatures across all union members, mimicking tsc's
    /// `getUnionSignatures`.
    ///
    /// Returns `Some(signatures)` if compatible cross-member signatures exist,
    /// `None` if no compatible set was found.
    ///
    /// When members have multiple overloads, this checks if there's at least one
    /// signature from each member that is compatible with a signature from every
    /// other member. If multiple members have multiple overloads and no compatible
    /// pair exists, returns `None` → the union is not callable (TS2349).
    pub(super) fn find_union_compatible_signatures(
        &mut self,
        sig_lists: &[(usize, Vec<CallSignature>)],
    ) -> Option<Vec<CallSignature>> {
        if sig_lists.is_empty() {
            return None;
        }

        let mut result: Vec<CallSignature> = Vec::new();

        // Count how many members have multiple overloads
        let mut multi_overload_count = 0;
        let mut single_overload_with_multi_idx: Option<usize> = None;
        for (i, (_, sigs)) in sig_lists.iter().enumerate() {
            if sigs.len() > 1 {
                multi_overload_count += 1;
                if single_overload_with_multi_idx.is_none() {
                    single_overload_with_multi_idx = Some(i);
                } else {
                    // Multiple members with overloads — use -1 sentinel
                    single_overload_with_multi_idx = None;
                }
            }
        }

        // Phase 1: Try to find matching signatures across all lists
        // For each signature in each member's list, check if there's a compatible
        // signature in every other member's list.
        for (list_idx, (_, sigs)) in sig_lists.iter().enumerate() {
            for sig in sigs {
                // Skip generic signatures (require exact match, only from first list)
                if !sig.type_params.is_empty() {
                    continue;
                }

                // Check if this signature already has a match in our result
                if result
                    .iter()
                    .any(|r| self.are_signatures_compatible_for_union(r, sig))
                {
                    continue;
                }

                // Try to find a matching signature in every other list
                let mut union_sigs: Vec<&CallSignature> = vec![sig];
                let mut all_match = true;

                for (other_idx, (_, other_sigs)) in sig_lists.iter().enumerate() {
                    if other_idx == list_idx {
                        continue;
                    }
                    // Find a compatible signature in this other list (try exact first, then partial)
                    let matching = other_sigs
                        .iter()
                        .find(|other| self.are_signatures_compatible_for_union(sig, other));
                    if let Some(m) = matching {
                        union_sigs.push(m);
                    } else {
                        all_match = false;
                        break;
                    }
                }

                if all_match {
                    // Create a combined signature: union return types, intersect this types
                    let mut combined_this: Option<TypeId> = sig.this_type;
                    let mut return_types = vec![sig.return_type];

                    for &matched_sig in &union_sigs[1..] {
                        return_types.push(matched_sig.return_type);
                        if let Some(this_type) = matched_sig.this_type {
                            combined_this = Some(match combined_this {
                                Some(existing) => self.interner.intersection2(existing, this_type),
                                None => this_type,
                            });
                        }
                    }

                    let union_return = if return_types.len() == 1 {
                        return_types[0]
                    } else {
                        self.interner.union(return_types)
                    };

                    result.push(CallSignature {
                        type_params: Vec::new(),
                        params: sig.params.clone(),
                        this_type: combined_this,
                        return_type: union_return,
                        type_predicate: None,
                        is_method: sig.is_method,
                    });
                }
            }
        }

        if !result.is_empty() {
            return Some(result);
        }

        // Phase 2: If only ONE member has multiple overloads, use that member's
        // overloads as the base and combine each with the single-signature members.
        // But first verify that each single-overload member is compatible with at
        // least one of the multi-overload member's signatures (per tsc's
        // intersectSignatureSets). If not, the union is not callable.
        if multi_overload_count == 1
            && let Some(master_idx) = single_overload_with_multi_idx
        {
            let (_, master_sigs) = &sig_lists[master_idx];

            // Verify each single-overload member has a compatible match
            // in the multi-overload member's signatures.
            for (other_idx, (_, other_sigs)) in sig_lists.iter().enumerate() {
                if other_idx == master_idx {
                    continue;
                }
                if let Some(other_sig) = other_sigs.first() {
                    let has_match = master_sigs
                        .iter()
                        .any(|ms| self.are_signatures_compatible_for_union(other_sig, ms));
                    if !has_match {
                        // Single-overload member is incompatible with all
                        // overloads of the multi-overload member → not callable.
                        return None;
                    }
                }
            }

            let mut combined_results: Vec<CallSignature> = master_sigs.clone();

            for (other_idx, (_, other_sigs)) in sig_lists.iter().enumerate() {
                if other_idx == master_idx {
                    continue;
                }
                // Single-signature member — combine with each master overload
                if let Some(other_sig) = other_sigs.first() {
                    if !other_sig.type_params.is_empty() {
                        return None; // Can't combine generic
                    }
                    for combined in &mut combined_results {
                        // Intersect this types
                        if let Some(other_this) = other_sig.this_type {
                            combined.this_type = Some(match combined.this_type {
                                Some(existing) => self.interner.intersection2(existing, other_this),
                                None => other_this,
                            });
                        }
                        // Union return types
                        combined.return_type = self
                            .interner
                            .factory()
                            .union2(combined.return_type, other_sig.return_type);
                    }
                }
            }

            return Some(combined_results);
        }

        // Multiple members with multiple overloads and no compatible pair → not callable
        None
    }

    /// Compute the combined `this` type for a union of callable types.
    ///
    /// In TypeScript, when calling a union type, the `this` context must satisfy
    /// ALL members' `this` requirements. The combined `this` type is the intersection
    /// of all members' `this` types. If no member has a `this` type, returns `None`.
    ///
    /// Conservative: only extracts `this` from single-signature functions/callables.
    /// Multi-overload callables are skipped because their `this` depends on which
    /// overload is selected during resolution, and any overload may satisfy the
    /// calling context.
    pub(super) fn compute_union_this_type(&self, members: &[TypeId]) -> Option<TypeId> {
        let mut this_types = Vec::new();

        for &member in members {
            let member = self.normalize_union_member(member);
            match self.interner.lookup(member) {
                Some(TypeData::Function(func_id)) => {
                    let function = self.interner.function_shape(func_id);
                    if let Some(this_type) = function.this_type {
                        this_types.push(this_type);
                    }
                }
                Some(TypeData::Callable(callable_id)) => {
                    let callable = self.interner.callable_shape(callable_id);
                    // Only consider single-overload callables. Multi-overload
                    // callables have per-overload this types that depend on
                    // overload resolution, so we can't pre-compute a combined
                    // this type for them.
                    if callable.call_signatures.len() == 1
                        && let Some(this_type) = callable.call_signatures[0].this_type
                    {
                        this_types.push(this_type);
                    }
                }
                _ => {
                    // Non-callable member or member without this type — doesn't constrain
                }
            }
        }

        if this_types.is_empty() {
            return None;
        }

        // Intersect all this types
        let mut result = this_types[0];
        for &this_type in &this_types[1..] {
            result = self.interner.intersection2(result, this_type);
        }
        Some(result)
    }

    fn is_function_like_union_member(&self, member: TypeId) -> bool {
        let member = self.normalize_union_member(member);
        match self.interner.lookup(member) {
            Some(TypeData::Intrinsic(IntrinsicKind::Function))
            | Some(TypeData::Function(_) | TypeData::Callable(_)) => true,
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                let apply = self.interner.intern_string("apply");
                let call = self.interner.intern_string("call");
                let has_apply = shape.properties.iter().any(|prop| prop.name == apply);
                let has_call = shape.properties.iter().any(|prop| prop.name == call);
                has_apply && has_call
            }
            Some(TypeData::Union(members_id)) => self
                .interner
                .type_list(members_id)
                .iter()
                .any(|&m| self.is_function_like_union_member(m)),
            Some(TypeData::Intersection(members_id)) => self
                .interner
                .type_list(members_id)
                .iter()
                .any(|&m| self.is_function_like_union_member(m)),
            _ => false,
        }
    }

    pub fn infer_call_signature(&mut self, sig: &CallSignature, arg_types: &[TypeId]) -> TypeId {
        let func = FunctionShape {
            params: sig.params.clone(),
            this_type: sig.this_type,
            return_type: sig.return_type,
            type_params: sig.type_params.clone(),
            type_predicate: sig.type_predicate,
            is_constructor: false,
            is_method: sig.is_method,
        };
        match self.resolve_function_call(&func, arg_types) {
            CallResult::Success(ret) => ret,
            // Return ERROR instead of ANY to avoid silencing TS2322 errors
            _ => TypeId::ERROR,
        }
    }

    pub fn infer_generic_function(&mut self, func: &FunctionShape, arg_types: &[TypeId]) -> TypeId {
        match self.resolve_function_call(func, arg_types) {
            CallResult::Success(ret) => ret,
            // Return ERROR instead of ANY to avoid silencing TS2322 errors
            _ => TypeId::ERROR,
        }
    }

    /// Retrieves the contextual function signature from a type.
    ///
    /// This is used to infer parameter types for function expressions.
    /// e.g., given `let x: (a: string) => void = (a) => ...`, this returns
    /// the shape of `(a: string) => void` so we can infer `a` is `string`.
    ///
    /// # Arguments
    /// * `db` - The type database
    /// * `type_id` - The contextual type to extract a signature from
    ///
    /// # Returns
    /// * `Some(FunctionShape)` if the type suggests a function structure
    /// * `None` if the type is not callable or has no suitable signature
    pub fn get_contextual_signature(
        db: &dyn TypeDatabase,
        type_id: TypeId,
    ) -> Option<FunctionShape> {
        Self::get_contextual_signature_for_arity(db, type_id, None)
    }

    pub fn get_contextual_signature_cached(
        db: &dyn QueryDatabase,
        type_id: TypeId,
    ) -> Option<FunctionShape> {
        Self::get_contextual_signature_for_arity_inner(db, Some(db), type_id, None)
    }

    /// Get the contextual signature for a type, optionally filtering by argument count.
    /// When `arg_count` is provided, selects the first overload whose arity matches.
    pub(super) fn get_contextual_signature_for_arity(
        db: &dyn TypeDatabase,
        type_id: TypeId,
        arg_count: Option<usize>,
    ) -> Option<FunctionShape> {
        Self::get_contextual_signature_for_arity_inner(db, None, type_id, arg_count)
    }

    pub(super) fn get_contextual_signature_for_arity_cached(
        db: &dyn QueryDatabase,
        type_id: TypeId,
        arg_count: Option<usize>,
    ) -> Option<FunctionShape> {
        Self::get_contextual_signature_for_arity_inner(db, Some(db), type_id, arg_count)
    }

    fn get_contextual_signature_for_arity_inner(
        db: &dyn TypeDatabase,
        query_db: Option<&dyn QueryDatabase>,
        type_id: TypeId,
        arg_count: Option<usize>,
    ) -> Option<FunctionShape> {
        fn from_call_signature(sig: &CallSignature) -> FunctionShape {
            FunctionShape {
                type_params: sig.type_params.clone(),
                params: sig.params.clone(),
                this_type: sig.this_type,
                return_type: sig.return_type,
                type_predicate: sig.type_predicate,
                is_constructor: false,
                is_method: sig.is_method,
            }
        }

        fn combine_contextual_signatures(
            db: &dyn TypeDatabase,
            signatures: Vec<&CallSignature>,
            arg_count: Option<usize>,
        ) -> Option<FunctionShape> {
            let first = *signatures.first()?;
            if signatures.len() == 1 {
                return Some(from_call_signature(first));
            }

            // Mixed-arity overload sets cannot be safely flattened into a single
            // contextual signature. Doing so widens shorter overloads through
            // trailing optional parameters and breaks generic callback/constructor
            // matching that depends on the original overload boundaries.
            if signatures
                .iter()
                .any(|sig| sig.params.len() != first.params.len())
            {
                return None;
            }

            // tsc's getIntersectedSignatures returns undefined when multiple
            // signatures are present and ANY has type parameters. This prevents
            // contextual typing of arrow functions assigned to overloaded types
            // with both generic and non-generic call signatures.
            if signatures.iter().any(|sig| !sig.type_params.is_empty()) {
                return None;
            }

            let effective_arg_count = arg_count.unwrap_or_else(|| {
                signatures
                    .iter()
                    .map(|sig| sig.params.len())
                    .max()
                    .unwrap_or(0)
            });

            let params = (0..effective_arg_count)
                .filter_map(|index| {
                    let mut param_types: Vec<TypeId> = signatures
                        .iter()
                        .filter_map(|sig| {
                            extract_param_type_at_for_call(
                                db,
                                &sig.params,
                                index,
                                effective_arg_count,
                            )
                        })
                        .collect();
                    if param_types.len() > 1 && param_types.iter().any(|&ty| ty != TypeId::ANY) {
                        param_types.retain(|&ty| ty != TypeId::ANY);
                    }
                    match param_types.len() {
                        0 => None,
                        1 => Some(ParamInfo::unnamed(param_types[0])),
                        _ => Some(ParamInfo::unnamed(db.union_literal_reduce(param_types))),
                    }
                })
                .collect();

            let mut return_types: Vec<TypeId> =
                signatures.iter().map(|sig| sig.return_type).collect();
            if return_types.len() > 1 && return_types.iter().any(|&ty| ty != TypeId::ANY) {
                return_types.retain(|&ty| ty != TypeId::ANY);
            }
            let return_type = match return_types.len() {
                0 => first.return_type,
                1 => return_types[0],
                _ => db.union_literal_reduce(return_types),
            };

            let type_params = if signatures
                .iter()
                .all(|sig| sig.type_params == first.type_params)
            {
                first.type_params.clone()
            } else {
                Vec::new()
            };

            let this_type = if signatures
                .iter()
                .all(|sig| sig.this_type == first.this_type)
            {
                first.this_type
            } else {
                let this_types: Vec<_> =
                    signatures.iter().filter_map(|sig| sig.this_type).collect();
                match this_types.len() {
                    0 => None,
                    1 => Some(this_types[0]),
                    _ => Some(db.union_literal_reduce(this_types)),
                }
            };

            let type_predicate = if signatures
                .iter()
                .all(|sig| sig.type_predicate == first.type_predicate)
            {
                first.type_predicate
            } else {
                None
            };

            let is_method = signatures.iter().all(|sig| sig.is_method);

            Some(FunctionShape {
                type_params,
                params,
                this_type,
                return_type,
                type_predicate,
                is_constructor: false,
                is_method,
            })
        }

        fn combine_function_shapes(
            db: &dyn TypeDatabase,
            shapes: Vec<FunctionShape>,
            arg_count: Option<usize>,
        ) -> Option<FunctionShape> {
            let first = shapes.first()?;
            if shapes.len() == 1 {
                return Some(first.clone());
            }

            if shapes.iter().any(|shape| !shape.type_params.is_empty()) {
                return None;
            }

            let effective_arg_count = arg_count.unwrap_or_else(|| {
                shapes
                    .iter()
                    .map(|shape| shape.params.len())
                    .max()
                    .unwrap_or(0)
            });

            let params = (0..effective_arg_count)
                .filter_map(|index| {
                    let mut param_types: Vec<TypeId> = shapes
                        .iter()
                        .filter_map(|shape| {
                            extract_param_type_at_for_call(
                                db,
                                &shape.params,
                                index,
                                effective_arg_count,
                            )
                        })
                        .collect();
                    if param_types.len() > 1 && param_types.iter().any(|&ty| ty != TypeId::ANY) {
                        param_types.retain(|&ty| ty != TypeId::ANY);
                    }
                    match param_types.len() {
                        0 => None,
                        1 => Some(ParamInfo::unnamed(param_types[0])),
                        _ => Some(ParamInfo::unnamed(db.union_literal_reduce(param_types))),
                    }
                })
                .collect();

            let mut return_types: Vec<TypeId> =
                shapes.iter().map(|shape| shape.return_type).collect();
            if return_types.len() > 1 && return_types.iter().any(|&ty| ty != TypeId::ANY) {
                return_types.retain(|&ty| ty != TypeId::ANY);
            }
            let return_type = match return_types.len() {
                0 => first.return_type,
                1 => return_types[0],
                _ => db.union_literal_reduce(return_types),
            };

            let this_type = if shapes
                .iter()
                .all(|shape| shape.this_type == first.this_type)
            {
                first.this_type
            } else {
                let this_types: Vec<_> =
                    shapes.iter().filter_map(|shape| shape.this_type).collect();
                match this_types.len() {
                    0 => None,
                    1 => Some(this_types[0]),
                    _ => Some(db.union_literal_reduce(this_types)),
                }
            };

            let type_predicate = if shapes
                .iter()
                .all(|shape| shape.type_predicate == first.type_predicate)
            {
                first.type_predicate
            } else {
                None
            };

            Some(FunctionShape {
                type_params: Vec::new(),
                params,
                this_type,
                return_type,
                type_predicate,
                is_constructor: shapes.iter().all(|shape| shape.is_constructor),
                is_method: shapes.iter().all(|shape| shape.is_method),
            })
        }

        fn signatures_match_for_contextual_union(
            left: &FunctionShape,
            right: &FunctionShape,
        ) -> bool {
            if left.type_params != right.type_params || left.params.len() != right.params.len() {
                return false;
            }

            left.params.iter().zip(right.params.iter()).all(|(l, r)| {
                l.type_id == r.type_id && l.optional == r.optional && l.rest == r.rest
            })
        }

        struct ContextualSignatureVisitor<'a> {
            db: &'a dyn TypeDatabase,
            query_db: Option<&'a dyn QueryDatabase>,
            arg_count: Option<usize>,
            // Cycle guard: resolving a Lazy/Ref back into an Application whose
            // base resolves back to the same node can loop forever. Track the
            // types currently on the visit stack and bail with None when we
            // re-enter.
            visiting: rustc_hash::FxHashSet<TypeId>,
        }

        impl<'a> ContextualSignatureVisitor<'a> {
            fn visit_guarded(&mut self, type_id: TypeId) -> Option<FunctionShape> {
                if !self.visiting.insert(type_id) {
                    return None;
                }
                let result = self.visit_type(self.db, type_id);
                self.visiting.remove(&type_id);
                result
            }
        }

        impl<'a> TypeVisitor for ContextualSignatureVisitor<'a> {
            type Output = Option<FunctionShape>;

            fn default_output() -> Self::Output {
                None
            }

            fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
                None
            }

            fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
                None
            }

            fn visit_ref(&mut self, ref_id: u32) -> Self::Output {
                // Resolve the reference by converting to TypeId and recursing
                // This handles named types like `type Handler<T> = ...`
                self.visit_guarded(TypeId(ref_id))
            }

            fn visit_lazy(&mut self, def_id: u32) -> Self::Output {
                // Resolve Lazy(DefId) types (interfaces, classes, type aliases)
                // so that Application types with a Lazy base can extract their
                // contextual signature for generic inference.
                let resolved = crate::evaluation::evaluate::evaluate_type(self.db, TypeId(def_id));
                if resolved != TypeId(def_id) {
                    self.visit_guarded(resolved)
                } else {
                    None
                }
            }

            fn visit_function(&mut self, shape_id: u32) -> Self::Output {
                // Direct match: return the function shape
                let shape = self.db.function_shape(FunctionShapeId(shape_id));
                Some(shape.as_ref().clone())
            }

            fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
                let shape = self.db.callable_shape(CallableShapeId(shape_id));

                // For contextual typing, prefer call signatures. Fall back to construct
                // signatures when none exist (super()/new calls have construct sigs only).
                let signatures = if shape.call_signatures.is_empty() {
                    &shape.construct_signatures
                } else {
                    &shape.call_signatures
                };

                // If arg_count is provided, prefer fixed-arity overloads over
                // catch-all rest signatures when both match. This keeps broad
                // implementation signatures from poisoning contextual typing.
                let matching = if let Some(count) = self.arg_count {
                    let mut matching: Vec<_> = signatures
                        .iter()
                        .filter(|sig| {
                            let min_args = crate::utils::required_param_count(&sig.params);
                            let has_rest = sig.params.iter().any(|p| p.rest);
                            count >= min_args && (has_rest || count <= sig.params.len())
                        })
                        .collect();
                    if matching.iter().any(|sig| {
                        sig.params.len() == count
                            && !sig.params.last().is_some_and(|param| param.rest)
                    }) {
                        matching.retain(|sig| {
                            sig.params.len() == count
                                && !sig.params.last().is_some_and(|param| param.rest)
                        });
                    }
                    if matching
                        .iter()
                        .any(|sig| !sig.params.last().is_some_and(|param| param.rest))
                    {
                        matching.retain(|sig| !sig.params.last().is_some_and(|param| param.rest));
                    }
                    if matching.is_empty() {
                        signatures.iter().take(1).collect()
                    } else {
                        matching
                    }
                } else {
                    signatures.iter().collect()
                };

                combine_contextual_signatures(self.db, matching, self.arg_count)
            }

            fn visit_application(&mut self, app_id: u32) -> Self::Output {
                use crate::types::TypeApplicationId;

                // 1. Retrieve the application data (Base<Args>)
                let app = self.db.type_application(TypeApplicationId(app_id));

                // 2. Resolve the base type to get the generic function signature
                // e.g., for Handler<string>, this gets the shape of Handler<T>
                let base_shape = self.visit_guarded(app.base)?;

                // 3. Build the substitution map
                // Maps generic parameters (e.g., T) to arguments (e.g., string)
                // This handles default type parameters automatically
                let subst =
                    TypeSubstitution::from_args(self.db, &base_shape.type_params, &app.args);

                // Optimization: If no substitution is needed, return base as-is
                if subst.is_empty() {
                    return Some(base_shape);
                }

                // 4. Instantiate the components of the function shape
                let instantiated_params: Vec<ParamInfo> = base_shape
                    .params
                    .iter()
                    .map(|p| ParamInfo {
                        name: p.name,
                        type_id: instantiate_type_cached(self.db, self.query_db, p.type_id, &subst),
                        optional: p.optional,
                        rest: p.rest,
                    })
                    .collect();

                let instantiated_return =
                    instantiate_type_cached(self.db, self.query_db, base_shape.return_type, &subst);

                let instantiated_this = base_shape
                    .this_type
                    .map(|t| instantiate_type_cached(self.db, self.query_db, t, &subst));

                // Handle type predicates (e.g., `x is T`)
                let instantiated_predicate =
                    base_shape
                        .type_predicate
                        .as_ref()
                        .map(|pred| TypePredicate {
                            asserts: pred.asserts,
                            target: pred.target,
                            type_id: pred.type_id.map(|t| {
                                instantiate_type_cached(self.db, self.query_db, t, &subst)
                            }),
                            parameter_index: pred.parameter_index,
                        });

                // 5. Return the concrete FunctionShape
                Some(FunctionShape {
                    // The generics are now consumed/applied, so the resulting signature
                    // is concrete (not generic).
                    type_params: Vec::new(),
                    params: instantiated_params,
                    this_type: instantiated_this,
                    return_type: instantiated_return,
                    type_predicate: instantiated_predicate,
                    is_constructor: base_shape.is_constructor,
                    is_method: base_shape.is_method,
                })
            }

            fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
                let members = self.db.type_list(TypeListId(list_id));
                let shapes: Vec<_> = members
                    .iter()
                    .filter_map(|&member| self.visit_guarded(member))
                    .collect();
                combine_function_shapes(self.db, shapes, self.arg_count)
            }

            fn visit_union(&mut self, list_id: u32) -> Self::Output {
                let members = self.db.type_list(TypeListId(list_id));
                let mut member_shapes = Vec::new();

                for &member in members.iter() {
                    if member.is_nullable() || matches!(member, TypeId::VOID | TypeId::NEVER) {
                        continue;
                    }

                    if let Some(shape) = self.visit_guarded(member) {
                        member_shapes.push(shape);
                    }
                }

                if member_shapes.is_empty() {
                    return None;
                }

                // Match tsc's contextual union signature behavior: ignore
                // non-callable members and, when any call signature is available,
                // ignore construct-only members. This lets unions like
                // `FunctionComponent<P> | ComponentClass<P> | string` contribute
                // the callable `P` shape needed for inference while still
                // preserving pure-constructor unions for `new`-style contexts.
                let prefer_call = member_shapes.iter().any(|shape| !shape.is_constructor);
                let filtered_shapes: Vec<_> = member_shapes
                    .into_iter()
                    .filter(|shape| shape.is_constructor != prefer_call)
                    .collect();
                let first = filtered_shapes.first()?;
                if filtered_shapes
                    .iter()
                    .skip(1)
                    .any(|shape| !signatures_match_for_contextual_union(first, shape))
                {
                    return None;
                }

                combine_function_shapes(self.db, filtered_shapes, self.arg_count)
            }
        }

        let mut visitor = ContextualSignatureVisitor {
            db,
            query_db,
            arg_count,
            visiting: rustc_hash::FxHashSet::default(),
        };
        visitor.visit_guarded(type_id)
    }

    /// Get the base constraint for an index type used in an `IndexAccess`.
    ///
    /// For a `TypeParameter` `K extends C`, returns `Some(C)`.
    /// For an Intersection containing a `TypeParameter` `K extends C`,
    /// returns `Some(C)` (the constraint is a superset of the intersection).
    /// Returns None if the index has no usable constraint.
    pub(super) fn get_index_constraint(&self, idx: TypeId) -> Option<TypeId> {
        match self.interner.lookup(idx)? {
            TypeData::TypeParameter(tp) => tp.constraint,
            TypeData::Intersection(list_id) => {
                // Look for a TypeParameter in the intersection and use its constraint
                let members = self.interner.type_list(list_id);
                for &member in members.iter() {
                    if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(member)
                        && let Some(constraint) = tp.constraint
                    {
                        return Some(constraint);
                    }
                }
                None
            }
            _ => None,
        }
    }
}
