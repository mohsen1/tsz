//! Conditional type evaluation.
//!
//! Handles TypeScript's conditional types: `T extends U ? X : Y`
//! Including distributive conditional types over union types.

use crate::instantiate::{TypeSubstitution, instantiate_type_with_infer};
use crate::subtype::{SubtypeChecker, TypeResolver};
use crate::types::*;
use rustc_hash::{FxHashMap, FxHashSet};

use super::super::evaluate::TypeEvaluator;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Maximum depth for tail-recursive conditional evaluation.
    /// This allows patterns like `type Loop<T> = T extends [...infer R] ? Loop<R> : never`
    /// to work with up to 1000 recursive calls instead of being limited to MAX_EVALUATE_DEPTH.
    const MAX_TAIL_RECURSION_DEPTH: usize = 1000;

    /// Evaluate a conditional type: T extends U ? X : Y
    ///
    /// Algorithm:
    /// 1. If check_type is a union and the conditional is distributive, distribute
    /// 2. Otherwise, check if check_type <: extends_type
    /// 3. If true -> return true_type
    /// 4. If false (disjoint) -> return false_type
    /// 5. If ambiguous (unresolved type param) -> return deferred conditional
    ///
    /// ## Tail-Recursion Elimination
    /// If the chosen branch (true/false) evaluates to another ConditionalType,
    /// we immediately evaluate it in the current stack frame instead of recursing.
    /// This allows tail-recursive patterns to work with up to MAX_TAIL_RECURSION_DEPTH
    /// iterations instead of being limited by MAX_EVALUATE_DEPTH.
    pub fn evaluate_conditional(&mut self, initial_cond: &ConditionalType) -> TypeId {
        // Setup loop state for tail-recursion elimination
        let mut current_cond = initial_cond.clone();
        let mut tail_recursion_count = 0;

        loop {
            let cond = &current_cond;
            let check_type = self.evaluate(cond.check_type);
            let extends_type = self.evaluate(cond.extends_type);

            if cond.is_distributive && check_type == TypeId::NEVER {
                return TypeId::NEVER;
            }

            if check_type == TypeId::ANY {
                // For distributive `any extends X ? T : F`:
                // - Distributive: return union of both branches (any distributes over the conditional)
                // - Non-distributive: return union of both branches (any poisons the result)
                // In both cases, we evaluate and union the branches to handle infer types correctly
                let true_eval = self.evaluate(cond.true_type);
                let false_eval = self.evaluate(cond.false_type);
                return self.interner().union2(true_eval, false_eval);
            }

            // Step 1: Check for distributivity
            // Only distribute for naked type parameters (recorded at lowering time).
            if cond.is_distributive
                && let Some(TypeKey::Union(members)) = self.interner().lookup(check_type)
            {
                let members = self.interner().type_list(members);
                return self.distribute_conditional(
                    members.as_ref(),
                    check_type, // Pass original check_type for substitution
                    extends_type,
                    cond.true_type,
                    cond.false_type,
                );
            }

            if let Some(TypeKey::Infer(info)) = self.interner().lookup(extends_type) {
                if matches!(
                    self.interner().lookup(check_type),
                    Some(TypeKey::TypeParameter(_)) | Some(TypeKey::Infer(_))
                ) {
                    return self.interner().conditional(cond.clone());
                }

                if check_type == TypeId::ANY {
                    let mut subst = TypeSubstitution::new();
                    subst.insert(info.name, check_type);
                    let true_eval = self.evaluate(instantiate_type_with_infer(
                        self.interner(),
                        cond.true_type,
                        &subst,
                    ));
                    let false_eval = self.evaluate(instantiate_type_with_infer(
                        self.interner(),
                        cond.false_type,
                        &subst,
                    ));
                    return self.interner().union2(true_eval, false_eval);
                }

                let mut subst = TypeSubstitution::new();
                subst.insert(info.name, check_type);
                let mut inferred = check_type;
                if let Some(constraint) = info.constraint {
                    let mut checker =
                        SubtypeChecker::with_resolver(self.interner(), self.resolver());
                    checker.allow_bivariant_rest = true;
                    let Some(filtered) =
                        self.filter_inferred_by_constraint(inferred, constraint, &mut checker)
                    else {
                        let false_inst =
                            instantiate_type_with_infer(self.interner(), cond.false_type, &subst);
                        return self.evaluate(false_inst);
                    };
                    inferred = filtered;
                }

                subst.insert(info.name, inferred);

                let true_inst =
                    instantiate_type_with_infer(self.interner(), cond.true_type, &subst);
                return self.evaluate(true_inst);
            }

            let extends_unwrapped = match self.interner().lookup(extends_type) {
                Some(TypeKey::ReadonlyType(inner)) => inner,
                _ => extends_type,
            };
            let check_unwrapped = match self.interner().lookup(check_type) {
                Some(TypeKey::ReadonlyType(inner)) => inner,
                _ => check_type,
            };

            // Handle array extends pattern with infer
            if let Some(TypeKey::Array(ext_elem)) = self.interner().lookup(extends_unwrapped)
                && let Some(TypeKey::Infer(info)) = self.interner().lookup(ext_elem)
            {
                return self.eval_conditional_array_infer(cond, check_unwrapped, info);
            }

            // Handle tuple extends pattern with infer
            if let Some(TypeKey::Tuple(extends_elements)) =
                self.interner().lookup(extends_unwrapped)
            {
                let extends_elements = self.interner().tuple_list(extends_elements);
                if extends_elements.len() == 1
                    && !extends_elements[0].rest
                    && let Some(TypeKey::Infer(info)) =
                        self.interner().lookup(extends_elements[0].type_id)
                {
                    return self.eval_conditional_tuple_infer(
                        cond,
                        check_unwrapped,
                        &extends_elements[0],
                        info,
                    );
                }
            }

            // Handle object extends pattern with infer
            if let Some(extends_shape_id) = match self.interner().lookup(extends_unwrapped) {
                Some(TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id)) => {
                    Some(shape_id)
                }
                _ => None,
            } {
                if let Some(result) =
                    self.eval_conditional_object_infer(cond, check_unwrapped, extends_shape_id)
                {
                    return result;
                }
            }

            // Step 2: Check for naked type parameter
            if let Some(TypeKey::TypeParameter(param)) = self.interner().lookup(check_type) {
                // If extends_type contains infer patterns and the type parameter has a constraint,
                // try to infer from the constraint. This handles cases like:
                // R extends Reducer<infer S, any> ? S : never
                // where R is constrained to Reducer<any, any>
                if self.type_contains_infer(extends_type)
                    && let Some(constraint) = param.constraint
                {
                    let mut checker =
                        SubtypeChecker::with_resolver(self.interner(), self.resolver());
                    checker.allow_bivariant_rest = true;
                    let mut bindings = FxHashMap::default();
                    let mut visited = FxHashSet::default();
                    if self.match_infer_pattern(
                        constraint,
                        extends_type,
                        &mut bindings,
                        &mut visited,
                        &mut checker,
                    ) {
                        let substituted_true = self.substitute_infer(cond.true_type, &bindings);
                        return self.evaluate(substituted_true);
                    }
                }
                // Type parameter hasn't been substituted - defer evaluation
                return self.interner().conditional(cond.clone());
            }

            // Step 3: Perform subtype check or infer pattern matching
            let mut checker = SubtypeChecker::with_resolver(self.interner(), self.resolver());
            checker.allow_bivariant_rest = true;

            if self.type_contains_infer(extends_type) {
                let mut bindings = FxHashMap::default();
                let mut visited = FxHashSet::default();
                if self.match_infer_pattern(
                    check_type,
                    extends_type,
                    &mut bindings,
                    &mut visited,
                    &mut checker,
                ) {
                    let substituted_true = self.substitute_infer(cond.true_type, &bindings);
                    return self.evaluate(substituted_true);
                }

                // Check if the result branch is directly a conditional for tail-recursion
                // IMPORTANT: Check BEFORE calling evaluate to avoid incrementing depth
                if tail_recursion_count < Self::MAX_TAIL_RECURSION_DEPTH {
                    if let Some(TypeKey::Conditional(next_cond_id)) =
                        self.interner().lookup(cond.false_type)
                    {
                        let next_cond = self.interner().conditional_type(next_cond_id);
                        current_cond = (*next_cond).clone();
                        tail_recursion_count += 1;
                        continue;
                    }
                }

                // Not a tail-recursive case - evaluate normally
                return self.evaluate(cond.false_type);
            }

            // Subtype check path
            let result_branch = if checker.is_subtype_of(check_type, extends_type) {
                // T <: U -> true branch
                cond.true_type
            } else {
                // Check if types are definitely disjoint
                // For now, we use a simple heuristic: if not subtype, assume disjoint
                // More sophisticated: check if intersection is never
                cond.false_type
            };

            // Check if the result branch is directly a conditional for tail-recursion
            // IMPORTANT: Check BEFORE calling evaluate to avoid incrementing depth
            if tail_recursion_count < Self::MAX_TAIL_RECURSION_DEPTH {
                if let Some(TypeKey::Conditional(next_cond_id)) =
                    self.interner().lookup(result_branch)
                {
                    let next_cond = self.interner().conditional_type(next_cond_id);
                    current_cond = (*next_cond).clone();
                    tail_recursion_count += 1;
                    continue;
                }
            }

            // Not a tail-recursive case - evaluate normally
            return self.evaluate(result_branch);
        }
    }

    /// Distribute a conditional type over a union.
    /// (A | B) extends U ? X : Y -> (A extends U ? X : Y) | (B extends U ? X : Y)
    pub(crate) fn distribute_conditional(
        &mut self,
        members: &[TypeId],
        original_check_type: TypeId,
        extends_type: TypeId,
        true_type: TypeId,
        false_type: TypeId,
    ) -> TypeId {
        // Limit distribution to prevent OOM with large unions
        const MAX_DISTRIBUTION_SIZE: usize = 100;
        if members.len() > MAX_DISTRIBUTION_SIZE {
            self.set_depth_exceeded(true);
            return TypeId::ERROR;
        }

        let mut results: Vec<TypeId> = Vec::with_capacity(members.len());

        for &member in members {
            // Check if depth was exceeded during previous iterations
            if self.is_depth_exceeded() {
                return TypeId::ERROR;
            }

            // Substitute the specific member if true_type or false_type references the original check_type
            // This handles cases like: NonNullable<T> = T extends null ? never : T
            // When T = A | B, we need (A extends null ? never : A) | (B extends null ? never : B)
            let substituted_true_type = if true_type == original_check_type {
                member
            } else {
                true_type
            };
            let substituted_false_type = if false_type == original_check_type {
                member
            } else {
                false_type
            };

            // Create conditional for this union member
            let member_cond = ConditionalType {
                check_type: member,
                extends_type,
                true_type: substituted_true_type,
                false_type: substituted_false_type,
                is_distributive: false,
            };

            // Recursively evaluate via evaluate() to respect depth limits
            let cond_type = self.interner().conditional(member_cond);
            let result = self.evaluate(cond_type);
            // Check if evaluation hit depth limit
            if result == TypeId::ERROR && self.is_depth_exceeded() {
                return TypeId::ERROR;
            }
            results.push(result);
        }

        // Combine results into a union
        self.interner().union(results)
    }

    /// Handle array extends pattern: T extends (infer U)[] ? ...
    fn eval_conditional_array_infer(
        &mut self,
        cond: &ConditionalType,
        check_unwrapped: TypeId,
        info: TypeParamInfo,
    ) -> TypeId {
        if matches!(
            self.interner().lookup(check_unwrapped),
            Some(TypeKey::TypeParameter(_)) | Some(TypeKey::Infer(_))
        ) {
            return self.interner().conditional(cond.clone());
        }

        let inferred = match self.interner().lookup(check_unwrapped) {
            Some(TypeKey::Array(elem)) => Some(elem),
            Some(TypeKey::Tuple(elements)) => {
                let elements = self.interner().tuple_list(elements);
                let mut parts = Vec::new();
                for element in elements.iter() {
                    if element.rest {
                        let rest_type = self.rest_element_type(element.type_id);
                        parts.push(rest_type);
                    } else {
                        let elem_type = if element.optional {
                            self.interner().union2(element.type_id, TypeId::UNDEFINED)
                        } else {
                            element.type_id
                        };
                        parts.push(elem_type);
                    }
                }
                if parts.is_empty() {
                    None
                } else {
                    Some(self.interner().union(parts))
                }
            }
            Some(TypeKey::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut parts = Vec::new();
                for &member in members.iter() {
                    match self.interner().lookup(member) {
                        Some(TypeKey::Array(elem)) => parts.push(elem),
                        Some(TypeKey::ReadonlyType(inner)) => {
                            let Some(TypeKey::Array(elem)) = self.interner().lookup(inner) else {
                                return self.evaluate(cond.false_type);
                            };
                            parts.push(elem);
                        }
                        _ => return self.evaluate(cond.false_type),
                    }
                }
                if parts.is_empty() {
                    None
                } else if parts.len() == 1 {
                    Some(parts[0])
                } else {
                    Some(self.interner().union(parts))
                }
            }
            _ => None,
        };

        let Some(mut inferred) = inferred else {
            return self.evaluate(cond.false_type);
        };

        let mut subst = TypeSubstitution::new();
        subst.insert(info.name, inferred);

        if let Some(constraint) = info.constraint {
            let mut checker = SubtypeChecker::with_resolver(self.interner(), self.resolver());
            checker.allow_bivariant_rest = true;
            let is_union = matches!(self.interner().lookup(inferred), Some(TypeKey::Union(_)));
            if is_union && !cond.is_distributive {
                // For unions in non-distributive conditionals, use filter that adds undefined
                inferred = self.filter_inferred_by_constraint_or_undefined(
                    inferred,
                    constraint,
                    &mut checker,
                );
            } else {
                // For single values or distributive conditionals, fail if constraint doesn't match
                if !checker.is_subtype_of(inferred, constraint) {
                    return self.evaluate(cond.false_type);
                }
            }
            subst.insert(info.name, inferred);
        }

        let true_inst = instantiate_type_with_infer(self.interner(), cond.true_type, &subst);
        self.evaluate(true_inst)
    }

    /// Handle tuple extends pattern: T extends [infer U] ? ...
    fn eval_conditional_tuple_infer(
        &mut self,
        cond: &ConditionalType,
        check_unwrapped: TypeId,
        extends_elem: &TupleElement,
        info: TypeParamInfo,
    ) -> TypeId {
        if matches!(
            self.interner().lookup(check_unwrapped),
            Some(TypeKey::TypeParameter(_)) | Some(TypeKey::Infer(_))
        ) {
            return self.interner().conditional(cond.clone());
        }

        let inferred = match self.interner().lookup(check_unwrapped) {
            Some(TypeKey::Tuple(check_elements)) => {
                let check_elements = self.interner().tuple_list(check_elements);
                if check_elements.is_empty() {
                    extends_elem.optional.then_some(TypeId::UNDEFINED)
                } else if check_elements.len() == 1 && !check_elements[0].rest {
                    let elem = &check_elements[0];
                    Some(if elem.optional {
                        self.interner().union2(elem.type_id, TypeId::UNDEFINED)
                    } else {
                        elem.type_id
                    })
                } else {
                    None
                }
            }
            Some(TypeKey::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut inferred_members = Vec::new();
                for &member in members.iter() {
                    let member_type = match self.interner().lookup(member) {
                        Some(TypeKey::ReadonlyType(inner)) => inner,
                        _ => member,
                    };
                    match self.interner().lookup(member_type) {
                        Some(TypeKey::Tuple(check_elements)) => {
                            let check_elements = self.interner().tuple_list(check_elements);
                            if check_elements.is_empty() {
                                if extends_elem.optional {
                                    inferred_members.push(TypeId::UNDEFINED);
                                    continue;
                                }
                                return self.evaluate(cond.false_type);
                            }
                            if check_elements.len() == 1 && !check_elements[0].rest {
                                let elem = &check_elements[0];
                                let elem_type = if elem.optional {
                                    self.interner().union2(elem.type_id, TypeId::UNDEFINED)
                                } else {
                                    elem.type_id
                                };
                                inferred_members.push(elem_type);
                            } else {
                                return self.evaluate(cond.false_type);
                            }
                        }
                        _ => return self.evaluate(cond.false_type),
                    }
                }
                if inferred_members.is_empty() {
                    None
                } else if inferred_members.len() == 1 {
                    Some(inferred_members[0])
                } else {
                    Some(self.interner().union(inferred_members))
                }
            }
            _ => None,
        };

        let Some(mut inferred) = inferred else {
            return self.evaluate(cond.false_type);
        };

        let mut subst = TypeSubstitution::new();
        subst.insert(info.name, inferred);

        if let Some(constraint) = info.constraint {
            let mut checker = SubtypeChecker::with_resolver(self.interner(), self.resolver());
            checker.allow_bivariant_rest = true;
            let Some(filtered) =
                self.filter_inferred_by_constraint(inferred, constraint, &mut checker)
            else {
                let false_inst =
                    instantiate_type_with_infer(self.interner(), cond.false_type, &subst);
                return self.evaluate(false_inst);
            };
            inferred = filtered;
            subst.insert(info.name, inferred);
        }

        let true_inst = instantiate_type_with_infer(self.interner(), cond.true_type, &subst);
        self.evaluate(true_inst)
    }

    /// Handle object extends pattern: T extends { prop: infer U } ? ...
    fn eval_conditional_object_infer(
        &mut self,
        cond: &ConditionalType,
        check_unwrapped: TypeId,
        extends_shape_id: ObjectShapeId,
    ) -> Option<TypeId> {
        let extends_shape = self.interner().object_shape(extends_shape_id);
        let mut infer_prop = None;
        let mut infer_nested = None;

        for prop in extends_shape.properties.iter() {
            if let Some(TypeKey::Infer(info)) = self.interner().lookup(prop.type_id) {
                if infer_prop.is_some() || infer_nested.is_some() {
                    return None;
                }
                infer_prop = Some((prop.name, info, prop.optional));
                continue;
            }

            let nested_type = match self.interner().lookup(prop.type_id) {
                Some(TypeKey::ReadonlyType(inner)) => inner,
                _ => prop.type_id,
            };
            if let Some(nested_shape_id) = match self.interner().lookup(nested_type) {
                Some(TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id)) => {
                    Some(shape_id)
                }
                _ => None,
            } {
                let nested_shape = self.interner().object_shape(nested_shape_id);
                let mut nested_infer = None;
                for nested_prop in nested_shape.properties.iter() {
                    if let Some(TypeKey::Infer(info)) = self.interner().lookup(nested_prop.type_id)
                    {
                        if nested_infer.is_some() {
                            nested_infer = None;
                            break;
                        }
                        nested_infer = Some((nested_prop.name, info));
                    }
                }
                if let Some((nested_name, info)) = nested_infer {
                    if infer_prop.is_some() || infer_nested.is_some() {
                        return None;
                    }
                    infer_nested = Some((prop.name, nested_name, info));
                }
            }
        }

        if let Some((prop_name, info, prop_optional)) = infer_prop {
            return Some(self.eval_conditional_object_prop_infer(
                cond,
                check_unwrapped,
                prop_name,
                info,
                prop_optional,
            ));
        }

        if let Some((outer_name, inner_name, info)) = infer_nested {
            return Some(self.eval_conditional_object_nested_infer(
                cond,
                check_unwrapped,
                outer_name,
                inner_name,
                info,
            ));
        }

        None
    }

    /// Handle object property infer pattern
    fn eval_conditional_object_prop_infer(
        &mut self,
        cond: &ConditionalType,
        check_unwrapped: TypeId,
        prop_name: tsz_common::interner::Atom,
        info: TypeParamInfo,
        prop_optional: bool,
    ) -> TypeId {
        if matches!(
            self.interner().lookup(check_unwrapped),
            Some(TypeKey::TypeParameter(_)) | Some(TypeKey::Infer(_))
        ) {
            return self.interner().conditional(cond.clone());
        }

        let inferred = match self.interner().lookup(check_unwrapped) {
            Some(TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.interner().object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .find(|prop| prop.name == prop_name)
                    .map(|prop| {
                        if prop_optional {
                            self.optional_property_type(prop)
                        } else {
                            prop.type_id
                        }
                    })
                    .or_else(|| prop_optional.then_some(TypeId::UNDEFINED))
            }
            Some(TypeKey::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut inferred_members = Vec::new();
                for &member in members.iter() {
                    let member_unwrapped = match self.interner().lookup(member) {
                        Some(TypeKey::ReadonlyType(inner)) => inner,
                        _ => member,
                    };
                    match self.interner().lookup(member_unwrapped) {
                        Some(TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id)) => {
                            let shape = self.interner().object_shape(shape_id);
                            if let Some(prop) =
                                shape.properties.iter().find(|prop| prop.name == prop_name)
                            {
                                inferred_members.push(if prop_optional {
                                    self.optional_property_type(prop)
                                } else {
                                    prop.type_id
                                });
                            } else if prop_optional {
                                inferred_members.push(TypeId::UNDEFINED);
                            } else {
                                return self.evaluate(cond.false_type);
                            }
                        }
                        _ => return self.evaluate(cond.false_type),
                    }
                }
                if inferred_members.is_empty() {
                    None
                } else if inferred_members.len() == 1 {
                    Some(inferred_members[0])
                } else {
                    Some(self.interner().union(inferred_members))
                }
            }
            _ => None,
        };

        let Some(mut inferred) = inferred else {
            return self.evaluate(cond.false_type);
        };

        let mut subst = TypeSubstitution::new();
        subst.insert(info.name, inferred);

        if let Some(constraint) = info.constraint {
            let mut checker = SubtypeChecker::with_resolver(self.interner(), self.resolver());
            checker.allow_bivariant_rest = true;
            let is_union = matches!(self.interner().lookup(inferred), Some(TypeKey::Union(_)));
            if prop_optional {
                let Some(filtered) =
                    self.filter_inferred_by_constraint(inferred, constraint, &mut checker)
                else {
                    let false_inst =
                        instantiate_type_with_infer(self.interner(), cond.false_type, &subst);
                    return self.evaluate(false_inst);
                };
                inferred = filtered;
            } else if is_union || cond.is_distributive {
                // For unions or distributive conditionals, use filter that adds undefined
                inferred = self.filter_inferred_by_constraint_or_undefined(
                    inferred,
                    constraint,
                    &mut checker,
                );
            } else {
                // For non-distributive single values, fail if constraint doesn't match
                if !checker.is_subtype_of(inferred, constraint) {
                    return self.evaluate(cond.false_type);
                }
            }
            subst.insert(info.name, inferred);
        }

        let true_inst = instantiate_type_with_infer(self.interner(), cond.true_type, &subst);
        self.evaluate(true_inst)
    }

    /// Handle nested object infer pattern
    fn eval_conditional_object_nested_infer(
        &mut self,
        cond: &ConditionalType,
        check_unwrapped: TypeId,
        outer_name: tsz_common::interner::Atom,
        inner_name: tsz_common::interner::Atom,
        info: TypeParamInfo,
    ) -> TypeId {
        if matches!(
            self.interner().lookup(check_unwrapped),
            Some(TypeKey::TypeParameter(_)) | Some(TypeKey::Infer(_))
        ) {
            return self.interner().conditional(cond.clone());
        }

        let inferred = match self.interner().lookup(check_unwrapped) {
            Some(TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.interner().object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .find(|prop| prop.name == outer_name)
                    .and_then(|prop| {
                        let inner_type = match self.interner().lookup(prop.type_id) {
                            Some(TypeKey::ReadonlyType(inner)) => inner,
                            _ => prop.type_id,
                        };
                        match self.interner().lookup(inner_type) {
                            Some(
                                TypeKey::Object(inner_shape_id)
                                | TypeKey::ObjectWithIndex(inner_shape_id),
                            ) => {
                                let inner_shape = self.interner().object_shape(inner_shape_id);
                                inner_shape
                                    .properties
                                    .iter()
                                    .find(|prop| prop.name == inner_name)
                                    .map(|prop| prop.type_id)
                            }
                            _ => None,
                        }
                    })
            }
            Some(TypeKey::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut inferred_members = Vec::new();
                for &member in members.iter() {
                    let member_unwrapped = match self.interner().lookup(member) {
                        Some(TypeKey::ReadonlyType(inner)) => inner,
                        _ => member,
                    };
                    let Some(TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id)) =
                        self.interner().lookup(member_unwrapped)
                    else {
                        return self.evaluate(cond.false_type);
                    };
                    let shape = self.interner().object_shape(shape_id);
                    let Some(prop) = shape.properties.iter().find(|prop| prop.name == outer_name)
                    else {
                        return self.evaluate(cond.false_type);
                    };
                    let inner_type = match self.interner().lookup(prop.type_id) {
                        Some(TypeKey::ReadonlyType(inner)) => inner,
                        _ => prop.type_id,
                    };
                    let Some(
                        TypeKey::Object(inner_shape_id) | TypeKey::ObjectWithIndex(inner_shape_id),
                    ) = self.interner().lookup(inner_type)
                    else {
                        return self.evaluate(cond.false_type);
                    };
                    let inner_shape = self.interner().object_shape(inner_shape_id);
                    let Some(inner_prop) = inner_shape
                        .properties
                        .iter()
                        .find(|prop| prop.name == inner_name)
                    else {
                        return self.evaluate(cond.false_type);
                    };
                    inferred_members.push(inner_prop.type_id);
                }
                if inferred_members.is_empty() {
                    None
                } else if inferred_members.len() == 1 {
                    Some(inferred_members[0])
                } else {
                    Some(self.interner().union(inferred_members))
                }
            }
            _ => None,
        };

        let Some(mut inferred) = inferred else {
            return self.evaluate(cond.false_type);
        };

        let mut subst = TypeSubstitution::new();
        subst.insert(info.name, inferred);

        if let Some(constraint) = info.constraint {
            let mut checker = SubtypeChecker::with_resolver(self.interner(), self.resolver());
            checker.allow_bivariant_rest = true;
            let is_union = matches!(self.interner().lookup(inferred), Some(TypeKey::Union(_)));
            if is_union || cond.is_distributive {
                // For unions or distributive conditionals, use filter that adds undefined
                inferred = self.filter_inferred_by_constraint_or_undefined(
                    inferred,
                    constraint,
                    &mut checker,
                );
            } else {
                // For non-distributive single values, fail if constraint doesn't match
                if !checker.is_subtype_of(inferred, constraint) {
                    return self.evaluate(cond.false_type);
                }
            }
            subst.insert(info.name, inferred);
        }

        let true_inst = instantiate_type_with_infer(self.interner(), cond.true_type, &subst);
        self.evaluate(true_inst)
    }
}
