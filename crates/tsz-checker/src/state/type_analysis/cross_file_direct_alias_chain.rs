use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_binder::{BinderState, symbol_flags};
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

impl<'a> CheckerState<'a> {
    /// Like `source_file_type_node_is_scope_independent`, but also follows
    /// file-local, non-generic type-alias chains that can lower lazily.
    pub(super) fn source_file_type_node_is_local_alias_chain_lowerable(
        arena: &NodeArena,
        binder: &BinderState,
        node_idx: NodeIndex,
        seen: &mut AliasCycleTracker,
    ) -> bool {
        if Self::source_file_type_node_is_scope_independent(arena, node_idx) {
            return true;
        }
        let Some(node) = arena.get(node_idx) else {
            return false;
        };
        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let Some(type_ref) = arena.get_type_ref(node) else {
                    return false;
                };
                if type_ref
                    .type_arguments
                    .as_ref()
                    .is_some_and(|args| !args.nodes.is_empty())
                {
                    return false;
                }
                let Some(name) = arena
                    .get(type_ref.type_name)
                    .and_then(|name_node| arena.get_identifier(name_node))
                    .map(|ident| ident.escaped_text.as_str())
                else {
                    return false;
                };
                let Some(sym_id) = binder.file_locals.get(name) else {
                    return false;
                };
                if seen.contains(&sym_id) {
                    return false;
                }
                let Some(symbol) = binder.get_symbol(sym_id) else {
                    return false;
                };
                if symbol.flags & symbol_flags::TYPE_ALIAS == 0 {
                    return false;
                }
                let disallowed = symbol_flags::VALUE
                    | symbol_flags::CLASS
                    | symbol_flags::VALUE_MODULE
                    | symbol_flags::NAMESPACE_MODULE;
                if symbol.flags & disallowed != 0 || symbol.declarations.len() != 1 {
                    return false;
                }
                let decl_idx = symbol.declarations[0];
                let Some(decl_node) = arena.get(decl_idx) else {
                    return false;
                };
                let Some(type_alias) = arena.get_type_alias(decl_node) else {
                    return false;
                };
                if type_alias
                    .type_parameters
                    .as_ref()
                    .is_some_and(|p| !p.nodes.is_empty())
                {
                    return false;
                }
                if !seen.push(sym_id) {
                    return false;
                }
                let result = Self::source_file_type_node_is_local_alias_chain_lowerable(
                    arena,
                    binder,
                    type_alias.type_node,
                    seen,
                );
                seen.pop(sym_id);
                result
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                arena.get_composite_type(node).is_some_and(|composite| {
                    composite.types.nodes.iter().copied().all(|member| {
                        Self::source_file_type_node_is_local_alias_chain_lowerable(
                            arena, binder, member, seen,
                        )
                    })
                })
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                arena.get_array_type(node).is_some_and(|array| {
                    Self::source_file_type_node_is_local_alias_chain_lowerable(
                        arena,
                        binder,
                        array.element_type,
                        seen,
                    )
                })
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                arena.get_tuple_type(node).is_some_and(|tuple| {
                    tuple.elements.nodes.iter().copied().all(|element| {
                        Self::source_file_type_node_is_local_alias_chain_lowerable(
                            arena, binder, element, seen,
                        )
                    })
                })
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE
                || k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE =>
            {
                arena.get_wrapped_type(node).is_some_and(|wrapped| {
                    Self::source_file_type_node_is_local_alias_chain_lowerable(
                        arena,
                        binder,
                        wrapped.type_node,
                        seen,
                    )
                })
            }
            _ => false,
        }
    }
}
