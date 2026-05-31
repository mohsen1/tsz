//! Source-backed parameter return summaries for declaration inference.

use super::super::DeclarationEmitter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn function_body_parameter_return_type_text(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        body_idx: NodeIndex,
    ) -> Option<String> {
        let returned_identifier = self.function_body_unique_return_identifier(body_idx)?;
        let type_annotation = self.function_parameter_type_annotation(func, returned_identifier)?;
        let type_text = self
            .single_line_mapped_type_annotation_text(type_annotation)
            .or_else(|| self.function_parameter_type_text(func, returned_identifier))?;
        (!type_text.trim().is_empty()).then_some(type_text)
    }

    pub(in crate::declaration_emitter) fn function_body_spread_array_return_type_text(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        body_idx: NodeIndex,
    ) -> Option<String> {
        let return_expr = self.function_body_single_return_expression(body_idx)?;
        let return_node = self.arena.get(return_expr)?;
        if return_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }
        let array = self.arena.get_literal_expr(return_node)?;
        let mut element_types = Vec::new();
        for &element_idx in &array.elements.nodes {
            let element_node = self.arena.get(element_idx)?;
            if element_node.kind != syntax_kind_ext::SPREAD_ELEMENT {
                return None;
            }
            let spread = self.arena.get_spread(element_node)?;
            let spread_expr = self.skip_parenthesized_expression(spread.expression)?;
            let parameter_type = self.function_parameter_type_text(func, spread_expr)?;
            element_types.push(format!("{parameter_type}[number]"));
        }
        match element_types.as_slice() {
            [] => None,
            [element_type] => Some(format!("{element_type}[]")),
            _ => Some(format!("({})[]", element_types.join(" | "))),
        }
    }
}
