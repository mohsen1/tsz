//! Async Promise-constructor helpers.
//!
//! Kept separate from `helpers_async.rs` so the main async emitter stays under
//! the file-size ceiling.

use super::super::*;
use crate::transforms::emit_utils;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn is_namespace_import_binding_name(&self, name: &str) -> bool {
        self.arena.nodes.iter().any(|node| {
            if node.kind != syntax_kind_ext::IMPORT_DECLARATION {
                return false;
            }
            let Some(import_decl) = self.arena.get_import_decl(node) else {
                return false;
            };
            let Some(clause_node) = self.arena.get(import_decl.import_clause) else {
                return false;
            };
            let Some(clause) = self.arena.get_import_clause(clause_node) else {
                return false;
            };
            let Some(named_bindings_node) = self.arena.get(clause.named_bindings) else {
                return false;
            };
            if named_bindings_node.kind != syntax_kind_ext::NAMESPACE_IMPORT {
                return false;
            }
            self.arena
                .get_named_imports(named_bindings_node)
                .and_then(|namespace_import| {
                    emit_utils::identifier_text(self.arena, namespace_import.name)
                })
                .is_some_and(|binding| binding == name)
        })
    }
}
