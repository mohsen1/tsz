//! Explicit value annotation recovery for identifier type computation.

use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Returns true when the symbol's value declaration is a variable
    /// declaration with an explicit type annotation (e.g. `const x: AB = ...`).
    /// Used by class-property-initializer evaluation to decide whether the
    /// declared type should override flow narrowing.
    pub(super) fn symbol_value_decl_has_explicit_type_annotation(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let decl = symbol.value_declaration;
        if decl.is_none() {
            return false;
        }
        let Some(decl_node) = self.ctx.arena.get(decl) else {
            return false;
        };
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return false;
        }
        self.ctx
            .arena
            .get_variable_declaration(decl_node)
            .is_some_and(|var| var.type_annotation.is_some())
    }

    pub(super) fn explicit_value_declared_type_for_symbol(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        fallback_decl: NodeIndex,
        declarations: &[NodeIndex],
    ) -> Option<TypeId> {
        let value_decl = self
            .preferred_value_declaration(sym_id, fallback_decl, declarations)
            .unwrap_or(fallback_decl);
        if value_decl.is_none() {
            return None;
        }
        let node = self.ctx.arena.get(value_decl)?;
        let var_decl = self.ctx.arena.get_variable_declaration(node)?;
        if var_decl.type_annotation.is_none() {
            return None;
        }
        if !self.type_annotation_targets_generic_type_only_import(var_decl.type_annotation) {
            return None;
        }
        self.ctx.node_types.remove(&var_decl.type_annotation.0);
        let annotated = self.get_type_from_type_node(var_decl.type_annotation);
        (!matches!(annotated, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)).then_some(annotated)
    }

    fn type_annotation_targets_generic_type_only_import(&mut self, annotation: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(annotation) else {
            return false;
        };
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return false;
        };
        if type_ref
            .type_arguments
            .as_ref()
            .is_none_or(|args| args.nodes.is_empty())
        {
            return false;
        }
        let Some(name_node) = self.ctx.arena.get(type_ref.type_name) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        self.resolve_type_only_import_alias_target_symbol(&ident.escaped_text)
            .is_some()
    }
}
