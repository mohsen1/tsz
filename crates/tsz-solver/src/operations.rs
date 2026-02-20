//! Type operations and expression evaluation.
//!
//! This module contains the "brain" of the type system - all the logic for
//! evaluating expressions, resolving calls, accessing properties, etc.
//!
//! ## Architecture Principle
//!
//! The Solver handles **WHAT** (type operations and relations), while the
//! Checker handles **WHERE** (AST traversal, scoping, control flow).
//!
//! All functions here:
//! - Take `TypeId` as input (not AST nodes)
//! - Return structured results (not formatted error strings)
//! - Are pure logic (no side effects, no diagnostic formatting)
//!
//! This allows the Solver to be:
//! - Unit tested without AST nodes
//! - Reused across different checkers
//! - Optimized independently
//!
//! ## Module Organization
//!
//! Some components have been extracted to separate modules:
//! - `binary_ops`: Binary operation evaluation (+, -, *, /, etc.)

// Re-exports from extracted modules
// Note: These are intentionally pub re-exported for external API use
pub use crate::binary_ops::{BinaryOpEvaluator, BinaryOpResult, PrimitiveClass};

use crate::diagnostics::PendingDiagnostic;
use crate::instantiate::{TypeSubstitution, instantiate_type};
#[cfg(test)]
use crate::types::*;
use crate::types::{
    CallSignature, CallableShape, CallableShapeId, FunctionShape, FunctionShapeId, IntrinsicKind,
    LiteralValue, ParamInfo, TemplateSpan, TupleElement, TypeData, TypeId, TypeListId,
    TypePredicate,
};
use crate::visitor::TypeVisitor;
use crate::{QueryDatabase, TypeDatabase};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use tracing::{debug, trace};

/// Maximum recursion depth for type constraint collection to prevent infinite loops.
pub const MAX_CONSTRAINT_RECURSION_DEPTH: usize = 100;

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
    },

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
    },
}

struct TupleRestExpansion {
    /// Fixed elements before the variadic portion (prefix)
    fixed: Vec<TupleElement>,
    /// The variadic element type (e.g., T for ...T[])
    variadic: Option<TypeId>,
    /// Fixed elements after the variadic portion (suffix/tail)
    tail: Vec<TupleElement>,
}

/// Evaluates function calls.
pub struct CallEvaluator<'a, C: AssignabilityChecker> {
    pub(crate) interner: &'a dyn QueryDatabase,
    pub(crate) checker: &'a mut C,
    pub(crate) defaulted_placeholders: FxHashSet<TypeId>,
    force_bivariant_callbacks: bool,
    /// Contextual type for the call expression's expected result
    /// Used for contextual type inference in generic functions
    pub(crate) contextual_type: Option<TypeId>,
    /// The `this` type provided by the caller (e.g. `obj` in `obj.method()`)
    pub(crate) actual_this_type: Option<TypeId>,
    /// Current recursion depth for `constrain_types` to prevent infinite loops
    pub(crate) constraint_recursion_depth: RefCell<usize>,
    /// Visited (source, target) pairs during constraint collection.
    pub(crate) constraint_pairs: RefCell<FxHashSet<(TypeId, TypeId)>>,
    /// After a generic call resolves, holds the instantiated type predicate (if any).
    /// This lets the checker retrieve the predicate with inferred type arguments applied.
    pub last_instantiated_predicate: Option<(TypePredicate, Vec<ParamInfo>)>,
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
            constraint_recursion_depth: RefCell::new(0),
            constraint_pairs: RefCell::new(FxHashSet::default()),
            last_instantiated_predicate: None,
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
            type_predicate: sig.type_predicate.clone(),
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
    pub fn get_contextual_signature_for_arity(
        db: &dyn TypeDatabase,
        type_id: TypeId,
        arg_count: Option<usize>,
    ) -> Option<FunctionShape> {
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

                // If arg_count is provided, select the first overload whose arity matches.
                let sig = if let Some(count) = self.arg_count {
                    signatures
                        .iter()
                        .find(|sig| {
                            let min_args =
                                sig.params.iter().filter(|p| !p.optional && !p.rest).count();
                            let has_rest = sig.params.iter().any(|p| p.rest);
                            count >= min_args && (has_rest || count <= sig.params.len())
                        })
                        .or_else(|| signatures.first())
                } else {
                    signatures.first()
                };

                sig.map(|sig| FunctionShape {
                    type_params: sig.type_params.clone(),
                    params: sig.params.clone(),
                    this_type: sig.this_type,
                    return_type: sig.return_type,
                    type_predicate: sig.type_predicate.clone(),
                    is_constructor: false,
                    is_method: sig.is_method,
                })
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
                            target: pred.target.clone(),
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
                for &member in members.iter() {
                    if let Some(shape) = self.visit_type(self.db, member) {
                        return Some(shape);
                    }
                }
                None
            }

            // Future: Handle Union (return None or intersect of params)
        }

        let mut visitor = ContextualSignatureVisitor { db, arg_count };
        visitor.visit_type(db, type_id)
    }

    /// Resolve a function call: func(args...) -> result
    ///
    /// This is pure type logic - no AST nodes, just types in and types out.
    pub fn resolve_call(&mut self, func_type: TypeId, arg_types: &[TypeId]) -> CallResult {
        self.last_instantiated_predicate = None;
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
            TypeData::Application(app_id) => {
                // Handle Application types (e.g., GenericCallable<string>)
                // Get the application and resolve the call on its base type
                let app = self.interner.type_application(app_id);
                // Resolve the call on the base type with type arguments applied
                // The application's base should already be a callable type after type evaluation
                self.resolve_call(app.base, arg_types)
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
            TypeData::Lazy(_)
            | TypeData::Conditional(_)
            | TypeData::IndexAccess(_, _)
            | TypeData::Mapped(_)
            | TypeData::TemplateLiteral(_) => {
                // Resolve meta-types to their actual types before checking callability.
                // This handles cases like conditional types that resolve to function types,
                // index access types like T["method"], and mapped types.
                let resolved = crate::evaluate::evaluate_type(self.interner, func_type);
                if resolved != func_type {
                    self.resolve_call(resolved, arg_types)
                } else {
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
    fn resolve_union_call(
        &mut self,
        union_type: TypeId,
        list_id: TypeListId,
        arg_types: &[TypeId],
    ) -> CallResult {
        let members = self.interner.type_list(list_id);

        // Check each member of the union
        let mut return_types = Vec::new();
        let mut failures = Vec::new();

        for &member in members.iter() {
            let result = self.resolve_call(member, arg_types);
            match result {
                CallResult::Success(return_type) => {
                    return_types.push(return_type);
                }
                CallResult::NotCallable { .. } => {
                    // At least one member is not callable
                    // This means the union as a whole is not callable
                    // (we can't call a union without knowing which branch is active)
                    return CallResult::NotCallable {
                        type_id: union_type,
                    };
                }
                other => {
                    // Track failures for potential overload reporting
                    failures.push(other);
                }
            }
        }

        // If any members succeeded, return a union of their return types
        // TypeScript allows calling a union of functions if at least one member accepts the arguments
        if !return_types.is_empty() {
            if return_types.len() == 1 {
                return CallResult::Success(return_types[0]);
            }
            // Return a union of all return types
            let union_result = self.interner.union(return_types);
            CallResult::Success(union_result)
        } else if !failures.is_empty() {
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
                for failure in &failures {
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

                return CallResult::ArgumentTypeMismatch {
                    index: 0,
                    expected: intersected_param,
                    actual: actual_arg_type,
                };
            }

            // Not all argument type mismatches, return the first failure
            failures
                .into_iter()
                .next()
                .unwrap_or(CallResult::NotCallable {
                    type_id: union_type,
                })
        } else {
            // Should not reach here, but handle gracefully
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

    /// Expand a `TypeParameter` to its constraint (if it has one).
    /// This is used when a `TypeParameter` from an outer scope is used as an argument.
    fn expand_type_param(&self, ty: TypeId) -> TypeId {
        match self.interner.lookup(ty) {
            Some(TypeData::TypeParameter(tp)) => tp.constraint.unwrap_or(ty),
            _ => ty,
        }
    }

    /// Resolve a call to a simple function type.
    pub(crate) fn resolve_function_call(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> CallResult {
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

        // Handle generic functions
        if !func.type_params.is_empty() {
            return self.resolve_generic_call(func, arg_types);
        }

        if let Some(result) = self.check_argument_types(&func.params, arg_types, func.is_method) {
            return result;
        }

        CallResult::Success(func.return_type)
    }

    /// Resolve a call to a generic function by inferring type arguments.
    fn check_argument_types(
        &mut self,
        params: &[ParamInfo],
        arg_types: &[TypeId],
        allow_bivariant_callbacks: bool,
    ) -> Option<CallResult> {
        self.check_argument_types_with(params, arg_types, false, allow_bivariant_callbacks)
    }

    pub(crate) fn check_argument_types_with(
        &mut self,
        params: &[ParamInfo],
        arg_types: &[TypeId],
        strict: bool,
        allow_bivariant_callbacks: bool,
    ) -> Option<CallResult> {
        let arg_count = arg_types.len();
        for (i, arg_type) in arg_types.iter().enumerate() {
            let Some(param_type) = self.param_type_for_arg_index(params, i, arg_count) else {
                break;
            };

            if *arg_type == param_type {
                continue;
            }

            // Expand TypeParameters to their constraints for assignability checking when the
            // *parameter* expects a concrete type (e.g. `object`) but the argument is an outer
            // type parameter with a compatible constraint.
            //
            // IMPORTANT: Do **not** expand when the parameter type is itself a type parameter;
            // otherwise a call like `freeze(obj)` where `obj: T extends object` can incorrectly
            // compare `object` (expanded) against `T` and fail, even though inference would (and
            // tsc does) infer the inner `T` to the outer `T`.
            let expanded_arg_type = match self.interner.lookup(param_type) {
                Some(TypeData::TypeParameter(_) | TypeData::Infer(_)) => *arg_type,
                _ => self.expand_type_param(*arg_type),
            };

            if expanded_arg_type == param_type {
                continue;
            }

            let assignable = if allow_bivariant_callbacks || self.force_bivariant_callbacks {
                self.checker
                    .is_assignable_to_bivariant_callback(expanded_arg_type, param_type)
            } else if strict {
                let result = self
                    .checker
                    .is_assignable_to_strict(expanded_arg_type, param_type);
                if !result {
                    tracing::debug!(
                        "Strict assignability failed at index {}: {:?} <: {:?}",
                        i,
                        self.interner.lookup(expanded_arg_type),
                        self.interner.lookup(param_type)
                    );
                }
                result
            } else {
                self.checker.is_assignable_to(expanded_arg_type, param_type)
            };

            if !assignable {
                return Some(CallResult::ArgumentTypeMismatch {
                    index: i,
                    expected: param_type,
                    actual: *arg_type,
                });
            }
        }
        None
    }

    pub(crate) fn arg_count_bounds(&self, params: &[ParamInfo]) -> (usize, Option<usize>) {
        let required = params.iter().filter(|p| !p.optional && !p.rest).count();
        let rest_param = params.last().filter(|param| param.rest);
        let Some(rest_param) = rest_param else {
            return (required, Some(params.len()));
        };

        let rest_param_type = self.unwrap_readonly(rest_param.type_id);
        match self.interner.lookup(rest_param_type) {
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                let (rest_min, rest_max) = self.tuple_length_bounds(&elements);
                let min = required + rest_min;
                let max = rest_max.map(|max| required + max);
                (min, max)
            }
            _ => (required, None),
        }
    }

    pub(crate) fn param_type_for_arg_index(
        &self,
        params: &[ParamInfo],
        arg_index: usize,
        arg_count: usize,
    ) -> Option<TypeId> {
        let rest_param = params.last().filter(|param| param.rest);
        let rest_start = if rest_param.is_some() {
            params.len().saturating_sub(1)
        } else {
            params.len()
        };

        if arg_index < rest_start {
            return Some(params[arg_index].type_id);
        }

        let rest_param = rest_param?;
        let offset = arg_index - rest_start;
        let rest_arg_count = arg_count.saturating_sub(rest_start);

        let rest_param_type = self.unwrap_readonly(rest_param.type_id);
        trace!(
            rest_param_type_id = %rest_param_type.0,
            rest_param_type_key = ?self.interner.lookup(rest_param_type),
            "Extracting element type from rest parameter"
        );
        match self.interner.lookup(rest_param_type) {
            Some(TypeData::Array(elem)) => {
                trace!(
                    elem_type_id = %elem.0,
                    elem_type_key = ?self.interner.lookup(elem),
                    "Extracted array element type"
                );
                Some(elem)
            }
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                self.tuple_rest_element_type(&elements, offset, rest_arg_count)
            }
            other => {
                trace!(?other, "Rest param is not Array or Tuple, returning as-is");
                Some(rest_param_type)
            }
        }
    }

    fn tuple_length_bounds(&self, elements: &[TupleElement]) -> (usize, Option<usize>) {
        let mut min = 0usize;
        let mut max = 0usize;
        let mut variadic = false;

        for elem in elements {
            if elem.rest {
                let expansion = self.expand_tuple_rest(elem.type_id);
                for fixed in expansion.fixed {
                    max += 1;
                    if !fixed.optional {
                        min += 1;
                    }
                }
                if expansion.variadic.is_some() {
                    variadic = true;
                }
                // Count tail elements from nested tuple spreads
                for tail_elem in expansion.tail {
                    max += 1;
                    if !tail_elem.optional {
                        min += 1;
                    }
                }
                continue;
            }
            max += 1;
            if !elem.optional {
                min += 1;
            }
        }

        (min, if variadic { None } else { Some(max) })
    }

    fn tuple_rest_element_type(
        &self,
        elements: &[TupleElement],
        offset: usize,
        rest_arg_count: usize,
    ) -> Option<TypeId> {
        let rest_index = elements.iter().position(|elem| elem.rest);
        let Some(rest_index) = rest_index else {
            return elements.get(offset).map(|elem| elem.type_id);
        };

        let (prefix, rest_and_tail) = elements.split_at(rest_index);
        let rest_elem = &rest_and_tail[0];
        let outer_tail = &rest_and_tail[1..];

        let expansion = self.expand_tuple_rest(rest_elem.type_id);
        let prefix_len = prefix.len();
        let rest_fixed_len = expansion.fixed.len();
        let expansion_tail_len = expansion.tail.len();
        let outer_tail_len = outer_tail.len();
        // Total suffix = expansion.tail + outer_tail
        let total_suffix_len = expansion_tail_len + outer_tail_len;

        if let Some(variadic) = expansion.variadic {
            let suffix_start = rest_arg_count.saturating_sub(total_suffix_len);
            if offset >= suffix_start {
                let suffix_index = offset - suffix_start;
                // First check expansion.tail, then outer_tail
                if suffix_index < expansion_tail_len {
                    return Some(expansion.tail[suffix_index].type_id);
                }
                let outer_index = suffix_index - expansion_tail_len;
                return outer_tail.get(outer_index).map(|elem| elem.type_id);
            }
            if offset < prefix_len {
                return Some(prefix[offset].type_id);
            }
            let fixed_end = prefix_len + rest_fixed_len;
            if offset < fixed_end {
                return Some(expansion.fixed[offset - prefix_len].type_id);
            }
            return Some(variadic);
        }

        // No variadic: prefix + expansion.fixed + expansion.tail + outer_tail
        let mut index = offset;
        if index < prefix_len {
            return Some(prefix[index].type_id);
        }
        index -= prefix_len;
        if index < rest_fixed_len {
            return Some(expansion.fixed[index].type_id);
        }
        index -= rest_fixed_len;
        if index < expansion_tail_len {
            return Some(expansion.tail[index].type_id);
        }
        index -= expansion_tail_len;
        outer_tail.get(index).map(|elem| elem.type_id)
    }

    pub(crate) fn rest_element_type(&self, type_id: TypeId) -> TypeId {
        match self.interner.lookup(type_id) {
            Some(TypeData::Array(elem)) => elem,
            _ => type_id,
        }
    }

    /// Maximum iterations for type unwrapping loops to prevent infinite loops.
    const MAX_UNWRAP_ITERATIONS: usize = 1000;

    fn unwrap_readonly(&self, mut type_id: TypeId) -> TypeId {
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > Self::MAX_UNWRAP_ITERATIONS {
                // Safety limit reached - return current type to prevent infinite loop
                return type_id;
            }
            match self.interner.lookup(type_id) {
                Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
                    type_id = inner;
                }
                _ => return type_id,
            }
        }
    }

    fn expand_tuple_rest(&self, type_id: TypeId) -> TupleRestExpansion {
        match self.interner.lookup(type_id) {
            Some(TypeData::Array(elem)) => TupleRestExpansion {
                fixed: Vec::new(),
                variadic: Some(elem),
                tail: Vec::new(),
            },
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                let mut fixed = Vec::new();
                for (i, elem) in elements.iter().enumerate() {
                    if elem.rest {
                        let inner = self.expand_tuple_rest(elem.type_id);
                        fixed.extend(inner.fixed);
                        // Capture tail elements: inner.tail + elements after the rest
                        let mut tail = inner.tail;
                        tail.extend(elements[i + 1..].iter().cloned());
                        return TupleRestExpansion {
                            fixed,
                            variadic: inner.variadic,
                            tail,
                        };
                    }
                    fixed.push(elem.clone());
                }
                TupleRestExpansion {
                    fixed,
                    variadic: None,
                    tail: Vec::new(),
                }
            }
            _ => TupleRestExpansion {
                fixed: Vec::new(),
                variadic: Some(type_id),
                tail: Vec::new(),
            },
        }
    }

    pub(crate) fn rest_tuple_inference_target(
        &mut self,
        params: &[ParamInfo],
        arg_types: &[TypeId],
        var_map: &FxHashMap<TypeId, crate::infer::InferenceVar>,
    ) -> Option<(usize, TypeId, TypeId)> {
        let rest_param = params.last().filter(|param| param.rest)?;
        let rest_start = params.len().saturating_sub(1);

        let rest_param_type = self.unwrap_readonly(rest_param.type_id);
        let target = match self.interner.lookup(rest_param_type) {
            Some(TypeData::TypeParameter(_)) if var_map.contains_key(&rest_param_type) => {
                Some((rest_start, rest_param_type, 0))
            }
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                elements.iter().enumerate().find_map(|(i, elem)| {
                    if !elem.rest {
                        return None;
                    }
                    if !var_map.contains_key(&elem.type_id) {
                        return None;
                    }

                    // Count trailing elements after the variadic part, but allow optional
                    // tail elements to be omitted when they don't match.
                    let tail = &elements[i + 1..];
                    let min_index = rest_start + i;
                    let mut trailing_count = 0usize;
                    let mut arg_index = arg_types.len();
                    for tail_elem in tail.iter().rev() {
                        if arg_index <= min_index {
                            break;
                        }
                        let arg_type = arg_types[arg_index - 1];
                        let assignable = self.checker.is_assignable_to(arg_type, tail_elem.type_id);
                        if tail_elem.optional && !assignable {
                            break;
                        }
                        trailing_count += 1;
                        arg_index -= 1;
                    }
                    Some((rest_start + i, elem.type_id, trailing_count))
                })
            }
            _ => None,
        }?;

        let (start_index, target_type, trailing_count) = target;
        if start_index >= arg_types.len() {
            return None;
        }

        // Extract the arguments that should be inferred for the variadic type parameter,
        // excluding both prefix fixed elements and trailing fixed elements.
        // For example, for `...args: [number, ...T, boolean]` with call `foo(1, 'a', 'b', true)`:
        //   - rest_start = 0 (rest param index)
        //   - start_index = 1 (after the prefix `number`)
        //   - trailing_count = 1 (the trailing `boolean`)
        //   - we should infer T from ['a', 'b'], not [1, 'a', 'b', true]
        //
        // The variadic arguments start at start_index and end before trailing elements.
        let end_index = arg_types.len().saturating_sub(trailing_count);
        let tuple_elements: Vec<TupleElement> = if start_index < end_index {
            arg_types[start_index..end_index]
                .iter()
                .map(|&ty| TupleElement {
                    type_id: ty,
                    name: None,
                    optional: false,
                    rest: false,
                })
                .collect()
        } else {
            Vec::new()
        };
        Some((
            start_index,
            target_type,
            self.interner.tuple(tuple_elements),
        ))
    }

    pub(crate) fn type_contains_placeholder(
        &self,
        ty: TypeId,
        var_map: &FxHashMap<TypeId, crate::infer::InferenceVar>,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if var_map.contains_key(&ty) {
            return true;
        }
        if !visited.insert(ty) {
            return false;
        }

        let key = match self.interner.lookup(ty) {
            Some(key) => key,
            None => return false,
        };

        match key {
            TypeData::Array(elem) => self.type_contains_placeholder(elem, var_map, visited),
            TypeData::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                elements
                    .iter()
                    .any(|elem| self.type_contains_placeholder(elem.type_id, var_map, visited))
            }
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|&member| self.type_contains_placeholder(member, var_map, visited))
            }
            TypeData::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_placeholder(prop.type_id, var_map, visited))
            }
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_placeholder(prop.type_id, var_map, visited))
                    || shape.string_index.as_ref().is_some_and(|idx| {
                        self.type_contains_placeholder(idx.key_type, var_map, visited)
                            || self.type_contains_placeholder(idx.value_type, var_map, visited)
                    })
                    || shape.number_index.as_ref().is_some_and(|idx| {
                        self.type_contains_placeholder(idx.key_type, var_map, visited)
                            || self.type_contains_placeholder(idx.value_type, var_map, visited)
                    })
            }
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.type_contains_placeholder(app.base, var_map, visited)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.type_contains_placeholder(arg, var_map, visited))
            }
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                shape.type_params.iter().any(|tp| {
                    tp.constraint.is_some_and(|constraint| {
                        self.type_contains_placeholder(constraint, var_map, visited)
                    }) || tp.default.is_some_and(|default| {
                        self.type_contains_placeholder(default, var_map, visited)
                    })
                }) || shape
                    .params
                    .iter()
                    .any(|param| self.type_contains_placeholder(param.type_id, var_map, visited))
                    || shape.this_type.is_some_and(|this_type| {
                        self.type_contains_placeholder(this_type, var_map, visited)
                    })
                    || self.type_contains_placeholder(shape.return_type, var_map, visited)
                    || shape.type_predicate.as_ref().is_some_and(|pred| {
                        pred.type_id
                            .is_some_and(|ty| self.type_contains_placeholder(ty, var_map, visited))
                    })
            }
            TypeData::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                let in_call = shape.call_signatures.iter().any(|sig| {
                    sig.type_params.iter().any(|tp| {
                        tp.constraint.is_some_and(|constraint| {
                            self.type_contains_placeholder(constraint, var_map, visited)
                        }) || tp.default.is_some_and(|default| {
                            self.type_contains_placeholder(default, var_map, visited)
                        })
                    }) || sig.params.iter().any(|param| {
                        self.type_contains_placeholder(param.type_id, var_map, visited)
                    }) || sig.this_type.is_some_and(|this_type| {
                        self.type_contains_placeholder(this_type, var_map, visited)
                    }) || self.type_contains_placeholder(sig.return_type, var_map, visited)
                        || sig.type_predicate.as_ref().is_some_and(|pred| {
                            pred.type_id.is_some_and(|ty| {
                                self.type_contains_placeholder(ty, var_map, visited)
                            })
                        })
                });
                if in_call {
                    return true;
                }
                let in_construct = shape.construct_signatures.iter().any(|sig| {
                    sig.type_params.iter().any(|tp| {
                        tp.constraint.is_some_and(|constraint| {
                            self.type_contains_placeholder(constraint, var_map, visited)
                        }) || tp.default.is_some_and(|default| {
                            self.type_contains_placeholder(default, var_map, visited)
                        })
                    }) || sig.params.iter().any(|param| {
                        self.type_contains_placeholder(param.type_id, var_map, visited)
                    }) || sig.this_type.is_some_and(|this_type| {
                        self.type_contains_placeholder(this_type, var_map, visited)
                    }) || self.type_contains_placeholder(sig.return_type, var_map, visited)
                        || sig.type_predicate.as_ref().is_some_and(|pred| {
                            pred.type_id.is_some_and(|ty| {
                                self.type_contains_placeholder(ty, var_map, visited)
                            })
                        })
                });
                if in_construct {
                    return true;
                }
                shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_placeholder(prop.type_id, var_map, visited))
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(cond_id);
                self.type_contains_placeholder(cond.check_type, var_map, visited)
                    || self.type_contains_placeholder(cond.extends_type, var_map, visited)
                    || self.type_contains_placeholder(cond.true_type, var_map, visited)
                    || self.type_contains_placeholder(cond.false_type, var_map, visited)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(mapped_id);
                mapped.type_param.constraint.is_some_and(|constraint| {
                    self.type_contains_placeholder(constraint, var_map, visited)
                }) || mapped.type_param.default.is_some_and(|default| {
                    self.type_contains_placeholder(default, var_map, visited)
                }) || self.type_contains_placeholder(mapped.constraint, var_map, visited)
                    || self.type_contains_placeholder(mapped.template, var_map, visited)
            }
            TypeData::IndexAccess(obj, idx) => {
                self.type_contains_placeholder(obj, var_map, visited)
                    || self.type_contains_placeholder(idx, var_map, visited)
            }
            TypeData::KeyOf(operand)
            | TypeData::ReadonlyType(operand)
            | TypeData::NoInfer(operand) => {
                self.type_contains_placeholder(operand, var_map, visited)
            }
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => {
                        self.type_contains_placeholder(*inner, var_map, visited)
                    }
                })
            }
            TypeData::StringIntrinsic { type_arg, .. } => {
                self.type_contains_placeholder(type_arg, var_map, visited)
            }
            TypeData::Enum(_def_id, member_type) => {
                self.type_contains_placeholder(member_type, var_map, visited)
            }
            TypeData::TypeParameter(_)
            | TypeData::Infer(_)
            | TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::BoundParameter(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ThisType
            | TypeData::ModuleNamespace(_)
            | TypeData::Error => false,
        }
    }

    /// Check if a type is contextually sensitive (requires contextual typing for inference).
    ///
    /// Contextually sensitive types include:
    /// - Function types (lambda expressions)
    /// - Callable types (object with call signatures)
    /// - Union/Intersection types containing contextually sensitive members
    /// - Object literals with callable properties (methods)
    ///
    /// These types need deferred inference in Round 2 after non-contextual
    /// arguments have been processed and type variables have been fixed.
    pub(crate) fn is_contextually_sensitive(&self, type_id: TypeId) -> bool {
        let key = match self.interner.lookup(type_id) {
            Some(key) => key,
            None => return false,
        };

        match key {
            // Function and callable types are contextually sensitive (lambdas or objects
            // with call signatures).
            TypeData::Function(_) | TypeData::Callable(_) => true,

            // Union/Intersection: contextually sensitive if any member is
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|&member| self.is_contextually_sensitive(member))
            }

            // Object types: check if any property is callable (has methods)
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|prop| self.is_contextually_sensitive(prop.type_id))
            }

            // Array types: check element type
            TypeData::Array(elem) => self.is_contextually_sensitive(elem),

            // Tuple types: check all elements
            TypeData::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                elements
                    .iter()
                    .any(|elem| self.is_contextually_sensitive(elem.type_id))
            }

            // Type applications: check base and arguments
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.is_contextually_sensitive(app.base)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.is_contextually_sensitive(arg))
            }

            // Readonly types: look through to inner type
            TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                self.is_contextually_sensitive(inner)
            }

            // Type parameters with constraints: check constraint
            TypeData::TypeParameter(info) | TypeData::Infer(info) => info
                .constraint
                .is_some_and(|constraint| self.is_contextually_sensitive(constraint)),

            // Index access: check both object and key types
            TypeData::IndexAccess(obj, key) => {
                self.is_contextually_sensitive(obj) || self.is_contextually_sensitive(key)
            }

            // Conditional types: check all branches
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(cond_id);
                self.is_contextually_sensitive(cond.check_type)
                    || self.is_contextually_sensitive(cond.extends_type)
                    || self.is_contextually_sensitive(cond.true_type)
                    || self.is_contextually_sensitive(cond.false_type)
            }

            // Mapped types: check constraint and template
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(mapped_id);
                self.is_contextually_sensitive(mapped.constraint)
                    || self.is_contextually_sensitive(mapped.template)
            }

            // KeyOf, StringIntrinsic: check operand
            TypeData::KeyOf(operand)
            | TypeData::StringIntrinsic {
                type_arg: operand, ..
            } => self.is_contextually_sensitive(operand),

            // Enum types: check member type
            TypeData::Enum(_def_id, member_type) => self.is_contextually_sensitive(member_type),

            // Template literals: check type spans
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => self.is_contextually_sensitive(*inner),
                })
            }

            // Non-contextually sensitive types
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
        }
    }

    /// Resolve a call to a callable type (with overloads).
    fn resolve_callable_call(
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
                type_predicate: sig.type_predicate.clone(),
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
        let mut sole_type_mismatch: Option<(usize, TypeId, TypeId)> = None; // (index, expected, actual)
        let mut has_non_count_non_type_failure = false;

        for sig in &callable.call_signatures {
            // Convert CallSignature to FunctionShape
            let func = FunctionShape {
                params: sig.params.clone(),
                this_type: sig.this_type,
                return_type: sig.return_type,
                type_params: sig.type_params.clone(),
                type_predicate: sig.type_predicate.clone(),
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
                } => {
                    all_arg_count_mismatches = false;
                    type_mismatch_count += 1;
                    if type_mismatch_count == 1 {
                        sole_type_mismatch = Some((index, expected, actual));
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
                    let expected = expected_max.unwrap_or(expected_min);
                    min_expected = min_expected.min(expected_min);
                    max_expected = max_expected.max(expected);
                    failures.push(
                        crate::diagnostics::PendingDiagnosticBuilder::argument_count_mismatch(
                            expected, actual,
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

        // If exactly one overload had a type mismatch (all others failed on arg count),
        // report TS2345 (the inner type error) instead of TS2769. This matches tsc's
        // "best candidate" behavior: when one overload clearly matches by arity but
        // fails on types, that overload's type error is surfaced directly.
        if !has_non_count_non_type_failure
            && type_mismatch_count == 1
            && let Some((index, expected, actual)) = sole_type_mismatch
        {
            return CallResult::ArgumentTypeMismatch {
                index,
                expected,
                actual,
            };
        }

        // If we got here, no signature matched
        CallResult::NoOverloadMatch {
            func_type: self.interner.callable(callable.clone()),
            arg_types: arg_types.to_vec(),
            failures,
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

// Re-exports from extracted modules
pub use crate::operations_generics::{GenericInstantiationResult, solve_generic_instantiation};
pub use crate::operations_iterators::{
    IteratorInfo, get_async_iterable_element_type, get_iterator_info,
};

#[cfg(test)]
#[path = "../tests/operations_tests.rs"]
mod tests;
