//! Source-backed parameter return summaries for declaration inference.

use super::super::DeclarationEmitter;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn function_body_parameter_return_type_text(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        body_idx: NodeIndex,
    ) -> Option<String> {
        let returned_identifier = self.function_body_unique_return_identifier(body_idx)?;
        if let Some(type_annotation) =
            self.function_parameter_type_annotation(func, returned_identifier)
            && let Some(type_text) = self
                .single_line_mapped_type_annotation_text(type_annotation)
                .or_else(|| self.returned_parameter_type_literal_text(type_annotation))
                .or_else(|| self.function_parameter_type_text(func, returned_identifier))
            && !type_text.trim().is_empty()
        {
            return Some(type_text);
        }

        self.returned_parameter_jsdoc_type_text(&func.parameters, returned_identifier)
    }

    pub(in crate::declaration_emitter) fn body_returned_parameter_jsdoc_type_text(
        &self,
        parameters: &NodeList,
        body_idx: NodeIndex,
    ) -> Option<String> {
        let returned_identifier = self.function_body_unique_return_identifier(body_idx)?;
        self.returned_parameter_jsdoc_type_text(parameters, returned_identifier)
    }

    fn returned_parameter_jsdoc_type_text(
        &self,
        parameters: &NodeList,
        returned_identifier: NodeIndex,
    ) -> Option<String> {
        let returned_name = self.get_identifier_text(returned_identifier)?;
        for (position, &param_idx) in parameters.nodes.iter().enumerate() {
            let param_node = self.arena.get(param_idx)?;
            let param = self.arena.get_parameter(param_node)?;
            if self.get_identifier_text(param.name).as_deref() != Some(returned_name.as_str()) {
                continue;
            }
            let jsdoc_param = self.jsdoc_param_decl_for_parameter(param_idx, position)?;
            let type_text = self.jsdoc_type_text_for_declaration_emit(&jsdoc_param.type_text);
            return (!type_text.trim().is_empty()).then_some(type_text);
        }
        None
    }

    fn returned_parameter_type_literal_text(&self, type_annotation: NodeIndex) -> Option<String> {
        let type_node = self.arena.get(type_annotation)?;
        if type_node.kind != syntax_kind_ext::TYPE_LITERAL {
            return None;
        }

        self.type_literal_annotation_text(type_annotation)
    }

    pub(in crate::declaration_emitter) fn type_literal_annotation_text(
        &self,
        type_annotation: NodeIndex,
    ) -> Option<String> {
        let type_node = self.arena.get(type_annotation)?;
        if type_node.kind != syntax_kind_ext::TYPE_LITERAL {
            return None;
        }

        self.emit_type_node_text(type_annotation)
            .map(|type_text| Self::trim_trailing_type_literal_annotation_punctuation(&type_text))
    }

    fn trim_trailing_type_literal_annotation_punctuation(type_text: &str) -> String {
        type_text
            .trim()
            .trim_end_matches(';')
            .trim_end()
            .to_string()
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
