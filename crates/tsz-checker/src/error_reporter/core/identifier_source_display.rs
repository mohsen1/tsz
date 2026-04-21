use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn identifier_array_object_literal_source_display(
        &mut self,
        expr_idx: NodeIndex,
        target: TypeId,
    ) -> Option<String> {
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if (symbol.flags & tsz_binder::symbol_flags::VARIABLE) == 0 {
            return None;
        }
        let &decl_idx = symbol.declarations.first()?;
        let decl = self.ctx.arena.get_variable_declaration_at(decl_idx)?;
        let init_idx = decl.initializer.into_option()?;
        let init_idx = self.ctx.arena.skip_parenthesized_and_assertions(init_idx);
        let init_node = self.ctx.arena.get(init_idx)?;
        if init_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }
        let literal = self.ctx.arena.get_literal_expr(init_node)?;
        if literal.elements.nodes.is_empty() {
            return None;
        }

        let mut ordered_names = Vec::new();
        let mut property_values: Vec<Vec<TypeId>> = Vec::new();
        for (element_index, &element_idx) in literal.elements.nodes.iter().enumerate() {
            let element_node = self.ctx.arena.get(element_idx)?;
            if element_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return None;
            }
            let object = self.ctx.arena.get_literal_expr(element_node)?;
            if element_index == 0 {
                for &child_idx in &object.elements.nodes {
                    let child = self.ctx.arena.get(child_idx)?;
                    let prop = self.ctx.arena.get_property_assignment(child)?;
                    let name = self.get_property_name(prop.name)?;
                    ordered_names.push(name);
                    property_values.push(vec![self.get_type_of_node(prop.initializer)]);
                }
                continue;
            }

            if object.elements.nodes.len() != ordered_names.len() {
                return None;
            }
            for (prop_index, &child_idx) in object.elements.nodes.iter().enumerate() {
                let child = self.ctx.arena.get(child_idx)?;
                let prop = self.ctx.arena.get_property_assignment(child)?;
                let name = self.get_property_name(prop.name)?;
                if name != ordered_names[prop_index] {
                    return None;
                }
                property_values[prop_index].push(self.get_type_of_node(prop.initializer));
            }
        }

        let fields = ordered_names
            .into_iter()
            .zip(property_values)
            .map(|(name, value_types)| {
                let widened_types = value_types
                    .into_iter()
                    .map(|ty| self.widen_type_for_display(ty))
                    .collect::<Vec<_>>();
                let value_type = if widened_types.len() == 1 {
                    widened_types[0]
                } else {
                    self.ctx.types.factory().union(widened_types)
                };
                let display = self.format_assignability_type_for_message(value_type, target);
                format!("{name}: {display}")
            })
            .collect::<Vec<_>>()
            .join("; ");
        Some(format!("{{ {fields}; }}[]"))
    }

    pub(in crate::error_reporter) fn identifier_literal_initializer_source_display(
        &mut self,
        expr_idx: NodeIndex,
        target: TypeId,
    ) -> Option<String> {
        let target = self.evaluate_type_for_assignability(target);
        if target != TypeId::UNDEFINED
            && crate::query_boundaries::common::enum_def_id(self.ctx.types, target).is_none()
        {
            return None;
        }
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if (symbol.flags & tsz_binder::symbol_flags::VARIABLE) == 0 {
            return None;
        }
        let &decl_idx = symbol.declarations.first()?;
        let decl = self.ctx.arena.get_variable_declaration_at(decl_idx)?;
        if decl.type_annotation.is_some() || decl.initializer.is_none() {
            return None;
        }

        let init_idx = self.ctx.arena.skip_parenthesized(decl.initializer);
        let init_node = self.ctx.arena.get(init_idx)?;
        match init_node.kind {
            k if k == tsz_scanner::SyntaxKind::TrueKeyword as u16 => Some("true".to_string()),
            k if k == tsz_scanner::SyntaxKind::FalseKeyword as u16 => Some("false".to_string()),
            _ => None,
        }
    }
}
