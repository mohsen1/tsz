use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn element_access_receiver_declared_element_display(
        &mut self,
        idx: NodeIndex,
        type_id: TypeId,
    ) -> Option<String> {
        let receiver = if self
            .ctx
            .arena
            .get(idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
        {
            idx
        } else {
            self.access_receiver_for_diagnostic_node(idx)?
        };
        let receiver_node = self.ctx.arena.get(receiver)?;
        if receiver_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return None;
        }

        let access = self.ctx.arena.get_access_expr(receiver_node)?;
        let base_expr = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(access.expression);
        let base_node = self.ctx.arena.get(base_expr)?;
        if base_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self.resolve_identifier_symbol(base_expr)?;
        let declared_type = self.get_type_of_symbol(sym_id);
        if matches!(declared_type, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }

        if let Some(display) = self.element_access_receiver_declared_index_value_display(
            declared_type,
            type_id,
            access.name_or_argument,
            base_expr,
        ) {
            return Some(display);
        }

        let declared_element_type =
            crate::query_boundaries::common::array_element_type(self.ctx.types, declared_type)
                .or_else(|| {
                    crate::query_boundaries::common::tuple_elements(self.ctx.types, declared_type)
                        .map(|elements| {
                            let element_types: Vec<TypeId> =
                                elements.iter().map(|elem| elem.type_id).collect();
                            match element_types.as_slice() {
                                [] => TypeId::NEVER,
                                [element_type] => *element_type,
                                _ => self.ctx.types.factory().union(element_types),
                            }
                        })
                })?;

        let declared_element_type = self.evaluate_type_with_env(declared_element_type);
        if matches!(declared_element_type, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }
        if !self.is_assignable_to(type_id, declared_element_type)
            && !self.is_assignable_to(declared_element_type, type_id)
        {
            return None;
        }

        Some(self.format_type_for_assignability_message(declared_element_type))
    }

    fn element_access_receiver_declared_index_value_display(
        &mut self,
        declared_type: TypeId,
        actual_type: TypeId,
        argument: NodeIndex,
        base_expr: NodeIndex,
    ) -> Option<String> {
        if crate::query_boundaries::common::array_element_type(self.ctx.types, declared_type)
            .is_some()
            || crate::query_boundaries::common::tuple_elements(self.ctx.types, declared_type)
                .is_some()
        {
            return None;
        }

        let argument = self.ctx.arena.skip_parenthesized_and_assertions(argument);
        let argument_node = self.ctx.arena.get(argument)?;
        let resolver = crate::query_boundaries::common::IndexSignatureResolver::new(self.ctx.types);
        let prefers_number_index =
            self.element_access_argument_prefers_number_index(argument, argument_node.kind);
        let (raw_index_value_type, selected_number_index) = if prefers_number_index {
            if let Some(index_value_type) = resolver.resolve_number_index(declared_type) {
                (index_value_type, true)
            } else {
                (resolver.resolve_string_index(declared_type)?, false)
            }
        } else if let Some(index_value_type) = resolver.resolve_string_index(declared_type) {
            (index_value_type, false)
        } else {
            (resolver.resolve_number_index(declared_type)?, true)
        };
        if raw_index_value_type == TypeId::ANY {
            return None;
        }

        let has_explicit_alias_surface = self
            .ctx
            .types
            .get_display_alias(raw_index_value_type)
            .is_some()
            || self
                .element_access_base_declares_index_value_alias(base_expr, selected_number_index);
        let index_value_type = self.evaluate_type_with_env(raw_index_value_type);
        if matches!(index_value_type, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }
        if !self.is_assignable_to(actual_type, index_value_type)
            && !self.is_assignable_to(index_value_type, actual_type)
        {
            return None;
        }

        Some(if has_explicit_alias_surface {
            self.format_type_for_property_receiver_message(raw_index_value_type)
        } else {
            self.format_structural_type_for_property_receiver_message(index_value_type)
        })
    }

    fn element_access_argument_prefers_number_index(
        &mut self,
        argument: NodeIndex,
        argument_kind: u16,
    ) -> bool {
        if argument_kind == SyntaxKind::NumericLiteral as u16 {
            return true;
        }

        let raw_argument_type = self.get_type_of_node(argument);
        let argument_type = self.evaluate_type_with_env(raw_argument_type);
        self.type_prefers_number_index(argument_type)
    }

    fn type_prefers_number_index(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::NUMBER {
            return true;
        }
        if crate::query_boundaries::common::number_literal_value(self.ctx.types, type_id).is_some()
        {
            return true;
        }
        crate::query_boundaries::common::union_members(self.ctx.types, type_id).is_some_and(
            |members| {
                !members.is_empty()
                    && members
                        .iter()
                        .all(|&member| self.type_prefers_number_index(member))
            },
        )
    }

    fn element_access_base_declares_index_value_alias(
        &mut self,
        base_expr: NodeIndex,
        wants_number_index: bool,
    ) -> bool {
        let Some(sym_id) = self.resolve_identifier_symbol(base_expr) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let Some(decl_node) = self.ctx.arena.get(symbol.value_declaration) else {
            return false;
        };
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        let Some(type_node) = self.ctx.arena.get(var_decl.type_annotation) else {
            return false;
        };
        if type_node.kind != syntax_kind_ext::TYPE_LITERAL {
            return false;
        }
        let Some(type_literal) = self.ctx.arena.get_type_literal(type_node) else {
            return false;
        };

        type_literal
            .members
            .nodes
            .iter()
            .copied()
            .any(|member_idx| {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    return false;
                };
                if member_node.kind != syntax_kind_ext::INDEX_SIGNATURE {
                    return false;
                }
                let Some(index_signature) = self.ctx.arena.get_index_signature(member_node) else {
                    return false;
                };
                if !self
                    .index_signature_matches_requested_index(index_signature, wants_number_index)
                {
                    return false;
                }
                self.ctx
                    .arena
                    .get(index_signature.type_annotation)
                    .is_some_and(|value_type| value_type.kind == syntax_kind_ext::TYPE_REFERENCE)
            })
    }

    fn index_signature_matches_requested_index(
        &mut self,
        index_signature: &tsz_parser::parser::node::IndexSignatureData,
        wants_number_index: bool,
    ) -> bool {
        index_signature.parameters.nodes.iter().any(|param_idx| {
            let Some(param_type_annotation) = self
                .ctx
                .arena
                .get(*param_idx)
                .and_then(|param_node| self.ctx.arena.get_parameter(param_node))
                .map(|param| param.type_annotation)
            else {
                return false;
            };
            if wants_number_index {
                return self
                    .ctx
                    .arena
                    .get(param_type_annotation)
                    .is_some_and(|param_type| param_type.kind == SyntaxKind::NumberKeyword as u16);
            }
            self.get_type_from_type_node(param_type_annotation) == TypeId::STRING
        })
    }

    pub(crate) fn format_structural_type_for_property_receiver_message(
        &mut self,
        type_id: TypeId,
    ) -> String {
        if let Some(display) = self.format_structural_object_for_property_receiver_message(type_id)
        {
            return display;
        }

        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_skip_application_alias_names()
            .with_skip_object_display_alias()
            .with_expand_scalar_mapped_alias_applications()
            .with_preserve_optional_parameter_surface_syntax(true);
        if let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, type_id)
            .or_else(|| self.ctx.definition_store.find_def_for_type(type_id))
            && self
                .ctx
                .definition_store
                .get(def_id)
                .is_some_and(|def| def.kind == tsz_solver::def::DefKind::TypeAlias)
        {
            formatter = formatter.with_skip_type_alias_def_id(def_id);
        }
        formatter.format(type_id).into_owned()
    }

    fn format_structural_object_for_property_receiver_message(
        &mut self,
        type_id: TypeId,
    ) -> Option<String> {
        let shape =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)?;
        if shape.properties.is_empty()
            && shape.string_index.is_none()
            && shape.number_index.is_none()
        {
            return None;
        }

        let mut parts = Vec::new();
        let mut properties = shape.properties.clone();
        crate::query_boundaries::common::normalize_display_property_order(&mut properties);
        for prop in properties {
            let name = self.ctx.types.resolve_atom(prop.name);
            let readonly = if prop.readonly { "readonly " } else { "" };
            let optional = if prop.optional { "?" } else { "" };
            let value = self.format_type_for_property_receiver_message(prop.type_id);
            parts.push(format!("{readonly}{name}{optional}: {value}"));
        }
        for index in shape.string_index.iter().chain(shape.number_index.iter()) {
            let key_name = index
                .param_name
                .map(|name| self.ctx.types.resolve_atom_ref(name).to_string())
                .unwrap_or_else(|| "x".to_string());
            let key_kind = self.format_type(index.key_type);
            let value = self.format_type_for_property_receiver_message(index.value_type);
            parts.push(format!("[{key_name}: {key_kind}]: {value}"));
        }

        Some(format!("{{ {}; }}", parts.join("; ")))
    }
}

pub(super) fn elide_long_property_receiver_object_literals(display: String) -> String {
    let preserve_count = if display.starts_with("Omit<") {
        3
    } else if display.starts_with("merge<") {
        5
    } else {
        return display;
    };

    let chars: Vec<char> = display.chars().collect();
    let mut out = String::with_capacity(display.len());
    let mut object_count = 0_u32;
    let mut idx = 0;

    while idx < chars.len() {
        if chars[idx] != '{' {
            out.push(chars[idx]);
            idx += 1;
            continue;
        }

        let start = idx;
        idx += 1;
        let mut depth = 1_i32;
        while idx < chars.len() && depth > 0 {
            match chars[idx] {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
            idx += 1;
        }

        object_count += 1;
        if object_count > preserve_count {
            out.push_str("{ ...; }");
        } else {
            out.extend(chars[start..idx].iter());
        }
    }

    out
}
