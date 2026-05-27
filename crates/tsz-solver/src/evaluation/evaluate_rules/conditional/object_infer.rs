//! Object conditional infer helpers.

use super::*;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Handle object extends pattern: T extends { prop: infer U } ? ...
    pub(super) fn eval_conditional_object_infer(
        &mut self,
        cond: &ConditionalType,
        check_unwrapped: TypeId,
        extends_shape_id: ObjectShapeId,
    ) -> Option<TypeId> {
        let extends_shape = self.interner().object_shape(extends_shape_id);
        let mut infer_props: SmallVec<[(Atom, TypeParamInfo, bool); 4]> = SmallVec::new();
        let mut infer_nested = None;

        for prop in &extends_shape.properties {
            if let Some(TypeData::Infer(info)) = self.interner().lookup(prop.type_id) {
                if infer_nested.is_some() {
                    return None;
                }
                infer_props.push((prop.name, info, prop.optional));
                continue;
            }

            let nested_type = match self.interner().lookup(prop.type_id) {
                Some(TypeData::ReadonlyType(inner)) => inner,
                _ => prop.type_id,
            };
            let mut captured = false;
            if let Some(nested_shape_id) = match self.interner().lookup(nested_type) {
                Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                    Some(shape_id)
                }
                _ => None,
            } {
                let nested_shape = self.interner().object_shape(nested_shape_id);
                let mut nested_infer = None;
                for nested_prop in &nested_shape.properties {
                    if let Some(TypeData::Infer(info)) = self.interner().lookup(nested_prop.type_id)
                    {
                        if nested_infer.is_some() {
                            nested_infer = None;
                            break;
                        }
                        nested_infer = Some((nested_prop.name, info));
                    }
                }
                if let Some((nested_name, info)) = nested_infer {
                    if !infer_props.is_empty() || infer_nested.is_some() {
                        return None;
                    }
                    infer_nested = Some((prop.name, nested_name, info));
                    captured = true;
                }
            }

            // A property that still carries an `infer` variable this fast path did
            // not record (e.g. inside a function parameter, an array, or a nested
            // object with multiple infers) cannot be modeled here. Defer to the
            // general `match_infer_pattern` engine, which handles every position
            // with variance-aware candidate merging.
            if !captured && self.type_contains_infer(prop.type_id) {
                return None;
            }
        }

        if infer_props.len() == 1 {
            let (prop_name, info, prop_optional) = infer_props[0];
            return Some(self.eval_conditional_object_prop_infer(
                cond,
                check_unwrapped,
                prop_name,
                info,
                prop_optional,
            ));
        }

        if infer_props.len() > 1 {
            return Some(self.eval_conditional_object_multi_prop_infer(
                cond,
                check_unwrapped,
                &infer_props,
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

    fn eval_conditional_object_multi_prop_infer(
        &mut self,
        cond: &ConditionalType,
        check_unwrapped: TypeId,
        infer_props: &[(Atom, TypeParamInfo, bool)],
    ) -> TypeId {
        let check_key = self.interner().lookup(check_unwrapped);
        if matches!(
            check_key,
            Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
        ) {
            return self.interner().conditional(*cond);
        }

        // Pass 1: resolve each property and accumulate inferred types per variable name.
        // Co-located uses of the same `infer T` union their candidates. Track which
        // variables received contributions from more than one slot, and the effective
        // constraint from the first occurrence that declares one.
        let mut accumulated: FxHashMap<Atom, TypeId> = FxHashMap::default();
        // Set when ≥2 distinct slots contributed to the same variable.
        let mut multi_slot: FxHashSet<Atom> = FxHashSet::default();
        // (constraint, optional) from the first constrained occurrence.
        let mut effective_constraint: FxHashMap<Atom, (TypeId, bool)> = FxHashMap::default();

        for &(prop_name, info, optional) in infer_props {
            let Some(inferred) =
                self.resolve_conditional_infer_property(check_unwrapped, prop_name, optional)
            else {
                return self.evaluate(cond.false_type);
            };

            if let Some(existing) = accumulated.get(&info.name).copied() {
                // tsc unions co-located infer candidates across property slots.
                multi_slot.insert(info.name);
                let merged = self.interner().union2(existing, inferred);
                accumulated.insert(info.name, merged);
            } else {
                accumulated.insert(info.name, inferred);
            }

            // The constraint declared at the first occurrence is the variable's constraint;
            // later co-located uses that lack a constraint are just additional candidates.
            if let Some(constraint) = info.constraint {
                effective_constraint
                    .entry(info.name)
                    .or_insert((constraint, optional));
            }
        }

        // Pass 2: apply each variable's effective constraint to its fully-accumulated type,
        // then build the substitution in declaration order.
        //
        // Constraint semantics differ by how the union arose:
        // - Multi-slot accumulation: the whole union must satisfy the constraint (tsc fails
        //   the conditional when `string | number extends string` is false).
        // - Single-slot with a union source property: filter per-member and keep matching
        //   parts (preserving the original `filter_inferred_by_constraint_or_undefined`
        //   behaviour for non-distributive unions).
        let mut subst = TypeSubstitution::new();
        for &(_, info, _) in infer_props {
            if subst.get(info.name).is_some() {
                continue; // already processed this variable
            }
            let Some(mut inferred) = accumulated.get(&info.name).copied() else {
                continue;
            };

            if let Some(&(constraint, opt)) = effective_constraint.get(&info.name) {
                let mut checker = self.conditional_subtype_checker();
                checker.allow_bivariant_rest = true;
                let is_union = matches!(self.interner().lookup(inferred), Some(TypeData::Union(_)));
                let is_multi = multi_slot.contains(&info.name);

                if opt {
                    let Some(filtered) =
                        self.filter_inferred_by_constraint(inferred, constraint, &mut checker)
                    else {
                        let false_inst =
                            instantiate_type_with_infer(self.interner(), cond.false_type, &subst);
                        return self.evaluate(false_inst);
                    };
                    inferred = filtered;
                } else if is_union && !cond.is_distributive && !is_multi {
                    // Union from a single source property — filter members, keep matching.
                    inferred = self.filter_inferred_by_constraint_or_undefined(
                        inferred,
                        constraint,
                        &mut checker,
                    );
                } else if !checker.is_subtype_of(inferred, constraint) {
                    return self.evaluate(cond.false_type);
                }
            }

            subst.insert(info.name, inferred);
        }

        let true_inst = instantiate_type_with_infer(self.interner(), cond.true_type, &subst);
        self.evaluate_preserving_tail_application_branch_alias(true_inst, Some(true_inst))
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
        let check_key = self.interner().lookup(check_unwrapped);
        if matches!(
            check_key,
            Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
        ) {
            return self.interner().conditional(*cond);
        }

        let inferred =
            self.resolve_conditional_infer_property(check_unwrapped, prop_name, prop_optional);

        let Some(mut inferred) = inferred else {
            return self.evaluate(cond.false_type);
        };

        let mut subst = TypeSubstitution::single(info.name, inferred);

        if let Some(constraint) = info.constraint {
            let mut checker = self.conditional_subtype_checker();
            checker.allow_bivariant_rest = true;
            let is_union = matches!(self.interner().lookup(inferred), Some(TypeData::Union(_)));
            if prop_optional {
                let Some(filtered) =
                    self.filter_inferred_by_constraint(inferred, constraint, &mut checker)
                else {
                    let false_inst =
                        instantiate_type_with_infer(self.interner(), cond.false_type, &subst);
                    return self.evaluate(false_inst);
                };
                inferred = filtered;
            } else if is_union && !cond.is_distributive {
                // Non-distributive union candidates keep the historical partial-match
                // behavior; distributive conditionals have already split union members.
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
        self.evaluate_preserving_tail_application_branch_alias(true_inst, Some(true_inst))
    }

    fn resolve_conditional_infer_property(
        &mut self,
        source: TypeId,
        prop_name: Atom,
        optional: bool,
    ) -> Option<TypeId> {
        if source == TypeId::OBJECT {
            return optional.then_some(TypeId::UNDEFINED);
        }

        if let Some(type_id) = self.implicit_sequence_property_type(source, prop_name) {
            return Some(type_id);
        }

        if let Some(query_db) = self.query_db() {
            let prop_name_str = self.interner().resolve_atom_ref(prop_name);
            return match query_db.resolve_property_access(source, &prop_name_str) {
                PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                PropertyAccessResult::PropertyNotFound { .. } => {
                    optional.then_some(TypeId::UNDEFINED)
                }
                _ => None,
            };
        }

        match self.interner().lookup(source) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner().object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .find(|prop| prop.name == prop_name)
                    .map(|prop| {
                        if optional {
                            self.optional_property_type(prop)
                        } else {
                            prop.type_id
                        }
                    })
                    .or_else(|| optional.then_some(TypeId::UNDEFINED))
            }
            Some(TypeData::Callable(callable_id)) => {
                // Callable types (class constructors) have static properties
                // that should participate in conditional infer resolution.
                // E.g., `typeof MyClass extends { defaultProps: infer D }` should
                // find `defaultProps` in the class constructor's static properties.
                let shape = self.interner().callable_shape(callable_id);
                shape
                    .properties
                    .iter()
                    .find(|prop| prop.name == prop_name)
                    .map(|prop| {
                        if optional {
                            self.optional_property_type(prop)
                        } else {
                            prop.type_id
                        }
                    })
                    .or_else(|| optional.then_some(TypeId::UNDEFINED))
            }
            Some(TypeData::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut inferred_members: SmallVec<[TypeId; 8]> = SmallVec::new();
                for &member in members.iter() {
                    let member_unwrapped = match self.interner().lookup(member) {
                        Some(TypeData::ReadonlyType(inner)) => inner,
                        _ => member,
                    };
                    let inferred = self.resolve_conditional_infer_property(
                        member_unwrapped,
                        prop_name,
                        optional,
                    )?;
                    inferred_members.push(inferred);
                }
                if inferred_members.is_empty() {
                    None
                } else if inferred_members.len() == 1 {
                    Some(inferred_members[0])
                } else {
                    Some(self.interner().union_from_slice(&inferred_members))
                }
            }
            Some(TypeData::Intersection(members)) => {
                // For intersection types (e.g., constructor & static props),
                // search each member for the property. The first member that has
                // the property wins (intersections share properties).
                let members = self.interner().type_list(members);
                for &member in members.iter() {
                    let member_unwrapped = match self.interner().lookup(member) {
                        Some(TypeData::ReadonlyType(inner)) => inner,
                        _ => member,
                    };
                    if let Some(inferred) = self.resolve_conditional_infer_property(
                        member_unwrapped,
                        prop_name,
                        optional,
                    ) {
                        return Some(inferred);
                    }
                }
                optional.then_some(TypeId::UNDEFINED)
            }
            _ => {
                // Fallback: try evaluating the source further and recursing.
                // This handles cases where the source is a TypeQuery, Lazy, Application
                // or other form that hasn't been fully evaluated.
                let evaluated = self.evaluate(source);
                if evaluated != source {
                    self.resolve_conditional_infer_property(evaluated, prop_name, optional)
                } else {
                    None
                }
            }
        }
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
        let check_key = self.interner().lookup(check_unwrapped);
        if matches!(
            check_key,
            Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
        ) {
            return self.interner().conditional(*cond);
        }

        let inferred = match check_key {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner().object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .find(|prop| prop.name == outer_name)
                    .and_then(|prop| {
                        let inner_type = match self.interner().lookup(prop.type_id) {
                            Some(TypeData::ReadonlyType(inner)) => inner,
                            _ => prop.type_id,
                        };
                        match self.interner().lookup(inner_type) {
                            Some(
                                TypeData::Object(inner_shape_id)
                                | TypeData::ObjectWithIndex(inner_shape_id),
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
            Some(TypeData::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut inferred_members: SmallVec<[TypeId; 8]> = SmallVec::new();
                for &member in members.iter() {
                    let member_unwrapped = match self.interner().lookup(member) {
                        Some(TypeData::ReadonlyType(inner)) => inner,
                        _ => member,
                    };
                    let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
                        self.interner().lookup(member_unwrapped)
                    else {
                        return self.evaluate(cond.false_type);
                    };
                    let shape = self.interner().object_shape(shape_id);
                    let Some(prop) = PropertyInfo::find_in_slice(&shape.properties, outer_name)
                    else {
                        return self.evaluate(cond.false_type);
                    };
                    let inner_type = match self.interner().lookup(prop.type_id) {
                        Some(TypeData::ReadonlyType(inner)) => inner,
                        _ => prop.type_id,
                    };
                    let Some(
                        TypeData::Object(inner_shape_id)
                        | TypeData::ObjectWithIndex(inner_shape_id),
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
                    Some(self.interner().union_from_slice(&inferred_members))
                }
            }
            _ => None,
        };

        let Some(mut inferred) = inferred else {
            return self.evaluate(cond.false_type);
        };

        let mut subst = TypeSubstitution::single(info.name, inferred);

        if let Some(constraint) = info.constraint {
            let mut checker = self.conditional_subtype_checker();
            checker.allow_bivariant_rest = true;
            let is_union = matches!(self.interner().lookup(inferred), Some(TypeData::Union(_)));
            if is_union && !cond.is_distributive {
                // Non-distributive union candidates keep the historical partial-match
                // behavior; distributive conditionals have already split union members.
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
        self.evaluate_preserving_tail_application_branch_alias(true_inst, Some(true_inst))
    }
}
