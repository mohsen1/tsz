//! Assignment target/source annotation-text extraction for TS2322-family
//! diagnostics.
//!
//! Extracted from `assignment_formatting.rs` as pure code motion to keep that
//! file under the 2000-LOC arch ceiling. No logic changes.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    /// The declared annotation node governing the assignment target at
    /// `anchor_idx` (variable declaration, parameter, or — for a return-value
    /// source — the enclosing function's return annotation). Node-based
    /// sibling of [`Self::direct_assignment_target_annotation_text`].
    pub(in crate::error_reporter) fn direct_assignment_target_annotation_node(
        &self,
        anchor_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = anchor_idx;
        let mut guard = 0;
        let source_is_return = self.assignment_source_is_return_expression(anchor_idx);
        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }
            let node = self.ctx.arena.get(current)?;
            if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node)
                && var_decl.type_annotation.is_some()
            {
                return Some(var_decl.type_annotation);
            }
            if let Some(param) = self.ctx.arena.get_parameter(node)
                && param.type_annotation.is_some()
            {
                return Some(param.type_annotation);
            }
            if source_is_return
                && let Some(function) = self.ctx.arena.get_function(node)
                && function.type_annotation.is_some()
            {
                return Some(function.type_annotation);
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }
        None
    }

    pub(super) fn direct_assignment_target_annotation_text(
        &self,
        anchor_idx: NodeIndex,
    ) -> Option<String> {
        if let Some(annotation_idx) = self.direct_assignment_target_annotation_node(anchor_idx) {
            return self
                .node_text(annotation_idx)
                .and_then(|text| self.sanitize_type_annotation_text_for_diagnostic(text, true));
        }
        self.source_assignment_target_annotation_text(anchor_idx)
    }

    pub(super) fn source_assignment_target_annotation_text(
        &self,
        anchor_idx: NodeIndex,
    ) -> Option<String> {
        let (start, end) = self.get_node_span(anchor_idx)?;
        let source = self.ctx.arena.source_files.first()?.text.as_ref();
        let start = start as usize;
        let end = end as usize;
        if start >= end || end > source.len() {
            return None;
        }
        let line_end = source[end..]
            .find('\n')
            .map_or(source.len(), |offset| end + offset);
        if let Some(text) = self.annotation_text_from_colon_fragment(&source[end..line_end]) {
            return Some(text);
        }

        let anchor_text = source[start..end].trim_start();
        if !anchor_text.starts_with("return") {
            return None;
        }
        let body_start = source[..start].rfind('{')?;
        let close_paren = source[..body_start].rfind(')')?;
        self.annotation_text_from_colon_fragment(&source[close_paren + 1..body_start])
    }

    pub(super) fn annotation_text_from_colon_fragment(&self, fragment: &str) -> Option<String> {
        let colon = fragment.find(':')?;
        if !fragment[..colon].trim().is_empty() {
            return None;
        }
        let type_fragment = &fragment[colon + 1..];
        let type_start = type_fragment
            .char_indices()
            .find_map(|(idx, ch)| (!ch.is_whitespace()).then_some(idx))?;
        let mut depth = 0u32;
        let mut end = type_fragment.len();
        for (idx, ch) in type_fragment[type_start..].char_indices() {
            let absolute_idx = type_start + idx;
            if depth == 0 && absolute_idx > type_start && matches!(ch, '=' | ';' | ',' | ')' | '{')
            {
                end = absolute_idx;
                break;
            }
            match ch {
                '<' | '(' | '[' | '{' => depth = depth.saturating_add(1),
                '>' | ')' | ']' | '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        let text = type_fragment[type_start..end].trim().to_string();
        self.sanitize_type_annotation_text_for_diagnostic(text, true)
    }
}
