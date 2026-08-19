//! Assignment-source/target type formatting for TS2322-family diagnostics.
//!
//! Extracted from `error_reporter/core/diagnostic_source.rs` to keep that
//! file under the LOC ceiling. Pure file-organization move; no logic changes.

use super::literal_widening_helpers::literal_display_appropriate_for_undefined_null_target;
use crate::query_boundaries::diagnostics as diagnostic_query;
use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// When the diagnostic source expression at `anchor_idx` is a plain
    /// `expr as T` / `<T>expr` assertion (not `as const`, not `satisfies`),
    /// return the asserted type formatted with its literal element / property
    /// types preserved.
    ///
    /// The value of such an assertion is the asserted type `T` exactly as
    /// written, which `tsc` treats as a *regular* (non-fresh) type. The general
    /// source-display fallbacks reach the inner literal expression node (the
    /// widening machinery peels assertions via `skip_parenthesized_and_assertions`)
    /// and re-widen its element/property literals as though they came from a
    /// fresh literal expression, collapsing `[1, 2, 3] as [1, 2, 3]` to
    /// `[number, number, number]`. Intercepting before that peel keeps the
    /// literals, matching `tsc`. Returns `None` for non-assertion sources so the
    /// caller proceeds with its normal display path (fresh literal expressions
    /// still widen). Shared by the two parallel TS2322 source-display pipelines
    /// (`format_assignment_source_type_for_diagnostic` and
    /// `format_top_level_assignability_message_types_at`) so they cannot diverge.
    pub(in crate::error_reporter) fn assertion_source_literal_display(
        &mut self,
        anchor_idx: NodeIndex,
        source: TypeId,
        target: TypeId,
    ) -> Option<String> {
        let expr_idx = self
            .direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx))?;
        self.expression_is_plain_type_assertion(expr_idx)
            .then(|| self.format_assignability_type_for_message(source, target))
    }

    /// True when an assignment/return diagnostic source is an identifier whose
    /// declared type is *explicitly* `unknown` or `any`, yet whose checked
    /// source type (`source`) has flow-narrowed to a more specific type — the
    /// canonical case being a user-defined `x is T` type-predicate guard over
    /// an `unknown`/`any` operand.
    ///
    /// `tsc` renders the narrowed checked type for such a source; tsz's
    /// declared-annotation / node-derived fallbacks below would otherwise
    /// repaint it with the stale declared top type (the source identifier's
    /// declared type, not the checked source type, drives the annotation text).
    /// Gated on the *declared symbol* type being a top type, on the checked
    /// source differing from it (i.e. narrowing actually occurred), and on the
    /// presence of an explicit annotation — so an implicit / control-flow `any`
    /// (`let x;`) keeps its existing display, which a separate path owns, and
    /// an un-narrowed `unknown`/`any` source still renders as declared. The
    /// decision is structural (declared top type vs. a different checked
    /// source), never keyed on rendered text or identifier name.
    pub(in crate::error_reporter) fn assignment_source_narrowed_from_declared_top_type(
        &mut self,
        anchor_idx: NodeIndex,
        source: TypeId,
    ) -> bool {
        if matches!(source, TypeId::ERROR) {
            return false;
        }
        let Some(expr_idx) = self.assignment_source_expression(anchor_idx) else {
            return false;
        };
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        if self.ctx.arena.get(expr_idx).map(|node| node.kind)
            != Some(tsz_scanner::SyntaxKind::Identifier as u16)
        {
            return false;
        }
        // Require an explicit `: unknown` / `: any` annotation on the
        // declaration; this excludes implicit / control-flow `any`.
        if self
            .declared_diagnostic_source_annotation_text(expr_idx)
            .is_none()
        {
            return false;
        }
        let Some(sym_id) = self.resolve_identifier_symbol(expr_idx) else {
            return false;
        };
        let declared = self.get_type_of_symbol(sym_id);
        matches!(declared, TypeId::UNKNOWN | TypeId::ANY) && declared != source
    }

    pub(in crate::error_reporter) fn format_assignment_source_type_for_diagnostic(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> String {
        // A fresh object-literal property whose value is a string/number/bigint
        // literal widens to its primitive when the target property type does not
        // admit that literal's domain — `{ configurable: "yes" }` against
        // `{ configurable?: boolean }` renders `string`, not `"yes"`; `{ f: "yes" }`
        // against `{ f?: 1 | 2 }` renders `string`. This mirrors tsc's
        // `isLiteralOfContextualType` widening of object-literal property types,
        // which the solver already applies in the failure reason (it carries
        // `string`) but the anchor-literal display branches below re-read from the
        // property value expression and un-widen. The rule is scoped to
        // object-literal property initializers: a plain assignment
        // (`let x: 1 | 2; x = "yes"`) instead follows `typeCouldHaveTopLevelSingletonTypes`
        // (the domain-agnostic literal-sensitivity gate below), which correctly
        // preserves the source there. Boolean literal sources are excluded — tsc
        // keeps `true` / `false` verbatim.
        if let Some(expr_idx) = self
            .direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx))
            && self.is_property_assignment_initializer(expr_idx)
            && let Some(anchor_literal) = self.literal_type_from_initializer(expr_idx)
            && self.scalar_source_widens_across_literal_domain(anchor_literal, target)
        {
            let widened =
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, anchor_literal);
            return self.format_assignability_type_for_message(widened, target);
        }
        // A source identifier declared `unknown`/`any` but flow-narrowed to a
        // concrete type must render the narrowed source, not the stale top-type
        // annotation.
        if let Some(expr_idx) = self
            .direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx))
            && self.source_identifier_narrowed_from_unknown_or_any(expr_idx, source)
        {
            return self.format_type_for_assignability_message(source);
        }
        // A source identifier flow-narrowed to a strict subset of its declared
        // union renders the narrowed checked type. `tsc` drops the `aliasSymbol`
        // for proper-subset flow results, so avoid repainting with the stale
        // declared alias/annotation below.
        if let Some(expr_idx) = self
            .direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx))
            && let Some(declared_type) = self.declared_type_of_variable_identifier_source(expr_idx)
            && self.source_flow_type_strictly_narrows_declared(source, declared_type)
        {
            return self.format_assignability_type_for_message(source, target);
        }
        // An inline / anonymous composite source annotation (`declare const s:
        // { a: number }; const t: string = s`) carries no `aliasSymbol`, so tsc
        // renders the structural shape rather than a coincidentally-shaped alias
        // name reached through the reverse type-to-def lookup. Suppress that
        // repaint for such annotations (the flow-narrowing guards above already
        // claimed the cases where the displayed source is a narrowed subset).
        if let Some(display) =
            self.anonymous_composite_annotation_source_display(anchor_idx, source)
        {
            return display;
        }
        // An inline tuple / function / constructor source annotation
        // (`[number, string]`, `(a: number) => void`, `new () => T`) likewise
        // carries no `aliasSymbol`, so tsc renders its expanded structural form
        // rather than repainting it with a coincidentally-shaped alias name
        // reached through the reverse type-to-def lookup (#17119). A
        // written-through alias reference (`: Fn`) is a `TYPE_REFERENCE`, not an
        // inline structural type, and is left to the established path.
        if let Some(display) =
            self.inline_structural_type_annotation_source_display(anchor_idx, source)
        {
            return display;
        }
        // A longhand primitive-keyword union source annotation
        // (`string | number | symbol`) likewise carries no `aliasSymbol`; render
        // it by its members instead of a coincidentally-shaped alias (#16610).
        if let Some(display) = self.longhand_primitive_union_source_display(anchor_idx, source) {
            return display;
        }
        // A `keyof any` / `keyof unknown` / `keyof never` source annotation
        // resolves to its fixed key-space result at type-construction time in
        // tsc (`getIndexType`), so it likewise carries no `aliasSymbol` — same
        // family as the longhand-union case above, different written spelling.
        if let Some(display) = self.keyof_degenerate_operand_source_display(anchor_idx, source) {
            return display;
        }
        // A deferred meta-type source — a bare conditional (`T extends U ? X : Y`)
        // or indexed-access (`T["x"]`), or an `Application` of a conditional/
        // indexed-bodied alias (`T95<U>`) — that still carries free type
        // parameters keeps its written spelling in tsc's assignability
        // diagnostics: the relation compares the apparent (branch-union /
        // constraint) form, but the displayed source type is the original. This
        // mirrors the target-side conditional and indexed-access guards in
        // `format_assignment_target_type_for_diagnostic`; without it the widening
        // fallbacks below collapse the source to its apparent form (e.g. `T95<U>`
        // rendered as `number | boolean`, `T["x"]` as `string | undefined`).
        // Scoped to a generic target: tsc only keeps the source's written
        // spelling when the target is itself generic (a deferred conditional or
        // type-parameter-bearing type, e.g. `T95<U>` vs `T94<U>`, `T["x"]` vs
        // `NonNullable<T["x"]>`). Against a *concrete* target tsc instead shows
        // the source's apparent constraint — `IsArray<T>` rendered as `boolean`
        // for `let t: true = x`, or `ReturnType<T[M]>` left to the established
        // application path for `x: A` — both already handled by the fallbacks
        // below, so this guard must not intercept them.
        if crate::query_boundaries::diagnostics::generic_deferred_source_keeps_spelling_against_generic_target(
            self.ctx.types,
            &self.ctx.definition_store,
            source,
            target,
        )
        {
            return self.format_type_for_assignability_message(source);
        }
        // The mirror case: a still-generic deferred conditional/indexed-access
        // source against a *concrete* target expands to its branch union
        // (`F<T> = T extends number ? string : boolean` against `number`
        // renders `string | boolean`, not `F<T>`) — tsc's apparent-type display
        // for a deferred conditional whose result is otherwise fully concrete.
        // Only fires when the guard above did not (i.e. the target is not
        // itself generic/deferred); see `deferred_conditional_source_branch_union_display`
        // for the concreteness/bare-check-param safety conditions.
        if let Some(display) = self.deferred_conditional_source_branch_union_display(source, target)
        {
            return display;
        }
        // For property-access source expressions whose underlying value type is
        // a `unique symbol` (e.g. `Symbol.toPrimitive`), tsc displays the source
        // as `typeof <expr>` rather than widening to `symbol`. Match that here
        // before any widening below collapses the source to its primitive.
        if let Some(display) = self.typeof_unique_symbol_source_display(anchor_idx) {
            return display;
        }
        if let Some(display) =
            self.js_constructor_instance_assignment_source_display(source, anchor_idx)
        {
            return display;
        }

        // Preserve the as-written reference for a generic interface/class source
        // (`O<T>`, `OwnerList<T>`) before the widening / structural-display
        // fallbacks below collapse it and re-derive an over-instantiated type
        // argument from the instantiated members. See
        // `generic_nominal_application_source_display` for the full rationale.
        if let Some(display) = self.generic_nominal_application_source_display(source) {
            return display;
        }

        // A generic application of a conditional/indexed-access-bodied type alias
        // (`Classify<"x">`, `Head<[a, b]>`, `Val<{…}>`, including through an alias
        // chain) drops tsc's `aliasSymbol` once it reduces to a concrete shape, so
        // tsc prints the resolved structural form (`{ s: "x"; }`, `[a, b]`, …).
        // The identifier-source fallbacks below otherwise repaint the source with
        // the declared annotation text and leak the `Classify<"x">` surface. The
        // shared reduction policy keeps the alias for free-type-parameter, stalled,
        // and mapped-bodied applications, and defers a non-generic alias wrapping
        // such an application (`type RO = DeepReadonly<C>`) to the established
        // computed-body path, matching tsc.
        if let Some(display) = self.reduced_generic_application_source_display(source) {
            return display;
        }

        // A source whose declared annotation is an inline call/construct
        // signature (`declare const v: () => 1`, `new () => 1`) renders through
        // the canonical structural formatter so the author's whitespace is
        // normalized (`()=>1` -> `() => 1`, `(x:1)=>void` -> `(x: 1) => void`),
        // while a literal written in the signature stays verbatim: tsc widens
        // only fresh literals, and a declared signature is non-fresh. See
        // `inline_signature_annotation_source_display`.
        if let Some(display) =
            self.inline_signature_annotation_source_display(anchor_idx, source, target)
        {
            return display;
        }

        let has_optional_callable_param =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, source)
                .is_some_and(|shape| shape.params.iter().any(|param| param.optional))
                || crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, source)
                    .is_some_and(|shape| {
                        shape
                            .call_signatures
                            .iter()
                            .chain(shape.construct_signatures.iter())
                            .any(|sig| sig.params.iter().any(|param| param.optional))
                    });
        if has_optional_callable_param {
            return self.format_assignability_type_for_message(source, target);
        }

        // Preserve the literal surface of a plain `as T` / `<T>` assertion source
        // before the widening fallbacks below; see
        // `assertion_source_literal_display`.
        if let Some(display) = self.assertion_source_literal_display(anchor_idx, source, target) {
            return display;
        }

        // A `keyof <operand>` source reduces to its key set in tsc diagnostics:
        // the literal members are preserved against a literal-sensitive target
        // (`"a" | "b"`, `2 | 1`) and the `keyof Name` spelling is kept for a named
        // operand. tsz otherwise leaks an unreduced `keyof { … }`, a widened
        // `string`, or the alias name depending on how the source was built;
        // normalize all three before the generic widening fallbacks below.
        if let Some(display) = self.keyof_source_assignment_display(source, target) {
            return display;
        }

        if let Some(expr_idx) = self.tuple_display_source_expression(anchor_idx)
            && let Some(display) =
                self.array_literal_tuple_source_type_display(expr_idx, source, target)
        {
            return display;
        }

        if let Some(display) = self.tuple_structural_source_display(source, target) {
            return display;
        }

        if source == TypeId::UNDEFINED
            && self.ctx.arena.get(anchor_idx).is_some_and(|node| {
                node.kind == tsz_parser::parser::syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
            })
        {
            return self.format_assignability_type_for_message(source, target);
        }

        // Generic intersection source reduction: when the source is an intersection
        // containing type parameters (e.g., `T & U`), tsc displays the reduced base
        // constraint instead of the raw generic intersection.  For example,
        // `T extends string | number | undefined` and `U extends string | null | undefined`
        // display as `string | undefined` rather than `T & U`.
        //
        // This matches tsc's `getBaseConstraintOfType` behavior for intersection types
        // in error messages.
        if let Some(reduced) = self.generic_intersection_source_display_substitution(source) {
            return self.format_type_for_assignability_message(reduced);
        }

        // For Lazy(DefId) source types representing named interfaces (non-generic),
        // return the interface name directly. This prevents get_type_of_node from
        // resolving the Lazy to its structural form, losing the name (e.g., showing
        // "{ constraint: Constraint<this>; ... }" instead of "Num").
        if let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, source)
            && let Some(def) = self.ctx.definition_store.get(def_id)
            && def.kind == tsz_solver::def::DefKind::Interface
            && def.type_params.is_empty()
        {
            let name = self.ctx.types.resolve_atom_ref(def.name);
            return name.to_string();
        }

        // A source identifier explicitly declared `unknown`/`any` that
        // flow-narrowed to a more specific checked type (the canonical case
        // being a user-defined `x is T` predicate guard over an `unknown`/`any`
        // operand) renders the narrowed checked type, exactly as tsc's
        // `typeToString(sourceType)` does. Resolve it here, before the
        // declared-annotation / widening fallbacks below repaint the source
        // with the stale declared top type.
        if self.assignment_source_narrowed_from_declared_top_type(anchor_idx, source) {
            return self.format_assignability_type_for_message(source, target);
        }

        if let Some(display) = self.jsdoc_annotated_expression_display(anchor_idx, target) {
            return display;
        }

        if crate::query_boundaries::common::literal_value(self.ctx.types, source).is_some()
            && crate::query_boundaries::common::string_intrinsic_components(self.ctx.types, target)
                .is_some_and(|(_, type_arg)| type_arg == TypeId::STRING)
        {
            let widened = self.widen_type_for_display(source);
            return self.format_assignability_type_for_message(widened, target);
        }
        // A `keyof` target only widens the source literal in the message when the
        // key set has no literal context. A concrete `keyof R` (operand is a plain
        // object type) reduces to a finite unit-literal key set (`"a" | "b"`,
        // `unique symbol | "str"`) that provides a literal context, so tsc renders
        // the source literal as-written. A generic/deferred `keyof T` (operand is a
        // mapped/computed type whose keys cannot be statically enumerated) has the
        // apparent key universe `string | number | symbol` and no literal context,
        // so tsc widens the source to its primitive base. Gate the keyof widening
        // paths on the operand NOT being a concrete object so concrete key unions
        // keep the source literal spelling.
        let keyof_target_widens_source = !self.keyof_target_has_concrete_object_operand(target);
        if keyof_target_widens_source
            && crate::query_boundaries::common::literal_value(self.ctx.types, source).is_some()
            && self.keyof_type_alias_body_display(target).is_some()
        {
            let widened = self.widen_type_for_display(source);
            return self.format_assignability_type_for_message(widened, target);
        }
        if keyof_target_widens_source
            && let Some(target_expr) = self.assignment_target_expression(anchor_idx)
            && self
                .keyof_type_alias_annotation_display_for_expression(target_expr)
                .is_some()
        {
            let widened = self.widen_type_for_display(source);
            return self.format_assignability_type_for_message(widened, target);
        }
        if keyof_target_widens_source
            && let Some(annotation) = self.direct_assignment_target_annotation_text(anchor_idx)
            && self
                .keyof_type_alias_annotation_display(&annotation)
                .is_some()
        {
            let widened = self.widen_type_for_display(source);
            return self.format_assignability_type_for_message(widened, target);
        }

        if let Some(expr_idx) = self.tuple_display_source_expression(anchor_idx)
            && let Some(display) =
                self.array_literal_tuple_source_type_display(expr_idx, source, target)
        {
            return display;
        }

        let in_arith_compound = self.in_arithmetic_compound_assignment_context(anchor_idx);
        if let Some(display) = self.literal_assignment_source_display_for_target(target, anchor_idx)
        {
            return display;
        }

        if let Some(display) = self.preferred_evaluated_source_display(source, target) {
            return display;
        }

        if let Some(display_type) = self.string_covered_template_union_source_display(source) {
            return self.format_assignability_type_for_message(display_type, target);
        }

        if let Some(display) = self.related_generic_indexed_access_source_display(source, target) {
            return display;
        }

        if !in_arith_compound
            && self.array_literal_element_source_widening_required_for_display(
                anchor_idx, source, target,
            )
        {
            let widened = self.widen_type_for_display(source);
            return self.format_assignability_type_for_message(widened, target);
        }

        if !in_arith_compound
            && self.is_literal_sensitive_assignment_target(target)
            && let Some(display) = self.literal_expression_display(anchor_idx)
            && literal_display_appropriate_for_undefined_null_target(
                self.ctx.types,
                target,
                &display,
            )
        {
            return display;
        }
        if !in_arith_compound
            && self.is_literal_sensitive_assignment_target(target)
            && crate::query_boundaries::common::literal_value(self.ctx.types, source).is_some()
        {
            return self.format_assignability_type_for_message(source, target);
        }

        if self.is_object_rest_assignment_target_anchor(anchor_idx) {
            return self.format_type_for_assignability_message(source);
        }

        if let Some(expr_idx) = self.direct_diagnostic_source_expression(anchor_idx) {
            if !in_arith_compound
                && self.is_literal_sensitive_assignment_target(target)
                && let Some(display) = self.literal_expression_display(expr_idx)
                && literal_display_appropriate_for_undefined_null_target(
                    self.ctx.types,
                    target,
                    &display,
                )
            {
                return display;
            }

            if let Some(display) = self.empty_array_literal_source_type_display(expr_idx) {
                return display;
            }

            if let Some(display) =
                self.array_literal_tuple_source_type_display(expr_idx, source, target)
            {
                return display;
            }

            if let Some(display) = self.object_literal_source_type_display(expr_idx, Some(target)) {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            if source != TypeId::UNKNOWN
                && (expr_type == TypeId::UNKNOWN || expr_type == source)
                && crate::query_boundaries::common::is_empty_object_type(self.ctx.types, source)
            {
                return self.format_assignability_type_for_message(source, target);
            }
            let expr_display_type = if expr_type == TypeId::UNKNOWN && source != TypeId::UNKNOWN {
                source
            } else {
                expr_type
            };
            if self.should_preserve_nuia_source_undefined_display(
                source,
                target,
                expr_idx,
                expr_display_type,
            ) {
                return self.format_type_for_assignability_message(source);
            }
            let node_is_array_of_source = crate::query_boundaries::common::array_element_type(
                self.ctx.types,
                expr_display_type,
            )
            .is_some_and(|elem| elem == source);
            if node_is_array_of_source {
                return self.format_assignability_type_for_message(source, target);
            }
            let node_is_target_not_source =
                expr_display_type == target && expr_display_type != source;
            let node_type_matches_source =
                expr_display_type != TypeId::ERROR && !node_is_target_not_source;
            if node_type_matches_source {
                if !in_arith_compound
                    && crate::query_boundaries::common::is_template_literal_type(
                        self.ctx.types,
                        target,
                    )
                    && let Some(display) = self.literal_expression_display(expr_idx)
                    && literal_display_appropriate_for_undefined_null_target(
                        self.ctx.types,
                        target,
                        &display,
                    )
                {
                    return display;
                }
                let preserve_literal_surface = self.target_preserves_literal_surface(target);
                if let Some(annotation_text) =
                    self.declared_diagnostic_source_annotation_text(expr_idx)
                    && self.should_prefer_declared_source_annotation_display(
                        expr_idx,
                        expr_display_type,
                        &annotation_text,
                    )
                {
                    if let Some(display) =
                        self.declared_intersection_annotation_display_for_expression(expr_idx)
                    {
                        return display;
                    }
                    return self.format_declared_annotation_for_diagnostic(&annotation_text);
                }
                let display_type = self
                    .widened_enum_member_assignment_source(expr_display_type, target)
                    .unwrap_or(expr_display_type);
                let display_type = self.widen_function_like_display_type(display_type);
                let display_type = if self.is_literal_sensitive_assignment_target(target)
                    || preserve_literal_surface
                {
                    display_type
                } else if crate::query_boundaries::common::keyof_inner_type(
                    self.ctx.types,
                    display_type,
                )
                .is_some()
                {
                    let evaluated = self.evaluate_type_for_assignability(display_type);
                    crate::query_boundaries::common::widen_type(self.ctx.types, evaluated)
                } else {
                    crate::query_boundaries::common::widen_type(self.ctx.types, display_type)
                };
                if let Some(display) =
                    self.new_expression_nominal_source_display(expr_idx, display_type)
                {
                    return display;
                }
                if crate::query_boundaries::common::array_element_type(self.ctx.types, display_type)
                    == Some(TypeId::UNKNOWN)
                    && let Some(display) = self.call_unknown_array_source_display(expr_idx, target)
                {
                    return display;
                }
                if let Some(display) =
                    self.declared_identifier_source_display(expr_idx, target, expr_display_type)
                {
                    return display;
                }
                if let Some(display) =
                    self.direct_type_query_primitive_source_display(expr_idx, display_type)
                {
                    return display;
                }
                if let Some(display) = self.rebuilt_array_source_display(display_type, target) {
                    return display;
                }
                // When widening rebuilt the type into a structurally-equivalent but
                // distinct `TypeId`, the new id does not carry the original
                // `TypeAlias` registration (`find_def_for_type`). The diagnostic
                // formatter relies on that registration to render the alias name
                // (`SimpleType`) instead of the expanded body
                // (`string | Promise<SimpleType>`). When the original is a
                // registered `TypeAlias`, format the original `TypeId` so the
                // printer recovers the alias name.
                let formatting_type = if display_type != expr_display_type
                    && self.is_registered_type_alias_for_display(expr_display_type)
                {
                    expr_display_type
                } else {
                    display_type
                };
                return self.format_assignability_type_for_message(formatting_type, target);
            }

            if node_type_matches_source
                && let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx)
            {
                if let Some(intersection_display) =
                    self.declared_intersection_annotation_display_for_expression(expr_idx)
                {
                    return intersection_display;
                }
                return display;
            }
        }
        if let Some(expr_idx) = self.tuple_display_source_expression(anchor_idx) {
            if let Some(display) = self.type_assertion_mapped_alias_source_display(expr_idx) {
                return display;
            }
            if let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx)
                && display.contains("=>")
            {
                return self.format_annotation_like_type(&display);
            }
            if let Some(display) = self.literal_expression_display(expr_idx)
                && !self.in_arithmetic_compound_assignment_context(anchor_idx)
                && (self.is_literal_sensitive_assignment_target(target)
                    || (self.assignment_source_is_return_expression(anchor_idx)
                        && crate::query_boundaries::common::contains_type_parameters(
                            self.ctx.types,
                            target,
                        )
                        && !self.is_property_assignment_initializer(expr_idx)
                        // When the target is a bare type parameter (e.g. T),
                        // tsc widens literals in error messages: "Type 'string'
                        // is not assignable to type 'T'" rather than "Type '\"\"'
                        // is not assignable to type 'T'". Preserve literals only
                        // for complex generic targets like indexed access types.
                        && !self.target_is_bare_type_parameter(target)))
                // For pre-widened property-elaboration sources, mirror tsc's
                // `getWidenedLiteralLikeTypeForContextualType`: only resurrect
                // the AST literal display when the source's primitive kind has
                // a matching literal kind in the target. Cross-primitive cases
                // (e.g. numeric literal `1` against boolean literal `true`)
                // widen the source so the diagnostic shows
                // `Type 'number' is not assignable to type 'true'.` instead of
                // `Type '1' ...`. Direct same-primitive mismatches like
                // `"bar"` vs `"foo"` keep the literal display.
                && !self.property_elaboration_widening_required_for_display(
                    expr_idx, source, target,
                )
                && literal_display_appropriate_for_undefined_null_target(
                    self.ctx.types,
                    target,
                    &display,
                )
            {
                return display;
            }

            if let Some(display) = self.empty_array_literal_source_type_display(expr_idx) {
                return display;
            }

            if let Some(display) =
                self.array_literal_tuple_source_type_display(expr_idx, source, target)
            {
                return display;
            }

            if let Some(display) = self.object_literal_source_type_display(expr_idx, Some(target)) {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            if source != TypeId::UNKNOWN
                && (expr_type == TypeId::UNKNOWN || expr_type == source)
                && crate::query_boundaries::common::is_empty_object_type(self.ctx.types, source)
            {
                return self.format_assignability_type_for_message(source, target);
            }
            let expr_display_type = if expr_type == TypeId::UNKNOWN && source != TypeId::UNKNOWN {
                source
            } else {
                expr_type
            };
            if self.should_preserve_nuia_source_undefined_display(
                source,
                target,
                expr_idx,
                expr_display_type,
            ) {
                return self.format_type_for_assignability_message(source);
            }
            let preserve_literal_surface = self.target_preserves_literal_surface(target);
            // NOTE: an enum-ish source is never repainted from its annotation
            // text. tsc renders enum types and members through `typeToString`,
            // which neither namespace-qualifies (`P.Q.S` prints `Q.S`) nor
            // shows an alias spelling (`type MA = Mode.A` prints `Mode.A`);
            // `should_prefer_declared_source_annotation_display` below refuses
            // enum-ish sources for the same reason.
            if expr_type != TypeId::ERROR
                && let Some(annotation_text) =
                    self.declared_diagnostic_source_annotation_text(expr_idx)
                && self.should_prefer_declared_source_annotation_display(
                    expr_idx,
                    expr_display_type,
                    &annotation_text,
                )
            {
                if let Some(display) =
                    self.declared_intersection_annotation_display_for_expression(expr_idx)
                {
                    return display;
                }
                return self.format_declared_annotation_for_diagnostic(&annotation_text);
            }
            // A source that is itself a deferred constraint-relative operand
            // (`Bag[KSel]`, bare `keyof T`, a distributive conditional) keeps
            // its written spelling here too — `widen_type_for_display` has no
            // dedicated `IndexAccess`/`KeyOf` case and otherwise falls through
            // to `judge`/relation machinery downstream that resolves it against
            // its constraint, collapsing e.g. `Bag[KSel]` to `number` before
            // the identity ever reaches display. This is the expression-typed
            // sibling of `generic_deferred_source_keeps_spelling_against_generic_target`
            // above: that guard only preserves the spelling when the TARGET is
            // also generic, but tsc keeps a deferred constraint-relative
            // SOURCE's own spelling regardless of whether the target is
            // concrete (oracle-verified via `scripts/conformance/oracle.sh` vs
            // pinned typescript@7.0.2, #17718 witness 2 family: `Type
            // 'Bag[KSel]' is not assignable to type 'string | undefined'.`).
            let preserve_deferred_surface =
                crate::query_boundaries::shape_predicates::is_deferred_constraint_relative_operand(
                    self.ctx.types.as_type_database(),
                    &self.ctx.definition_store,
                    expr_display_type,
                );
            let display_type = if expr_display_type != TypeId::ERROR {
                let widened_expr_type = if preserve_literal_surface || preserve_deferred_surface {
                    expr_display_type
                } else {
                    self.widen_type_for_display(expr_display_type)
                };
                self.widened_enum_member_assignment_source(widened_expr_type, target)
                    .unwrap_or(widened_expr_type)
            } else {
                self.widen_type_for_display(source)
            };
            // `widen_function_like_display_type` unconditionally evaluates its
            // input before checking whether it is even function-like — for a
            // deferred constraint-relative operand that eagerly resolves
            // `Bag[KSel]` to `number`, undoing the preservation above. Skip it
            // for the same operands `preserve_deferred_surface` already
            // protects; a deferred `IndexAccess`/`KeyOf`/`Conditional` is never
            // itself function-like, so there is nothing for this widening step
            // to do for it anyway.
            let display_type = if preserve_deferred_surface {
                display_type
            } else {
                self.widen_function_like_display_type(display_type)
            };
            if let Some(display) =
                self.new_expression_nominal_source_display(expr_idx, display_type)
            {
                return display;
            }
            if crate::query_boundaries::common::array_element_type(self.ctx.types, display_type)
                == Some(TypeId::UNKNOWN)
                && let Some(display) = self.call_unknown_array_source_display(expr_idx, target)
            {
                return display;
            }
            if let Some(display) =
                self.declared_identifier_source_display(expr_idx, target, expr_display_type)
            {
                return display;
            }
            if let Some(display) =
                self.direct_type_query_primitive_source_display(expr_idx, display_type)
            {
                return display;
            }
            if let Some(display) = self.rebuilt_array_source_display(display_type, target) {
                return display;
            }

            if let Some(sym_id) = self.resolve_identifier_symbol(expr_idx)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && symbol.has_any_flags(tsz_binder::symbol_flags::ENUM)
                && !symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER)
            {
                return self.format_assignability_type_for_message(display_type, target);
            }

            if expr_type == TypeId::ERROR
                && let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx)
            {
                if let Some(intersection_display) =
                    self.declared_intersection_annotation_display_for_expression(expr_idx)
                {
                    return intersection_display;
                }
                return display;
            }

            let display_type =
                if crate::query_boundaries::common::keyof_inner_type(self.ctx.types, display_type)
                    .is_some()
                {
                    let evaluated = self.evaluate_type_for_assignability(display_type);
                    crate::query_boundaries::common::widen_type(self.ctx.types, evaluated)
                } else {
                    display_type
                };
            let source_enum_symbol = self.enum_symbol_from_enumish_type(display_type);
            let target_enum_symbol = self.enum_symbol_from_enumish_type(target);
            if source_enum_symbol.is_some()
                && target_enum_symbol.is_some()
                && source_enum_symbol != target_enum_symbol
            {
                return self.format_assignability_type_for_message(display_type, target);
            }
            let formatted = self.format_type_for_assignability_message(display_type);
            let resolved_for_access = self.resolve_type_for_property_access(display_type);
            let resolved = self.judge_evaluate(resolved_for_access);
            if !formatted.contains('{')
                && !formatted.contains('[')
                && !formatted.contains('|')
                && !formatted.contains('&')
                && !formatted.contains('<')
                && !crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    display_type,
                )
                && crate::query_boundaries::index_signature::has_string_or_number_index_signature(
                    self.ctx.types,
                    resolved,
                )
            {
                if let Some(structural) = self.format_structural_indexed_object_type(resolved) {
                    return structural;
                }
                return self.format_type(resolved);
            }
            // For generic type aliases whose conditional body is ambiguous
            // (e.g. `IsArray<T>` where T extends `object`), skip annotation text.
            let eval_for_ambiguous = self.evaluate_type_for_assignability(display_type);
            let is_ambiguous_conditional_alias = self
                .compute_ambiguous_conditional_display(eval_for_ambiguous)
                .is_some();
            if let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx)
                && self.should_prefer_declared_source_annotation_display(
                    expr_idx,
                    expr_display_type,
                    &display,
                )
                && !is_ambiguous_conditional_alias
                && !display.starts_with("keyof ")
                && !display.starts_with("typeof ")
                && !Self::display_contains_mapped_clause(&display)
                // Don't use annotation text for union types — the TypeFormatter
                // reorders null/undefined to the end to match tsc's display.
                // Annotation text preserves the user's original order which
                // differs from tsc's canonical display.
                && (!display.contains(" | ")
                    || Self::display_has_member_literals_assignability(&display))
                // Don't use annotation text when the formatted type includes
                // `| undefined` (added by strictNullChecks for optional params)
                // that the raw annotation text doesn't have. The annotation text
                // reflects the source code literally and misses the semantic
                // `| undefined` injection.
                && (!formatted.contains("| undefined") || display.contains("| undefined"))
                // Don't use annotation text for string intrinsic types when it
                // differs from the formatted type. tsc collapses idempotent
                // nesting (e.g. Uppercase<Uppercase<string>> → Uppercase<string>)
                // at type creation time, so the annotation text may be stale.
                && (!crate::query_boundaries::common::is_string_intrinsic_type(
                    self.ctx.types,
                    display_type,
                ) || display.trim() == formatted)
            {
                if let Some(intersection_display) =
                    self.declared_intersection_annotation_display_for_expression(expr_idx)
                {
                    return intersection_display;
                }
                return self.format_annotation_like_type(&display);
            }
            if let Some(display) =
                self.direct_type_query_primitive_source_display(expr_idx, display_type)
            {
                return display;
            }
            return formatted;
        }

        // Check if source is a single-call-signature callable that tsc displays in
        // arrow syntax. For these, use the TypeFormatter instead of annotation text.
        let source_uses_arrow_syntax =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, source)
                .is_some_and(|shape| {
                    shape.call_signatures.len() == 1
                        && shape.construct_signatures.is_empty()
                        && shape.properties.is_empty()
                        && shape.string_index.is_none()
                        && shape.number_index.is_none()
                });
        if !source_uses_arrow_syntax {
            if let Some(annotation_text) =
                self.declared_type_annotation_text_for_symbol_type(source, true)
            {
                let display = self.format_declared_annotation_for_diagnostic(&annotation_text);
                return self.canonicalize_assignment_numeric_literal_union_display(
                    source, target, display,
                );
            }
            let evaluated_source = self.evaluate_type_with_env(source);
            if evaluated_source != source
                && let Some(annotation_text) =
                    self.declared_type_annotation_text_for_symbol_type(evaluated_source, true)
            {
                let display = self.format_declared_annotation_for_diagnostic(&annotation_text);
                return self.canonicalize_assignment_numeric_literal_union_display(
                    evaluated_source,
                    target,
                    display,
                );
            }
        }

        self.format_assignability_type_for_message(source, target)
    }

    fn should_preserve_nuia_source_undefined_display(
        &self,
        source: TypeId,
        target: TypeId,
        expr_idx: NodeIndex,
        expr_display_type: TypeId,
    ) -> bool {
        if !self.ctx.compiler_options.no_unchecked_indexed_access
            || expr_display_type == TypeId::ERROR
        {
            return false;
        }
        if !crate::query_boundaries::class_type::type_includes_undefined(self.ctx.types, source)
            || crate::query_boundaries::class_type::type_includes_undefined(self.ctx.types, target)
        {
            return false;
        }
        self.ctx.arena.get(expr_idx).is_some_and(|node| {
            node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                || node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        })
    }

    fn string_covered_template_union_source_display(&self, source: TypeId) -> Option<TypeId> {
        let members = crate::query_boundaries::common::union_members(self.ctx.types, source)?;
        if !members.contains(&TypeId::STRING) {
            return None;
        }
        members
            .iter()
            .all(|&member| self.is_string_covered_template_union_member(member))
            .then_some(TypeId::STRING)
    }

    fn is_string_covered_template_union_member(&self, type_id: TypeId) -> bool {
        type_id == TypeId::STRING
            || crate::query_boundaries::common::is_template_literal_type(self.ctx.types, type_id)
            || crate::query_boundaries::common::is_string_intrinsic_type(self.ctx.types, type_id)
            || crate::query_boundaries::common::literal_value(self.ctx.types, type_id)
                .is_some_and(|value| value.primitive_type_id() == TypeId::STRING)
    }

    pub(in crate::error_reporter) fn format_assignment_target_type_for_diagnostic(
        &mut self,
        target: TypeId,
        source: TypeId,
        anchor_idx: NodeIndex,
    ) -> String {
        if let Some(contextual_target) =
            self.object_literal_property_contextual_target_for_diagnostic(anchor_idx, target)
        {
            return self.format_object_literal_property_diag_target(contextual_target);
        }

        if let Some(display) =
            self.anonymous_composite_annotation_target_display(anchor_idx, target)
        {
            return display;
        }

        // An inline tuple / function / constructor target annotation
        // (`[number, string]`, `(a: number) => void`) carries no `aliasSymbol`,
        // so tsc renders its expanded structural form rather than a
        // coincidentally-shaped alias name (#17119) — the target mirror of the
        // source-side guard.
        if let Some(display) =
            self.inline_structural_type_annotation_target_display(anchor_idx, target)
        {
            return display;
        }

        let target_expr = self
            .assignment_target_expression(anchor_idx)
            .unwrap_or(anchor_idx);

        // When the target is a nullable union (e.g., `T | null | undefined`)
        // and the source is non-nullable, strip null/undefined from the
        // top-level display to match tsc's behavior.
        let display_target = self
            .strip_nullish_for_assignability_display(target, source)
            .unwrap_or(target);
        // A deferred conditional type renders verbatim in tsc diagnostics: the
        // exact `C extends E ? X : Y` form with its literal branches intact. The
        // annotation/alias heuristics below would otherwise collapse a conditional
        // with concrete branches into a branch union (e.g. `"yes" | "no"`); emit
        // the structural form directly instead.
        if crate::query_boundaries::common::is_conditional_type(self.ctx.types, display_target) {
            return self.format_type_diagnostic(display_target);
        }
        if crate::query_boundaries::common::is_index_access_type(self.ctx.types, display_target)
            && crate::query_boundaries::common::contains_type_parameters(
                self.ctx.types,
                display_target,
            )
        {
            return self.format_type_for_assignability_message(display_target);
        }
        // A longhand primitive-keyword union target annotation
        // (`string | number`) carries no `aliasSymbol`, so tsc renders it by its
        // members rather than repainting it with a coincidentally-shaped alias
        // reached through the reverse type-to-def lookup (#16610) — the target
        // mirror of the source-side guard. Gated on `display_target == target`
        // like the annotation-derived branches below, so the nullish strip above
        // keeps precedence (`string | undefined` against a non-nullish source
        // renders `string`, matching tsc's single-survivor collapse).
        if display_target == target
            && let Some(display) = self.longhand_primitive_union_target_display(anchor_idx, target)
        {
            return display;
        }
        if display_target == target
            && let Some(display) =
                self.readonly_array_alias_target_display(target_expr, display_target)
        {
            return display;
        }
        if let Some(display) = self.keyof_type_alias_body_display(display_target) {
            return display;
        }
        if let Some(display) = self.static_schema_array_structural_display(display_target, source) {
            return display;
        }

        if display_target == target
            && let Some(display) =
                self.keyof_type_alias_annotation_display_for_expression(target_expr)
        {
            return display;
        }
        if display_target == target
            && let Some(annotation) = self.direct_assignment_target_annotation_text(anchor_idx)
            && let Some(display) = self.keyof_type_alias_annotation_display(&annotation)
        {
            return display;
        }
        if display_target == target
            && let Some(display) = self.direct_assignment_target_annotation_text(anchor_idx)
            && display.trim() == "{}"
        {
            return self.format_annotation_like_type(&display);
        }
        if display_target == target
            && let Some(display) =
                self.declared_intersection_annotation_display_for_expression(target_expr)
        {
            return display;
        }
        if display_target == target
            && let Some(display) = self.declared_type_annotation_text_for_expression(target_expr)
            && display.contains('&')
            && display.contains('{')
            && !display.trim_start().starts_with("keyof ")
        {
            return self.format_annotation_like_type(&display);
        }
        if let Some(display) = self.declared_type_annotation_text_for_expression(target_expr)
            && (display.starts_with("keyof ") || Self::display_contains_mapped_clause(&display))
        {
            // For `typeof EnumName.Member`, tsc evaluates to the enum member type
            // and displays as `EnumName.Member` (without `typeof` prefix). Skip the
            // raw annotation text when the target resolves to an enum member type.
            if display.starts_with("typeof ")
                && crate::query_boundaries::common::enum_def_id(self.ctx.types, target).is_some()
            {
                // Fall through to use the TypeFormatter, which correctly displays
                // `TypeData::Enum` as qualified `W.a` style names.
            } else if display.starts_with("keyof ") && display_target == target {
                // For `keyof (A | B)` / `keyof (A & B)`, use the TypeFormatter so
                // that distribution rules apply (→ `keyof A & keyof B`).
                // For plain `keyof SomeName`, the annotation text is already correct
                // (tsc shows `keyof A`, not the expanded literal union). Only route
                // through TypeFormatter when the operand contains a union/intersection.
                //
                // An *anonymous* operand — an inline object type literal
                // (`keyof { a: 1 }`) — has no writable name, so tsc renders the
                // evaluated key set (`"a"`), not the `keyof { ... }` spelling.
                // Route those through the TypeFormatter, which reduces the
                // operator. A named operand keeps the annotation text.
                let operand_is_anonymous = crate::query_boundaries::common::keyof_inner_type(
                    self.ctx.types,
                    display_target,
                )
                .is_some_and(|operand| self.keyof_operand_is_anonymous(operand));
                let operand_text = display.trim_start_matches("keyof ").trim();
                let needs_distribution = operand_text.contains('|')
                    || (operand_text.contains('&') && operand_text.starts_with('('));
                if needs_distribution || operand_is_anonymous {
                    return self.format_type_for_assignability_message(display_target);
                }
                return self.format_annotation_like_type(&display);
            } else if display_target == target {
                // Only use annotation text when we didn't strip nullable members;
                // otherwise the annotation includes null/undefined that tsc omits.
                return self.format_annotation_like_type(&display);
            }
        }

        if display_target == target
            && let Some(display) = self.declared_type_annotation_text_for_expression(target_expr)
        {
            if display.trim() == "{}" {
                return self.format_annotation_like_type(&display);
            }
            if let Some(intersection_display) =
                self.declared_intersection_annotation_display_for_expression(target_expr)
            {
                return intersection_display;
            }
            if crate::query_boundaries::common::is_index_access_type(self.ctx.types, display_target)
                && Self::should_evaluate_indexed_access_annotation_for_assignment(&display)
            {
                let evaluated = self.evaluate_type_for_assignability(display_target);
                if evaluated != display_target && evaluated != TypeId::ERROR {
                    if let Some(members) =
                        crate::query_boundaries::common::union_members(self.ctx.types, evaluated)
                    {
                        let mut formatter = self
                            .ctx
                            .create_diagnostic_type_formatter()
                            .with_display_properties()
                            .with_preserve_optional_parameter_surface_syntax(true);
                        return members
                            .iter()
                            .map(|&member| formatter.format(member).into_owned())
                            .collect::<Vec<_>>()
                            .join(" | ");
                    }
                    let mut formatter = self
                        .ctx
                        .create_diagnostic_type_formatter()
                        .with_display_properties()
                        .with_skip_application_alias_names()
                        .with_preserve_optional_parameter_surface_syntax(true);
                    return formatter.format(evaluated).into_owned();
                }
            }
            let preserve_literal_surface = self.target_preserves_literal_surface(source);
            let fallback = if preserve_literal_surface {
                self.format_type_diagnostic(target)
            } else {
                // Use diagnostic mode to avoid synthetic `?: undefined` in unions
                self.format_type_diagnostic_widened(
                    self.widen_fresh_object_literal_properties_for_display(target),
                )
            };
            let assignability_display =
                self.format_assignability_type_for_message(display_target, source);
            if let Some((annotation_base, _)) = display.trim().split_once('<')
                && assignability_display.trim() == annotation_base.trim()
            {
                return self.format_annotation_like_type(&display);
            }
            if assignability_display.starts_with('"')
                || assignability_display.starts_with('`')
                || assignability_display == "true"
                || assignability_display == "false"
                || (crate::query_boundaries::common::string_intrinsic_components(
                    self.ctx.types,
                    display_target,
                )
                .is_some()
                    && assignability_display != fallback)
            {
                return assignability_display;
            }
            // Generic callable targets preserve type alias names from annotations
            let target_is_generic_callable =
                crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, target)
                    .is_some_and(|shape| {
                        shape
                            .call_signatures
                            .iter()
                            .chain(shape.construct_signatures.iter())
                            .any(|sig| !sig.type_params.is_empty())
                    })
                    || crate::query_boundaries::common::function_shape_for_type(
                        self.ctx.types,
                        target,
                    )
                    .is_some_and(|shape| !shape.type_params.is_empty());
            if target_is_generic_callable {
                return self.format_annotation_like_type(&display);
            }
            let display_trimmed = display.trim_start();
            if display_trimmed.starts_with('[') || display_trimmed.starts_with("readonly [") {
                return self.format_annotation_like_type(&display);
            }
            if self.tuple_target_has_application_display_alias(display_target)
                && let Some(tuple_display) =
                    self.raw_tuple_assignment_target_display_without_alias(display_target, true)
            {
                return tuple_display;
            }
            if !display_trimmed.contains('<')
                && assignability_display.contains('<')
                && let Some(tuple_display) =
                    self.raw_tuple_assignment_target_display_without_alias(display_target, true)
            {
                return tuple_display;
            }
            // When the target is a generic Application of a Lazy type alias
            // whose body recursively references the same alias (e.g.
            // `T2<U>` for `type T2<T> = [42, T2<{ x: T }>]`), expanding to
            // the alias body produces an unbounded `[42, [42, [..., ...]]]`
            // cascade because the printer flattens every nested Application
            // when alias names are skipped. tsc keeps the alias annotation
            // (`T2<U>`) in this case; preserve it here too.
            //
            // A recursive alias whose instantiation *converges* is different:
            // `Flatten<string[][]>` (`Flatten<T> = T extends readonly (infer
            // U)[] ? Flatten<U> : T`) fully reduces to `string`, and tsc
            // renders the reduced type — the conditional resolved away and
            // never stamped the alias onto the result. Only a non-converged
            // recursion (the evaluation stays deferred or still mentions the
            // cycle) keeps the annotation surface.
            if self.is_recursive_type_alias_application_for_display(target) {
                let evaluated = self.evaluate_type_for_assignability(display_target);
                let converges =
                    crate::query_boundaries::diagnostics::evaluated_alias_application_has_concrete_display(
                        self.ctx.types.as_type_database(),
                        display_target,
                        evaluated,
                    ) && crate::query_boundaries::diagnostics::application_reduces_to_displayable_shape(
                        self.ctx.types.as_type_database(),
                        evaluated,
                    ) && !crate::query_boundaries::recursive_alias::evaluated_recursion_still_reaches_alias(
                        self.ctx.types.as_type_database(),
                        &self.ctx.definition_store,
                        target,
                        evaluated,
                    );
                if !converges {
                    return self.format_annotation_like_type(&display);
                }
            }
            let preserve_tuple_alias_display = !display_trimmed.starts_with('{')
                && !display_trimmed.starts_with('(')
                && !display_trimmed.contains('[')
                && !display_trimmed.contains("=>")
                && !display_trimmed.contains(" | ")
                && !display_trimmed.contains(" & ")
                && !crate::query_boundaries::common::is_generic_application(self.ctx.types, target)
                && !self.type_alias_body_is_generic_application(target);
            if !preserve_tuple_alias_display
                && let Some(tuple_display) =
                    self.raw_tuple_assignment_target_display_without_alias(target, true)
            {
                return tuple_display;
            }
            if Self::display_has_member_literals_assignability(&display) {
                return self.format_annotation_like_type(&display);
            }
            if Self::display_has_member_literals_assignability(&fallback)
                && !Self::display_has_member_literals_assignability(&display)
            {
                return self.format_annotation_like_type(&display);
            }
            // When the fallback produces duplicate names in a union or tuple
            // (e.g., `Yep | Yep` or `[Yep, Yep]`) but the annotation text preserves
            // namespace-qualified names (e.g., `Foo.Yep | Bar.Yep` or
            // `[Foo.Yep, Bar.Yep]`), prefer the annotation text. This matches tsc's
            // behavior of qualifying types when they'd otherwise be ambiguous.
            if Self::has_duplicate_union_member_names(&fallback)
                && !Self::has_duplicate_union_member_names(&display)
            {
                return self.format_annotation_like_type(&display);
            }
            // When the target is an enum type, format_type() may resolve to
            // an unrelated type name (e.g., a DOM interface that shares the
            // same structural shape). Use the assignability formatter which
            // correctly produces namespace-qualified enum names.
            if crate::query_boundaries::common::enum_def_id(self.ctx.types, target).is_some() {
                return self.format_assignability_type_for_message(target, source);
            }
            // When the evaluated display contains our internal "error" sentinel
            // (from unresolved type names like `() => SymbolScope` where `SymbolScope`
            // is not defined), prefer the declared annotation text. tsc shows the
            // original annotation, not the evaluated error type.
            // Only applies when the annotation itself does not contain "error" (which
            // would indicate the user actually wrote a type named `error`).
            if fallback.contains("error") && !display.contains("error") {
                return self.format_annotation_like_type(&display);
            }
            // For Application types whose alias body is an IndexedAccess or
            // Conditional type (e.g. `type Cb<T> = {noAlias: () => T}["noAlias"]`),
            // tsc does not preserve the alias name in error messages — it shows the
            // structurally-evaluated form. The assignability_display computed above
            // already contains the correct expanded form.
            if assignability_display != fallback {
                let evaluated_for_display = self.evaluate_type_for_assignability(display_target);
                if self.should_use_evaluated_assignability_display(
                    display_target,
                    evaluated_for_display,
                ) {
                    return assignability_display;
                }
            }
            return fallback;
        }

        // When the target is an enum type without annotation text, use the
        // assignability formatter for correct qualified enum name display.
        if crate::query_boundaries::common::enum_def_id(self.ctx.types, display_target).is_some() {
            return self.format_assignability_type_for_message(display_target, source);
        }
        if let Some(tuple_display) =
            self.raw_tuple_assignment_target_display_without_alias(display_target, true)
        {
            return tuple_display;
        }

        if self.target_preserves_literal_surface(source) {
            let assignability_display =
                self.format_assignability_type_for_message(display_target, source);
            let fallback = self.format_type_diagnostic(display_target);
            if assignability_display.starts_with('"')
                || assignability_display.starts_with('`')
                || assignability_display == "true"
                || assignability_display == "false"
                || (crate::query_boundaries::common::string_intrinsic_components(
                    self.ctx.types,
                    display_target,
                )
                .is_some()
                    && assignability_display != fallback)
            {
                assignability_display
            } else {
                fallback
            }
        } else {
            // Use diagnostic mode to avoid synthetic `?: undefined` in unions
            let assignability_display =
                self.format_assignability_type_for_message(display_target, source);
            if self
                .lookup_type_alias_name_for_display(display_target)
                .is_some_and(|alias| alias == assignability_display)
            {
                return assignability_display;
            }
            let fallback = self.format_type_diagnostic_widened(
                self.widen_fresh_object_literal_properties_for_display(display_target),
            );
            if assignability_display.starts_with('"')
                || assignability_display.starts_with('`')
                || assignability_display == "true"
                || assignability_display == "false"
                || (crate::query_boundaries::common::string_intrinsic_components(
                    self.ctx.types,
                    display_target,
                )
                .is_some()
                    && assignability_display != fallback)
            {
                assignability_display
            } else {
                fallback
            }
        }
    }

    /// Whether the assignment target's declared annotation is a reference to
    /// a type alias (`x: MaybeBox`, `x: OrMissing<{ u: string }>`). tsc's
    /// `reportErrorResults` restores the original target whenever it carried
    /// an `aliasSymbol`; tsz's eager annotation resolution can lose the
    /// reference surface (a bare union alias resolves to the interned union).
    ///
    /// Returns `None` when no declared annotation node governs the anchor —
    /// the caller falls back to type-level provenance. When an annotation
    /// exists, its AST is *authoritative* (`Some(bool)`): a structurally
    /// identical anonymous annotation interns to the same `TypeId` as the
    /// alias body, so type-level signals (display aliases, reverse def
    /// lookups) cannot tell the two references apart, but the syntax can.
    pub(in crate::error_reporter) fn assignment_target_annotation_alias_reference_verdict(
        &mut self,
        anchor_idx: NodeIndex,
    ) -> Option<bool> {
        let annotation_idx = self.direct_assignment_target_annotation_node(anchor_idx)?;
        let node = self.ctx.arena.get(annotation_idx)?;
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return Some(false);
        }
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return Some(false);
        };
        let crate::symbol_resolver::TypeSymbolResolution::Type(sym_id) =
            self.resolve_identifier_symbol_in_type_position_without_tracking(type_ref.type_name)
        else {
            return Some(false);
        };
        Some(
            self.ctx
                .binder
                .get_symbol(sym_id)
                .is_some_and(|symbol| symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS)),
        )
    }

    fn tuple_target_has_application_display_alias(&self, target: TypeId) -> bool {
        crate::query_boundaries::common::is_tuple_type(self.ctx.types, target)
            && self
                .ctx
                .types
                .get_display_alias(target)
                .is_some_and(|alias| {
                    crate::query_boundaries::common::application_info(self.ctx.types, alias)
                        .is_some()
                })
    }

    /// True when `target` is `Application(Lazy(D), args)` and the alias body
    /// of `D` reaches another reference to `D` (directly or transitively).
    /// Used to suppress the `raw_tuple_…_without_alias` expansion path: a
    /// recursive alias rendered with alias names skipped collapses to an
    /// unbounded `[..., ...]` cascade rather than a useful structural form.
    fn is_recursive_type_alias_application_for_display(&self, target: TypeId) -> bool {
        crate::query_boundaries::recursive_alias::is_recursive_type_alias_application(
            self.ctx.types,
            &self.ctx.definition_store,
            target,
        )
    }

    fn raw_tuple_assignment_target_display_without_alias(
        &mut self,
        target: TypeId,
        allow_direct_tuple: bool,
    ) -> Option<String> {
        let mut current = target;
        let mut saw_generic_application = false;
        let display_target = 'resolved: {
            for _ in 0..4 {
                if crate::query_boundaries::common::is_tuple_type(self.ctx.types, current) {
                    if !allow_direct_tuple
                        && (crate::query_boundaries::common::is_generic_application(
                            self.ctx.types,
                            current,
                        ) || self.type_alias_body_is_generic_application(current))
                    {
                        saw_generic_application = true;
                        if crate::query_boundaries::common::is_generic_application(
                            self.ctx.types,
                            current,
                        ) {
                            let evaluated = self.evaluate_tuple_display_candidate(current);
                            if evaluated != current && evaluated != TypeId::ERROR {
                                current = evaluated;
                                continue;
                            }
                        }
                    }
                    break 'resolved current;
                }
                if current == TypeId::ERROR {
                    return None;
                }
                saw_generic_application |= crate::query_boundaries::common::is_generic_application(
                    self.ctx.types,
                    current,
                ) || self
                    .type_alias_body_is_generic_application(current);
                let evaluated = self.evaluate_tuple_display_candidate(current);
                if evaluated == current {
                    return None;
                }
                current = evaluated;
            }
            if crate::query_boundaries::common::is_tuple_type(self.ctx.types, current) {
                current
            } else {
                return None;
            }
        };
        if !allow_direct_tuple && !saw_generic_application {
            return None;
        }
        if crate::query_boundaries::common::lazy_def_id(self.ctx.types, display_target).is_some() {
            return None;
        }
        self.format_raw_tuple_assignment_target(display_target)
    }

    fn evaluate_tuple_display_candidate(&mut self, type_id: TypeId) -> TypeId {
        let evaluated = self.evaluate_application_type(type_id);
        if evaluated != type_id && evaluated != TypeId::ERROR {
            return evaluated;
        }
        let evaluated = self.evaluate_type_for_assignability(type_id);
        if evaluated != type_id && evaluated != TypeId::ERROR {
            return evaluated;
        }
        let evaluated = self.evaluate_type_with_env(type_id);
        if evaluated != type_id && evaluated != TypeId::ERROR {
            return evaluated;
        }
        type_id
    }

    fn format_raw_tuple_assignment_target(&mut self, type_id: TypeId) -> Option<String> {
        let elements = crate::query_boundaries::common::tuple_elements(self.ctx.types, type_id)?;
        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_display_properties()
            .with_skip_application_alias_names()
            .with_preserve_optional_parameter_surface_syntax(true);
        Some(formatter.format_tuple_elements_for_diagnostic(&elements))
    }

    fn type_alias_body_is_generic_application(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::common::lazy_def_id(self.ctx.types, type_id)
            .or_else(|| self.ctx.definition_store.find_def_for_type(type_id))
            .and_then(|def_id| self.ctx.definition_store.get(def_id))
            .is_some_and(|def| {
                def.kind == tsz_solver::def::DefKind::TypeAlias
                    && def.body.is_some_and(|body| {
                        crate::query_boundaries::common::is_generic_application(
                            self.ctx.types,
                            body,
                        )
                    })
            })
    }

    fn should_evaluate_indexed_access_annotation_for_assignment(display: &str) -> bool {
        display.contains("[keyof ")
    }

    pub(in crate::error_reporter) fn related_generic_indexed_access_source_display(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<String> {
        let (source_object, source_index) =
            crate::query_boundaries::common::index_access_types(self.ctx.types, source)?;
        let (target_object, target_index) =
            crate::query_boundaries::common::index_access_types(self.ctx.types, target)?;

        let source_index_info =
            crate::query_boundaries::common::type_param_info(self.ctx.types, source_index)?;
        crate::query_boundaries::common::type_param_info(self.ctx.types, target_index)?;
        if source_index == target_index {
            return None;
        }

        if source_object != target_object {
            return None;
        }

        let object_name = self.named_type_display_name(source_object)?;
        let source_index_display = self.ctx.types.resolve_atom_ref(source_index_info.name);
        Some(format!("{object_name}[{source_index_display}]"))
    }

    pub(in crate::error_reporter) fn format_nested_assignment_source_type_for_diagnostic(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> String {
        if crate::query_boundaries::common::literal_value(self.ctx.types, source).is_some()
            && crate::query_boundaries::common::string_intrinsic_components(self.ctx.types, target)
                .is_some_and(|(_, type_arg)| type_arg == TypeId::STRING)
        {
            let widened = self.widen_type_for_display(source);
            return self.format_assignability_type_for_message(widened, target);
        }

        if let Some(display) = self.preferred_evaluated_source_display(source, target) {
            return display;
        }

        // Skip the anchor-expression-derived path when `source` does not
        // correspond to the anchor's expression type. This happens during
        // nested elaboration of a structural failure (e.g. function-return
        // mismatch elaborates with the inner return types as `source`/
        // `target`, but the anchor still points at the outer assignment
        // expression). In that case the expression's type is the OUTER
        // value, which produces the bogus
        // "Type '(x: Object) => Object' is not assignable to type 'string'."
        // message — a category-error claim that the outer function is not
        // assignable to the inner return type.
        let anchor_expr_type = self
            .direct_diagnostic_source_expression(anchor_idx)
            .map(|expr_idx| self.get_type_of_node(expr_idx));
        let source_matches_anchor_expr = anchor_expr_type
            .is_some_and(|expr_type| expr_type == source || expr_type == TypeId::ERROR);
        if !source_matches_anchor_expr {
            return self.format_assignability_type_for_message(source, target);
        }

        if let Some(expr_idx) = self.direct_diagnostic_source_expression(anchor_idx) {
            if let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx) {
                if let Some(intersection_display) =
                    self.declared_intersection_annotation_display_for_expression(expr_idx)
                {
                    return intersection_display;
                }
                return display;
            }

            if let Some(display) = self.empty_array_literal_source_type_display(expr_idx) {
                return display;
            }

            if let Some(display) = self.object_literal_source_type_display(expr_idx, Some(target)) {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            if expr_type != TypeId::ERROR {
                let widened_expr_type = if self.target_preserves_literal_surface(target) {
                    expr_type
                } else {
                    self.widen_type_for_display(expr_type)
                };
                let display_type = self
                    .widened_enum_member_assignment_source(widened_expr_type, target)
                    .unwrap_or(widened_expr_type);
                let display_type = self.widen_function_like_display_type(display_type);
                if let Some(display) =
                    self.new_expression_nominal_source_display(expr_idx, display_type)
                {
                    return display;
                }
                return self.format_assignability_type_for_message(display_type, target);
            }
        }

        if let Some(expr_idx) = self.assignment_source_expression(anchor_idx) {
            if let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx) {
                if let Some(intersection_display) =
                    self.declared_intersection_annotation_display_for_expression(expr_idx)
                {
                    return intersection_display;
                }
                return display;
            }

            if let Some(display) = self.empty_array_literal_source_type_display(expr_idx) {
                return display;
            }

            if let Some(display) = self.object_literal_source_type_display(expr_idx, Some(target)) {
                return display;
            }

            if self.is_literal_sensitive_assignment_target(target)
                && let Some(display) =
                    self.call_object_literal_intersection_source_display(expr_idx, source, target)
            {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            let display_type = if expr_type != TypeId::ERROR {
                let widened_expr_type = if self.target_preserves_literal_surface(target) {
                    expr_type
                } else {
                    self.widen_type_for_display(expr_type)
                };
                self.widened_enum_member_assignment_source(widened_expr_type, target)
                    .unwrap_or(widened_expr_type)
            } else {
                self.widen_type_for_display(source)
            };
            let display_type = self.widen_function_like_display_type(display_type);
            if let Some(display) =
                self.new_expression_nominal_source_display(expr_idx, display_type)
            {
                return display;
            }
            return self.format_assignability_type_for_message(display_type, target);
        }

        // Check if source is a single-call-signature callable that tsc displays in
        // arrow syntax. For these, use the TypeFormatter instead of annotation text.
        let source_uses_arrow_syntax =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, source)
                .is_some_and(|shape| {
                    shape.call_signatures.len() == 1
                        && shape.construct_signatures.is_empty()
                        && shape.properties.is_empty()
                        && shape.string_index.is_none()
                        && shape.number_index.is_none()
                });
        if !source_uses_arrow_syntax {
            if let Some(annotation_text) =
                self.declared_type_annotation_text_for_symbol_type(source, true)
            {
                return self.format_declared_annotation_for_diagnostic(&annotation_text);
            }
            let evaluated_source = self.evaluate_type_with_env(source);
            if evaluated_source != source
                && let Some(annotation_text) =
                    self.declared_type_annotation_text_for_symbol_type(evaluated_source, true)
            {
                return self.format_declared_annotation_for_diagnostic(&annotation_text);
            }
        }

        self.format_assignability_type_for_message(source, target)
    }

    fn new_expression_nominal_source_display(
        &mut self,
        expr_idx: NodeIndex,
        display_type: TypeId,
    ) -> Option<String> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return None;
        }

        // When the result type is a union (e.g., `number | Date` from
        // `new unionOfDifferentReturnType(10)` where unionOfDifferentReturnType
        // is `{ new (a: number): number } | { new (a: number): Date }`),
        // TSC shows the actual result type, not the constructor variable name.
        // Return None to let the fallback formatting handle it.
        if crate::query_boundaries::common::union_members(self.ctx.types, display_type).is_some() {
            return None;
        }

        if let Some(new_expr) = self.ctx.arena.get_call_expr(node)
            && let Some(mut ctor_display) = self.expression_text(new_expr.expression)
        {
            if let Some(type_args) = &new_expr.type_arguments
                && !type_args.nodes.is_empty()
            {
                let rendered_args: Vec<String> = type_args
                    .nodes
                    .iter()
                    .map(|&arg| self.get_source_text_for_node(arg))
                    .collect();
                ctor_display.push('<');
                ctor_display.push_str(&rendered_args.join(", "));
                ctor_display.push('>');
                return Some(ctor_display);
            }
            // With display alias: show the named type (e.g. `D<unknown>` not `D`).
            // Without: variable name is not a type name; let caller format the actual type.
            if self.ctx.types.get_display_alias(display_type).is_some() {
                return Some(self.format_type_diagnostic_structural(display_type));
            }
            return None;
        }

        Some(self.format_property_receiver_type_for_diagnostic(display_type))
    }

    fn js_constructor_instance_assignment_source_display(
        &mut self,
        source: TypeId,
        anchor_idx: NodeIndex,
    ) -> Option<String> {
        crate::query_boundaries::common::object_shape_for_type(self.ctx.types, source)?;
        let expr_idx = self
            .direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx))?;
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let expr_node = self.ctx.arena.get(expr_idx)?;
        if expr_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }

        let source_sym = self.resolve_identifier_symbol(expr_idx)?;
        let source_symbol = self
            .get_cross_file_symbol(source_sym)
            .or_else(|| self.ctx.binder.get_symbol(source_sym))?;
        if (source_symbol.flags & tsz_binder::symbol_flags::VARIABLE) == 0 {
            return None;
        }

        let current_file_idx = self.ctx.current_file_idx as u32;
        let this_pos = expr_node.pos;
        source_symbol
            .stable_declarations
            .iter()
            .copied()
            .chain(std::iter::once(source_symbol.stable_value_declaration))
            .filter_map(|stable_loc| {
                if !stable_loc.is_known() {
                    return None;
                }
                let file_idx = if stable_loc.has_file_idx() {
                    stable_loc.file_idx
                } else {
                    current_file_idx
                };
                let (decl_idx, arena) = self.ctx.node_at_stable_location(stable_loc)?;
                let decl_node = arena.get(decl_idx)?;
                let declaration = arena.get_variable_declaration(decl_node)?;
                if file_idx == current_file_idx && decl_node.pos > this_pos {
                    return None;
                }

                let init_idx = arena.skip_parenthesized_and_assertions(declaration.initializer);
                let init_node = arena.get(init_idx)?;
                if init_node.kind != syntax_kind_ext::NEW_EXPRESSION {
                    return None;
                }
                let new_expr = arena.get_call_expr(init_node)?;
                let ctor_idx = arena.skip_parenthesized_and_assertions(new_expr.expression);
                let ctor_node = arena.get(ctor_idx)?;
                let ident = arena.get_identifier(ctor_node)?;
                Some((
                    file_idx == current_file_idx,
                    decl_node.pos,
                    ident.escaped_text.to_string(),
                ))
            })
            .max_by_key(|(same_file, decl_pos, _)| (*same_file, *decl_pos))
            .map(|(_, _, display)| display)
    }

    fn call_unknown_array_source_display(
        &mut self,
        expr_idx: NodeIndex,
        target: TypeId,
    ) -> Option<String> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        let call = self.ctx.arena.get_call_expr(node)?;

        let first_arg = *call.arguments.as_ref()?.nodes.first()?;
        let first_arg_type = self.get_type_of_node(first_arg);
        if matches!(first_arg_type, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }

        let element_type =
            crate::query_boundaries::common::array_element_type(self.ctx.types, first_arg_type)
                .or_else(|| {
                    tsz_solver::operations::get_iterator_info(self.ctx.types, first_arg_type, false)
                        .map(|info| info.yield_type)
                })?;
        if matches!(element_type, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }

        let recovered = diagnostic_query::display_array_type(
            self.ctx.types,
            self.widen_type_for_display(element_type),
        );
        Some(self.format_assignability_type_for_message(recovered, target))
    }

    fn preferred_evaluated_source_display(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<String> {
        let preserve_literal_surface = self.target_preserves_literal_surface(target);
        if crate::query_boundaries::common::is_template_literal_type(self.ctx.types, source) {
            return Some(self.format_type_diagnostic_structural(source));
        }

        let evaluated = self.evaluate_type_for_assignability(source);
        if evaluated == source || evaluated == TypeId::ERROR {
            return None;
        }

        if crate::query_boundaries::common::literal_value(self.ctx.types, evaluated).is_some()
            || crate::query_boundaries::common::is_template_literal_type(self.ctx.types, evaluated)
            || crate::query_boundaries::common::string_intrinsic_components(
                self.ctx.types,
                evaluated,
            )
            .is_some()
        {
            return Some(if preserve_literal_surface {
                self.format_type_diagnostic(evaluated)
            } else {
                self.format_type_diagnostic_structural(evaluated)
            });
        }

        None
    }

    pub(in crate::error_reporter) fn broad_mapped_index_signature_source_display(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<String> {
        let mapped = crate::query_boundaries::common::mapped_type_info(self.ctx.types, source)?;
        if mapped.name_type.is_some() || mapped.optional_modifier.is_some() {
            return None;
        }
        let key_kind = match mapped.constraint {
            TypeId::STRING => "string",
            TypeId::NUMBER => "number",
            _ => return None,
        };
        if crate::query_boundaries::common::contains_type_parameter_named(
            self.ctx.types,
            mapped.template,
            mapped.type_param.name,
        ) {
            return None;
        }
        if self
            .broad_mapped_index_signature_display_relation_outcome(source, target)
            .related
        {
            return None;
        }

        let readonly_prefix = match mapped.readonly_modifier {
            Some(tsz_solver::MappedModifier::Add) => "readonly ",
            Some(tsz_solver::MappedModifier::Remove) => "-readonly ",
            None => "",
        };
        let value_display = self.format_type_for_assignability_message(mapped.template);
        Some(format!(
            "{{ {readonly_prefix}[x: {key_kind}]: {value_display}; }}"
        ))
    }

    fn type_assertion_mapped_alias_source_display(
        &mut self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let node = self.ctx.arena.get(expr_idx)?;
        if !matches!(
            node.kind,
            syntax_kind_ext::AS_EXPRESSION | syntax_kind_ext::TYPE_ASSERTION
        ) {
            return None;
        }
        let assertion = self.ctx.arena.get_type_assertion(node)?;
        let assertion_type = self.get_type_from_type_node(assertion.type_node);
        let is_generic_mapped_alias =
            crate::query_boundaries::common::is_generic_application(self.ctx.types, assertion_type)
                && crate::query_boundaries::common::get_application_lazy_def_id(
                    self.ctx.types,
                    assertion_type,
                )
                .and_then(|def_id| self.ctx.definition_store.get(def_id))
                .is_some_and(|def| {
                    def.kind == tsz_solver::def::DefKind::TypeAlias
                        && def.body.is_some_and(|body| {
                            crate::query_boundaries::common::is_mapped_type(self.ctx.types, body)
                        })
                });
        if !is_generic_mapped_alias {
            return None;
        }
        self.node_text(assertion.type_node)
            .and_then(|text| self.sanitize_type_annotation_text_for_diagnostic(text, false))
            .map(|text| self.format_annotation_like_type(&text))
    }
}
