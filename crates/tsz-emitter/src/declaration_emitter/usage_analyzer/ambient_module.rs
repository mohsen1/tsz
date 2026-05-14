use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

use super::UsageAnalyzer;
use crate::transforms::emit_utils::string_literal_text;

impl<'a> UsageAnalyzer<'a> {
    pub(super) fn is_ambient_module_body_name(&self, name: NodeIndex) -> bool {
        string_literal_text(self.arena, name).is_some()
            || self
                .arena
                .get(name)
                .and_then(|node| self.arena.get_identifier(node))
                .is_some_and(|ident| ident.escaped_text == "global")
    }

    pub(super) fn analyze_ambient_module_member_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.analyze_function_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.analyze_class_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                self.analyze_interface_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                self.analyze_type_alias_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.analyze_enum_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.analyze_variable_statement(stmt_idx);
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                self.analyze_import_equals_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.analyze_module_declaration(stmt_idx);
            }
            _ => self.analyze_statement(stmt_idx),
        }
    }
}
