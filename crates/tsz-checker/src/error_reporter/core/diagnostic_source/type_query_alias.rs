use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn declared_source_annotation_names_type_query_alias(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        self.declared_source_type_query_alias_def_id(expr_idx)
            .is_some()
    }

    pub(in crate::error_reporter) fn declared_source_type_query_alias_def_id(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<tsz_solver::def::DefId> {
        self.declared_source_type_annotation_node(expr_idx)
            .and_then(|annotation_idx| {
                self.annotation_type_query_alias_def_id(self.ctx.arena, annotation_idx)
            })
    }

    pub(in crate::error_reporter) fn annotation_names_type_query_alias(
        &self,
        arena: &tsz_parser::NodeArena,
        annotation_idx: NodeIndex,
    ) -> bool {
        self.annotation_type_query_alias_def_id(arena, annotation_idx)
            .is_some()
    }

    fn annotation_type_query_alias_def_id(
        &self,
        arena: &tsz_parser::NodeArena,
        annotation_idx: NodeIndex,
    ) -> Option<tsz_solver::def::DefId> {
        // The delegate validates the reference resolves to a type alias and
        // finds its definition; this caller only narrows to aliases whose
        // declared body is a `typeof` query.
        let def_id = self.annotation_type_reference_alias_def_id(arena, annotation_idx)?;
        let type_ref = arena.get_type_ref(arena.get(annotation_idx)?)?;
        let sym_id = self
            .ctx
            .binder
            .resolve_identifier(arena, type_ref.type_name)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let has_type_query_body = symbol.declarations.iter().any(|&decl_idx| {
            arena
                .get(decl_idx)
                .and_then(|decl_node| arena.get_type_alias(decl_node))
                .and_then(|alias| arena.get(alias.type_node))
                .is_some_and(|body| body.kind == syntax_kind_ext::TYPE_QUERY)
        });
        has_type_query_body.then_some(def_id)
    }

    /// Resolve a `TYPE_REFERENCE` annotation node that names a type alias to its
    /// solver `DefId`, regardless of the alias body shape. Returns `None` for
    /// non-`TYPE_REFERENCE` annotations, references that do not resolve to a type
    /// alias, or aliases with no registered definition.
    pub(in crate::error_reporter) fn annotation_type_reference_alias_def_id(
        &self,
        arena: &tsz_parser::NodeArena,
        annotation_idx: NodeIndex,
    ) -> Option<tsz_solver::def::DefId> {
        let annotation_node = arena.get(annotation_idx)?;
        if annotation_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let type_ref = arena.get_type_ref(annotation_node)?;
        let sym_id = self
            .ctx
            .binder
            .resolve_identifier(arena, type_ref.type_name)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS) {
            return None;
        }
        let name_atom = self.ctx.types.intern_string(&symbol.escaped_name);
        self.ctx
            .definition_store
            .find_defs_by_name(name_atom)?
            .into_iter()
            .find(|def_id| {
                self.ctx.definition_store.get(*def_id).is_some_and(|def| {
                    def.kind == tsz_solver::def::DefKind::TypeAlias
                        && (def.symbol_id == Some(sym_id.0) || def.name == name_atom)
                })
            })
    }

    /// True when the source expression's declared annotation names a non-generic
    /// type alias that tsc renders by its underlying type rather than its alias
    /// name (a computed conditional / indexed-access / `keyof` / application /
    /// template / string-intrinsic body that collapses to a shared singleton, or
    /// a direct intrinsic/literal body). In that case the declared-alias source
    /// rewrite must not repaint the resolved scalar display with the alias name —
    /// tsc shows `string`, not `X1`, for `type X1 = true extends true ? string :
    /// number`.
    pub(in crate::error_reporter) fn declared_source_annotation_alias_displayed_as_underlying(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        self.declared_source_type_annotation_node(expr_idx)
            .and_then(|annotation_idx| {
                self.annotation_type_reference_alias_def_id(self.ctx.arena, annotation_idx)
            })
            .and_then(|def_id| {
                crate::query_boundaries::assignability_alias_display::type_alias_displayed_as_underlying(
                    self.ctx.types.as_type_database(),
                    &self.ctx.definition_store,
                    def_id,
                )
            })
            .is_some()
    }

    fn declared_source_type_annotation_node(&self, expr_idx: NodeIndex) -> Option<NodeIndex> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self
            .resolve_identifier_symbol(expr_idx)
            .or_else(|| self.ctx.binder.node_symbols.get(&expr_idx.0).copied())?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let mut declarations = Vec::new();
        if symbol.value_declaration.is_some() {
            declarations.push(symbol.value_declaration);
        }
        declarations.extend(symbol.declarations.iter().copied());

        declarations.into_iter().find_map(|decl_idx| {
            let decl_idx = if self
                .ctx
                .arena
                .get(decl_idx)
                .is_some_and(|node| node.kind == tsz_scanner::SyntaxKind::Identifier as u16)
            {
                self.ctx
                    .arena
                    .get_extended(decl_idx)
                    .map(|ext| ext.parent)
                    .filter(|parent| parent.is_some())
                    .unwrap_or(decl_idx)
            } else {
                decl_idx
            };
            let decl = self.ctx.arena.get(decl_idx)?;
            if let Some(param) = self.ctx.arena.get_parameter(decl)
                && param.type_annotation.is_some()
            {
                return Some(param.type_annotation);
            }
            if let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl)
                && var_decl.type_annotation.is_some()
            {
                return Some(var_decl.type_annotation);
            }
            if let Some(prop_decl) = self.ctx.arena.get_property_decl(decl)
                && prop_decl.type_annotation.is_some()
            {
                return Some(prop_decl.type_annotation);
            }
            None
        })
    }
}
