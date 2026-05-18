//! Computed-property helpers for symbol type lowering.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Pre-compute property names for computed property name expressions in interface members.
    /// Iterates over all members of all declarations, finds `COMPUTED_PROPERTY_NAME` nodes,
    /// evaluates the expression type, and builds a map from expression `NodeIndex` to `Atom`.
    pub(crate) fn precompute_computed_property_names(
        &mut self,
        declarations: &[NodeIndex],
    ) -> rustc_hash::FxHashMap<NodeIndex, tsz_common::Atom> {
        let mut map = rustc_hash::FxHashMap::default();
        for &decl_idx in declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = self.ctx.arena.get_interface(node) else {
                continue;
            };
            for &member_idx in &interface.members.nodes {
                let Some(member) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let name_idx = if let Some(sig) = self.ctx.arena.get_signature(member) {
                    sig.name
                } else if let Some(acc) = self.ctx.arena.get_accessor(member) {
                    acc.name
                } else {
                    continue;
                };
                let Some(name_node) = self.ctx.arena.get(name_idx) else {
                    continue;
                };
                if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    continue;
                }
                let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
                    continue;
                };
                let prev = self.ctx.checking_computed_property_name;
                self.ctx.checking_computed_property_name = Some(name_idx);
                let prev_preserve = self.ctx.preserve_literal_types;
                self.ctx.preserve_literal_types = true;
                let expr_type = self.get_type_of_node(computed.expression);
                self.ctx.preserve_literal_types = prev_preserve;
                self.ctx.checking_computed_property_name = prev;
                if let Some(name) =
                    crate::query_boundaries::type_computation::access::literal_property_name(
                        self.ctx.types,
                        expr_type,
                    )
                {
                    map.insert(computed.expression, name);
                } else if let Some(sym_ref) =
                    crate::query_boundaries::common::unique_symbol_ref(self.ctx.types, expr_type)
                {
                    let name = self
                        .ctx
                        .types
                        .intern_string(&format!("__unique_{}", sym_ref.0));
                    map.insert(computed.expression, name);
                } else if expr_type == TypeId::SYMBOL {
                    let name = self
                        .ctx
                        .types
                        .intern_string(&format!("__symbol_computed_{}", computed.expression.0));
                    map.insert(computed.expression, name);
                }
            }
        }
        map
    }

    pub(crate) fn precompute_symbol_named_computed_property_names(
        &mut self,
        declarations: &[NodeIndex],
    ) -> rustc_hash::FxHashSet<NodeIndex> {
        let mut set = rustc_hash::FxHashSet::default();
        for &decl_idx in declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = self.ctx.arena.get_interface(node) else {
                continue;
            };
            for &member_idx in &interface.members.nodes {
                let Some(member) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let name_idx = if let Some(sig) = self.ctx.arena.get_signature(member) {
                    sig.name
                } else if let Some(acc) = self.ctx.arena.get_accessor(member) {
                    acc.name
                } else {
                    continue;
                };
                let Some(name_node) = self.ctx.arena.get(name_idx) else {
                    continue;
                };
                if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    continue;
                }
                let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
                    continue;
                };
                let prev = self.ctx.checking_computed_property_name;
                self.ctx.checking_computed_property_name = Some(name_idx);
                let prev_preserve = self.ctx.preserve_literal_types;
                self.ctx.preserve_literal_types = true;
                let expr_type = self.get_type_of_node(computed.expression);
                self.ctx.preserve_literal_types = prev_preserve;
                self.ctx.checking_computed_property_name = prev;
                if crate::query_boundaries::common::unique_symbol_ref(self.ctx.types, expr_type)
                    .is_some()
                    || expr_type == TypeId::SYMBOL
                {
                    set.insert(computed.expression);
                }
            }
        }
        set
    }
}
