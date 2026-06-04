use crate::symbols_domain::name_text::{entity_name_text_in_arena, expression_name_text_in_arena};

use crate::types_domain::unique_symbol_arena::{
    has_declared_unique_symbol_owner, is_unique_symbol_type_annotation_unwrapped,
};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::{NodeAccess, NodeArena};

use tsz_solver::TypeId;

use tsz_solver::is_compiler_managed_type;

use super::type_node::TypeNodeChecker;

thread_local! {
    /// Depth and active-set guards for recursive type-alias resolution chains
    /// (see `ensure_type_alias_resolved`). Module-scoped rather than
    /// function-scoped so they can be reset at independent-compilation
    /// boundaries: the push/pop around `ensure_type_alias_resolved_inner` is
    /// manual (non-RAII), so a panic unwinding through that call — caught
    /// upstream by the batch driver — would otherwise leave a stale `DefId` in
    /// the active set, wrongly short-circuiting that alias in the next project.
    static ALIAS_RESOLVE_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static ALIAS_RESOLVE_STACK: std::cell::RefCell<Vec<tsz_solver::def::DefId>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Reset the type-alias resolution depth counter and active-set stack.
/// Called from `clear_all_thread_local_state` at batch row boundaries.
pub(crate) fn reset_alias_resolve_state() {
    ALIAS_RESOLVE_DEPTH.with(|c| c.set(0));
    ALIAS_RESOLVE_STACK.with(|stack| stack.borrow_mut().clear());
}

#[cfg(test)]
pub(crate) fn dirty_alias_resolve_state_for_test() {
    ALIAS_RESOLVE_DEPTH.with(|c| c.set(7));
    ALIAS_RESOLVE_STACK.with(|stack| stack.borrow_mut().push(tsz_solver::def::DefId::INVALID));
}

#[cfg(test)]
pub(crate) fn alias_resolve_state_is_clear_for_test() -> bool {
    ALIAS_RESOLVE_DEPTH.with(std::cell::Cell::get) == 0
        && ALIAS_RESOLVE_STACK.with(|stack| stack.borrow().is_empty())
}

include!("type_node_resolution_parts/part1.rs");
include!("type_node_resolution_parts/part2.rs");
