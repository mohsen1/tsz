//! Cross-file interface declaration lowering and heritage merging.
//!
//! Extracted from `cross_file.rs` to keep that module under the 2000-line
//! file-size limit; these methods lower interface declarations that live in a
//! sibling file's arena and merge their cross-file heritage base types.
use crate::state::CheckerState;
use crate::symbols_domain::name_text::expression_name_text_in_arena;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Lower a single interface declaration from a cross-file arena.
    ///
    /// When an interface is declared across multiple files (e.g., global script
    /// interface merging), each cross-file declaration lives in a different
    /// `NodeArena`. This method creates a `TypeLowering` bound to the source arena
    /// and uses name-based resolution via `file_locals` to resolve type references.
    pub(crate) fn lower_cross_file_interface_decl(
        &self,
        arena: &std::sync::Arc<tsz_parser::parser::node::NodeArena>,
        decl_idx: NodeIndex,
        sym_id: SymbolId,
    ) -> TypeId {
        use tsz_lowering::TypeLowering;
        use tsz_solver::is_compiler_managed_type;

        let arena_ref: &tsz_parser::parser::node::NodeArena = arena.as_ref();
        let lib_binders = self.get_lib_binders();

        // Cross-file type resolver: reads identifier text from the cross-file
        // arena, then resolves by name in the current binder's file_locals
        // (which includes merged global symbols from all files).
        let cross_type_resolver = |node_idx: NodeIndex| -> Option<u32> {
            let node = arena_ref.get(node_idx)?;
            let ident = arena_ref.get_identifier(node)?;
            let name = ident.escaped_text.as_str();
            if is_compiler_managed_type(name) {
                return None;
            }
            let sym = self.ctx.binder.file_locals.get(name)?;
            let symbol = self.ctx.binder.get_symbol_with_libs(sym, &lib_binders)?;
            if symbol.has_any_flags(symbol_flags::TYPE) {
                return Some(sym.0);
            }
            None
        };

        let cross_def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> {
            let node = arena_ref.get(node_idx)?;
            let ident = arena_ref.get_identifier(node)?;
            let name = ident.escaped_text.as_str();
            if is_compiler_managed_type(name) {
                return None;
            }
            let sym = self.ctx.binder.file_locals.get(name)?;
            let symbol = self.ctx.binder.get_symbol_with_libs(sym, &lib_binders)?;
            if symbol.has_any_flags(symbol_flags::TYPE) {
                Some(self.ctx.get_or_create_def_id(sym))
            } else {
                None
            }
        };

        let cross_value_resolver = |node_idx: NodeIndex| -> Option<u32> {
            let node = arena_ref.get(node_idx)?;
            let ident = arena_ref.get_identifier(node)?;
            let name = ident.escaped_text.as_str();
            let sym = self.ctx.binder.file_locals.get(name)?;
            let symbol = self.ctx.binder.get_symbol_with_libs(sym, &lib_binders)?;
            if (symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0 {
                Some(sym.0)
            } else {
                None
            }
        };

        let type_param_bindings = self.get_type_param_bindings();
        let lowering = TypeLowering::with_hybrid_resolver(
            arena_ref,
            self.ctx.types,
            &cross_type_resolver,
            &cross_def_id_resolver,
            &cross_value_resolver,
        )
        .with_type_param_bindings(type_param_bindings);

        lowering.lower_interface_declarations_with_symbol(&[decl_idx], sym_id)
    }

    /// Merge heritage types from cross-file interface declarations.
    ///
    /// `merge_interface_heritage_types` uses `self.ctx.arena` to read heritage
    /// clauses, so it silently skips cross-file declarations. This method handles
    /// those skipped declarations by reading from the source arena and resolving
    /// base types via `file_locals` name lookup.
    pub(crate) fn merge_cross_file_heritage(
        &mut self,
        declarations: &[NodeIndex],
        sym_id: SymbolId,
        mut derived_type: TypeId,
    ) -> TypeId {
        use tsz_scanner::SyntaxKind;

        for &decl_idx in declarations {
            let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) else {
                continue;
            };
            for arena in arenas.iter() {
                // Skip the local arena (already processed by merge_interface_heritage_types)
                if std::ptr::eq(arena.as_ref(), self.ctx.arena) {
                    continue;
                }
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
                    if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                        continue;
                    }

                    for &type_idx in &heritage.types.nodes {
                        let Some(type_node) = arena.get(type_idx) else {
                            continue;
                        };

                        let (expr_idx, type_arguments) =
                            if let Some(expr) = arena.get_expr_type_args(type_node) {
                                (expr.expression, expr.type_arguments.as_ref())
                            } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                                if let Some(type_ref) = arena.get_type_ref(type_node) {
                                    (type_ref.type_name, type_ref.type_arguments.as_ref())
                                } else {
                                    (type_idx, None)
                                }
                            } else {
                                (type_idx, None)
                            };

                        let Some(name) = expression_name_text_in_arena(arena, expr_idx) else {
                            continue;
                        };
                        let Some(base_sym_id) = self.resolve_cross_file_global_type_symbol(&name)
                        else {
                            continue;
                        };

                        let mut base_type = self.get_type_of_symbol(base_sym_id);
                        if base_type == TypeId::ERROR || base_type == TypeId::UNKNOWN {
                            continue;
                        }
                        if let Some(type_arguments) = type_arguments {
                            let base_params = self.get_type_params_for_symbol(base_sym_id);
                            if !base_params.is_empty() {
                                let mut type_args = Vec::with_capacity(type_arguments.nodes.len());
                                for &arg_idx in &type_arguments.nodes {
                                    type_args.push(
                                        self.resolve_cross_file_heritage_type_arg(arena, arg_idx),
                                    );
                                }
                                while type_args.len() < base_params.len() {
                                    let param = &base_params[type_args.len()];
                                    type_args.push(
                                        param
                                            .default
                                            .or(param.constraint)
                                            .unwrap_or(TypeId::UNKNOWN),
                                    );
                                }
                                if type_args.len() > base_params.len() {
                                    type_args.truncate(base_params.len());
                                }
                                let substitution =
                                    crate::query_boundaries::common::TypeSubstitution::from_args(
                                        self.ctx.types,
                                        &base_params,
                                        &type_args,
                                    );
                                base_type = crate::query_boundaries::common::instantiate_type(
                                    self.ctx.types,
                                    base_type,
                                    &substitution,
                                );
                            }
                        }

                        derived_type = self.merge_interface_types(derived_type, base_type);
                    }
                }
            }
        }

        derived_type
    }
}
