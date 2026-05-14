use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Returns true when `type_id` is a React library component alias type whose
    /// return-type check should be skipped to avoid cycle-detection false positives.
    ///
    /// The same logical alias can appear in several storage forms:
    /// - `Application { base: Lazy(def_id), args }` — explicit or defaulted type args
    /// - `Lazy(def_id)` — referenced without type args (lowering skipped defaults)
    /// - Evaluated union/object with a display alias or a `DefStore` entry naming the alias
    pub(in crate::checkers_domain::jsx) fn is_react_jsx_component_alias_application(
        &self,
        type_id: TypeId,
    ) -> bool {
        // Resolve to the DefId: try Application→Lazy, bare Lazy, display-alias→Lazy,
        // then fall back to the DefStore (covers evaluated unions registered under the alias).
        let display_alias = self.ctx.types.get_display_alias(type_id);
        let def_id =
            crate::query_boundaries::common::get_application_lazy_def_id(self.ctx.types, type_id)
                .or_else(|| {
                    display_alias.and_then(|alias| {
                        crate::query_boundaries::common::get_application_lazy_def_id(
                            self.ctx.types,
                            alias,
                        )
                    })
                })
                .or_else(|| crate::query_boundaries::common::lazy_def_id(self.ctx.types, type_id))
                .or_else(|| {
                    display_alias.and_then(|alias| {
                        crate::query_boundaries::common::lazy_def_id(self.ctx.types, alias)
                    })
                })
                .or_else(|| self.ctx.definition_store.find_def_for_type(type_id));
        let Some(def_id) = def_id else {
            return false;
        };
        let Some(name_atom) = self.ctx.definition_store.get_name(def_id) else {
            return false;
        };
        let name = self.ctx.types.resolve_atom_ref(name_atom);
        let is_react_component_alias_name = matches!(
            name.as_ref(),
            "ComponentType"
                | "ReactType"
                | "ComponentClass"
                | "StatelessComponent"
                | "FunctionComponent"
                | "SFC"
                | "PureComponent"
        );
        is_react_component_alias_name && self.react_component_alias_def_has_react_origin(def_id)
    }

    fn react_component_alias_def_has_react_origin(&self, def_id: tsz_solver::DefId) -> bool {
        if self
            .ctx
            .definition_store
            .get_symbol_id(def_id)
            .is_some_and(|raw| {
                let sym_id = tsz_binder::SymbolId(raw);
                self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
                    || self.symbol_parent_chain_contains_name(sym_id, "React")
            })
        {
            return true;
        }

        let display = self.format_type(self.ctx.types.factory().lazy(def_id));
        display.starts_with("React.")
    }

    fn symbol_parent_chain_contains_name(
        &self,
        mut sym_id: tsz_binder::SymbolId,
        name: &str,
    ) -> bool {
        let lib_binders = self.get_lib_binders();
        let mut depth = 0;
        while depth < 16 {
            let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
                return false;
            };
            let parent = symbol.parent;
            if parent.is_none() {
                return false;
            }
            if self
                .ctx
                .binder
                .get_symbol_with_libs(parent, &lib_binders)
                .is_some_and(|parent_symbol| parent_symbol.escaped_name == name)
            {
                return true;
            }
            sym_id = parent;
            depth += 1;
        }
        false
    }

    pub(super) fn is_react_jsx_component_alias_display(&self, type_id: TypeId) -> bool {
        let display = self.format_type(type_id);
        display == "React.ComponentType"
            || display == "ComponentType"
            || display == "React.ReactType"
            || display == "ReactType"
            || display.starts_with("React.ComponentType<")
            || display.starts_with("ComponentType<")
            || display.starts_with("React.ReactType<")
            || display.starts_with("ReactType<")
    }

    pub(super) fn is_react_jsx_component_branch_display(&self, type_id: TypeId) -> bool {
        let display = self.format_type(type_id);
        let base = display.split('<').next().unwrap_or(display.as_str());
        matches!(
            base,
            "React.ComponentClass"
                | "ComponentClass"
                | "React.FunctionComponent"
                | "FunctionComponent"
                | "React.StatelessComponent"
                | "StatelessComponent"
                | "React.SFC"
                | "SFC"
        )
    }

    pub(super) fn jsx_class_component_props_alias_hint(
        &self,
        instance_type: TypeId,
    ) -> Option<TypeId> {
        let app = crate::query_boundaries::common::type_application(self.ctx.types, instance_type)
            .or_else(|| {
                self.ctx
                    .types
                    .get_display_alias(instance_type)
                    .and_then(|alias| {
                        crate::query_boundaries::common::type_application(self.ctx.types, alias)
                    })
            })?;
        let &props_arg = app.args.first()?;
        crate::query_boundaries::common::type_has_displayable_name(self.ctx.types, props_arg)
            .then_some(props_arg)
    }

    pub(super) fn store_jsx_props_display_alias_if_matching(
        &mut self,
        props_type: TypeId,
        alias: TypeId,
    ) {
        if self.ctx.types.get_display_alias(props_type).is_some() {
            return;
        }
        let alias_evaluated = self.evaluate_type_with_env(alias);
        if alias_evaluated != TypeId::ERROR
            && self.is_assignable_to(alias_evaluated, props_type)
            && self.is_assignable_to(props_type, alias_evaluated)
        {
            self.ctx.types.store_display_alias(props_type, alias);
        }
    }

    pub(super) fn jsx_type_contains_callable_surface(&mut self, type_id: TypeId) -> bool {
        let mut stack = vec![type_id];
        let mut seen = rustc_hash::FxHashSet::default();
        while let Some(current) = stack.pop() {
            if !seen.insert(current) {
                continue;
            }
            let evaluated = self.evaluate_type_with_env(current);
            let resolved = self.resolve_type_for_property_access(evaluated);
            let resolved = self.resolve_lazy_type(resolved);
            if resolved != current {
                stack.push(resolved);
            }
            if crate::query_boundaries::common::function_shape_for_type(self.ctx.types, current)
                .is_some()
                || crate::query_boundaries::common::call_signatures_for_type(
                    self.ctx.types,
                    current,
                )
                .is_some_and(|sigs| !sigs.is_empty())
                || crate::query_boundaries::common::construct_signatures_for_type(
                    self.ctx.types,
                    current,
                )
                .is_some_and(|sigs| !sigs.is_empty())
            {
                return true;
            }
            if let Some(members) =
                crate::query_boundaries::common::intersection_members(self.ctx.types, current)
            {
                stack.extend(members);
            }
            if let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, current)
            {
                stack.extend(members);
            }
        }
        false
    }
}
