impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Main pattern matching function for infer types.
    ///
    /// Matches a source type against a pattern containing `infer` types,
    /// extracting the bound values into the bindings map.
    ///
    /// # Arguments
    /// * `source` - The concrete type to match against
    /// * `pattern` - The pattern type containing `infer` placeholders
    /// * `bindings` - Map to store extracted type bindings
    /// * `visited` - Set of already-visited type pairs (for cycle detection)
    /// * `checker` - Subtype checker for constraint validation
    ///
    /// # Returns
    /// `true` if the match succeeded and all bindings were extracted
    pub(crate) fn match_infer_pattern(
        &self,
        source: TypeId,
        pattern: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        if !visited.insert((source, pattern)) {
            return true;
        }

        if source == TypeId::NEVER {
            return self.bind_infer_defaults(pattern, TypeId::NEVER, bindings, checker);
        }

        if source == pattern {
            return true;
        }

        if let Some(TypeData::Union(members)) = self.interner().lookup(source) {
            let members = self.interner().type_list(members);
            return self
                .match_infer_pattern_union_members(&members, pattern, bindings, visited, checker);
        }

        let Some(pattern_key) = self.interner().lookup(pattern) else {
            return false;
        };

        match pattern_key {
            TypeData::Infer(info) => self.bind_infer(&info, source, bindings, checker),
            TypeData::Function(pattern_fn_id) => self.match_infer_function_pattern(
                source,
                pattern_fn_id,
                pattern,
                bindings,
                visited,
                checker,
            ),
            TypeData::Callable(pattern_shape_id) => self.match_infer_callable_pattern(
                source,
                pattern_shape_id,
                pattern,
                bindings,
                visited,
                checker,
            ),
            TypeData::Array(pattern_elem) => match self.interner().lookup(source) {
                Some(TypeData::Array(source_elem)) => {
                    self.match_infer_pattern(source_elem, pattern_elem, bindings, visited, checker)
                }
                Some(TypeData::Tuple(source_elems)) => {
                    // A tuple source matched against an array pattern `X[]` is
                    // a structural projection: every fixed element's type and
                    // every spread element's inner element type must satisfy
                    // `X`. Mirrors the residual matcher used by
                    // `match_tuple_elements`, so a tuple like
                    // `[boolean, ...number[]]` produced by residual reification
                    // can still pattern-match against `any[]`.
                    let source_elems = self.interner().tuple_list(source_elems);
                    self.match_residual_against_array_element(
                        &source_elems,
                        pattern_elem,
                        bindings,
                        visited,
                        checker,
                    )
                }
                // Union sources are caught by the top-level dispatch above and
                // routed through `match_infer_pattern_union_members`, so we
                // never reach here with `source = Union(...)`. Keep the match
                // arm explicit so a future change that loosens the top-level
                // catch still goes through the contravariance-aware helper
                // rather than a naive `union2` merge.
                Some(TypeData::Union(members)) => {
                    let members = self.interner().type_list(members);
                    self.match_infer_pattern_union_members(
                        &members, pattern, bindings, visited, checker,
                    )
                }
                _ => false,
            },
            TypeData::Tuple(pattern_elems) => match self.interner().lookup(source) {
                Some(TypeData::Tuple(source_elems)) => {
                    let source_elems = self.interner().tuple_list(source_elems);
                    let pattern_elems = self.interner().tuple_list(pattern_elems);
                    self.match_tuple_elements(
                        &source_elems,
                        &pattern_elems,
                        bindings,
                        visited,
                        checker,
                    )
                }
                // See note above: union sources are routed via the top-level
                // helper to keep merge semantics uniform.
                Some(TypeData::Union(members)) => {
                    let members = self.interner().type_list(members);
                    self.match_infer_pattern_union_members(
                        &members, pattern, bindings, visited, checker,
                    )
                }
                _ => false,
            },
            TypeData::ReadonlyType(pattern_inner) => {
                let source_inner = match self.interner().lookup(source) {
                    Some(TypeData::ReadonlyType(inner)) => inner,
                    _ => source,
                };
                self.match_infer_pattern(source_inner, pattern_inner, bindings, visited, checker)
            }
            TypeData::NoInfer(pattern_inner) => {
                // NoInfer<T> matches if source matches T (strip wrapper)
                let source_inner = match self.interner().lookup(source) {
                    Some(TypeData::NoInfer(inner)) => inner,
                    _ => source,
                };
                self.match_infer_pattern(source_inner, pattern_inner, bindings, visited, checker)
            }
            TypeData::Object(pattern_shape_id) => self.match_infer_object_pattern(
                source,
                pattern_shape_id,
                pattern,
                bindings,
                visited,
                checker,
            ),
            TypeData::ObjectWithIndex(pattern_shape_id) => self
                .match_infer_object_with_index_pattern(
                    source,
                    pattern_shape_id,
                    pattern,
                    bindings,
                    visited,
                    checker,
                ),
            TypeData::Application(pattern_app_id) => {
                // Declaration-level match: walk `source` through one-step
                // alias-application peeling until its base aligns with the
                // pattern's base. Handles `Cond<RHS>` where `RHS = ToPromise<X>`
                // and `ToPromise<X> = Promise<X>` by reducing the source
                // `Application(ToPromise, [X])` to `Application(Promise, [X])`
                // before matching `Application(Promise, [infer Y])`.
                let pattern_app = self.interner().type_application(pattern_app_id);
                if pattern_app.args.len() == 1
                    && let Some(TypeData::Lazy(def_id)) = self.interner().lookup(pattern_app.base)
                    && self.resolver().is_builtin_readonly_array_def(def_id)
                    && let Some(source_elem) =
                        crate::type_queries::get_array_element_type(self.interner(), source)
                {
                    return self.match_infer_pattern(
                        source_elem,
                        pattern_app.args[0],
                        bindings,
                        visited,
                        checker,
                    );
                }
                let mut current_source = source;
                for _ in 0..Self::MAX_ALIAS_REDUCTION_STEPS {
                    if let Some(TypeData::Application(source_app_id)) =
                        self.interner().lookup(current_source)
                    {
                        let source_app = self.interner().type_application(source_app_id);
                        if let Some(result) = self.try_match_application_args_to_pattern(
                            &source_app,
                            &pattern_app,
                            bindings,
                            visited,
                            checker,
                        ) {
                            return result;
                        }
                        if source_app.args.len() == pattern_app.args.len() {
                            let candidate_pattern = self
                                .interner()
                                .application(pattern_app.base, source_app.args.clone());
                            if checker.is_subtype_of(current_source, candidate_pattern) {
                                for (source_arg, pattern_arg) in
                                    source_app.args.iter().zip(pattern_app.args.iter())
                                {
                                    if !self.match_infer_pattern(
                                        *source_arg,
                                        *pattern_arg,
                                        bindings,
                                        visited,
                                        checker,
                                    ) {
                                        return false;
                                    }
                                }
                                return true;
                            }
                        }
                    }
                    let Some(peeled) = self.peel_alias_application(current_source) else {
                        break;
                    };
                    current_source = peeled;
                }

                // Source may have been evaluated from Application(Promise,[T]) to Object before
                // reaching this point; display_alias records the original Application for recovery.
                if let Some(recovered) = self.try_recover_application_from_display_alias(source)
                    && let Some(TypeData::Application(recovered_app_id)) =
                        self.interner().lookup(recovered)
                {
                    let recovered_app = self.interner().type_application(recovered_app_id);
                    if let Some(result) = self.try_match_application_args_to_pattern(
                        &recovered_app,
                        &pattern_app,
                        bindings,
                        visited,
                        checker,
                    ) {
                        return result;
                    }
                }

                // Fallback: Structural expansion
                // Expand the pattern Application to its structural form and recurse
                // This handles cases like: Reducer<infer S> matching a structural function type
                let expanded_pattern = self.evaluate_for_infer_match(pattern);

                // Only recurse if expansion actually changed the type
                if expanded_pattern != pattern {
                    if let Some(alias) = self.interner().get_display_alias(source)
                        && alias != source
                    {
                        let mut alias_bindings = bindings.clone();
                        let mut alias_visited = visited.clone();
                        if self.match_infer_pattern(
                            alias,
                            expanded_pattern,
                            &mut alias_bindings,
                            &mut alias_visited,
                            checker,
                        ) {
                            *bindings = alias_bindings;
                            return true;
                        }
                    }
                    return self.match_infer_pattern(
                        source,
                        expanded_pattern,
                        bindings,
                        visited,
                        checker,
                    );
                }

                false
            }
            TypeData::TemplateLiteral(pattern_spans_id) => {
                let pattern_spans = self.interner().template_list(pattern_spans_id);
                match self.interner().lookup(source) {
                    Some(TypeData::Literal(LiteralValue::String(atom))) => {
                        let source_text = self.interner().resolve_atom_ref(atom);
                        self.match_template_literal_string(
                            source_text.as_ref(),
                            pattern_spans.as_ref(),
                            bindings,
                            checker,
                        )
                    }
                    Some(TypeData::TemplateLiteral(source_spans_id)) => {
                        let source_spans = self.interner().template_list(source_spans_id);
                        self.match_template_literal_spans(
                            source,
                            source_spans.as_ref(),
                            pattern_spans.as_ref(),
                            bindings,
                            checker,
                        )
                    }
                    // Primitive string does not match template literal patterns; tsc takes the false branch.
                    _ => false,
                }
            }
            // Handle union pattern containing infer types
            // Pattern: infer S | T | U where S is infer and T, U are not
            // Source: A | T | U or a single type A
            // Algorithm: Match source members against non-infer pattern members,
            // then bind the infer to the remaining source members
            TypeData::Union(pattern_members) => {
                let members = self.interner().type_list(pattern_members);
                if members.iter().any(|&member| {
                    !matches!(self.interner().lookup(member), Some(TypeData::Infer(_)))
                        && self.type_contains_infer(member)
                }) {
                    for &member in members.iter() {
                        let mut local_bindings = bindings.clone();
                        let mut local_visited = FxHashSet::default();
                        if self.match_infer_pattern(
                            source,
                            member,
                            &mut local_bindings,
                            &mut local_visited,
                            checker,
                        ) {
                            *bindings = local_bindings;
                            return true;
                        }
                    }
                    return false;
                }
                self.match_infer_union_pattern(source, pattern_members, pattern, bindings, checker)
            }
            _ => checker.is_subtype_of(source, pattern),
        }
    }
}
