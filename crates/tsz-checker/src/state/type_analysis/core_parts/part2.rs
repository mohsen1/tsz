impl<'a> CheckerState<'a> {
    fn get_type_of_symbol_inner(&mut self, sym_id: SymbolId) -> TypeId {
        use tsz_solver::SymbolRef;
        let factory = self.ctx.types.factory();
        self.record_symbol_dependency(sym_id);
        let cross_file_owner_idx = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .filter(|&file_idx| file_idx != self.ctx.current_file_idx);
        let use_local_symbol_state = cross_file_owner_idx.is_none();
        if let Some(file_idx) = cross_file_owner_idx
            && let Some((cached, _params)) = self
                .ctx
                .cached_cross_file_symbol_type(sym_id, file_idx as u32)
        {
            return cached;
        }

        // Check cache first
        if cross_file_owner_idx.is_none()
            && let Some(&cached) = self.ctx.symbol_types.get(&sym_id)
        {
            let cached_is_stale_alias_placeholder =
                !self.ctx.symbol_resolution_set.contains(&sym_id)
                    && crate::query_boundaries::common::lazy_def_id(self.ctx.types, cached)
                        == self.ctx.get_existing_def_id(sym_id)
                    && self
                        .ctx
                        .binder
                        .get_symbol(sym_id)
                        .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::TYPE_ALIAS));
            if cached_is_stale_alias_placeholder {
                self.ctx.symbol_types.remove(&sym_id);
            } else {
                if cached == TypeId::ERROR && self.ctx.symbol_resolution_set.contains(&sym_id) {
                    // Pre-cache ANY sentinel to prevent re-entrancy: provisional_circular_function_symbol_type
                    // processes type annotations which may call get_type_of_symbol for the same symbol
                    // (e.g., `typeof foo<T>` in foo's own return type). Without this sentinel, the re-entrant
                    // call finds ERROR, detects circularity, and calls provisional again → stack overflow.
                    self.ctx.symbol_types.insert(sym_id, TypeId::ANY);
                    if let Some(provisional) =
                        self.provisional_circular_function_symbol_type(sym_id)
                    {
                        self.ctx.symbol_types.insert(sym_id, provisional);
                        trace!(
                            sym_id = sym_id.0,
                            type_id = provisional.0,
                            file = self.ctx.file_name.as_str(),
                            "(cached provisional) get_type_of_symbol"
                        );
                        tsz_common::perf_counters::record_compute_type_of_symbol_cache_hit();
                        return provisional;
                    }
                    // Restore ERROR if provisional failed
                    self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
                }
                let cached = self
                    .ctx
                    .symbol_types
                    .get(&sym_id)
                    .copied()
                    .unwrap_or(TypeId::ERROR);
                trace!(
                    sym_id = sym_id.0,
                    type_id = cached.0,
                    file = self.ctx.file_name.as_str(),
                    "(cached) get_type_of_symbol"
                );
                tsz_common::perf_counters::record_compute_type_of_symbol_cache_hit();
                return cached;
            }
        }

        // Check fuel - return ERROR if exhausted to prevent timeout
        if !self.ctx.consume_fuel() {
            // Cache ERROR so we don't keep trying to resolve this symbol
            if use_local_symbol_state {
                self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
            }
            return TypeId::ERROR;
        }

        // Check for circular reference
        if use_local_symbol_state && self.ctx.symbol_resolution_set.contains(&sym_id) {
            // Named entities use Lazy placeholders so circular dependencies can
            // defer evaluation; other symbols still return ERROR to avoid loops.
            let symbol = self.ctx.binder.get_symbol(sym_id);
            if let Some(symbol) = symbol {
                let flags = symbol.flags;
                if flags
                    & (symbol_flags::INTERFACE
                        | symbol_flags::CLASS
                        | symbol_flags::TYPE_ALIAS
                        | symbol_flags::ENUM
                        | symbol_flags::NAMESPACE_MODULE
                        | symbol_flags::VALUE_MODULE)
                    != 0
                {
                    if flags & symbol_flags::CLASS != 0
                        && let Some(partial) = self.circular_class_partial_constructor_type(sym_id)
                    {
                        return partial;
                    }
                    let def_id = self.ctx.get_or_create_def_id(sym_id);
                    let lazy_type = factory.lazy(def_id);
                    // Don't cache the Lazy type - we want to retry when the circular reference is broken
                    return lazy_type;
                }

                if flags & symbol_flags::FUNCTION != 0
                    && flags & symbol_flags::INTERFACE == 0
                    && let Some(provisional) =
                        self.provisional_circular_function_symbol_type(sym_id)
                {
                    self.ctx.symbol_types.insert(sym_id, provisional);
                    return provisional;
                }
            }

            // For non-named entities, cache ERROR to prevent repeated deep recursion
            // This is key for fixing timeout issues with circular class inheritance
            self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
            return TypeId::ERROR; // Circular reference - propagate error
        }

        // Check recursion depth to prevent stack overflow
        let depth = self.ctx.symbol_resolution_depth.get();
        if depth >= self.ctx.max_symbol_resolution_depth {
            // CRITICAL: Cache ERROR immediately to prevent repeated deep recursion
            if use_local_symbol_state {
                self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
            }
            return TypeId::ERROR; // Depth exceeded - prevent stack overflow
        }
        self.ctx.symbol_resolution_depth.set(depth + 1);

        // Push onto resolution stack
        if use_local_symbol_state {
            self.ctx.symbol_resolution_stack.push(sym_id);
            self.ctx.symbol_resolution_set.insert(sym_id);
        }

        // CRITICAL: Pre-cache a placeholder to break deep recursion chains
        // This prevents stack overflow in circular class inheritance by ensuring
        // that when we try to resolve this symbol again mid-resolution, we get
        // the cached value immediately instead of recursing deeper.
        // We'll overwrite this with the real result later (line 815).
        //
        // For named entities (Interface, Class, TypeAlias, Enum), use a Lazy type
        // as the placeholder instead of ERROR. This allows circular dependencies
        // like `interface User { filtered: Filtered } type Filtered = { [K in keyof User]: ... }`
        // to work correctly, since keyof Lazy(User) can defer evaluation instead of failing.
        if use_local_symbol_state {
            let symbol = self.ctx.binder.get_symbol(sym_id);
            let placeholder = if let Some(symbol) = symbol {
                let flags = symbol.flags;
                if flags
                    & (symbol_flags::INTERFACE
                        | symbol_flags::CLASS
                        | symbol_flags::TYPE_ALIAS
                        | symbol_flags::ENUM
                        | symbol_flags::NAMESPACE_MODULE
                        | symbol_flags::VALUE_MODULE)
                    != 0
                {
                    let def_id = self.ctx.get_or_create_def_id(sym_id);
                    factory.lazy(def_id)
                } else if flags & symbol_flags::FUNCTION != 0
                    && flags & symbol_flags::INTERFACE == 0
                {
                    // Pre-cache ANY sentinel to break re-entrancy during provisional computation.
                    // Without this, processing `typeof foo<T>` in foo's return type calls
                    // get_type_of_symbol(foo) which finds nothing cached → enters circular
                    // detection → calls provisional again → stack overflow.
                    self.ctx.symbol_types.insert(sym_id, TypeId::ANY);
                    self.provisional_circular_function_symbol_type(sym_id)
                        .unwrap_or(TypeId::ERROR)
                } else {
                    TypeId::ERROR
                }
            } else {
                TypeId::ERROR
            };
            trace!(
                sym_id = sym_id.0,
                placeholder = placeholder.0,
                is_lazy = lazy_def_id(self.ctx.types, placeholder).is_some(),
                file = self.ctx.file_name.as_str(),
                "get_type_of_symbol: inserted placeholder"
            );
            self.ctx.symbol_types.insert(sym_id, placeholder);
        }

        self.push_symbol_dependency(sym_id, true);
        let (result, type_params) = self.compute_type_of_symbol(sym_id);
        self.pop_symbol_dependency();

        // Pop from resolution stack
        if use_local_symbol_state {
            self.ctx.symbol_resolution_stack.pop();
            self.ctx.symbol_resolution_set.remove(&sym_id);
        }

        // Decrement recursion depth
        self.ctx
            .symbol_resolution_depth
            .set(self.ctx.symbol_resolution_depth.get() - 1);

        // Cache result.
        //
        // Guard against constructor type cache corruption from cycle-
        // fallback values: when an outer `get_class_constructor_type(C)`
        // is in progress and a nested `get_type_of_symbol(C)` arrives,
        // `compute_class_symbol_type` can observe a Lazy(DefId)
        // cycle-fallback and propagate it as `result`. That Lazy points
        // at the class's own DefId and resolves to the INSTANCE type —
        // caching it here would poison later value-position lookups of
        // the class (e.g. `C.staticProp` inside an instance method body)
        // and produce false TS2339. Instead, drop the placeholder so the
        // next lookup re-enters and observes the fully-built constructor
        // type after the outer resolution completes.
        let result_is_lazy_to_self = {
            use crate::query_boundaries::common as common_query;
            common_query::lazy_def_id(self.ctx.types.as_type_database(), result)
                .zip(self.ctx.get_existing_def_id(sym_id))
                .is_some_and(|(ld, od)| ld == od)
        };
        if let Some(file_idx) = cross_file_owner_idx {
            self.ctx.cache_cross_file_symbol_type(
                sym_id,
                file_idx as u32,
                result,
                type_params.clone(),
            );
        } else {
            let result_cached_locally = if result_is_lazy_to_self
                && self
                    .ctx
                    .binder
                    .get_symbol(sym_id)
                    .is_some_and(|s| s.has_any_flags(symbol_flags::CLASS))
            {
                self.ctx.symbol_types.remove(&sym_id);
                false
            } else {
                self.ctx.symbol_types.insert(sym_id, result);
                true
            };
            if result_cached_locally {
                self.cache_resolved_symbol_type_for_owner(sym_id, result);
            }
        }
        trace!(
            sym_id = sym_id.0,
            type_id = result.0,
            file = self.ctx.file_name.as_str(),
            "get_type_of_symbol"
        );

        // Also populate the type environment for Application expansion
        // IMPORTANT: We use the type_params returned by compute_type_of_symbol
        // because those are the same TypeIds used when lowering the type body.
        // Calling get_type_params_for_symbol would create fresh TypeIds that don't match.
        if use_local_symbol_state && result != TypeId::ANY && result != TypeId::ERROR {
            // For class symbols, we need to cache BOTH the constructor type (for value position)
            // and the instance type (for type position with typeof/TypeQuery resolution).
            let class_env_entry = self.ctx.binder.get_symbol(sym_id).and_then(|symbol| {
                if symbol.has_any_flags(symbol_flags::CLASS) {
                    self.class_instance_type_with_params_from_symbol(sym_id)
                } else {
                    None
                }
            });

            // Use try_borrow_mut to avoid panic if type_env is already borrowed.
            // This can happen during recursive type resolution (e.g., class inheritance).
            // If we can't borrow, skip the cache update - the type is still computed correctly.
            if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
                // Get the DefId if one exists (Phase 4.3 migration)
                let def_id = self.ctx.get_existing_def_id(sym_id);

                // For CLASS symbols:
                // - `result` is the constructor type (Callable with construct signatures)
                // - `instance_type` is the instance type (Object with properties)
                //
                // We cache the CONSTRUCTOR type in the type environment so that:
                // - `typeof Animal` resolves to the constructor type
                // - `Animal` used as a value resolves to the constructor type
                //
                // The instance type is still available via `class_instance_type_from_symbol`
                // for type position contexts where it's needed.
                if let Some((instance_type, _instance_params)) = &class_env_entry {
                    // This is a CLASS symbol - cache the constructor type (result)
                    // NOT the instance type. The instance type is used for class
                    // type position (e.g., `a: Animal`), not value position.
                    if type_params.is_empty() {
                        env.insert(SymbolRef(sym_id.0), result);
                        if let Some(def_id) = def_id {
                            env.insert_def(def_id, result);
                            // Also register the instance type so resolve_lazy returns it
                            // in type position (e.g., `{new(): Foo}` where Foo is a class)
                            env.insert_class_instance_type(def_id, *instance_type);
                        }
                    } else {
                        env.insert_with_params(SymbolRef(sym_id.0), result, type_params.clone());
                        if let Some(def_id) = def_id {
                            env.insert_def_with_params(def_id, result, type_params.clone());
                            // Also register the instance type for class
                            env.insert_class_instance_type(def_id, *instance_type);
                        }
                    }
                    // Register class extends relationship for nominal instanceof narrowing.
                    // Look up the parent class via InheritanceGraph (SymbolId-based) and
                    // convert to DefId so the solver can walk the extends chain.
                    // Must register in BOTH type environments (type_env for the evaluator
                    // and type_environment for the FlowAnalyzer's NarrowingContext).
                    if let Some(def_id) = def_id {
                        let parents = self.ctx.inheritance_graph.get_parents(sym_id);
                        if let Some(&parent_sym) = parents.first()
                            && let Some(parent_def_id) = self.ctx.get_existing_def_id(parent_sym)
                        {
                            env.register_class_extends(def_id, parent_def_id);
                            // Also register in type_environment so FlowAnalyzer sees it.
                            if let Ok(mut te) = self.ctx.type_environment.try_borrow_mut() {
                                te.register_class_extends(def_id, parent_def_id);
                            }
                        }
                    }
                } else if type_params.is_empty() {
                    // Check if resolve_lib_type_by_name already registered type params
                    // for this DefId. This happens for lib interfaces like Promise<T>,
                    // Array<T> where compute_type_of_symbol returns empty params but
                    // the lib resolution path registered them via ctx.insert_def_type_params.
                    let lib_params = def_id.and_then(|d| self.ctx.get_def_type_params(d));
                    if let Some(params) = lib_params {
                        env.insert_with_params(SymbolRef(sym_id.0), result, params.clone());
                        if let Some(def_id) = def_id {
                            env.insert_def_with_params(def_id, result, params);
                        }
                    } else {
                        env.insert(SymbolRef(sym_id.0), result);
                        if let Some(def_id) = def_id {
                            env.insert_def(def_id, result);
                        }
                    }
                } else {
                    env.insert_with_params(SymbolRef(sym_id.0), result, type_params.clone());
                    if let Some(def_id) = def_id {
                        env.insert_def_with_params(def_id, result, type_params.clone());
                    }
                }

                // Register numeric enums for Rule #7 (Open Numeric Enums)
                if let Some(def_id) = def_id {
                    self.maybe_register_numeric_enum(&mut env, sym_id, def_id);
                }

                // Register enum parent relationships for Task #17 (Enum Type Resolution)
                if let Some(def_id) = def_id
                    && let Some(symbol) = self.ctx.binder.symbols.get(sym_id)
                    && symbol.has_any_flags(symbol_flags::ENUM_MEMBER)
                {
                    let parent_sym_id = symbol.parent;
                    if let Some(parent_def_id) = self.ctx.get_existing_def_id(parent_sym_id) {
                        env.register_enum_parent(def_id, parent_def_id);
                    }
                }
            } else {
                let sym_name = self
                    .ctx
                    .binder
                    .get_symbol(sym_id)
                    .map_or("<unknown>", |s| s.escaped_name.as_str());
                tracing::warn!(
                    sym_id = sym_id.0,
                    sym_name = sym_name,
                    type_id = result.0,
                    type_params_count = type_params.len(),
                    "type_env try_borrow_mut FAILED - skipping insertion"
                );
            }

            // Mirror DefId mappings into type_environment (flow-analyzer env)
            // so both environments stay consistent. The type_env block above
            // handles SymbolRef + DefId writes to the evaluator env; this block
            // ensures the flow-analyzer env also has the DefId entries.
            if let Some(def_id) = self.ctx.get_existing_def_id(sym_id)
                && let Ok(mut env) = self.ctx.type_environment.try_borrow_mut()
            {
                if let Some((instance_type, _)) = &class_env_entry {
                    if type_params.is_empty() {
                        env.insert_def(def_id, result);
                    } else {
                        env.insert_def_with_params(def_id, result, type_params);
                    }
                    env.insert_class_instance_type(def_id, *instance_type);
                } else {
                    let lib_params = if type_params.is_empty() {
                        self.ctx.get_def_type_params(def_id)
                    } else {
                        None
                    };
                    if let Some(params) = lib_params {
                        env.insert_def_with_params(def_id, result, params);
                    } else if type_params.is_empty() {
                        env.insert_def(def_id, result);
                    } else {
                        env.insert_def_with_params(def_id, result, type_params);
                    }
                }
            }
            if class_env_entry.is_some()
                && let Some(def_id) = self.ctx.get_existing_def_id(sym_id)
            {
                // Register SymbolId <-> DefId mapping so resolve_type_query
                // can find the constructor type via DefId path.
                self.ctx.register_def_symbol_mapping_in_envs(def_id, sym_id);
            }

            // Register TypeId -> DefId reverse mapping for TYPE ALIASES only.
            // This enables diagnostics to display type alias names (e.g., "ExoticAnimal")
            // instead of structural expansions (e.g., "CatDog | ManBearPig | Platypus").
            //
            // Only type aliases need this: interfaces already get their names resolved
            // via ObjectShape.symbol in format_symbol_name, and registering interfaces
            // would cause false positives where inline types like `A | B` display
            // as a matching alias name instead of their structural form.
            //
            // Extract def_id before calling evaluate_type_with_env to avoid borrow
            // conflicts with symbol_to_def.
            let alias_def_id = self
                .ctx
                .symbol_to_def
                .borrow()
                .get(&sym_id)
                .copied()
                .filter(|_| {
                    self.ctx
                        .binder
                        .symbols
                        .get(sym_id)
                        .is_some_and(|s| s.has_any_flags(symbol_flags::TYPE_ALIAS))
                });
            if let Some(def_id) = alias_def_id {
                self.ctx
                    .definition_store
                    .register_type_to_def(result, def_id);
                self.ctx.definition_store.set_body(def_id, result);

                // Mark the body as "computed" when the declared alias body is a
                // non-generic reducing operator whose result tsc renders without
                // an `aliasSymbol`. `find_type_alias_by_body` and the diagnostic
                // formatters then show the underlying structural type rather than
                // the alias name. Generic aliases keep their name since the
                // operator is part of the definition, not a simplification.
                let body_is_computed = self
                    .ctx
                    .definition_store
                    .get(def_id)
                    .filter(|d| d.type_params.is_empty())
                    .and_then(|_| self.ctx.binder.get_symbol(sym_id))
                    .is_some_and(|symbol| {
                        symbol.declarations.iter().any(|&decl_idx| {
                            super::source_alias_attribution::alias_declaration_body_is_computed(
                                self.ctx.arena,
                                self.ctx.types,
                                decl_idx,
                                result,
                            )
                        })
                    });
                if body_is_computed {
                    self.ctx.definition_store.mark_body_as_computed(result);
                }
                // Also register the evaluated form of the type.
                // Type aliases with union/intersection bodies often contain Lazy
                // members (e.g., `type Exotic = CatDog | ManBearPig`). When these
                // are evaluated, the Lazy members resolve to concrete types,
                // producing a new TypeId.  Register this evaluated TypeId too so
                // diagnostic formatting can display the alias name regardless of
                // whether the raw or evaluated form is referenced.
                if !generic_query::contains_free_type_parameters(self.ctx.types, result)
                    && self.can_register_evaluated_alias_form(def_id, result)
                {
                    let evaluated = self.evaluate_type_with_env(result);
                    if evaluated != result {
                        self.ctx
                            .definition_store
                            .register_type_to_def(evaluated, def_id);
                        // A computed body keeps the same provenance after a second
                        // evaluation pass collapses its Lazy members: the evaluated
                        // form must also be skipped by `find_type_alias_by_body`,
                        // otherwise the reverse lookup repaints the alias name onto
                        // the shared structural result (e.g. a conditional that
                        // reduces to `{ a: 1; }`).
                        if body_is_computed {
                            self.ctx.definition_store.mark_body_as_computed(evaluated);
                        }
                    }
                }
            }
        }

        result
    }

    /// Resolve a `typeof X` type query with flow-sensitive narrowing.
    ///
    /// Delegates to [`get_type_from_type_query_flow_sensitive`] which resolves
    /// the expression type via `get_type_of_node` with control-flow narrowing
    /// enabled. Falls back to symbol-based resolution for edge cases.
    pub(crate) fn get_type_from_type_query(
        &mut self,
        idx: tsz_parser::parser::NodeIndex,
    ) -> tsz_solver::TypeId {
        self.get_type_from_type_query_with_request(idx, &TypingRequest::NONE)
    }

    pub(crate) fn get_type_from_type_query_with_request(
        &mut self,
        idx: tsz_parser::parser::NodeIndex,
        request: &TypingRequest,
    ) -> tsz_solver::TypeId {
        self.get_type_from_type_query_flow_sensitive_with_request(idx, request)
    }
}
