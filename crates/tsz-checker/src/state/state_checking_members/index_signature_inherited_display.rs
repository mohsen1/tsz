use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    /// Find the base class/interface member that introduced an inherited
    /// property, so TS2411 can render tsc's source-verbatim computed-name text
    /// (`declarationNameToString`) instead of the bare resolved property name.
    /// `shape.properties` only carries the resolved `Atom`, not the declaring
    /// node, so the heritage chain is walked (recursively, for multi-level
    /// `extends` chains) to recover it.
    pub(crate) fn inherited_member_display_name(
        &mut self,
        iface_node: NodeIndex,
        prop_name: &str,
    ) -> Option<String> {
        let mut visited = std::collections::HashSet::new();
        let name_idx =
            self.find_member_name_node_in_hierarchy(iface_node, prop_name, false, &mut visited)?;
        self.get_member_name_text(name_idx)
    }

    /// Recursive helper for [`Self::inherited_member_display_name`]. When
    /// `check_self_members` is `false`, `decl_node`'s own members are skipped
    /// (used for the starting node, whose own members are never the source of
    /// an *inherited* property) and only its heritage bases are searched.
    fn find_member_name_node_in_hierarchy(
        &mut self,
        decl_node: NodeIndex,
        prop_name: &str,
        check_self_members: bool,
        visited: &mut std::collections::HashSet<NodeIndex>,
    ) -> Option<NodeIndex> {
        if !visited.insert(decl_node) {
            return None;
        }
        let node = self.ctx.arena.get(decl_node)?;
        let (members, heritage) = if node.kind == syntax_kind_ext::CLASS_DECLARATION {
            let class = self.ctx.arena.get_class(node)?;
            (class.members.nodes.clone(), class.heritage_clauses.clone())
        } else if node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
            let iface = self.ctx.arena.get_interface(node)?;
            (iface.members.nodes.clone(), iface.heritage_clauses.clone())
        } else {
            return None;
        };

        if check_self_members {
            for &member_idx in &members {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind == syntax_kind_ext::INDEX_SIGNATURE {
                    continue;
                }
                let Some(name_idx) = self.get_member_name_node(member_node) else {
                    continue;
                };
                let resolved = self
                    .get_property_name_resolved(name_idx)
                    .or_else(|| self.get_member_name(member_idx));
                if resolved.as_deref() == Some(prop_name) {
                    return Some(name_idx);
                }
            }
        }

        let heritage = heritage?;
        for &clause_idx in &heritage.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage_clause) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };
            for &type_idx in &heritage_clause.types.nodes {
                let expr_idx = self
                    .ctx
                    .arena
                    .get(type_idx)
                    .and_then(|n| self.ctx.arena.get_expr_type_args(n))
                    .map(|eta| eta.expression)
                    .unwrap_or(type_idx);
                let Some(sym_id) = self.resolve_heritage_symbol(expr_idx) else {
                    continue;
                };
                let Some(symbol) = self.ctx.binder.symbols.get(sym_id) else {
                    continue;
                };
                for base_decl in symbol.declarations.clone() {
                    if let Some(found) =
                        self.find_member_name_node_in_hierarchy(base_decl, prop_name, true, visited)
                    {
                        return Some(found);
                    }
                }
            }
        }
        None
    }
}
