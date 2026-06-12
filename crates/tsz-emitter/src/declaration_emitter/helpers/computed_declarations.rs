use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

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

    pub(in crate::declaration_emitter) fn rewrite_recursive_static_class_expression_type(
        &self,
        prop_idx: NodeIndex,
        type_id: tsz_solver::types::TypeId,
    ) -> String {
        let printed = self.print_type_id(type_id);
        let Some(prop_node) = self.arena.get(prop_idx) else {
            return printed;
        };
        let Some(prop) = self.arena.get_property_decl(prop_node) else {
            return printed;
        };
        let Some(property_name) = self
            .arena
            .get_identifier_at(prop.name)
            .map(|ident| ident.escaped_text.clone())
        else {
            return printed;
        };
        if !self.property_initializer_is_recursive_class_expression(prop_idx, prop.initializer) {
            return printed;
        }
        let Some(interner) = self.type_interner else {
            return printed;
        };
        let Some(callable) = tsz_solver::type_queries::get_callable_shape(interner, type_id) else {
            return printed;
        };
        if !callable
            .properties
            .iter()
            .any(|prop| interner.resolve_atom(prop.name) == property_name)
        {
            return printed;
        }

        Self::elide_recursive_static_class_expression_member_text(&printed, &property_name)
    }

    fn elide_recursive_static_class_expression_member_text(
        printed: &str,
        property_name: &str,
    ) -> String {
        let mut output = String::with_capacity(printed.len() + crate::ELIDED_ANY.len());
        let segments = printed.split_inclusive('\n').collect::<Vec<_>>();
        let mut index = 0;

        while index < segments.len() {
            let segment = segments[index];

            let (line, newline) = segment
                .strip_suffix('\n')
                .map_or((segment, ""), |line| (line, "\n"));
            if let Some(line) =
                Self::elide_recursive_static_class_expression_member_line(line, property_name)
            {
                output.push_str(&line);
                output.push_str(newline);
                output.extend(segments[index + 1..].iter().copied());
                return output;
            }

            if let Some(end_index) = Self::recursive_static_class_expression_member_block_end(
                &segments,
                index,
                property_name,
            ) {
                output.push_str(
                    &Self::recursive_static_class_expression_member_replacement_line(
                        line,
                        property_name,
                    ),
                );
                if segments[end_index].ends_with('\n') {
                    output.push('\n');
                }
                output.extend(segments[end_index + 1..].iter().copied());
                return output;
            }

            output.push_str(segment);
            index += 1;
        }

        printed.to_string()
    }

    fn elide_recursive_static_class_expression_member_line(
        line: &str,
        property_name: &str,
    ) -> Option<String> {
        let trimmed_start = line.trim_start();
        let leading_len = line.len() - trimmed_start.len();
        let trimmed = trimmed_start.trim_end();
        if trimmed != format!("{property_name}: any;") {
            return None;
        }

        let trailing_len = trimmed_start.len() - trimmed.len();
        let trailing_start = line.len() - trailing_len;
        let mut output = String::with_capacity(line.len() + crate::ELIDED_ANY.len());
        output.push_str(&line[..leading_len]);
        output.push_str(property_name);
        output.push_str(": ");
        output.push_str(crate::ELIDED_ANY);
        output.push(';');
        output.push_str(&line[trailing_start..]);
        Some(output)
    }

    fn recursive_static_class_expression_member_block_end(
        segments: &[&str],
        start_index: usize,
        property_name: &str,
    ) -> Option<usize> {
        let first_line = segments
            .get(start_index)?
            .strip_suffix('\n')
            .unwrap_or(segments[start_index]);
        let trimmed = first_line.trim();
        if trimmed != format!("{property_name}: {{") {
            return None;
        }

        let mut depth = 0i32;
        for (offset, segment) in segments[start_index..].iter().enumerate() {
            let line = segment.strip_suffix('\n').unwrap_or(segment);
            for ch in line.chars() {
                match ch {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            if depth == 0 && line.trim_end().ends_with(';') {
                return Some(start_index + offset);
            }
        }
        None
    }

    fn recursive_static_class_expression_member_replacement_line(
        line: &str,
        property_name: &str,
    ) -> String {
        let trimmed_start = line.trim_start();
        let leading_len = line.len() - trimmed_start.len();
        let mut output = String::with_capacity(line.len() + crate::ELIDED_ANY.len());
        output.push_str(&line[..leading_len]);
        output.push_str(property_name);
        output.push_str(": ");
        output.push_str(crate::ELIDED_ANY);
        output.push(';');
        output
    }

    pub(in crate::declaration_emitter) fn property_initializer_is_recursive_class_expression(
        &self,
        prop_idx: NodeIndex,
        initializer_idx: NodeIndex,
    ) -> bool {
        let Some(class_expr) = self.arena.get_class_at(initializer_idx) else {
            return false;
        };
        let Some(enclosing_class_idx) = self
            .arena
            .get_extended(prop_idx)
            .map(|extended| extended.parent)
            .filter(|parent| {
                self.arena
                    .get(*parent)
                    .is_some_and(|node| node.kind == syntax_kind_ext::CLASS_DECLARATION)
            })
        else {
            return false;
        };
        let Some(enclosing_class_name) = self
            .arena
            .get_class_at(enclosing_class_idx)
            .and_then(|class| self.arena.get_identifier_at(class.name))
            .map(|ident| ident.escaped_text.clone())
        else {
            return false;
        };
        let Some(heritage_clauses) = class_expr.heritage_clauses.as_ref() else {
            return false;
        };

        heritage_clauses.nodes.iter().copied().any(|clause_idx| {
            self.arena
                .get_heritage_clause_at(clause_idx)
                .filter(|heritage| heritage.token == SyntaxKind::ExtendsKeyword as u16)
                .and_then(|heritage| heritage.types.nodes.first().copied())
                .map(|type_idx| {
                    self.arena
                        .get_expr_type_args_at(type_idx)
                        .map_or(type_idx, |expr_type_args| expr_type_args.expression)
                })
                .and_then(|expr_idx| self.arena.get_identifier_at(expr_idx))
                .is_some_and(|ident| ident.escaped_text == enclosing_class_name)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::DeclarationEmitter;

    #[test]
    fn recursive_static_class_expression_elision_rewrites_exact_member_line() {
        let printed = "{\n    new(): Root;\n    Root: any;\n}\n";

        let actual = DeclarationEmitter::elide_recursive_static_class_expression_member_text(
            printed, "Root",
        );

        assert_eq!(
            "{\n    new(): Root;\n    Root: /*elided*/ any;\n}\n",
            actual
        );
    }

    #[test]
    fn recursive_static_class_expression_elision_preserves_unmatched_text() {
        let printed = "{ Root: any; }\n    OtherRoot: any;\n";

        let actual = DeclarationEmitter::elide_recursive_static_class_expression_member_text(
            printed, "Root",
        );

        assert_eq!(printed, actual);
    }

    #[test]
    fn recursive_static_class_expression_elision_rewrites_nested_constructor_member() {
        let printed =
            "{\n    new (): {};\n    D: {\n        new (): {};\n        D: any;\n    };\n}";

        let actual =
            DeclarationEmitter::elide_recursive_static_class_expression_member_text(printed, "D");

        assert_eq!("{\n    new (): {};\n    D: /*elided*/ any;\n}", actual);
    }
}
