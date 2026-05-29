//! Array and tuple conditional infer helpers.

use super::*;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    pub(super) fn infer_pattern_has_unresolved_application(&mut self, type_id: TypeId) -> bool {
        let mut visited = FxHashSet::default();
        self.infer_pattern_has_unresolved_application_inner(type_id, &mut visited)
    }

    fn infer_pattern_has_unresolved_application_inner(
        &mut self,
        type_id: TypeId,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if type_id.is_intrinsic() || !visited.insert(type_id) {
            return false;
        }

        match self.interner().lookup(type_id) {
            Some(TypeData::Application(app_id)) => {
                let app = self.interner().type_application(app_id);
                let app_args_contain_infer =
                    app.args.iter().any(|&arg| self.type_contains_infer(arg));
                if app_args_contain_infer && self.application_base_is_unresolved(app.base) {
                    return true;
                }
                app.args
                    .iter()
                    .any(|&arg| self.infer_pattern_has_unresolved_application_inner(arg, visited))
            }
            Some(
                TypeData::Array(elem) | TypeData::ReadonlyType(elem) | TypeData::NoInfer(elem),
            ) => self.infer_pattern_has_unresolved_application_inner(elem, visited),
            Some(TypeData::Tuple(elements)) => {
                self.interner().tuple_list(elements).iter().any(|elem| {
                    self.infer_pattern_has_unresolved_application_inner(elem.type_id, visited)
                })
            }
            Some(TypeData::Union(members) | TypeData::Intersection(members)) => {
                self.interner().type_list(members).iter().any(|&member| {
                    self.infer_pattern_has_unresolved_application_inner(member, visited)
                })
            }
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner().object_shape(shape_id);
                shape.properties.iter().any(|prop| {
                    self.infer_pattern_has_unresolved_application_inner(prop.type_id, visited)
                }) || shape.string_index.as_ref().is_some_and(|index| {
                    self.infer_pattern_has_unresolved_application_inner(index.value_type, visited)
                }) || shape.number_index.as_ref().is_some_and(|index| {
                    self.infer_pattern_has_unresolved_application_inner(index.value_type, visited)
                })
            }
            Some(TypeData::Conditional(cond_id)) => {
                let cond = self.interner().get_conditional(cond_id);
                self.infer_pattern_has_unresolved_application_inner(cond.check_type, visited)
                    || self
                        .infer_pattern_has_unresolved_application_inner(cond.extends_type, visited)
                    || self.infer_pattern_has_unresolved_application_inner(cond.true_type, visited)
                    || self.infer_pattern_has_unresolved_application_inner(cond.false_type, visited)
            }
            _ => false,
        }
    }

    pub(super) fn application_array_infer_pattern(
        &self,
        app_id: crate::types::TypeApplicationId,
    ) -> Option<TypeParamInfo> {
        let app = self.interner().type_application(app_id);
        if app.args.len() != 1 {
            return None;
        }
        let Some(TypeData::Infer(info)) = self.interner().lookup(app.args[0]) else {
            return None;
        };

        let is_array_like_pattern = self.application_base_name_is(app.base, "Array")
            || self.application_base_name_is(app.base, "ReadonlyArray")
            || self.application_base_has_array_shape(app.base, false)
            || self.application_base_has_array_shape(app.base, true);

        is_array_like_pattern.then_some(info)
    }

    /// Handle array extends pattern: T extends (infer U)[] ? ...
    pub(super) fn eval_conditional_array_infer(
        &mut self,
        cond: &ConditionalType,
        check_unwrapped: TypeId,
        info: TypeParamInfo,
    ) -> TypeId {
        // PERF: Single lookup for type parameter check + inferred extraction
        let check_key = self.interner().lookup(check_unwrapped);
        let allow_readonly_array = matches!(
            self.interner().lookup(cond.extends_type),
            Some(TypeData::ReadonlyType(_))
        );
        if matches!(
            check_key,
            Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
        ) {
            return self.interner().conditional(*cond);
        }

        let inferred = match check_key {
            Some(TypeData::Array(elem)) => Some(elem),
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner().tuple_list(elements);
                let mut parts: SmallVec<[TypeId; 8]> = SmallVec::new();
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
                    Some(self.interner().union_from_slice(&parts))
                }
            }
            Some(TypeData::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut parts: SmallVec<[TypeId; 8]> = SmallVec::new();
                for &member in members.iter() {
                    match self.interner().lookup(member) {
                        Some(TypeData::Array(elem)) => parts.push(elem),
                        Some(TypeData::ReadonlyType(inner)) => {
                            let Some(TypeData::Array(elem)) = self.interner().lookup(inner) else {
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
                    Some(self.interner().union_from_slice(&parts))
                }
            }
            Some(TypeData::Application(app_id)) => {
                if let Some(element) = self.application_array_element(app_id, allow_readonly_array)
                {
                    Some(element)
                } else {
                    let app = self.interner().type_application(app_id);
                    if self.application_base_is_unresolved(app.base) {
                        return self.interner().conditional(*cond);
                    }
                    None
                }
            }
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                self.expanded_array_object_element(shape_id, allow_readonly_array)
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
        if matches!(
            self.interner().lookup(true_inst),
            Some(TypeData::Application(_))
        ) && crate::type_queries::contains_generic_type_parameters_db(self.interner(), true_inst)
        {
            return true_inst;
        }
        self.evaluate_preserving_tail_application_branch_alias(true_inst, Some(true_inst))
    }

    /// Handle concrete array extends pattern: `T extends Array<X>` (no infer in X).
    ///
    /// Extracts the element type `S` from `check_unwrapped`, then checks `S <: target_elem`.
    /// Returns `Some(true_branch)` or `Some(false_branch)` on success, `None` to fall
    /// through to the full structural subtype check.
    ///
    /// This avoids expanding `Array<X>` into its structural `ObjectWithIndex` form which
    /// triggers recursive method-signature comparisons that can hit cycle detection limits.
    pub(super) fn eval_conditional_array_concrete(
        &mut self,
        cond: &ConditionalType,
        check_unwrapped: TypeId,
        target_elem: TypeId,
        allow_readonly_array: bool,
    ) -> Option<TypeId> {
        // Defer when the check type is still a naked type parameter or infer variable —
        // the full conditional pipeline is required to handle those cases correctly.
        if matches!(
            self.interner().lookup(check_unwrapped),
            Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
        ) {
            return None;
        }

        let check_elem = self.extract_array_element(check_unwrapped, allow_readonly_array)?;

        let mut checker = self.conditional_subtype_checker();
        checker.allow_bivariant_rest = true;
        let branch = if checker.is_subtype_of(check_elem, target_elem) {
            cond.true_type
        } else {
            cond.false_type
        };
        Some(self.evaluate(branch))
    }

    /// Extract the element type from an array-like `check_type`.
    ///
    /// Handles `Array(elem)`, `ReadonlyType(Array(elem))`, `Tuple(...)`,
    /// `Application(Array|ReadonlyArray, [elem])`, and `ObjectWithIndex` array shapes.
    /// Returns `None` when the type is not an array-like shape.
    fn extract_array_element(
        &mut self,
        check_unwrapped: TypeId,
        allow_readonly_array: bool,
    ) -> Option<TypeId> {
        match self.interner().lookup(check_unwrapped) {
            Some(TypeData::Array(elem)) => Some(elem),
            Some(TypeData::ReadonlyType(inner)) if allow_readonly_array => {
                let Some(TypeData::Array(elem)) = self.interner().lookup(inner) else {
                    return None;
                };
                Some(elem)
            }
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner().tuple_list(elements);
                // Empty tuple — element type is `never`.
                if elements.is_empty() {
                    return Some(TypeId::NEVER);
                }
                let mut parts: SmallVec<[TypeId; 8]> = SmallVec::new();
                for element in elements.iter() {
                    if element.rest {
                        parts.push(self.rest_element_type(element.type_id));
                    } else if element.optional {
                        parts.push(self.interner().union2(element.type_id, TypeId::UNDEFINED));
                    } else {
                        parts.push(element.type_id);
                    }
                }
                Some(self.interner().union_from_slice(&parts))
            }
            Some(TypeData::Application(app_id)) => {
                self.application_array_concrete_element(app_id, allow_readonly_array)
            }
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                self.expanded_array_object_element(shape_id, allow_readonly_array)
            }
            _ => None,
        }
    }

    /// Return the element type of `Application(Array|ReadonlyArray, [X])` when X is
    /// not an `Infer` variable (those are handled by `eval_conditional_array_infer`).
    pub(super) fn application_array_concrete_element(
        &self,
        app_id: crate::types::TypeApplicationId,
        allow_readonly: bool,
    ) -> Option<TypeId> {
        let elem = self.application_array_element(app_id, allow_readonly)?;
        match self.interner().lookup(elem) {
            Some(TypeData::Infer(_)) => None,
            _ => Some(elem),
        }
    }

    /// Return `true` when `type_id` is `ReadonlyType(_)`, an `Application` of
    /// `ReadonlyArray`, or a user-defined alias that resolves to a readonly-array shape.
    pub(super) fn application_base_name_is_readonly_array(&self, type_id: TypeId) -> bool {
        match self.interner().lookup(type_id) {
            Some(TypeData::ReadonlyType(_)) => true,
            Some(TypeData::Application(app_id)) => {
                let app = self.interner().type_application(app_id);
                self.application_base_name_is(app.base, "ReadonlyArray")
                    || self.application_base_has_array_shape(app.base, true)
            }
            _ => false,
        }
    }

    pub(super) fn expanded_array_object_element(
        &self,
        shape_id: ObjectShapeId,
        allow_readonly_array: bool,
    ) -> Option<TypeId> {
        let shape = self.interner().object_shape(shape_id);
        let number_index = shape.number_index.as_ref()?;
        self.expanded_array_object_matches(shape_id, allow_readonly_array)
            .then_some(number_index.value_type)
    }

    fn expanded_array_object_matches(
        &self,
        shape_id: ObjectShapeId,
        allow_readonly_array: bool,
    ) -> bool {
        let shape = self.interner().object_shape(shape_id);
        if shape.number_index.is_none() {
            return false;
        }
        self.object_shape_has_array_markers(shape_id)
            || (allow_readonly_array && self.object_shape_has_readonly_array_markers(shape_id))
    }

    fn object_shape_has_array_markers(&self, shape_id: ObjectShapeId) -> bool {
        self.object_shape_has_property(shape_id, "push")
            && self.object_shape_has_property(shape_id, "shift")
    }

    fn object_shape_has_readonly_array_markers(&self, shape_id: ObjectShapeId) -> bool {
        self.object_shape_has_property(shape_id, "slice")
            && self.object_shape_has_property(shape_id, "concat")
    }

    fn object_shape_has_property(&self, shape_id: ObjectShapeId, expected: &str) -> bool {
        self.interner()
            .object_shape(shape_id)
            .properties
            .iter()
            .any(|prop| self.interner().resolve_atom_ref(prop.name).as_ref() == expected)
    }

    fn application_array_element(
        &self,
        app_id: crate::types::TypeApplicationId,
        allow_readonly_array: bool,
    ) -> Option<TypeId> {
        let app = self.interner().type_application(app_id);
        if app.args.len() != 1 {
            return None;
        }
        let is_array_application = self.application_base_name_is(app.base, "Array")
            || self.application_base_has_array_shape(app.base, false);
        let is_readonly_array_application = allow_readonly_array
            && (self.application_base_name_is(app.base, "ReadonlyArray")
                || self.application_base_has_array_shape(app.base, true));

        (is_array_application || is_readonly_array_application).then_some(app.args[0])
    }

    fn application_base_name_is(&self, base: TypeId, expected: &str) -> bool {
        if expected == "Array" && self.application_base_is_registered_array(base) {
            return true;
        }

        match self.interner().lookup(base) {
            Some(TypeData::Lazy(def_id)) => {
                if expected == "ReadonlyArray"
                    && self.resolver().is_builtin_readonly_array_def(def_id)
                {
                    return true;
                }
                self.resolver()
                    .get_def_name(def_id)
                    .is_some_and(|name| self.interner().resolve_atom_ref(name).as_ref() == expected)
            }
            Some(TypeData::TypeQuery(sym_ref)) => self
                .resolver()
                .symbol_to_def_id(sym_ref)
                .and_then(|def_id| self.resolver().get_def_name(def_id))
                .is_some_and(|name| self.interner().resolve_atom_ref(name).as_ref() == expected),
            Some(TypeData::UnresolvedTypeName(name)) => {
                self.interner().resolve_atom_ref(name).as_ref() == expected
            }
            _ => self
                .interner()
                .get_display_alias(base)
                .is_some_and(|alias| self.application_base_name_is(alias, expected)),
        }
    }

    fn application_base_is_registered_array(&self, base: TypeId) -> bool {
        self.interner()
            .get_array_base_type()
            .is_some_and(|array_base| self.application_bases_are_equivalent(base, array_base))
            || self
                .interner()
                .get_array_display_base_type()
                .is_some_and(|array_base| self.application_bases_are_equivalent(base, array_base))
    }

    fn application_bases_are_equivalent(&self, left: TypeId, right: TypeId) -> bool {
        if left == right {
            return true;
        }
        match (self.interner().lookup(left), self.interner().lookup(right)) {
            (Some(TypeData::Lazy(left_def)), Some(TypeData::Lazy(right_def))) => {
                self.resolver().defs_are_equivalent(left_def, right_def)
            }
            _ => false,
        }
    }

    fn application_base_has_array_shape(&self, base: TypeId, allow_readonly_array: bool) -> bool {
        match self.interner().lookup(base) {
            Some(TypeData::Lazy(def_id)) => self
                .resolver()
                .resolve_lazy(def_id, self.interner())
                .is_some_and(|resolved| {
                    self.resolved_application_base_has_array_shape(resolved, allow_readonly_array)
                }),
            Some(TypeData::TypeQuery(sym_ref)) => self
                .resolver()
                .symbol_to_def_id(sym_ref)
                .and_then(|def_id| self.resolver().resolve_lazy(def_id, self.interner()))
                .is_some_and(|resolved| {
                    self.resolved_application_base_has_array_shape(resolved, allow_readonly_array)
                }),
            _ => self
                .interner()
                .get_display_alias(base)
                .is_some_and(|alias| {
                    self.application_base_has_array_shape(alias, allow_readonly_array)
                }),
        }
    }

    fn application_base_is_unresolved(&self, base: TypeId) -> bool {
        match self.interner().lookup(base) {
            Some(TypeData::Lazy(def_id)) => {
                self.resolver().get_def_name(def_id).is_none()
                    && self
                        .resolver()
                        .resolve_lazy(def_id, self.interner())
                        .is_none()
            }
            Some(TypeData::TypeQuery(sym_ref)) => self
                .resolver()
                .symbol_to_def_id(sym_ref)
                .is_none_or(|def_id| {
                    self.resolver().get_def_name(def_id).is_none()
                        && self
                            .resolver()
                            .resolve_lazy(def_id, self.interner())
                            .is_none()
                }),
            _ => self
                .interner()
                .get_display_alias(base)
                .is_some_and(|alias| self.application_base_is_unresolved(alias)),
        }
    }

    fn resolved_application_base_has_array_shape(
        &self,
        type_id: TypeId,
        allow_readonly_array: bool,
    ) -> bool {
        match self.interner().lookup(type_id) {
            Some(TypeData::Array(_)) => true,
            Some(TypeData::ReadonlyType(inner)) => {
                allow_readonly_array
                    && matches!(self.interner().lookup(inner), Some(TypeData::Array(_)))
            }
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                self.expanded_array_object_matches(shape_id, allow_readonly_array)
            }
            _ => false,
        }
    }

    /// Handle tuple extends pattern: T extends [infer U] ? ...
    pub(super) fn eval_conditional_tuple_infer(
        &mut self,
        cond: &ConditionalType,
        check_unwrapped: TypeId,
        extends_elem: &TupleElement,
        info: TypeParamInfo,
    ) -> TypeId {
        let check_key = self.interner().lookup(check_unwrapped);
        if matches!(
            check_key,
            Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
        ) {
            return self.interner().conditional(*cond);
        }

        // `optionality_undefined` tracks whether the `undefined` in the inferred
        // candidate came from an optional source element (absent element reads as
        // `undefined`) rather than from the element's own declared type. A
        // constrained `infer` strips that optionality-`undefined` before applying
        // the constraint, matching tsc.
        let mut optionality_undefined = false;
        let inferred = match check_key {
            Some(TypeData::Tuple(check_elements)) => {
                let check_elements = self.interner().tuple_list(check_elements);
                if check_elements.is_empty() {
                    extends_elem.optional.then(|| {
                        optionality_undefined = true;
                        TypeId::UNDEFINED
                    })
                } else if check_elements.len() == 1 && !check_elements[0].rest {
                    let elem = &check_elements[0];
                    // Optional source cannot fill a required pattern slot.
                    if elem.optional && !extends_elem.optional {
                        None
                    } else {
                        let ty = if elem.optional {
                            optionality_undefined = true;
                            self.interner().union2(elem.type_id, TypeId::UNDEFINED)
                        } else {
                            elem.type_id
                        };
                        Some(ty)
                    }
                } else {
                    None
                }
            }
            Some(TypeData::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut inferred_members: SmallVec<[TypeId; 8]> = SmallVec::new();
                for &member in members.iter() {
                    let member_type = match self.interner().lookup(member) {
                        Some(TypeData::ReadonlyType(inner)) => inner,
                        _ => member,
                    };
                    match self.interner().lookup(member_type) {
                        Some(TypeData::Tuple(check_elements)) => {
                            let check_elements = self.interner().tuple_list(check_elements);
                            if check_elements.is_empty() {
                                if extends_elem.optional {
                                    optionality_undefined = true;
                                    inferred_members.push(TypeId::UNDEFINED);
                                    continue;
                                }
                                return self.evaluate(cond.false_type);
                            }
                            if check_elements.len() == 1 && !check_elements[0].rest {
                                let elem = &check_elements[0];
                                if elem.optional && !extends_elem.optional {
                                    return self.evaluate(cond.false_type);
                                }
                                let elem_type = if elem.optional {
                                    optionality_undefined = true;
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
            let filtered = if optionality_undefined {
                self.filter_optional_inferred_by_constraint(inferred, constraint, &mut checker)
            } else {
                self.filter_inferred_by_constraint(inferred, constraint, &mut checker)
            };
            let Some(filtered) = filtered else {
                // Constraint not satisfied: take the false branch, substituting any
                // bindings already established for this pattern.
                let false_inst =
                    instantiate_type_with_infer(self.interner(), cond.false_type, &subst);
                return self.evaluate(false_inst);
            };
            inferred = filtered;
            subst.insert(info.name, inferred);
        }

        let true_inst = instantiate_type_with_infer(self.interner(), cond.true_type, &subst);
        self.evaluate_preserving_tail_application_branch_alias(true_inst, Some(true_inst))
    }
}
