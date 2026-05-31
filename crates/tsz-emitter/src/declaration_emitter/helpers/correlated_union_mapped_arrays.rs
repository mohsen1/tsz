//! Mapped-array tuple inference helpers for DTS source calls.

use super::super::DeclarationEmitter;
use super::correlated_union::MappedArgumentInference;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn unwrap_mapped_array_tuple_element(
        element: &str,
    ) -> Option<String> {
        let trimmed = element.trim();
        if let Some(rest) = trimmed.strip_prefix("...") {
            let rest = rest.trim();
            let array_inner = Self::strip_array_suffix(rest).unwrap_or(rest);
            return Some(format!("...{array_inner}[]"));
        }
        let element = trimmed.trim_end_matches('?').trim();
        Self::strip_array_suffix(element).map(str::to_string)
    }

    pub(in crate::declaration_emitter) fn infer_mapped_tuple_spread_argument_substitution(
        &self,
        source_arena: &NodeArena,
        param_type_idx: NodeIndex,
        arg_idx: NodeIndex,
        type_param_names: &[String],
    ) -> Option<(String, String)> {
        let (type_arg_idx, inference) = self
            .mapped_argument_type_arg_and_inference_from_param_type(source_arena, param_type_idx)?;
        if !matches!(inference, MappedArgumentInference::IsomorphicArray) {
            return None;
        }
        let type_arg_text = self
            .emit_type_node_text_from_arena(source_arena, type_arg_idx)
            .or_else(|| self.source_slice_from_arena(source_arena, type_arg_idx))?;
        let param_elements = Self::tuple_type_text_elements_preserving_rest(&type_arg_text)?;
        let spread_index = param_elements.iter().position(|element| {
            let Some(name) = element.trim().strip_prefix("...").map(str::trim) else {
                return false;
            };
            type_param_names.iter().any(|known| known == name)
        })?;
        let type_param_name = param_elements[spread_index]
            .trim()
            .strip_prefix("...")?
            .trim()
            .to_string();
        if param_elements[spread_index + 1..]
            .iter()
            .any(|element| element.trim().starts_with("..."))
        {
            return None;
        }
        let arg_type_text = self
            .array_literal_tuple_argument_type_text(arg_idx)
            .or_else(|| self.call_argument_type_text_for_substitution(arg_idx, None))?;
        let arg_elements = Self::tuple_type_text_elements_preserving_rest(&arg_type_text)?;
        let fixed_prefix = spread_index;
        let fixed_suffix = param_elements.len().saturating_sub(spread_index + 1);
        if arg_elements.len() < fixed_prefix + fixed_suffix {
            return None;
        }
        for (expected, actual) in param_elements
            .iter()
            .take(fixed_prefix)
            .zip(arg_elements.iter().take(fixed_prefix))
            .chain(
                param_elements
                    .iter()
                    .rev()
                    .take(fixed_suffix)
                    .zip(arg_elements.iter().rev().take(fixed_suffix)),
            )
        {
            let actual_inner = Self::unwrap_mapped_array_tuple_element(actual)?;
            if actual_inner != expected.trim().trim_end_matches('?').trim() {
                return None;
            }
        }
        let end = arg_elements.len() - fixed_suffix;
        let mut inferred = Vec::new();
        for element in &arg_elements[fixed_prefix..end] {
            inferred.push(Self::unwrap_mapped_array_tuple_element(element)?);
        }
        Some((type_param_name, format!("[{}]", inferred.join(", "))))
    }
}
