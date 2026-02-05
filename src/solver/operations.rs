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
#[allow(unused_imports)]
pub use crate::solver::binary_ops::{BinaryOpEvaluator, BinaryOpResult, PrimitiveClass};

use crate::interner::Atom;
use crate::solver::diagnostics::PendingDiagnostic;
use crate::solver::infer::InferenceContext;
use crate::solver::instantiate::{TypeSubstitution, instantiate_type};
use crate::solver::types::*;
use crate::solver::utils;
use crate::solver::visitor::TypeVisitor;
use crate::solver::{QueryDatabase, TypeDatabase};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;

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

    /// Argument count mismatch
    ArgumentCountMismatch {
        expected_min: usize,
        expected_max: Option<usize>,
        actual: usize,
    },

    /// Argument type mismatch at specific position
    ArgumentTypeMismatch {
        index: usize,
        expected: TypeId,
        actual: TypeId,
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
    interner: &'a dyn QueryDatabase,
    checker: &'a mut C,
    defaulted_placeholders: FxHashSet<TypeId>,
    force_bivariant_callbacks: bool,
    /// Contextual type for the call expression's expected result
    /// Used for contextual type inference in generic functions
    contextual_type: Option<TypeId>,
    /// Current recursion depth for constrain_types to prevent infinite loops
    constraint_recursion_depth: RefCell<usize>,
    /// Visited (source, target) pairs during constraint collection.
    constraint_pairs: RefCell<FxHashSet<(TypeId, TypeId)>>,
}

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    pub fn new(interner: &'a dyn QueryDatabase, checker: &'a mut C) -> Self {
        CallEvaluator {
            interner,
            checker,
            defaulted_placeholders: FxHashSet::default(),
            force_bivariant_callbacks: false,
            contextual_type: None,
            constraint_recursion_depth: RefCell::new(0),
            constraint_pairs: RefCell::new(FxHashSet::default()),
        }
    }

    /// Set the contextual type for this call evaluation.
    /// This is used for contextual type inference when the expected return type
    /// can help constrain generic type parameters.
    /// Example: `let x: string = id(42)` should infer `T = string` from the context.
    pub fn set_contextual_type(&mut self, ctx_type: Option<TypeId>) {
        self.contextual_type = ctx_type;
    }

    pub fn set_force_bivariant_callbacks(&mut self, enabled: bool) {
        self.force_bivariant_callbacks = enabled;
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
            CallResult::ArgumentTypeMismatch { .. } => TypeId::ERROR,
            _ => TypeId::ERROR,
        }
    }

    pub fn infer_generic_function(&mut self, func: &FunctionShape, arg_types: &[TypeId]) -> TypeId {
        match self.resolve_function_call(func, arg_types) {
            CallResult::Success(ret) => ret,
            // Return ERROR instead of ANY to avoid silencing TS2322 errors
            CallResult::ArgumentTypeMismatch { .. } => TypeId::ERROR,
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
        struct ContextualSignatureVisitor<'a> {
            db: &'a dyn TypeDatabase,
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

                // For contextual typing, we look at call signatures (not construct signatures).
                // If there are multiple (overloads), we pick the first one for now.
                // TODO: Handle overloads properly by selecting the best match
                if let Some(sig) = shape.call_signatures.first() {
                    Some(FunctionShape {
                        type_params: sig.type_params.clone(),
                        params: sig.params.clone(),
                        this_type: sig.this_type,
                        return_type: sig.return_type,
                        type_predicate: sig.type_predicate.clone(),
                        is_constructor: false,
                        is_method: sig.is_method,
                    })
                } else {
                    None
                }
            }

            fn visit_application(&mut self, app_id: u32) -> Self::Output {
                use crate::solver::types::TypeApplicationId;

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

            // Future: Handle Union (return None or intersect of params)
            // Future: Handle Intersection (pick first callable member)
        }

        let mut visitor = ContextualSignatureVisitor { db };
        visitor.visit_type(db, type_id)
    }

    /// Resolve a function call: func(args...) -> result
    ///
    /// This is pure type logic - no AST nodes, just types in and types out.
    pub fn resolve_call(&mut self, func_type: TypeId, arg_types: &[TypeId]) -> CallResult {
        // Look up the function shape
        let key = match self.interner.lookup(func_type) {
            Some(k) => k,
            None => return CallResult::NotCallable { type_id: func_type },
        };

        match key {
            TypeKey::Function(f_id) => {
                let shape = self.interner.function_shape(f_id);
                self.resolve_function_call(shape.as_ref(), arg_types)
            }
            TypeKey::Callable(c_id) => {
                let shape = self.interner.callable_shape(c_id);
                self.resolve_callable_call(shape.as_ref(), arg_types)
            }
            TypeKey::Union(list_id) => {
                // Handle union types: if all members are callable with compatible signatures,
                // the union is callable
                self.resolve_union_call(func_type, list_id, arg_types)
            }
            TypeKey::Intersection(list_id) => {
                // Handle intersection types: if any member is callable, use that
                // This handles cases like: Function & { prop: number }
                self.resolve_intersection_call(func_type, list_id, arg_types)
            }
            TypeKey::Application(app_id) => {
                // Handle Application types (e.g., GenericCallable<string>)
                // Get the application and resolve the call on its base type
                let app = self.interner.type_application(app_id);
                // Resolve the call on the base type with type arguments applied
                // The application's base should already be a callable type after type evaluation
                self.resolve_call(app.base, arg_types)
            }
            TypeKey::TypeParameter(param_info) => {
                // For type parameters with callable constraints (e.g., T extends { (): string }),
                // resolve the call using the constraint type
                if let Some(constraint) = param_info.constraint {
                    self.resolve_call(constraint, arg_types)
                } else {
                    CallResult::NotCallable { type_id: func_type }
                }
            }
            _ => CallResult::NotCallable { type_id: func_type },
        }
    }

    /// Resolve a call on a union type.
    ///
    /// This handles cases like:
    /// - `(() => void) | (() => string)` - all members callable
    /// - `string | (() => void)` - mixed callable/non-callable (returns NotCallable)
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

    /// Expand a TypeParameter to its constraint (if it has one).
    /// This is used when a TypeParameter from an outer scope is used as an argument.
    fn expand_type_param(&self, ty: TypeId) -> TypeId {
        match self.interner.lookup(ty) {
            Some(TypeKey::TypeParameter(tp)) => tp.constraint.unwrap_or(ty),
            _ => ty,
        }
    }

    /// Resolve a call to a simple function type.
    fn resolve_function_call(&mut self, func: &FunctionShape, arg_types: &[TypeId]) -> CallResult {
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
    fn resolve_generic_call(&mut self, func: &FunctionShape, arg_types: &[TypeId]) -> CallResult {
        let previous_defaulted = std::mem::take(&mut self.defaulted_placeholders);
        let result = self.resolve_generic_call_inner(func, arg_types);
        self.defaulted_placeholders = previous_defaulted;
        result
    }

    fn resolve_generic_call_inner(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> CallResult {
        // Check argument count BEFORE type inference
        // This prevents false positive TS2554 errors for generic functions with optional/rest params
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

        let mut infer_ctx = InferenceContext::new(self.interner.as_type_database());
        let mut substitution = TypeSubstitution::new();
        let mut var_map: FxHashMap<TypeId, crate::solver::infer::InferenceVar> =
            FxHashMap::default();
        let mut type_param_vars = Vec::with_capacity(func.type_params.len());

        self.constraint_pairs.borrow_mut().clear();
        *self.constraint_recursion_depth.borrow_mut() = 0;

        // 1. Create inference variables and placeholders for each type parameter
        for tp in &func.type_params {
            // Allocate an inference variable first, then create a *unique* placeholder type
            // for that variable. We register the placeholder name (not the original type
            // parameter name) with the inference context so occurs-checks don't get confused
            // by identically-named type parameters from outer scopes (e.g., `T` inside `T`).
            let var = infer_ctx.fresh_var();
            type_param_vars.push(var);

            // Create a unique placeholder type for this inference variable
            // We use a TypeParameter with a special name to track it during constraint collection
            let placeholder_name = format!("__infer_{}", var.0);
            let placeholder_atom = self.interner.intern_string(&placeholder_name);
            infer_ctx.register_type_param(placeholder_atom, var, tp.is_const);
            let placeholder_key = TypeKey::TypeParameter(TypeParamInfo {
                is_const: tp.is_const,
                name: placeholder_atom,
                constraint: tp.constraint,
                default: None,
            });
            let placeholder_id = self.interner.intern(placeholder_key);

            substitution.insert(tp.name, placeholder_id);
            var_map.insert(placeholder_id, var);

            // Add the type parameter constraint as an upper bound, but only if the
            // constraint is concrete (doesn't reference other type params via placeholders).
            // Constraints like `keyof T` that depend on other type params can't be evaluated
            // during resolution since T may not be resolved yet. These are validated in the
            // post-resolution constraint check below.
            if let Some(constraint) = tp.constraint {
                let inst_constraint = instantiate_type(self.interner, constraint, &substitution);
                let mut visited = FxHashSet::default();
                if !self.type_contains_placeholder(inst_constraint, &var_map, &mut visited) {
                    infer_ctx.add_upper_bound(var, inst_constraint);
                }
            }

            if tp.default.is_some() {
                self.defaulted_placeholders.insert(placeholder_id);
            }
        }

        // 2. Instantiate parameters with placeholders
        let instantiated_params: Vec<ParamInfo> = func
            .params
            .iter()
            .map(|p| ParamInfo {
                name: p.name,
                type_id: instantiate_type(self.interner, p.type_id, &substitution),
                optional: p.optional,
                rest: p.rest,
            })
            .collect();

        // 2.5. Seed contextual constraints from return type BEFORE argument processing
        // This enables downward inference: `let x: string = id(...)` should infer T = string
        // Contextual hints use lower priority so explicit arguments can override
        if let Some(ctx_type) = self.contextual_type {
            let return_type_with_placeholders =
                instantiate_type(self.interner, func.return_type, &substitution);
            // CORRECT: return_type <: ctx_type
            // In assignment `let x: Target = Source`, the relation is `Source <: Target`
            // Therefore, the return value must be assignable to the expected type
            self.constrain_types(
                &mut infer_ctx,
                &var_map,
                return_type_with_placeholders, // source
                ctx_type,                      // target
                crate::solver::types::InferencePriority::ReturnType,
            );
        }

        // 3. Multi-pass constraint collection for proper contextual typing

        // Prepare rest tuple inference info
        let rest_tuple_inference =
            self.rest_tuple_inference_target(&instantiated_params, arg_types, &var_map);
        let rest_tuple_start = rest_tuple_inference.as_ref().map(|(start, _, _)| *start);

        // === Round 1: Process non-contextual arguments ===
        // These are arguments like arrays, primitives, and objects that don't need
        // contextual typing. Processing them first allows us to infer type parameters
        // that contextual arguments (lambdas) can then use.
        for (i, &arg_type) in arg_types.iter().enumerate() {
            if rest_tuple_start.is_some_and(|start| i >= start) {
                continue;
            }
            let Some(target_type) =
                self.param_type_for_arg_index(&instantiated_params, i, arg_types.len())
            else {
                break;
            };

            // Skip contextually sensitive arguments (will process in Round 2)
            if self.is_contextually_sensitive(arg_type) {
                continue;
            }

            let mut visited = FxHashSet::default();
            if !self.type_contains_placeholder(target_type, &var_map, &mut visited) {
                // No placeholder in target_type - check assignability directly
                if !self.checker.is_assignable_to(arg_type, target_type) {
                    return CallResult::ArgumentTypeMismatch {
                        index: i,
                        expected: target_type,
                        actual: arg_type,
                    };
                }
            } else {
                // Target type contains placeholders - check against their constraints
                if let Some(TypeKey::TypeParameter(tp)) = self.interner.lookup(target_type)
                    && let Some(constraint) = tp.constraint
                {
                    let inst_constraint =
                        instantiate_type(self.interner, constraint, &substitution);
                    let mut constraint_visited = FxHashSet::default();
                    if !self.type_contains_placeholder(
                        inst_constraint,
                        &var_map,
                        &mut constraint_visited,
                    ) {
                        // Constraint is fully concrete - safe to check now
                        if !self.checker.is_assignable_to(arg_type, inst_constraint) {
                            return CallResult::ArgumentTypeMismatch {
                                index: i,
                                expected: inst_constraint,
                                actual: arg_type,
                            };
                        }
                    }
                }
            }

            // arg_type <: target_type
            self.constrain_types(
                &mut infer_ctx,
                &var_map,
                arg_type,
                target_type,
                crate::solver::types::InferencePriority::NakedTypeVariable,
            );
        }

        // Process rest tuple in Round 1 (it's non-contextual)
        if let Some((_start, target_type, tuple_type)) = rest_tuple_inference {
            self.constrain_types(
                &mut infer_ctx,
                &var_map,
                tuple_type,
                target_type,
                crate::solver::types::InferencePriority::NakedTypeVariable,
            );
        }

        // === Fixing: Resolve variables with enough information ===
        // This "fixes" type variables that have candidates from Round 1,
        // preventing Round 2 from overriding them with lower-priority constraints.
        if let Err(_) = infer_ctx.fix_current_variables() {
            // Fixing failed - this might indicate a constraint conflict
            // Continue with partial fixing, final resolution will detect errors
        }

        // === Round 2: Process contextual arguments ===
        // These are arguments like lambdas that need contextual typing.
        // Now that non-contextual arguments have been processed, we can provide
        // proper contextual types to lambdas based on fixed type variables.
        for (i, &arg_type) in arg_types.iter().enumerate() {
            if rest_tuple_start.is_some_and(|start| i >= start) {
                continue;
            }
            let Some(target_type) =
                self.param_type_for_arg_index(&instantiated_params, i, arg_types.len())
            else {
                break;
            };

            // Only process contextually sensitive arguments in Round 2
            if !self.is_contextually_sensitive(arg_type) {
                continue;
            }

            // Get current substitution with fixed types from Round 1
            // The substitution maps placeholder names (like __infer_0) to their resolved types
            let placeholder_subst = infer_ctx.get_current_substitution();
            let contextual_target =
                instantiate_type(self.interner, target_type, &placeholder_subst);

            // Check assignability with contextual target type
            let mut visited = FxHashSet::default();
            if !self.type_contains_placeholder(contextual_target, &var_map, &mut visited) {
                // No placeholder in contextual_target - check assignability directly
                if !self.checker.is_assignable_to(arg_type, contextual_target) {
                    return CallResult::ArgumentTypeMismatch {
                        index: i,
                        expected: contextual_target,
                        actual: arg_type,
                    };
                }
            } else {
                // Target type contains placeholders - check against their constraints
                if let Some(TypeKey::TypeParameter(tp)) = self.interner.lookup(contextual_target)
                    && let Some(constraint) = tp.constraint
                {
                    let inst_constraint =
                        instantiate_type(self.interner, constraint, &placeholder_subst);
                    let mut constraint_visited = FxHashSet::default();
                    if !self.type_contains_placeholder(
                        inst_constraint,
                        &var_map,
                        &mut constraint_visited,
                    ) {
                        // Constraint is fully concrete - safe to check now
                        if !self.checker.is_assignable_to(arg_type, inst_constraint) {
                            return CallResult::ArgumentTypeMismatch {
                                index: i,
                                expected: inst_constraint,
                                actual: arg_type,
                            };
                        }
                    }
                }
            }

            // arg_type <: contextual_target (with ReturnType priority for contextual args)
            self.constrain_types(
                &mut infer_ctx,
                &var_map,
                arg_type,
                contextual_target,
                crate::solver::types::InferencePriority::ReturnType,
            );
        }

        // 4. Resolve inference variables
        let mut final_subst = TypeSubstitution::new();
        for (tp, &var) in func.type_params.iter().zip(type_param_vars.iter()) {
            let has_constraints = infer_ctx
                .get_constraints(var)
                .is_some_and(|c| !c.is_empty());

            let ty = if has_constraints {
                match infer_ctx.resolve_with_constraints_by(var, |source, target| {
                    self.checker.is_assignable_to(source, target)
                }) {
                    Ok(ty) => ty,
                    Err(_) => {
                        // Inference from constraints failed - try fallback options
                        // Use ERROR as ultimate fallback when constraints exist but inference fails
                        // (this indicates a real type conflict that should be reported)
                        if let Some(default) = tp.default {
                            instantiate_type(self.interner, default, &final_subst)
                        } else if let Some(constraint) = tp.constraint {
                            instantiate_type(self.interner, constraint, &final_subst)
                        } else {
                            TypeId::ERROR
                        }
                    }
                }
            } else if let Some(default) = tp.default {
                instantiate_type(self.interner, default, &final_subst)
            } else if let Some(constraint) = tp.constraint {
                instantiate_type(self.interner, constraint, &final_subst)
            } else {
                // TypeScript infers 'unknown' for unconstrained type parameters without defaults
                TypeId::UNKNOWN
            };

            final_subst.insert(tp.name, ty);

            if let Some(constraint) = tp.constraint {
                let constraint_ty = instantiate_type(self.interner, constraint, &final_subst);
                if !self.checker.is_assignable_to(ty, constraint_ty) {
                    // Inferred type doesn't satisfy constraint - report as type mismatch
                    // This allows the checker to emit TS2322 errors instead of silently accepting Any/ERROR
                    return CallResult::ArgumentTypeMismatch {
                        index: 0, // Placeholder - indicates a constraint violation occurred
                        expected: constraint_ty,
                        actual: ty,
                    };
                }
            }
        }

        let instantiated_params: Vec<ParamInfo> = func
            .params
            .iter()
            .map(|p| {
                let instantiated = instantiate_type(self.interner, p.type_id, &final_subst);
                ParamInfo {
                    name: p.name,
                    type_id: instantiated,
                    optional: p.optional,
                    rest: p.rest,
                }
            })
            .collect();
        let (min_args, max_args) = self.arg_count_bounds(&instantiated_params);
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
        if let Some(result) =
            self.check_argument_types_with(&instantiated_params, arg_types, true, func.is_method)
        {
            return result;
        }

        let return_type = instantiate_type(self.interner, func.return_type, &final_subst);
        CallResult::Success(return_type)
    }

    /// Computes contextual types for function parameters after Round 1 inference.
    ///
    /// This is used by the Checker to implement two-pass argument checking:
    /// 1. Checker checks non-contextual arguments (arrays, primitives)
    /// 2. Checker calls this method to run Round 1 inference on those arguments
    /// 3. This method returns the current type substitution (with fixed variables)
    /// 4. Checker uses the substitution to construct contextual types for lambdas
    /// 5. Checker checks lambdas with those contextual types (Round 2)
    ///
    /// # Arguments
    /// * `func` - The function shape being called
    /// * `arg_types` - The types of all arguments (both contextual and non-contextual)
    ///
    /// # Returns
    /// A `TypeSubstitution` mapping type parameter placeholder names to their
    /// inferred types after Round 1 inference. The Checker can use this to
    /// instantiate parameter types for contextual arguments.
    pub fn compute_contextual_types(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> TypeSubstitution {
        use crate::solver::types::InferencePriority;

        // Save state to prevent pollution if evaluator is reused
        let previous_defaulted = std::mem::take(&mut self.defaulted_placeholders);

        let mut infer_ctx = InferenceContext::new(self.interner.as_type_database());
        let mut substitution = TypeSubstitution::new();
        let mut var_map: FxHashMap<TypeId, crate::solver::infer::InferenceVar> =
            FxHashMap::default();
        let mut type_param_vars = Vec::with_capacity(func.type_params.len());

        self.constraint_pairs.borrow_mut().clear();
        *self.constraint_recursion_depth.borrow_mut() = 0;

        // 1. Create inference variables and placeholders for each type parameter
        for tp in &func.type_params {
            let var = infer_ctx.fresh_var();
            type_param_vars.push(var);

            let placeholder_name = format!("__infer_{}", var.0);
            let placeholder_atom = self.interner.intern_string(&placeholder_name);
            infer_ctx.register_type_param(placeholder_atom, var, tp.is_const);
            let placeholder_key = TypeKey::TypeParameter(TypeParamInfo {
                is_const: tp.is_const,
                name: placeholder_atom,
                constraint: tp.constraint,
                default: None,
            });
            let placeholder_id = self.interner.intern(placeholder_key);

            substitution.insert(tp.name, placeholder_id);
            var_map.insert(placeholder_id, var);

            // Track defaulted placeholders to prevent union inference in constrain_types
            if tp.default.is_some() {
                self.defaulted_placeholders.insert(placeholder_id);
            }

            // Add type parameter constraint as upper bound (if concrete)
            if let Some(constraint) = tp.constraint {
                let inst_constraint = instantiate_type(self.interner, constraint, &substitution);
                let mut visited = FxHashSet::default();
                if !self.type_contains_placeholder(inst_constraint, &var_map, &mut visited) {
                    infer_ctx.add_upper_bound(var, inst_constraint);
                }
            }
        }

        // 2. Instantiate parameters with placeholders
        let instantiated_params: Vec<ParamInfo> = func
            .params
            .iter()
            .map(|p| ParamInfo {
                name: p.name,
                type_id: instantiate_type(self.interner, p.type_id, &substitution),
                optional: p.optional,
                rest: p.rest,
            })
            .collect();

        // 2.5. Seed contextual constraints from return type
        if let Some(ctx_type) = self.contextual_type {
            let return_type_with_placeholders =
                instantiate_type(self.interner, func.return_type, &substitution);
            self.constrain_types(
                &mut infer_ctx,
                &var_map,
                return_type_with_placeholders,
                ctx_type,
                InferencePriority::ReturnType,
            );
        }

        // 3. Round 1: Process non-contextual arguments only
        let rest_tuple_inference =
            self.rest_tuple_inference_target(&instantiated_params, arg_types, &var_map);
        let rest_tuple_start = rest_tuple_inference.as_ref().map(|(start, _, _)| *start);

        for (i, &arg_type) in arg_types.iter().enumerate() {
            if rest_tuple_start.is_some_and(|start| i >= start) {
                continue;
            }
            let Some(target_type) =
                self.param_type_for_arg_index(&instantiated_params, i, arg_types.len())
            else {
                break;
            };

            // Skip contextually sensitive arguments (Checker will handle them in Round 2)
            if self.is_contextually_sensitive(arg_type) {
                continue;
            }

            // Add constraint for non-contextual arguments
            self.constrain_types(
                &mut infer_ctx,
                &var_map,
                arg_type,
                target_type,
                InferencePriority::NakedTypeVariable,
            );
        }

        // Process rest tuple in Round 1
        if let Some((_start, target_type, tuple_type)) = rest_tuple_inference {
            self.constrain_types(
                &mut infer_ctx,
                &var_map,
                tuple_type,
                target_type,
                InferencePriority::NakedTypeVariable,
            );
        }

        // 4. Fix variables with enough information from Round 1
        let _ = infer_ctx.fix_current_variables();

        // Restore state to prevent pollution if evaluator is reused
        self.defaulted_placeholders = previous_defaulted;

        // 5. Return current substitution for Checker to use in Round 2
        infer_ctx.get_current_substitution()
    }

    fn check_argument_types(
        &mut self,
        params: &[ParamInfo],
        arg_types: &[TypeId],
        allow_bivariant_callbacks: bool,
    ) -> Option<CallResult> {
        self.check_argument_types_with(params, arg_types, false, allow_bivariant_callbacks)
    }

    fn check_argument_types_with(
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

            // Expand TypeParameters to their constraints for assignability checking when the
            // *parameter* expects a concrete type (e.g. `object`) but the argument is an outer
            // type parameter with a compatible constraint.
            //
            // IMPORTANT: Do **not** expand when the parameter type is itself a type parameter;
            // otherwise a call like `freeze(obj)` where `obj: T extends object` can incorrectly
            // compare `object` (expanded) against `T` and fail, even though inference would (and
            // tsc does) infer the inner `T` to the outer `T`.
            let expanded_arg_type = match self.interner.lookup(param_type) {
                Some(TypeKey::TypeParameter(_)) | Some(TypeKey::Infer(_)) => *arg_type,
                _ => self.expand_type_param(*arg_type),
            };

            let assignable = if allow_bivariant_callbacks || self.force_bivariant_callbacks {
                self.checker
                    .is_assignable_to_bivariant_callback(expanded_arg_type, param_type)
            } else if strict {
                self.checker
                    .is_assignable_to_strict(expanded_arg_type, param_type)
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

    fn arg_count_bounds(&self, params: &[ParamInfo]) -> (usize, Option<usize>) {
        let required = params.iter().filter(|p| !p.optional && !p.rest).count();
        let rest_param = params.last().filter(|param| param.rest);
        let Some(rest_param) = rest_param else {
            return (required, Some(params.len()));
        };

        let rest_param_type = self.unwrap_readonly(rest_param.type_id);
        match self.interner.lookup(rest_param_type) {
            Some(TypeKey::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                let (rest_min, rest_max) = self.tuple_length_bounds(&elements);
                let min = required + rest_min;
                let max = rest_max.map(|max| required + max);
                (min, max)
            }
            _ => (required, None),
        }
    }

    fn param_type_for_arg_index(
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
        match self.interner.lookup(rest_param_type) {
            Some(TypeKey::Array(elem)) => Some(elem),
            Some(TypeKey::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                self.tuple_rest_element_type(&elements, offset, rest_arg_count)
            }
            _ => Some(rest_param_type),
        }
    }

    fn tuple_length_bounds(&self, elements: &[TupleElement]) -> (usize, Option<usize>) {
        let mut min = 0usize;
        let mut max = 0usize;
        let mut variadic = false;

        for elem in elements.iter() {
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

    fn rest_element_type(&self, type_id: TypeId) -> TypeId {
        match self.interner.lookup(type_id) {
            Some(TypeKey::Array(elem)) => elem,
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
                Some(TypeKey::ReadonlyType(inner)) => {
                    type_id = inner;
                }
                _ => return type_id,
            }
        }
    }

    fn expand_tuple_rest(&self, type_id: TypeId) -> TupleRestExpansion {
        match self.interner.lookup(type_id) {
            Some(TypeKey::Array(elem)) => TupleRestExpansion {
                fixed: Vec::new(),
                variadic: Some(elem),
                tail: Vec::new(),
            },
            Some(TypeKey::Tuple(elements)) => {
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

    fn rest_tuple_inference_target(
        &mut self,
        params: &[ParamInfo],
        arg_types: &[TypeId],
        var_map: &FxHashMap<TypeId, crate::solver::infer::InferenceVar>,
    ) -> Option<(usize, TypeId, TypeId)> {
        let rest_param = params.last().filter(|param| param.rest)?;
        let rest_start = params.len().saturating_sub(1);

        let rest_param_type = self.unwrap_readonly(rest_param.type_id);
        let target = match self.interner.lookup(rest_param_type) {
            Some(TypeKey::TypeParameter(_)) if var_map.contains_key(&rest_param_type) => {
                Some((rest_start, rest_param_type, 0))
            }
            Some(TypeKey::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                let mut prefix_len = 0usize;
                let mut target = None;
                #[allow(clippy::explicit_counter_loop)]
                for (i, elem) in elements.iter().enumerate() {
                    if elem.rest {
                        if var_map.contains_key(&elem.type_id) {
                            // Count trailing elements after the variadic part, but allow optional
                            // tail elements to be omitted when they don't match.
                            let tail = &elements[i + 1..];
                            let min_index = rest_start + prefix_len;
                            let mut trailing_count = 0usize;
                            let mut arg_index = arg_types.len();
                            for tail_elem in tail.iter().rev() {
                                if arg_index <= min_index {
                                    break;
                                }
                                let arg_type = arg_types[arg_index - 1];
                                let assignable =
                                    self.checker.is_assignable_to(arg_type, tail_elem.type_id);
                                if tail_elem.optional && !assignable {
                                    break;
                                }
                                trailing_count += 1;
                                arg_index -= 1;
                            }
                            target = Some((rest_start + prefix_len, elem.type_id, trailing_count));
                        }
                        break;
                    }
                    prefix_len += 1;
                }
                target
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

    fn type_contains_placeholder(
        &self,
        ty: TypeId,
        var_map: &FxHashMap<TypeId, crate::solver::infer::InferenceVar>,
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
            TypeKey::Array(elem) => self.type_contains_placeholder(elem, var_map, visited),
            TypeKey::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                elements
                    .iter()
                    .any(|elem| self.type_contains_placeholder(elem.type_id, var_map, visited))
            }
            TypeKey::Union(members) | TypeKey::Intersection(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|&member| self.type_contains_placeholder(member, var_map, visited))
            }
            TypeKey::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_placeholder(prop.type_id, var_map, visited))
            }
            TypeKey::ObjectWithIndex(shape_id) => {
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
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.type_contains_placeholder(app.base, var_map, visited)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.type_contains_placeholder(arg, var_map, visited))
            }
            TypeKey::Function(shape_id) => {
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
            }
            TypeKey::Callable(shape_id) => {
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
                });
                if in_construct {
                    return true;
                }
                shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_placeholder(prop.type_id, var_map, visited))
            }
            TypeKey::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(cond_id);
                self.type_contains_placeholder(cond.check_type, var_map, visited)
                    || self.type_contains_placeholder(cond.extends_type, var_map, visited)
                    || self.type_contains_placeholder(cond.true_type, var_map, visited)
                    || self.type_contains_placeholder(cond.false_type, var_map, visited)
            }
            TypeKey::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(mapped_id);
                mapped.type_param.constraint.is_some_and(|constraint| {
                    self.type_contains_placeholder(constraint, var_map, visited)
                }) || mapped.type_param.default.is_some_and(|default| {
                    self.type_contains_placeholder(default, var_map, visited)
                }) || self.type_contains_placeholder(mapped.constraint, var_map, visited)
                    || self.type_contains_placeholder(mapped.template, var_map, visited)
            }
            TypeKey::IndexAccess(obj, idx) => {
                self.type_contains_placeholder(obj, var_map, visited)
                    || self.type_contains_placeholder(idx, var_map, visited)
            }
            TypeKey::KeyOf(operand) | TypeKey::ReadonlyType(operand) => {
                self.type_contains_placeholder(operand, var_map, visited)
            }
            TypeKey::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => {
                        self.type_contains_placeholder(*inner, var_map, visited)
                    }
                })
            }
            TypeKey::StringIntrinsic { type_arg, .. } => {
                self.type_contains_placeholder(type_arg, var_map, visited)
            }
            TypeKey::Enum(_def_id, member_type) => {
                self.type_contains_placeholder(member_type, var_map, visited)
            }
            TypeKey::TypeParameter(_)
            | TypeKey::Infer(_)
            | TypeKey::Intrinsic(_)
            | TypeKey::Literal(_)
            | TypeKey::Lazy(_)
            | TypeKey::TypeQuery(_)
            | TypeKey::UniqueSymbol(_)
            | TypeKey::ThisType
            | TypeKey::ModuleNamespace(_)
            | TypeKey::Error => false,
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
    fn is_contextually_sensitive(&self, type_id: TypeId) -> bool {
        let key = match self.interner.lookup(type_id) {
            Some(key) => key,
            None => return false,
        };

        match key {
            // Function types are contextually sensitive (lambdas need contextual parameter types)
            TypeKey::Function(_) => true,

            // Callable types are contextually sensitive (objects with call signatures)
            TypeKey::Callable(_) => true,

            // Union/Intersection: contextually sensitive if any member is
            TypeKey::Union(members) | TypeKey::Intersection(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|&member| self.is_contextually_sensitive(member))
            }

            // Object types: check if any property is callable (has methods)
            TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|prop| self.is_contextually_sensitive(prop.type_id))
            }

            // Array types: check element type
            TypeKey::Array(elem) => self.is_contextually_sensitive(elem),

            // Tuple types: check all elements
            TypeKey::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                elements
                    .iter()
                    .any(|elem| self.is_contextually_sensitive(elem.type_id))
            }

            // Type applications: check base and arguments
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.is_contextually_sensitive(app.base)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.is_contextually_sensitive(arg))
            }

            // Readonly types: look through to inner type
            TypeKey::ReadonlyType(inner) => self.is_contextually_sensitive(inner),

            // Type parameters with constraints: check constraint
            TypeKey::TypeParameter(info) | TypeKey::Infer(info) => info
                .constraint
                .is_some_and(|constraint| self.is_contextually_sensitive(constraint)),

            // Index access: check both object and key types
            TypeKey::IndexAccess(obj, key) => {
                self.is_contextually_sensitive(obj) || self.is_contextually_sensitive(key)
            }

            // Conditional types: check all branches
            TypeKey::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(cond_id);
                self.is_contextually_sensitive(cond.check_type)
                    || self.is_contextually_sensitive(cond.extends_type)
                    || self.is_contextually_sensitive(cond.true_type)
                    || self.is_contextually_sensitive(cond.false_type)
            }

            // Mapped types: check constraint and template
            TypeKey::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(mapped_id);
                self.is_contextually_sensitive(mapped.constraint)
                    || self.is_contextually_sensitive(mapped.template)
            }

            // KeyOf, StringIntrinsic: check operand
            TypeKey::KeyOf(operand)
            | TypeKey::StringIntrinsic {
                type_arg: operand, ..
            } => self.is_contextually_sensitive(operand),

            // Enum types: check member type
            TypeKey::Enum(_def_id, member_type) => self.is_contextually_sensitive(member_type),

            // Template literals: check type spans
            TypeKey::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => self.is_contextually_sensitive(*inner),
                })
            }

            // Non-contextually sensitive types
            TypeKey::Intrinsic(_)
            | TypeKey::Literal(_)
            | TypeKey::Lazy(_)
            | TypeKey::TypeQuery(_)
            | TypeKey::UniqueSymbol(_)
            | TypeKey::ThisType
            | TypeKey::ModuleNamespace(_)
            | TypeKey::Error => false,
        }
    }

    /// Structural walker to collect constraints: source <: target
    fn constrain_types(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::solver::infer::InferenceVar>,
        source: TypeId,
        target: TypeId,
        priority: crate::solver::types::InferencePriority,
    ) {
        if !self.constraint_pairs.borrow_mut().insert((source, target)) {
            return;
        }

        // Check and increment recursion depth to prevent infinite loops
        {
            let mut depth = self.constraint_recursion_depth.borrow_mut();
            if *depth >= MAX_CONSTRAINT_RECURSION_DEPTH {
                // Safety limit reached - return to prevent infinite loop
                return;
            }
            *depth += 1;
        }

        // Perform the actual constraint collection
        self.constrain_types_impl(ctx, var_map, source, target, priority);

        // Decrement depth on return
        *self.constraint_recursion_depth.borrow_mut() -= 1;
    }

    /// Inner implementation of constrain_types
    fn constrain_types_impl(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::solver::infer::InferenceVar>,
        source: TypeId,
        target: TypeId,
        priority: crate::solver::types::InferencePriority,
    ) {
        if source == target {
            return;
        }

        // If target is an inference placeholder, add lower bound: source <: var
        if let Some(&var) = var_map.get(&target) {
            ctx.add_candidate(var, source, priority);
            return;
        }

        // If source is an inference placeholder, add upper bound: var <: target
        if let Some(&var) = var_map.get(&source) {
            ctx.add_upper_bound(var, target);
            return;
        }

        // Recurse structurally
        let source_key = self.interner.lookup(source);
        let target_key = self.interner.lookup(target);

        let is_nullish = |ty: TypeId| matches!(ty, TypeId::NULL | TypeId::UNDEFINED | TypeId::VOID);

        match (source_key, target_key) {
            (Some(TypeKey::ReadonlyType(s_inner)), Some(TypeKey::ReadonlyType(t_inner))) => {
                self.constrain_types(ctx, var_map, s_inner, t_inner, priority);
            }
            (Some(TypeKey::ReadonlyType(s_inner)), _) => {
                self.constrain_types(ctx, var_map, s_inner, target, priority);
            }
            (_, Some(TypeKey::ReadonlyType(t_inner))) => {
                self.constrain_types(ctx, var_map, source, t_inner, priority);
            }
            (
                Some(TypeKey::IndexAccess(s_obj, s_idx)),
                Some(TypeKey::IndexAccess(t_obj, t_idx)),
            ) => {
                self.constrain_types(ctx, var_map, s_obj, t_obj, priority);
                self.constrain_types(ctx, var_map, s_idx, t_idx, priority);
            }
            (Some(TypeKey::KeyOf(s_inner)), Some(TypeKey::KeyOf(t_inner))) => {
                self.constrain_types(ctx, var_map, t_inner, s_inner, priority);
            }
            (Some(TypeKey::TemplateLiteral(s_spans)), Some(TypeKey::TemplateLiteral(t_spans))) => {
                let s_spans = self.interner.template_list(s_spans);
                let t_spans = self.interner.template_list(t_spans);
                if s_spans.len() != t_spans.len() {
                    return;
                }

                for (s_span, t_span) in s_spans.iter().zip(t_spans.iter()) {
                    match (s_span, t_span) {
                        (TemplateSpan::Text(s_text), TemplateSpan::Text(t_text))
                            if s_text == t_text => {}
                        (TemplateSpan::Type(_), TemplateSpan::Type(_)) => {}
                        _ => return,
                    }
                }

                for (s_span, t_span) in s_spans.iter().zip(t_spans.iter()) {
                    if let (TemplateSpan::Type(s_type), TemplateSpan::Type(t_type)) =
                        (s_span, t_span)
                    {
                        self.constrain_types(ctx, var_map, *s_type, *t_type, priority);
                    }
                }
            }
            (Some(TypeKey::IndexAccess(s_obj, s_idx)), _) => {
                let evaluated = self.interner.evaluate_index_access(s_obj, s_idx);
                if evaluated != source {
                    self.constrain_types(ctx, var_map, evaluated, target, priority);
                }
            }
            (_, Some(TypeKey::IndexAccess(t_obj, t_idx))) => {
                let evaluated = self.interner.evaluate_index_access(t_obj, t_idx);
                if evaluated != target {
                    self.constrain_types(ctx, var_map, source, evaluated, priority);
                }
            }
            (Some(TypeKey::Conditional(cond_id)), _) => {
                let cond = self.interner.conditional_type(cond_id);
                let evaluated = self.interner.evaluate_conditional(cond.as_ref());
                if evaluated != source {
                    self.constrain_types(ctx, var_map, evaluated, target, priority);
                }
            }
            (_, Some(TypeKey::Conditional(cond_id))) => {
                let cond = self.interner.conditional_type(cond_id);
                let evaluated = self.interner.evaluate_conditional(cond.as_ref());
                if evaluated != target {
                    self.constrain_types(ctx, var_map, source, evaluated, priority);
                }
            }
            (Some(TypeKey::Mapped(mapped_id)), _) => {
                let mapped = self.interner.mapped_type(mapped_id);
                let evaluated = self.interner.evaluate_mapped(mapped.as_ref());
                if evaluated != source {
                    self.constrain_types(ctx, var_map, evaluated, target, priority);
                }
            }
            (_, Some(TypeKey::Mapped(mapped_id))) => {
                let mapped = self.interner.mapped_type(mapped_id);
                let evaluated = self.interner.evaluate_mapped(mapped.as_ref());
                if evaluated != target {
                    self.constrain_types(ctx, var_map, source, evaluated, priority);
                }
            }
            (Some(TypeKey::Union(s_members)), _) => {
                let s_members = self.interner.type_list(s_members);
                for &member in s_members.iter() {
                    self.constrain_types(ctx, var_map, member, target, priority);
                }
            }
            (_, Some(TypeKey::Intersection(t_members))) => {
                let t_members = self.interner.type_list(t_members);
                for &member in t_members.iter() {
                    self.constrain_types(ctx, var_map, source, member, priority);
                }
            }
            (_, Some(TypeKey::Union(t_members))) => {
                let t_members = self.interner.type_list(t_members);
                let mut non_nullable = None;
                let mut count = 0;
                for &member in t_members.iter() {
                    if !is_nullish(member) {
                        count += 1;
                        if count == 1 {
                            non_nullable = Some(member);
                        } else {
                            break;
                        }
                    }
                }
                if count == 1
                    && let Some(member) = non_nullable
                {
                    self.constrain_types(ctx, var_map, source, member, priority);
                    return;
                }

                let mut placeholder_member = None;
                let mut placeholder_count = 0;
                for &member in t_members.iter() {
                    let mut visited = FxHashSet::default();
                    if self.type_contains_placeholder(member, var_map, &mut visited) {
                        placeholder_count += 1;
                        if placeholder_count == 1 {
                            placeholder_member = Some(member);
                        } else {
                            break;
                        }
                    }
                }
                if placeholder_count == 1
                    && let Some(member) = placeholder_member
                    && !self.defaulted_placeholders.contains(&member)
                {
                    self.constrain_types(ctx, var_map, source, member, priority);
                }
            }
            (Some(TypeKey::Array(s_elem)), Some(TypeKey::Array(t_elem))) => {
                self.constrain_types(ctx, var_map, s_elem, t_elem, priority);
            }
            (Some(TypeKey::Tuple(s_elems)), Some(TypeKey::Array(t_elem))) => {
                let s_elems = self.interner.tuple_list(s_elems);
                for s_elem in s_elems.iter() {
                    if s_elem.rest {
                        let rest_elem_type = self.rest_element_type(s_elem.type_id);
                        self.constrain_types(ctx, var_map, rest_elem_type, t_elem, priority);
                    } else {
                        self.constrain_types(ctx, var_map, s_elem.type_id, t_elem, priority);
                    }
                }
            }
            (Some(TypeKey::Tuple(s_elems)), Some(TypeKey::Tuple(t_elems))) => {
                let s_elems = self.interner.tuple_list(s_elems);
                let t_elems = self.interner.tuple_list(t_elems);
                self.constrain_tuple_types(ctx, var_map, &s_elems, &t_elems, priority);
            }
            // Array/Tuple to Object/ObjectWithIndex: constrain elements against index signatures
            (Some(TypeKey::Array(s_elem)), Some(TypeKey::Object(t_shape_id))) => {
                let t_shape = self.interner.object_shape(t_shape_id);
                // Constrain array element type against target's string/number index signatures
                if let Some(string_idx) = &t_shape.string_index {
                    self.constrain_types(ctx, var_map, s_elem, string_idx.value_type, priority);
                }
                if let Some(number_idx) = &t_shape.number_index {
                    self.constrain_types(ctx, var_map, s_elem, number_idx.value_type, priority);
                }
            }
            (Some(TypeKey::Array(s_elem)), Some(TypeKey::ObjectWithIndex(t_shape_id))) => {
                let t_shape = self.interner.object_shape(t_shape_id);
                // Constrain array element type against target's string/number index signatures
                if let Some(string_idx) = &t_shape.string_index {
                    self.constrain_types(ctx, var_map, s_elem, string_idx.value_type, priority);
                }
                if let Some(number_idx) = &t_shape.number_index {
                    self.constrain_types(ctx, var_map, s_elem, number_idx.value_type, priority);
                }
            }
            (Some(TypeKey::Tuple(s_elems)), Some(TypeKey::Object(t_shape_id))) => {
                let s_elems = self.interner.tuple_list(s_elems);
                let t_shape = self.interner.object_shape(t_shape_id);
                // Constrain each tuple element against target's string/number index signatures
                for s_elem in s_elems.iter() {
                    let elem_type = if s_elem.rest {
                        self.rest_element_type(s_elem.type_id)
                    } else {
                        s_elem.type_id
                    };
                    if let Some(string_idx) = &t_shape.string_index {
                        self.constrain_types(
                            ctx,
                            var_map,
                            elem_type,
                            string_idx.value_type,
                            priority,
                        );
                    }
                    if let Some(number_idx) = &t_shape.number_index {
                        self.constrain_types(
                            ctx,
                            var_map,
                            elem_type,
                            number_idx.value_type,
                            priority,
                        );
                    }
                }
            }
            (Some(TypeKey::Tuple(s_elems)), Some(TypeKey::ObjectWithIndex(t_shape_id))) => {
                let s_elems = self.interner.tuple_list(s_elems);
                let t_shape = self.interner.object_shape(t_shape_id);
                // Constrain each tuple element against target's string/number index signatures
                for s_elem in s_elems.iter() {
                    let elem_type = if s_elem.rest {
                        self.rest_element_type(s_elem.type_id)
                    } else {
                        s_elem.type_id
                    };
                    if let Some(string_idx) = &t_shape.string_index {
                        self.constrain_types(
                            ctx,
                            var_map,
                            elem_type,
                            string_idx.value_type,
                            priority,
                        );
                    }
                    if let Some(number_idx) = &t_shape.number_index {
                        self.constrain_types(
                            ctx,
                            var_map,
                            elem_type,
                            number_idx.value_type,
                            priority,
                        );
                    }
                }
            }
            (Some(TypeKey::Function(s_fn_id)), Some(TypeKey::Function(t_fn_id))) => {
                let s_fn = self.interner.function_shape(s_fn_id);
                let t_fn = self.interner.function_shape(t_fn_id);
                // Contravariant parameters: target_param <: source_param
                for (s_p, t_p) in s_fn.params.iter().zip(t_fn.params.iter()) {
                    self.constrain_types(ctx, var_map, t_p.type_id, s_p.type_id, priority);
                }
                if let (Some(s_this), Some(t_this)) = (s_fn.this_type, t_fn.this_type) {
                    self.constrain_types(ctx, var_map, t_this, s_this, priority);
                }
                // Covariant return: source_return <: target_return
                self.constrain_types(ctx, var_map, s_fn.return_type, t_fn.return_type, priority);
            }
            (Some(TypeKey::Function(s_fn_id)), Some(TypeKey::Callable(t_callable_id))) => {
                let s_fn = self.interner.function_shape(s_fn_id);
                let t_callable = self.interner.callable_shape(t_callable_id);
                for sig in &t_callable.call_signatures {
                    self.constrain_function_to_call_signature(ctx, var_map, &s_fn, sig, priority);
                }
                if s_fn.is_constructor && t_callable.construct_signatures.len() == 1 {
                    let sig = &t_callable.construct_signatures[0];
                    if sig.type_params.is_empty() {
                        self.constrain_function_to_call_signature(
                            ctx, var_map, &s_fn, sig, priority,
                        );
                    }
                }
            }
            (Some(TypeKey::Callable(s_callable_id)), Some(TypeKey::Callable(t_callable_id))) => {
                let s_callable = self.interner.callable_shape(s_callable_id);
                let t_callable = self.interner.callable_shape(t_callable_id);
                self.constrain_matching_signatures(
                    ctx,
                    var_map,
                    &s_callable.call_signatures,
                    &t_callable.call_signatures,
                    false,
                    priority,
                );
                self.constrain_matching_signatures(
                    ctx,
                    var_map,
                    &s_callable.construct_signatures,
                    &t_callable.construct_signatures,
                    true,
                    priority,
                );
            }
            (Some(TypeKey::Callable(s_callable_id)), Some(TypeKey::Function(t_fn_id))) => {
                let s_callable = self.interner.callable_shape(s_callable_id);
                let t_fn = self.interner.function_shape(t_fn_id);
                if s_callable.call_signatures.len() == 1 {
                    let sig = &s_callable.call_signatures[0];
                    if sig.type_params.is_empty() {
                        self.constrain_call_signature_to_function(
                            ctx, var_map, sig, &t_fn, priority,
                        );
                    }
                } else if let Some(index) = self.select_signature_for_target(
                    &s_callable.call_signatures,
                    target,
                    var_map,
                    false,
                ) {
                    let sig = &s_callable.call_signatures[index];
                    self.constrain_call_signature_to_function(ctx, var_map, sig, &t_fn, priority);
                }
            }
            (Some(TypeKey::Object(s_shape_id)), Some(TypeKey::Object(t_shape_id))) => {
                let s_shape = self.interner.object_shape(s_shape_id);
                let t_shape = self.interner.object_shape(t_shape_id);
                self.constrain_properties(
                    ctx,
                    var_map,
                    &s_shape.properties,
                    &t_shape.properties,
                    priority,
                );
            }
            (
                Some(TypeKey::ObjectWithIndex(s_shape_id)),
                Some(TypeKey::ObjectWithIndex(t_shape_id)),
            ) => {
                let s_shape = self.interner.object_shape(s_shape_id);
                let t_shape = self.interner.object_shape(t_shape_id);
                self.constrain_properties(
                    ctx,
                    var_map,
                    &s_shape.properties,
                    &t_shape.properties,
                    priority,
                );
                if let (Some(s_idx), Some(t_idx)) = (&s_shape.string_index, &t_shape.string_index) {
                    self.constrain_types(
                        ctx,
                        var_map,
                        s_idx.value_type,
                        t_idx.value_type,
                        priority,
                    );
                }
                if let (Some(s_idx), Some(t_idx)) = (&s_shape.number_index, &t_shape.number_index) {
                    self.constrain_types(
                        ctx,
                        var_map,
                        s_idx.value_type,
                        t_idx.value_type,
                        priority,
                    );
                }
                self.constrain_properties_against_index_signatures(
                    ctx,
                    var_map,
                    &s_shape.properties,
                    &t_shape,
                    priority,
                );
                self.constrain_index_signatures_to_properties(
                    ctx,
                    var_map,
                    &s_shape,
                    &t_shape.properties,
                    priority,
                );
            }
            (Some(TypeKey::Object(s_shape_id)), Some(TypeKey::ObjectWithIndex(t_shape_id))) => {
                let s_shape = self.interner.object_shape(s_shape_id);
                let t_shape = self.interner.object_shape(t_shape_id);
                self.constrain_properties(
                    ctx,
                    var_map,
                    &s_shape.properties,
                    &t_shape.properties,
                    priority,
                );
                self.constrain_properties_against_index_signatures(
                    ctx,
                    var_map,
                    &s_shape.properties,
                    &t_shape,
                    priority,
                );
            }
            (Some(TypeKey::ObjectWithIndex(s_shape_id)), Some(TypeKey::Object(t_shape_id))) => {
                let s_shape = self.interner.object_shape(s_shape_id);
                let t_shape = self.interner.object_shape(t_shape_id);
                self.constrain_properties(
                    ctx,
                    var_map,
                    &s_shape.properties,
                    &t_shape.properties,
                    priority,
                );
                self.constrain_index_signatures_to_properties(
                    ctx,
                    var_map,
                    &s_shape,
                    &t_shape.properties,
                    priority,
                );
            }
            // Object/ObjectWithIndex to Array/Tuple: constrain index signatures to sequence element type
            (Some(TypeKey::Object(s_shape_id)), Some(TypeKey::Array(t_elem))) => {
                let s_shape = self.interner.object_shape(s_shape_id);
                // Constrain source's string/number index signatures against array element type
                if let Some(string_idx) = &s_shape.string_index {
                    self.constrain_types(ctx, var_map, string_idx.value_type, t_elem, priority);
                }
                if let Some(number_idx) = &s_shape.number_index {
                    self.constrain_types(ctx, var_map, number_idx.value_type, t_elem, priority);
                }
            }
            (Some(TypeKey::ObjectWithIndex(s_shape_id)), Some(TypeKey::Array(t_elem))) => {
                let s_shape = self.interner.object_shape(s_shape_id);
                // Constrain source's string/number index signatures against array element type
                if let Some(string_idx) = &s_shape.string_index {
                    self.constrain_types(ctx, var_map, string_idx.value_type, t_elem, priority);
                }
                if let Some(number_idx) = &s_shape.number_index {
                    self.constrain_types(ctx, var_map, number_idx.value_type, t_elem, priority);
                }
            }
            (Some(TypeKey::Object(s_shape_id)), Some(TypeKey::Tuple(t_elems))) => {
                let s_shape = self.interner.object_shape(s_shape_id);
                let t_elems = self.interner.tuple_list(t_elems);
                // Constrain source's string/number index signatures against each tuple element
                for t_elem in t_elems.iter() {
                    let elem_type = if t_elem.rest {
                        self.rest_element_type(t_elem.type_id)
                    } else {
                        t_elem.type_id
                    };
                    if let Some(string_idx) = &s_shape.string_index {
                        self.constrain_types(
                            ctx,
                            var_map,
                            string_idx.value_type,
                            elem_type,
                            priority,
                        );
                    }
                    if let Some(number_idx) = &s_shape.number_index {
                        self.constrain_types(
                            ctx,
                            var_map,
                            number_idx.value_type,
                            elem_type,
                            priority,
                        );
                    }
                }
            }
            (Some(TypeKey::ObjectWithIndex(s_shape_id)), Some(TypeKey::Tuple(t_elems))) => {
                let s_shape = self.interner.object_shape(s_shape_id);
                let t_elems = self.interner.tuple_list(t_elems);
                // Constrain source's string/number index signatures against each tuple element
                for t_elem in t_elems.iter() {
                    let elem_type = if t_elem.rest {
                        self.rest_element_type(t_elem.type_id)
                    } else {
                        t_elem.type_id
                    };
                    if let Some(string_idx) = &s_shape.string_index {
                        self.constrain_types(
                            ctx,
                            var_map,
                            string_idx.value_type,
                            elem_type,
                            priority,
                        );
                    }
                    if let Some(number_idx) = &s_shape.number_index {
                        self.constrain_types(
                            ctx,
                            var_map,
                            number_idx.value_type,
                            elem_type,
                            priority,
                        );
                    }
                }
            }
            (Some(TypeKey::Application(s_app_id)), Some(TypeKey::Application(t_app_id))) => {
                let s_app = self.interner.type_application(s_app_id);
                let t_app = self.interner.type_application(t_app_id);
                if s_app.base == t_app.base && s_app.args.len() == t_app.args.len() {
                    for (s_arg, t_arg) in s_app.args.iter().zip(t_app.args.iter()) {
                        self.constrain_types(ctx, var_map, *s_arg, *t_arg, priority);
                    }
                }
            }
            (Some(TypeKey::Enum(_, s_mem)), Some(TypeKey::Enum(_, t_mem))) => {
                self.constrain_types(ctx, var_map, s_mem, t_mem, priority);
            }
            _ => {}
        }
    }

    fn constrain_properties(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::solver::infer::InferenceVar>,
        source_props: &[PropertyInfo],
        target_props: &[PropertyInfo],
        priority: crate::solver::types::InferencePriority,
    ) {
        let mut source_idx = 0;
        let mut target_idx = 0;

        while source_idx < source_props.len() && target_idx < target_props.len() {
            let source = &source_props[source_idx];
            let target = &target_props[target_idx];

            match source.name.cmp(&target.name) {
                std::cmp::Ordering::Equal => {
                    self.constrain_types(ctx, var_map, source.type_id, target.type_id, priority);
                    // Check write type compatibility for mutable targets
                    // A readonly source cannot satisfy a mutable target
                    if !target.readonly {
                        // If source is readonly but target is mutable, this is a mismatch
                        // We constrain with ERROR to signal the failure
                        if source.readonly {
                            self.constrain_types(
                                ctx,
                                var_map,
                                TypeId::ERROR,
                                target.write_type,
                                priority,
                            );
                        }
                        self.constrain_types(
                            ctx,
                            var_map,
                            target.write_type,
                            source.write_type,
                            priority,
                        );
                    }
                    source_idx += 1;
                    target_idx += 1;
                }
                std::cmp::Ordering::Less => {
                    source_idx += 1;
                }
                std::cmp::Ordering::Greater => {
                    // Target property is missing from source
                    // For optional properties, we still need to collect constraints
                    // to properly infer type parameters (e.g., {} satisfies {a?: T})
                    if target.optional {
                        // Use undefined as the lower bound for missing optional properties
                        self.constrain_types(
                            ctx,
                            var_map,
                            TypeId::UNDEFINED,
                            target.type_id,
                            priority,
                        );
                    }
                    target_idx += 1;
                }
            }
        }

        // Handle remaining target properties that are missing from source
        while target_idx < target_props.len() {
            let target = &target_props[target_idx];
            if target.optional {
                // Use undefined as the lower bound for missing optional properties
                self.constrain_types(ctx, var_map, TypeId::UNDEFINED, target.type_id, priority);
            }
            target_idx += 1;
        }
    }

    fn constrain_function_to_call_signature(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::solver::infer::InferenceVar>,
        source: &FunctionShape,
        target: &CallSignature,
        priority: crate::solver::types::InferencePriority,
    ) {
        for (s_p, t_p) in source.params.iter().zip(target.params.iter()) {
            self.constrain_types(ctx, var_map, t_p.type_id, s_p.type_id, priority);
        }
        if let (Some(s_this), Some(t_this)) = (source.this_type, target.this_type) {
            self.constrain_types(ctx, var_map, t_this, s_this, priority);
        }
        self.constrain_types(
            ctx,
            var_map,
            source.return_type,
            target.return_type,
            priority,
        );
    }

    fn constrain_call_signature_to_function(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::solver::infer::InferenceVar>,
        source: &CallSignature,
        target: &FunctionShape,
        priority: crate::solver::types::InferencePriority,
    ) {
        for (s_p, t_p) in source.params.iter().zip(target.params.iter()) {
            self.constrain_types(ctx, var_map, t_p.type_id, s_p.type_id, priority);
        }
        if let (Some(s_this), Some(t_this)) = (source.this_type, target.this_type) {
            self.constrain_types(ctx, var_map, t_this, s_this, priority);
        }
        self.constrain_types(
            ctx,
            var_map,
            source.return_type,
            target.return_type,
            priority,
        );
    }

    fn constrain_call_signature_to_call_signature(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::solver::infer::InferenceVar>,
        source: &CallSignature,
        target: &CallSignature,
        priority: crate::solver::types::InferencePriority,
    ) {
        for (s_p, t_p) in source.params.iter().zip(target.params.iter()) {
            self.constrain_types(ctx, var_map, t_p.type_id, s_p.type_id, priority);
        }
        if let (Some(s_this), Some(t_this)) = (source.this_type, target.this_type) {
            self.constrain_types(ctx, var_map, t_this, s_this, priority);
        }
        self.constrain_types(
            ctx,
            var_map,
            source.return_type,
            target.return_type,
            priority,
        );
    }

    fn function_type_from_signature(&self, sig: &CallSignature, is_constructor: bool) -> TypeId {
        self.interner.function(FunctionShape {
            type_params: Vec::new(),
            params: sig.params.clone(),
            this_type: sig.this_type,
            return_type: sig.return_type,
            type_predicate: sig.type_predicate.clone(),
            is_constructor,
            is_method: false,
        })
    }

    fn erase_placeholders_for_inference(
        &self,
        ty: TypeId,
        var_map: &FxHashMap<TypeId, crate::solver::infer::InferenceVar>,
    ) -> TypeId {
        if var_map.is_empty() {
            return ty;
        }
        let mut visited = FxHashSet::default();
        if !self.type_contains_placeholder(ty, var_map, &mut visited) {
            return ty;
        }

        let mut substitution = TypeSubstitution::new();
        for (&placeholder, _) in var_map.iter() {
            if let Some(TypeKey::TypeParameter(info)) = self.interner.lookup(placeholder) {
                // Use UNKNOWN instead of ANY for unresolved placeholders
                // to expose hidden type errors instead of silently accepting all values
                substitution.insert(info.name, TypeId::UNKNOWN);
            }
        }

        instantiate_type(self.interner, ty, &substitution)
    }

    fn select_signature_for_target(
        &mut self,
        signatures: &[CallSignature],
        target_fn: TypeId,
        var_map: &FxHashMap<TypeId, crate::solver::infer::InferenceVar>,
        is_constructor: bool,
    ) -> Option<usize> {
        let target_erased = self.erase_placeholders_for_inference(target_fn, var_map);
        for (index, sig) in signatures.iter().enumerate() {
            if !sig.type_params.is_empty() {
                continue;
            }
            let source_fn = self.function_type_from_signature(sig, is_constructor);
            if self.checker.is_assignable_to(source_fn, target_erased) {
                return Some(index);
            }
        }
        None
    }

    fn constrain_matching_signatures(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::solver::infer::InferenceVar>,
        source_signatures: &[CallSignature],
        target_signatures: &[CallSignature],
        is_constructor: bool,
        priority: crate::solver::types::InferencePriority,
    ) {
        if source_signatures.is_empty() || target_signatures.is_empty() {
            return;
        }

        if source_signatures.len() == 1 && target_signatures.len() == 1 {
            let source_sig = &source_signatures[0];
            let target_sig = &target_signatures[0];
            if source_sig.type_params.is_empty() && target_sig.type_params.is_empty() {
                self.constrain_call_signature_to_call_signature(
                    ctx, var_map, source_sig, target_sig, priority,
                );
            }
            return;
        }

        if target_signatures.len() == 1 {
            let target_sig = &target_signatures[0];
            if target_sig.type_params.is_empty() {
                let source_sig = if source_signatures.len() == 1 {
                    let sig = &source_signatures[0];
                    if sig.type_params.is_empty() {
                        Some(sig)
                    } else {
                        None
                    }
                } else {
                    let target_fn = self.function_type_from_signature(target_sig, is_constructor);
                    self.select_signature_for_target(
                        source_signatures,
                        target_fn,
                        var_map,
                        is_constructor,
                    )
                    .and_then(|index| source_signatures.get(index))
                };
                if let Some(source_sig) = source_sig {
                    self.constrain_call_signature_to_call_signature(
                        ctx, var_map, source_sig, target_sig, priority,
                    );
                }
            }
            return;
        }

        if source_signatures.len() == 1 {
            let source_sig = &source_signatures[0];
            if source_sig.type_params.is_empty() {
                for target_sig in target_signatures {
                    if target_sig.type_params.is_empty() {
                        self.constrain_call_signature_to_call_signature(
                            ctx, var_map, source_sig, target_sig, priority,
                        );
                    }
                }
            }
            return;
        }

        for target_sig in target_signatures {
            if target_sig.type_params.is_empty() {
                let target_fn = self.function_type_from_signature(target_sig, is_constructor);
                if let Some(index) = self.select_signature_for_target(
                    source_signatures,
                    target_fn,
                    var_map,
                    is_constructor,
                ) {
                    let source_sig = &source_signatures[index];
                    self.constrain_call_signature_to_call_signature(
                        ctx, var_map, source_sig, target_sig, priority,
                    );
                }
            }
        }
    }

    fn constrain_properties_against_index_signatures(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::solver::infer::InferenceVar>,
        source_props: &[PropertyInfo],
        target: &ObjectShape,
        priority: crate::solver::types::InferencePriority,
    ) {
        let string_index = target.string_index.as_ref();
        let number_index = target.number_index.as_ref();

        if string_index.is_none() && number_index.is_none() {
            return;
        }

        for prop in source_props {
            let prop_type = self.optional_property_type(prop);

            if let Some(number_idx) = number_index
                && utils::is_numeric_property_name(self.interner, prop.name)
            {
                self.constrain_types(ctx, var_map, prop_type, number_idx.value_type, priority);
            }

            if let Some(string_idx) = string_index {
                self.constrain_types(ctx, var_map, prop_type, string_idx.value_type, priority);
            }
        }
    }

    fn constrain_index_signatures_to_properties(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::solver::infer::InferenceVar>,
        source: &ObjectShape,
        target_props: &[PropertyInfo],
        priority: crate::solver::types::InferencePriority,
    ) {
        let string_index = source.string_index.as_ref();
        let number_index = source.number_index.as_ref();

        if string_index.is_none() && number_index.is_none() {
            return;
        }

        for prop in target_props {
            let prop_type = self.optional_property_type(prop);

            if let Some(number_idx) = number_index
                && utils::is_numeric_property_name(self.interner, prop.name)
            {
                self.constrain_types(ctx, var_map, number_idx.value_type, prop_type, priority);
            }

            if let Some(string_idx) = string_index {
                self.constrain_types(ctx, var_map, string_idx.value_type, prop_type, priority);
            }
        }
    }

    fn optional_property_type(&self, prop: &PropertyInfo) -> TypeId {
        if prop.optional {
            self.interner.union2(prop.type_id, TypeId::UNDEFINED)
        } else {
            prop.type_id
        }
    }

    fn constrain_tuple_types(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::solver::infer::InferenceVar>,
        source: &[TupleElement],
        target: &[TupleElement],
        priority: crate::solver::types::InferencePriority,
    ) {
        for (i, t_elem) in target.iter().enumerate() {
            if t_elem.rest {
                if var_map.contains_key(&t_elem.type_id) {
                    let tail = &target[i + 1..];
                    let mut trailing_count = 0usize;
                    let mut source_index = source.len();
                    for tail_elem in tail.iter().rev() {
                        if source_index <= i {
                            break;
                        }
                        let s_elem = &source[source_index - 1];
                        if s_elem.rest {
                            break;
                        }
                        let assignable = self
                            .checker
                            .is_assignable_to(s_elem.type_id, tail_elem.type_id);
                        if tail_elem.optional && !assignable {
                            break;
                        }
                        trailing_count += 1;
                        source_index -= 1;
                    }

                    let end_index = source.len().saturating_sub(trailing_count).max(i);
                    let mut tail = Vec::new();
                    for s_elem in source.iter().take(end_index).skip(i) {
                        tail.push(TupleElement {
                            type_id: s_elem.type_id,
                            name: s_elem.name,
                            optional: s_elem.optional,
                            rest: s_elem.rest,
                        });
                        if s_elem.rest {
                            break;
                        }
                    }
                    if tail.len() == 1 && tail[0].rest {
                        self.constrain_types(
                            ctx,
                            var_map,
                            tail[0].type_id,
                            t_elem.type_id,
                            priority,
                        );
                    } else {
                        let tail_tuple = self.interner.tuple(tail);
                        self.constrain_types(ctx, var_map, tail_tuple, t_elem.type_id, priority);
                    }
                    return;
                }
                let rest_elem_type = self.rest_element_type(t_elem.type_id);
                for s_elem in source.iter().skip(i) {
                    if s_elem.rest {
                        self.constrain_types(
                            ctx,
                            var_map,
                            s_elem.type_id,
                            t_elem.type_id,
                            priority,
                        );
                    } else {
                        self.constrain_types(
                            ctx,
                            var_map,
                            s_elem.type_id,
                            rest_elem_type,
                            priority,
                        );
                    }
                }
                return;
            }

            let Some(s_elem) = source.get(i) else {
                if t_elem.optional {
                    continue;
                }
                return;
            };

            if s_elem.rest {
                return;
            }

            self.constrain_types(ctx, var_map, s_elem.type_id, t_elem.type_id, priority);
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
        let actual_count = arg_types.len();

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

            match self.resolve_function_call(&func, arg_types) {
                CallResult::Success(ret) => return CallResult::Success(ret),
                CallResult::ArgumentTypeMismatch {
                    index: _,
                    expected,
                    actual,
                } => {
                    all_arg_count_mismatches = false;
                    failures.push(
                        crate::solver::diagnostics::PendingDiagnosticBuilder::argument_not_assignable(
                            actual, expected
                        )
                    );
                }
                CallResult::ArgumentCountMismatch {
                    expected_min,
                    expected_max,
                    actual,
                } => {
                    let expected = expected_max.unwrap_or(expected_min);
                    min_expected = min_expected.min(expected_min);
                    max_expected = max_expected.max(expected);
                    failures.push(
                        crate::solver::diagnostics::PendingDiagnosticBuilder::argument_count_mismatch(
                            expected, actual
                        )
                    );
                }
                _ => {
                    all_arg_count_mismatches = false;
                }
            }
        }

        // If all signatures failed due to argument count mismatch, report TS2554 instead of TS2769
        if all_arg_count_mismatches && !failures.is_empty() {
            return CallResult::ArgumentCountMismatch {
                expected_min: min_expected,
                expected_max: if max_expected > min_expected {
                    Some(max_expected)
                } else {
                    None
                },
                actual: actual_count,
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

// =============================================================================
// Generic Type Instantiation
// =============================================================================

/// Result of validating type arguments against their type parameter constraints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenericInstantiationResult {
    /// All type arguments satisfy their constraints
    Success,
    /// A type argument doesn't satisfy its type parameter constraint
    ConstraintViolation {
        /// Index of the type parameter that failed
        param_index: usize,
        /// Name of the type parameter that failed
        param_name: Atom,
        /// The constraint type
        constraint: TypeId,
        /// The provided type argument that doesn't satisfy the constraint
        type_arg: TypeId,
    },
}

/// Validate type arguments against their type parameter constraints.
///
/// This function is used when explicit type arguments are provided to a generic.
/// It ensures that each type argument satisfies its corresponding type parameter's
/// constraint, emitting errors instead of silently falling back to `Any`.
///
/// # Arguments
/// * `type_params` - The declared type parameters (e.g., `<T extends string, U>`)
/// * `type_args` - The provided type arguments (e.g., `<number, boolean>`)
/// * `checker` - The assignability checker to use for constraint validation
///
/// # Returns
/// * `GenericInstantiationResult::Success` if all constraints are satisfied
/// * `GenericInstantiationResult::ConstraintViolation` if any constraint is violated
pub fn solve_generic_instantiation<C: AssignabilityChecker>(
    type_params: &[TypeParamInfo],
    type_args: &[TypeId],
    interner: &dyn TypeDatabase,
    checker: &mut C,
) -> GenericInstantiationResult {
    use crate::solver::{TypeSubstitution, instantiate_type};

    for (i, (param, &type_arg)) in type_params.iter().zip(type_args.iter()).enumerate() {
        if let Some(constraint) = param.constraint {
            // Constraints may reference earlier type parameters, so instantiate them
            let instantiated_constraint = if i > 0 {
                let mut subst = TypeSubstitution::new();
                for (j, p) in type_params.iter().take(i).enumerate() {
                    if let Some(&arg) = type_args.get(j) {
                        subst.insert(p.name, arg);
                    }
                }
                instantiate_type(interner, constraint, &subst)
            } else {
                constraint
            };

            // Validate that the type argument satisfies the constraint
            if !checker.is_assignable_to(type_arg, instantiated_constraint) {
                return GenericInstantiationResult::ConstraintViolation {
                    param_index: i,
                    param_name: param.name,
                    constraint: instantiated_constraint,
                    type_arg,
                };
            }
        }
    }
    GenericInstantiationResult::Success
}

// Re-export property access types from extracted module
pub use crate::solver::operations_property::*;

// =============================================================================
// Binary Operations - Extracted to binary_ops.rs
// =============================================================================
//
// Binary operation evaluation has been extracted to `solver/binary_ops.rs`.
// The following are re-exported from that module:
// - BinaryOpEvaluator
// - BinaryOpResult
// - PrimitiveClass
//
// This extraction reduces operations.rs by ~330 lines and makes the code
// more maintainable by separating concerns.

#[cfg(test)]
#[path = "tests/operations_tests.rs"]
mod tests;
