impl<'a> CheckerState<'a> {
    pub(super) fn evaluate_type_for_assignability_inner(&mut self, type_id: TypeId) -> TypeId {
        if let Some(evaluated) = self.evaluate_lazy_alias_for_assignability(type_id) {
            return evaluated;
        }
        if let Some(distributed) = self.distribute_intersection_union_for_assignability(type_id) {
            return distributed;
        }

        let kind = classify_for_assignability_eval(self.ctx.types, type_id);
        let mut evaluated = match kind {
            AssignabilityEvalKind::Application => {
                let result = self.evaluate_type_with_resolution(type_id);
                // Guard: if evaluation degraded a valid type to ERROR (e.g., due to
                // stack overflow protection tripping during deep recursive type
                // resolution), preserve the original type. ERROR is treated as
                // assignable to/from everything by the subtype checker, which would
                // silently suppress real type errors like TS2322. Keeping the original
                // Lazy type allows the compat checker's resolver to resolve it from the
                // type environment (populated during earlier successful resolution).
                if result == TypeId::ERROR && type_id != TypeId::ERROR {
                    return type_id;
                }
                result
            }
            AssignabilityEvalKind::NeedsEnvEval => {
                // For TypeQuery (typeof), resolve the value type directly from
                // get_type_of_symbol. The TypeEnvironment's types map may contain
                // the instance type for class symbols (stored by type-position
                // resolution paths like resolve_lazy_def_for_type_env), but
                // TypeQuery needs the value-position type (constructor for classes).
                if let Some(symbol_ref) = crate::query_boundaries::common::type_query_symbol(
                    self.ctx.types.as_type_database(),
                    type_id,
                ) {
                    let sym_id = tsz_binder::SymbolId(symbol_ref.0);
                    // For merged TYPE_ALIAS + VARIABLE symbols (e.g.,
                    // `type Input = Static<typeof Input>` + `const Input = ...`),
                    // get_type_of_symbol may return the type alias's circular
                    // Lazy(DefId) instead of the value's concrete type. Since
                    // TypeQuery always refers to the value side, resolve directly
                    // from the value declaration to avoid TS2344 false positives.
                    let flags = self
                        .ctx
                        .binder
                        .get_symbol(sym_id)
                        .map(|s| s.flags)
                        .unwrap_or(0);
                    if (flags & tsz_binder::symbol_flags::TYPE_ALIAS) != 0
                        && (flags & tsz_binder::symbol_flags::VARIABLE) != 0
                    {
                        let value_decl = self
                            .ctx
                            .binder
                            .get_symbol(sym_id)
                            .map(|s| s.value_declaration)
                            .unwrap_or(tsz_parser::NodeIndex::NONE);
                        self.type_of_value_declaration_for_symbol(sym_id, value_decl)
                    } else {
                        self.get_type_of_symbol(sym_id)
                    }
                } else {
                    self.evaluate_type_with_env(type_id)
                }
            }
            AssignabilityEvalKind::Resolved => type_id,
        };

        if evaluated != type_id && evaluated != TypeId::ERROR && evaluated != TypeId::ANY {
            let further = self.evaluate_type_for_assignability(evaluated);
            if further != TypeId::ERROR && further != TypeId::ANY {
                evaluated = further;
            }
        }

        // Distribution pass: normalize compound types so mixed representations do not
        // leak into relation checks (for example, `Lazy(Class)` + resolved class object).
        if let Some(distributed) = self.distribute_intersection_union_for_assignability(evaluated) {
            evaluated = distributed;
        } else if let Some(distributed) =
            map_compound_members(self.ctx.types, evaluated, |member| {
                self.evaluate_type_for_assignability(member)
            })
        {
            evaluated = distributed;
        }

        // tsc expands homomorphic mapped type applications (e.g. `PassThrough<A|B>`)
        // before structural comparison; mirror that for tuple elements.
        if let Some(elements) =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, evaluated)
        {
            let mut any_changed = false;
            let new_elements: Vec<tsz_solver::TupleElement> = elements
                .iter()
                .map(|elem| {
                    if matches!(
                        classify_for_assignability_eval(self.ctx.types, elem.type_id),
                        AssignabilityEvalKind::Resolved
                    ) {
                        return *elem;
                    }
                    let elem_eval = self.evaluate_type_for_assignability(elem.type_id);
                    if elem_eval != elem.type_id {
                        any_changed = true;
                    }
                    tsz_solver::TupleElement {
                        type_id: elem_eval,
                        ..*elem
                    }
                })
                .collect();
            if any_changed {
                evaluated = self.ctx.types.as_type_database().tuple(new_elements);
            }
        }

        if crate::query_boundaries::assignability::remapped_mapped_type_has_no_outer_type_params(
            self.ctx.types,
            evaluated,
        ) {
            let concrete = self.evaluate_concrete_remapped_mapped_type_with_resolution(evaluated);
            if concrete != evaluated {
                evaluated = concrete;
            }
        }

        evaluated = self.evaluate_awaited_application_for_assignability(evaluated);

        evaluated = self.normalize_callable_type_for_assignability(evaluated);

        evaluated
    }

    fn distribute_intersection_union_for_assignability(
        &mut self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        let members =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)?;
        let mut evaluated_members = Vec::with_capacity(members.len());
        let mut union_member_index = None;

        for member in members {
            let evaluated = self.evaluate_type_for_assignability(member);
            if union_member_index.is_none() && self.object_union_has_branch_only_keys(evaluated) {
                union_member_index = Some(evaluated_members.len());
            }
            evaluated_members.push(evaluated);
        }

        let union_member_index = union_member_index?;
        let union_members = crate::query_boundaries::common::union_members(
            self.ctx.types,
            evaluated_members[union_member_index],
        )?;
        let mut distributed = Vec::with_capacity(union_members.len());
        for branch in union_members {
            let mut branch_members = evaluated_members.clone();
            branch_members[union_member_index] = branch;
            distributed.push(self.ctx.types.factory().intersection(branch_members));
        }

        Some(self.ctx.types.factory().union_preserve_members(distributed))
    }

    fn object_union_has_branch_only_keys(&self, type_id: TypeId) -> bool {
        let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        else {
            return false;
        };
        if members.len() < 2 {
            return false;
        }

        let mut first_keys = None;
        for member in members {
            let Some(shape_id) =
                crate::query_boundaries::common::object_shape_id(self.ctx.types, member)
            else {
                return false;
            };
            let keys: FxHashSet<_> = self
                .ctx
                .types
                .object_shape(shape_id)
                .properties
                .iter()
                .map(|prop| prop.name)
                .collect();
            match &first_keys {
                Some(first) if first != &keys => return true,
                None => first_keys = Some(keys),
                _ => {}
            }
        }
        false
    }

    pub(super) fn concrete_remapped_mapped_assignability_target(
        &mut self,
        target: TypeId,
    ) -> Option<TypeId> {
        let resolved = self.evaluate_type_with_resolution(target);
        let mapped_id = crate::query_boundaries::common::mapped_type_id(self.ctx.types, resolved)?;
        let mapped = self.ctx.types.mapped_type(mapped_id);
        mapped.name_type?;
        let concrete = self.evaluate_concrete_remapped_mapped_type_with_resolution(resolved);
        (concrete != resolved).then_some(concrete)
    }

    /// Recursively evaluate Lazy property types within an Object type so that
    /// the solver's `types_are_comparable_for_assertion` sees concrete types
    /// instead of opaque `Lazy(DefId)` references.
    ///
    /// Recurses up to `max_depth` levels into nested Object types whose
    /// properties are Lazy.  Returns the original type unchanged if it is not
    /// an object or has no Lazy property types.
    pub(crate) fn deep_evaluate_object_properties(&mut self, type_id: TypeId) -> TypeId {
        self.deep_evaluate_object_properties_inner(type_id, 0)
    }

    fn deep_evaluate_object_properties_inner(&mut self, type_id: TypeId, depth: u32) -> TypeId {
        const MAX_DEPTH: u32 = 3;
        if depth >= MAX_DEPTH {
            return type_id;
        }

        // Tuples carry their element types directly (not via Object shape),
        // so the property-shape walk below would skip them. Resolve each
        // tuple element first so downstream comparable-for-assertion checks
        // (e.g. tuple-to-tuple element-wise overlap in
        // `types_are_comparable_for_assertion`) see concrete types instead
        // of unresolved `Lazy(DefId)` class refs — those refs short-circuit
        // the solver's depth>0 Lazy heuristic to "comparable", masking real
        // mismatches like `[C, D] as [A, I]`.
        if let Some(elements) =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, type_id)
        {
            let mut any_changed = false;
            let new_elements: Vec<tsz_solver::TupleElement> = elements
                .iter()
                .map(|elem| {
                    let mut eval_ty = elem.type_id;
                    if crate::query_boundaries::common::is_lazy_type(
                        self.ctx.types.as_type_database(),
                        eval_ty,
                    ) {
                        let resolved = self.evaluate_type_for_assignability(eval_ty);
                        if resolved != eval_ty {
                            any_changed = true;
                            eval_ty = resolved;
                        }
                    }
                    let deep = self.deep_evaluate_object_properties_inner(eval_ty, depth + 1);
                    if deep != eval_ty {
                        any_changed = true;
                        eval_ty = deep;
                    }
                    tsz_solver::TupleElement {
                        type_id: eval_ty,
                        ..*elem
                    }
                })
                .collect();
            if any_changed {
                return self.ctx.types.as_type_database().tuple(new_elements);
            }
            return type_id;
        }

        let db = self.ctx.types.as_type_database();
        // Use solver query API to get the shape id (handles Object and ObjectWithIndex)
        let shape_id = match crate::query_boundaries::common::object_shape_id(db, type_id) {
            Some(sid) => sid,
            None => return type_id,
        };

        let shape = db.object_shape(shape_id);
        let mut any_changed = false;
        let new_props: Vec<tsz_solver::PropertyInfo> = shape
            .properties
            .iter()
            .map(|p| {
                let mut eval_ty = p.type_id;
                // Resolve Lazy references (interface/type alias names)
                if crate::query_boundaries::common::is_lazy_type(
                    self.ctx.types.as_type_database(),
                    eval_ty,
                ) {
                    let resolved = self.evaluate_type_for_assignability(eval_ty);
                    if resolved != eval_ty {
                        any_changed = true;
                        eval_ty = resolved;
                    }
                }
                // Recurse into resolved Object types to resolve their properties too
                let deep = self.deep_evaluate_object_properties_inner(eval_ty, depth + 1);
                if deep != eval_ty {
                    any_changed = true;
                    eval_ty = deep;
                }

                let mut eval_write = p.write_type;
                if crate::query_boundaries::common::is_lazy_type(
                    self.ctx.types.as_type_database(),
                    eval_write,
                ) {
                    let resolved = self.evaluate_type_for_assignability(eval_write);
                    if resolved != eval_write {
                        any_changed = true;
                        eval_write = resolved;
                    }
                }

                tsz_solver::PropertyInfo {
                    type_id: eval_ty,
                    write_type: eval_write,
                    ..*p
                }
            })
            .collect();

        if !any_changed {
            return type_id;
        }

        // Re-intern the object with resolved property types
        self.ctx.types.as_type_database().object(new_props)
    }

    /// Resolve a deferred Mapped type by pre-resolving its constraint's Applications.
    ///
    /// When evaluation produces a deferred Mapped type (e.g., from Omit/Pick where
    /// the constraint contains Application types like `Exclude<keyof T, K>`), the
    /// solver's `TypeEvaluator` may have failed because lib type `DefIds` weren't
    /// registered in the `TypeEnvironment`. This method resolves the constraint through
    /// the checker's evaluation path and retries the Mapped type evaluation.
    pub(crate) fn resolve_deferred_mapped_type(&mut self, type_id: TypeId) -> TypeId {
        let Some(mapped_id) = crate::query_boundaries::state::type_environment::mapped_type_id(
            self.ctx.types.as_type_database(),
            type_id,
        ) else {
            return type_id;
        };
        let mapped = self.ctx.types.mapped_type(mapped_id);
        let constraint = mapped.constraint;
        let resolved_constraint = self.evaluate_mapped_constraint_with_resolution(constraint);
        if resolved_constraint != constraint {
            self.ctx
                .cache_env_eval_result_if_absent(constraint, resolved_constraint, false);
            let retry = self.evaluate_type_with_env_uncached(type_id);
            if retry != type_id {
                return retry;
            }
        }
        type_id
    }

    /// Substitute `ThisType` in a type with the enclosing class instance type.
    ///
    /// When inside a class body, `ThisType` represents the polymorphic `this` type
    /// (a type parameter bounded by the class). Since the `this` expression evaluates
    /// to the concrete class instance type, we must substitute `ThisType` → class
    /// instance type before assignability checks. This matches tsc's behavior where
    /// `return this`, `f(this)`, etc. succeed when the target type is `this`.
    pub(super) fn substitute_this_type_if_needed(&mut self, type_id: TypeId) -> TypeId {
        // Fast path: intrinsic types can't contain ThisType
        if type_id.is_intrinsic() {
            return type_id;
        }

        let needs_substitution =
            crate::query_boundaries::common::contains_this_type(self.ctx.types, type_id);

        if !needs_substitution {
            return type_id;
        }

        let Some(class_info) = &self.ctx.enclosing_class else {
            return type_id;
        };
        let class_idx = class_info.class_idx;

        let Some(node) = self.ctx.arena.get(class_idx) else {
            return type_id;
        };
        let Some(class_data) = self.ctx.arena.get_class(node) else {
            return type_id;
        };

        let instance_type = self.get_class_instance_type(class_idx, class_data);

        if crate::query_boundaries::common::is_this_type(self.ctx.types, type_id) {
            // Substitute bare `ThisType` with the concrete class instance type so
            // that `return this` / `f(this)` assignability succeeds by identity check.
            instance_type
        } else if crate::query_boundaries::common::index_access_types(self.ctx.types, type_id)
            .is_some()
        {
            // A direct indexed access like `this["x"]` is still anchored in the
            // current class context. Resolve it to the concrete property type for
            // assignment/call-argument checks, while leaving more complex wrappers
            // such as `Unwrap<this["x"]>` deferred below.
            let substituted = crate::query_boundaries::common::substitute_this_type(
                self.ctx.types,
                type_id,
                instance_type,
            );
            self.evaluate_type_with_env_uncached(substituted)
        } else {
            // Do NOT substitute complex types that merely contain `ThisType` in nested
            // positions (e.g. `Builder_instance` whose methods return `this`).  The
            // solver's `bind_property_receiver_this` already substitutes `this` during
            // property comparison using the object shape's receiver symbol.
            // Pre-substituting here creates a new TypeId (Builder_instance_subst) with no
            // symbol, so the subsequent `bind_property_receiver_this` call on the *target*
            // produces a Lazy/ref TypeId while the source stays as the concrete TypeId,
            // causing spurious TS2322 errors for fluent/builder patterns.
            type_id
        }
    }
}
