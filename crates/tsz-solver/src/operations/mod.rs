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
//! Components are organized into separate modules:
//! - `binary_ops`: Binary operation evaluation (+, -, *, /, etc.)
//! - `call_args`: Argument checking, parameter analysis, tuple rest handling, placeholder detection
//! - `constraints`: Type constraint collection for generic inference
//! - `constructors`: Constructor (new) expression resolution
//! - `generic_call`: Generic function call inference
//! - `generics`: Generic type instantiation validation
//! - `iterators`: Iterator/async iterator type extraction
//! - `property`: Property access resolution (includes helpers for mapped, primitive, array, etc.)
//! - `property_readonly`: Readonly property checks
//! - `property_visitor`: `TypeVisitor` impl for `PropertyAccessEvaluator`

pub mod binary_ops;
mod call_args;
mod constraints;
mod constructors;
mod generic_call;
pub mod generics;
pub mod iterators;
pub mod property;
mod property_readonly;
mod property_visitor;

// Re-exports from submodules
pub use binary_ops::{BinaryOpEvaluator, BinaryOpResult, PrimitiveClass};

use crate::diagnostics::PendingDiagnostic;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
#[cfg(test)]
use crate::types::*;
use crate::types::{
    CallSignature, CallableShape, CallableShapeId, FunctionShape, FunctionShapeId, IntrinsicKind,
    LiteralValue, ParamInfo, TypeData, TypeId, TypeListId, TypePredicate,
};
use crate::visitor::TypeVisitor;
use crate::{QueryDatabase, TypeDatabase};
use rustc_hash::FxHashSet;
use std::cell::RefCell;
use tracing::debug;

/// Maximum recursion depth for type constraint collection to prevent infinite loops.
pub const MAX_CONSTRAINT_RECURSION_DEPTH: usize = 100;
/// Maximum number of constrain-types steps per call evaluator pass.
/// This caps pathological recursive inference explosions while preserving
/// normal inference behavior on real-world calls.
pub const MAX_CONSTRAINT_STEPS: usize = 20_000;

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

    /// TS2350: Only a void function can be called with the 'new' keyword.
    NonVoidFunctionCalledWithNew,

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
    force_bivariant_callbacks: bool,
    /// Contextual type for the call expression's expected result
    /// Used for contextual type inference in generic functions
    pub(crate) contextual_type: Option<TypeId>,
    /// The `this` type provided by the caller (e.g. `obj` in `obj.method()`)
    pub(crate) actual_this_type: Option<TypeId>,
    /// Current recursion depth for `constrain_types` to prevent infinite loops
    pub(crate) constraint_recursion_depth: RefCell<usize>,
    /// Total constrain-types steps for the current inference pass.
    pub(crate) constraint_step_count: RefCell<usize>,
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
            constraint_step_count: RefCell::new(0),
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
            TypeData::Lazy(_)
            | TypeData::Conditional(_)
            | TypeData::IndexAccess(_, _)
            | TypeData::Mapped(_)
            | TypeData::TemplateLiteral(_) => {
                // Resolve meta-types to their actual types before checking callability.
                // This handles cases like conditional types that resolve to function types,
                // index access types like T["method"], and mapped types.
                let resolved = crate::evaluation::evaluate::evaluate_type(self.interner, func_type);
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
            };
        }

        // If we got here, no signature matched
        CallResult::NoOverloadMatch {
            func_type: self.interner.callable(callable.clone()),
            arg_types: arg_types.to_vec(),
            failures,
            fallback_return: callable
                .call_signatures
                .first()
                .map(|s| s.return_type)
                .unwrap_or(TypeId::ANY),
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
) -> (CallResult, Option<(TypePredicate, Vec<ParamInfo>)>) {
    let mut evaluator = CallEvaluator::new(interner, checker);
    evaluator.set_force_bivariant_callbacks(force_bivariant_callbacks);
    evaluator.set_contextual_type(contextual_type);
    evaluator.set_actual_this_type(actual_this_type);
    let result = evaluator.resolve_call(func_type, arg_types);
    let predicate = evaluator.last_instantiated_predicate.take();
    (result, predicate)
}

pub fn resolve_new_with_checker<C: AssignabilityChecker>(
    interner: &dyn QueryDatabase,
    checker: &mut C,
    type_id: TypeId,
    arg_types: &[TypeId],
    force_bivariant_callbacks: bool,
) -> CallResult {
    let mut evaluator = CallEvaluator::new(interner, checker);
    evaluator.set_force_bivariant_callbacks(force_bivariant_callbacks);
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

// Re-exports from submodules
pub use generics::{GenericInstantiationResult, solve_generic_instantiation};
pub use iterators::{IteratorInfo, get_async_iterable_element_type, get_iterator_info};

#[cfg(test)]
#[path = "../../tests/operations_tests.rs"]
mod tests;
