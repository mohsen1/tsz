use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

/// Collapse runs of horizontal whitespace (spaces and tabs) to a single space
/// outside string and template literals, so a single-line annotation echoed
/// verbatim from source text matches `tsc`'s printer, which emits canonical
/// spacing regardless of how the annotation was written (`P   &   Q` ->
/// `P & Q`). Only the interior of a `'...'` / `"..."` / `` `...` `` literal keeps
/// its exact spelling (`'a  b'` stays `'a  b'`), matching the canonical-rebuild
/// the `FUNCTION_TYPE`/`CONSTRUCTOR_TYPE` path
/// ([`CheckerState::canonical_function_type_annotation_text`]) already applies to
/// signatures.
///
/// Line breaks are preserved verbatim (they are not collapsed into the single
/// space): a *multi-line* annotation must keep its newline so
/// [`CheckerState::sanitize_type_annotation_text_for_diagnostic`]'s
/// first-newline guard still fires and routes it to the structural fallback (a
/// multi-line intersection carrying a type-literal member renders through the
/// structural formatter, not this raw echo). Collapsing the newline here would
/// smuggle such an annotation past that guard and change its rendering.
fn normalize_declared_annotation_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' | '"' | '`' => {
                if pending_space && !out.is_empty() {
                    out.push(' ');
                }
                pending_space = false;
                out.push(c);
                // Copy the literal verbatim, honoring backslash escapes, up to
                // and including the matching closing delimiter.
                while let Some(inner) = chars.next() {
                    out.push(inner);
                    if inner == '\\' {
                        if let Some(escaped) = chars.next() {
                            out.push(escaped);
                        }
                    } else if inner == c {
                        break;
                    }
                }
            }
            ' ' | '\t' => {
                pending_space = true;
            }
            // Line breaks are structural to the newline guard downstream — keep
            // them, and let a following non-space char stand on its own.
            '\n' | '\r' => {
                pending_space = false;
                out.push(c);
            }
            other => {
                if pending_space && !out.is_empty() {
                    out.push(' ');
                }
                pending_space = false;
                out.push(other);
            }
        }
    }
    out
}

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

    /// Whether `annotation_idx` is a bare `keyof any` / `keyof unknown` /
    /// `keyof never` type operator.
    ///
    /// `tsc`'s `getIndexType` resolves such an operand to its fixed key-space
    /// result (`string | number | symbol` / `never`) at type-construction time,
    /// so the resulting type carries no `aliasSymbol` and no memory of having
    /// been written as `keyof <operand>` — the operator never reaches
    /// `typeToString`. tsz's declared-annotation source text fallback
    /// (`declared_type_annotation_text_for_symbol_type` /
    /// `declared_type_annotation_text_for_expression_with_options`) would
    /// otherwise reproduce the written `keyof any` text verbatim for a
    /// `value: keyof any` declaration, repainting the already-correctly-
    /// evaluated structural union with syntax tsc never preserves. Scoped to a
    /// literal `any`/`unknown`/`never` keyword operand — a named/aliased
    /// operand keeps the existing `keyof Name` display path.
    pub(in crate::error_reporter) fn annotation_is_keyof_over_degenerate_operand(
        arena: &tsz_parser::NodeArena,
        annotation_idx: NodeIndex,
    ) -> bool {
        use crate::types_domain::queries::lib_resolution::{
            keyword_name_to_type_id, keyword_syntax_to_type_id,
        };
        let Some(node) = arena.get(annotation_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::TYPE_OPERATOR {
            return false;
        }
        let Some(type_op) = arena.get_type_operator(node) else {
            return false;
        };
        if type_op.operator != tsz_scanner::SyntaxKind::KeyOfKeyword as u16 {
            return false;
        }
        let Some(operand_node) = arena.get(type_op.type_node) else {
            return false;
        };
        // A primitive keyword operand (`any`, `unknown`, `never`) is parsed as
        // a `TYPE_REFERENCE` naming the keyword, not a bare keyword token node
        // (mirrors `type_node_is_primitive_keyword`'s two-shape check above);
        // fall back to the raw keyword-token kind for the rarer shape where the
        // parser does emit one directly.
        let operand_type_id = if operand_node.kind == syntax_kind_ext::TYPE_REFERENCE {
            arena
                .get_type_ref(operand_node)
                .and_then(|type_ref| arena.get(type_ref.type_name))
                .and_then(|name_node| arena.get_identifier(name_node))
                .and_then(|ident| keyword_name_to_type_id(&ident.escaped_text))
        } else {
            keyword_syntax_to_type_id(operand_node.kind)
        };
        matches!(
            operand_type_id,
            Some(TypeId::ANY | TypeId::UNKNOWN | TypeId::NEVER)
        )
    }

    /// Whether a declared type annotation is a structural tuple or
    /// function/constructor type that tsc always renders through `typeToString`
    /// (`[number, string]`, `(a: number) => void`) rather than preserving as
    /// written.
    ///
    /// tsz's structural `TypeFormatter` renders these identically to tsc —
    /// including named / optional / rest tuple members and function parameter
    /// names — so the declared-annotation source-text fallback
    /// (`declared_type_annotation_text_for_expression_with_options`) must NOT
    /// reproduce the written form, which leaks the author's whitespace:
    /// `[number,string]` or `[number,   string]` instead of `[number, string]`,
    /// and `(a:number,b:string)=>void` instead of `(a: number, b: string) =>
    /// void`. Returning `true` here makes the caller fall back to the canonical
    /// structural formatter, matching tsc. Parenthesized wrappers are unwrapped
    /// so `([number, string])` is classified by its inner node. Mirrors
    /// [`Self::annotation_is_keyof_over_degenerate_operand`], which drops the
    /// written form for the same reason.
    pub(in crate::error_reporter) fn annotation_is_canonicalized_structural_type(
        arena: &tsz_parser::NodeArena,
        annotation_idx: NodeIndex,
    ) -> bool {
        Self::unwrapped_annotation_node(arena, annotation_idx)
            .and_then(|idx| arena.get(idx))
            .is_some_and(|node| {
                matches!(
                    node.kind,
                    syntax_kind_ext::TUPLE_TYPE
                        | syntax_kind_ext::FUNCTION_TYPE
                        | syntax_kind_ext::CONSTRUCTOR_TYPE
                )
            })
    }

    /// Whether a declared type annotation, after unwrapping parenthesized
    /// wrappers, is a *non-generic* inline `FUNCTION_TYPE` or `CONSTRUCTOR_TYPE`
    /// — a bare call/construct signature written directly at a declaration site
    /// (`() => 1`, `(x: 1) => void`, `new () => 1`).
    ///
    /// Such a source is *declared* (non-fresh): tsc's `getWidenedType` widens
    /// only fresh literals, so a literal written inside a declared signature
    /// (`() => 1`) renders verbatim while its whitespace is still canonicalized
    /// by `typeToString` (`()=>1` -> `() => 1`). This predicate gates the
    /// canonical-formatter-under-preserve-scope source path that reconciles
    /// those two rules; a fresh function-expression source is not an inline
    /// signature annotation on a declared binding and never matches here.
    ///
    /// A *generic* signature (`<S>() => S[]`, `new <T>(x: T) => T`) is excluded:
    /// those keep the established `declared_identifier_source_display` handling,
    /// which owns tsc's alias-name / `?:`-surface rules for type-parameterized
    /// callables. The generic-ness is read from the signature's own
    /// `type_parameters` (both node kinds share `FunctionTypeData`).
    pub(in crate::error_reporter) fn annotation_is_inline_signature_type(
        arena: &tsz_parser::NodeArena,
        annotation_idx: NodeIndex,
    ) -> bool {
        let Some(node) =
            Self::unwrapped_annotation_node(arena, annotation_idx).and_then(|idx| arena.get(idx))
        else {
            return false;
        };
        if !matches!(
            node.kind,
            syntax_kind_ext::FUNCTION_TYPE | syntax_kind_ext::CONSTRUCTOR_TYPE
        ) {
            return false;
        }
        arena
            .get_function_type(node)
            .is_some_and(|data| data.type_parameters.is_none())
    }

    /// The `NodeIndex` of `annotation_idx` after peeling parenthesized-type
    /// wrappers (`([number, string])` is classified by its inner tuple).
    /// Bounded against pathological wrapper nesting; returns `None` on a
    /// missing node or when the wrapper depth bound is exceeded.
    fn unwrapped_annotation_node(
        arena: &tsz_parser::NodeArena,
        annotation_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut idx = annotation_idx;
        for _ in 0..16 {
            let node = arena.get(idx)?;
            if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE {
                idx = arena.get_wrapped_type(node)?.type_node;
                continue;
            }
            return Some(idx);
        }
        None
    }

    /// Whether a declared type annotation is one whose written form `tsc` never
    /// preserves in diagnostics, so the declared-annotation source-text fallback
    /// must return `None` and let the caller render the canonical structural
    /// form instead. Combines the three carve-outs — a `typeof`-alias reference,
    /// a `keyof` over a degenerate `any`/`unknown`/`never` operand, and an
    /// inline structural tuple / function / constructor type — into one gate so
    /// the several echo sites cannot drift when a future carve-out is added.
    pub(in crate::error_reporter) fn annotation_display_must_use_structural_formatter(
        &self,
        arena: &tsz_parser::NodeArena,
        annotation_idx: NodeIndex,
    ) -> bool {
        self.annotation_names_type_query_alias(arena, annotation_idx)
            || Self::annotation_is_keyof_over_degenerate_operand(arena, annotation_idx)
            || Self::annotation_is_canonicalized_structural_type(arena, annotation_idx)
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

    /// When the assignment target's declared annotation is a **bare,
    /// non-generic** `TYPE_REFERENCE` to a type alias whose lowered body is
    /// identity-equal to the displayed target type, render that alias's own
    /// declared name.
    ///
    /// `tsc` keys a type's display identity on the alias reference written at
    /// the use site (`aliasSymbol` travels with the *reference*, not the
    /// interned content), so `type First = string | number; type Second =
    /// string | number; const a: Second = flag` renders `Second`. tsz interns
    /// one `TypeId` per content and the reverse `type_to_def` table
    /// (`register_type_to_def`) is earliest-declaration-wins, so a global
    /// lookup answers `First` for both spellings. The written annotation is
    /// the per-occurrence provenance that recovers the reference identity.
    ///
    /// Declines — keeping the established display paths — for:
    /// - a reference with type arguments, or an alias with type parameters (a
    ///   generic application keeps its `Name[Args]` surface already);
    /// - an alias `tsc` renders by its underlying type
    ///   ([`type_alias_displayed_as_underlying`]: computed conditional /
    ///   indexed-access / `keyof` bodies, bare enum / interface / class
    ///   references, intrinsic singletons);
    /// - an alias whose body does not resolve to the exact displayed target
    ///   type, so a narrowed or unrelated target can never be repainted.
    ///
    /// A bare alias-to-alias forwarding body (`type Outer = Inner`) is
    /// **chased**, not declined: resolving a reference to an alias whose own
    /// body is itself just a bare (argument-less) reference to another alias
    /// never builds a new `Type` in tsc — it returns exactly the referenced
    /// alias's own `Type` object, `aliasSymbol` and all (oracle-pinned:
    /// `type Outer = Inner` written at the use site renders `Inner`, even
    /// through several forwarding hops). Chasing the *syntactic* chain here
    /// matters because the alternative — falling through to the global
    /// `type_to_def` reverse map — is first-writer-wins per interned
    /// `TypeId` and can land on an unrelated alias (or a lib alias like
    /// `PropertyKey`) that merely happens to share the same structural
    /// content, not the one actually written in the forwarding chain.
    ///
    /// [`type_alias_displayed_as_underlying`]: crate::query_boundaries::assignability_alias_display::type_alias_displayed_as_underlying
    pub(in crate::error_reporter) fn written_alias_reference_target_display(
        &mut self,
        anchor_idx: NodeIndex,
        target: TypeId,
    ) -> Option<String> {
        let target_expr = self
            .assignment_target_expression(anchor_idx)
            .unwrap_or(anchor_idx);
        self.written_alias_reference_display_for_expression(target_expr, target)
    }

    /// Source-side wrapper for the per-occurrence written-alias gate: resolve
    /// the *source expression* written at the diagnostic anchor and apply
    /// [`Self::written_alias_reference_display_for_expression`] to its declared
    /// annotation. `tsc` renders an identifier source by the declared type's
    /// own `aliasSymbol` — the alias reference written on the *declaration* —
    /// so `type SrcA = { x: number }; type SrcB = { x: number };
    /// declare const sb: SrcB; const n: number = sb` says `SrcB`, not the
    /// first-registered `SrcA`. Every decline of the shared core applies; a
    /// flow-narrowed source additionally declines through the identity guard
    /// (the narrowed type no longer equals the annotation's lowered body).
    pub(in crate::error_reporter) fn written_alias_reference_source_display(
        &mut self,
        anchor_idx: NodeIndex,
        source: TypeId,
    ) -> Option<String> {
        let expr_idx = self
            .direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx))?;
        self.written_alias_reference_display_for_expression(expr_idx, source)
    }

    /// Shared core of the per-occurrence written-alias gate: resolve
    /// `expr_idx`'s declared annotation to a bare, non-generic type-alias
    /// reference whose lowered body is identity-equal to `displayed`, and
    /// render that alias's declared name. See
    /// [`Self::written_alias_reference_target_display`] for the display-model
    /// rationale and the decline list.
    fn written_alias_reference_display_for_expression(
        &mut self,
        expr_idx: NodeIndex,
        displayed: TypeId,
    ) -> Option<String> {
        let def_id = {
            let (arena, annotation_idx) =
                self.declared_type_annotation_node_for_expression(expr_idx)?;
            let type_ref = arena.get_type_ref(arena.get(annotation_idx)?)?;
            if type_ref.type_arguments.is_some() {
                return None;
            }
            // An alias whose own declared RHS is a reference to another name
            // (`type Outer = Inner`, `type BoxNum = Box[number]`,
            // `type Foo2 = Id[{...}]`) either forwards (bare, no type
            // arguments — chased below to the terminal alias tsc actually
            // stamps) or applies (type arguments present — the established
            // application-aware paths already implement tsc's split there:
            // `BoxNum` keeps the alias, the recursive mapped `Id[{...}]` of
            // `deeplyNestedMappedTypes.ts` renders the substituted
            // application; repainting an application reference from here
            // regressed the latter, so it still declines outright). The
            // declaration walk mirrors `annotation_type_query_alias_def_id`.
            let sym_id = self
                .ctx
                .binder
                .resolve_identifier(arena, type_ref.type_name)?;
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            let rhs_reference = symbol.declarations.iter().find_map(|&decl_idx| {
                let decl_node = arena.get(decl_idx)?;
                let alias = arena.get_type_alias(decl_node)?;
                let rhs = arena.get(alias.type_node)?;
                (rhs.kind == syntax_kind_ext::TYPE_REFERENCE)
                    .then(|| arena.get_type_ref(rhs))
                    .flatten()
            });
            if let Some(rhs_ref) = rhs_reference {
                if rhs_ref.type_arguments.is_some() {
                    return None;
                }
                let self_def_id =
                    self.annotation_type_reference_alias_def_id(arena, annotation_idx)?;
                self.terminal_forwarding_alias_def_id(arena, self_def_id)
            } else {
                self.annotation_type_reference_alias_def_id(arena, annotation_idx)?
            }
        };
        let alias_name = {
            let def = self.ctx.definition_store.get(def_id)?;
            if !def.type_params.is_empty() {
                return None;
            }
            def.name
        };
        if crate::query_boundaries::assignability_alias_display::type_alias_displayed_as_underlying(
            self.ctx.types.as_type_database(),
            &self.ctx.definition_store,
            def_id,
        )
        .is_some()
        {
            return None;
        }
        let body = self.ctx.definition_store.get_body(def_id)?;
        // A bare alias-to-alias forwarding body: tsc renders the alias the
        // chain resolves to, not the forwarding name written here. (Routed
        // through the diagnostics boundary re-export — a direct `common::`
        // reference here would grow the #8225 quarantine counter.)
        if crate::query_boundaries::diagnostics::lazy_def_id(
            self.ctx.types.as_type_database(),
            body,
        )
        .and_then(|next| self.ctx.definition_store.get_kind(next))
            == Some(tsz_solver::def::DefKind::TypeAlias)
        {
            return None;
        }
        // Per-occurrence identity guard: the written alias must lower to the
        // exact type being displayed. A narrowed target, a nested property
        // target, or any other type reached through this anchor keeps its
        // established display.
        let resolved_body = self.resolve_lazy_type(body);
        if resolved_body != self.resolve_lazy_type(displayed) {
            return None;
        }
        Some(self.ctx.types.resolve_atom(alias_name))
    }

    /// Follows a bare (argument-less) alias-to-alias reference chain
    /// (`type Outer = Inner;`, possibly several hops deep) from `def_id`'s
    /// own declaration to the terminal alias `tsc` actually stamps.
    ///
    /// Each hop re-derives the *syntactic* RHS from the alias symbol's own
    /// declaration nodes — mirroring the first-hop walk in
    /// [`Self::written_alias_reference_target_display`] — rather than
    /// consulting the global `type_to_def` reverse map, which is
    /// first-writer-wins per interned `TypeId` and can land on an unrelated
    /// alias (or a lib alias like `PropertyKey`) that merely happens to
    /// share the same structural content.
    ///
    /// Stops (returns `def_id` unchanged, on the first hop, or the last
    /// resolved def on a later one) at an alias whose RHS is not a bare
    /// non-generic reference to another type alias — a structural body
    /// (union, object, ...) or an application (`Box[number]`) both
    /// terminate the walk, since an application already has its own
    /// established display policy. Also stops, defensively, when a hop
    /// cannot be resolved (a cross-file declaration this walk's single-arena
    /// lookup cannot follow, a symbol that lost its `TYPE_ALIAS` flag, a
    /// missing definition-store entry) or when a bound on the chain length
    /// is hit, so a malformed or pathological chain degrades to "keep the
    /// last alias resolved" rather than panicking or looping.
    fn terminal_forwarding_alias_def_id(
        &self,
        arena: &tsz_parser::NodeArena,
        mut def_id: tsz_solver::def::DefId,
    ) -> tsz_solver::def::DefId {
        for _ in 0..16 {
            let Some(def) = self.ctx.definition_store.get(def_id) else {
                break;
            };
            let Some(symbol_id) = def.symbol_id else {
                break;
            };
            let Some(symbol) = self.ctx.binder.get_symbol(tsz_binder::SymbolId(symbol_id)) else {
                break;
            };
            let next_name_node = symbol.declarations.iter().find_map(|&decl_idx| {
                let decl_node = arena.get(decl_idx)?;
                let alias = arena.get_type_alias(decl_node)?;
                let rhs = arena.get(alias.type_node)?;
                if rhs.kind != syntax_kind_ext::TYPE_REFERENCE {
                    return None;
                }
                let rhs_ref = arena.get_type_ref(rhs)?;
                if rhs_ref.type_arguments.is_some() {
                    return None;
                }
                Some(rhs_ref.type_name)
            });
            let Some(next_name_node) = next_name_node else {
                break;
            };
            let Some(next_sym_id) = self.ctx.binder.resolve_identifier(arena, next_name_node)
            else {
                break;
            };
            let Some(next_symbol) = self.ctx.binder.get_symbol(next_sym_id) else {
                break;
            };
            if !next_symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS) {
                break;
            }
            let name_atom = self.ctx.types.intern_string(&next_symbol.escaped_name);
            let Some(next_def_id) = self
                .ctx
                .definition_store
                .find_defs_by_name(name_atom)
                .and_then(|defs| {
                    defs.into_iter().find(|candidate| {
                        self.ctx.definition_store.get(*candidate).is_some_and(|nd| {
                            nd.kind == tsz_solver::def::DefKind::TypeAlias
                                && (nd.symbol_id == Some(next_sym_id.0) || nd.name == name_atom)
                        })
                    })
                })
            else {
                break;
            };
            if next_def_id == def_id {
                break;
            }
            def_id = next_def_id;
        }
        def_id
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

    /// Verbatim source text of a leaf annotation node (a parameter name, a
    /// non-signature type, a type-parameter clause). Shared by
    /// [`Self::declared_annotation_source_text`] for the parts of a
    /// `FUNCTION_TYPE`/`CONSTRUCTOR_TYPE` skeleton that are not themselves
    /// rebuilt canonically.
    fn declared_annotation_leaf_text(
        arena: &tsz_parser::NodeArena,
        idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(idx)?;
        let source = arena.source_files.first()?.text.as_ref();
        let start = node.pos as usize;
        let end = node.end as usize;
        if start >= end || end > source.len() {
            return None;
        }
        Some(normalize_declared_annotation_whitespace(
            &source[start..end],
        ))
    }

    /// Declared-annotation text for a parameter/return type position: recurses
    /// into [`Self::canonical_function_type_annotation_text`] when the type at
    /// `idx` is itself a `FUNCTION_TYPE`/`CONSTRUCTOR_TYPE` (a higher-order
    /// signature, e.g. the parameter type in `(f: (x:1)=>void) => void`), and
    /// falls back to the verbatim leaf text otherwise.
    fn declared_annotation_type_text(
        arena: &tsz_parser::NodeArena,
        idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(idx)?;
        if matches!(
            node.kind,
            syntax_kind_ext::FUNCTION_TYPE | syntax_kind_ext::CONSTRUCTOR_TYPE
        ) {
            return Self::canonical_function_type_annotation_text(arena, idx);
        }
        Self::declared_annotation_leaf_text(arena, idx)
    }

    /// Rebuild the canonical source spelling of a `FUNCTION_TYPE` /
    /// `CONSTRUCTOR_TYPE` annotation from its parsed structure instead of
    /// echoing the raw source substring.
    ///
    /// `tsc`'s printer always emits canonical signature spacing (`() => 1`,
    /// `(x: 1) => void`) regardless of how the user wrote it. The declared-
    /// annotation echo path this feeds (`annotation_is_canonicalized_structural_type`
    /// declining canonicalization when a literal is present, to avoid
    /// re-materializing a possibly-widened `TypeId`) otherwise leaks the
    /// exact written spacing into the diagnostic, so a badly-spaced source
    /// (`()=>1`, `(x:1)=>void`) renders unspaced. Rebuilding the skeleton
    /// (`new`, generics, parens, `:`, `,`, `=>`) canonically while taking
    /// each parameter/return type's own text verbatim from source keeps a
    /// written literal untouched, matching tsc's output for both
    /// canonically- and badly-spaced source.
    pub(in crate::error_reporter) fn canonical_function_type_annotation_text(
        arena: &tsz_parser::NodeArena,
        node_idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(node_idx)?;
        let func = arena.get_function_type(node)?;
        let mut out = String::new();
        if func.is_abstract {
            out.push_str("abstract ");
        }
        if node.kind == syntax_kind_ext::CONSTRUCTOR_TYPE {
            out.push_str("new ");
        }
        if let Some(type_params) = &func.type_parameters {
            out.push('<');
            for (i, &tp_idx) in type_params.nodes.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&Self::declared_annotation_leaf_text(arena, tp_idx)?);
            }
            out.push('>');
        }
        out.push('(');
        for (i, &param_idx) in func.parameters.nodes.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            let param_node = arena.get(param_idx)?;
            let param = arena.get_parameter(param_node)?;
            if param.dot_dot_dot_token {
                out.push_str("...");
            }
            out.push_str(&Self::declared_annotation_leaf_text(arena, param.name)?);
            if param.question_token {
                out.push('?');
            }
            if param.type_annotation.is_some() {
                out.push_str(": ");
                out.push_str(&Self::declared_annotation_type_text(
                    arena,
                    param.type_annotation,
                )?);
            }
        }
        out.push_str(") => ");
        out.push_str(&Self::declared_annotation_type_text(
            arena,
            func.type_annotation,
        )?);
        Some(out)
    }

    /// Declared-annotation source text for `annotation_idx`, taking the
    /// canonical `FUNCTION_TYPE`/`CONSTRUCTOR_TYPE` skeleton reconstruction
    /// over the raw substring when applicable. Shared by every call site that
    /// currently reads the annotation node's source text verbatim, so a
    /// badly-spaced signature normalizes everywhere the raw-echo fallback
    /// applies rather than only at one call site.
    pub(in crate::error_reporter) fn declared_annotation_source_text(
        arena: &tsz_parser::NodeArena,
        annotation_idx: NodeIndex,
    ) -> Option<String> {
        Self::declared_annotation_type_text(arena, annotation_idx)
    }
}

#[cfg(test)]
mod normalize_whitespace_tests {
    use super::normalize_declared_annotation_whitespace as norm;

    #[test]
    fn collapses_runs_outside_literals() {
        assert_eq!(norm("P   &   Q"), "P & Q");
        assert_eq!(norm("A   |   B"), "A | B");
        assert_eq!(norm("readonly   number[]"), "readonly number[]");
    }

    #[test]
    fn collapses_tabs_and_trims_but_keeps_newlines() {
        assert_eq!(norm("\tP\t&\tQ\t"), "P & Q");
        assert_eq!(norm("  P & Q  "), "P & Q");
        // A line break is preserved verbatim: the downstream sanitizer's
        // first-newline guard depends on it to reject multi-line annotations.
        assert_eq!(norm("P &\n  Q"), "P &\n Q");
        assert!(norm("A & C & {\n  f0: F0;\n}").contains('\n'));
    }

    #[test]
    fn already_canonical_is_idempotent() {
        assert_eq!(norm("P & Q"), "P & Q");
        assert_eq!(norm("{ a: number; b: string }"), "{ a: number; b: string }");
    }

    #[test]
    fn preserves_string_literal_interior() {
        // A string-literal type's own spelling is never re-spaced.
        assert_eq!(norm(r#""a  b" | "c""#), r#""a  b" | "c""#);
        assert_eq!(norm("'x   y'  &  Q"), "'x   y' & Q");
        // An escaped closing quote does not end the literal early.
        assert_eq!(norm(r#""a\"  b""#), r#""a\"  b""#);
    }

    #[test]
    fn preserves_template_literal_interior() {
        assert_eq!(norm("`a  ${T}` | X"), "`a  ${T}` | X");
    }
}
