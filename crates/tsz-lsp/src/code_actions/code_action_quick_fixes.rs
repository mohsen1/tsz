//! Diagnostic-driven quick fixes.
//!
//! - Add missing `await` (TS2339 on Promise)
//! - Convert `require` to `import` (TS80005)
//! - Add `override` modifier (TS4114)
//! - Fix spelling suggestions (TS2551)
//! - Prefix unused with underscore (TS6133)

use crate::diagnostics::LspDiagnostic;
use crate::rename::{TextEdit, WorkspaceEdit};
use crate::utils::find_node_at_offset;
use rustc_hash::FxHashMap;
use tsz_parser::NodeIndex;
use tsz_parser::syntax_kind_ext;

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};
use tsz_common::position::Range;

impl<'a> CodeActionProvider<'a> {
    /// Add missing `await` quick fix.
    ///
    /// When accessing a property on a Promise type, suggest adding `await` before the expression.
    pub fn add_missing_await_quickfix(&self, diag: &LspDiagnostic) -> Option<CodeAction> {
        let code = diag.code?;
        // TS2339: Property does not exist on type (including Promise)
        // TS2349: This expression is not callable
        // TS2351: This expression is not constructable
        if code != 2339 && code != 2349 && code != 2351 {
            return None;
        }

        // Check if the diagnostic message mentions Promise
        let msg = &diag.message;
        if !msg.contains("Promise") {
            return None;
        }

        let start_offset = self
            .line_map
            .position_to_offset(diag.range.start, self.source)?;
        let node_idx = find_node_at_offset(self.arena, start_offset);
        if node_idx.is_none() {
            return None;
        }

        // Walk up to find the expression that should be awaited
        let expr_idx = self.find_awaitable_expression(node_idx)?;
        let expr_node = self.arena.get(expr_idx)?;
        let expr_text = self
            .source
            .get(expr_node.pos as usize..expr_node.end as usize)?;

        let new_text = format!("(await {expr_text})");

        let replace_start = self.line_map.offset_to_position(expr_node.pos, self.source);
        let replace_end = self.line_map.offset_to_position(expr_node.end, self.source);

        let edit = TextEdit {
            range: Range::new(replace_start, replace_end),
            new_text,
        };

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![edit]);

        Some(CodeAction {
            title: "Add missing 'await'".to_string(),
            kind: CodeActionKind::QuickFix,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: true,
            data: Some(serde_json::json!({
                "fixName": "addMissingAwait",
                "fixId": "addMissingAwait",
                "fixAllDescription": "Fix all expressions possibly missing 'await'"
            })),
        })
    }

    /// Convert `require` to `import`.
    ///
    /// `const x = require('y')` → `import x from 'y'`
    pub fn convert_require_to_import_quickfix(&self, diag: &LspDiagnostic) -> Option<CodeAction> {
        let code = diag.code?;
        if code != 80005 {
            return None;
        }

        let start_offset = self
            .line_map
            .position_to_offset(diag.range.start, self.source)?;

        // Find the require call
        let (stmt_start, stmt_end, var_name, module_spec) =
            self.find_require_statement(start_offset)?;

        let new_text = format!("import {var_name} from {module_spec}");

        let replace_start = self.line_map.offset_to_position(stmt_start, self.source);
        let replace_end = self.line_map.offset_to_position(stmt_end, self.source);

        let edit = TextEdit {
            range: Range::new(replace_start, replace_end),
            new_text,
        };

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![edit]);

        Some(CodeAction {
            title: "Convert require to import".to_string(),
            kind: CodeActionKind::QuickFix,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: true,
            data: Some(serde_json::json!({
                "fixName": "requireInTs",
                "fixId": "requireInTs",
                "fixAllDescription": "Convert all require to import"
            })),
        })
    }

    /// Add `override` modifier.
    ///
    /// When a method overrides a base class method but is missing `override` (TS4114).
    pub fn add_override_modifier_quickfix(&self, diag: &LspDiagnostic) -> Option<CodeAction> {
        let code = diag.code?;
        if code != 4114 && code != 4116 {
            return None;
        }

        let start_offset = self
            .line_map
            .position_to_offset(diag.range.start, self.source)?;
        let node_idx = find_node_at_offset(self.arena, start_offset);
        if node_idx.is_none() {
            return None;
        }

        // Find the member declaration
        let member_idx = self.find_class_member_at(node_idx)?;
        let member_node = self.arena.get(member_idx)?;

        // Insert "override " before the member name/keyword
        let insert_offset = member_node.pos;
        let insert_pos = self.line_map.offset_to_position(insert_offset, self.source);

        let edit = TextEdit {
            range: Range::new(insert_pos, insert_pos),
            new_text: "override ".to_string(),
        };

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![edit]);

        Some(CodeAction {
            title: "Add 'override' modifier".to_string(),
            kind: CodeActionKind::QuickFix,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: true,
            data: Some(serde_json::json!({
                "fixName": "fixOverrideModifier",
                "fixId": "fixAddOverrideModifier",
                "fixAllDescription": "Add all missing 'override' modifiers"
            })),
        })
    }

    /// Fix spelling suggestion.
    ///
    /// When a property/variable name is close to an existing one (TS2551).
    pub fn fix_spelling_quickfix(&self, diag: &LspDiagnostic) -> Option<CodeAction> {
        let code = diag.code?;
        if code != 2551 && code != 2552 {
            return None;
        }

        // Extract the suggested name from the diagnostic message
        // Messages are like: "Property 'x' does not exist on type 'Y'. Did you mean 'z'?"
        let msg = &diag.message;
        let suggested = extract_did_you_mean(msg)?;

        let start_offset = self
            .line_map
            .position_to_offset(diag.range.start, self.source)?;
        let end_offset = self
            .line_map
            .position_to_offset(diag.range.end, self.source)?;

        let start_pos = self.line_map.offset_to_position(start_offset, self.source);
        let end_pos = self.line_map.offset_to_position(end_offset, self.source);

        let edit = TextEdit {
            range: Range::new(start_pos, end_pos),
            new_text: suggested.to_string(),
        };

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![edit]);

        Some(CodeAction {
            title: format!("Change spelling to '{suggested}'"),
            kind: CodeActionKind::QuickFix,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: true,
            data: Some(serde_json::json!({
                "fixName": "spelling",
                "fixId": "fixSpelling",
                "fixAllDescription": "Fix all detected spelling errors"
            })),
        })
    }

    /// Prefix unused variable/parameter with underscore.
    ///
    /// When a variable/parameter is unused (TS6133), offer to prefix with `_`.
    pub fn prefix_unused_with_underscore_quickfix(
        &self,
        diag: &LspDiagnostic,
    ) -> Option<CodeAction> {
        let code = diag.code?;
        if code != 6133 && code != 6196 {
            return None;
        }

        let start_offset = self
            .line_map
            .position_to_offset(diag.range.start, self.source)?;
        let end_offset = self
            .line_map
            .position_to_offset(diag.range.end, self.source)?;

        let name = self
            .source
            .get(start_offset as usize..end_offset as usize)?;

        // Don't prefix if already starts with underscore
        if name.starts_with('_') {
            return None;
        }

        let new_name = format!("_{name}");

        let start_pos = self.line_map.offset_to_position(start_offset, self.source);
        let end_pos = self.line_map.offset_to_position(end_offset, self.source);

        let edit = TextEdit {
            range: Range::new(start_pos, end_pos),
            new_text: new_name.clone(),
        };

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![edit]);

        Some(CodeAction {
            title: format!("Prefix '{name}' with underscore"),
            kind: CodeActionKind::QuickFix,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: Some(serde_json::json!({
                "fixName": "unusedIdentifier",
                "fixId": "unusedIdentifier_prefix",
                "fixAllDescription": "Prefix all unused declarations with '_'"
            })),
        })
    }

    fn find_awaitable_expression(&self, start: NodeIndex) -> Option<NodeIndex> {
        let mut current = start;
        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            {
                let access = self.arena.get_access_expr(node)?;
                return Some(access.expression);
            }
            if node.kind == syntax_kind_ext::CALL_EXPRESSION {
                let call = self.arena.get_call_expr(node)?;
                return Some(call.expression);
            }
            current = self.arena.get_extended(current)?.parent;
        }
        None
    }

    fn find_require_statement(&self, offset: u32) -> Option<(u32, u32, String, String)> {
        let mut current = find_node_at_offset(self.arena, offset);
        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                || node.kind == syntax_kind_ext::VARIABLE_DECLARATION
            {
                let text = self.source.get(node.pos as usize..node.end as usize)?;
                // Match pattern: const/let/var x = require('...')
                if let Some(caps) = parse_require_pattern(text) {
                    return Some((node.pos, node.end, caps.0, caps.1));
                }
            }
            current = self.arena.get_extended(current)?.parent;
        }
        None
    }

    fn find_class_member_at(&self, start: NodeIndex) -> Option<NodeIndex> {
        let mut current = start;
        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::METHOD_DECLARATION
                || node.kind == syntax_kind_ext::PROPERTY_DECLARATION
            {
                return Some(current);
            }
            current = self.arena.get_extended(current)?.parent;
        }
        None
    }
}

/// Extract the suggested name from a "Did you mean 'X'?" message.
fn extract_did_you_mean(msg: &str) -> Option<&str> {
    let marker = "Did you mean '";
    let start = msg.find(marker)? + marker.len();
    let rest = msg.get(start..)?;
    let end = rest.find('\'')?;
    rest.get(..end)
}

/// Parse `const x = require('y')` patterns and return (var_name, module_spec).
fn parse_require_pattern(text: &str) -> Option<(String, String)> {
    let text = text.trim();
    // Strip const/let/var
    let rest = text
        .strip_prefix("const ")
        .or_else(|| text.strip_prefix("let "))
        .or_else(|| text.strip_prefix("var "))?;

    let eq_pos = rest.find('=')?;
    let var_name = rest[..eq_pos].trim().to_string();
    let after_eq = rest[eq_pos + 1..].trim();

    // Match require('...' or "...")
    let after_require = after_eq.strip_prefix("require(")?;
    let after_require = after_require.strip_suffix(')')?.trim();
    let after_require = after_require.strip_suffix(';').unwrap_or(after_require);
    let module_spec = after_require.trim().to_string();

    Some((var_name, module_spec))
}
