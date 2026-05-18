use crate::state::CheckerState;
use tsz_binder::{Symbol, SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{NodeAccess, NodeArena};

impl<'a> CheckerState<'a> {
    /// Check if `sym_id` names a type alias that behaves like the built-in `Awaited<T>`.
    pub(crate) fn is_standard_or_conditional_awaited_alias(
        &self,
        sym_id: SymbolId,
        symbol: &Symbol,
    ) -> bool {
        if self.symbol_has_standard_lib_origin(sym_id) {
            return symbol.escaped_name.as_str() == "Awaited";
        }

        self.is_user_defined_promiselike_unwrapper(symbol)
    }

    /// Check whether `symbol` is a user-defined single-param recursive
    /// `PromiseLike` unwrapper.
    ///
    /// Structural rule: `type F<T> = T extends PromiseLike<infer U> ? F<U> : T`
    /// (or `Promise<infer U>`). Predicate/transform conditionals that merely
    /// mention `PromiseLike` are not Awaited-like and must take the normal
    /// conditional evaluation path.
    fn is_user_defined_promiselike_unwrapper(&self, symbol: &Symbol) -> bool {
        if !symbol.has_any_flags(symbol_flags::TYPE_ALIAS) || symbol.declarations.is_empty() {
            return false;
        }
        let decl_arena = if symbol.decl_file_idx != u32::MAX {
            self.ctx.get_arena_for_file(symbol.decl_file_idx)
        } else {
            self.ctx.arena
        };

        symbol.declarations.iter().any(|&decl_idx| {
            let Some(type_alias) = decl_arena.get_type_alias_at(decl_idx) else {
                return false;
            };
            if type_alias
                .type_parameters
                .as_ref()
                .is_none_or(|params| params.nodes.len() != 1)
            {
                return false;
            }
            let param_idx = type_alias
                .type_parameters
                .as_ref()
                .and_then(|params| params.nodes.first())
                .copied()
                .unwrap_or(NodeIndex::NONE);
            let Some(param) = decl_arena.get_type_parameter_at(param_idx) else {
                return false;
            };
            let Some(param_ident) = decl_arena.get_identifier_at(param.name) else {
                return false;
            };
            let param_name = param_ident.escaped_text.as_str();
            let Some(body_node) = decl_arena.get(type_alias.type_node) else {
                return false;
            };
            if body_node.kind != tsz_parser::parser::syntax_kind_ext::CONDITIONAL_TYPE {
                return false;
            }
            let Some(cond) = decl_arena.get_conditional_type(body_node) else {
                return false;
            };
            type_node_is_bare_type_parameter(decl_arena, cond.check_type, param_name)
                && type_node_is_promiselike_infer_pattern(decl_arena, cond.extends_type)
                && type_node_is_recursive_alias_application(
                    decl_arena,
                    cond.true_type,
                    symbol.escaped_name.as_str(),
                )
                && type_node_is_bare_type_parameter(decl_arena, cond.false_type, param_name)
        })
    }
}

fn is_builtin_promise_like_name(name: &str) -> bool {
    matches!(name, "Promise" | "PromiseLike")
}

fn type_node_is_bare_type_parameter(
    arena: &NodeArena,
    type_node: NodeIndex,
    param_name: &str,
) -> bool {
    arena
        .get_type_ref_at(type_node)
        .filter(|type_ref| type_ref.type_arguments.is_none())
        .and_then(|type_ref| arena.get_identifier_at(type_ref.type_name))
        .is_some_and(|ident| ident.escaped_text == param_name)
}

fn type_node_is_promiselike_infer_pattern(arena: &NodeArena, type_node: NodeIndex) -> bool {
    let Some(type_ref) = arena.get_type_ref_at(type_node) else {
        return false;
    };
    let Some(ident) = arena.get_identifier_at(type_ref.type_name) else {
        return false;
    };
    if !is_builtin_promise_like_name(ident.escaped_text.as_str()) {
        return false;
    }
    type_ref.type_arguments.as_ref().is_some_and(|args| {
        args.nodes
            .iter()
            .any(|&arg| type_node_contains_infer(arena, arg))
    })
}

fn type_node_is_recursive_alias_application(
    arena: &NodeArena,
    type_node: NodeIndex,
    alias_name: &str,
) -> bool {
    let Some(type_ref) = arena.get_type_ref_at(type_node) else {
        return false;
    };
    let Some(ident) = arena.get_identifier_at(type_ref.type_name) else {
        return false;
    };
    ident.escaped_text == alias_name
        && type_ref
            .type_arguments
            .as_ref()
            .is_some_and(|args| args.nodes.len() == 1)
}

fn type_node_contains_infer(arena: &NodeArena, root: NodeIndex) -> bool {
    let mut stack = vec![root];
    let mut remaining = 64usize;
    while let Some(idx) = stack.pop() {
        if remaining == 0 {
            return false;
        }
        remaining -= 1;
        if arena.get_infer_type_at(idx).is_some() {
            return true;
        }
        stack.extend(arena.get_children(idx));
    }
    false
}
