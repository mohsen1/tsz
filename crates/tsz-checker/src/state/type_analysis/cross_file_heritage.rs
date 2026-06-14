//! Cross-file interface heritage merging helpers.

use crate::state::CheckerState;
use crate::symbols_domain::name_text::expression_name_text_in_arena;
use crate::types_domain::queries::lib_resolution::keyword_syntax_to_type_id;
use tsz_binder::SymbolId;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::{NodeArena, NodeIndex};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn resolve_cross_file_heritage_type_arg(
        &mut self,
        arena: &NodeArena,
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
            origin: tsz_solver::TypeParamOrigin::User,
        })
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
                        let Some((expr_idx, type_arguments)) =
                            Self::heritage_base_expr_and_args(arena, type_idx)
                        else {
                            continue;
                        };
                        let Some(name) = expression_name_text_in_arena(arena, expr_idx) else {
                            continue;
                        };
                        let Some(base_sym_id) = self.resolve_cross_file_global_type_symbol(&name)
                        else {
                            continue;
                        };

                        let base_type = self.get_type_of_symbol(base_sym_id);
                        if base_type == TypeId::ERROR || base_type == TypeId::UNKNOWN {
                            continue;
                        }
                        let arg_indices = type_arguments
                            .map(|args| args.nodes.as_slice())
                            .unwrap_or(&[]);
                        let base_type = self.instantiate_cross_file_heritage_base(
                            base_sym_id,
                            base_type,
                            arg_indices,
                            arena,
                        );

                        derived_type = self.merge_interface_types_heritage(derived_type, base_type);
                    }
                }
            }
        }

        derived_type
    }

    /// Merge `extends` heritage for an interface whose declarations live in
    /// another file, reading the heritage clause from the **owner** arena and
    /// resolving each base in the **owner** binder.
    ///
    /// `merge_interface_heritage_types` reads heritage clauses through
    /// `self.ctx.arena`, so when a generic interface reference is resolved
    /// cross-file (the `type_reference_symbol_type_with_params` text-based
    /// path), the owner module's `extends` clause is invisible and every
    /// inherited member is dropped. `merge_cross_file_heritage` only covers the
    /// case where `declaration_arenas` carries an explicit out-of-arena entry
    /// (declaration merging), and resolves bases against the *importing* file's
    /// scope, so a base that is merely a sibling of the imported interface in
    /// its own module never resolves.
    ///
    /// This method discovers the owner arena by the symbol's declaring file
    /// index, resolves bases against the owner binder's `file_locals` (falling
    /// back to globals/libs for re-exported or ambient bases), and instantiates
    /// the base with arguments resolved in the **current** type-parameter scope.
    /// Because the merge runs in this checker — with the derived interface's own
    /// parameters already pushed — inherited members carry the same parameter
    /// identities as the derived body, so later instantiation substitutes them
    /// correctly (unlike a delegated child-checker body, whose parameters have
    /// independent identities, #13165). The base type itself is resolved through
    /// the normal symbol-type path, which recurses through this same merge, so
    /// multi-level chains compose transitively.
    pub(crate) fn merge_cross_file_interface_heritage(
        &mut self,
        sym_id: SymbolId,
        declarations: &[NodeIndex],
        mut derived_type: TypeId,
    ) -> TypeId {
        let Some(file_idx) = self.ctx.resolve_symbol_file_index(sym_id) else {
            return derived_type;
        };
        if file_idx == self.ctx.current_file_idx {
            return derived_type;
        }
        // Cheap borrowed pre-scan: skip the owner arena/binder Arc clones (and
        // all downstream work) for the common case of an imported interface
        // with no `extends` clause, where the merge would be a no-op anyway.
        let source_arena_ref = self.ctx.get_arena_for_file(file_idx as u32);
        if std::ptr::eq(source_arena_ref, self.ctx.arena)
            || !declarations
                .iter()
                .any(|&decl_idx| Self::interface_decl_has_extends(source_arena_ref, decl_idx))
        {
            return derived_type;
        }

        // Own the owner arena/binder so `&source_arena` stays valid (independent
        // of `self`) across the `&mut self` resolution calls below.
        let Some(source_arena) = self
            .ctx
            .all_arenas
            .as_ref()
            .and_then(|arenas| arenas.get(file_idx))
            .cloned()
        else {
            return derived_type;
        };
        let source_binder = self
            .ctx
            .all_binders
            .as_ref()
            .and_then(|binders| binders.get(file_idx))
            .cloned();

        for &decl_idx in declarations {
            let Some(node) = source_arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = source_arena.get_interface(node) else {
                continue;
            };
            let Some(ref heritage_clauses) = interface.heritage_clauses else {
                continue;
            };
            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = source_arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = source_arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                for &type_idx in &heritage.types.nodes {
                    let Some((expr_idx, type_arguments)) =
                        Self::heritage_base_expr_and_args(&source_arena, type_idx)
                    else {
                        continue;
                    };
                    let Some(name) = expression_name_text_in_arena(&source_arena, expr_idx) else {
                        continue;
                    };
                    // Resolve the base in the owner module first (its siblings
                    // and locally-bound imports), then globals/libs.
                    let Some(base_sym_id) = source_binder
                        .as_ref()
                        .and_then(|binder| binder.file_locals.get(&name))
                        .or_else(|| self.resolve_cross_file_global_type_symbol(&name))
                    else {
                        continue;
                    };

                    let base_type = self.get_type_of_symbol(base_sym_id);
                    if base_type == TypeId::ERROR || base_type == TypeId::UNKNOWN {
                        continue;
                    }
                    let arg_indices = type_arguments
                        .map(|args| args.nodes.as_slice())
                        .unwrap_or(&[]);
                    let base_type = self.instantiate_cross_file_heritage_base(
                        base_sym_id,
                        base_type,
                        arg_indices,
                        &source_arena,
                    );
                    derived_type = self.merge_interface_types_heritage(derived_type, base_type);
                }
            }
        }

        derived_type
    }

    /// Whether the interface declaration at `decl_idx` in `arena` has a
    /// non-empty heritage clause (`extends`). Cheap predicate used to gate the
    /// cross-file heritage merge before any allocation.
    fn interface_decl_has_extends(arena: &NodeArena, decl_idx: NodeIndex) -> bool {
        arena
            .get(decl_idx)
            .and_then(|node| arena.get_interface(node))
            .and_then(|interface| interface.heritage_clauses.as_ref())
            .is_some_and(|clauses| !clauses.nodes.is_empty())
    }

    /// Destructure a heritage `extends` type node into its base expression and
    /// optional type arguments, reading from `arena`. Handles both the
    /// `ExpressionWithTypeArguments` form (class/interface `extends`) and a bare
    /// `TypeReference`, falling back to the node itself with no arguments.
    fn heritage_base_expr_and_args(
        arena: &NodeArena,
        type_idx: NodeIndex,
    ) -> Option<(NodeIndex, Option<&tsz_parser::parser::NodeList>)> {
        let type_node = arena.get(type_idx)?;
        if let Some(expr) = arena.get_expr_type_args(type_node) {
            Some((expr.expression, expr.type_arguments.as_ref()))
        } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
            match arena.get_type_ref(type_node) {
                Some(type_ref) => Some((type_ref.type_name, type_ref.type_arguments.as_ref())),
                None => Some((type_idx, None)),
            }
        } else {
            Some((type_idx, None))
        }
    }

    /// Instantiate a cross-file heritage base type with the heritage clause's
    /// type arguments, resolving each argument in the **current** type-parameter
    /// scope (so inherited members carry the derived interface's own parameter
    /// identities). Missing trailing arguments fall back to each parameter's
    /// default/constraint; extra arguments are dropped. Returns `base_type`
    /// unchanged when the clause supplies no arguments or the base is
    /// non-generic.
    fn instantiate_cross_file_heritage_base(
        &mut self,
        base_sym_id: SymbolId,
        base_type: TypeId,
        arg_indices: &[NodeIndex],
        arena: &NodeArena,
    ) -> TypeId {
        if arg_indices.is_empty() {
            return base_type;
        }
        let base_params = self.get_type_params_for_symbol(base_sym_id);
        if base_params.is_empty() {
            return base_type;
        }
        let mut type_args = Vec::with_capacity(base_params.len().max(arg_indices.len()));
        for &arg_idx in arg_indices {
            type_args.push(self.resolve_cross_file_heritage_type_arg(arena, arg_idx));
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
        type_args.truncate(base_params.len());
        let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
            self.ctx.types,
            &base_params,
            &type_args,
        );
        crate::query_boundaries::common::instantiate_type(self.ctx.types, base_type, &substitution)
    }
}
