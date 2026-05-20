//! Shared helpers for cross-file delegation and heritage argument recovery.

use crate::state::CheckerState;
use crate::symbols_domain::name_text::expression_name_text_in_arena;
use crate::types_domain::queries::lib_resolution::keyword_syntax_to_type_id;
use tsz_parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

thread_local! {
    static CROSS_ARENA_INTERFACE_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

impl<'a> CheckerState<'a> {
    pub(crate) fn enter_cross_arena_interface_delegation() {
        CROSS_ARENA_INTERFACE_DEPTH.with(|c| c.set(c.get() + 1));
    }

    pub(crate) fn leave_cross_arena_interface_delegation() {
        CROSS_ARENA_INTERFACE_DEPTH.with(|c| c.set(c.get().saturating_sub(1)));
    }

    pub(crate) fn in_cross_arena_interface_delegation() -> bool {
        CROSS_ARENA_INTERFACE_DEPTH.with(|c| c.get() > 0)
    }

    pub(super) fn resolve_cross_file_global_type_symbol(
        &self,
        name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let normalized = name.strip_prefix("globalThis.").unwrap_or(name);
        let lib_binders = self.get_lib_binders();
        self.ctx
            .binder
            .file_locals
            .get(normalized)
            .or_else(|| {
                self.ctx
                    .binder
                    .get_global_type_with_libs(normalized, &lib_binders)
            })
            .or_else(|| {
                normalized
                    .rsplit('.')
                    .next()
                    .filter(|tail| *tail != normalized)
                    .and_then(|tail| {
                        self.ctx.binder.file_locals.get(tail).or_else(|| {
                            self.ctx
                                .binder
                                .get_global_type_with_libs(tail, &lib_binders)
                        })
                    })
            })
    }

    pub(super) fn resolve_cross_file_heritage_type_arg(
        &mut self,
        arena: &tsz_parser::NodeArena,
        node_idx: NodeIndex,
    ) -> TypeId {
        let Some(node) = arena.get(node_idx) else {
            return TypeId::UNKNOWN;
        };

        if let Some(builtin) = keyword_syntax_to_type_id(node.kind) {
            return builtin;
        }

        let name = if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            arena
                .get_type_ref(node)
                .and_then(|type_ref| expression_name_text_in_arena(arena, type_ref.type_name))
        } else {
            expression_name_text_in_arena(arena, node_idx)
        };

        let Some(name) = name else {
            return TypeId::UNKNOWN;
        };
        if name == "BuiltinIteratorReturn" {
            return self.builtin_iterator_return_intrinsic_type();
        }
        if let Some(&type_id) = self.ctx.type_parameter_scope.get(&name) {
            return type_id;
        }
        if let Some(sym_id) = self.resolve_cross_file_global_type_symbol(&name) {
            return self.get_type_of_symbol(sym_id);
        }

        let atom = self.ctx.types.intern_string(&name);
        self.ctx.types.type_param(tsz_solver::TypeParamInfo {
            name: atom,
            constraint: None,
            default: None,
            is_const: false,
        })
    }
}
