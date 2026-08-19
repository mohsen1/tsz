//! Literal widening for return and yield contributions.
//!
//! The per-contribution widening rules of tsc's `getReturnTypeFromBody` /
//! `checkAndAggregateReturnExpressionTypes`: whether a fresh literal return or
//! `yield` contribution widens to its primitive base, how a contextual return
//! type's literal domain gates that (`isLiteralOfContextualType` via
//! `getWidenedLiteralLikeTypeForContextualReturnTypeIfNeeded`), and the
//! const-assertion-preserving widener shared with the `NoInfer` path.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Apply literal widening to a single return expression's inferred type,
    /// matching tsc's `getReturnTypeFromBody` widening rules per-contribution:
    ///
    /// - When the function has a contextual return type, do not widen — except
    ///   inside a `satisfies` operand. A `satisfies` type only *validates* the
    ///   operand; it does not pin the body literal unless it actually contains
    ///   that literal (`isLiteralOfContextualType`). So in a `satisfies` operand
    ///   a non-pinning contextual return (`unknown`, `any`, a base primitive, an
    ///   object/function type — as in `() => 1 satisfies () => unknown`) widens
    ///   the fresh literal just like the no-context case, per tsc's
    ///   `getWidenedLiteralLikeTypeForContextualType`, while a literal/literal
    ///   union (`satisfies () => 1`) keeps it. Outside `satisfies` the contextual
    ///   return is a genuine contextual position that already shaped the literal,
    ///   so it is preserved unchanged.
    /// - When the outer scope requested literal preservation
    ///   (`preserve_literal_types`), do not widen.
    /// - When the return expression is wrapped in a const assertion
    ///   (`return x as const` or `return <const>x`), preserve the asserted
    ///   literal type even without a contextual return type. tsc keeps the
    ///   const-asserted literal as the inferred return type.
    /// - Otherwise widen literal types only when the return expression is fresh
    ///   (`return "a"` → return type `string`). Non-fresh references such as
    ///   parameters or annotated locals keep their declared literal-union type.
    pub(crate) fn maybe_widen_return_contribution(
        &mut self,
        expr_idx: NodeIndex,
        type_id: TypeId,
        return_context: Option<TypeId>,
    ) -> TypeId {
        if self.return_contribution_is_widenable(expr_idx, type_id, return_context) {
            let widened = self.widen_return_contribution_preserving_const(expr_idx, type_id);
            // The primitive-literal widener skips `TypeData::Enum`, so a fresh
            // enum-member return (`() => E.A`) must additionally widen to its
            // parent enum. No-op for the already-widened primitive/object result.
            let widened = self.widen_enum_member_type(widened);
            if !self.ctx.strict_null_checks() {
                // tsc widens null/undefined return contributions to `any`
                // under strictNullChecks: false (`return null` infers
                // `() => any`).
                return crate::query_boundaries::widening::widen_nullish_to_any_deep(
                    self.ctx.types,
                    widened,
                );
            }
            return widened;
        }
        type_id
    }

    /// Whether a single `yield` operand contribution would be widened by the
    /// generator yield-type aggregation in `check_generator_body_return` — the
    /// yield-path sibling of [`Self::return_contribution_is_widenable`].
    ///
    /// Deliberate differences from the return predicate: the contextual gate is
    /// applied at the aggregation site against the contextual *yield* type
    /// (`contextual_type_allows_literal`, tsc `isLiteralOfContextualType`), not
    /// per contribution; and there is no conditional-expression carve-out —
    /// tsc widens `yield cond ? 1 : 1` to `number` (the branches collapse to a
    /// single fresh literal), which `is_fresh_literal_expression`'s
    /// either-branch-fresh rule reproduces. The cheap type-shape check runs
    /// first so the AST freshness walk is skipped for the common non-literal
    /// operand.
    pub(crate) fn yield_contribution_is_widenable(
        &mut self,
        expr_idx: NodeIndex,
        type_id: TypeId,
    ) -> bool {
        if expr_idx.is_none() || self.ctx.preserve_literal_types {
            return false;
        }
        if crate::query_boundaries::common::is_literal_type(self.ctx.types, type_id) {
            return self.is_fresh_literal_expression(expr_idx);
        }
        self.is_enum_member_type_for_widening(type_id)
    }

    /// Whether a return contribution is a bare top-level `null`/`undefined`
    /// scalar whose non-strict `-> any` widening must be DEFERRED past the return
    /// union reduction (#16580 b5). `has_ts_nullable_flag` matches exactly `null`
    /// and `undefined` — never `void`, never a union — which is the set the
    /// reduction may drop or collapse; a nullish leaf nested in a fresh composite
    /// (`return [undefined]`) is a composite type here and widens in place.
    pub(crate) const fn is_bare_nonstrict_nullish_return(&self, type_id: TypeId) -> bool {
        !self.ctx.strict_null_checks()
            && crate::query_boundaries::type_predicates::has_ts_nullable_flag(type_id)
    }

    /// Whether a single return-expression contribution would be widened by
    /// `maybe_widen_return_contribution` — i.e. it is a fresh literal expression
    /// with none of the per-expression carve-outs (a pinning contextual return
    /// type, `preserve_literal_types`, a `const` assertion, or a conditional
    /// expression).
    ///
    /// Block-body inference collects the *unwidened* contributions, unions them,
    /// and widens the union only when it collapses to a single literal (tsc's
    /// `getWidenedType(getUnionType(unwidenedReturnTypes))`). Two distinct fresh
    /// literals (`return "a"; return "b"`) must stay a literal union, so the
    /// per-branch widen is deferred to that single-literal check. This predicate
    /// records, per branch, whether that survivor would have been widenable.
    pub(crate) fn return_contribution_is_widenable(
        &mut self,
        expr_idx: NodeIndex,
        type_id: TypeId,
        return_context: Option<TypeId>,
    ) -> bool {
        if let Some(ctx_type) = return_context {
            // #17501: record `NoInfer<free-param>` bodies; decision below unchanged.
            self.mark_noinfer_generic_return_body_if_applicable(expr_idx, ctx_type);
            // tsc `getWidenedLiteralLikeTypeForContextualReturnTypeIfNeeded`: a
            // contextual return type pins a fresh literal contribution only when
            // it actually admits that literal's domain
            // (`isLiteralOfContextualType`). A literal context of a different
            // base kind (`(a) => ''` against `(a: number) => 1`) widens the
            // contribution exactly like the no-context case, so the argument
            // relation and its elaboration see `string`, not `""` (#17686).
            // Non-literal contributions are outside `getWidenedLiteralType`'s
            // scope and keep the context's pin unconditionally (also skipping
            // the domain query on the hot non-literal path).
            let literal_like =
                crate::query_boundaries::common::is_literal_type(self.ctx.types, type_id)
                    || self.is_enum_member_type_for_widening(type_id);
            if !literal_like || self.contextual_type_allows_literal(ctx_type, type_id) {
                return false;
            }
        }
        if self.ctx.preserve_literal_types {
            return false;
        }
        if self.return_expression_is_const_assertion(expr_idx) {
            return false;
        }
        if self.ctx.arena.get(expr_idx).is_some_and(|node| {
            node.kind == tsz_parser::parser::syntax_kind_ext::CONDITIONAL_EXPRESSION
        }) {
            return false;
        }
        // An enum-member access (`return E.A`) is a fresh enum literal in tsc:
        // `getReturnTypeFromBody` widens it to the parent enum (`E`), exactly as
        // a fresh primitive literal widens to its base (`return "x"` → `string`).
        // Freshness alone gates this: `is_fresh_literal_expression` now recognizes
        // a direct enum-member access as fresh, so a non-fresh enum reference
        // (`const c: E.A = E.A; return c`) correctly keeps `E.A`. The carve-outs
        // above (a pinning contextual return, `preserve_literal_types`, an
        // `as const` assertion, a conditional deferred to union collapse) already
        // returned, so enum members observe the same preservation rules. The widen
        // itself runs through `widen_enum_member_type` at each widenable site,
        // since the primitive literal widener leaves `TypeData::Enum` untouched.
        self.is_fresh_literal_expression(expr_idx)
    }

    /// Widen a fresh return-expression contribution while preserving literal
    /// property types whose object-literal initializer is a const assertion.
    ///
    /// tsc's `getWidenedType` only widens types carrying the widening flag. A
    /// per-property const assertion such as `{ type: "tracked" as const }`
    /// produces a *regular* (non-widening) literal, so the inferred return type
    /// keeps `type: "tracked"` while still widening its non-asserted siblings
    /// (`store: "x"` → `store: string`). This matters for discriminated-union
    /// narrowing on the inferred return type: widening the discriminant to
    /// `string` collapses the union and produces false `TS2339`/`TS2322`.
    ///
    /// The plain `widen_literal_type` widens every literal leaf unconditionally,
    /// so this AST-driven walk recurses through object-literal initializers and
    /// preserves the const-asserted subtrees, mirroring the const-assertion
    /// carve-out already applied to whole-expression `return x as const`.
    pub(crate) fn widen_return_contribution_preserving_const(
        &mut self,
        expr_idx: NodeIndex,
        type_id: TypeId,
    ) -> TypeId {
        let expr_idx = self.unwrap_parenthesized_expression(expr_idx);

        // A const-asserted subtree is preserved wholesale.
        if self.return_expression_is_const_assertion(expr_idx) {
            return type_id;
        }

        // Only object literals need per-property preservation. Other fresh
        // expressions (bare literals, array literals, template/conditional)
        // keep the existing blanket widening.
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return self.widen_literal_type(type_id);
        };
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return self.widen_literal_type(type_id);
        }
        let Some(obj) = self.ctx.arena.get_literal_expr(node) else {
            return self.widen_literal_type(type_id);
        };
        let Some(shape) =
            crate::query_boundaries::widening::object_shape_for_type(self.ctx.types, type_id)
        else {
            return self.widen_literal_type(type_id);
        };

        // Map declared property names to their initializer expression so each
        // shape property can consult its own AST node. Spread/shorthand members
        // are not recorded; their properties fall back to plain widening (a
        // no-op for the annotated/non-fresh types that spreads contribute).
        let element_nodes: Vec<NodeIndex> = obj.elements.nodes.clone();
        let mut initializer_for: rustc_hash::FxHashMap<String, NodeIndex> =
            rustc_hash::FxHashMap::default();
        for element_idx in element_nodes {
            let Some(element) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            if element.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                && let Some(prop) = self.ctx.arena.get_property_assignment(element)
                && let Some(name) = self.get_property_name(prop.name)
            {
                initializer_for.insert(name, prop.initializer);
            }
        }

        let mut new_props = Vec::with_capacity(shape.properties.len());
        let mut changed = false;
        for prop in &shape.properties {
            let name = self.ctx.types.resolve_atom(prop.name);
            let widened_type = match initializer_for.get(name.as_str()) {
                // Recurse so nested object-literal const assertions
                // (`{ outer: { type: "x" as const } }`) are preserved too.
                Some(&init_idx) => {
                    self.widen_return_contribution_preserving_const(init_idx, prop.type_id)
                }
                // Spread/shorthand-sourced property: widen as before.
                None => self.widen_literal_type(prop.type_id),
            };
            if widened_type != prop.type_id {
                changed = true;
            }
            let mut new_prop = prop.clone();
            new_prop.type_id = widened_type;
            new_prop.write_type = widened_type;
            new_props.push(new_prop);
        }

        if !changed {
            return type_id;
        }

        crate::query_boundaries::widening::rebuild_object_with_shape_metadata(
            self.ctx.types,
            type_id,
            &shape,
            new_props,
        )
    }
}
