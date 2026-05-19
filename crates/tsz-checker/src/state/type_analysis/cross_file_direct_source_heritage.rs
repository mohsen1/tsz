//! Direct source-file interface heritage helpers.

use crate::state::CheckerState;
use tsz_binder::{BinderState, symbol_flags};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn merge_direct_source_file_interface_heritage(
        &mut self,
        mut derived: TypeId,
        declarations: &[(NodeIndex, &NodeArena)],
        delegate_binder: &BinderState,
        symbol_arena: &NodeArena,
    ) -> Option<TypeId> {
        for (decl_idx, arena) in declarations {
            let interface = arena
                .get(*decl_idx)
                .and_then(|node| arena.get_interface(node))?;
            let Some(heritage_clauses) = interface.heritage_clauses.as_ref() else {
                continue;
            };
            for clause_idx in heritage_clauses.nodes.iter().copied() {
                let clause = arena
                    .get(clause_idx)
                    .and_then(|node| arena.get_heritage_clause(node))?;
                for type_idx in clause.types.nodes.iter().copied() {
                    let type_node = arena.get(type_idx)?;
                    let (expression, type_arguments) =
                        if let Some(expr) = arena.get_expr_type_args(type_node) {
                            (expr.expression, expr.type_arguments.as_ref())
                        } else if let Some(type_ref) = arena.get_type_ref(type_node) {
                            (type_ref.type_name, type_ref.type_arguments.as_ref())
                        } else if type_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
                            (type_idx, None)
                        } else {
                            return None;
                        };
                    if type_arguments.is_some_and(|args| !args.nodes.is_empty()) {
                        return None;
                    }
                    let name = arena
                        .get(expression)
                        .and_then(|node| arena.get_identifier(node))
                        .map(|ident| ident.escaped_text.as_str())?;
                    let base_sym_id = delegate_binder.file_locals.get(name)?;
                    let (base_type, base_params) = self.direct_cross_file_interface_lowering(
                        base_sym_id,
                        delegate_binder,
                        symbol_arena,
                        false,
                        true,
                    )?;
                    if !base_params.is_empty() {
                        return None;
                    }
                    derived = self.merge_interface_types(derived, base_type);
                }
            }
        }
        Some(derived)
    }

    pub(super) fn merge_direct_declaration_file_interface_heritage(
        &mut self,
        mut derived: TypeId,
        declarations: &[(NodeIndex, &NodeArena)],
        delegate_binder: &BinderState,
        symbol_arena: &NodeArena,
    ) -> Option<TypeId> {
        for (decl_idx, arena) in declarations {
            let interface = arena
                .get(*decl_idx)
                .and_then(|node| arena.get_interface(node))?;
            let self_name = arena
                .get(interface.name)
                .and_then(|node| arena.get_identifier(node))
                .map(|ident| ident.escaped_text.as_str())?;
            let Some(heritage_clauses) = interface.heritage_clauses.as_ref() else {
                continue;
            };
            for clause_idx in heritage_clauses.nodes.iter().copied() {
                let clause = arena
                    .get(clause_idx)
                    .and_then(|node| arena.get_heritage_clause(node))?;
                if clause.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                    return None;
                }
                for type_idx in clause.types.nodes.iter().copied() {
                    let base_type = self.direct_declaration_file_heritage_base_type(
                        arena,
                        type_idx,
                        delegate_binder,
                        symbol_arena,
                        self_name,
                    )?;
                    derived = self.merge_interface_types(derived, base_type);
                }
            }
        }
        Some(derived)
    }

    fn direct_declaration_file_heritage_base_type(
        &mut self,
        arena: &NodeArena,
        type_idx: NodeIndex,
        delegate_binder: &BinderState,
        symbol_arena: &NodeArena,
        self_name: &str,
    ) -> Option<TypeId> {
        let type_node = arena.get(type_idx)?;
        let (expression, type_arguments) = if let Some(expr) = arena.get_expr_type_args(type_node) {
            (expr.expression, expr.type_arguments.as_ref())
        } else if let Some(type_ref) = arena.get_type_ref(type_node) {
            (type_ref.type_name, type_ref.type_arguments.as_ref())
        } else if type_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            (type_idx, None)
        } else {
            return None;
        };
        if type_arguments.is_some_and(|args| !args.nodes.is_empty()) {
            return None;
        }

        let base_name = Self::entity_name_text_in_direct_arena(arena, expression)?;
        if base_name == self_name {
            return None;
        }

        if let Some(base_sym_id) = delegate_binder.file_locals.get(&base_name)
            && let Some(base_symbol) = delegate_binder.get_symbol(base_sym_id)
            && base_symbol.flags & symbol_flags::INTERFACE != 0
        {
            let declarations =
                self.cross_file_interface_declarations(base_sym_id, delegate_binder, symbol_arena)?;
            if Self::interface_declarations_have_heritage(&declarations) {
                return None;
            }
            let (base_type, base_params) = self.direct_cross_file_interface_lowering(
                base_sym_id,
                delegate_binder,
                symbol_arena,
                false,
                false,
            )?;
            return base_params.is_empty().then_some(base_type);
        }

        let lib_name = base_name.strip_prefix("globalThis.").unwrap_or(&base_name);
        self.resolve_lib_type_by_name(lib_name)
    }
}
