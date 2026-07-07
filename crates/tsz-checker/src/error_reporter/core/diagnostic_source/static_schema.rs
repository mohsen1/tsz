use crate::query_boundaries::diagnostics as diagnostic_query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn static_schema_array_structural_display(
        &mut self,
        array_type: TypeId,
        other: TypeId,
    ) -> Option<String> {
        if crate::query_boundaries::state::checking::is_type_parameter(self.ctx.types, array_type) {
            return None;
        }
        let format_peer =
            diagnostic_query::type_parameter_constraint(self.ctx.types, other).unwrap_or(other);
        let element_type = diagnostic_query::array_element_type(self.ctx.types, array_type)?;
        let array_display = self.format_type_diagnostic(array_type);
        if let Some(display) =
            self.static_schema_array_structural_display_text(&array_display, other)
        {
            return Some(display);
        }
        // Fast, deterministic path: reduce the already-resolved schema element
        // object in place, recursively rewriting its nested `Static<…>`
        // projection members, then render it with the object `display_alias`
        // suppressed. This operates on the resolved element shape, so it does not
        // re-type-check the schema's `const` value declaration the way the
        // re-resolution paths below do. Those paths re-evaluate enough of the
        // schema to exhaust the shared display work budget on the first of a
        // message's two renders and then truncate back to the bare alias
        // (`Input[]`) on the second, making the structural display
        // non-deterministic.
        //
        // The rewrite is itself recursion-depth bounded, so it runs with the
        // display budget suspended: the attempt must not spend the fuel the
        // re-resolution fallback below relies on when the shape-only rewrite
        // cannot fully reduce a member (it leaves a residual `Static<…>`), which
        // happens when the element type was resolved without the lib types the
        // full schema needs. Only the fully-reduced result is used here; a
        // residual one falls through to the re-resolution paths with the budget
        // intact.
        let fully_reduced = {
            let _suspend =
                crate::error_reporter::display_budget::SuspendDisplayBudgetScope::enter();
            self.rewrite_nested_static_projection_members(element_type, 0)
                .filter(|&reduced| !self.type_has_residual_static_schema(reduced, 0))
        };
        if let Some(reduced) = fully_reduced {
            let rebuilt = self.static_schema_array_display_type(reduced);
            return Some(
                self.format_type_for_assignability_message_skip_object_display_alias(rebuilt),
            );
        }
        if let Some(static_type) = self.static_schema_alias_element_structural_type(element_type) {
            return Some(self.format_static_schema_array_structural_type(static_type, format_peer));
        }
        let schema_type = self
            .static_schema_application_schema_type(element_type)
            .or_else(|| self.static_schema_alias_application_schema_type(element_type))?;
        let schema_type = self
            .static_schema_type_query_value_type(schema_type)
            .unwrap_or_else(|| self.resolve_type_query_type(schema_type));
        let schema_type = self.evaluate_type_for_assignability(schema_type);
        if let Some(static_type) = self.typebox_schema_static_type(schema_type, 0) {
            return Some(self.format_static_schema_array_structural_type(static_type, format_peer));
        }
        let evaluated_element = self.static_schema_element_structural_type(element_type)?;
        Some(self.format_static_schema_array_structural_type(evaluated_element, format_peer))
    }

    fn static_schema_alias_element_structural_type(
        &mut self,
        element_type: TypeId,
    ) -> Option<TypeId> {
        if let Some(static_type) = self.static_schema_alias_def_structural_type(
            crate::query_boundaries::common::lazy_def_id(self.ctx.types, element_type),
        ) {
            return Some(static_type);
        }
        if let Some(static_type) = self.static_schema_alias_def_structural_type(
            self.ctx.definition_store.find_def_for_type(element_type),
        ) {
            return Some(static_type);
        }
        if let Some(alias) = self.ctx.types.get_display_alias(element_type) {
            if self.is_static_schema_application(alias) {
                return self.static_schema_element_structural_type(alias);
            }
            if let Some(static_type) = self.static_schema_alias_def_structural_type(
                crate::query_boundaries::common::lazy_def_id(self.ctx.types, alias),
            ) {
                return Some(static_type);
            }
            if let Some(static_type) = self.static_schema_alias_def_structural_type(
                self.ctx.definition_store.find_def_for_type(alias),
            ) {
                return Some(static_type);
            }
        }
        None
    }

    fn alias_static_array_structural_display(
        &mut self,
        array_display: &str,
        format_peer: TypeId,
    ) -> Option<String> {
        let alias_name = array_display.strip_suffix("[]")?;
        if !alias_name
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        {
            return None;
        }
        let atom = self.ctx.types.intern_string(alias_name);
        if let Some(defs) = self.ctx.definition_store.find_defs_by_name(atom) {
            for def_id in defs {
                if let Some(static_type) =
                    self.static_schema_alias_def_structural_type(Some(def_id))
                {
                    return Some(
                        self.format_static_schema_array_structural_type(static_type, format_peer),
                    );
                }
            }
        }
        if let Some(display) =
            self.static_schema_value_name_array_structural_display(alias_name, format_peer)
        {
            return Some(display);
        }
        let sym_id = self.ctx.binder.file_locals.get(alias_name)?;
        let def_id = self.ctx.get_existing_def_id(sym_id)?;
        let static_type = self.static_schema_alias_def_structural_type(Some(def_id))?;
        Some(self.format_static_schema_array_structural_type(static_type, format_peer))
    }

    fn static_schema_alias_def_structural_type(
        &mut self,
        def_id: Option<tsz_solver::def::DefId>,
    ) -> Option<TypeId> {
        let def_id = def_id?;
        let def = self.ctx.definition_store.get(def_id)?;
        if def.kind != tsz_solver::def::DefKind::TypeAlias {
            return None;
        }
        let body = def.body?;
        self.is_static_schema_application(body)
            .then_some(body)
            .and_then(|body| self.static_schema_element_structural_type(body))
    }

    fn is_static_schema_application(&self, type_id: TypeId) -> bool {
        self.static_schema_application_schema_type(type_id)
            .is_some()
    }

    /// Whether `type_id` (or any nested object member, bounded by `depth`) is
    /// still an unreduced `Static<…>` projection application. Used to detect
    /// when the in-place shape rewrite under-reduced (so the caller falls back
    /// to the full re-resolution path) versus produced the fully structural
    /// form.
    fn type_has_residual_static_schema(&self, type_id: TypeId, depth: u8) -> bool {
        if depth > 12 {
            return false;
        }
        if self.is_static_schema_application(type_id) {
            return true;
        }
        diagnostic_query::object_shape_for_type(self.ctx.types, type_id).is_some_and(|shape| {
            shape
                .properties
                .iter()
                .any(|prop| self.type_has_residual_static_schema(prop.type_id, depth + 1))
        })
    }

    pub(crate) fn type_alias_projects_static_member(&self, base: TypeId) -> bool {
        let Some(def_id) = diagnostic_query::lazy_def_id(self.ctx.types, base) else {
            return false;
        };
        let Some(def) = self.ctx.definition_store.get(def_id) else {
            return false;
        };
        if def.kind != tsz_solver::def::DefKind::TypeAlias {
            return false;
        }
        let Some(body) = def.body else {
            return false;
        };
        let Some(indexed) = diagnostic_query::get_indexed_access_type(self.ctx.types, body) else {
            return false;
        };
        self.is_static_property_name(indexed.index_type)
    }

    fn is_static_property_name(&self, type_id: TypeId) -> bool {
        diagnostic_query::string_literal_value(self.ctx.types, type_id)
            .is_some_and(|name| self.ctx.types.resolve_atom_ref(name).as_ref() == "static")
    }

    fn static_schema_application_info(&self, type_id: TypeId) -> Option<(TypeId, Vec<TypeId>)> {
        let app_info =
            diagnostic_query::application_info(self.ctx.types, type_id).or_else(|| {
                let alias = self.ctx.types.get_display_alias(type_id)?;
                diagnostic_query::application_info(self.ctx.types, alias)
            })?;
        self.type_alias_projects_static_member(app_info.0)
            .then_some(app_info)
    }

    fn static_schema_element_structural_type(&mut self, element_type: TypeId) -> Option<TypeId> {
        use diagnostic_query::PropertyAccessResult;

        if let Some(schema_type) = self.static_schema_application_schema_type(element_type) {
            let schema_type = self
                .static_schema_type_query_value_type(schema_type)
                .unwrap_or_else(|| self.resolve_type_query_type(schema_type));
            let schema_type = self.evaluate_type_for_assignability(schema_type);
            if let Some(static_type) = self.typebox_schema_static_type(schema_type, 0) {
                return Some(static_type);
            }
            match self.resolve_property_access_with_env(schema_type, "static") {
                PropertyAccessResult::Success { type_id, .. }
                | PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type: Some(type_id),
                    ..
                } => {
                    let property_type = self.evaluate_type_with_env(type_id);
                    if diagnostic_query::object_shape_for_type(self.ctx.types, property_type)
                        .is_some()
                    {
                        return Some(property_type);
                    }
                }
                _ => {}
            }
        }

        let mut current = self.evaluate_type_for_assignability(element_type);
        for _ in 0..6 {
            if matches!(current, TypeId::ERROR | TypeId::UNKNOWN) {
                return None;
            }
            if diagnostic_query::object_shape_for_type(self.ctx.types, current).is_some() {
                return Some(current);
            }

            let indexed = diagnostic_query::get_indexed_access_type(self.ctx.types, current)?;
            let prop_atom =
                diagnostic_query::string_literal_value(self.ctx.types, indexed.index_type)?;
            let prop_name = self.ctx.types.resolve_atom_ref(prop_atom).to_string();
            let object_type = self.evaluate_type_with_env(indexed.object_type);
            current = match self.resolve_property_access_with_env(object_type, &prop_name) {
                PropertyAccessResult::Success { type_id, .. }
                | PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type: Some(type_id),
                    ..
                } => self.evaluate_type_with_env(type_id),
                _ => return None,
            };
        }
        None
    }

    pub(crate) fn static_schema_application_schema_type(&self, type_id: TypeId) -> Option<TypeId> {
        let (_base, args) = self.static_schema_application_info(type_id)?;
        args.first().copied()
    }

    fn static_schema_alias_application_schema_type(&self, type_id: TypeId) -> Option<TypeId> {
        if let Some(schema_type) = self.static_schema_type_alias_body_schema_type(type_id) {
            return Some(schema_type);
        }
        let alias = self.ctx.types.get_display_alias(type_id)?;
        self.static_schema_application_schema_type(alias)
            .or_else(|| self.static_schema_type_alias_body_schema_type(alias))
    }

    fn static_schema_type_alias_body_schema_type(&self, type_id: TypeId) -> Option<TypeId> {
        let def_id = diagnostic_query::lazy_def_id(self.ctx.types, type_id)?;
        let def = self.ctx.definition_store.get(def_id)?;
        if def.kind != tsz_solver::def::DefKind::TypeAlias || !def.type_params.is_empty() {
            return None;
        }
        self.static_schema_application_schema_type(def.body?)
    }

    fn static_schema_type_query_value_type(&mut self, type_id: TypeId) -> Option<TypeId> {
        let sym_ref = diagnostic_query::get_type_query_symbol_ref(self.ctx.types, type_id)?;
        let sym_id = crate::query_boundaries::definition_identity::symbol_ref_to_symbol_id(sym_ref);
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let value_decl = symbol.value_declaration.into_option().or_else(|| {
            symbol.declarations.iter().copied().find(|decl| {
                self.ctx.arena.get(*decl).is_some_and(|node| {
                    node.kind == tsz_parser::syntax_kind_ext::VARIABLE_DECLARATION
                })
            })
        })?;
        Some(self.type_of_value_declaration_for_symbol(sym_id, value_decl))
    }

    fn typebox_schema_static_type(&mut self, schema_type: TypeId, depth: u8) -> Option<TypeId> {
        if depth > 12 {
            return None;
        }
        let schema_type = self.evaluate_type_for_assignability(schema_type);

        if let Some(static_type) = self.schema_property_type(schema_type, "static") {
            let static_type = self.evaluate_type_for_assignability(static_type);
            if !matches!(static_type, TypeId::ERROR | TypeId::UNKNOWN)
                && !diagnostic_query::contains_free_type_parameters(self.ctx.types, static_type)
            {
                let static_type = self
                    .rewrite_nested_static_projection_members(static_type, depth + 1)
                    .unwrap_or(static_type);
                if !self.type_has_residual_static_schema(static_type, depth + 1) {
                    return Some(static_type);
                }
            }
        }

        let properties_type = self.schema_property_type(schema_type, "properties")?;
        let properties_type = self.evaluate_type_for_assignability(properties_type);
        let shape = diagnostic_query::object_shape_for_type(self.ctx.types, properties_type)?;
        let mut properties = Vec::with_capacity(shape.properties.len());
        for prop in &shape.properties {
            let prop_type = self.typebox_schema_static_type(prop.type_id, depth + 1)?;
            let mut static_prop = tsz_solver::PropertyInfo::new(prop.name, prop_type);
            static_prop.optional = prop.optional;
            static_prop.readonly = prop.readonly;
            static_prop.declaration_order = prop.declaration_order;
            properties.push(static_prop);
        }
        Some(diagnostic_query::object_type_from_properties(
            self.ctx.types,
            properties,
        ))
    }

    fn rewrite_nested_static_projection_members(
        &mut self,
        type_id: TypeId,
        depth: u8,
    ) -> Option<TypeId> {
        if depth > 12 {
            return None;
        }
        if let Some(schema_type) = self.static_schema_application_schema_type(type_id) {
            let schema_type = self.evaluate_type_for_assignability(schema_type);
            return self.typebox_schema_static_type(schema_type, depth + 1);
        }

        let type_id = self.evaluate_type_for_assignability(type_id);
        let shape = diagnostic_query::object_shape_for_type(self.ctx.types, type_id)?;
        let mut changed = false;
        let mut properties = Vec::with_capacity(shape.properties.len());
        for prop in &shape.properties {
            let mut next = prop.clone();
            if let Some(rewritten) =
                self.rewrite_nested_static_projection_members(prop.type_id, depth + 1)
            {
                next.type_id = rewritten;
                changed = true;
            }
            properties.push(next);
        }
        changed.then(|| diagnostic_query::object_type_from_properties(self.ctx.types, properties))
    }

    fn schema_property_type(&mut self, schema_type: TypeId, property: &str) -> Option<TypeId> {
        use diagnostic_query::PropertyAccessResult;

        match self.resolve_property_access_with_env(schema_type, property) {
            PropertyAccessResult::Success { type_id, .. }
            | PropertyAccessResult::PossiblyNullOrUndefined {
                property_type: Some(type_id),
                ..
            } => Some(type_id),
            _ => None,
        }
    }

    pub(in crate::error_reporter) fn type_query_static_array_structural_display(
        &mut self,
        array_display: &str,
    ) -> Option<String> {
        let schema_name = array_display
            .strip_prefix("(typeof ")?
            .strip_suffix(".static)[]")?;
        let static_type = self.static_schema_value_name_structural_type(schema_name)?;
        let rebuilt = self.static_schema_array_display_type(static_type);
        Some(self.format_type_diagnostic(rebuilt))
    }

    fn static_schema_value_name_array_structural_display(
        &mut self,
        schema_name: &str,
        format_peer: TypeId,
    ) -> Option<String> {
        let static_type = self.static_schema_value_name_structural_type(schema_name)?;
        Some(self.format_static_schema_array_structural_type(static_type, format_peer))
    }

    fn static_schema_value_name_structural_type(&mut self, schema_name: &str) -> Option<TypeId> {
        let sym_id = self.ctx.binder.file_locals.get(schema_name)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let value_decl = symbol.value_declaration.into_option().or_else(|| {
            symbol.declarations.iter().copied().find(|decl| {
                self.ctx.arena.get(*decl).is_some_and(|node| {
                    node.kind == tsz_parser::syntax_kind_ext::VARIABLE_DECLARATION
                })
            })
        })?;
        let schema_type = self.type_of_value_declaration_for_symbol(sym_id, value_decl);
        let schema_type = self.evaluate_type_for_assignability(schema_type);
        let static_type = self.typebox_schema_static_type(schema_type, 0)?;
        Some(
            self.rewrite_nested_static_projection_members(static_type, 0)
                .unwrap_or(static_type),
        )
    }

    pub(in crate::error_reporter) fn static_schema_array_structural_display_text(
        &mut self,
        array_display: &str,
        other: TypeId,
    ) -> Option<String> {
        if let Some(query_display) = self.type_query_static_array_structural_display(array_display)
        {
            return Some(query_display);
        }
        let format_peer =
            diagnostic_query::type_parameter_constraint(self.ctx.types, other).unwrap_or(other);
        self.alias_static_array_structural_display(array_display, format_peer)
    }

    pub(in crate::error_reporter) fn static_schema_type_parameter_array_constraint_display(
        &mut self,
        type_parameter: TypeId,
        array_display: &str,
        other: TypeId,
    ) -> Option<String> {
        if !array_display.trim().ends_with("[]") {
            return None;
        }
        let constraint =
            diagnostic_query::type_parameter_constraint(self.ctx.types, type_parameter)?;
        if constraint == type_parameter {
            return None;
        }
        self.static_schema_array_structural_display(constraint, other)
    }

    pub(in crate::error_reporter) fn rewrite_static_schema_array_target_in_ts2322_message(
        &mut self,
        message: String,
        format_peer: TypeId,
    ) -> String {
        let Some(rest) = message.strip_prefix("Type '") else {
            return message;
        };
        let Some((source_display, target_part)) = rest.split_once("' is not assignable to type '")
        else {
            return message;
        };
        let Some(target_display) = target_part.strip_suffix("'.") else {
            return message;
        };
        let Some(display) =
            self.static_schema_array_structural_display_text(target_display, format_peer)
        else {
            return message;
        };
        crate::diagnostics::format_message(
            crate::diagnostics::diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[source_display, &display],
        )
    }

    /// Evaluate, widen, and normalize a schema's reduced element type, then wrap
    /// it back into the array type to display. Shared by the structural-display
    /// entry points so they prepare the element identically.
    fn static_schema_array_display_type(&mut self, element_type: TypeId) -> TypeId {
        let element_type = self.evaluate_type_for_assignability(element_type);
        let element_type = self.widen_type_for_display(element_type);
        let element_type = self.normalize_assignability_display_type(element_type);
        self.ctx.types.array(element_type)
    }

    fn format_static_schema_array_structural_type(
        &mut self,
        static_type: TypeId,
        other: TypeId,
    ) -> String {
        let rebuilt = self.static_schema_array_display_type(static_type);
        self.format_assignability_type_for_message(rebuilt, other)
    }
}
