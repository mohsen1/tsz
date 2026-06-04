//! Object conditional infer helpers.

use super::*;

/// Outcome of resolving a single `infer` property of a conditional's extends
/// pattern against the (concrete) check type.
///
/// This distinguishes the three cases tsc's `inferFromProperties` produces, which
/// a plain `Option<TypeId>` cannot:
///
/// * [`Candidate`](InferPropertyResolution::Candidate) — the property is present;
///   its type is the inference candidate.
/// * [`NoCandidate`](InferPropertyResolution::NoCandidate) — an *optional* pattern
///   property is absent in the source. tsc contributes no candidate, so the
///   conditional still matches (true branch) but the infer variable stays unbound
///   and defaults to its constraint (or `unknown`). Crucially it must **not** pick
///   up a spurious `undefined`, which would corrupt a plain `infer R` (→ `undefined`
///   instead of `unknown`) and make a constrained `infer R extends C` fail its
///   constraint and collapse the conditional to its false branch.
/// * [`NoMatch`](InferPropertyResolution::NoMatch) — a *required* pattern property
///   is absent (or the source cannot supply it): the conditional takes its false
///   branch.
enum InferPropertyResolution {
    Candidate(TypeId),
    NoCandidate,
    NoMatch,
}

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
            if !captured && self.cached_type_contains_infer(prop.type_id) {
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
            let inferred =
                match self.resolve_conditional_infer_property(check_unwrapped, prop_name, optional)
                {
                    InferPropertyResolution::Candidate(ty) => ty,
                    InferPropertyResolution::NoMatch => return self.evaluate(cond.false_type),
                    // No candidate from this slot: leave the variable unbound here. If no
                    // other slot supplies one, pass 2 defaults it to its constraint (or
                    // `unknown`); a co-located slot that does supply a candidate still wins.
                    InferPropertyResolution::NoCandidate => continue,
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
        // A constrained `infer` is a whole-candidate check (tsc): the accumulated
        // candidate is kept only when it is assignable to the constraint as a
        // whole; otherwise the conditional takes its false branch. Optional
        // properties first strip the optionality-`undefined` they contribute, then
        // apply the same whole-candidate check.
        let mut subst = TypeSubstitution::new();
        for &(_, info, _) in infer_props {
            if subst.get(info.name).is_some() {
                continue; // already processed this variable
            }
            let Some(mut inferred) = accumulated.get(&info.name).copied() else {
                // No candidate from any slot (all were absent optionals): default the
                // variable to its constraint (or `unknown`), matching tsc's
                // `getInferredType`, and take the true branch.
                let default_ty = info.constraint.unwrap_or(TypeId::UNKNOWN);
                subst.insert(info.name, default_ty);
                continue;
            };

            if let Some(&(constraint, opt)) = effective_constraint.get(&info.name) {
                let mut checker = self.conditional_subtype_checker();
                checker.allow_bivariant_rest = true;

                if opt {
                    // Optional property: strip the optionality-`undefined` from the
                    // candidate, then apply the constraint as a whole-candidate check.
                    let Some(filtered) = self.filter_optional_inferred_by_constraint(
                        inferred,
                        constraint,
                        &mut checker,
                    ) else {
                        let false_inst =
                            instantiate_type_with_infer(self.interner(), cond.false_type, &subst);
                        return self.evaluate(false_inst);
                    };
                    inferred = filtered;
                } else if !checker.is_subtype_of(inferred, constraint) {
                    // A constrained `infer U extends C` is a whole-candidate check in
                    // tsc: if the full inferred candidate is not assignable to the
                    // constraint, the conditional resolves to its false branch. No
                    // per-member union filtering. Distributive conditionals have
                    // already split union check types into individual members.
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

        let mut inferred = match self.resolve_conditional_infer_property(
            check_unwrapped,
            prop_name,
            prop_optional,
        ) {
            InferPropertyResolution::Candidate(ty) => ty,
            InferPropertyResolution::NoMatch => return self.evaluate(cond.false_type),
            InferPropertyResolution::NoCandidate => {
                // tsc's `getInferredType`: an infer variable with no candidate
                // resolves to its constraint (or `unknown`), and the conditional
                // takes its true branch. The absent optional property neither
                // supplies a spurious `undefined` candidate nor forces the false
                // branch.
                let default_ty = info.constraint.unwrap_or(TypeId::UNKNOWN);
                let subst = TypeSubstitution::single(info.name, default_ty);
                let true_inst =
                    instantiate_type_with_infer(self.interner(), cond.true_type, &subst);
                return self
                    .evaluate_preserving_tail_application_branch_alias(true_inst, Some(true_inst));
            }
        };

        let mut subst = TypeSubstitution::single(info.name, inferred);

        if let Some(constraint) = info.constraint {
            let mut checker = self.conditional_subtype_checker();
            checker.allow_bivariant_rest = true;
            if prop_optional {
                // Optional property: strip the optionality-`undefined` from the
                // candidate, then apply the constraint as a whole-candidate check.
                let Some(filtered) =
                    self.filter_optional_inferred_by_constraint(inferred, constraint, &mut checker)
                else {
                    let false_inst =
                        instantiate_type_with_infer(self.interner(), cond.false_type, &subst);
                    return self.evaluate(false_inst);
                };
                inferred = filtered;
            } else if !checker.is_subtype_of(inferred, constraint) {
                // Whole-candidate constraint check (tsc): a constrained `infer`
                // whose full candidate is not assignable to the constraint takes
                // the false branch. No per-member union filtering; distributive
                // conditionals have already split union members.
                return self.evaluate(cond.false_type);
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
    ) -> InferPropertyResolution {
        // A `readonly` wrapper does not change a property's value type, so strip
        // it before dispatching; otherwise a `readonly [...]`/`ReadonlyArray`
        // source would miss the tuple/object arms below and defer. The union and
        // intersection arms unwrap their members the same way.
        let source = crate::type_queries::data::unwrap_readonly(self.interner(), source);

        // Absent property: an optional pattern position contributes no candidate
        // (tsc skips it), while a required one fails the match. Shared by every
        // "property not found" arm below.
        let absent = if optional {
            InferPropertyResolution::NoCandidate
        } else {
            InferPropertyResolution::NoMatch
        };

        if source == TypeId::OBJECT {
            return absent;
        }

        if let Some(type_id) = self.implicit_sequence_property_type(source, prop_name) {
            return InferPropertyResolution::Candidate(type_id);
        }

        if let Some(query_db) = self.query_db() {
            let prop_name_str = self.interner().resolve_atom_ref(prop_name);
            return match query_db.resolve_property_access(source, &prop_name_str) {
                PropertyAccessResult::Success { type_id, .. } => {
                    InferPropertyResolution::Candidate(type_id)
                }
                PropertyAccessResult::PropertyNotFound { .. } => absent,
                _ => InferPropertyResolution::NoMatch,
            };
        }

        match self.interner().lookup(source) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner().object_shape(shape_id);
                self.property_candidate(&shape.properties, prop_name, optional)
                    .unwrap_or(absent)
            }
            Some(TypeData::Callable(callable_id)) => {
                // Callable types (class constructors) have static properties
                // that should participate in conditional infer resolution.
                // E.g., `typeof MyClass extends { defaultProps: infer D }` should
                // find `defaultProps` in the class constructor's static properties.
                let shape = self.interner().callable_shape(callable_id);
                self.property_candidate(&shape.properties, prop_name, optional)
                    .unwrap_or(absent)
            }
            Some(TypeData::Union(members)) => {
                // A concrete union check type contributes the union of each member's
                // candidate. A member that lacks an optional property contributes
                // `undefined` (the indexed-access reading of an absent optional);
                // a member that fails the match (required prop absent) fails the
                // whole conditional.
                let members = self.interner().type_list(members);
                let mut inferred_members: SmallVec<[TypeId; 8]> = SmallVec::new();
                for &member in members.iter() {
                    let member_unwrapped = match self.interner().lookup(member) {
                        Some(TypeData::ReadonlyType(inner)) => inner,
                        _ => member,
                    };
                    match self.resolve_conditional_infer_property(
                        member_unwrapped,
                        prop_name,
                        optional,
                    ) {
                        InferPropertyResolution::Candidate(ty) => inferred_members.push(ty),
                        InferPropertyResolution::NoCandidate => {
                            inferred_members.push(TypeId::UNDEFINED)
                        }
                        InferPropertyResolution::NoMatch => {
                            return InferPropertyResolution::NoMatch;
                        }
                    }
                }
                match inferred_members.len() {
                    0 => absent,
                    1 => InferPropertyResolution::Candidate(inferred_members[0]),
                    _ => InferPropertyResolution::Candidate(
                        self.interner().union_from_slice(&inferred_members),
                    ),
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
                    if let InferPropertyResolution::Candidate(inferred) = self
                        .resolve_conditional_infer_property(member_unwrapped, prop_name, optional)
                    {
                        return InferPropertyResolution::Candidate(inferred);
                    }
                }
                absent
            }
            Some(TypeData::Tuple(elements_id)) => {
                // A numeric-index pattern property (`{ 0: infer A }`) reads the
                // tuple's element at that index, mirroring tsc's structural
                // `inferFromProperties` over the apparent (tuple) type. `length`
                // is already served above by `implicit_sequence_property_type`.
                // The dedicated `query_db` path above covers this when a query
                // database is connected; this arm keeps the behaviour correct in
                // pure-evaluation contexts (e.g. type-alias conditional infer)
                // where `query_db` is `None`, instead of falling through to a
                // spurious `NoMatch`/false-branch.
                let prop_name_str = self.interner().resolve_atom_ref(prop_name);
                let Some(index) = crate::operations::sequence_property::parse_numeric_index(
                    prop_name_str.as_ref(),
                ) else {
                    // A non-numeric key (e.g. an array-prototype method) is not a
                    // guaranteed tuple property; tuples are closed shapes, so a
                    // missing key fails the structural match (false branch).
                    return InferPropertyResolution::NoMatch;
                };
                let elements = self.interner().tuple_list(elements_id);
                match crate::operations::sequence_property::tuple_fixed_slot(
                    self.interner(),
                    &elements,
                    index,
                ) {
                    // Guaranteed (non-optional) slot present: contributes the
                    // element type as the inference candidate, with no
                    // optionality-`undefined`.
                    Some((element_type, false)) => InferPropertyResolution::Candidate(element_type),
                    // Optional slot present: the property is not guaranteed, so a
                    // required pattern position (`{ 0: infer A }`) takes the false
                    // branch — `[number?]` is not assignable to `{ 0: unknown }`.
                    // An optional pattern position (`{ 0?: infer A }`) still binds
                    // the bare element type.
                    Some((element_type, true)) => {
                        if optional {
                            InferPropertyResolution::Candidate(element_type)
                        } else {
                            InferPropertyResolution::NoMatch
                        }
                    }
                    // Index beyond the tuple's fixed slots (or inside an unbounded
                    // rest): the property is absent. A tuple is a closed shape, so
                    // a missing numeric key fails the match for both required and
                    // optional pattern positions (`[] extends { 0?: infer A }` is
                    // tsc's false branch), unlike an open object literal.
                    None => InferPropertyResolution::NoMatch,
                }
            }
            _ => {
                // Fallback: try evaluating the source further and recursing.
                // This handles cases where the source is a TypeQuery, Lazy, Application
                // or other form that hasn't been fully evaluated.
                let evaluated = self.evaluate(source);
                if evaluated != source {
                    self.resolve_conditional_infer_property(evaluated, prop_name, optional)
                } else {
                    InferPropertyResolution::NoMatch
                }
            }
        }
    }

    /// Look up `prop_name` among a property list (an object or callable shape),
    /// returning its candidate type when present. `None` means the property is
    /// absent — the caller decides whether that is a no-candidate optional or a
    /// hard no-match.
    fn property_candidate(
        &self,
        properties: &[PropertyInfo],
        prop_name: Atom,
        optional: bool,
    ) -> Option<InferPropertyResolution> {
        properties
            .iter()
            .find(|prop| prop.name == prop_name)
            .map(|prop| {
                InferPropertyResolution::Candidate(if optional {
                    self.optional_property_type(prop)
                } else {
                    prop.type_id
                })
            })
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

        let Some(inferred) = inferred else {
            return self.evaluate(cond.false_type);
        };

        let mut subst = TypeSubstitution::single(info.name, inferred);

        if let Some(constraint) = info.constraint {
            let mut checker = self.conditional_subtype_checker();
            checker.allow_bivariant_rest = true;
            if !checker.is_subtype_of(inferred, constraint) {
                // Whole-candidate constraint check (tsc): a constrained `infer`
                // whose full candidate is not assignable to the constraint takes
                // the false branch. No per-member union filtering; distributive
                // conditionals have already split union members.
                return self.evaluate(cond.false_type);
            }
            subst.insert(info.name, inferred);
        }

        let true_inst = instantiate_type_with_infer(self.interner(), cond.true_type, &subst);
        self.evaluate_preserving_tail_application_branch_alias(true_inst, Some(true_inst))
    }
}
