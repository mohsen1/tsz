//! Recursive type-alias shape classifiers used by depth checking.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    /// True when a conditional alias has a branch whose top-level union contains
    /// the same alias applied to the same type parameters.
    ///
    /// Productive recursive object shapes such as `{ next: Node<T> }` are valid
    /// in `tsc`, but top-level union growth like `U | Recur<T>` does not make
    /// progress on the recursive input and must be bounded by the TS2589
    /// instantiation-depth path at concrete use sites.
    pub(crate) fn type_alias_has_same_input_recursive_conditional_union_body(
        &mut self,
        alias_sid: tsz_binder::SymbolId,
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(alias_sid) else {
            return false;
        };
        let declarations = symbol.declarations.clone();
        declarations.into_iter().any(|decl_idx| {
            self.ctx.arena.get(decl_idx).is_some_and(|decl_node| {
                decl_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    && self
                        .ctx
                        .arena
                        .get_type_alias(decl_node)
                        .is_some_and(|alias| {
                            self.conditional_body_has_same_input_recursive_union_branch(
                                alias.type_node,
                                alias_sid,
                            )
                        })
            })
        })
    }

    fn conditional_body_has_same_input_recursive_union_branch(
        &mut self,
        node_idx: NodeIndex,
        alias_sid: tsz_binder::SymbolId,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::CONDITIONAL_TYPE
            && let Some(cond) = self.ctx.arena.get_conditional_type(node)
            && (self.type_node_top_level_union_contains_same_input_recursive_alias_ref(
                cond.true_type,
                alias_sid,
            ) || self.type_node_top_level_union_contains_same_input_recursive_alias_ref(
                cond.false_type,
                alias_sid,
            ))
        {
            return true;
        }

        self.ctx
            .arena
            .get_children(node_idx)
            .into_iter()
            .any(|child_idx| {
                self.conditional_body_has_same_input_recursive_union_branch(child_idx, alias_sid)
            })
    }

    fn type_node_top_level_union_contains_same_input_recursive_alias_ref(
        &self,
        node_idx: NodeIndex,
        alias_sid: tsz_binder::SymbolId,
    ) -> bool {
        let Some(unwrapped_idx) = self.unwrap_parenthesized_type(node_idx) else {
            return false;
        };
        let Some(node) = self.ctx.arena.get(unwrapped_idx) else {
            return false;
        };

        if node.kind != syntax_kind_ext::UNION_TYPE {
            return false;
        }

        self.ctx
            .arena
            .get_composite_type(node)
            .is_some_and(|union| {
                union.types.nodes.iter().copied().any(|member_idx| {
                    self.type_node_is_same_input_recursive_alias_ref(member_idx, alias_sid)
                })
            })
    }

    fn type_node_is_same_input_recursive_alias_ref(
        &self,
        node_idx: NodeIndex,
        alias_sid: tsz_binder::SymbolId,
    ) -> bool {
        let Some(unwrapped_idx) = self.unwrap_parenthesized_type(node_idx) else {
            return false;
        };
        let Some(node) = self.ctx.arena.get(unwrapped_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return false;
        };
        let resolved = self
            .resolve_type_symbol_for_lowering(type_ref.type_name)
            .map(tsz_binder::SymbolId);
        resolved == Some(alias_sid)
            && type_ref
                .type_arguments
                .as_ref()
                .is_some_and(|args| self.type_args_match_alias_params(alias_sid, args))
    }
}
