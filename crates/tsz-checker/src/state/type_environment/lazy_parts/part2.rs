impl<'a> CheckerState<'a> {
    pub(crate) fn ensure_application_symbols_resolved_inner(
        &mut self,
        type_id: TypeId,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) -> bool {
        let mut fully_resolved = true;

        // Use a worklist so we resolve dependencies transitively, including
        // definitions discovered while traversing lazily resolved references.
        let mut worklist: Vec<TypeId> = vec![type_id];
        let mut seen_types: rustc_hash::FxHashSet<TypeId> = rustc_hash::FxHashSet::default();
        let mut seen_def_ids: rustc_hash::FxHashSet<tsz_solver::DefId> =
            rustc_hash::FxHashSet::default();
        let mut seen_type_queries: rustc_hash::FxHashSet<tsz_solver::SymbolRef> =
            rustc_hash::FxHashSet::default();
        let mut resolved_types: rustc_hash::FxHashSet<TypeId> = rustc_hash::FxHashSet::default();

        while let Some(current) = worklist.pop() {
            // Check global fuel - bail if exhausted (prevents unbounded work
            // on deeply-nested generic type graphs like react16.d.ts).
            if APP_SYMBOL_RESOLUTION_FUEL.get() >= MAX_APP_SYMBOL_RESOLUTION_FUEL {
                fully_resolved = false;
                break;
            }

            if !seen_types.insert(current) {
                continue;
            }

            // Skip types already resolved in a previous call — their transitive
            // dependencies are guaranteed to be resolved too.  Without this,
            // deeply-nested Application chains (e.g., 50-deep `merge(merge(…))`)
            // cause O(N²) re-traversal of already-resolved intermediate types.
            if self.ctx.application_symbols_resolved.contains(&current) {
                resolved_types.insert(current);
                continue;
            }

            resolved_types.insert(current);

            for_each_direct_referenced_type(self.ctx.types, current, |next| {
                worklist.push(next);
            });

            if let Some(def_id) = lazy_def_id(self.ctx.types, current) {
                if !seen_def_ids.insert(def_id) {
                    continue;
                }

                // Consume fuel for each DefId resolution (the expensive part)
                APP_SYMBOL_RESOLUTION_FUEL.set(APP_SYMBOL_RESOLUTION_FUEL.get() + 1);
                increment_global_resolution_fuel();
                if global_resolution_fuel_exhausted() {
                    fully_resolved = false;
                    break;
                }

                match self.resolve_lazy_def_for_type_env(def_id) {
                    Some((inserted, resolved)) => {
                        fully_resolved &= inserted;
                        if resolved != TypeId::ANY && resolved != TypeId::ERROR {
                            worklist.push(resolved);
                        }
                    }
                    None => {
                        fully_resolved = false;
                    }
                }
            } else if let Some(def_id) = enum_def_id(self.ctx.types, current) {
                if !seen_def_ids.insert(def_id) {
                    continue;
                }

                // Consume fuel for enum resolution too
                APP_SYMBOL_RESOLUTION_FUEL.set(APP_SYMBOL_RESOLUTION_FUEL.get() + 1);
                increment_global_resolution_fuel();
                if global_resolution_fuel_exhausted() {
                    fully_resolved = false;
                    break;
                }

                match self.resolve_enum_def_for_type_env(def_id) {
                    Some((inserted, resolved)) => {
                        fully_resolved &= inserted;
                        if resolved != TypeId::ANY && resolved != TypeId::ERROR {
                            worklist.push(resolved);
                        }
                    }
                    None => {
                        fully_resolved = false;
                    }
                }
            } else if let Some(symbol_ref) = get_type_query_symbol_ref(self.ctx.types, current) {
                if !seen_type_queries.insert(symbol_ref) {
                    continue;
                }

                let sym_id = SymbolId(symbol_ref.0);
                let symbol = self.ctx.binder.get_symbol(sym_id);
                if symbol.is_none() {
                    continue;
                }

                // TypeQuery represents `typeof X` — a value-space query.
                // If the symbol is already registered in the environment (e.g.,
                // as a class constructor type from get_type_of_symbol), skip
                // re-resolution. type_reference_symbol_type returns the TYPE-space
                // result (instance type for classes), which would incorrectly
                // overwrite the VALUE-space result (constructor type) needed by
                // typeof expressions.
                if let Ok(env) = self.ctx.type_env.try_borrow()
                    && env.contains(tsz_solver::SymbolRef(sym_id.0))
                {
                    continue;
                }

                // Consume fuel for type query resolution
                APP_SYMBOL_RESOLUTION_FUEL.set(APP_SYMBOL_RESOLUTION_FUEL.get() + 1);
                increment_global_resolution_fuel();
                if global_resolution_fuel_exhausted() {
                    fully_resolved = false;
                    break;
                }

                let resolved = if symbol.as_ref().is_some_and(|s| {
                    s.has_any_flags(symbol_flags::TYPE_ALIAS | symbol_flags::VARIABLE)
                }) {
                    let value_decl = symbol
                        .map(|s| s.value_declaration)
                        .unwrap_or(tsz_parser::NodeIndex::NONE);
                    self.type_of_value_declaration_for_symbol(sym_id, value_decl)
                } else {
                    self.get_type_of_symbol(sym_id)
                };
                let inserted = self.insert_type_env_symbol(sym_id, resolved);
                fully_resolved &= inserted;
                if resolved != TypeId::ANY && resolved != TypeId::ERROR {
                    worklist.push(resolved);
                }
            }
        }

        if fully_resolved {
            visited.extend(resolved_types);
        }

        fully_resolved
    }

    fn resolve_lazy_def_for_type_env(
        &mut self,
        def_id: tsz_solver::DefId,
    ) -> Option<(bool, TypeId)> {
        if let Some((original_sym_id, owner_file_idx)) = self.ctx.def_symbol_identity(def_id) {
            if let Some(file_idx) = owner_file_idx
                && file_idx != self.ctx.current_file_idx
            {
                self.ctx
                    .register_symbol_file_target(original_sym_id, file_idx);
            }
            // For CLASS symbols, prefer the instance type over the constructor
            // type returned by get_type_of_symbol.  During class construction
            // (Phase 2 of get_class_instance_type_inner), symbol_instance_types
            // is not populated yet, but class_instance_type_cache holds the
            // partial instance type.  Without this, TypeEnvironment::resolve_lazy
            // returns the constructor type (Callable), causing false TS2339 on
            // property access for self-referential parameters (e.g. `p.x` where
            // `p: Point` inside class Point).
            // If the symbol is an import ALIAS whose target is a CLASS, follow
            // it to the actual target. This handles cross-file class references
            // in module augmentations where the DefId was created for the alias.
            // Only CLASS targets are followed to avoid interfering with type-only
            // exports and other alias semantics.
            let (sym_id, symbol, was_alias_resolved) = {
                let alias_target = self.ctx.resolve_import_alias_and_register(original_sym_id);
                if let Some(target) = alias_target {
                    let target_sym = self.get_cross_file_symbol(target);
                    let is_class_target = target_sym
                        .is_some_and(|s| s.has_any_flags(tsz_binder::symbol_flags::CLASS));
                    if is_class_target {
                        (target, target_sym, true)
                    } else {
                        (
                            original_sym_id,
                            self.get_cross_file_symbol(original_sym_id),
                            false,
                        )
                    }
                } else {
                    (
                        original_sym_id,
                        self.get_cross_file_symbol(original_sym_id),
                        false,
                    )
                }
            };
            let is_class = symbol.is_some_and(|s| s.has_any_flags(tsz_binder::symbol_flags::CLASS));
            let resolved = if let Some(symbol) = symbol
                && is_class
            {
                self.ctx
                    .symbol_instance_types
                    .get(&sym_id)
                    .copied()
                    .or_else(|| {
                        symbol
                            .primary_declaration()
                            .and_then(|idx| self.ctx.class_instance_type_cache.get(&idx).copied())
                    })
                    .unwrap_or_else(|| {
                        // Try building the instance type directly from the class symbol.
                        // With cross_file_symbol_targets registered by resolve_import_alias,
                        // this can delegate to a child checker with the correct arena.
                        if let Some(inst) = self.class_instance_type_from_symbol(sym_id) {
                            return inst;
                        }
                        let constructor = self.get_type_of_symbol(sym_id);
                        // Re-check: get_type_of_symbol may have populated
                        // symbol_instance_types as a side effect of class
                        // type computation. Prefer instance type over
                        // constructor for type-position references.
                        self.ctx
                            .symbol_instance_types
                            .get(&sym_id)
                            .copied()
                            .or_else(|| self.instance_type_from_constructor_type(constructor))
                            .unwrap_or(constructor)
                    })
            } else {
                self.get_type_of_symbol(sym_id)
            };
            let inserted = self.insert_type_env_symbol(sym_id, resolved);

            // When import alias resolution remapped the symbol (e.g., ALIAS
            // SymbolId → CLASS SymbolId from another file), insert_type_env_symbol
            // registers under the CLASS symbol's DefId, not the original DefId from
            // the Lazy type. Register under the original def_id so Lazy(DefId)
            // resolves correctly during property access.
            if was_alias_resolved && let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
                if is_class {
                    env.insert_class_instance_type(def_id, resolved);
                }
                env.insert_def(def_id, resolved);
            }

            Some((inserted, resolved))
        } else {
            None
        }
    }

    fn resolve_enum_def_for_type_env(
        &mut self,
        def_id: tsz_solver::DefId,
    ) -> Option<(bool, TypeId)> {
        if let Some((sym_id, owner_file_idx)) = self.ctx.def_symbol_identity(def_id) {
            if let Some(file_idx) = owner_file_idx
                && file_idx != self.ctx.current_file_idx
            {
                self.ctx.register_symbol_file_target(sym_id, file_idx);
            }
            let resolved = self.type_reference_symbol_type(sym_id);
            let inserted = self.insert_type_env_symbol(sym_id, resolved);
            Some((inserted, resolved))
        } else {
            None
        }
    }
}
