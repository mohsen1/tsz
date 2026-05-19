use crate::state::CheckerState;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

impl<'a> CheckerState<'a> {
    pub(crate) fn prewarm_member_type_reference_params(
        &mut self,
        declarations: &[NodeIndex],
    ) -> rustc_hash::FxHashMap<tsz_solver::def::DefId, Vec<tsz_solver::TypeParamInfo>> {
        // PERF: declaration files like react16.d.ts contain extremely large interface
        // graphs. Walking every descendant of every interface just to prewarm an
        // optional cache can dominate checker time. The lowering path already falls
        // back to `ctx.get_def_type_params(def_id)` on demand, so skipping the eager
        // prewarm here preserves correctness while avoiding repeated full-tree scans.
        if self.ctx.is_declaration_file() {
            return rustc_hash::FxHashMap::default();
        }

        let mut stack = Vec::new();
        let mut params_by_def = rustc_hash::FxHashMap::default();

        for &decl_idx in declarations {
            stack.push(decl_idx);

            while let Some(node_idx) = stack.pop() {
                let Some(node) = self.ctx.arena.get(node_idx) else {
                    continue;
                };

                if node.kind == syntax_kind_ext::TYPE_REFERENCE
                    && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                {
                    let has_type_args = type_ref
                        .type_arguments
                        .as_ref()
                        .is_some_and(|args| !args.nodes.is_empty());
                    if !has_type_args
                        && let Some(sym_id_raw) =
                            self.resolve_type_symbol_for_lowering(type_ref.type_name)
                    {
                        let sym_id = tsz_binder::SymbolId(sym_id_raw);
                        let def_id = self.ctx.get_or_create_def_id(sym_id);
                        let params = self.get_type_params_for_symbol(sym_id);
                        if !params.is_empty() {
                            params_by_def.insert(def_id, params);
                        }
                    }
                }

                stack.extend(self.ctx.arena.get_children(node_idx));
            }
        }

        params_by_def
    }

    /// Pre-compute property names for computed property name expressions in interface members.
    /// Iterates over all members of all declarations, finds `COMPUTED_PROPERTY_NAME` nodes,
    /// evaluates the expression type, and builds a map from expression `NodeIndex` to Atom.
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
                // Get the name node from signature or accessor
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
                // Set checking_computed_property_name so that TS2467 (type parameter
                // reference in computed property name) is properly emitted.
                let prev = self.ctx.checking_computed_property_name;
                self.ctx.checking_computed_property_name = Some(name_idx);
                // Preserve literal types so that string literal expressions like
                // ["computed"] resolve to the literal type "computed" rather than
                // widening to `string`. Without this, get_literal_property_name
                // cannot extract the property name from the widened type.
                let prev_preserve = self.ctx.preserve_literal_types;
                self.ctx.preserve_literal_types = true;
                // Evaluate the expression type and get the property name
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
                {
                    set.insert(computed.expression);
                }
            }
        }
        set
    }
}
