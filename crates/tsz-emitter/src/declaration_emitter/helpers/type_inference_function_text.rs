//! Function-type-text parsing helpers extracted from `type_inference.rs`.
//!
//! These are pure (mostly static) helpers that parse the textual representation
//! of a function/arrow type produced by the declaration emitter. Splitting them
//! into their own file keeps `type_inference.rs` under the file-size ceiling
//! enforced by `tsz-solver`'s `test_emitter_file_size_ceiling` ratchet.

use super::super::DeclarationEmitter;
use tsz_parser::parser::NodeIndex;

#[derive(Clone, Debug)]
pub(super) struct FunctionTypeParamText {
    pub(super) name: Option<String>,
    pub(super) rest: bool,
    pub(super) optional: bool,
    pub(super) type_text: String,
}

#[derive(Clone, Debug)]
pub(super) struct FunctionTypeTextParts {
    pub(super) type_param_names: Vec<String>,
    pub(super) parameters: Vec<FunctionTypeParamText>,
    pub(super) return_type: String,
}

impl<'a> DeclarationEmitter<'a> {
    pub(super) fn infer_function_type_substitutions(
        source: &FunctionTypeTextParts,
        argument: &FunctionTypeTextParts,
        type_param_names: &[String],
        substitutions: &mut Vec<(String, String)>,
    ) {
        for (source_param_index, source_param) in source.parameters.iter().enumerate() {
            if source_param.rest
                || !type_param_names
                    .iter()
                    .any(|name| name.as_str() == source_param.type_text)
            {
                continue;
            }
            if substitutions
                .iter()
                .any(|(name, _)| name.as_str() == source_param.type_text)
            {
                continue;
            }
            if let Some(argument_param) = argument.parameters.get(source_param_index) {
                substitutions.push((
                    source_param.type_text.clone(),
                    Self::parenthesize_generic_function_type_argument(&argument_param.type_text),
                ));
            } else if source_param.optional {
                substitutions.push((source_param.type_text.clone(), "unknown".to_string()));
            }
        }

        let mut argument_type_param_substitutions = Vec::new();
        for (source_param_index, source_param) in source.parameters.iter().enumerate() {
            let source_type_text =
                Self::replace_whole_words_in_text(&source_param.type_text, substitutions);
            let source_type_text = source_type_text.trim();
            if source_param.rest {
                let Some(rest_elements) =
                    Self::tuple_type_text_elements_for_inference(source_type_text)
                else {
                    if type_param_names
                        .iter()
                        .any(|name| name.as_str() == source_param.type_text)
                        && !substitutions
                            .iter()
                            .any(|(name, _)| name.as_str() == source_param.type_text)
                    {
                        let remaining_params = argument
                            .parameters
                            .iter()
                            .skip(source_param_index)
                            .collect::<Vec<_>>();
                        let value_text = if let [argument_param] = remaining_params.as_slice()
                            && argument_param.rest
                        {
                            argument_param.type_text.trim().to_string()
                        } else {
                            let tuple_items = remaining_params
                                .into_iter()
                                .map(Self::tuple_item_text_for_function_param)
                                .collect::<Vec<_>>();
                            format!("[{}]", tuple_items.join(", "))
                        };
                        substitutions.push((source_param.type_text.clone(), value_text));
                    }
                    continue;
                };
                for (argument_param, source_item) in argument
                    .parameters
                    .iter()
                    .skip(source_param_index)
                    .zip(rest_elements.iter())
                {
                    Self::infer_argument_function_type_param_substitution(
                        argument_param,
                        source_item,
                        &argument.type_param_names,
                        &mut argument_type_param_substitutions,
                    );
                }
                continue;
            }

            if let Some(argument_param) = argument.parameters.get(source_param_index) {
                Self::infer_argument_function_type_param_substitution(
                    argument_param,
                    source_type_text,
                    &argument.type_param_names,
                    &mut argument_type_param_substitutions,
                );
            }
        }

        if type_param_names
            .iter()
            .any(|name| name.as_str() == source.return_type)
            && !substitutions
                .iter()
                .any(|(name, _)| name.as_str() == source.return_type)
        {
            let return_type = Self::replace_whole_words_in_text(
                &argument.return_type,
                &argument_type_param_substitutions,
            );
            substitutions.push((
                source.return_type.clone(),
                Self::parenthesize_generic_function_type_argument(&return_type),
            ));
        }
    }

    fn infer_argument_function_type_param_substitution(
        argument_param: &FunctionTypeParamText,
        source_type_text: &str,
        argument_type_param_names: &[String],
        substitutions: &mut Vec<(String, String)>,
    ) {
        let argument_type_text = argument_param.type_text.trim();
        if !argument_type_param_names
            .iter()
            .any(|name| name.as_str() == argument_type_text)
            || substitutions
                .iter()
                .any(|(name, _)| name.as_str() == argument_type_text)
        {
            return;
        }
        substitutions.push((argument_type_text.to_string(), source_type_text.to_string()));
    }

    fn tuple_item_text_for_function_param(param: &FunctionTypeParamText) -> String {
        let type_text = param.type_text.trim();
        if let Some(name) = param.name.as_deref() {
            if param.optional {
                return format!("{name}?: {type_text}");
            }
            return format!("{name}: {type_text}");
        }
        type_text.to_string()
    }

    fn tuple_type_text_elements_for_inference(type_text: &str) -> Option<Vec<String>> {
        let mut text = type_text.trim();
        if let Some(rest) = text.strip_prefix("readonly ") {
            text = rest.trim();
        }
        if !text.starts_with('[') || !text.ends_with(']') {
            return None;
        }
        let inner = text[1..text.len() - 1].trim();
        if inner.is_empty() {
            return Some(Vec::new());
        }
        Some(
            Self::split_top_level_commas(inner)
                .into_iter()
                .map(|part| {
                    let mut part = part.trim();
                    if let Some(rest) = part.strip_prefix("...") {
                        part = rest.trim();
                    }
                    if let Some(colon) = Self::find_top_level_byte(part, b':') {
                        part = part[colon + 1..].trim();
                    }
                    part.trim_end_matches('?').trim().to_string()
                })
                .collect(),
        )
    }

    pub(super) fn function_type_parts_for_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<FunctionTypeTextParts> {
        if let Some(type_text) = self.local_variable_initializer_type_text(expr_idx)
            && let Some(parts) = Self::parse_function_type_text(&type_text)
        {
            return Some(parts);
        }

        if let Some(sym_id) = self.value_reference_symbol(expr_idx) {
            let binder = self.binder?;
            let sym_id = self
                .resolve_portability_import_alias(sym_id, binder)
                .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));
            if let Some(parts) = self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
                let decl_node = source_arena.get(decl_idx)?;
                let func = source_arena.get_function(decl_node)?;
                let return_type = self
                    .emit_type_node_text_from_arena(source_arena, func.type_annotation)
                    .or_else(|| self.source_slice_from_arena(source_arena, func.type_annotation))?
                    .trim_end()
                    .trim_end_matches(';')
                    .trim()
                    .to_string();
                let mut parameters = Vec::new();
                for &param_idx in &func.parameters.nodes {
                    let Some(param_node) = source_arena.get(param_idx) else {
                        continue;
                    };
                    let Some(param) = source_arena.get_parameter(param_node) else {
                        continue;
                    };
                    let type_text = self
                        .source_slice_from_arena(source_arena, param.type_annotation)
                        .unwrap_or_else(|| "any".to_string())
                        .trim()
                        .to_string();
                    let name = self.identifier_text_from_arena(source_arena, param.name);
                    parameters.push(FunctionTypeParamText {
                        name,
                        rest: param.dot_dot_dot_token,
                        optional: param.question_token || param.initializer != NodeIndex::NONE,
                        type_text,
                    });
                }
                let type_param_names = func
                    .type_parameters
                    .as_ref()
                    .map(|type_params| {
                        type_params
                            .nodes
                            .iter()
                            .copied()
                            .filter_map(|param_idx| {
                                source_arena
                                    .get(param_idx)
                                    .and_then(|node| source_arena.get_type_parameter(node))
                                    .and_then(|param| {
                                        self.identifier_text_from_arena(source_arena, param.name)
                                    })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Some(FunctionTypeTextParts {
                    type_param_names,
                    parameters,
                    return_type,
                })
            }) {
                return Some(parts);
            }
        }

        if let Some(type_text) = self.preferred_expression_type_text(expr_idx)
            && let Some(parts) = Self::parse_function_type_text(&type_text)
        {
            return Some(parts);
        }

        None
    }

    pub(super) fn parse_function_type_text(type_text: &str) -> Option<FunctionTypeTextParts> {
        let trimmed = type_text.trim().trim_end_matches(';').trim();
        let arrow_index = Self::find_top_level_arrow(trimmed)?;
        let params_text = trimmed.get(..arrow_index)?.trim();
        let return_type = trimmed
            .get(arrow_index + 2..)?
            .trim()
            .trim_end_matches(';')
            .trim()
            .to_string();
        let params_text = params_text
            .strip_prefix('(')
            .and_then(|text| text.strip_suffix(')'))?;
        let mut parameters = Vec::new();
        for raw_param in Self::split_top_level_commas(params_text) {
            let raw_param = raw_param.trim();
            if raw_param.is_empty() {
                continue;
            }
            let rest = raw_param.starts_with("...");
            let raw_param = raw_param.strip_prefix("...").unwrap_or(raw_param).trim();
            let (optional, type_text) =
                if let Some(colon_index) = Self::find_top_level_byte(raw_param, b':') {
                    let name_text = raw_param.get(..colon_index)?.trim();
                    let type_text = raw_param.get(colon_index + 1..)?.trim();
                    (name_text.ends_with('?'), type_text)
                } else {
                    (false, raw_param)
                };
            let name = Self::find_top_level_byte(raw_param, b':').and_then(|colon_index| {
                raw_param
                    .get(..colon_index)
                    .map(str::trim)
                    .map(|name| name.trim_end_matches('?').trim().to_string())
                    .filter(|name| !name.is_empty())
            });
            parameters.push(FunctionTypeParamText {
                name,
                rest,
                optional,
                type_text: type_text.to_string(),
            });
        }

        (!return_type.is_empty()).then_some(FunctionTypeTextParts {
            type_param_names: Vec::new(),
            parameters,
            return_type,
        })
    }

    pub(super) fn find_top_level_arrow(text: &str) -> Option<usize> {
        let bytes = text.as_bytes();
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;
        let mut angle_depth = 0usize;
        let mut i = 0usize;
        while i + 1 < bytes.len() {
            match bytes[i] {
                b'(' => paren_depth += 1,
                b')' => paren_depth = paren_depth.saturating_sub(1),
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'{' => brace_depth += 1,
                b'}' => brace_depth = brace_depth.saturating_sub(1),
                b'<' => angle_depth += 1,
                b'>' => angle_depth = angle_depth.saturating_sub(1),
                b'=' if bytes[i + 1] == b'>'
                    && paren_depth == 0
                    && bracket_depth == 0
                    && brace_depth == 0
                    && angle_depth == 0 =>
                {
                    return Some(i);
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    pub(super) fn split_top_level_commas(text: &str) -> Vec<&str> {
        let mut parts = Vec::new();
        let mut start = 0usize;
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;
        let mut angle_depth = 0usize;

        for (idx, byte) in text.bytes().enumerate() {
            match byte {
                b'(' => paren_depth += 1,
                b')' => paren_depth = paren_depth.saturating_sub(1),
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'{' => brace_depth += 1,
                b'}' => brace_depth = brace_depth.saturating_sub(1),
                b'<' => angle_depth += 1,
                b'>' => angle_depth = angle_depth.saturating_sub(1),
                b',' if paren_depth == 0
                    && bracket_depth == 0
                    && brace_depth == 0
                    && angle_depth == 0 =>
                {
                    if let Some(part) = text.get(start..idx) {
                        parts.push(part);
                    }
                    start = idx + 1;
                }
                _ => {}
            }
        }
        if let Some(part) = text.get(start..) {
            parts.push(part);
        }
        parts
    }

    pub(super) fn find_top_level_byte(text: &str, target: u8) -> Option<usize> {
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;
        let mut angle_depth = 0usize;

        for (idx, byte) in text.bytes().enumerate() {
            match byte {
                b'(' => paren_depth += 1,
                b')' => paren_depth = paren_depth.saturating_sub(1),
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'{' => brace_depth += 1,
                b'}' => brace_depth = brace_depth.saturating_sub(1),
                b'<' => angle_depth += 1,
                b'>' => angle_depth = angle_depth.saturating_sub(1),
                byte if byte == target
                    && paren_depth == 0
                    && bracket_depth == 0
                    && brace_depth == 0
                    && angle_depth == 0 =>
                {
                    return Some(idx);
                }
                _ => {}
            }
        }
        None
    }

    pub(super) fn parenthesize_generic_function_type_argument(type_text: &str) -> String {
        let trimmed = type_text.trim();
        if trimmed.starts_with('<') && trimmed.contains("=>") {
            format!("({trimmed})")
        } else {
            trimmed.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rest_type_param_infers_rest_array_from_rest_callback() {
        let source = DeclarationEmitter::parse_function_type_text("(...args: A) => B").unwrap();
        let argument =
            DeclarationEmitter::parse_function_type_text("(...args: any[]) => boolean").unwrap();
        let mut substitutions = Vec::new();

        DeclarationEmitter::infer_function_type_substitutions(
            &source,
            &argument,
            &["A".to_string(), "B".to_string()],
            &mut substitutions,
        );

        assert_eq!(
            substitutions,
            vec![
                ("A".to_string(), "any[]".to_string()),
                ("B".to_string(), "boolean".to_string()),
            ]
        );
    }
}
