//! Merged type-alias/value cache helpers.

use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn compute_value_type_for_merged_alias(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<TypeId> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let mut decl = symbol.value_declaration;

        if let Some(decl_node) = self.ctx.arena.get(decl)
            && decl_node.kind == SyntaxKind::Identifier as u16
        {
            decl = self.ctx.arena.get_extended(decl)?.parent;
        }

        let decl_node = self.ctx.arena.get(decl)?;
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;

        if var_decl.type_annotation.is_some() {
            let ann_type = self.get_type_from_type_node(var_decl.type_annotation);
            if ann_type != TypeId::ERROR && ann_type != TypeId::ANY {
                return Some(ann_type);
            }
        }

        if var_decl.initializer.is_some() {
            let init_type = self.get_type_of_node(var_decl.initializer);
            if init_type != TypeId::ERROR && init_type != TypeId::UNKNOWN {
                return Some(init_type);
            }
        }

        None
    }

    pub(crate) fn merged_alias_value_decl_refs_type_alias(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let mut decl = symbol.value_declaration;

        if let Some(decl_node) = self.ctx.arena.get(decl)
            && decl_node.kind == SyntaxKind::Identifier as u16
            && let Some(ext) = self.ctx.arena.get_extended(decl)
        {
            decl = ext.parent;
        }

        let Some(decl_node) = self.ctx.arena.get(decl) else {
            return false;
        };
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return false;
        };

        (var_decl.type_annotation.is_some()
            && self.type_position_subtree_refs_symbol(var_decl.type_annotation, sym_id))
            || (var_decl.initializer.is_some()
                && self.type_position_subtree_refs_symbol(var_decl.initializer, sym_id))
    }

    fn type_position_subtree_refs_symbol(&self, root: NodeIndex, sym_id: SymbolId) -> bool {
        let mut stack = vec![root];
        while let Some(idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };

            let lookup_target = match node.kind {
                k if k == syntax_kind_ext::TYPE_REFERENCE => {
                    self.ctx.arena.get_type_ref(node).map(|tr| tr.type_name)
                }
                k if k == syntax_kind_ext::TYPE_QUERY => {
                    self.ctx.arena.get_type_query(node).map(|tq| tq.expr_name)
                }
                _ => None,
            };
            if let Some(target) = lookup_target
                && self.resolve_type_symbol_for_lowering(target) == Some(sym_id.0)
            {
                return true;
            }

            stack.extend(self.ctx.arena.get_children(idx));
        }
        false
    }
}
