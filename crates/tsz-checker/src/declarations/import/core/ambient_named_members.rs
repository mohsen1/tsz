use crate::state::CheckerState;
use tsz_parser::parser::{node::ImportDeclData, syntax_kind_ext};

impl<'a> CheckerState<'a> {
    pub(crate) fn check_named_imports_against_empty_ambient_module(
        &mut self,
        import: &ImportDeclData,
        module_name: &str,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let Some(clause_node) = self.ctx.arena.get(import.import_clause) else {
            return;
        };
        let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
            return;
        };
        let Some(bindings_node) = self.ctx.arena.get(clause.named_bindings) else {
            return;
        };
        if bindings_node.kind != syntax_kind_ext::NAMED_IMPORTS {
            return;
        }
        let Some(named_imports) = self.ctx.arena.get_named_imports(bindings_node) else {
            return;
        };

        let quoted_module = format!("\"{module_name}\"");
        for &element_idx in &named_imports.elements.nodes {
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            let Some(specifier) = self.ctx.arena.get_specifier(element_node) else {
                continue;
            };
            let name_idx = if specifier.property_name.is_none() {
                specifier.name
            } else {
                specifier.property_name
            };
            let Some(name_node) = self.ctx.arena.get(name_idx) else {
                continue;
            };
            let Some(identifier) = self.ctx.arena.get_identifier(name_node) else {
                continue;
            };
            let import_name = identifier.escaped_text.as_str();
            if import_name == "default" {
                continue;
            }
            let message = format_message(
                diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER,
                &[&quoted_module, import_name],
            );
            self.error_at_node(
                name_idx,
                &message,
                diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER,
            );
        }
    }

    pub(crate) fn check_js_type_only_imports_for_ambient_module(
        &mut self,
        import: &ImportDeclData,
        module_name: &str,
    ) {
        self.check_js_type_only_imports_after_import_validation(import, module_name);
    }
}
