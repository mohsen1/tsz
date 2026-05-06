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
        if interface_sym_id == target_sym_id {
            return true;
        }

        let Some(symbol) = self.ctx.binder.get_symbol(interface_sym_id) else {
            return false;
        };

        // Check each declaration's heritage clauses
        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = self.ctx.arena.get_interface(node) else {
                continue;
            };
            let Some(ref heritage_clauses) = interface.heritage_clauses else {
                continue;
            };
            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = self.ctx.arena.get(type_idx) else {
                        continue;
                    };
                    // Extract the base expression (might be an ExpressionWithTypeArguments)
                    let expr_idx = if let Some(eta) = self.ctx.arena.get_expr_type_args(type_node) {
                        eta.expression
                    } else {
                        type_idx
                    };
                    if let Some(base_sym) = self.resolve_heritage_symbol(expr_idx)
                        && base_sym == target_sym_id
                    {
                        return true;
                    }
                }
            }
        }
        false
    }
}
