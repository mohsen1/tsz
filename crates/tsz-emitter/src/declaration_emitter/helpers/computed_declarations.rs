use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

use super::super::DeclarationEmitter;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn class_static_computed_index_access_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let access = self.arena.get_access_expr(expr_node)?;
        let class_expr = self.skip_parenthesized_non_null_and_comma(access.expression);
        let class_name = self.get_identifier_text(class_expr)?;
        let class_idx = self.class_declaration_for_value_reference(class_expr, &class_name)?;
        let class_node = self.arena.get(class_idx)?;
        let class = self.arena.get_class(class_node)?;

        let mut members = vec![class_name];
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let Some(method) = self.arena.get_method_decl(member_node) else {
                continue;
            };
            if !self.arena.is_static(&method.modifiers) {
                continue;
            }
            if self
                .arena
                .get(method.name)
                .is_none_or(|name| name.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME)
            {
                continue;
            }
            let function_type = self.method_function_type_text(member_idx, method, 0)?;
            members.push(format!("({function_type})"));
        }

        (members.len() > 1).then(|| members.join(" | "))
    }

    fn class_declaration_for_value_reference(
        &self,
        expr_idx: NodeIndex,
        class_name: &str,
    ) -> Option<NodeIndex> {
        if let (Some(binder), Some(sym_id)) = (self.binder, self.value_reference_symbol(expr_idx))
            && let Some(symbol) = binder.symbols.get(sym_id)
        {
            for decl_idx in symbol.declarations.iter().copied() {
                if self
                    .arena
                    .get(decl_idx)
                    .is_some_and(|node| self.arena.get_class(node).is_some())
                {
                    return Some(decl_idx);
                }
            }
        }

        if let Some(source_file_idx) = self.current_source_file_idx
            && let Some(source_file_node) = self.arena.get(source_file_idx)
            && let Some(source_file) = self.arena.get_source_file(source_file_node)
            && let Some(class_idx) =
                source_file
                    .statements
                    .nodes
                    .iter()
                    .copied()
                    .find(|&stmt_idx| {
                        self.arena
                            .get(stmt_idx)
                            .and_then(|node| self.arena.get_class(node))
                            .is_some_and(|class| {
                                self.get_identifier_text(class.name).as_deref() == Some(class_name)
                            })
                    })
        {
            return Some(class_idx);
        }

        self.arena.nodes.iter().enumerate().find_map(|(idx, node)| {
            self.arena.get_class(node).and_then(|class| {
                (self.get_identifier_text(class.name).as_deref() == Some(class_name))
                    .then_some(NodeIndex(idx as u32))
            })
        })
    }

    pub(in crate::declaration_emitter) fn method_function_type_text(
        &self,
        method_idx: NodeIndex,
        method: &tsz_parser::parser::node::MethodDeclData,
        depth: u32,
    ) -> Option<String> {
        let mut scratch = self.scratch_declaration_emitter();
        scratch.indent_level = depth;
        scratch.write("(");
        scratch.emit_parameters_with_body(&method.parameters, method.body);
        scratch.write(") => ");
        scratch.emit_method_function_type_return(method_idx, method);
        let type_text = scratch.writer.take_output();
        (!type_text.trim().is_empty()).then_some(type_text)
    }

    pub(in crate::declaration_emitter) fn broad_object_index_signature_value_type(
        line: &str,
    ) -> Option<&str> {
        let trimmed = line.trim_start();
        let without_readonly = trimmed
            .strip_prefix("readonly ")
            .unwrap_or(trimmed)
            .trim_start();
        (without_readonly.starts_with("[x: string]:")
            || without_readonly.starts_with("[x: number]:")
            || without_readonly.starts_with("[x: symbol]:"))
        .then(|| Self::object_literal_property_value_type(without_readonly))
        .flatten()
    }

    pub(in crate::declaration_emitter) fn object_literal_property_value_type(
        line: &str,
    ) -> Option<&str> {
        let trimmed = line.trim().trim_end_matches(';').trim();
        let without_readonly = trimmed
            .strip_prefix("readonly ")
            .unwrap_or(trimmed)
            .trim_start();
        let colon_idx = if without_readonly.starts_with('[') {
            let bracket_end = without_readonly.find(']')?;
            without_readonly.get(bracket_end + 1..)?.find(':')? + bracket_end + 1
        } else {
            without_readonly.find(':')?
        };
        without_readonly.get(colon_idx + 1..).map(str::trim)
    }
}
