use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Recover a non-generic `readonly` array / `readonly` tuple type-alias name
    /// from an argument expression's declared annotation, for the `TS2345`
    /// argument-mismatch diagnostic.
    ///
    /// tsz interns array and `readonly` array/tuple types purely structurally, so
    /// a shared `readonly number[]` `TypeId` carries no per-reference
    /// `aliasSymbol`; the diagnostic formatter's reverse `find_def_for_type`
    /// lookup deliberately excludes `Array`/`ReadonlyType` because that lookup is
    /// unsound for structurally-interned ids (many aliases share one id). `tsc`
    /// renders such a source by the alias name it was *referenced through*, which
    /// is recoverable only from the source expression's declared annotation —
    /// exactly as the `TS2322` `AssignmentSource` role already does. A *generic*
    /// alias (`Immutable<number>`) survives as an `Application` and keeps its name
    /// already, so only the non-generic collapse is repaired here.
    ///
    /// `source_expr_idx` is the source/argument expression node; the annotation is
    /// resolved through its declaring identifier, so a non-identifier source (an
    /// array literal, a call, an assertion) yields `None` and keeps the existing
    /// structural display.
    pub(in crate::error_reporter) fn readonly_array_alias_source_display(
        &mut self,
        source_expr_idx: NodeIndex,
        source_type: TypeId,
    ) -> Option<String> {
        // Scope strictly to the `readonly` array / `readonly` tuple forms that
        // lose their alias on interning; every other source keeps its display.
        crate::query_boundaries::common::readonly_inner_type(self.ctx.types, source_type)?;

        // Resolve the annotation as a reference to a registered type alias; this
        // validates the `TYPE_REFERENCE` kind and the alias binding in one step.
        let annotation_idx = self.declared_source_type_annotation_node(source_expr_idx)?;
        let def_id = self.annotation_type_reference_alias_def_id(self.ctx.arena, annotation_idx)?;

        // Only a bare, non-generic alias collapses to the shared structural id and
        // loses its name; a generic alias (`Immutable<number>`) keeps its name via
        // the `Application` path, so reject a reference with type arguments or an
        // alias with type parameters.
        let reference_has_type_arguments = self
            .ctx
            .arena
            .get(annotation_idx)
            .and_then(|node| self.ctx.arena.get_type_ref(node))
            .is_some_and(|type_ref| type_ref.type_arguments.is_some());
        if reference_has_type_arguments
            || self
                .ctx
                .definition_store
                .get(def_id)
                .is_none_or(|def| !def.type_params.is_empty())
        {
            return None;
        }

        // Defer to the structural fallback in the same cases the `TS2322` source
        // path does, reusing the already-resolved `annotation_idx`/`def_id`:
        //  - a source identifier declared `unknown`/`any` but flow-narrowed to a
        //    concrete type renders its narrowed type, not its declared annotation;
        //  - a `typeof`-bodied alias keeps its own display policy;
        //  - a computed-body alias (a conditional / indexed-access / `keyof` /
        //    intrinsic body that tsc renders by its underlying type) drops its
        //    `aliasSymbol` and must not be repainted with the alias name.
        let ident_idx = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(source_expr_idx);
        if self.source_identifier_narrowed_from_unknown_or_any(ident_idx, source_type) {
            return None;
        }
        if self.annotation_names_type_query_alias(self.ctx.arena, annotation_idx) {
            return None;
        }
        if crate::query_boundaries::assignability_alias_display::type_alias_displayed_as_underlying(
            self.ctx.types.as_type_database(),
            &self.ctx.definition_store,
            def_id,
        )
        .is_some()
        {
            return None;
        }

        let annotation_text = self.declared_type_annotation_text_for_expression(source_expr_idx)?;
        Some(self.format_declared_annotation_for_diagnostic(&annotation_text))
    }

    /// Recover a non-generic `readonly` array / tuple type-alias name from a
    /// TS2322 assignment target's declared annotation.
    ///
    /// `ReadonlyType` ids are structurally interned, so the solver formatter's
    /// reverse type-to-def lookup cannot distinguish `const x: R` from an inline
    /// `const x: readonly T[]`. The target annotation is the provenance that
    /// proves `tsc` would have an `aliasSymbol`; without it, callers must keep
    /// the structural `readonly ...` display.
    pub(in crate::error_reporter) fn readonly_array_alias_target_display(
        &mut self,
        target_expr_idx: NodeIndex,
        target_type: TypeId,
    ) -> Option<String> {
        crate::query_boundaries::common::readonly_inner_type(self.ctx.types, target_type)?;

        let (arena, annotation_idx) =
            self.declared_type_annotation_node_for_expression(target_expr_idx)?;
        let type_ref = arena.get_type_ref(arena.get(annotation_idx)?)?;
        if type_ref.type_arguments.is_some() {
            return None;
        }
        let def_id = self.annotation_type_reference_alias_def_id(arena, annotation_idx)?;
        if self
            .ctx
            .definition_store
            .get(def_id)
            .is_none_or(|def| !def.type_params.is_empty())
        {
            return None;
        }
        if crate::query_boundaries::assignability_alias_display::type_alias_displayed_as_underlying(
            self.ctx.types.as_type_database(),
            &self.ctx.definition_store,
            def_id,
        )
        .is_some()
        {
            return None;
        }

        let annotation_text = self.declared_type_annotation_text_for_expression(target_expr_idx)?;
        Some(self.format_declared_annotation_for_diagnostic(&annotation_text))
    }

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

    /// When the source expression is an identifier whose declared annotation is
    /// a **non-generic** `TYPE_REFERENCE` to a type alias whose name `tsc`
    /// preserves in diagnostics (its `aliasSymbol` survives — the body is not a
    /// computed type rendered by its underlying form), return that alias name.
    ///
    /// `tsc` stamps an `aliasSymbol` onto a referenced structural type so the
    /// alias spelling survives into diagnostics. `tsz` interns array,
    /// readonly-array, and readonly-tuple types purely structurally, so a shared
    /// `readonly number[]` `TypeId` carries no per-reference alias and the name
    /// is recoverable only from the source expression's annotation. A diagnostic
    /// whose source is such a structurally-interned type (notably `TS4104`
    /// readonly-to-mutable) consults this to render the alias `tsc` shows (`RA`
    /// rather than `readonly number[]`). A generic alias application
    /// (`Immutable<string>`) keeps its `Name<Args>` surface through the
    /// structural formatter and is intentionally excluded.
    pub(in crate::error_reporter) fn declared_source_type_reference_alias_name(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let annotation_idx = self.declared_source_type_annotation_node(expr_idx)?;
        let annotation_node = self.ctx.arena.get(annotation_idx)?;
        // `get_type_ref` yields `Some` only for a `TYPE_REFERENCE` node, so it
        // also serves as the annotation-kind gate.
        let type_ref = self.ctx.arena.get_type_ref(annotation_node)?;
        // Only a bare (no-type-argument) reference loses its name; a generic
        // application keeps its `Name<Args>` surface through the formatter.
        if type_ref.type_arguments.is_some() {
            return None;
        }
        let def_id = self.annotation_type_reference_alias_def_id(self.ctx.arena, annotation_idx)?;
        let alias_name = {
            let def = self.ctx.definition_store.get(def_id)?;
            if !def.type_params.is_empty() {
                return None;
            }
            def.name
        };
        // A non-generic alias whose body `tsc` renders by its underlying type
        // (computed conditional / indexed-access / `keyof` / reducing
        // application / intrinsic-or-literal singleton) carries no
        // `aliasSymbol`; keep that underlying display rather than repainting it
        // with the alias name.
        if crate::query_boundaries::assignability_alias_display::type_alias_displayed_as_underlying(
            self.ctx.types.as_type_database(),
            &self.ctx.definition_store,
            def_id,
        )
        .is_some()
        {
            return None;
        }
        Some(self.ctx.types.resolve_atom(alias_name))
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
