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
    pub(super) optional: bool,
    pub(super) type_text: String,
}

#[derive(Clone, Debug)]
pub(super) struct FunctionTypeTextParts {
    pub(super) parameters: Vec<FunctionTypeParamText>,
    pub(super) return_type: String,
}

impl<'a> DeclarationEmitter<'a> {
    pub(super) fn format_constructor_arrow_object_returns_in_type_text(type_text: &str) -> String {
        if !type_text.contains("new ") || !type_text.contains("=> {") {
            return type_text.to_string();
        }

        let mut output = String::new();
        let mut search_cursor = 0usize;
        let mut emit_cursor = 0usize;
        let mut changed = false;
        while let Some(relative_arrow) = type_text[search_cursor..].find("=> {") {
            let arrow_start = search_cursor + relative_arrow;
            let object_start = arrow_start + "=> ".len();
            let inner_start = object_start + 1;
            let Some(inner_end) = Self::matching_compact_object_type_end(type_text, object_start)
            else {
                break;
            };
            let inner = &type_text[inner_start..inner_end];
            if inner.contains('\n') || inner.contains('{') || inner.contains('}') {
                search_cursor = inner_end + 1;
                continue;
            }

            let members = if inner.contains(';') {
                inner
                    .split(';')
                    .map(str::trim)
                    .filter(|member| !member.is_empty())
                    .collect::<Vec<_>>()
            } else {
                inner
                    .split(',')
                    .map(str::trim)
                    .filter(|member| !member.is_empty())
                    .collect::<Vec<_>>()
            };
            if members.is_empty() || members.iter().any(|member| !member.contains(':')) {
                search_cursor = inner_end + 1;
                continue;
            }

            output.push_str(&type_text[emit_cursor..object_start]);
            output.push_str("{\n");
            for member in members {
                output.push_str("    ");
                output.push_str(member.trim_end_matches(',').trim());
                output.push_str(";\n");
            }
            output.push('}');
            search_cursor = inner_end + 1;
            emit_cursor = search_cursor;
            changed = true;
        }

        if !changed {
            type_text.to_string()
        } else {
            output.push_str(&type_text[emit_cursor..]);
            output
        }
    }

    fn matching_compact_object_type_end(type_text: &str, object_start: usize) -> Option<usize> {
        let bytes = type_text.as_bytes();
        if bytes.get(object_start).copied() != Some(b'{') {
            return None;
        }

        let mut depth = 0usize;
        let mut i = object_start;
        while i < bytes.len() {
            match bytes[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
            i += 1;
        }

        None
    }

    pub(super) fn infer_function_type_substitutions(
        source: &FunctionTypeTextParts,
        argument: &FunctionTypeTextParts,
        type_param_names: &[String],
        substitutions: &mut Vec<(String, String)>,
    ) {
        for (source_param_index, source_param) in source.parameters.iter().enumerate() {
            if !type_param_names
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

        if type_param_names
            .iter()
            .any(|name| name.as_str() == source.return_type)
            && !substitutions
                .iter()
                .any(|(name, _)| name.as_str() == source.return_type)
        {
            substitutions.push((
                source.return_type.clone(),
                Self::parenthesize_generic_function_type_argument(&argument.return_type),
            ));
        }
    }

    pub(super) fn function_type_parts_for_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<FunctionTypeTextParts> {
        if let Some(type_text) = self.preferred_expression_type_text(expr_idx)
            && let Some(parts) = Self::parse_function_type_text(&type_text)
        {
            return Some(parts);
        }

        let sym_id = self.value_reference_symbol(expr_idx)?;
        let binder = self.binder?;
        let sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));
        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
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
                parameters.push(FunctionTypeParamText {
                    optional: param.question_token || param.initializer != NodeIndex::NONE,
                    type_text,
                });
            }
            Some(FunctionTypeTextParts {
                parameters,
                return_type,
            })
        })
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
            let raw_param = raw_param.strip_prefix("...").unwrap_or(raw_param).trim();
            let (optional, type_text) =
                if let Some(colon_index) = Self::find_top_level_byte(raw_param, b':') {
                    let name_text = raw_param.get(..colon_index)?.trim();
                    let type_text = raw_param.get(colon_index + 1..)?.trim();
                    (name_text.ends_with('?'), type_text)
                } else {
                    (false, raw_param)
                };
            parameters.push(FunctionTypeParamText {
                optional,
                type_text: type_text.to_string(),
            });
        }

        (!return_type.is_empty()).then_some(FunctionTypeTextParts {
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
