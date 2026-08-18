use crate::query_boundaries::diagnostics as diagnostic_query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// The non-generic `TypeAlias` `DefId` registered against `ty`, if any.
    /// Prefers the raw alias body registration (`find_type_alias_by_body`) and
    /// falls back to the checker's evaluated-form registration
    /// (`find_def_for_type`); generic aliases (which need type-argument display)
    /// return `None`. The diagnostic formatter resolves an alias name through the
    /// same registration, so this is the canonical "does `ty` render as a bare
    /// non-generic alias name" query.
    pub(crate) fn registered_non_generic_type_alias_def(
        &self,
        ty: TypeId,
    ) -> Option<tsz_solver::def::DefId> {
        let def_id = self
            .ctx
            .definition_store
            .find_type_alias_by_body(ty)
            .or_else(|| {
                let def_id = self.ctx.definition_store.find_def_for_type(ty)?;
                let def = self.ctx.definition_store.get(def_id)?;
                (def.kind == tsz_solver::def::DefKind::TypeAlias).then_some(def_id)
            })?;
        let def = self.ctx.definition_store.get(def_id)?;
        def.type_params.is_empty().then_some(def_id)
    }

    /// Look up a displayable non-generic type alias name for a `TypeId`.
    pub(crate) fn lookup_type_alias_name_for_display(&self, ty: TypeId) -> Option<String> {
        // Only check composite types - tsc does NOT preserve alias names for
        // primitive types (number, string, etc.) or literal types.
        // Restricting to object/function/callable/union/intersection types avoids
        // regressions like `number` -> `TypeOfInfinity`.
        let is_object =
            crate::query_boundaries::diagnostics::object_shape_for_type(self.ctx.types, ty)
                .is_some();
        let is_union = if !is_object {
            crate::query_boundaries::diagnostics::union_members(self.ctx.types, ty).is_some()
        } else {
            false
        };
        let is_function = if !is_object && !is_union {
            crate::query_boundaries::diagnostics::function_shape_for_type(self.ctx.types, ty)
                .is_some()
                || crate::query_boundaries::diagnostics::callable_shape_for_type(self.ctx.types, ty)
                    .is_some()
        } else {
            false
        };
        if !is_object && !is_function && !is_union {
            return None;
        }

        // If the type has a display alias (produced by evaluating a generic
        // Application like B<string>), let the formatter handle it - using the
        // raw alias name would lose the type arguments.
        if self.ctx.types.get_display_alias(ty).is_some_and(|alias| {
            crate::query_boundaries::diagnostics::type_application(self.ctx.types, alias).is_some()
        }) {
            return None;
        }
        if let Some(alias) = self.ctx.types.get_display_alias(ty)
            && let Some(def_id) =
                crate::query_boundaries::diagnostics::lazy_def_id(self.ctx.types, alias)
            && let Some(def) = self.ctx.definition_store.get(def_id)
            && def.kind == tsz_solver::def::DefKind::TypeAlias
            && def.type_params.is_empty()
        {
            let name = self.ctx.types.resolve_atom_ref(def.name);
            if name.contains('<') {
                return Some(name.to_string());
            }
        }

        // For intersection types (e.g., typeof X & Function), expand to the full
        // type representation rather than using the alias name. This matches tsc's
        // behavior in assignability messages for complex intersection types.
        if crate::query_boundaries::diagnostics::intersection_members(self.ctx.types, ty).is_some()
        {
            return None;
        }

        if let Some(def_id) = self.ctx.definition_store.find_def_for_type(ty)
            && let Some(def) = self.ctx.definition_store.get(def_id)
            && def.kind != tsz_solver::def::DefKind::TypeAlias
            && !is_union
        {
            return None;
        }

        // Only a non-generic type alias registered against this `TypeId` is a
        // candidate; generic aliases need type-argument display (`B<string>`,
        // not `B`).
        let def_id = self.registered_non_generic_type_alias_def(ty)?;
        let def = self.ctx.definition_store.get(def_id)?;
        // `type T = typeof value` aliases display as the resolved value type
        // in assignment diagnostics. Do not repaint that resolved body as `T`.
        if def.body.is_some_and(|body| {
            crate::query_boundaries::diagnostics::is_type_query_type(self.ctx.types, body)
        }) || self.type_alias_definition_body_is_type_query(&def)
        {
            return None;
        }
        // Skip aliases whose body was computed by intersection reduction or
        // conditional evaluation. tsc shows the expanded form for these.
        if let Some(body) = def.body
            && self.ctx.definition_store.is_computed_body(body)
        {
            return None;
        }
        // Skip a non-generic alias whose tuple body was built by flattening a
        // fixed-tuple spread (`type T = [...[a, b], c]`); tsc stamps no
        // `aliasSymbol` on the spread result, so it renders `[a, b, c]`, not
        // `T`. Keyed per def because the flattened tuple shares its interned
        // `TypeId` with a directly-written `type T = [a, b, c]`.
        if self
            .ctx
            .definition_store
            .is_tuple_spread_flattened_alias(def_id)
        {
            return None;
        }
        let name = self.ctx.types.resolve_atom_ref(def.name);
        Some(name.to_string())
    }

    pub(crate) fn recursive_non_generic_alias_body_name(&self, ty: TypeId) -> String {
        self.recursive_non_generic_alias_body_display_name(ty)
            .unwrap_or_else(|| self.format_type_diagnostic(ty))
    }

    /// The alias *name* when `ty` is the registered body of a recursive
    /// non-generic type alias (`type Box2 = Box<Box2 | number>` → `Box2`);
    /// `None` for every other shape so the caller picks its own fallback.
    pub(crate) fn recursive_non_generic_alias_body_display_name(
        &self,
        ty: TypeId,
    ) -> Option<String> {
        crate::query_boundaries::recursive_alias::recursive_non_generic_type_alias_body_name(
            self.ctx.types.as_type_database(),
            &self.ctx.definition_store,
            ty,
        )
        .map(|name| self.ctx.types.resolve_atom_ref(name).to_string())
    }

    pub(in crate::error_reporter) fn compute_ambiguous_conditional_display(
        &mut self,
        ty: TypeId,
    ) -> Option<TypeId> {
        self.compute_ambiguous_conditional_display_inner(ty, true)
    }

    /// Core of [`Self::compute_ambiguous_conditional_display`], parameterized
    /// on whether an unconstrained named generic alias keeps its spelling.
    ///
    /// `keep_unconstrained_named_alias = true` is tsc's behavior when the
    /// caller has no target context (or the target is itself generic/deferred,
    /// e.g. `T95<U>` against `T94<U>`): the alias is preserved. A TS2322
    /// source display against a *concrete* target instead expands even the
    /// unconstrained case (`F<T>` against `number` shows `string | boolean`,
    /// see `deferred_conditional_source_branch_union_display`), so that caller
    /// passes `false` to reuse this same branch-union/determinism logic
    /// without the carve-out.
    pub(in crate::error_reporter) fn compute_ambiguous_conditional_display_inner(
        &mut self,
        ty: TypeId,
        keep_unconstrained_named_alias: bool,
    ) -> Option<TypeId> {
        let db = self.ctx.types.as_type_database();
        let cond = crate::query_boundaries::state::type_environment::get_conditional_type(db, ty)?;
        if !cond.is_distributive {
            return None;
        }
        let param_info =
            crate::query_boundaries::diagnostics::type_param_info(db, cond.check_type)?;
        let branches_are_concrete =
            !crate::query_boundaries::diagnostics::contains_type_parameters(db, cond.true_type)
                && !crate::query_boundaries::diagnostics::contains_type_parameters(
                    db,
                    cond.false_type,
                );
        if !branches_are_concrete {
            return None;
        }
        // A *named* generic conditional alias deferred on an UNCONSTRAINED type
        // parameter (`T95<U>` from `type T95<T> = T extends string ? boolean :
        // number`) keeps its alias spelling in tsc when the caller has no
        // concrete-target context. The branch union is shown for an anonymous
        // conditional, one whose check parameter carries a real constraint
        // that makes the branch genuinely ambiguous (`IsArray<T extends
        // object>` → `boolean`), or (per `keep_unconstrained_named_alias`)
        // whenever a TS2322-family caller has already established the paired
        // target is concrete.
        let check_param_unconstrained = param_info
            .constraint
            .is_none_or(|c| c == TypeId::UNKNOWN || c == TypeId::ANY);
        if keep_unconstrained_named_alias
            && check_param_unconstrained
            && self.ctx.types.get_display_alias(ty).is_some_and(|alias| {
                crate::query_boundaries::diagnostics::type_application(self.ctx.types, alias)
                    .is_some()
                    && crate::query_boundaries::diagnostics::contains_type_parameters(
                        self.ctx.types,
                        alias,
                    )
            })
        {
            return None;
        }
        let constraint = match param_info.constraint {
            Some(c) => c,
            None => {
                return Some(diagnostic_query::display_union_type(
                    self.ctx.types,
                    vec![cond.true_type, cond.false_type],
                ));
            }
        };
        if crate::query_boundaries::assignability::is_fresh_subtype_of(
            db,
            constraint,
            cond.extends_type,
        ) {
            return None;
        }
        let extends_members =
            crate::query_boundaries::diagnostics::union_members(db, cond.extends_type)
                .unwrap_or_else(|| vec![cond.extends_type].into());
        let has_overlap = extends_members.iter().any(|&m| {
            crate::query_boundaries::assignability::is_fresh_subtype_of(db, m, constraint)
        });
        if has_overlap {
            Some(diagnostic_query::display_union_type(
                self.ctx.types,
                vec![cond.true_type, cond.false_type],
            ))
        } else {
            None
        }
    }
}
