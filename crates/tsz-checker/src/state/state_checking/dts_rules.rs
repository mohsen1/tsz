//! Declaration file (.d.ts) ambient context checks.
//!
//! Covers TS1036 (statements not allowed in ambient contexts) and
//! TS1046 (top-level declarations must start with declare/export).

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    /// Emit TS1036 for non-declaration statements in .d.ts files.
    /// In .d.ts files the entire file is implicitly ambient, so non-declaration
    /// statements (break, continue, return, if, while, for, debugger, etc.) are not allowed.
    /// Returns `true` if a diagnostic was emitted (the first non-declaration
    /// statement in the file). TSC only reports the first TS1036 per file.
    pub(crate) fn check_dts_statement_in_ambient_context(&mut self, stmt_idx: NodeIndex) -> bool {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        let is_non_declaration = matches!(
            node.kind,
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT
                || k == syntax_kind_ext::IF_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT
                || k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT
                || k == syntax_kind_ext::BREAK_STATEMENT
                || k == syntax_kind_ext::CONTINUE_STATEMENT
                || k == syntax_kind_ext::RETURN_STATEMENT
                || k == syntax_kind_ext::WITH_STATEMENT
                || k == syntax_kind_ext::SWITCH_STATEMENT
                || k == syntax_kind_ext::THROW_STATEMENT
                || k == syntax_kind_ext::TRY_STATEMENT
                || k == syntax_kind_ext::DEBUGGER_STATEMENT
                || k == syntax_kind_ext::LABELED_STATEMENT
        );

        if is_non_declaration && let Some((pos, end)) = self.ctx.get_node_span(stmt_idx) {
            self.ctx.error(
                pos,
                end - pos,
                diagnostic_messages::STATEMENTS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS.to_string(),
                diagnostic_codes::STATEMENTS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
            );
            return true;
        }
        false
    }

    /// Emit TS1046 for the first top-level declaration in a .d.ts file that
    /// lacks both `declare` and `export` modifiers.
    pub(crate) fn check_dts_top_level_declare_or_export(&mut self, statements: &[NodeIndex]) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_parser::parser::syntax_kind_ext;

        for &stmt_idx in statements {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            let modifiers: Option<&Option<tsz_parser::parser::NodeList>> = match node.kind {
                k if k == syntax_kind_ext::MODULE_DECLARATION => {
                    self.ctx.arena.get_module(node).map(|m| &m.modifiers)
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    self.ctx.arena.get_class(node).map(|c| &c.modifiers)
                }
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    self.ctx.arena.get_function(node).map(|f| &f.modifiers)
                }
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    self.ctx.arena.get_variable(node).map(|v| &v.modifiers)
                }
                k if k == syntax_kind_ext::ENUM_DECLARATION => {
                    self.ctx.arena.get_enum(node).map(|e| &e.modifiers)
                }
                _ => None,
            };

            let Some(modifiers) = modifiers else {
                continue;
            };

            let has_declare = self.ctx.arena.is_declare(modifiers);
            let has_export = self
                .ctx
                .arena
                .has_modifier(modifiers, tsz_scanner::SyntaxKind::ExportKeyword);

            if !has_declare && !has_export {
                if let Some((pos, _end)) = self.ctx.get_node_span(stmt_idx) {
                    let keyword_len: u32 = match node.kind {
                        k if k == syntax_kind_ext::MODULE_DECLARATION => 9,
                        k if k == syntax_kind_ext::CLASS_DECLARATION => 5,
                        k if k == syntax_kind_ext::FUNCTION_DECLARATION => 8,
                        k if k == syntax_kind_ext::VARIABLE_STATEMENT => 3,
                        k if k == syntax_kind_ext::ENUM_DECLARATION => 4,
                        _ => 1,
                    };
                    self.ctx.error(
                        pos,
                        keyword_len,
                        diagnostic_messages::TOP_LEVEL_DECLARATIONS_IN_D_TS_FILES_MUST_START_WITH_EITHER_A_DECLARE_OR_EXPORT.to_string(),
                        diagnostic_codes::TOP_LEVEL_DECLARATIONS_IN_D_TS_FILES_MUST_START_WITH_EITHER_A_DECLARE_OR_EXPORT,
                    );
                }
                return;
            }
        }
    }
}
