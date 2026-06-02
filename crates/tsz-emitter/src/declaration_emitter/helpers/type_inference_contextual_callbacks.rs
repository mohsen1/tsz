//! Contextual callback return helpers for declaration emit.

use super::super::DeclarationEmitter;
use super::type_inference_function_text::FunctionTypeTextParts;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter::helpers) fn infer_contextual_callback_return_substitution(
        &self,
        source_function_type: &FunctionTypeTextParts,
        arg_idx: NodeIndex,
        type_param_names: &[String],
        blocked_return_type_params: &[String],
        substitutions: &[(String, String)],
    ) -> Option<(String, String)> {
        let return_param = source_function_type.return_type.trim();
        if !type_param_names
            .iter()
            .any(|name| name.as_str() == return_param)
            || blocked_return_type_params
                .iter()
                .any(|name| name.as_str() == return_param)
            || substitutions
                .iter()
                .any(|(name, _)| name.as_str() == return_param)
        {
            return None;
        }

        let arg_node = self.arena.get(arg_idx)?;
        let func = self.arena.get_function(arg_node)?;
        let return_expr = if self
            .arena
            .get(func.body)
            .is_some_and(|node| node.kind == syntax_kind_ext::BLOCK)
        {
            self.function_body_single_return_expression(func.body)?
        } else {
            func.body
        };

        let mut parameter_context = Vec::new();
        for (source_param, &arg_param_idx) in source_function_type
            .parameters
            .iter()
            .zip(&func.parameters.nodes)
        {
            if source_param.rest {
                continue;
            }
            let Some(arg_param_node) = self.arena.get(arg_param_idx) else {
                continue;
            };
            let Some(arg_param) = self.arena.get_parameter(arg_param_node) else {
                continue;
            };
            let Some(arg_name) = self.get_identifier_text(arg_param.name) else {
                continue;
            };
            let contextual_type =
                Self::replace_whole_words_in_text(&source_param.type_text, substitutions);
            if contextual_type.trim().is_empty()
                || type_param_names
                    .iter()
                    .any(|name| Self::contains_whole_word_in_text(&contextual_type, name))
            {
                continue;
            }
            parameter_context.push((arg_name, contextual_type));
        }

        let value_text =
            self.contextual_callback_return_type_text(return_expr, &parameter_context)?;
        Some((
            return_param.to_string(),
            Self::parenthesize_generic_function_type_argument(&value_text),
        ))
    }

    fn contextual_callback_return_type_text(
        &self,
        expr_idx: NodeIndex,
        parameter_context: &[(String, String)],
    ) -> Option<String> {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind == SyntaxKind::Identifier as u16 {
            let name = self.get_identifier_text(expr_idx)?;
            return parameter_context.iter().find_map(|(candidate, type_text)| {
                (candidate == &name).then(|| type_text.clone())
            });
        }
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(expr_node)?;
        if self.get_identifier_text(access.name_or_argument).as_deref() != Some("length") {
            return None;
        }
        let receiver_name = self.get_identifier_text(access.expression)?;
        let receiver_type = parameter_context
            .iter()
            .find_map(|(candidate, type_text)| (candidate == &receiver_name).then_some(type_text))?
            .trim();
        Self::type_text_has_length_number(receiver_type).then(|| "number".to_string())
    }

    fn type_text_has_length_number(type_text: &str) -> bool {
        let text = type_text.trim();
        text == "string" || text.ends_with("[]") || text.starts_with('[') && text.ends_with(']')
    }
}
