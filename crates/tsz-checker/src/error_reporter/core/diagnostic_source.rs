//! Diagnostic source/target expression analysis and formatting.

mod assignment_annotation_text;
mod assignment_formatting;
mod assignment_source_preservation;
mod assignment_widening;
mod collection_source_display;
mod compound_assignment_context;
mod computed_index_source_display;
mod contextual_index_display;
mod direct_source_expression;
mod generic_source_display;
mod keyof_source_display;
mod literal_surface;
mod literal_widening_helpers;
mod literal_widening_policy;
mod numeric_literal_union_source;
mod object_literal_anchors;
mod object_literal_targets;
mod recursive_alias_display;
mod span_diagnostic_queries;
mod static_schema;
mod tuple_source_display;
mod type_query_alias;
mod wrapper_provenance;

use crate::query_boundaries::diagnostics as diagnostic_query;
use crate::state::CheckerState;
use crate::types_domain::type_node_helpers::type_node_includes_explicit_undefined;
use span_diagnostic_queries::strip_module_specifier_extension;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn declared_type_annotation_text_for_expression_with_options(
        &self,
        expr_idx: NodeIndex,
        allow_object_shapes: bool,
    ) -> Option<String> {
        let node_text_in_arena = |arena: &tsz_parser::NodeArena, node_idx: NodeIndex| {
            let node = arena.get(node_idx)?;
            let source = arena.source_files.first()?.text.as_ref();
            let start = node.pos as usize;
            let end = node.end as usize;
            if start >= end || end > source.len() {
                return None;
            }
            Some(source[start..end].to_string())
        };
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }

        // Scope-chain resolution covers values; `node_symbols` recovers
        // declaration-site identifiers such as class property names.
        let sym_id = self
            .resolve_identifier_symbol(expr_idx)
            .or_else(|| self.ctx.binder.node_symbols.get(&expr_idx.0).copied())?;
        let symbol = self.get_cross_file_symbol(sym_id)?;
        let owner_binder = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
            .or_else(|| {
                self.ctx
                    .binder
                    .symbol_arenas
                    .get(&sym_id)
                    .and_then(|arena| self.ctx.get_binder_for_arena(arena))
            })
            .unwrap_or(self.ctx.binder);
        let fallback_arena = if symbol.decl_file_idx != u32::MAX {
            self.ctx.get_arena_for_file(symbol.decl_file_idx)
        } else {
            owner_binder
                .symbol_arenas
                .get(&sym_id)
                .map(std::convert::AsRef::as_ref)
                .unwrap_or(self.ctx.arena)
        };

        let mut declarations: Vec<(NodeIndex, &tsz_parser::NodeArena)> = Vec::new();
        let mut push_declaration = |decl_idx: NodeIndex| {
            if decl_idx.is_none() {
                return;
            }

            let mut pushed = false;
            if let Some(arenas) = owner_binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                for arena in arenas {
                    let arena = arena.as_ref();
                    if arena.get(decl_idx).is_none() {
                        continue;
                    }
                    let key = (decl_idx, arena as *const tsz_parser::NodeArena);
                    if declarations.iter().all(|(existing_idx, existing_arena)| {
                        (
                            *existing_idx,
                            *existing_arena as *const tsz_parser::NodeArena,
                        ) != key
                    }) {
                        declarations.push((decl_idx, arena));
                    }
                    pushed = true;
                }
            }

            if !pushed && fallback_arena.get(decl_idx).is_some() {
                let key = (decl_idx, fallback_arena as *const tsz_parser::NodeArena);
                if declarations.iter().all(|(existing_idx, existing_arena)| {
                    (
                        *existing_idx,
                        *existing_arena as *const tsz_parser::NodeArena,
                    ) != key
                }) {
                    declarations.push((decl_idx, fallback_arena));
                }
            }
        };

        push_declaration(symbol.value_declaration);
        for &decl_idx in &symbol.declarations {
            push_declaration(decl_idx);
        }

        for (decl_idx, decl_arena) in declarations {
            let decl_idx = if decl_arena
                .get(decl_idx)
                .is_some_and(|node| node.kind == tsz_scanner::SyntaxKind::Identifier as u16)
            {
                let parent = decl_arena
                    .get_extended(decl_idx)
                    .map(|ext| ext.parent)
                    .unwrap_or(NodeIndex::NONE);
                let parent_node = decl_arena.get(parent);
                if parent.is_some()
                    && parent_node.is_some_and(|node| {
                        decl_arena.get_variable_declaration(node).is_some()
                            || decl_arena.get_parameter(node).is_some()
                    })
                {
                    parent
                } else {
                    decl_idx
                }
            } else {
                decl_idx
            };
            let decl = decl_arena.get(decl_idx)?;
            if let Some(param) = decl_arena.get_parameter(decl)
                && param.type_annotation.is_some()
            {
                if self.annotation_names_type_query_alias(decl_arena, param.type_annotation) {
                    return None;
                }
                let mut text =
                    node_text_in_arena(decl_arena, param.type_annotation).and_then(|text| {
                        self.sanitize_type_annotation_text_for_diagnostic(text, allow_object_shapes)
                    })?;
                let annotation_contains_undefined =
                    type_node_includes_explicit_undefined(decl_arena, param.type_annotation);
                if param.question_token
                    && self.ctx.strict_null_checks()
                    && !annotation_contains_undefined
                {
                    if text.contains("=>") {
                        text = format!("({text}) | undefined");
                    } else {
                        text.push_str(" | undefined");
                    }
                }
                return Some(text);
            }

            if let Some(var_decl) = decl_arena.get_variable_declaration(decl)
                && var_decl.type_annotation.is_some()
            {
                if self.annotation_names_type_query_alias(decl_arena, var_decl.type_annotation) {
                    return None;
                }
                return node_text_in_arena(decl_arena, var_decl.type_annotation).and_then(|text| {
                    self.sanitize_type_annotation_text_for_diagnostic(text, allow_object_shapes)
                });
            }

            // tsc shows class-property annotation text in TS2322, not the
            // evaluated type, which may be `() => error` for unresolved names.
            if let Some(prop_decl) = decl_arena.get_property_decl(decl)
                && prop_decl.type_annotation.is_some()
            {
                if self.annotation_names_type_query_alias(decl_arena, prop_decl.type_annotation) {
                    return None;
                }
                return node_text_in_arena(decl_arena, prop_decl.type_annotation).and_then(
                    |text| {
                        self.sanitize_type_annotation_text_for_diagnostic(text, allow_object_shapes)
                    },
                );
            }
        }

        None
    }

    pub(crate) fn declared_type_annotation_text_for_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        self.declared_type_annotation_text_for_expression_with_options(expr_idx, false)
    }

    fn declared_diagnostic_source_annotation_text(&self, expr_idx: NodeIndex) -> Option<String> {
        self.declared_type_annotation_text_for_expression_with_options(expr_idx, true)
    }

    fn declared_type_annotation_text_for_symbol_type(
        &self,
        ty: TypeId,
        allow_object_shapes: bool,
    ) -> Option<String> {
        let sym_id = self.ctx.resolve_type_to_symbol_id(ty)?;
        let symbol = self.get_cross_file_symbol(sym_id)?;
        let decl_idx = symbol.value_declaration;
        if decl_idx.is_none() {
            return None;
        }

        let owner_binder = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
            .or_else(|| {
                self.ctx
                    .binder
                    .symbol_arenas
                    .get(&sym_id)
                    .and_then(|arena| self.ctx.get_binder_for_arena(arena))
            })
            .unwrap_or(self.ctx.binder);
        let fallback_arena = if symbol.decl_file_idx != u32::MAX {
            self.ctx.get_arena_for_file(symbol.decl_file_idx)
        } else {
            owner_binder
                .symbol_arenas
                .get(&sym_id)
                .map(std::convert::AsRef::as_ref)
                .unwrap_or(self.ctx.arena)
        };

        let decl_arena = owner_binder
            .declaration_arenas
            .get(&(sym_id, decl_idx))
            .and_then(|arenas| arenas.first().map(|arena| arena.as_ref()))
            .filter(|arena| arena.get(decl_idx).is_some())
            .unwrap_or(fallback_arena);
        let decl = decl_arena.get(decl_idx)?;

        let node_text_in_arena = |arena: &tsz_parser::NodeArena, node_idx: NodeIndex| {
            let node = arena.get(node_idx)?;
            let source = arena.source_files.first()?.text.as_ref();
            let start = node.pos as usize;
            let end = node.end as usize;
            if start >= end || end > source.len() {
                return None;
            }
            Some(source[start..end].to_string())
        };

        if let Some(param) = decl_arena.get_parameter(decl)
            && param.type_annotation.is_some()
        {
            if self.annotation_names_type_query_alias(decl_arena, param.type_annotation) {
                return None;
            }
            return node_text_in_arena(decl_arena, param.type_annotation).and_then(|text| {
                self.sanitize_type_annotation_text_for_diagnostic(text, allow_object_shapes)
            });
        }

        if let Some(var_decl) = decl_arena.get_variable_declaration(decl)
            && var_decl.type_annotation.is_some()
        {
            if self.annotation_names_type_query_alias(decl_arena, var_decl.type_annotation) {
                return None;
            }
            return node_text_in_arena(decl_arena, var_decl.type_annotation).and_then(|text| {
                self.sanitize_type_annotation_text_for_diagnostic(text, allow_object_shapes)
            });
        }

        None
    }

    fn declared_type_annotation_node_for_symbol(
        &self,
        sym_id: SymbolId,
    ) -> Option<(&tsz_parser::NodeArena, NodeIndex)> {
        let symbol_record = self.get_cross_file_symbol(sym_id)?;
        let declaration_idx = symbol_record.value_declaration;
        if declaration_idx.is_none() {
            return None;
        }

        let declaration_binder = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
            .or_else(|| {
                self.ctx
                    .binder
                    .symbol_arenas
                    .get(&sym_id)
                    .and_then(|arena| self.ctx.get_binder_for_arena(arena))
            })
            .unwrap_or(self.ctx.binder);
        let symbol_arena = if symbol_record.decl_file_idx != u32::MAX {
            self.ctx.get_arena_for_file(symbol_record.decl_file_idx)
        } else {
            declaration_binder
                .symbol_arenas
                .get(&sym_id)
                .map_or(self.ctx.arena, std::convert::AsRef::as_ref)
        };

        let annotation_arena = declaration_binder
            .declaration_arenas
            .get(&(sym_id, declaration_idx))
            .and_then(|arenas| arenas.first().map(|arena| arena.as_ref()))
            .filter(|arena| arena.get(declaration_idx).is_some())
            .unwrap_or(symbol_arena);
        let declaration = annotation_arena.get(declaration_idx)?;

        if let Some(param) = annotation_arena.get_parameter(declaration)
            && param.type_annotation.is_some()
        {
            return Some((annotation_arena, param.type_annotation));
        }

        if let Some(var_decl) = annotation_arena.get_variable_declaration(declaration)
            && var_decl.type_annotation.is_some()
        {
            return Some((annotation_arena, var_decl.type_annotation));
        }

        None
    }

    /// Resolve a source/target identifier expression to its declared type
    /// annotation node (`(arena, annotation_idx)`), mirroring the symbol
    /// resolution used by `declared_type_annotation_text_for_expression` but
    /// returning the AST node rather than its source text. Used to classify
    /// whether the annotation was written inline (`{ a: number }`) or as a named
    /// reference, which decides alias-name display per tsc's `aliasSymbol` policy.
    pub(in crate::error_reporter) fn declared_type_annotation_node_for_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<(&tsz_parser::NodeArena, NodeIndex)> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self
            .resolve_identifier_symbol(expr_idx)
            .or_else(|| self.ctx.binder.node_symbols.get(&expr_idx.0).copied())?;
        self.declared_type_annotation_node_for_symbol(sym_id)
    }

    /// True when `annotation_idx` is an inline / anonymous *composite* type
    /// annotation — a type literal (`{ … }`), or a union/intersection whose
    /// constituents are all themselves anonymous composites — with no named
    /// type-reference constituent.
    ///
    /// tsc attaches an `aliasSymbol` (and spells the alias name) only when the
    /// annotation referenced an alias; an annotation written inline carries none
    /// and is rendered structurally. A *mixed* union/intersection (some member is
    /// a named reference) returns `false` so the established per-name display path
    /// keeps the reference members' names rather than over-suppressing them.
    pub(in crate::error_reporter) fn annotation_is_anonymous_structural_composite(
        arena: &tsz_parser::NodeArena,
        annotation_idx: NodeIndex,
    ) -> bool {
        Self::annotation_is_anonymous_structural_composite_at(arena, annotation_idx, 0)
    }

    fn annotation_is_anonymous_structural_composite_at(
        arena: &tsz_parser::NodeArena,
        annotation_idx: NodeIndex,
        depth: u32,
    ) -> bool {
        if depth > 32 {
            return false;
        }
        let Some(node) = arena.get(annotation_idx) else {
            return false;
        };
        match node.kind {
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                arena.get_wrapped_type(node).is_some_and(|wrapped| {
                    Self::annotation_is_anonymous_structural_composite_at(
                        arena,
                        wrapped.type_node,
                        depth + 1,
                    )
                })
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => true,
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                arena.get_composite_type(node).is_some_and(|composite| {
                    !composite.types.nodes.is_empty()
                        && composite.types.nodes.iter().all(|&member_idx| {
                            Self::annotation_is_anonymous_structural_composite_at(
                                arena,
                                member_idx,
                                depth + 1,
                            )
                        })
                })
            }
            _ => false,
        }
    }

    fn declared_annotation_can_name_union_source(&self, sym_id: SymbolId) -> bool {
        let Some((annotation_arena, annotation_idx)) =
            self.declared_type_annotation_node_for_symbol(sym_id)
        else {
            return false;
        };
        annotation_arena.get(annotation_idx).is_some_and(|node| {
            matches!(
                node.kind,
                syntax_kind_ext::TYPE_REFERENCE
                    | syntax_kind_ext::UNION_TYPE
                    | syntax_kind_ext::PARENTHESIZED_TYPE
            )
        })
    }

    /// The declared (pre-narrowing) type of a source *variable* identifier
    /// expression, or `None` when `expr_idx` is not a usable variable
    /// identifier. Merged `INTERFACE`+`VALUE` symbols resolve to the interface
    /// side via `get_type_of_symbol`, so they are excluded; `Error`/`unknown`
    /// declared types carry no useful alias to compare against. Shared by the
    /// source-display paths that compare the flow-narrowed checked type against
    /// the declared type.
    pub(in crate::error_reporter) fn declared_type_of_variable_identifier_source(
        &mut self,
        expr_idx: NodeIndex,
    ) -> Option<TypeId> {
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::VARIABLE) {
            return None;
        }
        if symbol.has_any_flags(tsz_binder::symbol_flags::INTERFACE)
            && !symbol.has_any_flags(tsz_binder::symbol_flags::CLASS)
        {
            return None;
        }
        if !self.declared_annotation_can_name_union_source(sym_id) {
            return None;
        }
        let declared_type = self.get_type_of_symbol(sym_id);
        if matches!(declared_type, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }
        Some(declared_type)
    }

    /// True when a source identifier's flow-narrowed checked type
    /// `expr_display_type` is a strict narrowing of its `declared_type` —
    /// either a strict subtype (assignable one way only) or a strict union
    /// member subset (flow eliminated some declared union members). `tsc` drops
    /// the declared `aliasSymbol` on `filterType`/`getNarrowedType` whenever the
    /// result is a proper subset, so the narrowed structural type is displayed
    /// rather than the declared alias/annotation name. Shared by the source
    /// display path (`format_assignment_source_type_for_diagnostic`) and the
    /// TS2322/TS2741 alias repaint (`declared_generic_alias_assignment_pair_display`)
    /// so both agree on the narrowing-drops-the-alias rule. It mirrors the
    /// narrowing detection inlined in `declared_identifier_source_display`.
    ///
    /// `declared_type == TypeId::ANY` is intentionally not detected here: `any`
    /// is bidirectionally related to every type, so the subtype check never
    /// fires; the `any`/`unknown` top-type narrowing is handled separately by
    /// the `source_identifier_narrowed_from_unknown_or_any` and
    /// `assignment_source_narrowed_from_declared_top_type` guards.
    pub(in crate::error_reporter) fn source_flow_type_strictly_narrows_declared(
        &mut self,
        expr_display_type: TypeId,
        declared_type: TypeId,
    ) -> bool {
        if expr_display_type == declared_type {
            return false;
        }
        // Restricted to a *declared union*: `tsc` drops the `aliasSymbol` on
        // `filterType`, which narrows unions by eliminating members. A non-union
        // declared alias (e.g. a static-schema array alias `Input[]` whose
        // structural expansion is asymmetrically related under the decision-only
        // relation) must keep its established display path and is not a member
        // of this flow-narrowing-drops-the-alias family.
        if diagnostic_query::union_members(self.ctx.types, declared_type).is_none() {
            return false;
        }
        let is_assignability_narrower = self
            .diagnostic_source_narrowing_relation_outcome(expr_display_type, declared_type)
            .related
            && !self
                .diagnostic_source_narrowing_relation_outcome(declared_type, expr_display_type)
                .related;
        is_assignability_narrower
            || self.is_strict_union_member_subset(expr_display_type, declared_type)
    }

    pub(in crate::error_reporter) fn should_prefer_declared_source_annotation_display(
        &mut self,
        expr_idx: NodeIndex,
        expr_type: TypeId,
        annotation_text: &str,
    ) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return false;
        }

        // A source identifier declared `unknown`/`any` but flow-narrowed to a
        // concrete type must render its narrowed type, not its stale declared
        // annotation; `tsc` prints the narrowed type here.
        if self.source_identifier_narrowed_from_unknown_or_any(expr_idx, expr_type) {
            return false;
        }
        let annotation = annotation_text.trim();
        if self.declared_source_annotation_names_type_query_alias(expr_idx) {
            return false;
        }
        // A computed-body alias carries no `aliasSymbol` in tsc, so its source is
        // rendered structurally, never by name. Scalar bodies already expand (the
        // index-signature gate below returns `false`), but tuple/array bodies slip
        // past that gate via their numeric index signature; route them through the
        // shared display policy so every reducible body drops the alias annotation.
        if self.source_declared_type_is_displayed_as_underlying(expr_type) {
            return false;
        }
        if annotation.contains("`${") {
            return true;
        }
        if annotation.contains('&') && !annotation.starts_with("keyof ") {
            if self.source_type_contains_number_literal_only_union(expr_type) {
                return false;
            }
            return !annotation.starts_with("null |") && !annotation.starts_with("undefined |");
        }

        let display_type =
            self.widen_function_like_display_type(self.widen_type_for_display(expr_type));
        let formatted = self.format_type_for_assignability_message(display_type);
        if formatted == "unknown"
            && annotation.contains('<')
            && crate::query_boundaries::common::contains_type_parameters(self.ctx.types, expr_type)
        {
            return true;
        }
        // Keep declaration-site function signatures whenever the fallback display
        // has diverged from the annotation. tsc prefers the declared callable
        // surface for source identifiers, especially when the computed display has
        // widened return literals or otherwise normalized the signature.
        if annotation.contains("=>") {
            if annotation.contains("?:") && formatted.contains("| undefined") {
                return false;
            }
            return formatted != annotation;
        }
        let resolved = self.resolve_type_for_property_access(display_type);
        let evaluated = self.judge_evaluate(resolved);
        let has_index_signature =
            crate::query_boundaries::index_signature::has_string_or_number_index_signature(
                self.ctx.types,
                evaluated,
            );
        if !formatted.starts_with('{') && !has_index_signature {
            return false;
        }

        // Don't use annotation text when it starts with `null` or `undefined` in
        // a union — the computed type formatter correctly reorders null/undefined
        // to the end (matching tsc's display), but annotation text preserves
        // source order which would put them first.
        if (annotation.starts_with("null |") || annotation.starts_with("undefined |"))
            && !annotation.contains('&')
        {
            return false;
        }
        if annotation.contains('&') || !annotation.starts_with('{') {
            return true;
        }

        if annotation.contains('[') && annotation.contains(']') && formatted.contains("__unique_") {
            return true;
        }

        false
    }

    /// True when `ty` resolves to a non-generic type alias that tsc renders by
    /// its underlying (computed) type rather than its declared name — see
    /// [`crate::query_boundaries::assignability_alias_display::type_displayed_as_underlying`],
    /// which owns the `Lazy(DefId)` / resolved-shape resolution behind the query
    /// boundary.
    fn source_declared_type_is_displayed_as_underlying(&self, ty: TypeId) -> bool {
        crate::query_boundaries::assignability_alias_display::type_displayed_as_underlying(
            self.ctx.types.as_type_database(),
            &self.ctx.definition_store,
            ty,
        )
        .is_some()
    }

    pub(crate) fn format_type_diagnostic_structural(&self, ty: TypeId) -> String {
        let mut formatter =
            tsz_solver::TypeFormatter::with_symbols(self.ctx.types, &self.ctx.binder.symbols)
                .with_def_store(&self.ctx.definition_store)
                .with_diagnostic_mode()
                .with_strict_null_checks(self.ctx.compiler_options.strict_null_checks)
                .with_display_properties();
        formatter.format(ty).into_owned()
    }

    fn synthesized_object_parent_display_name(&self, ty: TypeId) -> Option<String> {
        use crate::query_boundaries::common::object_shape_id;
        use tsz_binder::symbol_flags;

        let shape_id = object_shape_id(self.ctx.types, ty)?;
        let shape = self.ctx.types.object_shape(shape_id);
        let has_js_ctor_brand = shape.properties.iter().any(|prop| {
            self.ctx
                .types
                .resolve_atom_ref(prop.name)
                .starts_with("__js_ctor_brand_")
        });
        let mut parent_ids = shape.properties.iter().filter_map(|prop| prop.parent_id);
        let parent_sym = parent_ids.next()?;
        if parent_ids.any(|other| other != parent_sym) {
            return None;
        }

        let symbol = self.get_cross_file_symbol(parent_sym)?;
        if !has_js_ctor_brand && !symbol.has_any_flags(symbol_flags::FUNCTION | symbol_flags::CLASS)
        {
            return None;
        }

        Some(symbol.escaped_name.clone())
    }

    pub(crate) fn property_receiver_application_base_name(
        &self,
        type_id: TypeId,
    ) -> Option<String> {
        let app = crate::query_boundaries::common::type_application(self.ctx.types, type_id)?;
        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)
            .or_else(|| self.ctx.definition_store.find_def_for_type(app.base))?;
        let def = self.ctx.definition_store.get(def_id)?;
        Some(self.ctx.types.resolve_atom(def.name))
    }

    pub(crate) fn format_property_receiver_type_for_diagnostic(&mut self, ty: TypeId) -> String {
        if let Some(module_name) = self.ctx.namespace_module_names.get(&ty) {
            return format!(
                "typeof import(\"{}\")",
                strip_module_specifier_extension(module_name)
            );
        }
        let evaluated = self.evaluate_type_for_assignability(ty);
        if evaluated != ty
            && self.named_type_display_name(evaluated).is_some()
            && crate::query_boundaries::common::type_application(self.ctx.types, ty).is_some()
        {
            return self.format_type_for_assignability_message(evaluated);
        }
        let application_display =
            crate::query_boundaries::common::type_application(self.ctx.types, ty)
                .map(|_| ty)
                .or_else(|| {
                    self.ctx.types.get_display_alias(ty).filter(|&alias| {
                        crate::query_boundaries::common::type_application(self.ctx.types, alias)
                            .is_some()
                            && !crate::query_boundaries::diagnostics::empty_object_display_alias_is_marker_render(
                                self.ctx.types,
                                &self.ctx.definition_store,
                                ty,
                                alias,
                            )
                    })
                });
        if let Some(application_display) = application_display
            && !diagnostic_query::application_base_has_conditional_alias_body(
                self.ctx.types,
                &self.ctx.definition_store,
                application_display,
            )
        {
            let display_ty =
                self.normalize_property_receiver_application_display_type(application_display);
            let preserve_object_args = self
                .property_receiver_application_base_name(display_ty)
                .is_some_and(|name| name == "merge");
            let mut formatter = self
                .ctx
                .create_diagnostic_type_formatter()
                .with_long_property_receiver_display()
                .with_display_properties()
                .with_skip_application_alias_names();
            if !preserve_object_args {
                formatter = formatter.with_long_property_receiver_object_elision_end_depth(192);
            } else {
                formatter = formatter.with_long_property_receiver_object_elision_end_depth(0);
            }
            return Self::truncate_property_receiver_display(
                formatter.format(display_ty).into_owned(),
            );
        }
        let has_object_shape =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty).is_some();
        let has_def = self.ctx.definition_store.find_def_for_type(ty).is_some();
        let has_alias = self
            .ctx
            .definition_store
            .find_type_alias_by_body(ty)
            .is_some();
        let has_namespace_name = self.ctx.namespace_module_names.contains_key(&ty);
        // If this type was produced by evaluating a generic application
        // (e.g., `Omit<this, K>` → `{}`), fall through to
        // `format_type_for_assignability_message` which respects the display_alias
        // mechanism and renders `Omit<this, K>` instead of the structural form.
        let has_display_alias = self.ctx.types.get_display_alias(ty).is_some();
        // Preserve namespace identity (`typeof import("...")`) for CommonJS
        // namespace objects that are represented as anonymous object shapes.
        // Structural widening here drops the namespace tag and expands the full
        // object literal in diagnostics.
        if has_namespace_name {
            return self.format_type_diagnostic(ty);
        }
        if has_object_shape && !has_def && !has_alias && !has_display_alias {
            // Only widen literal properties of *fresh* object literal types
            // (e.g., the type of `{ x: 1 }` expression). Declared object
            // annotations like `let a: { __foo: 10 }` preserve their literal
            // property types in property-access diagnostics, matching tsc.
            let display_ty =
                if crate::query_boundaries::common::is_fresh_object_type(self.ctx.types, ty) {
                    self.widen_fresh_object_literal_properties_for_display(ty)
                } else {
                    ty
                };
            return Self::truncate_property_receiver_display(
                self.format_type_diagnostic_widened(display_ty),
            );
        }
        // Only widen object-like types (to convert literal properties to primitives).
        // For literal/primitive receiver types (e.g., `""`, `42`), tsc preserves the
        // literal in TS2339 messages (e.g., `'""'` not `'string'`).  Unions whose
        // every member is a literal are also preserved (e.g., `"foo" | "bar"`) —
        // widening them to `string` loses discriminative information tsc keeps in
        // property-existence diagnostics.
        let is_literal_or_primitive =
            crate::query_boundaries::common::literal_value(self.ctx.types, ty).is_some()
                || crate::query_boundaries::common::is_primitive_type(self.ctx.types, ty);
        let is_union_of_literals = !is_literal_or_primitive
            && crate::query_boundaries::common::union_members(self.ctx.types, ty).is_some_and(
                |members| {
                    !members.is_empty()
                        && members.iter().all(|&m| {
                            crate::query_boundaries::common::literal_value(self.ctx.types, m)
                                .is_some()
                        })
                },
            );
        let ty = if is_literal_or_primitive || is_union_of_literals {
            ty
        } else {
            self.widen_type_for_display(ty)
        };
        let mut assignability_display = self.format_type_for_property_receiver_message(ty);
        if assignability_display.len() > 320 && assignability_display.starts_with("Omit<") {
            assignability_display = self.format_long_property_receiver_type_for_diagnostic(ty);
        }
        let assignability_display = Self::truncate_property_receiver_display(assignability_display);
        if let Some(name) = self.synthesized_object_parent_display_name(ty) {
            let generic_prefix = format!("{name}<");
            if assignability_display.starts_with(&generic_prefix) {
                return assignability_display;
            }
            return name;
        }
        if self.ctx.definition_store.find_def_for_type(ty).is_none()
            && self
                .ctx
                .definition_store
                .find_type_alias_by_body(ty)
                .is_some()
            && !(assignability_display.starts_with("Omit<")
                || assignability_display.starts_with("merge<"))
        {
            return self.format_type_diagnostic_structural(ty);
        }
        assignability_display
    }

    pub(crate) fn preferred_constructor_display_name(&mut self, type_id: TypeId) -> Option<String> {
        let base_name = self.named_type_display_name(type_id)?;
        let is_callable_or_constructible =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
                .is_some()
                || crate::query_boundaries::common::function_shape_for_type(
                    self.ctx.types,
                    type_id,
                )
                .is_some();
        if !is_callable_or_constructible {
            return None;
        }

        let constructor_name = format!("{base_name}Constructor");
        let constructor_type = self.resolve_lib_type_by_name(&constructor_name)?;
        if constructor_type.is_unknown_or_error() {
            return None;
        }

        let source_type = self.widen_type_for_display(type_id);
        let constructor_type = self.widen_type_for_display(constructor_type);
        crate::query_boundaries::assignability::are_types_structurally_identical(
            self.ctx.types,
            &self.ctx,
            source_type,
            constructor_type,
        )
        .then_some(constructor_name)
    }

    /// When a source expression is a property/element access whose value type
    /// is `unique symbol` (e.g. `Symbol.toPrimitive`), tsc renders the
    /// assignability source as `typeof <expr>` rather than widening to
    /// `symbol`. Mirrors that behavior so diagnostics like
    /// "Type 'typeof Symbol.toPrimitive' is not assignable to type 'object'"
    /// match tsc.
    fn typeof_unique_symbol_source_display(&mut self, anchor_idx: NodeIndex) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;
        let expr_idx = self.direct_diagnostic_source_expression(anchor_idx)?;
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }
        let expr_type = self.get_type_of_node(expr_idx);
        if !crate::query_boundaries::common::is_unique_symbol_type(self.ctx.types, expr_type) {
            return None;
        }
        let text = self.node_text(expr_idx)?;
        // node_text spans the AST node; for trailing-semicolon expressions
        // (e.g. `"" in Symbol.toPrimitive;`) the parsed PropertyAccess can
        // include the `;` byte. tsc strips it before display.
        let text = text.trim().trim_end_matches(';').trim_end().to_string();
        Some(format!("typeof {text}"))
    }

    fn jsdoc_annotated_expression_display(
        &mut self,
        expr_idx: NodeIndex,
        target: TypeId,
    ) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = expr_idx;
        loop {
            // Skip JSDoc-derived source display when `current` is the name of a
            // class property declaration whose leading JSDoc `@type` describes
            // the declared (target) type, not an initializer/source expression.
            // Without this guard the property name picks up the property's own
            // `@type` annotation as the "source" string and produces tautological
            // diagnostics like "Type 'boolean' is not assignable to type 'boolean'."
            // for e.g. `/** @type {boolean} */ #foo = 3` where the source is `3`.
            if self
                .ctx
                .arena
                .node_info(current)
                .and_then(|info| self.ctx.arena.get(info.parent))
                .is_some_and(|parent| {
                    matches!(
                        parent.kind,
                        syntax_kind_ext::PROPERTY_ASSIGNMENT
                            | syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                            | syntax_kind_ext::METHOD_DECLARATION
                            | syntax_kind_ext::GET_ACCESSOR
                            | syntax_kind_ext::SET_ACCESSOR
                            | syntax_kind_ext::PROPERTY_DECLARATION
                    )
                })
            {
                return None;
            }
            if let Some(type_id) = self.jsdoc_type_annotation_for_node_direct(current) {
                // When `current` is a CommonJS module-exports assignment (e.g.
                // `/** @type {string} */ module.exports = 0;`), the `@type`
                // describes the declared export type, not the source RHS type.
                // Returning the annotated type as the source display yields
                // "Type 'string' is not assignable to type 'string'" where the
                // RHS is actually a `number`. Skip the rewrite in that case so
                // the real source type (e.g., `number`) is displayed.
                if self.is_jsdoc_declared_target_assignment(current) {
                    return None;
                }
                let display_type = self.widen_function_like_display_type(type_id);
                return Some(self.format_assignability_type_for_message(display_type, target));
            }

            let node = self.ctx.arena.get(current)?;
            if node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                return None;
            }

            let paren = self.ctx.arena.get_parenthesized(node)?;
            current = paren.expression;
        }
    }

    /// Determine whether `node` is the LHS (or the whole binary expression) of
    /// a CommonJS `module.exports = X` / `exports = X` assignment in a JS file.
    /// For these forms a leading JSDoc `@type` annotation declares the target
    /// type, not the source type, and must not drive source-side display.
    fn is_jsdoc_declared_target_assignment(&self, node: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        if !self.is_js_file() {
            return false;
        }
        let Some(node_data) = self.ctx.arena.get(node) else {
            return false;
        };
        // Resolve the enclosing assignment binary expression.  The JSDoc
        // annotation may have been attached to the wrapping ExpressionStatement,
        // so accept that form too (`/** @type {string} */ module.exports = 0;`).
        let binary_idx = match node_data.kind {
            k if k == syntax_kind_ext::BINARY_EXPRESSION => node,
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                let Some(stmt) = self.ctx.arena.get_expression_statement(node_data) else {
                    return false;
                };
                stmt.expression
            }
            _ => {
                // If `node` is the LHS of an assignment, walk to the parent.
                let Some(parent_idx) = self
                    .ctx
                    .arena
                    .node_info(node)
                    .map(|info| info.parent)
                    .filter(|idx| idx.is_some())
                else {
                    return false;
                };
                let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                    return false;
                };
                if parent_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                    return false;
                }
                parent_idx
            }
        };

        let Some(binary_node) = self.ctx.arena.get(binary_idx) else {
            return false;
        };
        if binary_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(binary) = self.ctx.arena.get_binary_expr(binary_node) else {
            return false;
        };
        if binary.operator_token != tsz_scanner::SyntaxKind::EqualsToken as u16 {
            return false;
        }
        if self.is_commonjs_module_exports_assignment(binary.left) {
            return true;
        }
        // Same target-annotation carve-out for `Foo.prototype = X`.
        let n = match self.ctx.arena.get(binary.left) {
            Some(n) => n,
            None => return false,
        };
        if n.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        self.ctx
            .arena
            .get_access_expr(n)
            .and_then(|a| self.ctx.arena.get(a.name_or_argument))
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .is_some_and(|i| i.escaped_text == "prototype")
    }

    fn empty_array_literal_source_type_display(&self, expr_idx: NodeIndex) -> Option<String> {
        // Only skip parentheses, not type assertions.  When the source is
        // `[] as Foo`, the diagnostic should display the asserted type `Foo`,
        // not the inner empty array's intrinsic type.  Returning `None` here
        // lets the caller fall through to `get_type_of_node` (or further display
        // policy) which yields the asserted type.  Mirrors the behavior of
        // `object_literal_source_type_display` for `({} as Foo)`.
        let expr_idx = self.ctx.arena.skip_parenthesized(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }
        let literal = self.ctx.arena.get_literal_expr(node)?;
        if !literal.elements.nodes.is_empty() {
            return None;
        }
        Some(if self.ctx.strict_null_checks() {
            "never[]".to_string()
        } else {
            "undefined[]".to_string()
        })
    }

    pub(crate) fn tuple_structural_source_display(
        &mut self,
        source_type: TypeId,
        target: TypeId,
    ) -> Option<String> {
        let target = self.evaluate_type_for_assignability(target);
        if !crate::query_boundaries::common::is_tuple_type(self.ctx.types, target) {
            return None;
        }
        // Track whether the source is a readonly-wrapped tuple. tsc renders
        // `readonly [...]` for the source side when the value type is
        // `readonly` (e.g. produced by `as const`); without this prefix the
        // assignment-failure message reads `Type '[1]'...` instead of
        // `Type 'readonly [1]'...` for sources whose readonliness is the
        // very property the assignment is failing on.
        // This can evaluate applications/lazy aliases; reuse it so recursive
        // readonly tuple sources do not take the same tuple path twice.
        let source_elements =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, source_type);
        let source_is_readonly_tuple =
            crate::query_boundaries::type_computation::complex::is_readonly_type(
                self.ctx.types,
                source_type,
            ) && source_elements.is_some();
        let elements = source_elements.or_else(|| {
            let evaluated = self.evaluate_type_for_assignability(source_type);
            crate::query_boundaries::common::tuple_elements(self.ctx.types, evaluated)
        })?;
        if elements.is_empty() {
            return None;
        }

        // Single-rest tuples (`[...T[]]`) collapse to the array type `T[]` in
        // tsc's diagnostic display, except when the rest element is a type
        // parameter (which keeps the bracketed `[...T]` form). The canonical
        // type formatter already implements this rule, so defer to it instead
        // of building a per-element display that would produce `[...T[]]`.
        if elements.len() == 1 && elements[0].rest {
            return None;
        }

        let mut parts = Vec::with_capacity(elements.len());
        for element in elements {
            // Rest element `type_id` is the array type itself (e.g.
            // `number[]`), not the element type. The canonical tuple printer
            // renders rest elements as `...{type_id}` — a bare `...` prefix —
            // so do the same here. Wrapping `part` with `[]` produced
            // `...number[][]` instead of `...number[]`.
            let mut part = self.format_type_for_assignability_message(element.type_id);
            if element.optional {
                part.push('?');
            }
            if element.rest {
                part = format!("...{part}");
            }
            parts.push(part);
        }
        let body = format!("[{}]", parts.join(", "));
        if source_is_readonly_tuple {
            Some(format!("readonly {body}"))
        } else {
            Some(body)
        }
    }

    pub(crate) fn object_literal_source_type_display(
        &mut self,
        expr_idx: NodeIndex,
        target: Option<TypeId>,
    ) -> Option<String> {
        // Only skip parentheses, not type assertions.  When the source is
        // `<foo>({})`, the diagnostic should display the asserted type name
        // `foo`, not the inner object literal `{}`.  Returning `None` here
        // lets the caller fall through to `get_type_of_node` which yields
        // the asserted type.
        let expr_idx = self.ctx.arena.skip_parenthesized(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind == syntax_kind_ext::RETURN_STATEMENT
            && let Some(return_stmt) = self.ctx.arena.get_return_statement(node)
            && return_stmt.expression.is_some()
        {
            return self.object_literal_source_type_display(return_stmt.expression, target);
        }
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }

        let literal = self.ctx.arena.get_literal_expr(node)?;
        let target = target.map(|target| self.evaluate_type_for_assignability(target));
        if let Some(display) =
            self.computed_index_signature_object_literal_source_display(expr_idx, target)
        {
            return Some(display);
        }
        let preserve_literal_source_for_normalized_union =
            target.is_some_and(|target| self.target_is_normalized_object_literal_union(target));
        let target_shape = target.and_then(|target| {
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, target)
        });
        let mut parts = Vec::new();
        let mut contextual_index_key_kind: Option<&'static str> = None;
        let mut contextual_index_value_types = Vec::new();
        let mut all_contextual_index_properties = !literal.elements.nodes.is_empty();
        for child_idx in literal.elements.nodes.iter().copied() {
            let child = self.ctx.arena.get(child_idx)?;
            let (name_idx, value_idx) = if child.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                let prop = self.ctx.arena.get_property_assignment(child)?;
                (prop.name, prop.initializer)
            } else if child.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                let prop = self.ctx.arena.get_shorthand_property(child)?;
                (prop.name, prop.name)
            } else {
                return None;
            };
            let name_node = self.ctx.arena.get(name_idx)?;
            let display_name = match name_node.kind {
                k if k == tsz_scanner::SyntaxKind::Identifier as u16 => self
                    .ctx
                    .arena
                    .get_identifier(name_node)?
                    .escaped_text
                    .clone(),
                k if k == tsz_scanner::SyntaxKind::StringLiteral as u16
                    || k == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
                {
                    let lit = self.ctx.arena.get_literal(name_node)?;
                    format!("\"{}\"", lit.text)
                }
                k if k == tsz_scanner::SyntaxKind::NumericLiteral as u16 => {
                    self.ctx.arena.get_literal(name_node)?.text.clone()
                }
                k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                    if let Some(name) = self.get_member_name_display_text(name_idx) {
                        name
                    } else {
                        let computed = self.ctx.arena.get_computed_property(name_node)?;
                        let expr = self.node_text(computed.expression)?;
                        format!("[{expr}]", expr = expr.trim())
                    }
                }
                _ => return None,
            };
            let computed_index_kind =
                self.contextual_computed_index_key_kind(name_idx, target_shape.as_deref());
            match (contextual_index_key_kind, computed_index_kind) {
                (None, Some(kind)) => contextual_index_key_kind = Some(kind),
                (Some(existing), Some(kind)) if existing == kind => {}
                _ => all_contextual_index_properties = false,
            }
            let property_name = self
                .get_property_name(name_idx)
                .map(|name| self.ctx.types.intern_string(&name));
            if self
                .ctx
                .arena
                .get(value_idx)
                .is_some_and(|node| node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16)
            {
                parts.push(format!("{display_name}: this"));
                continue;
            }
            let value_type = self.get_type_of_node(value_idx);
            if value_type == TypeId::ERROR {
                return None;
            }

            // tsc preserves a fresh literal property only when the contextual
            // (target) property type carries a literal of the *same* primitive
            // base (mirroring `getWidenedLiteralLikeTypeForContextualType`); the
            // base must match so a numeric source against a string-literal target
            // still widens. The former check recognized string literals only, so
            // numeric/boolean/bigint properties were wrongly widened.
            let source_literal_base =
                diagnostic_query::widen_literal_to_primitive(self.ctx.types, value_type);
            let target_accepts_literal = property_name
                .and_then(|name| {
                    // First try the direct object shape
                    if let Some(shape) = target_shape.as_ref() {
                        return shape
                            .properties
                            .iter()
                            .find(|p| p.name == name)
                            .filter(|p| {
                                self.type_contains_literal_of_primitive_base(
                                    p.type_id,
                                    source_literal_base,
                                )
                            })
                            .map(|p| p.type_id);
                    }
                    // For union targets, check each member's properties. The
                    // per-member gate already enforces the base match, so the
                    // returned type needs no re-check below.
                    let target = target?;
                    let members = diagnostic_query::union_members(self.ctx.types, target)?;
                    for member in &members {
                        if let Some(member_shape) =
                            crate::query_boundaries::common::object_shape_for_type(
                                self.ctx.types,
                                *member,
                            )
                            && let Some(prop) =
                                member_shape.properties.iter().find(|p| p.name == name)
                            && self.type_contains_literal_of_primitive_base(
                                prop.type_id,
                                source_literal_base,
                            )
                        {
                            return Some(prop.type_id);
                        }
                    }
                    None
                })
                .is_some();
            if let Some(literal_display) = self.literal_expression_display(value_idx) {
                let preserve_normalized_union_boolean = preserve_literal_source_for_normalized_union
                    && matches!(literal_display.as_str(), "true" | "false");
                if target_accepts_literal || preserve_normalized_union_boolean {
                    parts.push(format!("{display_name}: {literal_display}"));
                    continue;
                }
            }

            // For nested object literals, recurse
            if let Some(nested_display) = self.object_literal_source_type_display(value_idx, None) {
                parts.push(format!("{display_name}: {nested_display}"));
                continue;
            }

            // Fall back to type system for non-literal expressions.
            // For function properties, merge parameter types from target shape.
            let value_display_type = property_name
                .and_then(|name| {
                    let shape = target_shape.as_ref()?;
                    shape
                        .properties
                        .iter()
                        .find(|prop| prop.name == name)
                        .map(|prop| prop.type_id)
                })
                .filter(|target_prop_type| {
                    crate::query_boundaries::diagnostics::function_shape(self.ctx.types, value_type)
                        .is_some()
                        && crate::query_boundaries::diagnostics::function_shape(
                            self.ctx.types,
                            *target_prop_type,
                        )
                        .is_some()
                })
                .and_then(|target_prop_type| {
                    let value_shape = crate::query_boundaries::diagnostics::function_shape(
                        self.ctx.types,
                        value_type,
                    )?;
                    let target_shape = crate::query_boundaries::diagnostics::function_shape(
                        self.ctx.types,
                        target_prop_type,
                    )?;
                    let merged_params: Vec<_> = value_shape
                        .params
                        .iter()
                        .zip(target_shape.params.iter())
                        .map(|(value_param, target_param)| {
                            diagnostic_query::display_param_with_type(
                                value_param,
                                target_param.type_id,
                            )
                        })
                        .collect();
                    let merged = diagnostic_query::function_type_with_params_replaced(
                        self.ctx.types,
                        &value_shape,
                        merged_params,
                    );
                    Some(merged)
                })
                .unwrap_or(value_type);
            let value_display_type = if target_accepts_literal {
                value_display_type
            } else {
                let widened = self.widen_type_for_display(value_display_type);
                if crate::query_boundaries::common::is_template_literal_type(
                    self.ctx.types,
                    widened,
                ) || crate::query_boundaries::common::is_string_intrinsic_type(
                    self.ctx.types,
                    widened,
                ) {
                    TypeId::STRING
                } else {
                    widened
                }
            };
            let widened_value_display_type =
                self.widen_function_like_display_type(value_display_type);
            let value_display =
                self.format_type_for_assignability_message(widened_value_display_type);
            if computed_index_kind.is_some() {
                contextual_index_value_types.push(widened_value_display_type);
            }
            parts.push(format!("{display_name}: {value_display}"));
        }

        if parts.is_empty() {
            return Some("{}".to_string());
        }

        if let Some(index_display) = self.contextual_index_signature_source_display(
            all_contextual_index_properties,
            contextual_index_key_kind,
            contextual_index_value_types,
        ) {
            return Some(index_display);
        }

        Some(format!("{{ {}; }}", parts.join("; ")))
    }

    pub(in crate::error_reporter) fn is_literal_sensitive_assignment_target(
        &mut self,
        target: TypeId,
    ) -> bool {
        if crate::query_boundaries::common::string_intrinsic_components(self.ctx.types, target)
            .is_some_and(|(_, type_arg)| type_arg == TypeId::STRING)
        {
            return false;
        }

        let target = self.evaluate_type_for_assignability(target);
        if target == TypeId::UNDEFINED || target == TypeId::NULL {
            return true;
        }
        // A deferred generic indexed access `O[K]` (K a type parameter, e.g.
        // `K extends keyof O`) stays unevaluated rather than distributing into
        // the value-type union. tsc still preserves a source literal against
        // such a target when the value union reachable through `K`'s constraint
        // admits it (`Type '123' is not assignable to type 'Type[K]'`, not
        // `'number'`). Consult the constraint value union `O[keyof O]` for the
        // literal-sensitivity decision; the displayed target text stays `O[K]`.
        if let Some((object_type, index_type)) =
            crate::query_boundaries::common::index_access_types(self.ctx.types, target)
            && let Some(index_param) =
                crate::query_boundaries::common::type_param_info(self.ctx.types, index_type)
            && let Some(index_constraint) = index_param.constraint
        {
            self.ensure_relation_input_ready(object_type);
            let evaluated_object = self.evaluate_type_with_env(object_type);
            let value_index_access = self
                .ctx
                .types
                .factory()
                .index_access(evaluated_object, index_constraint);
            let value_union = self.evaluate_type_with_env(value_index_access);
            if value_union != target
                && value_union != TypeId::ERROR
                && self.is_literal_sensitive_assignment_target_inner(value_union)
            {
                return true;
            }
        }
        self.is_literal_sensitive_assignment_target_inner(target)
    }

    /// Check if the target type is a bare type parameter (e.g. `T`).
    /// Used to decide whether to widen literals in error messages:
    /// tsc widens `""` → `string` when the target is a simple type param,
    /// but preserves literals for complex generic targets like `Type[K]`.
    pub(in crate::error_reporter) fn target_is_bare_type_parameter(&self, target: TypeId) -> bool {
        crate::query_boundaries::state::checking::is_type_parameter(self.ctx.types, target)
    }

    fn is_literal_sensitive_assignment_target_inner(&self, target: TypeId) -> bool {
        // NoInfer<T> wraps T without changing its literal nature — unwrap and check inner
        if let Some(inner) =
            crate::query_boundaries::common::no_infer_inner_type(self.ctx.types, target)
        {
            return self.is_literal_sensitive_assignment_target_inner(inner);
        }
        if crate::query_boundaries::common::literal_value(self.ctx.types, target).is_some() {
            return true;
        }
        if crate::query_boundaries::common::enum_def_id(self.ctx.types, target).is_some() {
            return true;
        }
        if crate::query_boundaries::common::is_symbol_or_unique_symbol(self.ctx.types, target)
            && target != TypeId::SYMBOL
        {
            return true;
        }
        // Template literal types (e.g., `:${string}:`) expect specific string
        // patterns — preserving the source literal in the diagnostic is more
        // informative than showing widened `string`.
        if crate::query_boundaries::common::is_template_literal_type(self.ctx.types, target) {
            return true;
        }
        if let Some(list) = crate::query_boundaries::common::union_list_id(self.ctx.types, target)
            .or_else(|| {
                crate::query_boundaries::common::intersection_list_id(self.ctx.types, target)
            })
        {
            return self
                .ctx
                .types
                .type_list(list)
                .iter()
                .copied()
                .any(|member| {
                    // tsc stores `boolean` inside a union as `true | false`, whose
                    // members are singletons, so a `string | boolean` target keeps
                    // a literal source (`5` stays `5`).
                    member == TypeId::BOOLEAN
                        || self.is_literal_sensitive_assignment_target_inner(member)
                });
        }
        // Deferred instantiable targets (`Cfg[K]`, `Cond<T>`, alias/enum refs)
        // answer through their constraints, mirroring tsc's
        // `typeCouldHaveTopLevelSingletonTypes` -> `getConstraintOfType`.
        if crate::query_boundaries::diagnostics::is_deferred_instantiable_display_target(
            self.ctx.types,
            target,
        ) {
            return crate::query_boundaries::diagnostics::relation_target_could_hold_singleton(
                self.ctx.types,
                &self.ctx,
                target,
            );
        }
        target == TypeId::NEVER
    }

    /// The parent-enum display type of an enum-member assignment source when
    /// the top-level display widens it, mirroring tsc's `reportRelationError`
    /// gate: the member generalizes exactly when the target could not hold a
    /// top-level singleton type. A literal, template-literal, enum, or
    /// singleton-capable union/instantiable target preserves the member
    /// spelling (`EM.X`, returns `None`); a primitive or all-primitive
    /// union/intersection target widens it (`Some(EM)`).
    pub(in crate::error_reporter) fn widened_enum_member_assignment_source(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<TypeId> {
        let widened_source = self.widen_enum_member_type(source);
        if widened_source == source {
            return None;
        }

        let target = self.evaluate_type_for_assignability(target);
        (!crate::query_boundaries::diagnostics::relation_target_could_hold_singleton(
            self.ctx.types,
            &self.ctx,
            target,
        ))
        .then_some(widened_source)
    }

    pub(in crate::error_reporter) fn unresolved_unused_renaming_property_in_type_query(
        &self,
        name: &str,
        idx: NodeIndex,
    ) -> Option<String> {
        let mut saw_type_query = false;
        let mut current = idx;
        let mut guard = 0;

        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::TYPE_QUERY {
                saw_type_query = true;
            }

            if matches!(
                node.kind,
                syntax_kind_ext::FUNCTION_TYPE
                    | syntax_kind_ext::CONSTRUCTOR_TYPE
                    | syntax_kind_ext::CALL_SIGNATURE
                    | syntax_kind_ext::CONSTRUCT_SIGNATURE
                    | syntax_kind_ext::METHOD_SIGNATURE
                    | syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::FUNCTION_EXPRESSION
                    | syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::METHOD_DECLARATION
                    | syntax_kind_ext::CONSTRUCTOR
                    | syntax_kind_ext::GET_ACCESSOR
                    | syntax_kind_ext::SET_ACCESSOR
            ) {
                if !saw_type_query {
                    return None;
                }
                return self.find_renamed_binding_property_for_name(current, name);
            }

            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        None
    }

    fn find_renamed_binding_property_for_name(
        &self,
        root: NodeIndex,
        name: &str,
    ) -> Option<String> {
        let mut stack = vec![root];
        while let Some(node_idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(node_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::BINDING_ELEMENT
                && let Some(binding) = self.ctx.arena.get_binding_element(node)
                && binding.property_name.is_some()
                && binding.name.is_some()
                && self.ctx.arena.get_identifier_text(binding.name) == Some(name)
            {
                let prop_name = self
                    .ctx
                    .arena
                    .get_identifier_text(binding.property_name)
                    .map(str::to_string)?;
                return Some(prop_name);
            }

            stack.extend(self.ctx.arena.get_children(node_idx));
        }
        None
    }

    /// True when `expr_idx` references an identifier whose *declared* type is
    /// `unknown` or `any`, but whose value at this position has been
    /// flow-narrowed to a concrete type `source` (e.g. by a `x is T`
    /// type-predicate guard).
    ///
    /// In that case the written `unknown`/`any` annotation no longer describes
    /// the value, so the diagnostic source display must render the narrowed
    /// type rather than repainting it with the declared annotation text. `tsc`
    /// renders the narrowed type here, so the annotation-recovery heuristics
    /// (which exist to recover lost alias/parameter *names*, not to widen back
    /// to a supertype) must be suppressed.
    ///
    /// The check is intentionally scoped to the `unknown`/`any` declarations
    /// only: those are the universal supertypes for which narrowing to a
    /// concrete type is unambiguous, and a concrete declared type already
    /// equals its own narrowed form (so the annotation faithfully describes the
    /// value). It performs no relation work, so it is free of the cache
    /// side-effects that gate the surrounding display heuristics.
    pub(in crate::error_reporter) fn source_identifier_narrowed_from_unknown_or_any(
        &mut self,
        expr_idx: NodeIndex,
        source: TypeId,
    ) -> bool {
        if matches!(source, TypeId::UNKNOWN | TypeId::ANY | TypeId::ERROR) {
            return false;
        }
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(sym_id) = self.resolve_identifier_symbol(expr_idx) else {
            return false;
        };
        matches!(
            self.get_type_of_symbol(sym_id),
            TypeId::UNKNOWN | TypeId::ANY
        )
    }

    pub(in crate::error_reporter) fn declared_identifier_source_display(
        &mut self,
        expr_idx: NodeIndex,
        target: TypeId,
        expr_display_type: TypeId,
    ) -> Option<String> {
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }
        // The source is a *declared* identifier reference, not a fresh function
        // expression, so its signature return literals (`{ m(): 1 }`, `() => 1`)
        // are rendered verbatim — `tsc` widens only fresh literals. Fresh
        // function-expression sources never reach this path and keep widening.
        let _preserve_signature_returns =
            crate::error_reporter::core::type_display::PreserveSignatureReturnLiteralsScope::enter(
            );
        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::VARIABLE) {
            return None;
        }
        // Merged INTERFACE+VALUE: `get_type_of_symbol` returns the interface side.
        if symbol.has_any_flags(tsz_binder::symbol_flags::INTERFACE)
            && !symbol.has_any_flags(tsz_binder::symbol_flags::CLASS)
        {
            return None;
        }

        let declared_type = self.get_type_of_symbol(sym_id);
        if matches!(declared_type, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }
        // A flow-narrowed `any` operand keeps its declared `any` here via the
        // `prefer_declared_display` path below; suppress it so the narrowed type
        // is rendered, matching `tsc`. (`unknown` is already handled by the
        // guard above.)
        if self.source_identifier_narrowed_from_unknown_or_any(expr_idx, expr_display_type) {
            return None;
        }
        if let Some(annotation_text) = self.declared_diagnostic_source_annotation_text(expr_idx)
            && let Some(declared_enum_symbol) = self
                .enum_symbol_from_enumish_type(declared_type)
                .or_else(|| self.enum_symbol_from_enumish_type(expr_display_type))
            && Some(declared_enum_symbol) == self.enum_symbol_from_enumish_type(target)
            && !annotation_text.contains(" | ")
            && !annotation_text.contains(" & ")
            && !annotation_text.contains('<')
        {
            return Some(self.format_declared_annotation_for_diagnostic(&annotation_text));
        }
        let expr_enum_display_type = if self
            .enum_symbol_from_enumish_type(expr_display_type)
            .is_some()
        {
            expr_display_type
        } else {
            declared_type
        };
        let expr_enum_symbol = self.enum_symbol_from_enumish_type(expr_enum_display_type);
        let target_enum_symbol = self.enum_symbol_from_enumish_type(target);
        if expr_enum_symbol.is_some()
            && target_enum_symbol.is_some()
            && expr_enum_symbol != target_enum_symbol
        {
            return Some(
                self.format_assignability_type_for_message(expr_enum_display_type, target),
            );
        }
        if self
            .declared_diagnostic_source_annotation_text(expr_idx)
            .is_some_and(|annotation_text| annotation_text.trim_start().starts_with("typeof "))
        {
            return None;
        }
        let type_query_alias_def_id = self.declared_source_type_query_alias_def_id(expr_idx);
        let prefer_declared_display = if declared_type == TypeId::ANY
            && expr_display_type != TypeId::ANY
        {
            let mut decl_idx = symbol.value_declaration;
            let mut decl_node = self.ctx.arena.get(decl_idx)?;
            if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                && let Some(ext) = self.ctx.arena.get_extended(decl_idx)
                && ext.parent.is_some()
                && let Some(parent_node) = self.ctx.arena.get(ext.parent)
                && parent_node.kind == tsz_parser::parser::syntax_kind_ext::VARIABLE_DECLARATION
            {
                decl_idx = ext.parent;
                decl_node = parent_node;
            }
            let is_control_flow_typed_any = self
                .ctx
                .arena
                .get_variable_declaration(decl_node)
                .is_some_and(|decl| {
                    decl.type_annotation.is_none()
                        && !self.ctx.arena.is_const_variable_declaration(decl_idx)
                        && match decl.initializer {
                            idx if idx.is_none() => true,
                            idx => {
                                let inner = self.ctx.arena.skip_parenthesized(idx);
                                inner.is_some()
                                    && self.ctx.arena.get(inner).is_some_and(|node| {
                                        node.kind == tsz_scanner::SyntaxKind::NullKeyword as u16
                                            || node.kind
                                                == tsz_scanner::SyntaxKind::UndefinedKeyword as u16
                                            || self.ctx.arena.get_identifier(node).is_some_and(
                                                |ident| ident.escaped_text == "undefined",
                                            )
                                    })
                            }
                        }
                });
            !is_control_flow_typed_any
        } else {
            // A type is strictly narrower when it is a subtype or when flow
            // eliminated declared union members; the subset check handles
            // surviving members structurally compatible with eliminated ones.
            let expr_is_assignability_narrower = expr_display_type != declared_type
                && self
                    .diagnostic_source_narrowing_relation_outcome(expr_display_type, declared_type)
                    .related
                && !self
                    .diagnostic_source_narrowing_relation_outcome(declared_type, expr_display_type)
                    .related;
            let expr_is_union_subset_narrower = expr_display_type != declared_type
                && self.is_strict_union_member_subset(expr_display_type, declared_type);
            !(expr_is_assignability_narrower || expr_is_union_subset_narrower)
        };

        // If flow narrowing narrowed a nullable union to specifically null or
        // undefined, don't override with the broader declared type. For example,
        // `x: number | null` narrowed to `null` should show
        // "Type 'null' is not assignable to type 'string'", not
        // "Type 'number' is not assignable to type 'string'" (which happens
        // because strip_nullish_for_assignability_display strips the null member
        // when the target is non-nullable, leaving only "number").
        if (expr_display_type == TypeId::NULL || expr_display_type == TypeId::UNDEFINED)
            && expr_display_type != declared_type
            && let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, declared_type)
            && members.contains(&expr_display_type)
        {
            return None;
        }

        if let Some(display) =
            self.identifier_wide_symbol_object_literal_source_display(expr_idx, target)
        {
            return Some(display);
        }
        if let Some(display) = self.identifier_array_object_literal_source_display(expr_idx, target)
        {
            return Some(display);
        }
        if let Some(display) = self.identifier_literal_initializer_source_display(expr_idx, target)
        {
            return Some(display);
        }
        if prefer_declared_display
            && let Some(display) =
                self.declared_numeric_literal_union_alias_source_display(expr_idx, declared_type)
        {
            return Some(display);
        }
        if prefer_declared_display
            && let Some(display) =
                self.recursive_alias_application_source_display(expr_idx, declared_type)
        {
            return Some(display);
        }
        if let Some(display) = self.narrowed_string_literal_residual_union_display(
            declared_type,
            expr_display_type,
            target,
        ) {
            return Some(display);
        }
        if let Some(display) = self.rebuilt_array_source_display(declared_type, target) {
            return Some(display);
        }
        if let Some(display) =
            self.broad_mapped_index_signature_source_display(declared_type, target)
        {
            return Some(display);
        }

        // Preserve literal property types from declared annotations while
        // leaving fresh object-literal display_properties to the widening path.
        if prefer_declared_display
            && self
                .ctx
                .types
                .get_display_properties(declared_type)
                .is_none()
        {
            let widened =
                crate::query_boundaries::common::widen_type(self.ctx.types, declared_type);
            if widened != declared_type {
                let literal_display =
                    self.format_assignability_type_for_message(declared_type, target);
                let widened_display = self.format_assignability_type_for_message(widened, target);
                if literal_display != widened_display {
                    // tsc widens a declared *unit-literal* source (`0n`, `"x"`,
                    // `42`, `true`) to its base when the target cannot hold a
                    // literal (`boolean`, `bigint`, …), and keeps the literal only
                    // against a literal-sensitive target (`0`, `"x"`). The
                    // call-argument source path already mirrors this; do the same
                    // for return/assignment identifier sources so the three
                    // positions agree with tsc. Compound literal surfaces
                    // (tuples, objects, `as const`) have no scalar `literal_value`
                    // and keep their existing preserve-the-literal behaviour.
                    if crate::query_boundaries::assignability_alias_display::is_unit_literal_type(
                        self.ctx.types.as_type_database(),
                        declared_type,
                    ) && !self.is_literal_sensitive_assignment_target(target)
                    {
                        return Some(widened_display);
                    }
                    return Some(literal_display);
                }
            }
        }

        if prefer_declared_display
            && declared_type != expr_display_type
            && crate::query_boundaries::diagnostics::finite_mapped_property_surface(
                self.ctx.types,
                declared_type,
            )
            && !diagnostic_query::type_has_displayable_name(self.ctx.types, target)
        {
            return Some(self.format_type_diagnostic(declared_type));
        }

        let mut declared_display_type =
            self.widen_function_like_display_type(self.widen_type_for_display(declared_type));
        let expr_display_type =
            self.widen_function_like_display_type(self.widen_type_for_display(expr_display_type));
        if self.ctx.compiler_options.exact_optional_property_types
            && (crate::query_boundaries::common::callable_shape_for_type(
                self.ctx.types,
                declared_type,
            )
            .is_some_and(|shape| {
                shape
                    .call_signatures
                    .iter()
                    .chain(shape.construct_signatures.iter())
                    .any(|sig| !sig.type_params.is_empty())
            }) || crate::query_boundaries::common::function_shape_for_type(
                self.ctx.types,
                declared_type,
            )
            .is_some_and(|shape| !shape.type_params.is_empty()))
        {
            declared_display_type = declared_type;
        }
        let declared_is_generic_callable = crate::query_boundaries::common::callable_shape_for_type(
            self.ctx.types,
            declared_display_type,
        )
        .is_some_and(|shape| {
            shape
                .call_signatures
                .iter()
                .chain(shape.construct_signatures.iter())
                .any(|sig| !sig.type_params.is_empty())
        })
            || crate::query_boundaries::common::function_shape_for_type(
                self.ctx.types,
                declared_display_type,
            )
            .is_some_and(|shape| !shape.type_params.is_empty());
        if declared_is_generic_callable
            && let Some(annotation_text) = self.declared_diagnostic_source_annotation_text(expr_idx)
        {
            if self.ctx.compiler_options.exact_optional_property_types
                && prefer_declared_display
                && annotation_text.contains("?:")
            {
                return Some(self.format_declared_annotation_for_diagnostic(&annotation_text));
            }
            // Check if this is a single-call-signature OR single-construct-signature
            // callable that tsc displays in arrow syntax (e.g., `<S>() => S[]` or
            // `new <T>(x: T) => T`). For these, skip annotation text and use the
            // TypeFormatter which correctly produces arrow syntax.
            let should_use_arrow_syntax = crate::query_boundaries::common::callable_shape_for_type(
                self.ctx.types,
                declared_display_type,
            )
            .is_some_and(|shape| {
                let single_call =
                    shape.call_signatures.len() == 1 && shape.construct_signatures.is_empty();
                let single_construct =
                    shape.construct_signatures.len() == 1 && shape.call_signatures.is_empty();
                (single_call || single_construct)
                    && shape.properties.is_empty()
                    && shape.string_index.is_none()
                    && shape.number_index.is_none()
            });
            if !should_use_arrow_syntax {
                let annotation_display =
                    self.format_declared_annotation_for_diagnostic(&annotation_text);
                let expr_display =
                    self.format_assignability_type_for_message(expr_display_type, target);
                if prefer_declared_display && annotation_display != expr_display {
                    return Some(annotation_display);
                }
            }
        }
        let declared_display = if let Some(def_id) = type_query_alias_def_id {
            self.format_type_diagnostic_for_assignability_display_skipping_type_alias(
                declared_display_type,
                def_id,
            )
        } else if declared_is_generic_callable {
            let mut formatter =
                tsz_solver::TypeFormatter::with_symbols(self.ctx.types, &self.ctx.binder.symbols)
                    .with_def_store(&self.ctx.definition_store)
                    .with_diagnostic_mode()
                    .with_strict_null_checks(self.ctx.compiler_options.strict_null_checks)
                    .with_exact_optional_property_types(
                        self.ctx.compiler_options.exact_optional_property_types,
                    );
            formatter.format(declared_display_type).into_owned()
        } else {
            self.format_assignability_type_for_message(declared_display_type, target)
        };
        let expr_display = self.format_assignability_type_for_message(expr_display_type, target);

        (prefer_declared_display && declared_display != expr_display).then(|| {
            self.canonicalize_assignment_numeric_literal_union_display(
                declared_type,
                target,
                declared_display,
            )
        })
    }
}
