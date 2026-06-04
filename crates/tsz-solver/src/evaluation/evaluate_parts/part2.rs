impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Record display-alias provenance after a successful application
    /// evaluation.
    ///
    /// Decides whether to repaint the alias name onto the evaluated
    /// structural form. Skipping the repaint protects unrelated diagnostics
    /// from being relabeled when:
    /// * the result is a non-empty structural shape that already existed
    ///   before this application,
    /// * the result is itself one of the application arguments,
    /// * a conditional branch alias is already pinned on `result`.
    ///
    /// When `my_apparent_branch` is set by the conditional evaluator and is
    /// distinct from the original application, also installs a one-step
    /// forward alias so the formatter shows the apparent intermediate name
    /// (e.g. `DeepReadonlyObject<Part>` instead of `DeepReadonly<Part>`).
    fn record_application_evaluation_display_aliases(
        &mut self,
        result: TypeId,
        original_type_id: TypeId,
        original_args: &[TypeId],
        is_type_alias_def: bool,
        prefer_application_display_alias: bool,
        my_apparent_branch: Option<TypeId>,
    ) {
        let display_origin = if self.expand_application_display_alias_args
            && let Some(TypeData::Application(original_app_id)) =
                self.interner.lookup(original_type_id)
        {
            let original_app = self.interner.type_application(original_app_id);
            let expanded_args = self.expand_type_args(&original_app.args);
            if expanded_args.as_ref() != original_app.args.as_slice() {
                let candidate = self
                    .interner
                    .application(original_app.base, expanded_args.into_owned());
                if crate::visitor::contains_type_by_id(self.interner, candidate, result) {
                    original_type_id
                } else {
                    candidate
                }
            } else {
                original_type_id
            }
        } else {
            original_type_id
        };
        let has_param_args = original_args.iter().any(|&arg| {
            crate::type_queries::contains_generic_type_parameters_db(self.interner, arg)
        });
        // For concrete args the alias repaint is unconditional; for
        // generic args only Conditional/IndexAccess/Mapped results get
        // repainted (deferred mapped aliases retain the as-written
        // relationship needed for diagnostics like `Mapped<K>[Remapped<K>]`).
        if has_param_args
            && !matches!(
                self.interner.lookup(result),
                Some(
                    crate::types::TypeData::Conditional(_)
                        | crate::types::TypeData::IndexAccess(_, _)
                        | crate::types::TypeData::Mapped(_)
                )
            )
        {
            return;
        }

        let result_is_non_empty_structural = match self.interner.lookup(result) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                !shape.properties.is_empty()
                    || shape.string_index.is_some()
                    || shape.number_index.is_some()
            }
            Some(TypeData::Intersection(_)) => true,
            _ => false,
        };
        let result_is_application_arg = original_args.contains(&result);
        let skip_type_alias_repaint = matches!(
            self.interner.lookup(display_origin),
            Some(TypeData::Application(_))
        ) && result_is_non_empty_structural
            && (result_is_application_arg
                || (is_type_alias_def
                    && match (
                        self.interner.lookup_alloc_order(result),
                        self.interner.lookup_alloc_order(display_origin),
                    ) {
                        (Some(result_order), Some(display_order)) => result_order <= display_order,
                        _ => result.0 <= display_origin.0,
                    }));
        let keep_existing_conditional_branch_alias = is_type_alias_def
            && !prefer_application_display_alias
            && matches!(
                self.interner.lookup(display_origin),
                Some(TypeData::Application(_))
            )
            && display_provenance::display_alias(self.interner, result).is_some();
        if !skip_type_alias_repaint && !keep_existing_conditional_branch_alias {
            let priority = if prefer_application_display_alias
                || (self.expand_application_display_alias_args
                    && matches!(
                        self.interner.lookup(display_origin),
                        Some(TypeData::Application(_))
                    )) {
                AliasApplicationPriority::PreferApplication
            } else {
                AliasApplicationPriority::PreserveExisting
            };
            display_provenance::record_alias_application(
                self.interner,
                AliasApplicationProvenance {
                    evaluated: result,
                    application: display_origin,
                },
                priority,
            );
        }

        // If the conditional branch resolved to an intermediate
        // Application (e.g. `DeepReadonly<Part>` -> conditional ->
        // `DeepReadonlyObject<Part>`), store a forward display alias so
        // the formatter shows the one-step apparent type name that tsc
        // displays.
        if let Some(branch_app) = my_apparent_branch
            && branch_app != original_type_id
            && branch_app != result
            && !has_param_args
            && matches!(
                self.interner.lookup(branch_app),
                Some(crate::types::TypeData::Application(_))
            )
        {
            display_provenance::record_alias_application(
                self.interner,
                AliasApplicationProvenance {
                    evaluated: original_type_id,
                    application: branch_app,
                },
                AliasApplicationPriority::PreserveExisting,
            );
        }
    }

    fn store_intermediate_application_display_alias(
        &self,
        instantiated: TypeId,
        original_type_id: TypeId,
        evaluated: TypeId,
        original_args: &[TypeId],
    ) {
        if instantiated == original_type_id || evaluated == TypeId::ERROR {
            return;
        }
        // Only install this forward alias when the intermediate application
        // appears to have been introduced after the outer application.
        // If the instantiated application predates the outer one, it can be a
        // user-authored type occurrence and globally aliasing it risks repainting
        // unrelated diagnostics.
        let instantiated_is_new_intermediate = match (
            self.interner.lookup_alloc_order(instantiated),
            self.interner.lookup_alloc_order(original_type_id),
        ) {
            (Some(instantiated_order), Some(original_order)) => instantiated_order > original_order,
            _ => instantiated.0 > original_type_id.0,
        };
        if !instantiated_is_new_intermediate {
            return;
        }
        let instantiated_is_application = matches!(
            self.interner.lookup(instantiated),
            Some(TypeData::Application(_))
        );
        let original_is_application = matches!(
            self.interner.lookup(original_type_id),
            Some(TypeData::Application(_))
        );

        if !original_is_application {
            return;
        }

        if !instantiated_is_application {
            // Structural-body path: the type alias body resolved to a structural
            // type rather than another Application (e.g.
            // `type LinkedList<T> = T & { next: LinkedList<T> }` evaluates to an
            // Intersection). Map `evaluated → original_type_id` so diagnostics show
            // the alias name instead of the expanded structural form.
            //
            // `evaluated_is_mapped` is checked first: Mapped is a subset of structural,
            // so true short-circuits the more expensive `is_structural_display_alias_result`
            // call and avoids a duplicate `lookup(evaluated)`.
            let evaluated_is_mapped =
                matches!(self.interner.lookup(evaluated), Some(TypeData::Mapped(_)));
            if evaluated_is_mapped
                || Self::is_structural_display_alias_result(self.interner, evaluated)
            {
                // Only store the display alias when `evaluated` was freshly produced
                // by this evaluation (allocated after `original_type_id`). If it
                // pre-exists, it was already interned by a different alias and
                // overwriting its alias would corrupt diagnostics for that other alias.
                // For example, `NestedRecord<"x.y.z", string>` and `Id<...string...>`
                // can evaluate to the same structural object; the NestedRecord evaluation
                // must not replace the `Id<...>` alias that was recorded first.
                let evaluated_is_fresh = match (
                    self.interner.lookup_alloc_order(evaluated),
                    self.interner.lookup_alloc_order(original_type_id),
                ) {
                    (Some(eval_order), Some(orig_order)) => eval_order > orig_order,
                    _ => evaluated.0 > original_type_id.0,
                };
                // Safe to store in two cases:
                // 1. Recursive aliases: the recursive self-reference ensures the structural
                //    type is unique to this instantiation, so aliasing is unambiguous.
                // 2. Generic aliases whose body evaluates to a fresh Mapped type: each
                //    distinct set of type-argument TypeIds produces a distinct MappedType
                //    node (the constraint is baked into the interned key). Storing the
                //    alias lets diagnostics show e.g. `Mapped2<K>` instead of the
                //    expanded `{ [P in K as \`get${P}\`]: ... }` form, matching tsc.
                if evaluated_is_fresh
                    && (evaluated_is_mapped
                        || self.is_recursive_type_alias_application(original_type_id))
                {
                    self.interner
                        .store_display_alias_preferring_application(evaluated, original_type_id);
                }
            }
            return;
        }

        // Application→Application chain: when the outer application's args contain
        // generic type parameters, skip storing the alias. Intermediate Applications
        // in a type-alias chain (e.g. `Outer<T>` instantiated to `Inner<T>`) must not
        // displace the outer Application as the canonical display alias.
        if original_args.iter().any(|&arg| {
            crate::type_queries::contains_generic_type_parameters_db(self.interner, arg)
        }) {
            return;
        }

        if !Self::is_structural_display_alias_result(self.interner, evaluated) {
            return;
        }

        display_provenance::record_alias_application(
            self.interner,
            AliasApplicationProvenance {
                evaluated: instantiated,
                application: original_type_id,
            },
            AliasApplicationPriority::PreferApplication,
        );
    }

    fn is_recursive_type_alias_application(&self, type_id: TypeId) -> bool {
        let Some(TypeData::Application(app_id)) = self.interner.lookup(type_id) else {
            return false;
        };
        let app = self.interner.type_application(app_id);
        let Some(TypeData::Lazy(def_id)) = self.interner.lookup(app.base) else {
            return false;
        };
        if self.resolver.get_def_kind(def_id) != Some(DefKind::TypeAlias) {
            return false;
        }
        let Some(body) = self.resolver.resolve_lazy(def_id, self.interner) else {
            return false;
        };
        let mut visited = FxHashSet::default();
        self.type_reaches_alias_def(body, def_id, &mut visited)
    }

    fn type_reaches_alias_def(
        &self,
        type_id: TypeId,
        target_def_id: DefId,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if type_id.is_intrinsic() || !visited.insert(type_id) {
            return false;
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::Lazy(def_id))
                if self.resolver.defs_are_equivalent(def_id, target_def_id) =>
            {
                return true;
            }
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                if let Some(TypeData::Lazy(def_id)) = self.interner.lookup(app.base)
                    && self.resolver.defs_are_equivalent(def_id, target_def_id)
                {
                    return true;
                }
            }
            _ => {}
        }

        let mut found = false;
        crate::visitor::for_each_child_by_id(self.interner, type_id, |child| {
            if !found {
                found = self.type_reaches_alias_def(child, target_def_id, visited);
            }
        });
        found
    }

    /// Record a back-reference from an evaluated structural form to its
    /// originating parametric Application — the interface/class counterpart
    /// to `store_intermediate_application_display_alias` (which only stores
    /// for type-alias bodies that are themselves Applications).
    ///
    /// Read by `reduce_alias_body_to_application_form` to recover the
    /// Application form when the source has been eagerly evaluated to its
    /// structural shape (e.g. `Promise<{id}>` substituted into a structural
    /// Object). The downstream `store_display_alias_preferring_application`
    /// applies its own safety gates (alloc-order, intrinsic-skip, generic-
    /// args) that prevent overriding aliases for pre-existing types.
    fn store_parametric_structural_back_reference(
        &mut self,
        evaluated: TypeId,
        original_type_id: TypeId,
    ) {
        if evaluated == original_type_id || evaluated == TypeId::ERROR {
            return;
        }
        let Some(TypeData::Application(app_id)) = self.interner.lookup(original_type_id) else {
            return;
        };
        let app = self.interner.type_application(app_id);
        if app.args.is_empty() {
            return;
        }
        let app_def = match self.interner.lookup(app.base) {
            Some(TypeData::Lazy(def_id)) => self
                .resolver
                .get_def_kind(def_id)
                .map(|kind| (def_id, kind)),
            Some(TypeData::TypeQuery(sym_ref)) => {
                self.resolver.symbol_to_def_id(sym_ref).and_then(|def_id| {
                    self.resolver
                        .get_def_kind(def_id)
                        .map(|kind| (def_id, kind))
                })
            }
            _ => None,
        };
        let Some((_, app_kind)) = app_def else {
            return;
        };
        // This back-reference is for nominal parametric shapes. Type-alias
        // applications still need their evaluated structural form for displays
        // such as TS2339 on conditional helper aliases. If the resolver cannot
        // prove a nominal interface/class origin, do not repaint a structural
        // result as an arbitrary application.
        if !matches!(
            app_kind,
            crate::def::DefKind::Interface | crate::def::DefKind::Class
        ) {
            return;
        }
        if app.args.contains(&evaluated) {
            return;
        }
        // Fast path: all-intrinsic args trivially have no free type
        // parameters; skip the recursive `contains_generic_type_parameters_db`
        // traversal that fires on every parametric application evaluation.
        let all_intrinsic = app.args.iter().all(|a| a.is_intrinsic());
        if !all_intrinsic
            && app.args.iter().any(|&arg| {
                crate::type_queries::contains_generic_type_parameters_db(self.interner, arg)
            })
        {
            return;
        }
        if !Self::is_structural_display_alias_result(self.interner, evaluated) {
            return;
        }
        display_provenance::record_alias_application(
            self.interner,
            AliasApplicationProvenance {
                evaluated,
                application: original_type_id,
            },
            AliasApplicationPriority::PreferApplication,
        );
    }

    fn is_structural_display_alias_result(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
        matches!(
            interner.lookup(type_id),
            Some(
                TypeData::Object(_)
                    | TypeData::ObjectWithIndex(_)
                    | TypeData::Array(_)
                    | TypeData::Tuple(_)
                    | TypeData::Function(_)
                    | TypeData::Callable(_)
                    | TypeData::Intersection(_)
                    | TypeData::Mapped(_)
            )
        )
    }

    // Additional evaluator support methods live in the nested support module.
}
