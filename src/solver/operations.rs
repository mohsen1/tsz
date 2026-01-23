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

use crate::interner::Atom;
use crate::solver::diagnostics::PendingDiagnostic;
use crate::solver::evaluate::evaluate_type;
use crate::solver::infer::InferenceContext;
use crate::solver::instantiate::{TypeSubstitution, instantiate_type};
use crate::solver::types::*;
use crate::solver::utils;
use crate::solver::{
    ApparentMemberKind, QueryDatabase, TypeDatabase, apparent_object_member_kind,
    apparent_primitive_member_kind,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;

/// Maximum recursion depth for type constraint collection to prevent infinite loops.
const MAX_CONSTRAINT_RECURSION_DEPTH: usize = 100;

pub trait AssignabilityChecker {
    fn is_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool;

    fn is_assignable_to_strict(&mut self, source: TypeId, target: TypeId) -> bool {
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
    /// Current recursion depth for constrain_types to prevent infinite loops
    constraint_recursion_depth: RefCell<usize>,
}

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    pub fn new(interner: &'a dyn QueryDatabase, checker: &'a mut C) -> Self {
        CallEvaluator {
            interner,
            checker,
            defaulted_placeholders: FxHashSet::default(),
            constraint_recursion_depth: RefCell::new(0),
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
            is_method: false,
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

        // If all members succeeded, return a union of their return types
        if !return_types.is_empty() && failures.is_empty() {
            if return_types.len() == 1 {
                return CallResult::Success(return_types[0]);
            }
            // Return a union of all return types
            let union_result = self.interner.union(return_types);
            CallResult::Success(union_result)
        } else if !failures.is_empty() {
            // At least one member failed with a non-NotCallable error
            // Return the first failure (similar to how overloads are handled)
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

        if let Some(result) = self.check_argument_types(&func.params, arg_types) {
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
        let mut infer_ctx = InferenceContext::new(self.interner.as_type_database());
        let mut substitution = TypeSubstitution::new();
        let mut var_map: FxHashMap<TypeId, crate::solver::infer::InferenceVar> =
            FxHashMap::default();
        let mut type_param_vars = Vec::with_capacity(func.type_params.len());

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
            infer_ctx.register_type_param(placeholder_atom, var);
            let placeholder_key = TypeKey::TypeParameter(TypeParamInfo {
                name: placeholder_atom,
                constraint: tp.constraint,
                default: None,
            });
            let placeholder_id = self.interner.intern(placeholder_key);

            substitution.insert(tp.name, placeholder_id);
            var_map.insert(placeholder_id, var);

            // Add the type parameter constraint as an upper bound for the inference variable.
            // This ensures that inferred types like tuples [string, boolean] are validated
            // against constraints like `T extends any[]` during resolution.
            if let Some(constraint) = tp.constraint {
                infer_ctx.add_upper_bound(var, constraint);
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

        // 3. Collect constraints from arguments
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
                    // Check if argument is assignable to the type parameter's constraint
                    if !self.checker.is_assignable_to(arg_type, constraint) {
                        return CallResult::ArgumentTypeMismatch {
                            index: i,
                            expected: constraint,
                            actual: arg_type,
                        };
                    }
                }
            }

            // arg_type <: target_type
            self.constrain_types(&mut infer_ctx, &var_map, arg_type, target_type);
        }
        if let Some((_start, target_type, tuple_type)) = rest_tuple_inference {
            self.constrain_types(&mut infer_ctx, &var_map, tuple_type, target_type);
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
                        // Use ERROR as ultimate fallback to avoid returning Any (which silences TS2322)
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
                TypeId::ERROR
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
        if let Some(result) = self.check_argument_types_with(&instantiated_params, arg_types, true)
        {
            return result;
        }

        let return_type = instantiate_type(self.interner, func.return_type, &final_subst);
        CallResult::Success(return_type)
    }

    fn check_argument_types(
        &mut self,
        params: &[ParamInfo],
        arg_types: &[TypeId],
    ) -> Option<CallResult> {
        self.check_argument_types_with(params, arg_types, false)
    }

    fn check_argument_types_with(
        &mut self,
        params: &[ParamInfo],
        arg_types: &[TypeId],
        strict: bool,
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

            let assignable = if strict {
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
            TypeKey::TypeParameter(_)
            | TypeKey::Infer(_)
            | TypeKey::Intrinsic(_)
            | TypeKey::Literal(_)
            | TypeKey::Ref(_)
            | TypeKey::TypeQuery(_)
            | TypeKey::UniqueSymbol(_)
            | TypeKey::ThisType
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
    ) {
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
        self.constrain_types_impl(ctx, var_map, source, target);

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
    ) {
        if source == target {
            return;
        }

        // If target is an inference placeholder, add lower bound: source <: var
        if let Some(&var) = var_map.get(&target) {
            ctx.add_lower_bound(var, source);
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
                self.constrain_types(ctx, var_map, s_inner, t_inner);
            }
            (Some(TypeKey::ReadonlyType(s_inner)), _) => {
                self.constrain_types(ctx, var_map, s_inner, target);
            }
            (_, Some(TypeKey::ReadonlyType(t_inner))) => {
                self.constrain_types(ctx, var_map, source, t_inner);
            }
            (
                Some(TypeKey::IndexAccess(s_obj, s_idx)),
                Some(TypeKey::IndexAccess(t_obj, t_idx)),
            ) => {
                self.constrain_types(ctx, var_map, s_obj, t_obj);
                self.constrain_types(ctx, var_map, s_idx, t_idx);
            }
            (Some(TypeKey::KeyOf(s_inner)), Some(TypeKey::KeyOf(t_inner))) => {
                self.constrain_types(ctx, var_map, t_inner, s_inner);
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
                        self.constrain_types(ctx, var_map, *s_type, *t_type);
                    }
                }
            }
            (Some(TypeKey::IndexAccess(s_obj, s_idx)), _) => {
                let evaluated = self.interner.evaluate_index_access(s_obj, s_idx);
                if evaluated != source {
                    self.constrain_types(ctx, var_map, evaluated, target);
                }
            }
            (_, Some(TypeKey::IndexAccess(t_obj, t_idx))) => {
                let evaluated = self.interner.evaluate_index_access(t_obj, t_idx);
                if evaluated != target {
                    self.constrain_types(ctx, var_map, source, evaluated);
                }
            }
            (Some(TypeKey::Conditional(cond_id)), _) => {
                let cond = self.interner.conditional_type(cond_id);
                let evaluated = self.interner.evaluate_conditional(cond.as_ref());
                if evaluated != source {
                    self.constrain_types(ctx, var_map, evaluated, target);
                }
            }
            (_, Some(TypeKey::Conditional(cond_id))) => {
                let cond = self.interner.conditional_type(cond_id);
                let evaluated = self.interner.evaluate_conditional(cond.as_ref());
                if evaluated != target {
                    self.constrain_types(ctx, var_map, source, evaluated);
                }
            }
            (Some(TypeKey::Mapped(mapped_id)), _) => {
                let mapped = self.interner.mapped_type(mapped_id);
                let evaluated = self.interner.evaluate_mapped(mapped.as_ref());
                if evaluated != source {
                    self.constrain_types(ctx, var_map, evaluated, target);
                }
            }
            (_, Some(TypeKey::Mapped(mapped_id))) => {
                let mapped = self.interner.mapped_type(mapped_id);
                let evaluated = self.interner.evaluate_mapped(mapped.as_ref());
                if evaluated != target {
                    self.constrain_types(ctx, var_map, source, evaluated);
                }
            }
            (Some(TypeKey::Union(s_members)), _) => {
                let s_members = self.interner.type_list(s_members);
                for &member in s_members.iter() {
                    self.constrain_types(ctx, var_map, member, target);
                }
            }
            (_, Some(TypeKey::Intersection(t_members))) => {
                let t_members = self.interner.type_list(t_members);
                for &member in t_members.iter() {
                    self.constrain_types(ctx, var_map, source, member);
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
                    self.constrain_types(ctx, var_map, source, member);
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
                    self.constrain_types(ctx, var_map, source, member);
                }
            }
            (Some(TypeKey::Array(s_elem)), Some(TypeKey::Array(t_elem))) => {
                self.constrain_types(ctx, var_map, s_elem, t_elem);
            }
            (Some(TypeKey::Tuple(s_elems)), Some(TypeKey::Array(t_elem))) => {
                let s_elems = self.interner.tuple_list(s_elems);
                for s_elem in s_elems.iter() {
                    if s_elem.rest {
                        let rest_elem_type = self.rest_element_type(s_elem.type_id);
                        self.constrain_types(ctx, var_map, rest_elem_type, t_elem);
                    } else {
                        self.constrain_types(ctx, var_map, s_elem.type_id, t_elem);
                    }
                }
            }
            (Some(TypeKey::Tuple(s_elems)), Some(TypeKey::Tuple(t_elems))) => {
                let s_elems = self.interner.tuple_list(s_elems);
                let t_elems = self.interner.tuple_list(t_elems);
                self.constrain_tuple_types(ctx, var_map, &s_elems, &t_elems);
            }
            (Some(TypeKey::Function(s_fn_id)), Some(TypeKey::Function(t_fn_id))) => {
                let s_fn = self.interner.function_shape(s_fn_id);
                let t_fn = self.interner.function_shape(t_fn_id);
                // Contravariant parameters: target_param <: source_param
                for (s_p, t_p) in s_fn.params.iter().zip(t_fn.params.iter()) {
                    self.constrain_types(ctx, var_map, t_p.type_id, s_p.type_id);
                }
                if let (Some(s_this), Some(t_this)) = (s_fn.this_type, t_fn.this_type) {
                    self.constrain_types(ctx, var_map, t_this, s_this);
                }
                // Covariant return: source_return <: target_return
                self.constrain_types(ctx, var_map, s_fn.return_type, t_fn.return_type);
            }
            (Some(TypeKey::Function(s_fn_id)), Some(TypeKey::Callable(t_callable_id))) => {
                let s_fn = self.interner.function_shape(s_fn_id);
                let t_callable = self.interner.callable_shape(t_callable_id);
                for sig in &t_callable.call_signatures {
                    self.constrain_function_to_call_signature(ctx, var_map, &s_fn, sig);
                }
                if s_fn.is_constructor && t_callable.construct_signatures.len() == 1 {
                    let sig = &t_callable.construct_signatures[0];
                    if sig.type_params.is_empty() {
                        self.constrain_function_to_call_signature(ctx, var_map, &s_fn, sig);
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
                );
                self.constrain_matching_signatures(
                    ctx,
                    var_map,
                    &s_callable.construct_signatures,
                    &t_callable.construct_signatures,
                    true,
                );
            }
            (Some(TypeKey::Callable(s_callable_id)), Some(TypeKey::Function(t_fn_id))) => {
                let s_callable = self.interner.callable_shape(s_callable_id);
                let t_fn = self.interner.function_shape(t_fn_id);
                if s_callable.call_signatures.len() == 1 {
                    let sig = &s_callable.call_signatures[0];
                    if sig.type_params.is_empty() {
                        self.constrain_call_signature_to_function(ctx, var_map, sig, &t_fn);
                    }
                } else if let Some(index) = self.select_signature_for_target(
                    &s_callable.call_signatures,
                    target,
                    var_map,
                    false,
                ) {
                    let sig = &s_callable.call_signatures[index];
                    self.constrain_call_signature_to_function(ctx, var_map, sig, &t_fn);
                }
            }
            (Some(TypeKey::Object(s_shape_id)), Some(TypeKey::Object(t_shape_id))) => {
                let s_shape = self.interner.object_shape(s_shape_id);
                let t_shape = self.interner.object_shape(t_shape_id);
                self.constrain_properties(ctx, var_map, &s_shape.properties, &t_shape.properties);
            }
            (
                Some(TypeKey::ObjectWithIndex(s_shape_id)),
                Some(TypeKey::ObjectWithIndex(t_shape_id)),
            ) => {
                let s_shape = self.interner.object_shape(s_shape_id);
                let t_shape = self.interner.object_shape(t_shape_id);
                self.constrain_properties(ctx, var_map, &s_shape.properties, &t_shape.properties);
                if let (Some(s_idx), Some(t_idx)) = (&s_shape.string_index, &t_shape.string_index) {
                    self.constrain_types(ctx, var_map, s_idx.value_type, t_idx.value_type);
                }
                if let (Some(s_idx), Some(t_idx)) = (&s_shape.number_index, &t_shape.number_index) {
                    self.constrain_types(ctx, var_map, s_idx.value_type, t_idx.value_type);
                }
                self.constrain_properties_against_index_signatures(
                    ctx,
                    var_map,
                    &s_shape.properties,
                    &t_shape,
                );
                self.constrain_index_signatures_to_properties(
                    ctx,
                    var_map,
                    &s_shape,
                    &t_shape.properties,
                );
            }
            (Some(TypeKey::Object(s_shape_id)), Some(TypeKey::ObjectWithIndex(t_shape_id))) => {
                let s_shape = self.interner.object_shape(s_shape_id);
                let t_shape = self.interner.object_shape(t_shape_id);
                self.constrain_properties(ctx, var_map, &s_shape.properties, &t_shape.properties);
                self.constrain_properties_against_index_signatures(
                    ctx,
                    var_map,
                    &s_shape.properties,
                    &t_shape,
                );
            }
            (Some(TypeKey::ObjectWithIndex(s_shape_id)), Some(TypeKey::Object(t_shape_id))) => {
                let s_shape = self.interner.object_shape(s_shape_id);
                let t_shape = self.interner.object_shape(t_shape_id);
                self.constrain_properties(ctx, var_map, &s_shape.properties, &t_shape.properties);
                self.constrain_index_signatures_to_properties(
                    ctx,
                    var_map,
                    &s_shape,
                    &t_shape.properties,
                );
            }
            (Some(TypeKey::Application(s_app_id)), Some(TypeKey::Application(t_app_id))) => {
                let s_app = self.interner.type_application(s_app_id);
                let t_app = self.interner.type_application(t_app_id);
                if s_app.base == t_app.base && s_app.args.len() == t_app.args.len() {
                    for (s_arg, t_arg) in s_app.args.iter().zip(t_app.args.iter()) {
                        self.constrain_types(ctx, var_map, *s_arg, *t_arg);
                    }
                }
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
    ) {
        let mut source_idx = 0;
        let mut target_idx = 0;

        while source_idx < source_props.len() && target_idx < target_props.len() {
            let source = &source_props[source_idx];
            let target = &target_props[target_idx];

            match source.name.cmp(&target.name) {
                std::cmp::Ordering::Equal => {
                    self.constrain_types(ctx, var_map, source.type_id, target.type_id);
                    if !target.readonly
                        && (source.write_type != source.type_id
                            || target.write_type != target.type_id)
                    {
                        self.constrain_types(ctx, var_map, target.write_type, source.write_type);
                    }
                    source_idx += 1;
                    target_idx += 1;
                }
                std::cmp::Ordering::Less => {
                    source_idx += 1;
                }
                std::cmp::Ordering::Greater => {
                    target_idx += 1;
                }
            }
        }
    }

    fn constrain_function_to_call_signature(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::solver::infer::InferenceVar>,
        source: &FunctionShape,
        target: &CallSignature,
    ) {
        for (s_p, t_p) in source.params.iter().zip(target.params.iter()) {
            self.constrain_types(ctx, var_map, t_p.type_id, s_p.type_id);
        }
        if let (Some(s_this), Some(t_this)) = (source.this_type, target.this_type) {
            self.constrain_types(ctx, var_map, t_this, s_this);
        }
        self.constrain_types(ctx, var_map, source.return_type, target.return_type);
    }

    fn constrain_call_signature_to_function(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::solver::infer::InferenceVar>,
        source: &CallSignature,
        target: &FunctionShape,
    ) {
        for (s_p, t_p) in source.params.iter().zip(target.params.iter()) {
            self.constrain_types(ctx, var_map, t_p.type_id, s_p.type_id);
        }
        if let (Some(s_this), Some(t_this)) = (source.this_type, target.this_type) {
            self.constrain_types(ctx, var_map, t_this, s_this);
        }
        self.constrain_types(ctx, var_map, source.return_type, target.return_type);
    }

    fn constrain_call_signature_to_call_signature(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::solver::infer::InferenceVar>,
        source: &CallSignature,
        target: &CallSignature,
    ) {
        for (s_p, t_p) in source.params.iter().zip(target.params.iter()) {
            self.constrain_types(ctx, var_map, t_p.type_id, s_p.type_id);
        }
        if let (Some(s_this), Some(t_this)) = (source.this_type, target.this_type) {
            self.constrain_types(ctx, var_map, t_this, s_this);
        }
        self.constrain_types(ctx, var_map, source.return_type, target.return_type);
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
    ) {
        if source_signatures.is_empty() || target_signatures.is_empty() {
            return;
        }

        if source_signatures.len() == 1 && target_signatures.len() == 1 {
            let source_sig = &source_signatures[0];
            let target_sig = &target_signatures[0];
            if source_sig.type_params.is_empty() && target_sig.type_params.is_empty() {
                self.constrain_call_signature_to_call_signature(
                    ctx, var_map, source_sig, target_sig,
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
                        ctx, var_map, source_sig, target_sig,
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
                            ctx, var_map, source_sig, target_sig,
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
                        ctx, var_map, source_sig, target_sig,
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
                self.constrain_types(ctx, var_map, prop_type, number_idx.value_type);
            }

            if let Some(string_idx) = string_index {
                self.constrain_types(ctx, var_map, prop_type, string_idx.value_type);
            }
        }
    }

    fn constrain_index_signatures_to_properties(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::solver::infer::InferenceVar>,
        source: &ObjectShape,
        target_props: &[PropertyInfo],
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
                self.constrain_types(ctx, var_map, number_idx.value_type, prop_type);
            }

            if let Some(string_idx) = string_index {
                self.constrain_types(ctx, var_map, string_idx.value_type, prop_type);
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
                        self.constrain_types(ctx, var_map, tail[0].type_id, t_elem.type_id);
                    } else {
                        let tail_tuple = self.interner.tuple(tail);
                        self.constrain_types(ctx, var_map, tail_tuple, t_elem.type_id);
                    }
                    return;
                }
                let rest_elem_type = self.rest_element_type(t_elem.type_id);
                for s_elem in source.iter().skip(i) {
                    if s_elem.rest {
                        self.constrain_types(ctx, var_map, s_elem.type_id, t_elem.type_id);
                    } else {
                        self.constrain_types(ctx, var_map, s_elem.type_id, rest_elem_type);
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

            self.constrain_types(ctx, var_map, s_elem.type_id, t_elem.type_id);
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
                is_method: false,
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
                is_method: false,
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
    checker: &mut C,
) -> GenericInstantiationResult {
    for (i, (param, &type_arg)) in type_params.iter().zip(type_args.iter()).enumerate() {
        if let Some(constraint) = param.constraint {
            // Validate that the type argument satisfies the constraint
            if !checker.is_assignable_to(type_arg, constraint) {
                return GenericInstantiationResult::ConstraintViolation {
                    param_index: i,
                    param_name: param.name,
                    constraint,
                    type_arg,
                };
            }
        }
    }
    GenericInstantiationResult::Success
}

// =============================================================================
// Property Access Resolution
// =============================================================================

/// Result of attempting to access a property on a type.
#[derive(Clone, Debug)]
pub enum PropertyAccessResult {
    /// Property exists, returns its type
    Success {
        type_id: TypeId,
        /// True if this property was resolved via an index signature
        /// (not an explicit property declaration). Used for error 4111.
        from_index_signature: bool,
    },

    /// Property does not exist on this type
    PropertyNotFound {
        type_id: TypeId,
        property_name: Atom,
    },

    /// Type is possibly null or undefined.
    /// Contains the type of the property from non-nullable members (if any),
    /// and the specific nullable type causing the error.
    PossiblyNullOrUndefined {
        /// Type from valid non-nullable members (for recovery/optional chaining)
        property_type: Option<TypeId>,
        /// The nullable type causing the issue: NULL, UNDEFINED, or union of both
        cause: TypeId,
    },

    /// Type is unknown
    IsUnknown,
}

/// Evaluates property access.
pub struct PropertyAccessEvaluator<'a> {
    interner: &'a dyn TypeDatabase,
    no_unchecked_indexed_access: bool,
    mapped_access_visiting: RefCell<FxHashSet<TypeId>>,
    mapped_access_depth: RefCell<u32>,
}

struct MappedAccessGuard<'a> {
    evaluator: &'a PropertyAccessEvaluator<'a>,
    obj_type: TypeId,
}

impl<'a> Drop for MappedAccessGuard<'a> {
    fn drop(&mut self) {
        self.evaluator
            .mapped_access_visiting
            .borrow_mut()
            .remove(&self.obj_type);
        *self.evaluator.mapped_access_depth.borrow_mut() -= 1;
    }
}

impl<'a> PropertyAccessEvaluator<'a> {
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        PropertyAccessEvaluator {
            interner,
            no_unchecked_indexed_access: false,
            mapped_access_visiting: RefCell::new(FxHashSet::default()),
            mapped_access_depth: RefCell::new(0),
        }
    }

    pub fn set_no_unchecked_indexed_access(&mut self, enabled: bool) {
        self.no_unchecked_indexed_access = enabled;
    }

    /// Resolve property access: obj.prop -> type
    pub fn resolve_property_access(
        &self,
        obj_type: TypeId,
        prop_name: &str,
    ) -> PropertyAccessResult {
        self.resolve_property_access_inner(obj_type, prop_name, None)
    }

    fn enter_mapped_access_guard(&self, obj_type: TypeId) -> Option<MappedAccessGuard<'_>> {
        const MAX_MAPPED_ACCESS_DEPTH: u32 = 50;

        let mut depth = self.mapped_access_depth.borrow_mut();
        if *depth >= MAX_MAPPED_ACCESS_DEPTH {
            return None;
        }
        *depth += 1;
        drop(depth);

        let mut visiting = self.mapped_access_visiting.borrow_mut();
        if !visiting.insert(obj_type) {
            drop(visiting);
            *self.mapped_access_depth.borrow_mut() -= 1;
            return None;
        }

        Some(MappedAccessGuard {
            evaluator: self,
            obj_type,
        })
    }

    /// Check if a property name is a private field (starts with #)
    #[allow(dead_code)] // Infrastructure for private field checking
    fn is_private_field(&self, prop_name: &str) -> bool {
        prop_name.starts_with('#')
    }

    fn resolve_property_access_inner(
        &self,
        obj_type: TypeId,
        prop_name: &str,
        prop_atom: Option<Atom>,
    ) -> PropertyAccessResult {
        // Handle intrinsic types first
        if obj_type == TypeId::ANY {
            // Any type allows any property access, returning any
            return PropertyAccessResult::Success {
                type_id: TypeId::ANY,
                from_index_signature: false,
            };
        }

        if obj_type == TypeId::ERROR {
            // Error type suppresses further errors, returns error
            return PropertyAccessResult::Success {
                type_id: TypeId::ERROR,
                from_index_signature: false,
            };
        }

        if obj_type == TypeId::UNKNOWN {
            return PropertyAccessResult::IsUnknown;
        }

        if obj_type == TypeId::NULL || obj_type == TypeId::UNDEFINED || obj_type == TypeId::VOID {
            let cause = if obj_type == TypeId::VOID {
                TypeId::UNDEFINED
            } else {
                obj_type
            };
            return PropertyAccessResult::PossiblyNullOrUndefined {
                property_type: None,
                cause,
            };
        }

        // Handle Symbol primitive properties
        if obj_type == TypeId::SYMBOL {
            let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
            return self.resolve_symbol_primitive_property(prop_name, prop_atom);
        }

        // Look up the type key
        let key = match self.interner.lookup(obj_type) {
            Some(k) => k,
            None => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                return PropertyAccessResult::PropertyNotFound {
                    type_id: obj_type,
                    property_name: prop_atom,
                };
            }
        };

        match key {
            TypeKey::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                if let Some(prop) =
                    self.lookup_object_property(shape_id, &shape.properties, prop_atom)
                {
                    return PropertyAccessResult::Success {
                        type_id: self.optional_property_type(prop),
                        from_index_signature: false,
                    };
                }
                if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
                    return result;
                }
                PropertyAccessResult::PropertyNotFound {
                    type_id: obj_type,
                    property_name: prop_atom,
                }
            }

            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                if let Some(prop) =
                    self.lookup_object_property(shape_id, &shape.properties, prop_atom)
                {
                    return PropertyAccessResult::Success {
                        type_id: self.optional_property_type(prop),
                        from_index_signature: false,
                    };
                }

                if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
                    return result;
                }

                // Check string index signature (THIS is the case for error 4111)
                if let Some(ref idx) = shape.string_index {
                    return PropertyAccessResult::Success {
                        type_id: self.add_undefined_if_unchecked(idx.value_type),
                        from_index_signature: true, // Resolved via index signature!
                    };
                }

                PropertyAccessResult::PropertyNotFound {
                    type_id: obj_type,
                    property_name: prop_atom,
                }
            }

            TypeKey::Function(_) => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                self.resolve_function_property(obj_type, prop_name, prop_atom)
            }

            TypeKey::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                for prop in &shape.properties {
                    if prop.name == prop_atom {
                        return PropertyAccessResult::Success {
                            type_id: self.optional_property_type(prop),
                            from_index_signature: false,
                        };
                    }
                }
                // Check string index signature (for static index signatures on class constructors)
                if let Some(ref idx) = shape.string_index {
                    return PropertyAccessResult::Success {
                        type_id: self.add_undefined_if_unchecked(idx.value_type),
                        from_index_signature: true,
                    };
                }
                self.resolve_function_property(obj_type, prop_name, prop_atom)
            }

            TypeKey::Union(members) => {
                let members = self.interner.type_list(members);
                // Property access on union: partition into nullable and non-nullable members
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                let mut valid_results = Vec::new();
                let mut nullable_causes = Vec::new();
                let mut any_from_index = false; // Track if any member used index signature

                for &member in members.iter() {
                    // Check for null/undefined directly
                    if member == TypeId::NULL
                        || member == TypeId::UNDEFINED
                        || member == TypeId::VOID
                    {
                        let cause = if member == TypeId::VOID {
                            TypeId::UNDEFINED
                        } else {
                            member
                        };
                        nullable_causes.push(cause);
                        continue;
                    }

                    match self.resolve_property_access_inner(member, prop_name, Some(prop_atom)) {
                        PropertyAccessResult::Success {
                            type_id,
                            from_index_signature,
                        } => {
                            valid_results.push(type_id);
                            if from_index_signature {
                                any_from_index = true; // Propagate: if ANY member uses index, flag it
                            }
                        }
                        PropertyAccessResult::PossiblyNullOrUndefined {
                            property_type,
                            cause,
                        } => {
                            if let Some(t) = property_type {
                                valid_results.push(t);
                            }
                            nullable_causes.push(cause);
                        }
                        // If any non-nullable member is missing the property, it's a PropertyNotFound error
                        _ => {
                            return PropertyAccessResult::PropertyNotFound {
                                type_id: obj_type,
                                property_name: prop_atom,
                            };
                        }
                    }
                }

                // If there are nullable causes, return PossiblyNullOrUndefined
                if !nullable_causes.is_empty() {
                    let cause = if nullable_causes.len() == 1 {
                        nullable_causes[0]
                    } else {
                        self.interner.union(nullable_causes)
                    };

                    let mut property_type = if valid_results.is_empty() {
                        None
                    } else if valid_results.len() == 1 {
                        Some(valid_results[0])
                    } else {
                        Some(self.interner.union(valid_results))
                    };

                    if any_from_index
                        && self.no_unchecked_indexed_access
                        && let Some(t) = property_type
                    {
                        property_type = Some(self.add_undefined_if_unchecked(t));
                    }

                    return PropertyAccessResult::PossiblyNullOrUndefined {
                        property_type,
                        cause,
                    };
                }

                let mut type_id = self.interner.union(valid_results);
                if any_from_index && self.no_unchecked_indexed_access {
                    type_id = self.add_undefined_if_unchecked(type_id);
                }

                // Union of all result types
                PropertyAccessResult::Success {
                    type_id,
                    from_index_signature: any_from_index, // Contagious across union members
                }
            }

            TypeKey::Intersection(members) => {
                let members = self.interner.type_list(members);
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                let mut results = Vec::new();
                let mut any_from_index = false;
                let mut nullable_causes = Vec::new();
                let mut saw_unknown = false;

                for &member in members.iter() {
                    match self.resolve_property_access_inner(member, prop_name, Some(prop_atom)) {
                        PropertyAccessResult::Success {
                            type_id,
                            from_index_signature,
                        } => {
                            results.push(type_id);
                            if from_index_signature {
                                any_from_index = true;
                            }
                        }
                        PropertyAccessResult::PossiblyNullOrUndefined {
                            property_type,
                            cause,
                        } => {
                            if let Some(t) = property_type {
                                results.push(t);
                            }
                            nullable_causes.push(cause);
                        }
                        PropertyAccessResult::IsUnknown => {
                            saw_unknown = true;
                        }
                        PropertyAccessResult::PropertyNotFound { .. } => {}
                    }
                }

                if results.is_empty() {
                    if !nullable_causes.is_empty() {
                        let cause = if nullable_causes.len() == 1 {
                            nullable_causes[0]
                        } else {
                            self.interner.union(nullable_causes)
                        };
                        return PropertyAccessResult::PossiblyNullOrUndefined {
                            property_type: None,
                            cause,
                        };
                    }
                    if saw_unknown {
                        return PropertyAccessResult::IsUnknown;
                    }
                    return PropertyAccessResult::PropertyNotFound {
                        type_id: obj_type,
                        property_name: prop_atom,
                    };
                }

                let mut type_id = if results.len() == 1 {
                    results[0]
                } else {
                    self.interner.intersection(results)
                };
                if any_from_index && self.no_unchecked_indexed_access {
                    type_id = self.add_undefined_if_unchecked(type_id);
                }

                PropertyAccessResult::Success {
                    type_id,
                    from_index_signature: any_from_index,
                }
            }

            TypeKey::ReadonlyType(inner) => {
                self.resolve_property_access_inner(inner, prop_name, prop_atom)
            }

            TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                if let Some(constraint) = info.constraint {
                    if constraint == obj_type {
                        PropertyAccessResult::PropertyNotFound {
                            type_id: obj_type,
                            property_name: prop_atom,
                        }
                    } else {
                        self.resolve_property_access_inner(constraint, prop_name, Some(prop_atom))
                    }
                } else {
                    PropertyAccessResult::PropertyNotFound {
                        type_id: obj_type,
                        property_name: prop_atom,
                    }
                }
            }

            // TS apparent members: literals inherit primitive wrapper methods.
            TypeKey::Literal(ref literal) => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                match literal {
                    LiteralValue::String(_) => self.resolve_string_property(prop_name, prop_atom),
                    LiteralValue::Number(_) => self.resolve_number_property(prop_name, prop_atom),
                    LiteralValue::Boolean(_) => self.resolve_boolean_property(prop_name, prop_atom),
                    LiteralValue::BigInt(_) => self.resolve_bigint_property(prop_name, prop_atom),
                }
            }

            TypeKey::TemplateLiteral(_) => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                self.resolve_string_property(prop_name, prop_atom)
            }

            // Built-in properties
            TypeKey::Intrinsic(IntrinsicKind::String) => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                self.resolve_string_property(prop_name, prop_atom)
            }

            TypeKey::Intrinsic(IntrinsicKind::Number) => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                self.resolve_number_property(prop_name, prop_atom)
            }

            TypeKey::Intrinsic(IntrinsicKind::Boolean) => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                self.resolve_boolean_property(prop_name, prop_atom)
            }

            TypeKey::Intrinsic(IntrinsicKind::Bigint) => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                self.resolve_bigint_property(prop_name, prop_atom)
            }

            TypeKey::Intrinsic(IntrinsicKind::Object) => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                self.resolve_object_member(prop_name, prop_atom).unwrap_or(
                    PropertyAccessResult::PropertyNotFound {
                        type_id: obj_type,
                        property_name: prop_atom,
                    },
                )
            }

            TypeKey::Array(_) => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                self.resolve_array_property(obj_type, prop_name, prop_atom)
            }

            TypeKey::Tuple(_) => {
                let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
                self.resolve_array_property(obj_type, prop_name, prop_atom)
            }

            // Application: evaluate the generic type and resolve property on the result
            TypeKey::Application(_) => {
                let _guard = match self.enter_mapped_access_guard(obj_type) {
                    Some(guard) => guard,
                    None => {
                        // Instead of returning IsUnknown (which causes TS2571), treat as property not found
                        // This handles circular references and deep nesting more conservatively
                        return PropertyAccessResult::PropertyNotFound {
                            type_id: obj_type,
                            property_name: prop_atom
                                .unwrap_or_else(|| self.interner.intern_string(prop_name)),
                        };
                    }
                };

                let evaluated = evaluate_type(self.interner, obj_type);
                if evaluated != obj_type {
                    // Successfully evaluated - resolve property on the concrete type
                    self.resolve_property_access_inner(evaluated, prop_name, prop_atom)
                } else {
                    // Evaluation didn't change the type - property not found
                    PropertyAccessResult::PropertyNotFound {
                        type_id: obj_type,
                        property_name: prop_atom
                            .unwrap_or_else(|| self.interner.intern_string(prop_name)),
                    }
                }
            }

            // Mapped: evaluate the mapped type to get concrete properties
            TypeKey::Mapped(_) => {
                let _guard = match self.enter_mapped_access_guard(obj_type) {
                    Some(guard) => guard,
                    None => {
                        return PropertyAccessResult::PropertyNotFound {
                            type_id: obj_type,
                            property_name: prop_atom
                                .unwrap_or_else(|| self.interner.intern_string(prop_name)),
                        };
                    }
                };

                let evaluated = evaluate_type(self.interner, obj_type);
                if evaluated != obj_type {
                    // Successfully evaluated - resolve property on the concrete type
                    self.resolve_property_access_inner(evaluated, prop_name, prop_atom)
                } else {
                    // Evaluation didn't change the type - property not found
                    PropertyAccessResult::PropertyNotFound {
                        type_id: obj_type,
                        property_name: prop_atom
                            .unwrap_or_else(|| self.interner.intern_string(prop_name)),
                    }
                }
            }

            _ => PropertyAccessResult::PropertyNotFound {
                type_id: obj_type,
                property_name: prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name)),
            },
        }
    }

    fn lookup_object_property<'props>(
        &self,
        shape_id: ObjectShapeId,
        props: &'props [PropertyInfo],
        prop_atom: Atom,
    ) -> Option<&'props PropertyInfo> {
        match self.interner.object_property_index(shape_id, prop_atom) {
            PropertyLookup::Found(idx) => props.get(idx),
            PropertyLookup::NotFound => None,
            PropertyLookup::Uncached => props.iter().find(|p| p.name == prop_atom),
        }
    }

    fn any_args_function(&self, return_type: TypeId) -> TypeId {
        let rest_array = self.interner.array(TypeId::ANY);
        let rest_param = ParamInfo {
            name: None,
            type_id: rest_array,
            optional: false,
            rest: true,
        };
        self.interner.function(FunctionShape {
            params: vec![rest_param],
            this_type: None,
            return_type,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    }

    fn method_result(&self, return_type: TypeId) -> PropertyAccessResult {
        PropertyAccessResult::Success {
            type_id: self.any_args_function(return_type),
            from_index_signature: false,
        }
    }

    fn add_undefined_if_unchecked(&self, type_id: TypeId) -> TypeId {
        if !self.no_unchecked_indexed_access || type_id == TypeId::UNDEFINED {
            return type_id;
        }
        self.interner.union2(type_id, TypeId::UNDEFINED)
    }

    fn optional_property_type(&self, prop: &PropertyInfo) -> TypeId {
        if prop.optional {
            self.interner.union2(prop.type_id, TypeId::UNDEFINED)
        } else {
            prop.type_id
        }
    }

    fn resolve_apparent_property(
        &self,
        kind: IntrinsicKind,
        owner_type: TypeId,
        prop_name: &str,
        prop_atom: Atom,
    ) -> PropertyAccessResult {
        match apparent_primitive_member_kind(self.interner, kind, prop_name) {
            Some(ApparentMemberKind::Value(type_id)) => PropertyAccessResult::Success {
                type_id,
                from_index_signature: false,
            },
            Some(ApparentMemberKind::Method(return_type)) => self.method_result(return_type),
            None => PropertyAccessResult::PropertyNotFound {
                type_id: owner_type,
                property_name: prop_atom,
            },
        }
    }

    fn resolve_object_member(
        &self,
        prop_name: &str,
        _prop_atom: Atom,
    ) -> Option<PropertyAccessResult> {
        match apparent_object_member_kind(prop_name) {
            Some(ApparentMemberKind::Value(type_id)) => Some(PropertyAccessResult::Success {
                type_id,
                from_index_signature: false,
            }),
            Some(ApparentMemberKind::Method(return_type)) => Some(self.method_result(return_type)),
            None => None,
        }
    }

    /// Resolve properties on string type.
    fn resolve_string_property(&self, prop_name: &str, prop_atom: Atom) -> PropertyAccessResult {
        self.resolve_primitive_property(IntrinsicKind::String, TypeId::STRING, prop_name, prop_atom)
    }

    /// Resolve properties on number type.
    fn resolve_number_property(&self, prop_name: &str, prop_atom: Atom) -> PropertyAccessResult {
        self.resolve_primitive_property(IntrinsicKind::Number, TypeId::NUMBER, prop_name, prop_atom)
    }

    /// Resolve properties on boolean type.
    fn resolve_boolean_property(&self, prop_name: &str, prop_atom: Atom) -> PropertyAccessResult {
        self.resolve_primitive_property(
            IntrinsicKind::Boolean,
            TypeId::BOOLEAN,
            prop_name,
            prop_atom,
        )
    }

    /// Resolve properties on bigint type.
    fn resolve_bigint_property(&self, prop_name: &str, prop_atom: Atom) -> PropertyAccessResult {
        self.resolve_primitive_property(IntrinsicKind::Bigint, TypeId::BIGINT, prop_name, prop_atom)
    }

    /// Helper to resolve properties on primitive types.
    /// Extracted to reduce duplication across string/number/boolean/bigint property resolvers.
    fn resolve_primitive_property(
        &self,
        kind: IntrinsicKind,
        type_id: TypeId,
        prop_name: &str,
        prop_atom: Atom,
    ) -> PropertyAccessResult {
        self.resolve_apparent_property(kind, type_id, prop_name, prop_atom)
    }

    /// Resolve properties on symbol primitive type.
    fn resolve_symbol_primitive_property(
        &self,
        prop_name: &str,
        prop_atom: Atom,
    ) -> PropertyAccessResult {
        if prop_name == "toString" || prop_name == "valueOf" {
            return PropertyAccessResult::Success {
                type_id: TypeId::ANY,
                from_index_signature: false,
            };
        }

        self.resolve_apparent_property(IntrinsicKind::Symbol, TypeId::SYMBOL, prop_name, prop_atom)
    }

    /// Resolve properties on array type.
    fn resolve_array_property(
        &self,
        array_type: TypeId,
        prop_name: &str,
        prop_atom: Atom,
    ) -> PropertyAccessResult {
        let element_type = self.array_element_type(array_type);
        let array_of_element = self.interner.array(element_type);
        let element_or_undefined = self.element_type_with_undefined(element_type);

        match prop_name {
            // Array properties
            "length" => PropertyAccessResult::Success {
                type_id: TypeId::NUMBER,
                from_index_signature: false,
            },

            // Array methods that return arrays
            "concat" => {
                let union_item = self.interner.union2(element_type, array_of_element);
                let rest_items = self.interner.array(union_item);
                self.function_result(
                    Vec::new(),
                    vec![self.param(rest_items, false, true)],
                    array_of_element,
                )
            }
            "filter" => {
                let callback =
                    self.array_callback_type(element_type, array_of_element, TypeId::BOOLEAN);
                self.function_result(
                    Vec::new(),
                    vec![
                        self.param(callback, false, false),
                        self.param(TypeId::ANY, true, false),
                    ],
                    array_of_element,
                )
            }
            "flat" => {
                let flat_element = self.flatten_once_type(element_type);
                let flat_array = self.interner.array(flat_element);
                self.function_result(
                    Vec::new(),
                    vec![self.param(TypeId::NUMBER, true, false)],
                    flat_array,
                )
            }
            "flatMap" => {
                let u_param = self.type_param("U");
                let u_type = self.type_param_type(&u_param);
                let array_u = self.interner.array(u_type);
                let callback_return = self.interner.union2(u_type, array_u);
                let callback =
                    self.array_callback_type(element_type, array_of_element, callback_return);
                self.function_result(
                    vec![u_param],
                    vec![
                        self.param(callback, false, false),
                        self.param(TypeId::ANY, true, false),
                    ],
                    array_u,
                )
            }
            "map" => {
                let u_param = self.type_param("U");
                let u_type = self.type_param_type(&u_param);
                let callback = self.array_callback_type(element_type, array_of_element, u_type);
                let array_u = self.interner.array(u_type);
                self.function_result(
                    vec![u_param],
                    vec![
                        self.param(callback, false, false),
                        self.param(TypeId::ANY, true, false),
                    ],
                    array_u,
                )
            }
            "reverse" | "toReversed" => {
                self.function_result(Vec::new(), Vec::new(), array_of_element)
            }
            "slice" => self.function_result(
                Vec::new(),
                vec![
                    self.param(TypeId::NUMBER, true, false),
                    self.param(TypeId::NUMBER, true, false),
                ],
                array_of_element,
            ),
            "sort" | "toSorted" => {
                let compare = self.array_compare_callback_type(element_type);
                self.function_result(
                    Vec::new(),
                    vec![self.param(compare, true, false)],
                    array_of_element,
                )
            }
            "splice" | "toSpliced" => self.function_result(
                Vec::new(),
                vec![
                    self.param(TypeId::NUMBER, false, false),
                    self.param(TypeId::NUMBER, true, false),
                    self.param(self.interner.array(element_type), false, true),
                ],
                array_of_element,
            ),
            "with" => self.function_result(
                Vec::new(),
                vec![
                    self.param(TypeId::NUMBER, false, false),
                    self.param(element_type, false, false),
                ],
                array_of_element,
            ),

            // Array methods that return specific types
            "at" => self.function_result(
                Vec::new(),
                vec![self.param(TypeId::NUMBER, false, false)],
                element_or_undefined,
            ),
            "find" | "findLast" => {
                let callback =
                    self.array_callback_type(element_type, array_of_element, TypeId::BOOLEAN);
                self.function_result(
                    Vec::new(),
                    vec![
                        self.param(callback, false, false),
                        self.param(TypeId::ANY, true, false),
                    ],
                    element_or_undefined,
                )
            }
            "pop" | "shift" => self.function_result(Vec::new(), Vec::new(), element_or_undefined),

            "every" | "includes" | "some" => {
                let params = match prop_name {
                    "includes" => vec![
                        self.param(element_type, false, false),
                        self.param(TypeId::NUMBER, true, false),
                    ],
                    _ => {
                        let callback = self.array_callback_type(
                            element_type,
                            array_of_element,
                            TypeId::BOOLEAN,
                        );
                        vec![
                            self.param(callback, false, false),
                            self.param(TypeId::ANY, true, false),
                        ]
                    }
                };
                self.function_result(Vec::new(), params, TypeId::BOOLEAN)
            }

            "findIndex" | "findLastIndex" | "indexOf" | "lastIndexOf" | "push" | "unshift" => {
                let params = match prop_name {
                    "push" | "unshift" => {
                        vec![self.param(self.interner.array(element_type), false, true)]
                    }
                    "indexOf" | "lastIndexOf" => vec![
                        self.param(element_type, false, false),
                        self.param(TypeId::NUMBER, true, false),
                    ],
                    _ => {
                        let callback = self.array_callback_type(
                            element_type,
                            array_of_element,
                            TypeId::BOOLEAN,
                        );
                        vec![
                            self.param(callback, false, false),
                            self.param(TypeId::ANY, true, false),
                        ]
                    }
                };
                self.function_result(Vec::new(), params, TypeId::NUMBER)
            }

            "forEach" | "copyWithin" | "fill" => {
                let (params, return_type) = match prop_name {
                    "forEach" => {
                        let callback =
                            self.array_callback_type(element_type, array_of_element, TypeId::VOID);
                        (
                            vec![
                                self.param(callback, false, false),
                                self.param(TypeId::ANY, true, false),
                            ],
                            TypeId::VOID,
                        )
                    }
                    "copyWithin" => (
                        vec![
                            self.param(TypeId::NUMBER, false, false),
                            self.param(TypeId::NUMBER, true, false),
                            self.param(TypeId::NUMBER, true, false),
                        ],
                        array_of_element,
                    ),
                    _ => (
                        vec![
                            self.param(element_type, false, false),
                            self.param(TypeId::NUMBER, true, false),
                            self.param(TypeId::NUMBER, true, false),
                        ],
                        array_of_element,
                    ),
                };
                self.function_result(Vec::new(), params, return_type)
            }

            "join" | "toLocaleString" | "toString" => {
                let params = if prop_name == "join" {
                    vec![self.param(TypeId::STRING, true, false)]
                } else {
                    Vec::new()
                };
                self.function_result(Vec::new(), params, TypeId::STRING)
            }

            "entries" | "keys" | "values" => {
                let return_type = match prop_name {
                    "entries" => {
                        let tuple = self.interner.tuple(vec![
                            TupleElement {
                                type_id: TypeId::NUMBER,
                                name: None,
                                optional: false,
                                rest: false,
                            },
                            TupleElement {
                                type_id: element_type,
                                name: None,
                                optional: false,
                                rest: false,
                            },
                        ]);
                        self.interner.array(tuple)
                    }
                    "keys" => self.interner.array(TypeId::NUMBER),
                    _ => array_of_element,
                };
                self.function_result(Vec::new(), Vec::new(), return_type)
            }

            "reduce" | "reduceRight" => {
                self.callable_result(self.array_reduce_callable(element_type, array_of_element))
            }

            _ => PropertyAccessResult::PropertyNotFound {
                type_id: array_type,
                property_name: prop_atom,
            },
        }
    }

    pub(crate) fn array_element_type(&self, array_type: TypeId) -> TypeId {
        match self.interner.lookup(array_type) {
            Some(TypeKey::Array(elem)) => elem,
            Some(TypeKey::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                self.tuple_element_union(&elements)
            }
            _ => TypeId::ERROR, // Return ERROR instead of ANY for non-array/tuple types
        }
    }

    fn tuple_element_union(&self, elements: &[TupleElement]) -> TypeId {
        let mut members = Vec::new();
        for elem in elements {
            let mut ty = if elem.rest {
                self.array_element_type(elem.type_id)
            } else {
                elem.type_id
            };
            if elem.optional {
                ty = self.element_type_with_undefined(ty);
            }
            members.push(ty);
        }
        self.interner.union(members)
    }

    fn element_type_with_undefined(&self, element_type: TypeId) -> TypeId {
        self.interner.union2(element_type, TypeId::UNDEFINED)
    }

    fn flatten_once_type(&self, element_type: TypeId) -> TypeId {
        match self.interner.lookup(element_type) {
            Some(TypeKey::Array(elem)) => elem,
            Some(TypeKey::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                self.tuple_element_union(&elements)
            }
            Some(TypeKey::Union(members)) => {
                let members = self.interner.type_list(members);
                let mut flat = Vec::with_capacity(members.len());
                for &member in members.iter() {
                    flat.push(self.flatten_once_type(member));
                }
                self.interner.union(flat)
            }
            _ => element_type,
        }
    }

    fn array_callback_type(
        &self,
        element_type: TypeId,
        array_type: TypeId,
        return_type: TypeId,
    ) -> TypeId {
        self.function_type(
            Vec::new(),
            vec![
                self.param(element_type, false, false),
                self.param(TypeId::NUMBER, false, false),
                self.param(array_type, false, false),
            ],
            return_type,
        )
    }

    fn array_compare_callback_type(&self, element_type: TypeId) -> TypeId {
        self.function_type(
            Vec::new(),
            vec![
                self.param(element_type, false, false),
                self.param(element_type, false, false),
            ],
            TypeId::NUMBER,
        )
    }

    fn array_reduce_callable(&self, element_type: TypeId, array_type: TypeId) -> CallableShape {
        let callback_no_init =
            self.array_reduce_callback_type(element_type, element_type, array_type);
        let no_init = CallSignature {
            type_params: Vec::new(),
            params: vec![self.param(callback_no_init, false, false)],
            this_type: None,
            return_type: element_type,
            type_predicate: None,
        };

        let u_param = self.type_param("U");
        let u_type = self.type_param_type(&u_param);
        let callback_with_init = self.array_reduce_callback_type(u_type, element_type, array_type);
        let with_init = CallSignature {
            type_params: vec![u_param],
            params: vec![
                self.param(callback_with_init, false, false),
                self.param(u_type, false, false),
            ],
            this_type: None,
            return_type: u_type,
            type_predicate: None,
        };

        CallableShape {
            call_signatures: vec![no_init, with_init],
            construct_signatures: Vec::new(),
            properties: Vec::new(),
            ..Default::default()
        }
    }

    fn array_reduce_callback_type(
        &self,
        accumulator_type: TypeId,
        element_type: TypeId,
        array_type: TypeId,
    ) -> TypeId {
        self.function_type(
            Vec::new(),
            vec![
                self.param(accumulator_type, false, false),
                self.param(element_type, false, false),
                self.param(TypeId::NUMBER, false, false),
                self.param(array_type, false, false),
            ],
            accumulator_type,
        )
    }

    fn type_param(&self, name: &str) -> TypeParamInfo {
        TypeParamInfo {
            name: self.interner.intern_string(name),
            constraint: None,
            default: None,
        }
    }

    fn type_param_type(&self, param: &TypeParamInfo) -> TypeId {
        self.interner.intern(TypeKey::TypeParameter(param.clone()))
    }

    fn param(&self, type_id: TypeId, optional: bool, rest: bool) -> ParamInfo {
        ParamInfo {
            name: None,
            type_id,
            optional,
            rest,
        }
    }

    fn function_type(
        &self,
        type_params: Vec<TypeParamInfo>,
        params: Vec<ParamInfo>,
        return_type: TypeId,
    ) -> TypeId {
        self.interner.function(FunctionShape {
            type_params,
            params,
            this_type: None,
            return_type,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    }

    fn function_result(
        &self,
        type_params: Vec<TypeParamInfo>,
        params: Vec<ParamInfo>,
        return_type: TypeId,
    ) -> PropertyAccessResult {
        PropertyAccessResult::Success {
            type_id: self.function_type(type_params, params, return_type),
            from_index_signature: false,
        }
    }

    fn callable_result(&self, callable: CallableShape) -> PropertyAccessResult {
        PropertyAccessResult::Success {
            type_id: self.interner.callable(callable),
            from_index_signature: false,
        }
    }

    fn resolve_function_property(
        &self,
        func_type: TypeId,
        prop_name: &str,
        prop_atom: Atom,
    ) -> PropertyAccessResult {
        match prop_name {
            "apply" | "call" | "bind" => self.method_result(TypeId::ANY),
            "toString" => self.method_result(TypeId::STRING),
            "length" => PropertyAccessResult::Success {
                type_id: TypeId::NUMBER,
                from_index_signature: false,
            },
            "prototype" | "arguments" => PropertyAccessResult::Success {
                type_id: TypeId::ANY,
                from_index_signature: false,
            },
            "caller" => PropertyAccessResult::Success {
                type_id: self.any_args_function(TypeId::ANY),
                from_index_signature: false,
            },
            _ => {
                if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
                    return result;
                }
                PropertyAccessResult::PropertyNotFound {
                    type_id: func_type,
                    property_name: prop_atom,
                }
            }
        }
    }
}

pub fn property_is_readonly(interner: &dyn TypeDatabase, type_id: TypeId, prop_name: &str) -> bool {
    match interner.lookup(type_id) {
        Some(TypeKey::ReadonlyType(inner)) => {
            if let Some(TypeKey::Array(_) | TypeKey::Tuple(_)) = interner.lookup(inner)
                && is_numeric_index_name(prop_name)
            {
                return true;
            }
            property_is_readonly(interner, inner, prop_name)
        }
        Some(TypeKey::Object(shape_id)) => {
            object_property_is_readonly(interner, shape_id, prop_name)
        }
        Some(TypeKey::ObjectWithIndex(shape_id)) => {
            indexed_object_property_is_readonly(interner, shape_id, prop_name)
        }
        Some(TypeKey::Union(types)) | Some(TypeKey::Intersection(types)) => {
            let types = interner.type_list(types);
            types
                .iter()
                .any(|t| property_is_readonly(interner, *t, prop_name))
        }
        _ => false,
    }
}

/// Check if a property on a plain object type is readonly.
fn object_property_is_readonly(
    interner: &dyn TypeDatabase,
    shape_id: ObjectShapeId,
    prop_name: &str,
) -> bool {
    let shape = interner.object_shape(shape_id);
    let prop_atom = interner.intern_string(prop_name);
    shape
        .properties
        .iter()
        .find(|prop| prop.name == prop_atom)
        .is_some_and(|prop| prop.readonly)
}

/// Check if a property on an indexed object type is readonly.
/// Checks both named properties and index signatures.
fn indexed_object_property_is_readonly(
    interner: &dyn TypeDatabase,
    shape_id: ObjectShapeId,
    prop_name: &str,
) -> bool {
    let shape = interner.object_shape(shape_id);
    let prop_atom = interner.intern_string(prop_name);

    // Check named property first
    if let Some(prop) = shape.properties.iter().find(|prop| prop.name == prop_atom) {
        return prop.readonly;
    }

    // Check index signatures for numeric properties
    if is_numeric_index_name(prop_name) {
        if shape.string_index.as_ref().is_some_and(|idx| idx.readonly) {
            return true;
        }
        if shape.number_index.as_ref().is_some_and(|idx| idx.readonly) {
            return true;
        }
    }

    false
}

/// Check if an index signature is readonly for the given type.
///
/// # Parameters
/// - `wants_string`: Check if string index signature should be readonly
/// - `wants_number`: Check if numeric index signature should be readonly
///
/// # Returns
/// `true` if the requested index signature is readonly, `false` otherwise.
///
/// # Examples
/// - `{ readonly [x: string]: string }` → `is_readonly_index_signature(t, true, false)` = `true`
/// - `{ [x: string]: string }` → `is_readonly_index_signature(t, true, false)` = `false`
pub fn is_readonly_index_signature(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    wants_string: bool,
    wants_number: bool,
) -> bool {
    match interner.lookup(type_id) {
        Some(TypeKey::ReadonlyType(inner)) => {
            if wants_number
                && let Some(TypeKey::Array(_) | TypeKey::Tuple(_)) = interner.lookup(inner)
            {
                return true;
            }
            is_readonly_index_signature(interner, inner, wants_string, wants_number)
        }
        Some(TypeKey::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            (wants_string && shape.string_index.as_ref().is_some_and(|idx| idx.readonly))
                || (wants_number && shape.number_index.as_ref().is_some_and(|idx| idx.readonly))
        }
        Some(TypeKey::Union(types)) | Some(TypeKey::Intersection(types)) => {
            let types = interner.type_list(types);
            types
                .iter()
                .any(|t| is_readonly_index_signature(interner, *t, wants_string, wants_number))
        }
        _ => false,
    }
}

/// Check if a string represents a valid numeric property name.
///
/// Returns `true` only for non-negative finite integers that round-trip correctly
/// through JavaScript's `Number.toString()` conversion.
///
/// This is used for determining if a property access can use numeric index signatures:
/// - `"0"` through `"4294967295"` are valid numeric property names (fits in usize)
/// - `"1.5"`, `"-1"`, `NaN`, `Infinity` are NOT valid numeric property names
///
/// # Examples
/// - `is_numeric_index_name("0")` → `true`
/// - `is_numeric_index_name("42")` → `true`
/// - `is_numeric_index_name("1.5")` → `false` (fractional part)
/// - `is_numeric_index_name("-1")` → `false` (negative)
/// - `is_numeric_index_name("NaN")` → `false` (special value)
fn is_numeric_index_name(name: &str) -> bool {
    let parsed: f64 = match name.parse() {
        Ok(value) => value,
        Err(_) => return false,
    };
    if !parsed.is_finite() || parsed.fract() != 0.0 || parsed < 0.0 {
        return false;
    }
    parsed <= (usize::MAX as f64)
}

// =============================================================================
// Binary Operations
// =============================================================================

/// Result of a binary operation.
#[derive(Clone, Debug)]
pub enum BinaryOpResult {
    /// Operation succeeded
    Success(TypeId),

    /// Type error in operation
    TypeError {
        left: TypeId,
        right: TypeId,
        op: &'static str,
    },
}

/// Evaluates binary operations.
pub struct BinaryOpEvaluator<'a> {
    interner: &'a dyn TypeDatabase,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum PrimitiveClass {
    String,
    Number,
    Boolean,
    Bigint,
    Symbol,
    Null,
    Undefined,
}

impl<'a> BinaryOpEvaluator<'a> {
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        BinaryOpEvaluator { interner }
    }

    /// Check if a type is number-like (number, number literal, numeric enum, or any).
    ///
    /// This is used for type inference in arithmetic expressions and overloaded operators.
    /// A type is considered number-like if it is:
    /// - The `number` intrinsic type
    /// - A number literal (e.g., `42`, `3.14`)
    /// - A union of number literals (numeric enum type)
    /// - The `any` type (accepts all)
    ///
    /// ## Examples:
    /// - `number` ✅
    /// - `42` ✅ (number literal)
    /// - `1 | 2 | 3` ✅ (numeric enum)
    /// - `any` ✅
    /// - `string` ❌
    /// - `boolean` ❌
    fn is_number_like(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::NUMBER || type_id == TypeId::ANY {
            return true;
        }
        if let Some(key) = self.interner.lookup(type_id) {
            match key {
                TypeKey::Literal(LiteralValue::Number(_)) => return true,
                // Enum types are represented as unions of number literals
                TypeKey::Union(list_id) => {
                    let members = self.interner.type_list(list_id);
                    // An empty union is not number-like
                    if members.is_empty() {
                        return false;
                    }
                    // Check if all members are number-like (numeric enum)
                    return members.iter().all(|&m| self.is_number_like(m));
                }
                // Ref types (enum references) need to be resolved
                TypeKey::Ref(_) => {
                    // Enum refs are not directly number-like without resolution
                    // The checker handles this at a higher level
                    return false;
                }
                _ => {}
            }
        }
        false
    }

    /// Check if a type is string-like (string, string literal, template literal, or any).
    ///
    /// This is used for type inference in string operations and overload resolution.
    /// A type is considered string-like if it is:
    /// - The `string` intrinsic type
    /// - A string literal (e.g., `"hello"`)
    /// - A template literal type (e.g., `` `hello${world}` ``)
    /// - The `any` type (accepts all)
    ///
    /// ## Examples:
    /// - `string` ✅
    /// - `"hello"` ✅ (string literal)
    /// - `` `foo${bar}` `` ✅ (template literal)
    /// - `any` ✅
    /// - `number` ❌
    /// - `42` ❌
    fn is_string_like(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::STRING || type_id == TypeId::ANY {
            return true;
        }
        if let Some(key) = self.interner.lookup(type_id) {
            match key {
                TypeKey::Literal(LiteralValue::String(_)) => return true,
                TypeKey::TemplateLiteral(_) => return true,
                _ => {}
            }
        }
        false
    }

    /// Check if a type is bigint-like (bigint, bigint literal, bigint enum, or any).
    ///
    /// This is used for type inference in bigint arithmetic operations.
    /// A type is considered bigint-like if it is:
    /// - The `bigint` intrinsic type
    /// - A bigint literal (e.g., `42n`)
    /// - A union of bigint literals (bigint enum type)
    /// - The `any` type (accepts all)
    ///
    /// ## Examples:
    /// - `bigint` ✅
    /// - `42n` ✅ (bigint literal)
    /// - `1n | 2n | 3n` ✅ (bigint enum)
    /// - `any` ✅
    /// - `number` ❌
    /// - `42` ❌ (number literal)
    fn is_bigint_like(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::BIGINT || type_id == TypeId::ANY {
            return true;
        }
        if let Some(key) = self.interner.lookup(type_id) {
            match key {
                TypeKey::Literal(LiteralValue::BigInt(_)) => return true,
                // Enum types can also be bigint-based (though rare)
                TypeKey::Union(list_id) => {
                    let members = self.interner.type_list(list_id);
                    if members.is_empty() {
                        return false;
                    }
                    // Check if all members are bigint-like (bigint enum)
                    return members.iter().all(|&m| self.is_bigint_like(m));
                }
                _ => {}
            }
        }
        false
    }

    /// Check if a type is valid for arithmetic operations (number, bigint, enum, or any)
    /// This is used for TS2362/TS2363 error checking.
    pub fn is_arithmetic_operand(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::ANY {
            return true;
        }
        self.is_number_like(type_id) || self.is_bigint_like(type_id)
    }

    /// Evaluate a binary operation: left op right -> result
    pub fn evaluate(&self, left: TypeId, right: TypeId, op: &'static str) -> BinaryOpResult {
        match op {
            "+" => self.evaluate_plus(left, right),
            "-" | "*" | "/" | "%" => self.evaluate_arithmetic(left, right, op),
            "==" | "!=" | "===" | "!==" => {
                if self.has_overlap(left, right) {
                    BinaryOpResult::Success(TypeId::BOOLEAN)
                } else {
                    BinaryOpResult::TypeError { left, right, op }
                }
            }
            "<" | ">" | "<=" | ">=" => self.evaluate_comparison(left, right),
            "&&" | "||" => self.evaluate_logical(left, right),
            _ => BinaryOpResult::TypeError { left, right, op },
        }
    }

    fn evaluate_plus(&self, left: TypeId, right: TypeId) -> BinaryOpResult {
        // any + anything = any (and vice versa)
        if left == TypeId::ANY || right == TypeId::ANY {
            return BinaryOpResult::Success(TypeId::ANY);
        }

        // string-like + anything = string (and vice versa)
        if self.is_string_like(left) || self.is_string_like(right) {
            return BinaryOpResult::Success(TypeId::STRING);
        }

        // number-like + number-like = number
        if self.is_number_like(left) && self.is_number_like(right) {
            return BinaryOpResult::Success(TypeId::NUMBER);
        }

        // bigint-like + bigint-like = bigint
        if self.is_bigint_like(left) && self.is_bigint_like(right) {
            return BinaryOpResult::Success(TypeId::BIGINT);
        }

        BinaryOpResult::TypeError {
            left,
            right,
            op: "+",
        }
    }

    fn evaluate_arithmetic(&self, left: TypeId, right: TypeId, op: &'static str) -> BinaryOpResult {
        // any allows all operations
        if left == TypeId::ANY || right == TypeId::ANY {
            return BinaryOpResult::Success(TypeId::NUMBER);
        }

        // number-like * number-like = number
        if self.is_number_like(left) && self.is_number_like(right) {
            return BinaryOpResult::Success(TypeId::NUMBER);
        }

        // bigint-like * bigint-like = bigint
        if self.is_bigint_like(left) && self.is_bigint_like(right) {
            return BinaryOpResult::Success(TypeId::BIGINT);
        }

        BinaryOpResult::TypeError { left, right, op }
    }

    fn evaluate_comparison(&self, _left: TypeId, _right: TypeId) -> BinaryOpResult {
        BinaryOpResult::Success(TypeId::BOOLEAN)
    }

    fn evaluate_logical(&self, left: TypeId, right: TypeId) -> BinaryOpResult {
        // For && and ||, TypeScript returns a union of the two types
        BinaryOpResult::Success(self.interner.union2(left, right))
    }

    fn has_overlap(&self, left: TypeId, right: TypeId) -> bool {
        if left == right {
            return true;
        }
        if left == TypeId::ANY
            || right == TypeId::ANY
            || left == TypeId::UNKNOWN
            || right == TypeId::UNKNOWN
            || left == TypeId::ERROR
            || right == TypeId::ERROR
        {
            return true;
        }
        if left == TypeId::NEVER || right == TypeId::NEVER {
            return false;
        }

        if let Some(TypeKey::TypeParameter(info) | TypeKey::Infer(info)) =
            self.interner.lookup(left)
        {
            if let Some(constraint) = info.constraint {
                return self.has_overlap(constraint, right);
            }
            return true;
        }

        if let Some(TypeKey::TypeParameter(info) | TypeKey::Infer(info)) =
            self.interner.lookup(right)
        {
            if let Some(constraint) = info.constraint {
                return self.has_overlap(left, constraint);
            }
            return true;
        }

        if let Some(TypeKey::Union(members)) = self.interner.lookup(left) {
            let members = self.interner.type_list(members);
            return members
                .iter()
                .any(|member| self.has_overlap(*member, right));
        }
        if let Some(TypeKey::Union(members)) = self.interner.lookup(right) {
            let members = self.interner.type_list(members);
            return members.iter().any(|member| self.has_overlap(left, *member));
        }

        if let (Some(TypeKey::Literal(left_lit)), Some(TypeKey::Literal(right_lit))) =
            (self.interner.lookup(left), self.interner.lookup(right))
        {
            return left_lit == right_lit;
        }

        if self.primitive_classes_disjoint(left, right) {
            return false;
        }

        if self.interner.intersection2(left, right) == TypeId::NEVER {
            return false;
        }

        true
    }

    fn primitive_classes_disjoint(&self, left: TypeId, right: TypeId) -> bool {
        match (self.primitive_class(left), self.primitive_class(right)) {
            (Some(left_class), Some(right_class)) => left_class != right_class,
            _ => false,
        }
    }

    fn primitive_class(&self, type_id: TypeId) -> Option<PrimitiveClass> {
        let key = self.interner.lookup(type_id)?;
        match key {
            TypeKey::Intrinsic(kind) => match kind {
                IntrinsicKind::String => Some(PrimitiveClass::String),
                IntrinsicKind::Number => Some(PrimitiveClass::Number),
                IntrinsicKind::Boolean => Some(PrimitiveClass::Boolean),
                IntrinsicKind::Bigint => Some(PrimitiveClass::Bigint),
                IntrinsicKind::Symbol => Some(PrimitiveClass::Symbol),
                IntrinsicKind::Null => Some(PrimitiveClass::Null),
                IntrinsicKind::Undefined | IntrinsicKind::Void => Some(PrimitiveClass::Undefined),
                _ => None,
            },
            TypeKey::Literal(literal) => match literal {
                LiteralValue::String(_) => Some(PrimitiveClass::String),
                LiteralValue::Number(_) => Some(PrimitiveClass::Number),
                LiteralValue::Boolean(_) => Some(PrimitiveClass::Boolean),
                LiteralValue::BigInt(_) => Some(PrimitiveClass::Bigint),
            },
            TypeKey::TemplateLiteral(_) => Some(PrimitiveClass::String),
            TypeKey::UniqueSymbol(_) => Some(PrimitiveClass::Symbol),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "operations_tests.rs"]
mod tests;
