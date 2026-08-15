//! [`CheckerState::assign_binding_pattern_symbol_types_with_request_reporting`],
//! split out of `destructuring.rs` to stay under that file's line ratchet.
//!
//! This is the TS7031-reporting variant of
//! [`CheckerState::assign_binding_pattern_symbol_types_with_request`] — see
//! that function's doc comment in `destructuring.rs` for why the two exist
//! separately rather than always reporting.

use crate::context::TypingRequest;
use crate::query_boundaries::binding_patterns;
use crate::query_boundaries::common as common_query;
use crate::query_boundaries::flow as flow_boundary;
use crate::query_boundaries::state::checking as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Same as [`Self::assign_binding_pattern_symbol_types_with_request`], plus
    /// TS7031 (`Binding element 'x' implicitly has an 'any' type.`) for a leaf
    /// whose element type is exactly `null`/`undefined` and widens to `any`
    /// under non-strict null checks (`flow_boundary::widen_null_undefined_to_any`
    /// below) — the destructuring-binding twin of the mutable-binding TS7005
    /// and return-position TS7010 checks (`mutable_binding_nullish.rs`,
    /// `return_type_nullish.rs`). `report_widened_any` is `false` for contexts
    /// this does not (yet) cover — parameter-destructuring defaults and
    /// for-in/for-of loop variables — and threads unchanged through nested
    /// pattern recursion so a nested leaf (`var {p: {q}} = {p: {q: undefined}}`)
    /// reports too.
    pub(crate) fn assign_binding_pattern_symbol_types_with_request_reporting(
        &mut self,
        pattern_idx: NodeIndex,
        parent_type: TypeId,
        request: &TypingRequest,
        report_widened_any: bool,
    ) {
        let parent_type =
            self.normalize_parameter_binding_pattern_source_type(pattern_idx, parent_type);

        // Skip nested pattern processing for ERROR types to prevent cascading
        // diagnostics. When a parent element resolves to ERROR (e.g., from
        // destructuring `unknown`), nested patterns should not emit further errors.
        if parent_type == TypeId::ERROR {
            return;
        }

        self.report_unknown_empty_binding_pattern(pattern_idx, parent_type);

        // A binding default's literal type is only preserved (under `const`) when
        // the destructuring source is a tuple — i.e. a fresh array literal or a
        // tuple-typed value, where the positional element literal is meaningful
        // (`const [first = 0] = [10, 20]` → `0 | 10`). For non-tuple sources
        // (arrays, or unions like `RegExpMatchArray | never[]`) the default
        // widens as usual, matching tsc once the source element is taken into
        // account.
        let source_is_tuple = query::tuple_elements(
            self.ctx.types,
            query::unwrap_readonly_deep(self.ctx.types, parent_type),
        )
        .is_some();

        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };
        let Some(pattern_data) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        for (i, &element_idx) in pattern_data.elements.nodes.iter().enumerate() {
            if element_idx.is_none() {
                continue;
            }

            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }

            let Some(element_data) = self.ctx.arena.get_binding_element(element_node) else {
                continue;
            };

            let mut element_type = if parent_type == TypeId::ANY {
                TypeId::ANY
            } else {
                self.get_binding_element_type_with_request(
                    pattern_idx,
                    i,
                    parent_type,
                    element_data,
                    request,
                )
            };

            // If there's an initializer, the type incorporates it.
            // TypeScript widens the inferred type with the initializer type.
            // Set contextual type for function-like defaults so parameter types
            // are inferred from the expected element type (e.g., `{ f: id = arg => arg }: T`).
            if element_data.initializer.is_some() {
                // A default value guarantees the binding won't be undefined at runtime,
                // so strip `undefined` from the element type. This matches tsc behavior:
                // `{ name = "default" }: { name?: string }` gives `name` type `string`.
                // Route through flow observation boundary for centralized policy.
                if self.ctx.strict_null_checks() {
                    element_type = flow_boundary::narrow_destructuring_default(
                        self.ctx.types,
                        element_type,
                        true,
                    );
                }

                // Provide the element type as contextual type for the default
                // value expression. This is needed for:
                // - Arrow/function defaults: infers parameter types
                // - Array literal defaults: produces tuples instead of widened arrays
                //   e.g., `[b, {x}]=["abc", {x: 10}]` needs the default typed as
                //   a tuple `[string, {x: number}]`, not `(string | {x: number})[]`
                let request = if element_type != TypeId::ANY
                    && element_type != TypeId::UNKNOWN
                    && element_type != TypeId::ERROR
                {
                    request.read().contextual(element_type)
                } else {
                    request.read().contextual_opt(None)
                };
                self.invalidate_expression_for_contextual_retry(element_data.initializer);
                // Under `const`/`readonly`, tsc does not widen the default's literal
                // type (`widenTypeInferredFromInitializer` skips the `Constant` case),
                // so `const [first = 0] = [10, 20]` yields `0 | 10`, not `number`.
                // `let`/`var`/parameter defaults widen, matching the standard path.
                // Gated to tuple sources so non-tuple sources (e.g. a `number[]` or a
                // `RegExpMatchArray | never[]` union) keep their widening behavior.
                let is_const_declaration = self.binding_pattern_is_const_declaration(pattern_idx);
                let prev_preserve = self.ctx.preserve_literal_types;
                if source_is_tuple && is_const_declaration {
                    self.ctx.preserve_literal_types = true;
                }
                let init_type =
                    self.get_type_of_node_with_request(element_data.initializer, &request);
                self.ctx.preserve_literal_types = prev_preserve;

                // When the destructuring SOURCE is genuinely `any`, every element is
                // `any` and stays `any` (tsc's `isTypeAny(parentType)` short-circuit):
                // do NOT fold the default initializer's type onto it, or a nested
                // computed-key would then be checked against the init type and fire a
                // spurious TS2537. The initializer is still evaluated above for its own
                // checks; only the type override is skipped.
                if parent_type != TypeId::ANY {
                    // A genuinely `any`-typed default (`= fallback` where
                    // `fallback: any`) always widens the element to `any`,
                    // matching tsc's union-with-initializer rule: `any` as a
                    // union member absorbs every other member
                    // (`db.union2`/`normalize_union` already encode this), so
                    // the `related` short-circuit below — which exists to
                    // avoid a redundant union call when the default's type is
                    // already covered by the slot type — must not apply here.
                    // `destructuring_relation_outcome(any, concreteSlotType)`
                    // reports `related` (any is assignable to anything), which
                    // would otherwise keep `element_type` at the concrete,
                    // un-widened slot type and let a nested computed key
                    // (`{ [k]: y } = fallback`) see a source with no index
                    // signature — a false TS2538 (oracle-verified: tsc widens
                    // to `any` here even under `--strict false`).
                    if element_type == TypeId::ANY
                        || element_type == TypeId::UNKNOWN
                        || init_type == TypeId::ANY
                    {
                        element_type = init_type;
                    } else if !self
                        .destructuring_relation_outcome(init_type, element_type)
                        .related
                    {
                        element_type = binding_patterns::binding_pattern_initializer_union_type(
                            self.ctx.types,
                            element_type,
                            init_type,
                        );
                    }
                    // A tuple-positional slot's literal type (`10` from `[10, 20]`)
                    // is preserved above so a `const` default can combine with it
                    // precisely, but a non-const declaration must still widen the
                    // combined result the same way the ordinary (non-destructuring)
                    // initializer path does: `let [first = 0] = [10, 20]` widens to
                    // `number`, not `0 | 10` (oracle-verified, typescript@7.0.2).
                    // This also covers the case where the default's literal exactly
                    // matches the slot's (`let [x = 10] = [10, 20]`), which takes the
                    // `related` branch above and would otherwise leave `element_type`
                    // at the unwidened literal `10`.
                    if source_is_tuple && !is_const_declaration {
                        element_type =
                            common_query::widen_literal_type(self.ctx.types, element_type);
                    }
                }
            }

            let Some(name_node) = self.ctx.arena.get(element_data.name) else {
                continue;
            };

            // Identifier binding: cache the inferred type on the symbol.
            if name_node.kind == SyntaxKind::Identifier as u16
                && let Some(sym_id) = self.ctx.binder.get_node_symbol(element_data.name)
            {
                // A bare `unique symbol` binding element widens to `symbol`, matching
                // tsc's `widenTypeForVariableLikeDeclaration` (the `isBindingElement`
                // branch always widens — a binding element's pattern annotation types
                // the *source*, not the element binding, so `const [db]: [typeof cs] = t`
                // still yields `db: symbol`). A binding element is never the unique
                // symbol's mint site, so no ref guard is needed; `is_unique_symbol_type`
                // is bare-only, so a `typeof a | typeof b` element is preserved. This
                // widens the *element* only — nested patterns recurse below with the
                // un-widened `element_type` and widen at their own leaf identifiers.
                let element_type =
                    if common_query::is_unique_symbol_type(self.ctx.types, element_type) {
                        TypeId::SYMBOL
                    } else {
                        element_type
                    };
                // Route null/undefined widening through the flow observation boundary.
                let final_type = flow_boundary::widen_null_undefined_to_any(
                    self.ctx.types,
                    element_type,
                    self.ctx.strict_null_checks(),
                );
                self.cache_symbol_type(sym_id, final_type);

                // TS7031: the leaf's own checked type was exactly `null`/`undefined`
                // (not already `any` through some other declared source — that
                // case leaves `element_type` unchanged by the widen above) and
                // widened to `any` here. Comparing the pre/post-widen types
                // directly (rather than re-deriving "genuine widening leaf"
                // provenance the way the TS7005/TS7010 array-literal siblings
                // must) is exact at this per-element granularity: there is no
                // BCT collapse to hide an already-`any` source behind.
                //
                // Excludes an element with its own default (`element_data.
                // initializer.is_some()`): `destructuring_relation_outcome`'s
                // "is the default related to the raw slot type" check above can
                // leave `element_type` at the raw nullish slot type uncombined
                // with the default's own type in some cases (e.g. a `number`
                // default against a `null`/`undefined` slot), which would make
                // this fire on an element tsc does not flag (the default's own
                // type is what should govern). Fixing that combination is a
                // separate, pre-existing destructuring-relation question; this
                // gate stays conservative and defers to it rather than guess.
                if report_widened_any
                    && element_data.initializer.is_none()
                    && final_type == TypeId::ANY
                    && element_type != TypeId::ANY
                {
                    use crate::diagnostics::diagnostic_codes;
                    let binding_name = self.parameter_name_for_error(element_data.name);
                    self.error_at_node_msg(
                        element_data.name,
                        diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE,
                        &[&binding_name, "any"],
                    );
                }
            }

            // Nested binding patterns: check iterability for array patterns, then recurse
            if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                // Check iterability for nested array destructuring
                self.check_destructuring_iterability(
                    element_data.name,
                    element_type,
                    NodeIndex::NONE,
                );
                let nested_request = if element_type != TypeId::ANY
                    && element_type != TypeId::UNKNOWN
                    && element_type != TypeId::ERROR
                {
                    request.read().contextual(element_type)
                } else {
                    request.read().contextual_opt(None)
                };
                self.assign_binding_pattern_symbol_types_with_request_reporting(
                    element_data.name,
                    element_type,
                    &nested_request,
                    report_widened_any,
                );
            } else if name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
                let nested_request = if element_type != TypeId::ANY
                    && element_type != TypeId::UNKNOWN
                    && element_type != TypeId::ERROR
                {
                    request.read().contextual(element_type)
                } else {
                    request.read().contextual_opt(None)
                };
                self.assign_binding_pattern_symbol_types_with_request_reporting(
                    element_data.name,
                    element_type,
                    &nested_request,
                    report_widened_any,
                );
            }
        }
    }
}
