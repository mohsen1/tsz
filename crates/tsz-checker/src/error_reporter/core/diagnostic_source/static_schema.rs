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
        if !array_display.contains("Static<typeof") && !array_display.ends_with("[]") {
            return None;
        }
        if !self.is_static_schema_application_display(element_type) {
            return None;
        }
        if let Some(schema_type) = self.static_schema_application_schema_type(element_type) {
            let schema_type = self.evaluate_type_for_assignability(schema_type);
            let schema_display = self.format_type_diagnostic(schema_type);
            if let Some(display) = Self::typebox_schema_static_display_from_text(&schema_display) {
                return Some(format!("{display}[]"));
            }
        }
        let evaluated_element = self.static_schema_element_structural_type(element_type)?;
        let widened_element = self
            .normalize_assignability_display_type(self.widen_type_for_display(evaluated_element));
        let rebuilt = self.ctx.types.array(widened_element);
        Some(self.format_assignability_type_for_message(rebuilt, other))
    }

    fn is_static_schema_application_display(&mut self, type_id: TypeId) -> bool {
        if self
            .format_type_diagnostic(type_id)
            .starts_with("Static<typeof ")
        {
            return true;
        }
        let app_info = crate::query_boundaries::common::application_info(self.ctx.types, type_id)
            .or_else(|| {
                let alias = self.ctx.types.get_display_alias(type_id)?;
                crate::query_boundaries::common::application_info(self.ctx.types, alias)
            });
        app_info.is_some_and(|(base, _)| self.format_type_diagnostic(base) == "Static")
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

    fn static_schema_application_schema_type(&self, type_id: TypeId) -> Option<TypeId> {
        let (_base, args) =
            crate::query_boundaries::common::application_info(self.ctx.types, type_id).or_else(
                || {
                    let alias = self.ctx.types.get_display_alias(type_id)?;
                    crate::query_boundaries::common::application_info(self.ctx.types, alias)
                },
            )?;
        args.first().copied()
    }

    fn typebox_schema_static_type(&mut self, schema_type: TypeId, depth: u8) -> Option<TypeId> {
        use crate::query_boundaries::common::PropertyAccessResult;

        if depth > 6 {
            return None;
        }
        let schema_type = self.evaluate_type_for_assignability(schema_type);
        if self.format_type_diagnostic(schema_type) == "TString" {
            return Some(TypeId::STRING);
        }

        let properties_type = if let Some((base, args)) =
            crate::query_boundaries::common::application_info(self.ctx.types, schema_type).or_else(
                || {
                    let alias = self.ctx.types.get_display_alias(schema_type)?;
                    crate::query_boundaries::common::application_info(self.ctx.types, alias)
                },
            ) {
            if self.format_type_diagnostic(base) != "TObject" {
                return None;
            }
            args.first().copied()?
        } else {
            match self.resolve_property_access_with_env(schema_type, "properties") {
                PropertyAccessResult::Success { type_id, .. }
                | PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type: Some(type_id),
                    ..
                } => type_id,
                _ => return None,
            }
        };
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

    fn typebox_schema_static_display_from_text(schema: &str) -> Option<String> {
        let schema = schema.trim();
        if schema == "TString" {
            return Some("string".to_string());
        }
        let inner = schema.strip_prefix("TObject<")?.strip_suffix('>')?.trim();
        let properties = inner.strip_prefix('{')?.strip_suffix('}')?.trim();
        let mut rendered = Vec::new();
        for entry in Self::split_top_level_typebox_properties(properties) {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }
            let colon = Self::top_level_colon(entry)?;
            let name = entry[..colon].trim();
            let value = entry[colon + 1..].trim();
            let value_display = Self::typebox_schema_static_display_from_text(value)?;
            rendered.push(format!("{name}: {value_display};"));
        }
        Some(format!("{{ {} }}", rendered.join(" ")))
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
        let schema_display = self.format_type_diagnostic(schema_type);
        Self::typebox_schema_static_display_from_text(&schema_display)
            .map(|display| format!("{display}[]"))
    }

    fn split_top_level_typebox_properties(properties: &str) -> Vec<&str> {
        let mut parts = Vec::new();
        let mut start = 0;
        let mut angle_depth = 0i32;
        let mut brace_depth = 0i32;
        for (idx, ch) in properties.char_indices() {
            match ch {
                '<' => angle_depth += 1,
                '>' => angle_depth -= 1,
                '{' => brace_depth += 1,
                '}' => brace_depth -= 1,
                ';' if angle_depth == 0 && brace_depth == 0 => {
                    parts.push(&properties[start..idx]);
                    start = idx + ch.len_utf8();
                }
                _ => {}
            }
        }
        if start < properties.len() {
            parts.push(&properties[start..]);
        }
        parts
    }

    fn top_level_colon(entry: &str) -> Option<usize> {
        let mut angle_depth = 0i32;
        let mut brace_depth = 0i32;
        for (idx, ch) in entry.char_indices() {
            match ch {
                '<' => angle_depth += 1,
                '>' => angle_depth -= 1,
                '{' => brace_depth += 1,
                '}' => brace_depth -= 1,
                ':' if angle_depth == 0 && brace_depth == 0 => return Some(idx),
                _ => {}
            }
        }
        None
    }
}
