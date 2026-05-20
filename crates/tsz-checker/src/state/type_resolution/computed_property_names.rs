use crate::state::CheckerState;
use crate::types_domain::queries::core::get_literal_or_well_known_property_name;
use crate::types_domain::queries::lib_resolution::resolve_name_to_lib_symbol;
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    pub(crate) fn prewarm_member_type_reference_params(
        &mut self,
        declarations: &[NodeIndex],
    ) -> rustc_hash::FxHashMap<tsz_solver::def::DefId, Vec<tsz_solver::TypeParamInfo>> {
        let declarations_with_arenas: Vec<_> = declarations
            .iter()
            .map(|&decl_idx| (decl_idx, self.ctx.arena))
            .collect();
        self.prewarm_member_type_reference_params_in_arenas(&declarations_with_arenas)
    }

    pub(crate) fn prewarm_member_type_reference_params_in_arenas(
        &mut self,
        declarations: &[(NodeIndex, &NodeArena)],
    ) -> rustc_hash::FxHashMap<tsz_solver::def::DefId, Vec<tsz_solver::TypeParamInfo>> {
        // PERF: declaration files like react16.d.ts contain extremely large interface
        // graphs. Walking every descendant of every interface just to prewarm an
        // optional cache can dominate checker time. The lowering path already falls
        // back to `ctx.get_def_type_params(def_id)` on demand, so skipping the eager
        // prewarm here preserves correctness while avoiding repeated full-tree scans.
        if self.ctx.is_declaration_file() {
            return rustc_hash::FxHashMap::default();
        }

        let mut stack = Vec::new();
        let mut params_by_def = rustc_hash::FxHashMap::default();

        for &(decl_idx, decl_arena) in declarations {
            stack.push(decl_idx);

            while let Some(node_idx) = stack.pop() {
                let Some(node) = decl_arena.get(node_idx) else {
                    continue;
                };

                if node.kind == syntax_kind_ext::TYPE_REFERENCE
                    && let Some(type_ref) = decl_arena.get_type_ref(node)
                {
                    let has_type_args = type_ref
                        .type_arguments
                        .as_ref()
                        .is_some_and(|args| !args.nodes.is_empty());
                    if !has_type_args
                        && let Some(sym_id) = self
                            .resolve_type_reference_symbol_in_arena(decl_arena, type_ref.type_name)
                    {
                        let def_id = self.ctx.get_or_create_def_id(sym_id);
                        let params = self.get_type_params_for_symbol(sym_id);
                        if !params.is_empty() {
                            params_by_def.insert(def_id, params);
                        }
                    }
                }

                stack.extend(decl_arena.get_children(node_idx));
            }
        }

        params_by_def
    }

    /// Pre-compute property names for computed property name expressions in interface members.
    /// Iterates over all members of all declarations, finds `COMPUTED_PROPERTY_NAME` nodes,
    /// evaluates the expression type, and builds a map from expression `NodeIndex` to Atom.
    pub(crate) fn precompute_computed_property_names(
        &mut self,
        declarations: &[NodeIndex],
    ) -> rustc_hash::FxHashMap<(NodeIndex, usize), tsz_common::Atom> {
        let declarations_with_arenas: Vec<_> = declarations
            .iter()
            .map(|&decl_idx| (decl_idx, self.ctx.arena))
            .collect();
        self.precompute_computed_property_names_in_arenas(&declarations_with_arenas)
    }

    pub(crate) fn precompute_computed_property_names_in_arenas(
        &mut self,
        declarations: &[(NodeIndex, &NodeArena)],
    ) -> rustc_hash::FxHashMap<(NodeIndex, usize), tsz_common::Atom> {
        let mut map = rustc_hash::FxHashMap::default();
        for &(decl_idx, decl_arena) in declarations {
            let arena_key = decl_arena as *const NodeArena as usize;
            let Some(node) = decl_arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = decl_arena.get_interface(node) else {
                continue;
            };
            for &member_idx in &interface.members.nodes {
                let Some(member) = decl_arena.get(member_idx) else {
                    continue;
                };
                // Get the name node from signature or accessor
                let name_idx = if let Some(sig) = decl_arena.get_signature(member) {
                    sig.name
                } else if let Some(acc) = decl_arena.get_accessor(member) {
                    acc.name
                } else {
                    continue;
                };
                let Some(name_node) = decl_arena.get(name_idx) else {
                    continue;
                };
                if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    continue;
                }
                let Some(computed) = decl_arena.get_computed_property(name_node) else {
                    continue;
                };
                if let Some(name) =
                    self.resolve_computed_property_name_in_arena(decl_arena, name_idx)
                {
                    map.insert(
                        (computed.expression, arena_key),
                        self.ctx.types.intern_string(&name),
                    );
                }
            }
        }
        map
    }

    pub(crate) fn precompute_symbol_named_computed_property_names(
        &mut self,
        declarations: &[NodeIndex],
    ) -> rustc_hash::FxHashSet<(NodeIndex, usize)> {
        let declarations_with_arenas: Vec<_> = declarations
            .iter()
            .map(|&decl_idx| (decl_idx, self.ctx.arena))
            .collect();
        self.precompute_symbol_named_computed_property_names_in_arenas(&declarations_with_arenas)
    }

    pub(crate) fn precompute_symbol_named_computed_property_names_in_arenas(
        &mut self,
        declarations: &[(NodeIndex, &NodeArena)],
    ) -> rustc_hash::FxHashSet<(NodeIndex, usize)> {
        let mut set = rustc_hash::FxHashSet::default();
        for &(decl_idx, decl_arena) in declarations {
            let arena_key = decl_arena as *const NodeArena as usize;
            let Some(node) = decl_arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = decl_arena.get_interface(node) else {
                continue;
            };
            for &member_idx in &interface.members.nodes {
                let Some(member) = decl_arena.get(member_idx) else {
                    continue;
                };
                let name_idx = if let Some(sig) = decl_arena.get_signature(member) {
                    sig.name
                } else if let Some(acc) = decl_arena.get_accessor(member) {
                    acc.name
                } else {
                    continue;
                };
                let Some(name_node) = decl_arena.get(name_idx) else {
                    continue;
                };
                if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    continue;
                }
                let Some(computed) = decl_arena.get_computed_property(name_node) else {
                    continue;
                };
                if self
                    .resolve_computed_property_name_in_arena(decl_arena, name_idx)
                    .is_some_and(|name| {
                        name.starts_with("__unique_") || name.starts_with("__symbol_")
                    })
                {
                    set.insert((computed.expression, arena_key));
                }
            }
        }
        set
    }

    fn resolve_type_reference_symbol_in_arena(
        &self,
        arena: &NodeArena,
        type_name: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        if std::ptr::eq(arena, self.ctx.arena) {
            return self
                .resolve_type_symbol_for_lowering(type_name)
                .map(tsz_binder::SymbolId);
        }

        let name = arena.get_identifier_text(type_name)?;
        resolve_name_to_lib_symbol(
            name,
            self.ctx.binder,
            self.ctx.global_file_locals_index.as_deref(),
            self.ctx
                .all_binders
                .as_ref()
                .map(|binders| binders.as_ref().as_slice()),
            &self.ctx.lib_contexts,
        )
    }

    fn resolve_computed_property_name_in_arena(
        &mut self,
        arena: &NodeArena,
        name_idx: NodeIndex,
    ) -> Option<String> {
        if std::ptr::eq(arena, self.ctx.arena) {
            return self.resolve_local_computed_property_name(name_idx);
        }

        if let Some(name) = get_literal_or_well_known_property_name(arena, name_idx) {
            return Some(name);
        }

        let name_node = arena.get(name_idx)?;
        let computed = arena.get_computed_property(name_node)?;
        if let Some(name) =
            self.computed_expression_literal_name_in_arena(arena, computed.expression)
        {
            return Some(name);
        }
        let sym_id = self.resolve_computed_property_symbol_in_arena(arena, computed.expression)?;
        Some(format!("__unique_{}", sym_id.0))
    }

    fn resolve_local_computed_property_name(&mut self, name_idx: NodeIndex) -> Option<String> {
        if let Some(name) = get_literal_or_well_known_property_name(self.ctx.arena, name_idx) {
            return Some(name);
        }

        let name_node = self.ctx.arena.get(name_idx)?;
        let computed = self.ctx.arena.get_computed_property(name_node)?;
        if let Some(name) =
            self.computed_expression_literal_name_in_arena(self.ctx.arena, computed.expression)
        {
            return Some(name);
        }
        let prev = self.ctx.checking_computed_property_name;
        self.ctx.checking_computed_property_name = Some(name_idx);
        let prev_preserve = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;
        let expr_type = self.get_type_of_node(computed.expression);
        self.ctx.preserve_literal_types = prev_preserve;
        self.ctx.checking_computed_property_name = prev;
        if let Some(name) = crate::query_boundaries::type_computation::access::literal_property_name(
            self.ctx.types,
            expr_type,
        ) {
            Some(self.ctx.types.resolve_atom_ref(name).to_string())
        } else if let Some(name) =
            self.symbol_valued_binding_property_name(computed.expression, expr_type)
        {
            Some(name)
        } else {
            crate::query_boundaries::common::unique_symbol_ref(self.ctx.types, expr_type)
                .map(|sym_ref| format!("__unique_{}", sym_ref.0))
        }
    }

    fn computed_expression_literal_name_in_arena(
        &self,
        arena: &NodeArena,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(expr_idx)?;
        if matches!(
            node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
        ) {
            return crate::types_domain::queries::core::get_literal_property_name(arena, expr_idx);
        }
        None
    }

    fn resolve_computed_property_symbol_in_arena(
        &self,
        arena: &NodeArena,
        mut expr_idx: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        while let Some(node) = arena.get(expr_idx)
            && node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
        {
            expr_idx = arena.get_parenthesized(node)?.expression;
        }

        let name = if arena
            .get(expr_idx)
            .is_some_and(|node| node.kind == SyntaxKind::Identifier as u16)
        {
            arena.get_identifier_text(expr_idx)?.to_string()
        } else {
            crate::symbols_domain::name_text::expression_name_text_in_arena(arena, expr_idx)?
        };

        resolve_name_to_lib_symbol(
            &name,
            self.ctx.binder,
            self.ctx.global_file_locals_index.as_deref(),
            self.ctx
                .all_binders
                .as_ref()
                .map(|binders| binders.as_ref().as_slice()),
            &self.ctx.lib_contexts,
        )
    }
}
