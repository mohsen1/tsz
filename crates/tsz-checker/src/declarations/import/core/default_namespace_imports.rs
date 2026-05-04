//! Helpers for JS default imports that bind ambient namespaces.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    pub(super) fn ambient_module_declares_namespace(
        &self,
        module_name: &str,
        namespace_name: &str,
    ) -> bool {
        let clean_module = module_name.trim_matches('"').trim_matches('\'');
        let Some(all_arenas) = self.ctx.all_arenas.as_ref() else {
            return false;
        };

        for arena in all_arenas.iter() {
            for node in &arena.nodes {
                if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                    continue;
                }
                let Some(module_decl) = arena.get_module(node) else {
                    continue;
                };
                let Some(name_node) = arena.get(module_decl.name) else {
                    continue;
                };
                if !arena.get_literal(name_node).is_some_and(|lit| {
                    lit.text.trim_matches('"').trim_matches('\'') == clean_module
                }) {
                    continue;
                }
                let Some(body_node) = arena.get(module_decl.body) else {
                    continue;
                };
                let Some(block) = arena.get_module_block(body_node) else {
                    continue;
                };
                let Some(statements) = block.statements.as_ref() else {
                    continue;
                };
                for &stmt_idx in &statements.nodes {
                    let Some(stmt_node) = arena.get(stmt_idx) else {
                        continue;
                    };
                    if stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION {
                        continue;
                    }
                    let Some(inner_module) = arena.get_module(stmt_node) else {
                        continue;
                    };
                    let Some(inner_name_node) = arena.get(inner_module.name) else {
                        continue;
                    };
                    if arena
                        .get_identifier(inner_name_node)
                        .is_some_and(|ident| ident.escaped_text == namespace_name)
                    {
                        return true;
                    }
                }
            }
        }

        false
    }

    pub(super) fn report_checked_js_default_namespace_import_value_uses(
        &mut self,
        binding_idx: NodeIndex,
        local_name: &str,
    ) {
        let Some(local_sym_id) = self.resolve_identifier_symbol_without_tracking(binding_idx)
        else {
            return;
        };

        for (raw_idx, node) in self.ctx.arena.nodes.iter().enumerate() {
            if node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }
            let idx = NodeIndex(raw_idx as u32);
            if idx == binding_idx || self.is_identifier_in_type_position(idx) {
                continue;
            }
            let Some(ident) = self.ctx.arena.get_identifier(node) else {
                continue;
            };
            if ident.escaped_text != local_name {
                continue;
            }
            if self.resolve_identifier_symbol_without_tracking(idx) != Some(local_sym_id) {
                continue;
            }
            self.report_wrong_meaning(
                local_name,
                idx,
                local_sym_id,
                crate::query_boundaries::name_resolution::NameLookupKind::Namespace,
                crate::query_boundaries::name_resolution::NameLookupKind::Value,
            );
        }
    }
}
