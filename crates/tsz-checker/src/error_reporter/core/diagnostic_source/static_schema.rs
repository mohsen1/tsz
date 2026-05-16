use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn static_schema_array_structural_display(
        &mut self,
        array_type: TypeId,
        other: TypeId,
    ) -> Option<String> {
        let element_type =
            crate::query_boundaries::common::array_element_type(self.ctx.types, array_type)?;
        let array_display = self.format_type_diagnostic(array_type);
        if let Some(display) = self.type_query_static_array_structural_display(&array_display) {
            return Some(display);
        }
        if !self.is_static_schema_application(element_type) {
            return None;
        }
        if let Some(schema_type) = self.static_schema_application_schema_type(element_type) {
            let schema_type = self.evaluate_type_for_assignability(schema_type);
            if let Some(static_type) = self.typebox_schema_static_type(schema_type, 0) {
                let static_type = self.evaluate_type_for_assignability(static_type);
                let static_type = self.widen_type_for_display(static_type);
                let static_type = self.normalize_assignability_display_type(static_type);
                let rebuilt = self.ctx.types.array(static_type);
                let display = self.format_assignability_type_for_message(rebuilt, other);
                return Some(display);
            }
        }
        let evaluated_element = self.static_schema_element_structural_type(element_type)?;
        let widened_element = self
            .normalize_assignability_display_type(self.widen_type_for_display(evaluated_element));
        let rebuilt = self.ctx.types.array(widened_element);
        Some(self.format_assignability_type_for_message(rebuilt, other))
    }

    fn is_static_schema_application(&self, type_id: TypeId) -> bool {
        self.static_schema_application_schema_type(type_id)
            .is_some()
    }

    pub(crate) fn type_alias_projects_static_member(&self, base: TypeId) -> bool {
        let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, base)
        else {
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
        let Some(indexed) =
            crate::query_boundaries::common::get_indexed_access_type(self.ctx.types, body)
        else {
            return false;
        };
        self.is_static_property_name(indexed.index_type)
    }

    fn is_static_property_name(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::common::string_literal_value(self.ctx.types, type_id)
            .is_some_and(|name| self.ctx.types.resolve_atom_ref(name).as_ref() == "static")
    }

    fn static_schema_application_info(&self, type_id: TypeId) -> Option<(TypeId, Vec<TypeId>)> {
        let app_info = crate::query_boundaries::common::application_info(self.ctx.types, type_id)
            .or_else(|| {
            let alias = self.ctx.types.get_display_alias(type_id)?;
            crate::query_boundaries::common::application_info(self.ctx.types, alias)
        })?;
        self.type_alias_projects_static_member(app_info.0)
            .then_some(app_info)
    }

    fn static_schema_element_structural_type(&mut self, element_type: TypeId) -> Option<TypeId> {
        use crate::query_boundaries::common::PropertyAccessResult;

        if let Some(schema_type) = self.static_schema_application_schema_type(element_type) {
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
                    if crate::query_boundaries::common::object_shape_for_type(
                        self.ctx.types,
                        property_type,
                    )
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
            if crate::query_boundaries::common::object_shape_for_type(self.ctx.types, current)
                .is_some()
            {
                return Some(current);
            }

            let indexed =
                crate::query_boundaries::common::get_indexed_access_type(self.ctx.types, current)?;
            let prop_atom = crate::query_boundaries::common::string_literal_value(
                self.ctx.types,
                indexed.index_type,
            )?;
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

    fn typebox_schema_static_type(&mut self, schema_type: TypeId, depth: u8) -> Option<TypeId> {
        if depth > 12 {
            return None;
        }
        let schema_type = self.evaluate_type_for_assignability(schema_type);

        if let Some(static_type) = self.schema_property_type(schema_type, "static") {
            let static_type = self.evaluate_type_for_assignability(static_type);
            if !matches!(static_type, TypeId::ERROR | TypeId::UNKNOWN)
                && !crate::query_boundaries::common::contains_free_type_parameters(
                    self.ctx.types,
                    static_type,
                )
            {
                return Some(
                    self.rewrite_nested_static_projection_members(static_type, depth + 1)
                        .unwrap_or(static_type),
                );
            }
        }

        let properties_type = self.schema_property_type(schema_type, "properties")?;
        let properties_type = self.evaluate_type_for_assignability(properties_type);
        let shape = crate::query_boundaries::common::object_shape_for_type(
            self.ctx.types,
            properties_type,
        )?;
        let mut properties = Vec::with_capacity(shape.properties.len());
        for prop in &shape.properties {
            let prop_type = self.typebox_schema_static_type(prop.type_id, depth + 1)?;
            let mut static_prop = tsz_solver::PropertyInfo::new(prop.name, prop_type);
            static_prop.optional = prop.optional;
            static_prop.readonly = prop.readonly;
            static_prop.declaration_order = prop.declaration_order;
            properties.push(static_prop);
        }
        Some(self.ctx.types.factory().object(properties))
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
        let shape =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)?;
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
        changed.then(|| self.ctx.types.factory().object(properties))
    }

    fn schema_property_type(&mut self, schema_type: TypeId, property: &str) -> Option<TypeId> {
        use crate::query_boundaries::common::PropertyAccessResult;

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
        let sym_id = self.ctx.binder.file_locals.get(schema_name)?;
        let value_decl = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .map(|symbol| symbol.value_declaration)
            .unwrap_or(tsz_parser::NodeIndex::NONE);
        let schema_type = self.type_of_value_declaration_for_symbol(sym_id, value_decl);
        let schema_type = self.evaluate_type_for_assignability(schema_type);
        let static_type = self.typebox_schema_static_type(schema_type, 0)?;
        let static_type = self.evaluate_type_for_assignability(static_type);
        let static_type = self.widen_type_for_display(static_type);
        let static_type = self.normalize_assignability_display_type(static_type);
        let rebuilt = self.ctx.types.array(static_type);
        Some(self.format_type_diagnostic(rebuilt))
    }
}
