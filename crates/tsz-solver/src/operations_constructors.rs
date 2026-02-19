//! Constructor (new) expression resolution.
//!
//! This module handles `new` expressions, resolving construct signatures
//! for classes and interfaces. It mirrors the function call resolution
//! but with construct-specific semantics (e.g., union strictness, mixin pattern).

use crate::operations::{AssignabilityChecker, CallEvaluator, CallResult};
use crate::types::{CallableShape, FunctionShape, TypeData, TypeId, TypeListId};

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    /// Resolve a `new` expression (constructor call).
    ///
    /// This mirrors `resolve_call` but handles construct signatures instead of call signatures.
    /// Key differences from function calls:
    /// - Uses `construct_signatures` instead of `call_signatures`
    /// - For unions: ALL members must be constructable (stricter than function calls)
    /// - For intersections: Returns intersection of instance types (Mixin pattern)
    pub fn resolve_new(&mut self, type_id: TypeId, arg_types: &[TypeId]) -> CallResult {
        let key = match self.interner.lookup(type_id) {
            Some(k) => k,
            None => return CallResult::NotCallable { type_id },
        };

        match key {
            TypeData::Function(f_id) => {
                let shape = self.interner.function_shape(f_id);
                if shape.is_constructor {
                    self.resolve_function_call(shape.as_ref(), arg_types)
                } else {
                    // In TypeScript, `new f()` on a regular function (not arrow)
                    // is allowed and returns `any`. This matches the JS semantics
                    // where any function can be used as a constructor.
                    CallResult::Success(TypeId::ANY)
                }
            }
            TypeData::Callable(c_id) => {
                let shape = self.interner.callable_shape(c_id);
                self.resolve_callable_new(shape.as_ref(), arg_types)
            }
            TypeData::Union(list_id) => self.resolve_union_new(type_id, list_id, arg_types),
            TypeData::Intersection(list_id) => {
                self.resolve_intersection_new(type_id, list_id, arg_types)
            }
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.resolve_new(app.base, arg_types)
            }
            TypeData::TypeParameter(param_info) => {
                if let Some(constraint) = param_info.constraint {
                    self.resolve_new(constraint, arg_types)
                } else {
                    CallResult::NotCallable { type_id }
                }
            }
            TypeData::Lazy(_)
            | TypeData::Conditional(_)
            | TypeData::IndexAccess(_, _)
            | TypeData::Mapped(_)
            | TypeData::TemplateLiteral(_) => {
                // Resolve meta-types to their actual types before checking constructability.
                let resolved = crate::evaluate::evaluate_type(self.interner, type_id);
                if resolved != type_id {
                    self.resolve_new(resolved, arg_types)
                } else {
                    CallResult::NotCallable { type_id }
                }
            }
            _ => CallResult::NotCallable { type_id },
        }
    }

    /// Resolve a `new` expression on a Callable type.
    ///
    /// This handles classes and interfaces with construct signatures.
    fn resolve_callable_new(&mut self, shape: &CallableShape, arg_types: &[TypeId]) -> CallResult {
        if shape.construct_signatures.is_empty() {
            // If there are call signatures but no construct signatures (e.g. a method
            // accessed as a property), TypeScript allows `new` and returns `any`,
            // matching JS semantics where any function can be used as a constructor.
            if !shape.call_signatures.is_empty() {
                return CallResult::Success(TypeId::ANY);
            }
            return CallResult::NotCallable {
                type_id: self.interner.callable(shape.clone()),
            };
        }

        if shape.construct_signatures.len() == 1 {
            let sig = &shape.construct_signatures[0];
            let func = FunctionShape {
                params: sig.params.clone(),
                this_type: sig.this_type,
                return_type: sig.return_type,
                type_params: sig.type_params.clone(),
                type_predicate: sig.type_predicate.clone(),
                is_constructor: true,
                is_method: false,
            };
            return self.resolve_function_call(&func, arg_types);
        }

        // Handle overloads (similar to resolve_callable_call)
        let mut failures = Vec::new();
        let mut all_arg_count_mismatches = true;
        let mut min_expected = usize::MAX;
        let mut max_expected = 0;
        let mut any_has_rest = false;
        let actual_count = arg_types.len();
        // Track single count-compatible overload that fails on types (see resolve_callable_call).
        let mut type_mismatch_count: usize = 0;
        let mut sole_type_mismatch: Option<(usize, TypeId, TypeId)> = None;
        let mut has_non_count_non_type_failure = false;

        for sig in &shape.construct_signatures {
            let func = FunctionShape {
                params: sig.params.clone(),
                this_type: sig.this_type,
                return_type: sig.return_type,
                type_params: sig.type_params.clone(),
                type_predicate: sig.type_predicate.clone(),
                is_constructor: true,
                is_method: false,
            };

            match self.resolve_function_call(&func, arg_types) {
                CallResult::Success(ret) => return CallResult::Success(ret),
                CallResult::TypeParameterConstraintViolation { return_type, .. } => {
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

        if all_arg_count_mismatches && !failures.is_empty() {
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

        // Same "best candidate" heuristic as resolve_callable_call.
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

        CallResult::NoOverloadMatch {
            func_type: self.interner.callable(shape.clone()),
            arg_types: arg_types.to_vec(),
            failures,
        }
    }

    /// Resolve a `new` expression on a union type.
    ///
    /// For unions, ALL members must be constructable (stricter than function calls).
    /// If all members succeed, returns a union of their instance types.
    ///
    /// Example: `typeof A | typeof B` where both A and B are concrete classes
    /// - `new (typeof A | typeof B)()` succeeds and returns `A | B`
    fn resolve_union_new(
        &mut self,
        union_type: TypeId,
        list_id: TypeListId,
        arg_types: &[TypeId],
    ) -> CallResult {
        let members = self.interner.type_list(list_id);
        let mut return_types = Vec::new();
        let mut failures = Vec::new();

        for &member in members.iter() {
            match self.resolve_new(member, arg_types) {
                CallResult::Success(ret) => {
                    return_types.push(ret);
                }
                CallResult::NotCallable { .. } => {
                    return CallResult::NotCallable {
                        type_id: union_type,
                    };
                }
                err => {
                    failures.push(err);
                }
            }
        }

        // If any members succeeded, return a union of their return types
        if !return_types.is_empty() {
            if return_types.len() == 1 {
                return CallResult::Success(return_types[0]);
            }
            // Return a union of all return types
            let union_result = self.interner.union(return_types);
            CallResult::Success(union_result)
        } else if !failures.is_empty() {
            // If no members succeeded, return the first failure
            failures.into_iter().next().unwrap()
        } else {
            CallResult::NotCallable {
                type_id: union_type,
            }
        }
    }

    /// Resolve a `new` expression on an intersection type.
    ///
    /// This handles the Mixin pattern: an intersection of constructors results in
    /// a constructor that returns the intersection of their instance types.
    ///
    /// e.g. `type Mixin = (new () => A) & (new () => B);`
    ///      `new Mixin()` -> `A & B`
    fn resolve_intersection_new(
        &mut self,
        intersection_type: TypeId,
        list_id: TypeListId,
        arg_types: &[TypeId],
    ) -> CallResult {
        let members = self.interner.type_list(list_id);
        let mut return_types = Vec::new();
        let mut failures = Vec::new();

        for &member in members.iter() {
            // Try to resolve new on each member
            match self.resolve_new(member, arg_types) {
                CallResult::Success(ret) => {
                    return_types.push(ret);
                }
                CallResult::NotCallable { .. } => {
                    // Ignore non-constructable members in an intersection
                    // (e.g. Constructor & { staticProp: number })
                    continue;
                }
                err => {
                    // If it IS constructable but failed (e.g. arg mismatch), record it
                    failures.push(err);
                }
            }
        }

        if !return_types.is_empty() {
            if return_types.len() == 1 {
                return CallResult::Success(return_types[0]);
            }
            // Return intersection of all instance types (Mixin pattern)
            let intersection_result = self.interner.intersection(return_types);
            CallResult::Success(intersection_result)
        } else if !failures.is_empty() {
            // If we found constructors but they failed matching args, return the failure
            failures.into_iter().next().unwrap()
        } else {
            // No constructable members found
            CallResult::NotCallable {
                type_id: intersection_type,
            }
        }
    }
}
