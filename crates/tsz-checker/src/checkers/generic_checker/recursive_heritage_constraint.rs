//! Recursive heritage constraint helpers used by TS2344 validation.
//!
//! Extracted from `constraint_validation.rs` to keep that file under the
//! checker per-file size guard. Behavior is unchanged; only the physical
//! location of these helpers moved.

use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn satisfies_recursive_heritage_constraint(
        &self,
        type_arg: TypeId,
        constraint: TypeId,
    ) -> bool {
        let db = self.ctx.types.as_type_database();

        // Get the Application base DefId from the constraint.
        // e.g., for AA<BB>, get the DefId of AA.
        let Some(constraint_base_def) = query::application_base_def_id(db, constraint) else {
            return false;
        };

        // Get the type_arg's DefId (it must be an interface/class, i.e., Lazy type).
        let type_arg_def = query::lazy_def_id(db, type_arg);
        let Some(type_arg_def) = type_arg_def else {
            // When the type_arg is an Application (e.g., AA<BB>) with the same base
            // as the constraint (e.g., AA<AA<BB>>), AND the inner type arguments of
            // the Application type_arg extend the constraint base via heritage, the
            // constraint is coinductively satisfied. This handles recursive constraints
            // like `T extends AA<T>` where `interface BB extends AA<AA<BB>>` — checking
            // `AA<BB>` against `AA<AA<BB>>` leads to infinite nesting that tsc resolves
            // via deeply-nested type detection.
            if let Some((Some(type_arg_base_def), ref type_arg_args)) =
                query::application_base_def_and_args(db, type_arg)
                && type_arg_base_def == constraint_base_def
            {
                // Same base type (e.g., both are AA<...>).
                // Check if any inner type argument extends the constraint base,
                // which would create the circular recursion pattern.
                for &inner_arg in type_arg_args.iter() {
                    if let Some(inner_def) = query::lazy_def_id(db, inner_arg) {
                        let inner_sym = self.ctx.def_to_symbol_id(inner_def);
                        let constraint_sym = self.ctx.def_to_symbol_id(constraint_base_def);
                        if let (Some(inner_sym_id), Some(constraint_sym_id)) =
                            (inner_sym, constraint_sym)
                            && self.interface_extends_symbol(inner_sym_id, constraint_sym_id)
                        {
                            return true;
                        }
                    }
                }
            }
            return false;
        };

        // Resolve DefIds to SymbolIds
        let type_arg_sym = self.ctx.def_to_symbol_id(type_arg_def);
        let constraint_base_sym = self.ctx.def_to_symbol_id(constraint_base_def);

        let (Some(type_arg_sym_id), Some(constraint_base_sym_id)) =
            (type_arg_sym, constraint_base_sym)
        else {
            return false;
        };

        // Check if type_arg's interface heritage chain includes the constraint's
        // base interface. Walk the heritage clauses in the binder to find if BB
        // extends any instantiation of AA.
        self.interface_extends_symbol(type_arg_sym_id, constraint_base_sym_id)
    }

    /// Check if an interface symbol extends (directly or transitively) a target symbol.
    pub(super) fn interface_extends_symbol(
        &self,
        interface_sym_id: tsz_binder::SymbolId,
        target_sym_id: tsz_binder::SymbolId,
    ) -> bool {
        self.interface_extends_symbol_inner(interface_sym_id, target_sym_id, &mut Vec::new(), None)
    }

    fn interface_extends_symbol_inner(
        &self,
        interface_sym_id: tsz_binder::SymbolId,
        target_sym_id: tsz_binder::SymbolId,
        seen: &mut Vec<tsz_binder::SymbolId>,
        preferred_binder: Option<&tsz_binder::BinderState>,
    ) -> bool {
        if interface_sym_id == target_sym_id {
            return true;
        }
        if seen.contains(&interface_sym_id) {
            return false;
        }
        seen.push(interface_sym_id);

        let Some((owner_binder, symbol)) = preferred_binder
            .and_then(|binder| {
                binder
                    .get_symbol(interface_sym_id)
                    .map(|symbol| (binder, symbol))
            })
            .or_else(|| self.symbol_and_binder_for_heritage(interface_sym_id))
        else {
            return false;
        };

        // Check each declaration's heritage clauses
        for &decl_idx in &symbol.declarations {
            let arena =
                owner_binder.arena_for_declaration_or(interface_sym_id, decl_idx, self.ctx.arena);
            let Some(node) = arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = arena.get_interface(node) else {
                continue;
            };
            let Some(ref heritage_clauses) = interface.heritage_clauses else {
                continue;
            };
            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = arena.get(type_idx) else {
                        continue;
                    };
                    // Extract the base expression (might be an ExpressionWithTypeArguments)
                    let expr_idx = if let Some(eta) = arena.get_expr_type_args(type_node) {
                        eta.expression
                    } else {
                        type_idx
                    };
                    if let Some(base_sym) = owner_binder
                        .node_symbols
                        .get(&expr_idx.0)
                        .copied()
                        .or_else(|| {
                            self.resolve_heritage_symbol_with_binder(arena, owner_binder, expr_idx)
                        })
                        .or_else(|| self.resolve_heritage_symbol(expr_idx))
                        && (base_sym == target_sym_id
                            || self.interface_extends_symbol_inner(
                                base_sym,
                                target_sym_id,
                                seen,
                                Some(owner_binder),
                            ))
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn symbol_and_binder_for_heritage(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<(&tsz_binder::BinderState, &tsz_binder::Symbol)> {
        if let Some(file_idx) = self.ctx.resolve_symbol_file_index(sym_id)
            && let Some(binder) = self.ctx.get_binder_for_file(file_idx)
            && let Some(symbol) = binder.get_symbol(sym_id)
        {
            return Some((binder, symbol));
        }
        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
            return Some((self.ctx.binder, symbol));
        }
        if let Some((binder, symbol)) = self.ctx.lib_contexts.iter().find_map(|lib| {
            lib.binder
                .get_symbol(sym_id)
                .map(|symbol| (lib.binder.as_ref(), symbol))
        }) {
            return Some((binder, symbol));
        }
        self.ctx.all_binders.as_ref().and_then(|binders| {
            binders.iter().find_map(|binder| {
                binder
                    .get_symbol(sym_id)
                    .map(|symbol| (binder.as_ref(), symbol))
            })
        })
    }

    fn resolve_heritage_symbol_with_binder(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        binder: &tsz_binder::BinderState,
        expr_idx: tsz_parser::NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        let node = arena.get(expr_idx)?;
        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            return binder.resolve_identifier(arena, expr_idx);
        }
        None
    }

    pub(super) fn symbol_id_for_heritage_type_name(
        &mut self,
        type_id: TypeId,
    ) -> Option<tsz_binder::SymbolId> {
        let name = self.format_type_diagnostic(type_id);
        let name = name.strip_prefix("globalThis.").unwrap_or(&name);
        if name.is_empty()
            || !name
                .chars()
                .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
        {
            return None;
        }

        if let Some(index) = &self.ctx.global_file_locals_index
            && let Some(candidates) = index.get(name)
        {
            for &(file_idx, sym_id) in candidates {
                if self
                    .ctx
                    .get_binder_for_file(file_idx)
                    .and_then(|binder| binder.get_symbol(sym_id))
                    .is_some()
                {
                    self.ctx.register_symbol_file_target(sym_id, file_idx);
                    return Some(sym_id);
                }
            }
        }

        let lib_binders = self.get_lib_binders();
        self.ctx
            .binder
            .get_global_type_with_libs(name, &lib_binders)
    }

    pub(super) fn member_has_conflicting_constraint_property(
        &mut self,
        member: TypeId,
        constraint: TypeId,
    ) -> bool {
        let member = self.resolve_lazy_type(member);
        let member = self.evaluate_type_for_assignability(member);
        let constraint = self.resolve_lazy_type(constraint);
        let constraint = self.evaluate_type_for_assignability(constraint);
        let db = self.ctx.types.as_type_database();
        let Some(member_shape) = crate::query_boundaries::common::object_shape_for_type(db, member)
        else {
            return false;
        };
        let Some(constraint_shape) =
            crate::query_boundaries::common::object_shape_for_type(db, constraint)
        else {
            return false;
        };
        member_shape.properties.iter().any(|member_prop| {
            constraint_shape
                .properties
                .iter()
                .find(|constraint_prop| constraint_prop.name == member_prop.name)
                .is_some_and(|constraint_prop| {
                    let member_type = self.resolve_lazy_type(member_prop.type_id);
                    let member_type = self.evaluate_type_for_assignability(member_type);
                    let constraint_type = self.resolve_lazy_type(constraint_prop.type_id);
                    let constraint_type = self.evaluate_type_for_assignability(constraint_type);
                    if crate::query_boundaries::assignability::are_types_structurally_identical(
                        self.ctx.types,
                        &self.ctx,
                        member_type,
                        constraint_type,
                    ) {
                        return false;
                    }
                    if self.is_assignable_to(member_type, constraint_type) {
                        return false;
                    }
                    // Some recursive DOM property types are interned through
                    // different paths and can miss both relation and canonical
                    // identity checks while still rendering identically. They
                    // are not genuine merge conflicts.
                    self.format_type_diagnostic(member_type)
                        != self.format_type_diagnostic(constraint_type)
                })
        })
    }
}
