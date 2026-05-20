use crate::rename::{TextEdit, WorkspaceEdit};
use rustc_hash::FxHashMap;
use tsz_common::position::{Position, Range};
use tsz_parser::NodeIndex;

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};

impl<'a> CodeActionProvider<'a> {
    /// Returns "Surround with ..." code actions for the given selection range.
    pub fn surround_with_actions(&self, _root: NodeIndex, range: Range) -> Vec<CodeAction> {
        // Only offer surround actions when there is a non-empty selection.
        if range.start == range.end {
            return Vec::new();
        }

        let start_offset = match self.line_map.position_to_offset(range.start, self.source) {
            Some(o) => o as usize,
            None => return Vec::new(),
        };
        let end_offset = match self.line_map.position_to_offset(range.end, self.source) {
            Some(o) => o as usize,
            None => return Vec::new(),
        };

        let selected_text = match self.source.get(start_offset..end_offset) {
            Some(t) => t,
            None => return Vec::new(),
        };

        // Derive base indentation from the first selected line.
        let base_indent = self.get_indentation_at_position(&Position::new(range.start.line, 0));
        let indent_unit = self.indent_unit_from(&base_indent);
        let inner_indent = format!("{base_indent}{indent_unit}");

        // Re-indent every line of the selection by one additional level.
        let indented_body = Self::indent_text(selected_text, &inner_indent, &base_indent);

        let templates: Vec<(&str, String)> = vec![
            (
                "Surround with if statement",
                format!("{base_indent}if (condition) {{\n{indented_body}\n{base_indent}}}",),
            ),
            (
                "Surround with try/catch",
                format!(
                    "{base_indent}try {{\n{indented_body}\n{base_indent}}} catch (error) {{\n{base_indent}}}",
                ),
            ),
            (
                "Surround with for loop",
                format!(
                    "{base_indent}for (let i = 0; i < array.length; i++) {{\n{indented_body}\n{base_indent}}}",
                ),
            ),
            (
                "Surround with while loop",
                format!("{base_indent}while (condition) {{\n{indented_body}\n{base_indent}}}",),
            ),
            (
                "Surround with IIFE",
                format!("{base_indent}(() => {{\n{indented_body}\n{base_indent}}})()",),
            ),
        ];

        templates
            .into_iter()
            .map(|(title, new_text)| {
                let edit = TextEdit { range, new_text };
                let mut changes = FxHashMap::default();
                changes.insert(self.file_name.clone(), vec![edit]);

                CodeAction {
                    title: title.to_string(),
                    kind: CodeActionKind::Refactor,
                    edit: Some(WorkspaceEdit { changes }),
                    is_preferred: false,
                    data: None,
                }
            })
            .collect()
    }

    /// Indent `text` so that every line is at `inner_indent` level.
    ///
    /// The first line's existing leading whitespace (which should match
    /// `base_indent`) is replaced with `inner_indent`. Subsequent lines that
    /// start with `base_indent` get the same treatment; other lines are
    /// prefixed with the additional indent unit.
    fn indent_text(text: &str, inner_indent: &str, base_indent: &str) -> String {
        let lines: Vec<&str> = text.lines().collect();
        let mut result = String::new();
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                result.push('\n');
            }
            if line.is_empty() {
                // Keep blank lines blank.
                continue;
            }
            if let Some(rest) = line.strip_prefix(base_indent) {
                result.push_str(inner_indent);
                result.push_str(rest);
            } else {
                // Line has less indent than base -- just add the inner indent.
                result.push_str(inner_indent);
                result.push_str(line.trim_start());
            }
        }
        result
    }
}
