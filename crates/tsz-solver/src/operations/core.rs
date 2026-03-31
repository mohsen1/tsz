use crate::contextual::extractors::extract_param_type_at_for_call;
use crate::diagnostics::PendingDiagnostic;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::types::{
    CallSignature, CallableShape, CallableShapeId, FunctionShape, FunctionShapeId, IntrinsicKind,
    LiteralValue, ParamInfo, TupleElement, TypeData, TypeId, TypeListId, TypePredicate,
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
    pub(super) force_bivariant_callbacks: bool,
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
}

#[derive(Clone, Copy)]
enum UnionCallSignatureCompatibility {
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
struct CombinedUnionSignature {
    /// Intersected parameter types at each position
    param_types: Vec<TypeId>,
    /// Minimum required arguments (max of all members' required counts)
    min_required: usize,
    /// Maximum allowed arguments (None if unbounded / has rest)
    max_allowed: Option<usize>,
    /// Unioned return type from all members
    return_type: TypeId,
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

    fn normalize_union_member(&self, mut member: TypeId) -> TypeId {
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
    fn collect_union_call_signature_lists(
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
    /// parameters (allowing extra optional params) and their `this` types are
    /// identical (by TypeId equality).
    fn are_signatures_compatible_for_union(&self, a: &CallSignature, b: &CallSignature) -> bool {
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

        // Check parameter types are identical (by TypeId) for overlapping positions
        for i in 0..min_total {
            if a.params[i].type_id != b.params[i].type_id {
                return false;
            }
        }

        // Check this types match
        match (a.this_type, b.this_type) {
            (Some(a_this), Some(b_this)) => a_this == b_this,
            (None, None) => true,
            _ => false,
        }
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
    fn find_union_compatible_signatures(
        &self,
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
    fn compute_union_this_type(&self, members: &[TypeId]) -> Option<TypeId> {
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

    /// Get the contextual signature for a type, optionally filtering by argument count.
    /// When `arg_count` is provided, selects the first overload whose arity matches.
    fn get_contextual_signature_for_arity(
        db: &dyn TypeDatabase,
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

        struct ContextualSignatureVisitor<'a> {
            db: &'a dyn TypeDatabase,
            arg_count: Option<usize>,
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
                self.visit_type(self.db, TypeId(ref_id))
            }

            fn visit_lazy(&mut self, def_id: u32) -> Self::Output {
                // Resolve Lazy(DefId) types (interfaces, classes, type aliases)
                // so that Application types with a Lazy base can extract their
                // contextual signature for generic inference.
                let resolved = crate::evaluation::evaluate::evaluate_type(self.db, TypeId(def_id));
                if resolved != TypeId(def_id) {
                    self.visit_type(self.db, resolved)
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
                let base_shape = self.visit_type(self.db, app.base)?;

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
                        type_id: instantiate_type(self.db, p.type_id, &subst),
                        optional: p.optional,
                        rest: p.rest,
                    })
                    .collect();

                let instantiated_return = instantiate_type(self.db, base_shape.return_type, &subst);

                let instantiated_this = base_shape
                    .this_type
                    .map(|t| instantiate_type(self.db, t, &subst));

                // Handle type predicates (e.g., `x is T`)
                let instantiated_predicate =
                    base_shape
                        .type_predicate
                        .as_ref()
                        .map(|pred| TypePredicate {
                            asserts: pred.asserts,
                            target: pred.target,
                            type_id: pred.type_id.map(|t| instantiate_type(self.db, t, &subst)),
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
                    .filter_map(|&member| self.visit_type(self.db, member))
                    .collect();
                combine_function_shapes(self.db, shapes, self.arg_count)
            }

            fn visit_union(&mut self, list_id: u32) -> Self::Output {
                let members = self.db.type_list(TypeListId(list_id));
                let mut callable_member: Option<FunctionShape> = None;

                for &member in members.iter() {
                    if member.is_nullable() || matches!(member, TypeId::VOID | TypeId::NEVER) {
                        continue;
                    }

                    let shape = self.visit_type(self.db, member)?;

                    if callable_member.is_some() {
                        // Optional callback unions like `Fn | undefined` should preserve
                        // the callable shape, but we intentionally stay conservative for
                        // true unions of multiple callable members.
                        return None;
                    }

                    callable_member = Some(shape);
                }

                callable_member
            }
        }

        let mut visitor = ContextualSignatureVisitor { db, arg_count };
        visitor.visit_type(db, type_id)
    }

    /// Get the base constraint for an index type used in an `IndexAccess`.
    ///
    /// For a `TypeParameter` `K extends C`, returns `Some(C)`.
    /// For an Intersection containing a `TypeParameter` `K extends C`,
    /// returns `Some(C)` (the constraint is a superset of the intersection).
    /// Returns None if the index has no usable constraint.
    fn get_index_constraint(&self, idx: TypeId) -> Option<TypeId> {
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

    /// Resolve a function call: func(args...) -> result
    ///
    /// This is pure type logic - no AST nodes, just types in and types out.
    pub fn resolve_call(&mut self, func_type: TypeId, arg_types: &[TypeId]) -> CallResult {
        self.last_instantiated_predicate = None;
        self.last_instantiated_params = None;
        // Look up the function shape
        let key = match self.interner.lookup(func_type) {
            Some(k) => k,
            None => return CallResult::NotCallable { type_id: func_type },
        };

        match key {
            TypeData::Function(f_id) => {
                let shape = self.interner.function_shape(f_id);
                self.resolve_function_call(shape.as_ref(), arg_types)
            }
            TypeData::Callable(c_id) => {
                let shape = self.interner.callable_shape(c_id);
                self.resolve_callable_call(shape.as_ref(), arg_types)
            }
            TypeData::Union(list_id) => {
                // Handle union types: if all members are callable with compatible signatures,
                // the union is callable
                self.resolve_union_call(func_type, list_id, arg_types)
            }
            TypeData::Intersection(list_id) => {
                // Handle intersection types: if any member is callable, use that
                // This handles cases like: Function & { prop: number }
                self.resolve_intersection_call(func_type, list_id, arg_types)
            }
            TypeData::Application(_app_id) => {
                // Handle Application types (e.g., GenericCallable<string>)
                // Evaluate the application type to properly instantiate its base type with arguments
                let evaluated = self.checker.evaluate_type(func_type);
                if evaluated != func_type {
                    self.resolve_call(evaluated, arg_types)
                } else {
                    CallResult::NotCallable { type_id: func_type }
                }
            }
            TypeData::TypeParameter(param_info) => {
                // For type parameters with callable constraints (e.g., T extends { (): string }),
                // resolve the call using the constraint type
                if let Some(constraint) = param_info.constraint {
                    self.resolve_call(constraint, arg_types)
                } else {
                    CallResult::NotCallable { type_id: func_type }
                }
            }
            TypeData::Conditional(cond_id) => {
                // First try to evaluate the conditional type to a concrete type.
                let resolved = crate::evaluation::evaluate::evaluate_type(self.interner, func_type);
                if resolved != func_type {
                    return self.resolve_call(resolved, arg_types);
                }
                // For deferred conditional types (containing type parameters that
                // can't be resolved yet), check if both branches are callable.
                // tsc extracts call signatures from both branches of a deferred
                // conditional type. For example:
                //   type Q<T> = number extends T ? (n: number) => void : never;
                // When T is unknown, Q<T> is still callable because the true branch
                // is callable and the false branch is `never`.
                let cond = self.interner.conditional_type(cond_id);
                let true_type = cond.true_type;
                let false_type = cond.false_type;
                let true_is_never = true_type == TypeId::NEVER;
                let false_is_never = false_type == TypeId::NEVER;
                if true_is_never && false_is_never {
                    CallResult::NotCallable { type_id: func_type }
                } else if false_is_never {
                    self.resolve_call(true_type, arg_types)
                } else if true_is_never {
                    self.resolve_call(false_type, arg_types)
                } else {
                    // Both branches are non-never — try calling the true branch.
                    // If that succeeds, also try the false branch and union their
                    // return types, matching tsc behavior.
                    let true_result = self.resolve_call(true_type, arg_types);
                    let false_result = self.resolve_call(false_type, arg_types);
                    match (&true_result, &false_result) {
                        (CallResult::Success(true_ret), CallResult::Success(false_ret)) => {
                            CallResult::Success(self.interner.union2(*true_ret, *false_ret))
                        }
                        (CallResult::Success(_), _) | (_, CallResult::Success(_)) => {
                            // One branch callable, other not — still callable
                            // (the non-callable branch may be unreachable)
                            match true_result {
                                CallResult::Success(_) => true_result,
                                _ => false_result,
                            }
                        }
                        _ => CallResult::NotCallable { type_id: func_type },
                    }
                }
            }
            TypeData::Lazy(_)
            | TypeData::IndexAccess(_, _)
            | TypeData::Mapped(_)
            | TypeData::TemplateLiteral(_)
            | TypeData::TypeQuery(_) => {
                // Resolve meta-types to their actual types before checking callability.
                // This handles cases like index access types like T["method"],
                // and mapped types.
                //
                // Use checker.evaluate_type() which has a full resolver context,
                // rather than the standalone evaluate_type() with NoopResolver.
                // This is needed because IndexAccess types like T[K] where
                // T extends Record<K, F> require resolving Lazy(DefId) references
                // (e.g., Record's DefId) to expand Application types into their
                // structural form (mapped types) before the index can be resolved.
                let resolved = self.checker.evaluate_type(func_type);
                if resolved != func_type {
                    self.resolve_call(resolved, arg_types)
                } else {
                    // If evaluation couldn't resolve (e.g., IndexAccess with generic
                    // index), try using the base constraint. For `Obj[K]` where
                    // `K extends "a" | "b"`, substitute the constraint to get
                    // `Obj["a" | "b"]` which can be evaluated to a concrete callable type.
                    // This matches tsc's getBaseConstraintOfType for indexed access types.
                    if let Some(TypeData::IndexAccess(obj, idx)) = self.interner.lookup(func_type) {
                        // Try to get the base constraint of the index type.
                        // For a TypeParameter K extends C, use C.
                        // For an Intersection containing a TypeParameter, use
                        // the TypeParameter's constraint (which is a superset).
                        let constraint = self.get_index_constraint(idx);
                        if let Some(constraint_type) = constraint {
                            let eval_constraint = self.checker.evaluate_type(constraint_type);
                            let constrained_access =
                                self.interner.index_access(obj, eval_constraint);
                            let constrained = self.checker.evaluate_type(constrained_access);
                            if constrained != func_type {
                                return self.resolve_call(constrained, arg_types);
                            }
                        }
                    }
                    CallResult::NotCallable { type_id: func_type }
                }
            }
            // The `Function` intrinsic type is callable in TypeScript and returns `any`.
            // This matches tsc behavior: `declare const f: Function; f()` is valid.
            TypeData::Intrinsic(IntrinsicKind::Function | IntrinsicKind::Any) => {
                CallResult::Success(TypeId::ANY)
            }
            // `any` is callable and returns `any`
            // `error` propagates as error
            TypeData::Error => CallResult::Success(TypeId::ERROR),
            _ => CallResult::NotCallable { type_id: func_type },
        }
    }

    /// Resolve a call on a union type.
    ///
    /// This handles cases like:
    /// - `(() => void) | (() => string)` - all members callable
    /// - `string | (() => void)` - mixed callable/non-callable (returns `NotCallable`)
    ///
    /// When all union members are callable with compatible signatures, this returns
    /// a union of their return types.
    fn union_call_signature_bounds(&self, members: &[TypeId]) -> UnionCallSignatureCompatibility {
        let mut has_rest = false;
        let mut has_non_rest = false;
        let mut min_required = 0usize;
        let mut max_allowed: Option<usize> = Some(0);
        let mut found_callable = false;
        let mut signatures: Vec<Vec<ParamInfo>> = Vec::new();

        for &member in members.iter() {
            let Some(signature) = self.extract_union_call_signature(member) else {
                return UnionCallSignatureCompatibility::Unknown;
            };
            found_callable = true;
            signatures.push(signature);
        }

        if !found_callable || signatures.is_empty() {
            return UnionCallSignatureCompatibility::Unknown;
        }

        let max_params = signatures.iter().map(Vec::len).max().unwrap_or_default();

        for index in 0..max_params {
            let mut saw_required = false;
            let mut saw_optional = false;
            let mut saw_rest = false;
            let mut saw_absent = false;
            let mut saw_non_rest = false;

            for signature in &signatures {
                if index >= signature.len() {
                    saw_absent = true;
                    continue;
                }

                let param = &signature[index];
                if param.rest {
                    saw_rest = true;
                    if index != signature.len() - 1 {
                        return UnionCallSignatureCompatibility::Unknown;
                    }
                    saw_non_rest = false;
                } else {
                    saw_non_rest = true;
                    if param.is_required() {
                        saw_required = true;
                    } else {
                        saw_optional = true;
                    }
                }
            }

            if saw_rest && saw_non_rest {
                return UnionCallSignatureCompatibility::Incompatible;
            }

            if saw_required && saw_absent {
                return UnionCallSignatureCompatibility::Incompatible;
            }

            if saw_required && saw_optional && index > 0 {
                return UnionCallSignatureCompatibility::Incompatible;
            }

            if saw_required {
                min_required += 1;
                max_allowed = max_allowed.map(|max| max + 1);
            } else if saw_optional || saw_rest || saw_absent {
                max_allowed = max_allowed.and_then(|max| max.checked_add(1));
            }

            if saw_rest {
                has_rest = true;
            }
            if saw_non_rest {
                has_non_rest = true;
            }
        }

        let max_allowed = if has_rest && has_non_rest {
            return UnionCallSignatureCompatibility::Incompatible;
        } else if has_rest {
            None
        } else {
            max_allowed
        };

        UnionCallSignatureCompatibility::Compatible {
            min_required,
            max_allowed,
        }
    }

    fn extract_union_call_signature(&self, member: TypeId) -> Option<Vec<ParamInfo>> {
        let member = self.normalize_union_member(member);
        match self.interner.lookup(member) {
            Some(TypeData::Function(func_id)) => {
                let function = self.interner.function_shape(func_id);
                if !function.type_params.is_empty() {
                    return None;
                }
                Some(function.params.clone())
            }
            Some(TypeData::Callable(callable_id)) => {
                let callable = self.interner.callable_shape(callable_id);
                if callable.call_signatures.len() != 1 {
                    return None;
                }
                let signature = &callable.call_signatures[0];
                if !signature.type_params.is_empty() {
                    return None;
                }
                Some(signature.params.clone())
            }
            _ => None,
        }
    }

    fn is_single_signature_callable_member(&self, member: TypeId) -> bool {
        let member = self.normalize_union_member(member);
        match self.interner.lookup(member) {
            Some(TypeData::Function(_)) => true,
            Some(TypeData::Callable(callable_id)) => {
                let callable = self.interner.callable_shape(callable_id);
                callable.call_signatures.len() == 1
            }
            _ => false,
        }
    }

    /// Try to compute a combined call signature for a union type.
    ///
    /// In TypeScript, when all members of a union have exactly one call signature
    /// (non-generic), the union is callable with a combined signature where:
    /// - Parameter types are intersected (contravariant position)
    /// - Return types are unioned
    /// - Required param count is the max across all members
    ///
    /// Returns `None` if any member is not callable or has multiple/generic signatures.
    fn try_compute_combined_union_signature(
        &self,
        members: &[TypeId],
    ) -> Option<CombinedUnionSignature> {
        if members.is_empty() {
            return None;
        }

        // Collect single signatures from each member: (params, return_type, has_rest)
        let mut all_signatures: Vec<(Vec<ParamInfo>, TypeId, bool)> = Vec::new();

        for &member in members {
            let member = self.normalize_union_member(member);
            match self.interner.lookup(member) {
                Some(TypeData::Function(func_id)) => {
                    let function = self.interner.function_shape(func_id);
                    if !function.type_params.is_empty() {
                        return None; // generic functions need separate handling
                    }
                    let has_rest = function.params.iter().any(|p| p.rest);
                    all_signatures.push((function.params.clone(), function.return_type, has_rest));
                }
                Some(TypeData::Callable(callable_id)) => {
                    let callable = self.interner.callable_shape(callable_id);
                    if callable.call_signatures.len() != 1 {
                        return None; // multiple overloads need separate handling
                    }
                    let sig = &callable.call_signatures[0];
                    if !sig.type_params.is_empty() {
                        return None;
                    }
                    let has_rest = sig.params.iter().any(|p| p.rest);
                    all_signatures.push((sig.params.clone(), sig.return_type, has_rest));
                }
                _ => return None, // not callable
            }
        }

        if all_signatures.is_empty() {
            return None;
        }

        // Determine max param count for iterating all positions
        let max_param_count = all_signatures
            .iter()
            .map(|(params, _, _)| params.len())
            .max()
            .unwrap_or(0);

        let mut combined_params = Vec::new();
        let mut min_required = 0;

        for i in 0..max_param_count {
            let mut param_types_at_pos = Vec::new();
            let mut any_required = false;

            for (params, _, has_rest) in &all_signatures {
                if i < params.len() {
                    let param = &params[i];
                    if param.rest {
                        // For rest params like `...b: number[]`, extract the element type
                        // so we intersect `number` (not `number[]`) with other members' types
                        if let Some(elem) = crate::type_queries::get_array_element_type(
                            self.interner,
                            param.type_id,
                        ) {
                            param_types_at_pos.push(elem);
                        } else {
                            // Can't extract element type; bail out
                            return None;
                        }
                    } else {
                        param_types_at_pos.push(param.type_id);
                    }
                    if param.is_required() {
                        any_required = true;
                    }
                } else if *has_rest {
                    // Position i is beyond this member's positional params, but the
                    // member has a rest param that covers all remaining positions.
                    // Include its element type in the intersection.
                    if let Some(rest_param) = params.last().filter(|p| p.rest)
                        && let Some(elem) = crate::type_queries::get_array_element_type(
                            self.interner,
                            rest_param.type_id,
                        )
                    {
                        param_types_at_pos.push(elem);
                    }
                }
                // If a member doesn't have a param at this position and has no rest,
                // it doesn't constrain the type (absent). But if ANY member requires
                // it, the combined signature requires it.
            }

            // Intersect all param types at this position
            let combined_type = if param_types_at_pos.len() == 1 {
                param_types_at_pos[0]
            } else if param_types_at_pos.is_empty() {
                // Shouldn't happen since we iterate up to max_param_count
                continue;
            } else {
                let mut result = param_types_at_pos[0];
                for &pt in &param_types_at_pos[1..] {
                    result = self.interner.intersection2(result, pt);
                }
                result
            };

            combined_params.push(combined_type);

            if any_required {
                min_required = i + 1;
            }
        }

        // Compute max_allowed using tsc's Phase 1 matching semantics:
        // The member(s) with the highest min_required become the "base" of the
        // combined signature (all other members' signatures partially match them
        // because their min ≤ base.min). The combined inherits the base member's
        // parameter shape for determining max_allowed.
        //
        // - If any base member has rest → unlimited (None)
        // - Otherwise → max of base members' param counts
        // - If all members have the same min, they're all base members → use
        //   existing max_param_count / any_has_rest logic
        let max_allowed = {
            // Compute per-member min_required
            let member_mins: Vec<usize> = all_signatures
                .iter()
                .map(|(params, _, _)| {
                    params
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| p.is_required() && !p.rest)
                        .map(|(i, _)| i + 1)
                        .max()
                        .unwrap_or(0)
                })
                .collect();

            let max_min = *member_mins.iter().max().unwrap_or(&0);

            // Collect base members (those with the highest min_required)
            let base_has_rest = all_signatures
                .iter()
                .zip(member_mins.iter())
                .any(|((_, _, has_rest), &m_min)| m_min == max_min && *has_rest);
            let base_max_params = all_signatures
                .iter()
                .zip(member_mins.iter())
                .filter(|&(_, &m_min)| m_min == max_min)
                .map(|((params, _, _), _)| params.len())
                .max()
                .unwrap_or(0);

            if base_has_rest {
                None // Base member(s) have rest → unlimited
            } else {
                Some(base_max_params)
            }
        };

        // Union all return types
        let return_types: Vec<TypeId> = all_signatures.iter().map(|(_, ret, _)| *ret).collect();
        let return_type = self.interner.union(return_types);

        Some(CombinedUnionSignature {
            param_types: combined_params,
            min_required,
            max_allowed,
            return_type,
        })
    }

    fn build_union_call_result(
        &self,
        union_type: TypeId,
        failures: &mut Vec<CallResult>,
        return_types: Vec<TypeId>,
        combined_return_override: Option<TypeId>,
    ) -> CallResult {
        if return_types.is_empty() {
            if failures.is_empty() {
                return CallResult::NotCallable {
                    type_id: union_type,
                };
            }

            // At least one member failed with a non-NotCallable error
            // Check if all failures are ArgumentTypeMismatch - if so, compute the intersection
            // of all parameter types to get the expected type (e.g., for union of functions
            // with incompatible parameter types like (x: number) => void | (x: boolean) => void)
            let all_arg_mismatches = failures
                .iter()
                .all(|f| matches!(f, CallResult::ArgumentTypeMismatch { .. }));

            if all_arg_mismatches && !failures.is_empty() {
                // Extract all parameter types from the failures
                let mut param_types = Vec::new();
                for failure in failures.iter() {
                    if let CallResult::ArgumentTypeMismatch { expected, .. } = failure {
                        param_types.push(*expected);
                    }
                }

                // Compute the intersection of all parameter types
                // For incompatible primitives like number & boolean, this becomes never
                let intersected_param = if param_types.len() == 1 {
                    param_types[0]
                } else {
                    // Build intersection by combining all types
                    let mut result = param_types[0];
                    for &param_type in &param_types[1..] {
                        result = self.interner.intersection2(result, param_type);
                    }
                    result
                };

                // Return a single ArgumentTypeMismatch with the intersected type
                // Use the first argument type as the actual
                let actual_arg_type =
                    if let Some(CallResult::ArgumentTypeMismatch { actual, .. }) = failures.first()
                    {
                        *actual
                    } else {
                        // Should never reach here, but use ERROR instead of UNKNOWN
                        TypeId::ERROR
                    };

                // Use the combined return type from the union's signatures, but ONLY
                // when all union members expected the same parameter type. When params
                // differ (e.g., {x: string} vs {y: string}), excess property issues can
                // cause false failures, and leaking a non-ERROR return type would cascade
                // into downstream narrowing problems.
                let all_same_param = param_types.windows(2).all(|w| w[0] == w[1]);
                let combined_return = if all_same_param {
                    combined_return_override.unwrap_or(TypeId::ERROR)
                } else {
                    TypeId::ERROR
                };

                return CallResult::ArgumentTypeMismatch {
                    index: 0,
                    expected: intersected_param,
                    actual: actual_arg_type,
                    fallback_return: combined_return,
                };
            }

            // Not all argument type mismatches, return the first failure
            return failures
                .drain(..)
                .next()
                .unwrap_or(CallResult::NotCallable {
                    type_id: union_type,
                });
        }

        if return_types.len() == 1 {
            return CallResult::Success(return_types[0]);
        }

        // Return a union of all return types
        let union_result = self.interner.union(return_types);
        CallResult::Success(union_result)
    }
    fn resolve_union_call(
        &mut self,
        union_type: TypeId,
        list_id: TypeListId,
        arg_types: &[TypeId],
    ) -> CallResult {
        let members = self.interner.type_list(list_id);

        // Phase 0: Check `this` parameter for the union.
        // TSC computes the intersection of all members' `this` types and checks the
        // calling context against it. A call fails with TS2684 if the `this` context
        // doesn't satisfy ALL members' `this` requirements.
        if let Some(combined_this) = self.compute_union_this_type(&members) {
            let actual_this = self.actual_this_type.unwrap_or(TypeId::VOID);
            if !self.checker.is_assignable_to(actual_this, combined_this) {
                return CallResult::ThisTypeMismatch {
                    expected_this: combined_this,
                    actual_this,
                };
            }
        }

        // Phase 0.5: Check multi-overload union members for compatible signatures.
        // When multiple union members have multiple overloads, first try to find
        // compatible signatures across members. If found, validate `this` types.
        // If not found, fall through to per-member resolution (Phase 2) which
        // resolves each member's overloads independently — this matches tsc's
        // behavior for cases like `(A[] | B[]).filter(cb)` where each array type
        // has overloaded `filter` but per-member resolution succeeds.
        let sig_lists = self.collect_union_call_signature_lists(&members);
        let has_multi_overload_members =
            sig_lists.iter().filter(|(_, sigs)| sigs.len() > 1).count();

        if has_multi_overload_members >= 2 {
            if let Some(unified_sigs) = self.find_union_compatible_signatures(&sig_lists) {
                // Compatible signatures found — check `this` type constraint.
                // The unified signatures have intersected `this` types from
                // the matched overloads across all members.
                let unified_this = unified_sigs
                    .iter()
                    .filter_map(|s| s.this_type)
                    .reduce(|a, b| self.interner.intersection2(a, b));

                if let Some(combined_this) = unified_this {
                    let actual_this = self.actual_this_type.unwrap_or(TypeId::VOID);
                    if !self.checker.is_assignable_to(actual_this, combined_this) {
                        return CallResult::ThisTypeMismatch {
                            expected_this: combined_this,
                            actual_this,
                        };
                    }
                }
            } else {
                // No compatible signatures found across multi-overload members.
                // Per tsc's getUnionSignatures: when multiple union members have
                // multiple overloads and no compatible pair exists, the union is
                // not callable (TS2349). However, our compatibility check skips
                // generic signatures, so only report NotCallable when all overloads
                // across multi-overload members are non-generic. For generic
                // overloads, fall through to per-member resolution.
                let all_non_generic = sig_lists
                    .iter()
                    .filter(|(_, sigs)| sigs.len() > 1)
                    .all(|(_, sigs)| sigs.iter().all(|s| s.type_params.is_empty()));
                if all_non_generic {
                    return CallResult::NotCallable {
                        type_id: union_type,
                    };
                }
            }
        } else if has_multi_overload_members == 1 {
            // One member has multiple overloads, others have one each.
            // Per tsc's getUnionSignatures/intersectSignatureSets: each single-overload
            // member's signature must be compatible with at least one overload from the
            // multi-overload member. If any single-overload member has no compatible
            // match, the union is not callable (TS2349).
            //
            // Use TypeId equality for `this` types (safe, no side effects) plus the
            // None-matches-any rule from tsc's compareSignaturesIdentical.
            let multi_idx = sig_lists.iter().position(|(_, sigs)| sigs.len() > 1);
            if let Some(multi_idx) = multi_idx {
                let multi_sigs = &sig_lists[multi_idx].1;
                let mut all_compatible = true;
                for (idx, (_, sigs)) in sig_lists.iter().enumerate() {
                    if idx == multi_idx {
                        continue;
                    }
                    if let Some(single_sig) = sigs.first() {
                        // Check if this single-overload sig is compatible with ANY
                        // overload from the multi-overload member.
                        let has_match = multi_sigs.iter().any(|multi_sig| {
                            // Skip generic signatures
                            if !single_sig.type_params.is_empty()
                                || !multi_sig.type_params.is_empty()
                            {
                                return false;
                            }
                            // Check required param count
                            let s_req =
                                single_sig.params.iter().filter(|p| p.is_required()).count();
                            let m_req = multi_sig.params.iter().filter(|p| p.is_required()).count();
                            if s_req != m_req {
                                return false;
                            }
                            // Check param types
                            let min_total = single_sig.params.len().min(multi_sig.params.len());
                            for i in 0..min_total {
                                if single_sig.params[i].type_id != multi_sig.params[i].type_id {
                                    return false;
                                }
                            }
                            // Check this types (None matches any per tsc)
                            match (single_sig.this_type, multi_sig.this_type) {
                                (Some(a), Some(b)) => a == b,
                                _ => true,
                            }
                        });
                        if !has_match {
                            all_compatible = false;
                            break;
                        }
                    }
                }
                if !all_compatible {
                    let all_non_generic = sig_lists
                        .iter()
                        .filter(|(_, sigs)| sigs.len() > 1)
                        .all(|(_, sigs)| sigs.iter().all(|s| s.type_params.is_empty()));
                    if all_non_generic {
                        return CallResult::NotCallable {
                            type_id: union_type,
                        };
                    }
                }
            }
        }

        // Try to compute a combined signature for the union.
        // TypeScript computes combined arity (max required params across members)
        // and intersected parameter types with unioned return types.
        let combined = self.try_compute_combined_union_signature(&members);

        // Phase 1: Argument count validation using combined signature.
        // This catches cases where members have different param counts —
        // the combined signature requires the maximum number of params.
        if let Some(ref combined) = combined {
            if arg_types.len() < combined.min_required {
                return CallResult::ArgumentCountMismatch {
                    expected_min: combined.min_required,
                    expected_max: combined.max_allowed,
                    actual: arg_types.len(),
                };
            }
            if let Some(max) = combined.max_allowed
                && arg_types.len() > max
            {
                return CallResult::ArgumentCountMismatch {
                    expected_min: combined.min_required,
                    expected_max: combined.max_allowed,
                    actual: arg_types.len(),
                };
            }
        }

        // Phase 2: Per-member resolution for argument type checking.
        // This avoids over-constraining via intersection when tsc would reduce the union.
        let compatibility = if combined.is_some() {
            // Combined signature already validated arity; skip old bounds check
            UnionCallSignatureCompatibility::Unknown
        } else {
            let compat = self.union_call_signature_bounds(&members);
            if matches!(compat, UnionCallSignatureCompatibility::Incompatible) {
                return CallResult::NotCallable {
                    type_id: union_type,
                };
            }
            compat
        };

        let mut return_types = Vec::new();
        let mut failures = Vec::new();

        for &member in members.iter() {
            let result = self.resolve_call(member, arg_types);
            match result {
                CallResult::Success(return_type) => {
                    return_types.push(return_type);
                }
                CallResult::NotCallable { .. } => {
                    return CallResult::NotCallable {
                        type_id: union_type,
                    };
                }
                other => {
                    failures.push(other);
                }
            }
        }
        // Phase 3: Result aggregation.
        // When we have a combined signature and some members fail on arity
        // (because they have fewer params than the combined requires),
        // use the combined return type since the overall call is valid.
        if let Some(ref combined) = combined {
            let all_failures_are_arity = !failures.is_empty()
                && failures
                    .iter()
                    .all(|f| matches!(f, CallResult::ArgumentCountMismatch { .. }));

            if all_failures_are_arity && !return_types.is_empty() {
                // Some members succeeded, some failed on arity only.
                // The combined arity check passed, so the call is valid.
                return CallResult::Success(combined.return_type);
            }

            if all_failures_are_arity && return_types.is_empty() {
                // All members failed on arity but combined check passed.
                // Validate argument types against the combined (intersected) params.
                for (i, &arg_type) in arg_types.iter().enumerate() {
                    if i < combined.param_types.len() {
                        let param_type = combined.param_types[i];
                        if !self.checker.is_assignable_to(arg_type, param_type) {
                            return CallResult::ArgumentTypeMismatch {
                                index: i,
                                expected: param_type,
                                actual: arg_type,
                                fallback_return: combined.return_type,
                            };
                        }
                    }
                }
                return CallResult::Success(combined.return_type);
            }

            // When all per-member resolutions fail (with any combination of arity
            // or type mismatches), validate against the combined (intersected)
            // parameter types instead. TSC intersects parameter types across union
            // members, so an arg like `{x: 0, y: 0}` satisfies `{x: number} &
            // {y: number}` even though it fails excess-property checks against each
            // individual member type. Individual members may also fail on arity when
            // they have fewer params than the combined signature allows.
            if !failures.is_empty() && return_types.is_empty() {
                let mut all_pass = true;
                for (i, &arg_type) in arg_types.iter().enumerate() {
                    if i < combined.param_types.len() {
                        let param_type = combined.param_types[i];
                        if !self.checker.is_assignable_to(arg_type, param_type) {
                            all_pass = false;
                            break;
                        }
                    }
                }
                if all_pass {
                    return CallResult::Success(combined.return_type);
                }
                // When the combined (intersected) parameter type is `never` and all
                // per-member calls fail, this MAY be a false positive from correlated
                // type parameters. For example, calling a union of functions obtained
                // from `MappedType[K]` where the argument type correlates with K.
                // In tsc, K links the handler and argument, so the call succeeds.
                //
                // Only apply this fallback when the argument types contain type
                // parameters — this indicates a generic/correlated context where
                // the caller's type parameter determines which union member is
                // actually reached. When all arguments are fully concrete (e.g.,
                // `string | number` passed to `((a: string) => void) | ((a: number) => void)`),
                // tsc correctly rejects the call (TS2345).
                let has_generic_args = arg_types.iter().any(|&arg_type| {
                    crate::type_queries::contains_type_parameters_db(self.interner, arg_type)
                });
                if has_generic_args && combined.param_types.contains(&TypeId::NEVER) {
                    let all_arg_mismatch = failures
                        .iter()
                        .all(|f| matches!(f, CallResult::ArgumentTypeMismatch { .. }));
                    if all_arg_mismatch {
                        let mut param_union_pass = true;
                        for (i, &arg_type) in arg_types.iter().enumerate() {
                            if i < combined.param_types.len()
                                && combined.param_types[i] == TypeId::NEVER
                            {
                                // Collect per-member param types at this position
                                let member_param_types: Vec<TypeId> = failures
                                    .iter()
                                    .filter_map(|f| {
                                        if let CallResult::ArgumentTypeMismatch {
                                            expected, ..
                                        } = f
                                        {
                                            Some(*expected)
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();
                                let param_union = self.interner.union(member_param_types);
                                if !self.checker.is_assignable_to(arg_type, param_union) {
                                    param_union_pass = false;
                                    break;
                                }
                            }
                        }
                        if param_union_pass {
                            return CallResult::Success(combined.return_type);
                        }
                    }
                }
            }
        }

        // Standard per-member result aggregation (no combined signature or mixed failures)
        if !return_types.is_empty() {
            if combined.is_none()
                && !failures.is_empty()
                && members
                    .iter()
                    .copied()
                    .all(|member| self.is_single_signature_callable_member(member))
            {
                return CallResult::NotCallable {
                    type_id: union_type,
                };
            }

            match compatibility {
                UnionCallSignatureCompatibility::Compatible {
                    min_required,
                    max_allowed,
                } => {
                    if arg_types.len() < min_required {
                        return CallResult::ArgumentCountMismatch {
                            expected_min: min_required,
                            expected_max: max_allowed,
                            actual: arg_types.len(),
                        };
                    }
                    if let Some(max_allowed) = max_allowed
                        && arg_types.len() > max_allowed
                    {
                        return CallResult::ArgumentCountMismatch {
                            expected_min: min_required,
                            expected_max: Some(max_allowed),
                            actual: arg_types.len(),
                        };
                    }
                    if failures
                        .iter()
                        .all(|f| matches!(f, CallResult::ArgumentCountMismatch { .. }))
                    {
                        return self.build_union_call_result(
                            union_type,
                            &mut failures,
                            return_types,
                            combined.as_ref().map(|c| c.return_type),
                        );
                    }
                }
                UnionCallSignatureCompatibility::Unknown => {}
                UnionCallSignatureCompatibility::Incompatible => unreachable!(),
            }

            if return_types.len() == 1 {
                return CallResult::Success(return_types[0]);
            }
            let union_result = self.interner.union(return_types);
            CallResult::Success(union_result)
        } else if !failures.is_empty() {
            self.build_union_call_result(
                union_type,
                &mut failures,
                return_types,
                combined.as_ref().map(|c| c.return_type),
            )
        } else {
            CallResult::NotCallable {
                type_id: union_type,
            }
        }
    }

    /// Resolve a call on an intersection type.
    ///
    /// This handles cases like:
    /// - `Function & { prop: number }` - intersection with callable member
    /// - Overloaded functions merged via intersection
    ///
    /// When at least one intersection member is callable, this delegates to that member.
    /// For intersections with multiple callable members, we use the first one.
    fn resolve_intersection_call(
        &mut self,
        intersection_type: TypeId,
        list_id: TypeListId,
        arg_types: &[TypeId],
    ) -> CallResult {
        let members = self.interner.type_list(list_id);

        // For intersection types: if ANY member is callable, the intersection is callable
        // This is different from unions where ALL members must be callable
        // We try each member in order and use the first callable one
        for &member in members.iter() {
            let result = self.resolve_call(member, arg_types);
            match result {
                CallResult::Success(return_type) => {
                    // Found a callable member - use its return type
                    return CallResult::Success(return_type);
                }
                CallResult::NotCallable { .. } => {
                    // This member is not callable, try the next one
                    continue;
                }
                other => {
                    // Got a different error (argument mismatch, etc.)
                    // Return this error as it's likely the most relevant
                    return other;
                }
            }
        }

        // No members were callable
        CallResult::NotCallable {
            type_id: intersection_type,
        }
    }

    /// Resolve a call to a simple function type.
    pub(crate) fn resolve_function_call(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> CallResult {
        // Handle generic functions FIRST so uninstantiated this_types don't fail assignability
        if !func.type_params.is_empty() {
            return self.resolve_generic_call(func, arg_types);
        }

        // Check `this` context if specified by the function shape
        if let Some(expected_this) = func.this_type {
            if let Some(actual_this) = self.actual_this_type {
                if !self.checker.is_assignable_to(actual_this, expected_this) {
                    return CallResult::ThisTypeMismatch {
                        expected_this,
                        actual_this,
                    };
                }
            }
            // Note: if `actual_this_type` is None, we technically should check if `void` is assignable to `expected_this`.
            // But TSC behavior for missing `this` might require strict checking. Let's do it:
            else if !self.checker.is_assignable_to(TypeId::VOID, expected_this) {
                return CallResult::ThisTypeMismatch {
                    expected_this,
                    actual_this: TypeId::VOID,
                };
            }
        }

        // Check argument count
        let (min_args, max_args) = self.arg_count_bounds(&func.params);

        if arg_types.len() < min_args {
            // For variadic tuple rest params (e.g. `...args: [...T[], Required]`),
            // TSC checks assignability of the args-as-tuple against the rest param
            // type, producing TS2345 instead of TS2555. Detect this case and return
            // ArgumentTypeMismatch so the checker emits TS2345.
            if let Some(rest_param) = func.params.last().filter(|p| p.rest) {
                let rest_type = self.unwrap_readonly(rest_param.type_id);
                // `...args: never` means any call is invalid — TSC builds an empty
                // tuple and checks it against `never`, producing TS2345.
                let should_type_check = if rest_type == TypeId::NEVER {
                    true
                } else if let Some(TypeData::Tuple(elements)) = self.interner.lookup(rest_type) {
                    let elems = self.interner.tuple_list(elements);
                    elems.iter().any(|e| e.rest)
                } else {
                    false
                };
                if should_type_check {
                    // Build tuple type from actual args
                    let args_tuple_elems: Vec<TupleElement> = arg_types
                        .iter()
                        .map(|&t| TupleElement {
                            type_id: t,
                            name: None,
                            optional: false,
                            rest: false,
                        })
                        .collect();
                    let args_tuple = self.interner.tuple(args_tuple_elems);
                    return CallResult::ArgumentTypeMismatch {
                        index: 0,
                        expected: rest_type,
                        actual: args_tuple,
                        fallback_return: func.return_type,
                    };
                }
            }
            return CallResult::ArgumentCountMismatch {
                expected_min: min_args,
                expected_max: max_args,
                actual: arg_types.len(),
            };
        }

        if let Some(max) = max_args
            && arg_types.len() > max
        {
            return CallResult::ArgumentCountMismatch {
                expected_min: min_args,
                expected_max: Some(max),
                actual: arg_types.len(),
            };
        }

        // Generic functions handled above

        if let Some(result) = self.check_argument_types(&func.params, arg_types, func.is_method) {
            return result;
        }

        // Even if arg count and individual arg types pass, a `...args: never` rest param
        // means no call is valid. TSC checks the args-as-tuple against `never`.
        if let Some(rest_param) = func.params.last().filter(|p| p.rest) {
            let rest_type = self.unwrap_readonly(rest_param.type_id);
            if rest_type == TypeId::NEVER {
                let rest_start = func.params.len().saturating_sub(1);
                let rest_args = &arg_types[rest_start.min(arg_types.len())..];
                let args_tuple_elems: Vec<TupleElement> = rest_args
                    .iter()
                    .map(|&t| TupleElement {
                        type_id: t,
                        name: None,
                        optional: false,
                        rest: false,
                    })
                    .collect();
                let args_tuple = self.interner.tuple(args_tuple_elems);
                return CallResult::ArgumentTypeMismatch {
                    index: 0,
                    expected: rest_type,
                    actual: args_tuple,
                    fallback_return: func.return_type,
                };
            }
        }

        CallResult::Success(func.return_type)
    }

    /// Resolve a call to a callable type (with overloads).
    pub(crate) fn resolve_callable_call(
        &mut self,
        callable: &CallableShape,
        arg_types: &[TypeId],
    ) -> CallResult {
        // If there are no call signatures at all, this type is not callable
        // (e.g., a class constructor without call signatures)
        if callable.call_signatures.is_empty() {
            return CallResult::NotCallable {
                type_id: self.interner.callable(callable.clone()),
            };
        }

        if callable.call_signatures.len() == 1 {
            let sig = &callable.call_signatures[0];
            let func = FunctionShape {
                params: sig.params.clone(),
                this_type: sig.this_type,
                return_type: sig.return_type,
                type_params: sig.type_params.clone(),
                type_predicate: sig.type_predicate,
                is_constructor: false,
                is_method: sig.is_method,
            };
            return self.resolve_function_call(&func, arg_types);
        }

        // Try each call signature
        let mut failures = Vec::new();
        let mut all_arg_count_mismatches = true;
        let mut min_expected = usize::MAX;
        let mut max_expected = 0;
        let mut any_has_rest = false;
        let actual_count = arg_types.len();
        let mut exact_expected_counts = FxHashSet::default();
        // Track if exactly one overload matched argument count but had a type mismatch.
        // When there is a single "count-compatible" overload that fails only on types,
        // tsc reports TS2345 (the inner type error) rather than TS2769 (no overload matched).
        let mut type_mismatch_count: usize = 0;
        let mut first_type_mismatch: Option<(usize, TypeId, TypeId)> = None; // (index, expected, actual)
        let mut all_mismatches_identical = true;
        let mut has_non_count_non_type_failure = false;

        for sig in &callable.call_signatures {
            // Convert CallSignature to FunctionShape
            let func = FunctionShape {
                params: sig.params.clone(),
                this_type: sig.this_type,
                return_type: sig.return_type,
                type_params: sig.type_params.clone(),
                type_predicate: sig.type_predicate,
                is_constructor: false,
                is_method: sig.is_method,
            };
            tracing::debug!("resolve_callable_call: signature = {sig:?}");

            match self.resolve_function_call(&func, arg_types) {
                CallResult::Success(ret) => return CallResult::Success(ret),
                CallResult::TypeParameterConstraintViolation { return_type, .. } => {
                    // Constraint violation is a "near match" - return the type
                    // for overload resolution (treat as success with error)
                    return CallResult::Success(return_type);
                }
                CallResult::ArgumentTypeMismatch {
                    index,
                    expected,
                    actual,
                    ..
                } => {
                    all_arg_count_mismatches = false;
                    type_mismatch_count += 1;
                    if type_mismatch_count == 1 {
                        first_type_mismatch = Some((index, expected, actual));
                    } else if first_type_mismatch != Some((index, expected, actual)) {
                        all_mismatches_identical = false;
                    }
                    failures.push(
                        crate::diagnostics::PendingDiagnosticBuilder::argument_not_assignable(
                            actual, expected,
                        ),
                    );
                }
                CallResult::ArgumentCountMismatch {
                    expected_min,
                    expected_max,
                    actual,
                } => {
                    if expected_max.is_none() {
                        any_has_rest = true;
                    } else if expected_min == expected_max.unwrap_or(expected_min) {
                        exact_expected_counts.insert(expected_min);
                    }
                    let max = expected_max.unwrap_or(expected_min);
                    min_expected = min_expected.min(expected_min);
                    max_expected = max_expected.max(max);
                    failures.push(
                        crate::diagnostics::PendingDiagnosticBuilder::argument_count_mismatch(
                            expected_min,
                            max,
                            actual,
                        ),
                    );
                }
                _ => {
                    all_arg_count_mismatches = false;
                    has_non_count_non_type_failure = true;
                }
            }
        }

        // If all signatures failed due to argument count mismatch, report TS2554 instead of TS2769
        if all_arg_count_mismatches && !failures.is_empty() {
            if !any_has_rest
                && !exact_expected_counts.is_empty()
                && !exact_expected_counts.contains(&actual_count)
            {
                let mut lower = None;
                let mut upper = None;
                for &count in &exact_expected_counts {
                    if count < actual_count {
                        lower = Some(lower.map_or(count, |prev: usize| prev.max(count)));
                    } else if count > actual_count {
                        upper = Some(upper.map_or(count, |prev: usize| prev.min(count)));
                    }
                }
                if let (Some(expected_low), Some(expected_high)) = (lower, upper) {
                    return CallResult::OverloadArgumentCountMismatch {
                        actual: actual_count,
                        expected_low,
                        expected_high,
                    };
                }
            }
            return CallResult::ArgumentCountMismatch {
                expected_min: min_expected,
                expected_max: if any_has_rest {
                    None
                } else if max_expected > min_expected {
                    Some(max_expected)
                } else {
                    Some(min_expected)
                },
                actual: actual_count,
            };
        }

        // If all type mismatches are identical (or there's exactly one), and no other failures occurred,
        // report TS2345 (the inner type error) instead of TS2769. This handles duplicate signatures
        // or overloads where the failing parameter has the exact same type in all matching overloads.
        if !has_non_count_non_type_failure
            && type_mismatch_count > 0
            && all_mismatches_identical
            && let Some((index, expected, actual)) = first_type_mismatch
        {
            return CallResult::ArgumentTypeMismatch {
                index,
                expected,
                actual,
                fallback_return: TypeId::ERROR,
            };
        }

        // If we got here, no signature matched.
        // Use the last overload signature's return type as the fallback (matching
        // tsc behavior). tsc uses the last declaration's return type for error
        // recovery, allowing downstream code to see the expected shape. For
        // example, `[].concat(...)` on `never[]` should still produce `never[]`,
        // not `never`, so that chained `.map()` resolves correctly.
        let fallback_return = callable
            .call_signatures
            .last()
            .map(|s| s.return_type)
            .unwrap_or(TypeId::NEVER);
        CallResult::NoOverloadMatch {
            func_type: self.interner.callable(callable.clone()),
            arg_types: arg_types.to_vec(),
            failures,
            fallback_return,
        }
    }
}

pub fn infer_call_signature<C: AssignabilityChecker>(
    interner: &dyn QueryDatabase,
    checker: &mut C,
    sig: &CallSignature,
    arg_types: &[TypeId],
) -> TypeId {
    let mut evaluator = CallEvaluator::new(interner, checker);
    evaluator.infer_call_signature(sig, arg_types)
}

pub fn infer_generic_function<C: AssignabilityChecker>(
    interner: &dyn QueryDatabase,
    checker: &mut C,
    func: &FunctionShape,
    arg_types: &[TypeId],
) -> TypeId {
    let mut evaluator = CallEvaluator::new(interner, checker);
    evaluator.infer_generic_function(func, arg_types)
}

pub fn resolve_call_with_checker<C: AssignabilityChecker>(
    interner: &dyn QueryDatabase,
    checker: &mut C,
    func_type: TypeId,
    arg_types: &[TypeId],
    force_bivariant_callbacks: bool,
    contextual_type: Option<TypeId>,
    actual_this_type: Option<TypeId>,
) -> CallWithCheckerResult {
    let mut evaluator = CallEvaluator::new(interner, checker);
    evaluator.set_force_bivariant_callbacks(force_bivariant_callbacks);
    evaluator.set_contextual_type(contextual_type);
    evaluator.set_actual_this_type(actual_this_type);
    let result = evaluator.resolve_call(func_type, arg_types);
    let predicate = evaluator.last_instantiated_predicate.take();
    let instantiated_params = evaluator.last_instantiated_params.take();
    (result, predicate, instantiated_params)
}

pub fn resolve_new_with_checker<C: AssignabilityChecker>(
    interner: &dyn QueryDatabase,
    checker: &mut C,
    type_id: TypeId,
    arg_types: &[TypeId],
    force_bivariant_callbacks: bool,
    contextual_type: Option<TypeId>,
) -> CallResult {
    let mut evaluator = CallEvaluator::new(interner, checker);
    evaluator.set_force_bivariant_callbacks(force_bivariant_callbacks);
    evaluator.set_contextual_type(contextual_type);
    evaluator.resolve_new(type_id, arg_types)
}

pub fn compute_contextual_types_with_compat_checker<'a, R, F>(
    interner: &'a dyn QueryDatabase,
    resolver: &'a R,
    shape: &FunctionShape,
    arg_types: &[TypeId],
    contextual_type: Option<TypeId>,
    configure_checker: F,
) -> TypeSubstitution
where
    R: crate::TypeResolver,
    F: FnOnce(&mut crate::CompatChecker<'a, R>),
{
    let mut checker = crate::CompatChecker::with_resolver(interner, resolver);
    configure_checker(&mut checker);

    let mut evaluator = CallEvaluator::new(interner, &mut checker);
    evaluator.set_contextual_type(contextual_type);
    evaluator.compute_contextual_types(shape, arg_types)
}

pub fn get_contextual_signature_with_compat_checker(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<FunctionShape> {
    CallEvaluator::<crate::CompatChecker>::get_contextual_signature(db, type_id)
}

pub fn get_contextual_signature_for_arity_with_compat_checker(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    arg_count: usize,
) -> Option<FunctionShape> {
    CallEvaluator::<crate::CompatChecker>::get_contextual_signature_for_arity(
        db,
        type_id,
        Some(arg_count),
    )
}
