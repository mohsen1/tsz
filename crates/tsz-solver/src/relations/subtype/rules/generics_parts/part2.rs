impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Distribute a homomorphic mapped type over an intersection argument.
    ///
    /// When the mapped type has the form `{ [K in keyof (A & B)]: (A & B)[K] }`
    /// (possibly with readonly/optional modifiers), this is equivalent to
    /// `{ [K in keyof A]: A[K] } & { [K in keyof B]: B[K] }` with the same
    /// modifiers. This implements the tsc equivalence:
    ///   `Readonly<A & B>` ≡ `Readonly<A> & Readonly<B>`
    ///
    /// Returns `Some(distributed_intersection)` if distribution applies, `None` otherwise.
    fn try_distribute_mapped_over_intersection(
        &mut self,
        mapped_id: MappedTypeId,
    ) -> Option<TypeId> {
        let mapped = self.interner.get_mapped(mapped_id);

        // Must not have name remapping (as clause)
        if mapped.name_type.is_some() {
            return None;
        }

        // Constraint must be keyof(S) for some S
        let constraint_source = keyof_inner_type(self.interner, mapped.constraint)?;

        // S must be an intersection
        let list_id = intersection_list_id(self.interner, constraint_source)?;
        let members = self.interner.type_list(list_id).to_vec();

        if members.len() < 2 {
            return None;
        }

        // Template must be S[K] (identity indexed access form)
        let (template_obj, template_idx) = index_access_parts(self.interner, mapped.template)?;
        let idx_param = type_param_info(self.interner, template_idx)?;
        if idx_param.name != mapped.type_param.name || template_obj != constraint_source {
            return None;
        }

        // Distribute: for each member M, create { [K in keyof M]: M[K] } with same modifiers
        let mut distributed_members = Vec::with_capacity(members.len());
        for &member in &members {
            let member_constraint = self.interner.keyof(member);
            let member_k = self.interner.type_param(TypeParamInfo {
                name: mapped.type_param.name,
                constraint: Some(member_constraint),
                default: None,
                is_const: false,
            });
            let member_template = self.interner.index_access(member, member_k);
            let member_mapped = self.interner.mapped(MappedType {
                type_param: mapped.type_param,
                constraint: member_constraint,
                name_type: None,
                template: member_template,
                readonly_modifier: mapped.readonly_modifier,
                optional_modifier: mapped.optional_modifier,
            });
            distributed_members.push(member_mapped);
        }

        Some(self.interner.intersection(distributed_members))
    }

    fn try_expand_mapped_with_constraint(&mut self, mapped_id: MappedTypeId) -> Option<TypeId> {
        use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
        let mapped = self.interner.get_mapped(mapped_id);
        if let Some(TypeData::KeyOf(source)) = self.interner.lookup(mapped.constraint)
            && let Some(TypeData::TypeParameter(param)) = self.interner.lookup(source)
            && let Some(constraint) = param.constraint
        {
            // A self-referential bound like `T extends Box<T>` is not a concrete
            // structural expansion source. Substituting it back into a mapped type
            // can make recursive constraints look satisfiable simply because the
            // relation checker re-enters the same bound coinductively.
            if contains_type_parameter_named(self.interner, constraint, param.name) {
                return None;
            }

            let subst = TypeSubstitution::single(param.name, constraint);
            // Use keyof(constraint) directly to prevent eager evaluation
            // which would break array/tuple preservation in evaluate_mapped.
            let inst_constraint = self.interner.keyof(constraint);
            let inst_template = instantiate_type(self.interner, mapped.template, &subst);
            let inst_name = mapped
                .name_type
                .map(|n| instantiate_type(self.interner, n, &subst));
            let new_mapped_id = self.interner.mapped(MappedType {
                type_param: mapped.type_param,
                constraint: inst_constraint,
                name_type: inst_name,
                template: inst_template,
                optional_modifier: mapped.optional_modifier,
                readonly_modifier: mapped.readonly_modifier,
            });
            if let Some(TypeData::Mapped(m_id)) = self.interner.lookup(new_mapped_id) {
                let new_mapped = self.interner.get_mapped(m_id);
                let res = crate::evaluation::evaluate::evaluate_mapped(self.interner, &new_mapped);
                if res != TypeId::ERROR && res != new_mapped_id {
                    return Some(res);
                }
            }
        }
        None
    }

    /// Try to expand an Application type to its structural form.
    /// Returns None if the application cannot be expanded (missing type params or body).
    ///
    pub(crate) fn try_expand_application(&mut self, app_id: TypeApplicationId) -> Option<TypeId> {
        use crate::instantiation::instantiate::TypeSubstitution;

        let app = self.interner.type_application(app_id);

        let def_id = self.application_base_def_id(app.base)?;
        let type_params = self.resolver.get_lazy_type_params(def_id)?;
        let resolved_body = match self.resolver.resolve_lazy(def_id, self.interner) {
            Some(body) => body,
            None => {
                // Re-entrant lib resolution: the application's base def has
                // no body registered yet. The caller propagates `None` into a
                // structural fallback that can produce a cacheable False —
                // record the undetermined-result event so the enclosing
                // `check_subtype` call skips caching for this pair.
                crate::relations::subtype::cache::note_lazy_resolve_failure();
                return None;
            }
        };
        let effective_body = if matches!(
            self.resolver.get_def_kind(def_id),
            Some(crate::def::DefKind::Class)
        ) {
            match self.interner.lookup(resolved_body) {
                Some(TypeData::Callable(cs_id)) => {
                    let shape = self.interner.callable_shape(cs_id);
                    shape
                        .construct_signatures
                        .first()
                        .map(|sig| sig.return_type)
                        .unwrap_or(resolved_body)
                }
                _ => resolved_body,
            }
        } else {
            resolved_body
        };

        // Skip expansion if the resolved type is just this Application
        // (prevents infinite recursion on self-referential types)
        if let Some(resolved_app_id) = application_id(self.interner, effective_body)
            && resolved_app_id == app_id
        {
            return None;
        }

        // Homomorphic identity mapped type passthrough: if the body is
        // `{ [K in keyof T]: T[K] }` and the argument for T is a genuine primitive type,
        // return the arg directly. This mirrors evaluate_application().
        // Only applies for identity templates (T[K]), not arbitrary ones like Data.
        // For `any`: only passthrough when the type parameter is constrained to array/tuple.
        // Otherwise, `any` must flow through mapped type expansion to produce
        // `{ [x: string]: any }` (matching tsc's behavior for `Objectish<any>`).
        if let Some(TypeData::Mapped(mapped_id)) = self.interner.lookup(effective_body) {
            let mapped = self.interner.get_mapped(mapped_id);
            if let Some(TypeData::KeyOf(source)) = self.interner.lookup(mapped.constraint)
                && let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(source)
                && let Some(idx) = type_params.iter().position(|p| p.name == tp.name)
                && idx < app.args.len()
                // Verify template is T[K] (identity indexed access)
                && let Some(TypeData::IndexAccess(obj, key)) = self.interner.lookup(mapped.template)
                && obj == source
                && matches!(self.interner.lookup(key), Some(TypeData::TypeParameter(kp)) if kp.name == mapped.type_param.name)
            {
                let arg = app.args[idx];
                let is_any_like = arg == TypeId::ANY
                    || arg == TypeId::UNKNOWN
                    || arg == TypeId::NEVER
                    || arg == TypeId::ERROR;
                let should_passthrough = if is_any_like {
                    tp.constraint.is_some_and(|c| {
                        matches!(
                            self.interner.lookup(c),
                            Some(TypeData::Array(_) | TypeData::Tuple(_))
                        )
                    })
                } else {
                    is_primitive_type(self.interner, arg)
                };
                if should_passthrough {
                    return Some(arg);
                }
            }
        }

        // Create substitution and instantiate
        let substitution = TypeSubstitution::from_args(self.interner, &type_params, &app.args);
        let app_type = self.interner.application(app.base, app.args.clone());

        let mut instantiated = crate::instantiation::instantiate::instantiate_type_cached(
            self.interner,
            self.query_db,
            effective_body,
            &substitution,
        );
        if crate::contains_this_type(self.interner, instantiated) {
            instantiated = crate::instantiation::instantiate::substitute_this_type_cached(
                self.interner,
                self.query_db,
                instantiated,
                app_type,
            );
        }

        // Evaluate the instantiated body before returning. When the distributive
        // conditional path in TypeInstantiator distributes a union-typed parameter
        // over conditional branches, it produces a union of unevaluated Conditional
        // nodes. Those Conditionals must be evaluated here so the SubtypeChecker
        // sees concrete types (tuples, objects, etc.) rather than structural
        // Conditional nodes that it cannot directly compare to source types.
        let evaluated = self.evaluate_type(instantiated);
        Some(if evaluated != instantiated {
            evaluated
        } else {
            instantiated
        })
    }
}
