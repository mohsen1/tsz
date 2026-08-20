//! Object and array literal elaboration helpers for call diagnostics.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::error_reporter::fingerprint_policy::DiagnosticAnchorKind;
use crate::query_boundaries::common as query_common;
use crate::query_boundaries::common::ContextualTypeContext;
use crate::query_boundaries::relation_types::RelationFailure;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Elaborate object literal property type mismatches with TS2322.
    fn try_elaborate_object_literal_properties(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
    ) -> bool {
        self.try_elaborate_object_literal_properties_with_source(arg_idx, param_type, None)
    }

    pub(super) fn try_elaborate_object_literal_properties_with_source(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
        source_type_override: Option<TypeId>,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        // When exactOptionalPropertyTypes is enabled and the failure is due to
        // exact optional property mismatch, don't elaborate per-property errors.
        // The caller will emit a top-level TS2375 instead.
        let node_source_type = self.get_type_of_node(arg_idx);
        let source_type = source_type_override.unwrap_or(node_source_type);
        if self.has_exact_optional_property_mismatch(source_type, param_type) {
            return false;
        }

        let overall_target_is_union =
            crate::query_boundaries::common::union_members(self.ctx.types, param_type).is_some();

        // Normalize optional/nullish wrappers (e.g., `{...} | undefined`).
        let mut effective_param_type = if let (Some(non_nullish), Some(_nullish_cause)) =
            self.split_nullish_type(param_type)
        {
            non_nullish
        } else {
            param_type
        };

        // Don't elaborate `never` targets — tsc emits a single TS2345 instead.
        if effective_param_type == TypeId::NEVER {
            return false;
        }

        // Don't elaborate into object literal properties when the target is a
        // primitive type (string, number, boolean, etc.).  Primitives can expose
        // properties via index signatures or prototypes, which causes misleading
        // per-property TS2322 errors instead of the correct top-level mismatch
        // (e.g., "Type '{ 0: number }' is not assignable to type 'string'").
        if crate::query_boundaries::common::is_primitive_type(self.ctx.types, effective_param_type)
        {
            return false;
        }

        let arg_node = match self.ctx.arena.get(arg_idx) {
            Some(node) => node,
            None => return false,
        };

        let obj = match self.ctx.arena.get_literal_expr(arg_node) {
            Some(obj) => obj.clone(),
            None => return false,
        };

        let resolved_param_type = self.resolve_type_for_property_access(effective_param_type);
        let evaluated_param_type = self.judge_evaluate(resolved_param_type);
        let contextual_param_type = self.evaluate_contextual_type(effective_param_type);
        let lazy_resolved_param_type = self.resolve_lazy_type(effective_param_type);
        let lazy_evaluated_param_type = self.evaluate_contextual_type(lazy_resolved_param_type);
        let assignability_param_type = self.evaluate_type_for_assignability(effective_param_type);
        let lazy_member_param_type = self.resolve_lazy_members_in_union(assignability_param_type);
        // The target as WRITTEN — before the nullish split and the
        // discriminant narrowing: the per-property elaboration target derives
        // from this when every union constituent (nullish arms included)
        // exposes the key, so `A | B | undefined` falls back to the
        // best-matching member exactly like tsc's undefined indexed access
        // (see `full_union_object_literal_property_target`).
        let pre_narrow_param_type = param_type;
        let mut narrowed_by_discriminant = false;
        for candidate in [
            effective_param_type,
            contextual_param_type,
            evaluated_param_type,
            resolved_param_type,
            lazy_resolved_param_type,
            lazy_evaluated_param_type,
            assignability_param_type,
            lazy_member_param_type,
        ] {
            let narrowed = self.narrow_contextual_union_via_object_literal_discriminants(
                candidate,
                &obj.elements.nodes,
            );
            if narrowed != candidate {
                effective_param_type = narrowed;
                narrowed_by_discriminant = true;
                break;
            }
        }

        // A missing required property does NOT pre-empt per-property elaboration.
        // Like tsc's `elaborateObjectLiteral`, the loop below reports each present
        // property's type mismatch; the missing-property diagnostic (TS2741/TS2739,
        // or the argument-level TS2345 "Property 'X' is missing") is only the
        // fallback the caller emits when this function returns `false` (i.e. no
        // present mismatch was elaborated). The per-property check re-tests the
        // un-widened literal/contextual source type, so a widened literal (`'name'`
        // → `string`) does not produce a spurious mismatch against a `'name'`
        // target — the false positive a prior up-front bail tried to avoid.
        let diagnostics_before_epc = self.ctx.diagnostics.len();
        self.check_object_literal_excess_properties(source_type, effective_param_type, arg_idx);
        // `check_object_literal_excess_properties` can trigger a contextual-type
        // refresh that retains/drops earlier implicit-any diagnostics (see
        // object_literal_support.rs). `recent_diagnostics` clamps its start, so
        // an unrelated shrink cannot panic the slice.
        let had_excess_property = self
            .ctx
            .recent_diagnostics(diagnostics_before_epc)
            .iter()
            .any(|diag| {
                matches!(
                    diag.code,
                    diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
                        | diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_BUT_DOES_NOT_EXIST_IN_TYPE_DID
                )
            });
        if had_excess_property {
            return true;
        }
        if narrowed_by_discriminant && self.target_has_indexed_access_surface(param_type) {
            return false;
        }

        // tsc parity for `getBestMatchIndexedAccessTypeOrUndefined`
        // (`elaborateElementwise`): when the target is a union that contains an
        // array-like member, a fresh object-literal source does NOT resolve its
        // per-property target through any member that happens to expose the key
        // via an index signature. Instead tsc's `findBestTypeForObjectLiteral`
        // selects the first non-array-like member in union order, and the
        // per-property target comes from that member alone. If the best-matching
        // member lacks the key, elaboration is skipped for that property and the
        // outer assignment error is reported.
        let array_union_best_match_member =
            self.object_literal_array_union_best_match_member(effective_param_type);

        let mut elaborated = false;
        let mut seen_named_properties: rustc_hash::FxHashSet<String> =
            rustc_hash::FxHashSet::default();
        let mut duplicate_named_properties: rustc_hash::FxHashSet<String> =
            rustc_hash::FxHashSet::default();
        let mut first_named_property_name_idx: rustc_hash::FxHashMap<String, NodeIndex> =
            rustc_hash::FxHashMap::default();
        let mut last_named_property_value_idx: rustc_hash::FxHashMap<String, NodeIndex> =
            rustc_hash::FxHashMap::default();

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            let (prop_name_idx, prop_value_idx) = match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    match self.ctx.arena.get_property_assignment(elem_node) {
                        Some(prop) => (prop.name, prop.initializer),
                        None => continue,
                    }
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    match self.ctx.arena.get_shorthand_property(elem_node) {
                        Some(prop) => (prop.name, prop.name),
                        None => continue,
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    match self.ctx.arena.get_method_decl(elem_node) {
                        Some(method) => (method.name, elem_idx),
                        None => continue,
                    }
                }
                _ => continue,
            };

            let is_computed_property = self
                .ctx
                .arena
                .get(prop_name_idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME);
            let prop_name = match self.object_literal_property_name_text(prop_name_idx) {
                Some(name) => name,
                None if is_computed_property => {
                    match self.get_property_name_resolved(prop_name_idx) {
                        Some(name) => name,
                        None => continue,
                    }
                }
                None => continue,
            };

            if !seen_named_properties.insert(prop_name.clone()) {
                duplicate_named_properties.insert(prop_name.clone());
            } else {
                first_named_property_name_idx.insert(prop_name.clone(), prop_name_idx);
            }
            last_named_property_value_idx.insert(prop_name, prop_value_idx);
        }

        let mut duplicate_winner_source_types: rustc_hash::FxHashMap<String, TypeId> =
            rustc_hash::FxHashMap::default();
        for (prop_name, &winner_idx) in &last_named_property_value_idx {
            if !duplicate_named_properties.contains(prop_name) {
                continue;
            }
            let winner_ty = self.elaboration_source_expression_type(winner_idx);
            let winner_ty = if winner_ty == TypeId::ERROR || winner_ty == TypeId::ANY {
                self.get_type_of_node(winner_idx)
            } else {
                winner_ty
            };
            if winner_ty != TypeId::ERROR && winner_ty != TypeId::ANY {
                duplicate_winner_source_types.insert(prop_name.clone(), winner_ty);
            }
        }
        let mut emitted_duplicate_primary: rustc_hash::FxHashSet<String> =
            rustc_hash::FxHashSet::default();

        // The object type that DECLARES the members being elaborated. tsc's
        // TS6500 pointer names this type and anchors in its declaration, so the
        // candidates are the target as narrowed for elaboration first, then the
        // target as written — never a property's own type.
        let expected_type_owners = [effective_param_type, param_type];

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Only elaborate regular property assignments, shorthand properties,
            // and method declarations
            let (prop_name_idx, prop_value_idx) = match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    match self.ctx.arena.get_property_assignment(elem_node) {
                        Some(prop) => (prop.name, prop.initializer),
                        None => continue,
                    }
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    match self.ctx.arena.get_shorthand_property(elem_node) {
                        Some(prop) => (prop.name, prop.name),
                        None => continue,
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    match self.ctx.arena.get_method_decl(elem_node) {
                        Some(method) => (method.name, elem_idx),
                        None => continue,
                    }
                }
                _ => continue,
            };

            // Get the property name string.
            // For computed property names (e.g., `[SYM]`), fall back to type-level
            // resolution so unique symbols and const-evaluated keys are resolved.
            let is_computed_property = self
                .ctx
                .arena
                .get(prop_name_idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME);
            // Track whether the computed key is a numeric/string literal (text-resolvable).
            // Symbol-keyed computed properties (`[sym]`, `[Symbol.iterator]`) fall through to
            // `get_property_name_resolved` and must always use TS2418, not TS2322.
            //
            // `object_literal_property_name_text` also resolves a *symbol* computed
            // name (a `unique symbol` const, or a well-known symbol like
            // `Symbol.iterator` via `get_symbol_property_name_from_expr`) to a display
            // string, so its success alone cannot tell a literal-spelled key apart from
            // a symbol one — checked separately with the purely syntactic
            // `is_eagerly_bound_member_name`, which is `true` for a computed name only
            // when its expression is itself a string/numeric/no-substitution-template
            // literal (`["p"]`, `[0]`, `` [`p`] ``), never for an identifier or
            // property-access expression. Oracled against `typescript@7.0.2`:
            // `{ [Symbol.iterator]: string } = { [Symbol.iterator]: 1 }` against a
            // *declared* member reports TS2418, not TS2322 (#16662).
            let mut is_computed_literal_key = false;
            let prop_name = match self.object_literal_property_name_text(prop_name_idx) {
                Some(name) => {
                    if is_computed_property {
                        is_computed_literal_key = self.is_eagerly_bound_member_name(prop_name_idx);
                    }
                    name
                }
                None if is_computed_property => {
                    match self.get_property_name_resolved(prop_name_idx) {
                        Some(name) => name,
                        None => continue,
                    }
                }
                None => continue,
            };
            // A genuine wide/plain `symbol` computed key is an index-signature
            // contributor, not a late-bound named member: it folds into a
            // `[k: symbol]: V` signature, so a value mismatch is owned by the
            // whole-object relation (TS2322 with a `'symbol' index signatures
            // are incompatible` chain), exactly like a wide `string`/`number`
            // key — whose synthetic key name does not resolve, so it is skipped
            // above. A bare `symbol`-typed const binding otherwise resolves to a
            // binding-identity (`__unique_<id>`) name here and would wrongly
            // drill into the late-bound per-property TS2418 below. A syntactic
            // `Symbol.<member>` well-known key keeps its late-bound TS2418 (via
            // the object-literal computation reporter) and is excluded. #16662.
            if is_computed_property
                && !self.computed_member_key_is_well_known_symbol_syntax(prop_name_idx)
                && self.computed_member_key_is_wide_symbol(prop_name_idx)
            {
                continue;
            }
            // Resolve the per-property target against the best-matching union
            // member when one exists (tsc's `findBestTypeForObjectLiteral`),
            // else against the whole target. A `None` here — the best member
            // lacks the key — makes the loop skip this property, so the outer
            // assignment error is reported instead of drilling into the value.
            //
            // When discriminant narrowing selected a member above but EVERY
            // union constituent exposes the key, tsc's per-property target is
            // the indexed access over the FULL union, not the narrowed
            // member's property type (`getBestMatchIndexedAccessTypeOrUndefined`
            // first step) — used for the check, the leaf display, and the
            // nested-literal recursion alike.
            let target_source_type = array_union_best_match_member.unwrap_or(effective_param_type);
            let full_union_target = (narrowed_by_discriminant
                && array_union_best_match_member.is_none())
            .then(|| {
                self.full_union_object_literal_property_target(
                    pre_narrow_param_type,
                    prop_name_idx,
                    &prop_name,
                )
            })
            .flatten();
            let Some((target_prop_type, target_prop_type_for_diagnostic)) = full_union_target
                .or_else(|| {
                    self.object_literal_target_property_type(
                        target_source_type,
                        prop_name_idx,
                        &prop_name,
                    )
                })
            else {
                continue;
            };

            if self.target_has_never_indexed_access_surface(target_prop_type)
                || self.target_has_never_indexed_access_surface(target_prop_type_for_diagnostic)
            {
                continue; // tsc elaborateElementwise: keep TS2322 on outer object for generic indexed-access props
            }

            // Get the type of the property value in the object literal.
            // Use the cached (contextually-typed) type for the assignability check.
            // This preserves literal types that were narrowed by contextual typing
            // (e.g., `value: "hello"` in a mapped type context stays as `"hello"`,
            // not widened to `string`).
            //
            // When the cached type is widened (e.g., `string` for a `'name'` literal)
            // and fails assignability, fall back to the literal type. This avoids
            // spurious TS2322 errors like `Type '"name"' is not assignable to type
            // '"name"'` where the source was widened during arg collection but the
            // target preserves the literal from inference.
            let is_function_value = self.ctx.arena.get(prop_value_idx).is_some_and(|node| {
                matches!(
                    node.kind,
                    syntax_kind_ext::ARROW_FUNCTION
                        | syntax_kind_ext::FUNCTION_EXPRESSION
                        | syntax_kind_ext::METHOD_DECLARATION
                )
            });
            let cached_prop_type = self.get_type_of_node(prop_value_idx);
            // For function-valued properties (especially method declarations),
            // get_type_of_node returns the contextually-typed version which may
            // already incorporate the target's return type. Use the property type
            // from the source object type instead, which reflects the actual
            // (non-contextual) type as seen at the argument level.
            let source_obj_prop_type = if is_function_value {
                let node_source_prop =
                    match self.resolve_property_access_with_env(node_source_type, &prop_name) {
                        tsz_solver::operations::property::PropertyAccessResult::Success {
                            type_id,
                            ..
                        } => Some(type_id),
                        _ => None,
                    };
                let override_source_prop =
                    match self.resolve_property_access_with_env(source_type, &prop_name) {
                        tsz_solver::operations::property::PropertyAccessResult::Success {
                            type_id,
                            ..
                        } => Some(type_id),
                        _ => None,
                    };
                node_source_prop.or(override_source_prop)
            } else {
                None
            };
            let source_prop_type = if is_computed_property
                && !is_function_value
                && let Some(literal_type) = self.literal_type_from_initializer(prop_value_idx)
            {
                literal_type
            } else if !is_function_value
                && cached_prop_type != TypeId::ERROR
                && cached_prop_type != TypeId::ANY
                && target_prop_type != TypeId::ERROR
                && target_prop_type != TypeId::ANY
                && !self
                    .call_arg_relation_outcome(cached_prop_type, target_prop_type)
                    .related
            {
                // If the cached type fails, try the literal type from the initializer.
                // When a generic call widens literals during inference (e.g., `'name'` → string),
                // the literal type may actually be assignable to the inferred target.
                if let Some(literal_type) = self.literal_type_from_initializer(prop_value_idx) {
                    if self
                        .call_arg_relation_outcome(literal_type, target_prop_type)
                        .related
                    {
                        literal_type
                    } else {
                        cached_prop_type
                    }
                } else if self.ctx.arena.get(prop_value_idx).is_some_and(|n| {
                    matches!(
                        n.kind,
                        syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                            | syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    )
                }) {
                    // For nested object/array literal properties, the cached type may
                    // have been widened before the property's contextual type was
                    // available: `{a: 1}` → `{a: number}`, and — the case that
                    // matters for a tuple target — a bare tuple literal `[1, "ok"]`
                    // widens to the array `(number | "ok")[]`, which then fails the
                    // closed-tuple relation with a spurious arity mismatch ("Target
                    // requires N element(s) but source may have fewer"). Re-type the
                    // literal with the target member as context, exactly like tsc's
                    // `elaborateElementwise`, so `[1, "ok"]` becomes `[number, "ok"]`;
                    // if that contextual form relates, the mismatch is not real.
                    // Example: `const x: { pair: [number, "ok"] } = { pair: [1, "ok"],
                    // bad: … }` — the sibling `bad` failure elaborates the object, and
                    // the cached array `pair` must recover its tuple form here.
                    let contextual_request =
                        crate::context::TypingRequest::with_contextual_type(target_prop_type);
                    let contextual_prop_type =
                        self.get_type_of_node_with_request(prop_value_idx, &contextual_request);
                    if contextual_prop_type != TypeId::ERROR
                        && contextual_prop_type != TypeId::ANY
                        && (self
                            .call_arg_relation_outcome(contextual_prop_type, target_prop_type)
                            .related
                            || self.contextual_tuple_recovers_elementwise_failure(
                                cached_prop_type,
                                contextual_prop_type,
                                target_prop_type,
                            ))
                    {
                        contextual_prop_type
                    } else {
                        cached_prop_type
                    }
                } else {
                    cached_prop_type
                }
            } else {
                cached_prop_type
            };

            // For function values, emit TS2322 at the property level when there's a type mismatch.
            // This applies to both optional and required function properties.
            // Use the source object property type (from the argument-level type) if available,
            // since get_type_of_node on method declarations may return the contextually-typed
            // version that doesn't reflect the actual mismatch.
            let duplicate_winner_source_prop =
                duplicate_winner_source_types.get(&prop_name).copied();
            let is_last_duplicate_value = last_named_property_value_idx
                .get(&prop_name)
                .is_some_and(|&winner_idx| winner_idx == prop_value_idx);

            let effective_source_prop = duplicate_winner_source_prop
                .or(source_obj_prop_type)
                .unwrap_or(source_prop_type);
            if is_function_value
                && duplicate_named_properties.contains(&prop_name)
                && target_prop_type != TypeId::ERROR
                && target_prop_type != TypeId::ANY
            {
                let duplicate_source_for_check = duplicate_winner_source_prop
                    .or_else(|| is_last_duplicate_value.then_some(source_prop_type));
                if let Some(duplicate_source_for_check) = duplicate_source_for_check
                    && duplicate_source_for_check != TypeId::ERROR
                    && duplicate_source_for_check != TypeId::ANY
                    && !self
                        .call_arg_relation_outcome(duplicate_source_for_check, target_prop_type)
                        .related
                {
                    let source_prop_type_for_diagnostic =
                        crate::query_boundaries::assignability::rewrite_function_error_slots_to_any(
                            self.ctx.types,
                            self.widen_function_like_call_source(duplicate_source_for_check),
                        );
                    let target_for_diag = if target_prop_type != target_prop_type_for_diagnostic {
                        target_prop_type_for_diagnostic
                    } else {
                        target_prop_type
                    };
                    let target_for_diag =
                        self.present_callable_property_target_display_type(target_for_diag);
                    if let Some(&first_name_idx) = first_named_property_name_idx.get(&prop_name)
                        && first_name_idx != prop_name_idx
                        && emitted_duplicate_primary.insert(prop_name.clone())
                    {
                        self.error_type_not_assignable_at_with_display_types(
                            source_prop_type_for_diagnostic,
                            target_for_diag,
                            first_name_idx,
                        );
                    }
                    self.with_expected_type_from_property_pointer(
                        &expected_type_owners,
                        &prop_name,
                        prop_value_idx,
                        |this| {
                            this.error_type_not_assignable_at_with_display_types(
                                source_prop_type_for_diagnostic,
                                target_for_diag,
                                prop_name_idx,
                            );
                        },
                    );
                    elaborated = true;
                    continue;
                }
            }
            if is_function_value
                && effective_source_prop != TypeId::ERROR
                && effective_source_prop != TypeId::ANY
                && target_prop_type != TypeId::ERROR
                && target_prop_type != TypeId::ANY
                && !self
                    .call_arg_relation_outcome(effective_source_prop, target_prop_type)
                    .related
            {
                let source_prop_type_for_diagnostic =
                    crate::query_boundaries::assignability::rewrite_function_error_slots_to_any(
                        self.ctx.types,
                        self.widen_function_like_call_source(effective_source_prop),
                    );
                // Use the diagnostic target type if available (for optional properties),
                // otherwise use the effective target type
                let target_for_diag = if overall_target_is_union {
                    if let (Some(non_nullish), Some(_)) = self.split_nullish_type(target_prop_type)
                    {
                        non_nullish
                    } else {
                        target_prop_type_for_diagnostic
                    }
                } else {
                    target_prop_type_for_diagnostic
                };
                let target_for_diag =
                    self.present_callable_property_target_display_type(target_for_diag);
                if duplicate_named_properties.contains(&prop_name) {
                    if let Some(&first_name_idx) = first_named_property_name_idx.get(&prop_name)
                        && first_name_idx != prop_name_idx
                        && emitted_duplicate_primary.insert(prop_name.clone())
                    {
                        self.error_type_not_assignable_at_with_display_types(
                            source_prop_type_for_diagnostic,
                            target_for_diag,
                            first_name_idx,
                        );
                    }
                    // Keep the source/target display types stable for duplicate
                    // properties; anchor at the property name so both duplicate
                    // declarations can surface their own TS2322 positions.
                    self.with_expected_type_from_property_pointer(
                        &expected_type_owners,
                        &prop_name,
                        prop_value_idx,
                        |this| {
                            this.error_type_not_assignable_at_with_display_types(
                                source_prop_type_for_diagnostic,
                                target_for_diag,
                                prop_name_idx,
                            );
                        },
                    );
                    elaborated = true;
                    continue;
                }
                // Shorthand method members (`{ m(x: string) {} }`) elaborate the
                // same way tsc does for property-arrow members
                // (`{ m: (x: string) => {} }`): route through the canonical
                // relation -> reason -> diagnostic boundary so the TS2322 carries
                // the nested elaboration chain (e.g. "Types of parameters 'x' and
                // 'x' are incompatible.").
                //
                // The anchor is the method name; `assignment_source_expression`
                // resolves a method-name anchor to the method declaration itself,
                // so the source display is the method's call signature and the
                // method name is not resolved as a value reference (which would
                // otherwise surface a spurious "Cannot find name").
                let is_method = self
                    .ctx
                    .arena
                    .get(prop_value_idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::METHOD_DECLARATION);
                if is_method {
                    self.with_expected_type_from_property_pointer(
                        &expected_type_owners,
                        &prop_name,
                        prop_value_idx,
                        |this| {
                            this.error_type_not_assignable_at_with_anchor(
                                source_prop_type_for_diagnostic,
                                target_for_diag,
                                prop_name_idx,
                            );
                        },
                    );
                } else {
                    // For arrow/function expression property values, try deeper
                    // elaboration first. tsc's elaborateElementwise recurses
                    // into function return expressions so the error points at
                    // the body expression (e.g., `"hello"` in `b: () => "hello"`)
                    // rather than the property name. Unlike the callback argument
                    // path (try_elaborate_function_arg_return_error), the property
                    // context reports the return type mismatch, not the full
                    // function type mismatch.
                    let elaborated_body = (|| {
                        let func_node = self.ctx.arena.get(prop_value_idx)?;
                        let func = self.ctx.arena.get_function(func_node)?;
                        // `tsc`'s `elaborateArrowFunction` bails (no body-return
                        // drill) when any parameter carries an explicit type
                        // annotation, reporting the function-type frame instead.
                        // Without this gate, an annotated-parameter property arrow
                        // whose body return also mismatches anchors the TS2322 at
                        // the body expression and drops the parameter-contravariance
                        // frame that `tsc` emits at the property.
                        if self.function_value_has_explicit_param_annotation(prop_value_idx) {
                            return None;
                        }
                        let expected_ret = self.callable_return_type_for_drill(target_prop_type)?;
                        if expected_ret == TypeId::VOID || expected_ret == TypeId::ANY {
                            return None;
                        }
                        let body_node = self.ctx.arena.get(func.body)?;
                        // Only expression-bodied arrows (not block bodies)
                        if body_node.kind == syntax_kind_ext::BLOCK {
                            return None;
                        }
                        let body_type = self.get_type_of_node(func.body);
                        if body_type == TypeId::ERROR
                            || body_type == TypeId::ANY
                            || self
                                .return_relation_outcome(body_type, expected_ret)
                                .related
                        {
                            return None;
                        }
                        Some((body_type, expected_ret, func.body))
                    })();
                    if let Some((body_type, expected_ret, body_idx)) = elaborated_body {
                        // When the body already has a TS2322 diagnostic (from
                        // contextual return type checking in function_type.rs),
                        // skip emitting a redundant parent-level error. tsc only
                        // emits the leaf-level property errors, not the parent
                        // "Type X is not assignable to Type Y" with "Types of
                        // property are incompatible" related info.
                        if let Some(body_node) = self.ctx.arena.get(body_idx)
                            && self.has_diagnostic_code_within_span(
                                body_node.pos,
                                body_node.end,
                                tsz_common::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                            ) {
                                // The body's own contextual return check already
                                // reported here, so this frame owns no emit of
                                // its own — but it is still the frame tsc
                                // attaches TS6502 to, and the pointer's owner
                                // is only known at this level.
                                self.attach_expected_type_from_return_pointer(
                                    0,
                                    &expected_type_owners,
                                    &prop_name,
                                    body_idx,
                                    prop_name_idx,
                                );
                                elaborated = true;
                                continue;
                            }
                        // Try deeper elaboration into the body expression
                        // (e.g., object literal properties) before falling back
                        // to the parent-level error.
                        if self.try_elaborate_assignment_source_error(body_idx, expected_ret) {
                            elaborated = true;
                            continue;
                        }
                        let before = self.ctx.diagnostics.len();
                        self.error_type_not_assignable_at_with_anchor(
                            body_type,
                            expected_ret,
                            body_idx,
                        );
                        self.attach_expected_type_from_return_pointer(
                            before,
                            &expected_type_owners,
                            &prop_name,
                            body_idx,
                            prop_name_idx,
                        );
                    } else {
                        self.with_expected_type_from_property_pointer(
                            &expected_type_owners,
                            &prop_name,
                            prop_value_idx,
                            |this| {
                                this.error_type_not_assignable_at_with_anchor(
                                    source_prop_type_for_diagnostic,
                                    target_for_diag,
                                    prop_name_idx,
                                );
                            },
                        );
                    }
                }
                elaborated = true;
                continue;
            }

            // Only try to elaborate sub-expression errors when the property value
            // is NOT assignable to the target. Without this guard, elaboration can
            // produce false-positive TS2322 errors on nested elements (e.g., array
            // literal elements) even when the overall property type is compatible.
            //
            // Peel through parenthesized and comma/assignment expression wrappers
            // before checking the inner expression kind. tsc's elaborateElementwise
            // walks past these wrappers so `a: q = ({ ... })` still drills into the
            // trailing object literal for per-property elaboration.
            let unwrapped_prop_value_idx = {
                let mut current = prop_value_idx;
                for _ in 0..16 {
                    let Some(node) = self.ctx.arena.get(current) else {
                        break;
                    };
                    if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                        && let Some(paren) = self.ctx.arena.get_parenthesized(node)
                    {
                        current = paren.expression;
                        continue;
                    }
                    if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                        && let Some(bin) = self.ctx.arena.get_binary_expr(node)
                        && (bin.operator_token == SyntaxKind::CommaToken as u16
                            || bin.operator_token == SyntaxKind::EqualsToken as u16)
                    {
                        current = bin.right;
                        continue;
                    }
                    break;
                }
                current
            };
            if source_prop_type != TypeId::ERROR
                && source_prop_type != TypeId::ANY
                && target_prop_type != TypeId::ERROR
                && target_prop_type != TypeId::ANY
                && !self
                    .call_arg_relation_outcome(source_prop_type, target_prop_type)
                    .related
                && self
                    .ctx
                    .arena
                    .get(unwrapped_prop_value_idx)
                    .is_some_and(|node| {
                        matches!(
                            node.kind,
                            syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                | syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                | syntax_kind_ext::ARROW_FUNCTION
                                | syntax_kind_ext::FUNCTION_EXPRESSION
                                | syntax_kind_ext::CONDITIONAL_EXPRESSION
                        )
                    })
                && self.try_elaborate_assignment_source_error(
                    unwrapped_prop_value_idx,
                    target_prop_type,
                )
            {
                elaborated = true;
                continue;
            }
            if self.object_literal_numeric_members_assign_to_mapped_target(
                unwrapped_prop_value_idx,
                target_prop_type,
            ) {
                elaborated = true;
                continue;
            }

            // Skip if types are unresolved
            if source_prop_type == TypeId::ERROR
                || source_prop_type == TypeId::ANY
                || target_prop_type == TypeId::ERROR
                || target_prop_type == TypeId::ANY
            {
                continue;
            }

            // Check if the property value type is assignable to the target property type
            let prop_assignable = self
                .call_arg_relation_outcome(source_prop_type, target_prop_type)
                .related;
            if !prop_assignable {
                if self.try_elaborate_assignment_source_error(prop_value_idx, target_prop_type) {
                    elaborated = true;
                    continue;
                }

                // TS2820: before emitting generic TS2322, check if the property
                // value is a string literal that is a near-miss of a target union
                // member. Use the AST literal type (not the widened source_prop_type)
                // so that `"hdpvd"` is compared against `"hddvd" | "bluray"`.
                if let Some(literal_source_type) =
                    self.literal_type_from_initializer(prop_value_idx)
                {
                    let evaluated_target =
                        self.evaluate_type_with_env(target_prop_type_for_diagnostic);
                    if let Some(suggestion) = self
                        .find_string_literal_spelling_suggestion(
                            literal_source_type,
                            target_prop_type,
                        )
                        .or_else(|| {
                            self.find_string_literal_spelling_suggestion(
                                literal_source_type,
                                evaluated_target,
                            )
                        })
                    {
                        let src_str = self.format_type_diagnostic(literal_source_type);
                        let tgt_str = self
                            .format_type_for_assignability_message(target_prop_type_for_diagnostic);
                        let display_target = self.format_ts2820_target_display(
                            target_prop_type_for_diagnostic,
                            evaluated_target,
                            &tgt_str,
                        );
                        let msg = format_message(
                            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_DID_YOU_MEAN,
                            &[&src_str, &display_target, &suggestion],
                        );
                        let anchor_idx = self.resolve_diagnostic_anchor_node(
                            prop_name_idx,
                            DiagnosticAnchorKind::Exact,
                        );
                        if let Some(anchor) =
                            self.resolve_diagnostic_anchor(anchor_idx, DiagnosticAnchorKind::Exact)
                        {
                            self.ctx
                                .push_diagnostic(crate::diagnostics::Diagnostic::error(
                                    self.ctx.file_name.clone(),
                                    anchor.start,
                                    anchor.length,
                                    msg,
                                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_DID_YOU_MEAN,
                                ));
                        }
                        elaborated = true;
                        continue;
                    }
                }

                // TS2418 applies when:
                //   (a) the key is computed AND late-bound — an identifier over
                //       a `const`, an enum member, or a symbol reference — since
                //       tsc's `isComputedNonLiteralName` is false for a
                //       literal-spelled computed name (`["x"]`, `[0]`,
                //       `` [`x`] ``), which is an ordinary property name and
                //       takes TS2322/TS2353 no matter how the target matches it,
                //       AND
                //   (b) the late-bound key has no named property in the target
                //       (it matches only via an index signature), since a
                //       late-bound key resolving to a named member takes TS2322.
                let key_is_late_bound = is_computed_property
                    && !self.computed_member_name_is_literal_spelled(prop_name_idx);
                let target_has_named_member_for_key = is_computed_property
                    && self.target_has_named_property_for_key(effective_param_type, &prop_name);
                // A *missing-property* failure keeps its own code even when the
                // member was written with a computed key. `tsc` runs
                // `reportUnmatchedProperty` on the property relation itself and
                // only a failure that is not missing-property falls back to the
                // computed-property message, so the discriminator is the shape
                // of the relation failure — never the kind of the key. Measured
                // against `typescript@7.0.2` across every key kind that reaches
                // here (late-bound `const` string, numeric `const`, enum member,
                // `unique symbol`, well-known symbol): all report TS2741/TS2739.
                // The negative control is a union-typed target member, whose
                // failure is not missing-property and which keeps TS2418.
                let computed_failure_is_missing_property = is_computed_property
                    && target_has_named_member_for_key
                    && matches!(
                        self.call_arg_relation_outcome(source_prop_type, target_prop_type)
                            .failure,
                        Some(
                            RelationFailure::MissingProperty { .. }
                                | RelationFailure::MissingProperties { .. }
                        )
                    );
                if key_is_late_bound
                    && !(is_computed_literal_key && target_has_named_member_for_key)
                    && !computed_failure_is_missing_property
                {
                    // `tsc` widens a computed-property value in `TS2418` only when
                    // the key resolves through an index signature (the contextual
                    // object-literal and union-index reporters do the same). When
                    // the key matches a declared named member — e.g. a
                    // `unique symbol` property whose declared type is a literal —
                    // the fresh literal type is preserved instead.
                    //
                    // "Declared type is a literal" is the operative half, and it
                    // is a property of the *target member type*, not of the key:
                    // this is ordinary object-literal freshness, where a fresh
                    // literal widens unless the contextual type is literal-
                    // bearing. Oracled on `typescript@7.0.2`: a `unique symbol`
                    // key against `{ [S]: 1 }` keeps `'2'`, and the same key
                    // against `{ [S]: number }` widens to `'string'` — as does a
                    // late-bound `const` string key against `{ inner: number }`.
                    // Keyed on the target so both key kinds get one rule.
                    let target_member_is_literal_bearing = self
                        .computed_member_target_is_literal_bearing(target_prop_type_for_diagnostic);
                    let computed_source =
                        if target_has_named_member_for_key && target_member_is_literal_bearing {
                            self.expression_display_type_preferring_literal(
                                prop_value_idx,
                                source_prop_type,
                            )
                        } else {
                            let widened = self
                                .literal_type_from_initializer(prop_value_idx)
                                .unwrap_or(source_prop_type);
                            self.widen_literal_type(widened)
                        };
                    let src_str = self.format_type_for_assignability_message(computed_source);
                    let tgt_str =
                        self.format_type_for_assignability_message(target_prop_type_for_diagnostic);
                    let msg = format_message(
                        diagnostic_messages::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&src_str, &tgt_str],
                    );
                    self.error_at_node(
                        prop_name_idx,
                        &msg,
                        diagnostic_codes::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                } else {
                    if self.try_emit_property_weak_type_violation(
                        source_prop_type,
                        target_prop_type,
                        target_prop_type_for_diagnostic,
                        prop_value_idx,
                        prop_name_idx,
                    ) {
                        elaborated = true;
                        continue;
                    }
                    let source_prop_type_for_diagnostic =
                        self.widen_expression_type_if_fresh(prop_value_idx, source_prop_type);
                    let source_prop_type_for_diagnostic =
                        self.widen_function_like_call_source(source_prop_type_for_diagnostic);
                    // TSC's elaborateElementwise uses TS2322 ("Type X is not
                    // assignable to type Y") for `this` keyword property values
                    // instead of the more specific TS2741 missing-property code.
                    // The `this` type represents the class instance which may have
                    // extra members beyond the target interface, making the general
                    // TS2322 message more appropriate than enumerating missing props.
                    let value_is_this_keyword = self
                        .ctx
                        .arena
                        .get(prop_value_idx)
                        .is_some_and(|n| n.kind == SyntaxKind::ThisKeyword as u16);
                    // tsc's `elaborateDidYouMeanToCallOrConstruct` anchors
                    // missing-property codes (TS2741/TS2739/TS2740) on the
                    // property initializer when the initializer is a bare
                    // identifier whose type has call/construct signatures —
                    // so the "Did you mean to use 'new'/call this expression"
                    // related hint and the primary diagnostic both point at
                    // the identifier value. For plain variable references or
                    // other shapes, tsc keeps the anchor on the property name.
                    let value_is_bare_identifier = self
                        .ctx
                        .arena
                        .get(prop_value_idx)
                        .is_some_and(|n| n.kind == SyntaxKind::Identifier as u16);
                    let value_is_callable_or_constructor = value_is_bare_identifier
                        && source_prop_type != TypeId::ERROR
                        && source_prop_type != TypeId::ANY
                        && (crate::query_boundaries::common::has_call_signatures(
                            self.ctx.types,
                            source_prop_type,
                        ) || crate::query_boundaries::common::has_construct_signatures(
                            self.ctx.types,
                            source_prop_type,
                        ));
                    let value_anchor_for_missing_props = if elem_node.kind
                        == syntax_kind_ext::PROPERTY_ASSIGNMENT
                        && prop_value_idx != prop_name_idx
                        && !value_is_this_keyword
                        && value_is_callable_or_constructor
                    {
                        Some(prop_value_idx)
                    } else {
                        None
                    };
                    if target_prop_type != target_prop_type_for_diagnostic {
                        self.with_expected_type_from_property_pointer(
                            &expected_type_owners,
                            &prop_name,
                            prop_value_idx,
                            |this| {
                                this.error_type_not_assignable_at_with_display_types(
                                    source_prop_type_for_diagnostic,
                                    target_prop_type_for_diagnostic,
                                    prop_name_idx,
                                );
                            },
                        );
                    } else {
                        self.with_expected_type_from_property_pointer(
                            &expected_type_owners,
                            &prop_name,
                            prop_value_idx,
                            |this| {
                                this.error_type_not_assignable_at_with_anchor_elaboration_inner_with_value_anchor(
                                    source_prop_type_for_diagnostic,
                                    target_prop_type_for_diagnostic,
                                    prop_name_idx,
                                    value_anchor_for_missing_props,
                                    value_is_this_keyword,
                                );
                            },
                        );
                    }
                }
                elaborated = true;
            } else if self.emit_polymorphic_this_property_assignment_error(
                source_prop_type,
                target_prop_type,
                prop_name_idx,
            ) {
                elaborated = true;
                continue;
            }
        }

        // When the object literal has properties that all matched the target (elaborated
        // == false), but the only missing properties are Object.prototype methods
        // (valueOf, toString, etc.), suppress the error — those methods are implicitly
        // present from Object.prototype. However, only suppress when the source actually
        // HAS properties; an empty object literal `{}` has no properties to satisfy the
        // target, so the structural mismatch is real and should produce TS2322/TS2345.
        if !elaborated
            && !obj.elements.nodes.is_empty()
            && self.should_suppress_object_literal_call_mismatch(source_type, effective_param_type)
        {
            return true;
        }

        elaborated
    }

    fn object_literal_numeric_members_assign_to_mapped_target(
        &mut self,
        object_idx: NodeIndex,
        target_type: TypeId,
    ) -> bool {
        if !self.type_has_mapped_alias_surface(target_type, 0) {
            return false;
        }
        let Some(node) = self.ctx.arena.get(object_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(obj) = self.ctx.arena.get_literal_expr(node).cloned() else {
            return false;
        };
        if obj.elements.nodes.is_empty() {
            return false;
        }

        let mut saw_numeric_member = false;
        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                return false;
            };
            let (prop_name_idx, prop_value_idx) = match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) else {
                        return false;
                    };
                    (prop.name, prop.initializer)
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.ctx.arena.get_shorthand_property(elem_node) else {
                        return false;
                    };
                    (prop.name, prop.name)
                }
                _ => return false,
            };

            let Some(prop_name) = self.object_literal_property_name_text(prop_name_idx) else {
                return false;
            };
            if !tsz_solver::utils::is_numeric_literal_name(&prop_name) {
                return false;
            }
            saw_numeric_member = true;

            let Some((target_member_type, _)) =
                self.object_literal_target_property_type(target_type, prop_name_idx, &prop_name)
            else {
                return false;
            };
            if target_member_type == TypeId::ERROR || target_member_type == TypeId::ANY {
                continue;
            }

            let source_member_type = self.get_type_of_node(prop_value_idx);
            if source_member_type == TypeId::ERROR || source_member_type == TypeId::ANY {
                continue;
            }
            if !self
                .call_arg_relation_outcome(source_member_type, target_member_type)
                .related
            {
                return false;
            }
        }

        saw_numeric_member
    }

    fn type_has_mapped_alias_surface(&mut self, target_type: TypeId, depth: usize) -> bool {
        if depth > 8 {
            return false;
        }
        if query_common::mapped_type_id(self.ctx.types, target_type).is_some() {
            return true;
        }
        if let Some(members) = query_common::union_members(self.ctx.types, target_type) {
            return members
                .iter()
                .copied()
                .filter(|&member| member != TypeId::NULL && member != TypeId::UNDEFINED)
                .any(|member| self.type_has_mapped_alias_surface(member, depth + 1));
        }
        if let Some((base, _args)) = query_common::application_info(self.ctx.types, target_type)
            && let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(base)
        {
            let (body_type, _type_params) = self.type_reference_symbol_type_with_params(sym_id);
            return self.type_has_mapped_alias_surface(body_type, depth + 1);
        }
        false
    }

    /// tsc's `findBestTypeForObjectLiteral` (the object-literal branch of
    /// `getBestMatchingType`, used by `getBestMatchIndexedAccessTypeOrUndefined`):
    /// when a fresh object-literal source is related to a union target that has
    /// an array-like member, the best-matching member for per-property
    /// elaboration is the first non-array-like member in union order. Returns
    /// that member, or `None` when the target does not resolve to such a union
    /// (no array-like member, or not a union at all).
    ///
    /// This is a deliberate narrowing of tsc's full `getBestMatchingType`
    /// (which also scores members by discriminant/property overlap): the
    /// array-like branch is the one that governs the recursive-JSON-alias shape
    /// this gate targets (`… | Json[] | { [k: string]: Json }`), where the
    /// leading primitive/object member is selected. Array-like = `T[]`, tuple,
    /// or `readonly T[]` (tsc's `isArrayLikeType`).
    pub(crate) fn object_literal_array_union_best_match_member(
        &mut self,
        param_type: TypeId,
    ) -> Option<TypeId> {
        use crate::query_boundaries::type_checking_utilities::{
            ArrayLikeKind, classify_array_like,
        };

        let resolved = self.resolve_type_for_property_access(param_type);
        let evaluated = self.judge_evaluate(resolved);
        for candidate in [param_type, resolved, evaluated] {
            let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, candidate)
            else {
                continue;
            };
            let db = self.ctx.types.as_type_database();
            let is_array_like = |member: TypeId| {
                matches!(
                    classify_array_like(db, member),
                    ArrayLikeKind::Array(_) | ArrayLikeKind::Tuple | ArrayLikeKind::Readonly(_)
                )
            };
            if !members.iter().any(|&member| is_array_like(member)) {
                continue;
            }
            return members
                .iter()
                .copied()
                .find(|&member| !is_array_like(member));
        }
        None
    }

    fn target_has_never_indexed_access_surface(&self, target_type: TypeId) -> bool {
        crate::query_boundaries::diagnostics::contains_never_index_access_surface(
            self.ctx.types.as_type_database(),
            &self.ctx.definition_store,
            target_type,
            8,
        )
    }

    fn target_has_indexed_access_surface(&self, target_type: TypeId) -> bool {
        self.type_has_indexed_access_surface(target_type, 0)
    }

    /// `true` when any shape reachable from `target_type` has a named property
    /// (not an index signature) whose atom equals `prop_name`.
    fn target_has_named_property_for_key(&mut self, target_type: TypeId, prop_name: &str) -> bool {
        let prop_atom = self.ctx.types.intern_string(prop_name);
        let resolved = self.resolve_type_for_property_access(target_type);
        let evaluated = self.evaluate_type_with_env(target_type);
        let has_named = |type_id: TypeId| {
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)
                .is_some_and(|shape| shape.properties.iter().any(|p| p.name == prop_atom))
        };
        [target_type, resolved, evaluated]
            .into_iter()
            .any(|candidate| {
                crate::query_boundaries::common::union_members(self.ctx.types, candidate)
                    .map_or_else(
                        || has_named(candidate),
                        |ms| ms.iter().copied().any(has_named),
                    )
            })
    }

    /// `true` when `target_type` is a literal type, or a union any member of
    /// which is, so a fresh literal written under it keeps its literal type
    /// instead of widening. This is the contextual half of ordinary
    /// object-literal freshness, asked of the computed member's *target* type.
    fn computed_member_target_is_literal_bearing(&mut self, target_type: TypeId) -> bool {
        let evaluated = self.evaluate_type_with_env(target_type);
        let is_literal = |type_id: TypeId| {
            crate::query_boundaries::common::is_literal_type(self.ctx.types, type_id)
        };
        [target_type, evaluated].into_iter().any(|candidate| {
            crate::query_boundaries::common::union_members(self.ctx.types, candidate).map_or_else(
                || is_literal(candidate),
                |ms| ms.iter().copied().any(is_literal),
            )
        })
    }

    fn type_has_indexed_access_surface(&self, target_type: TypeId, depth: usize) -> bool {
        if depth > 8 {
            return false;
        }
        let db = self.ctx.types.as_type_database();
        if crate::query_boundaries::common::index_access_types(db, target_type).is_some() {
            return true;
        }
        if let Some(members) = crate::query_boundaries::common::union_members(db, target_type)
            && members
                .iter()
                .any(|&member| self.type_has_indexed_access_surface(member, depth + 1))
        {
            return true;
        }
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(db, target_type)
            && members
                .iter()
                .any(|&member| self.type_has_indexed_access_surface(member, depth + 1))
        {
            return true;
        }
        if crate::query_boundaries::common::is_generic_application(self.ctx.types, target_type)
            && let Some(def_id) = crate::query_boundaries::common::get_application_lazy_def_id(
                self.ctx.types,
                target_type,
            )
            && let Some(def) = self.ctx.definition_store.get(def_id)
            && def.kind == tsz_solver::def::DefKind::TypeAlias
            && let Some(body) = def.body
        {
            return self.type_has_indexed_access_surface(body, depth + 1);
        }

        false
    }

    /// Elaborate array literal element type mismatches with TS2322.
    pub(super) fn try_elaborate_array_literal_elements(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        // When the target type is `never`, don't elaborate into element-level TS2322 errors.
        if param_type == TypeId::NEVER {
            return false;
        }

        let effective_param_type = self.evaluate_type_with_env(param_type);
        let effective_param_type = self.resolve_type_for_property_access(effective_param_type);
        let effective_param_type = self.resolve_lazy_type(effective_param_type);
        let effective_param_type = self.evaluate_application_type(effective_param_type);

        let arg_node = match self.ctx.arena.get(arg_idx) {
            Some(node) if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => node,
            _ => return false,
        };

        let arr = match self.ctx.arena.get_literal_expr(arg_node) {
            Some(arr) => arr.clone(),
            None => return false,
        };
        // When the call argument targets a generic parameter, normally we skip
        // element-wise elaboration because the parameter type still contains
        // unresolved type parameters. However, when the resolved target type
        // (e.g., a constraint substituted in for a violated type parameter)
        // is fully concrete, elaboration is safe and matches tsc's behavior
        // of pointing at the offending element with TS2322.
        if self.call_argument_targets_generic_parameter(arg_idx) {
            let db = self.ctx.types.as_type_database();
            let target_unresolved =
                crate::query_boundaries::common::contains_type_parameters(db, effective_param_type)
                    || crate::query_boundaries::common::contains_infer_types(
                        db,
                        effective_param_type,
                    );
            if target_unresolved {
                return false;
            }
        }

        let ctx_helper = ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            effective_param_type,
            self.ctx.compiler_options.no_implicit_any,
        );
        let tuple_target_elements =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, effective_param_type);

        // tsc's `checkArrayLiteral` carries a cumulative `hasOmittedExpression`
        // flag: once an elision precedes a present element, `elaborateArrayLiteral`
        // compares that element's *widened, `| undefined`-unioned* source type
        // (`addOptionality(widen(type), /*isProperty*/ true, true)`) against the
        // target slot (issue #15612). tsz models the hole as absence (`never?`)
        // rather than a distinct `missingType`, so the source tuple keeps the bare
        // element type and the whole-tuple relation reports a coarse message. Only
        // failing assignments reach this elaborator, so recovering the widening
        // here — for a present element after an elision, in a tuple context, under
        // `exactOptionalPropertyTypes` — drills to `T | undefined` vs the required
        // target slot without perturbing the (already-correct) assignability
        // decision. The optional-target case never fails the relation, so it never
        // reaches this path and stays green.
        let elision_widening_applies = self.ctx.compiler_options.exact_optional_property_types
            && tuple_target_elements.is_some();
        let mut saw_elision = false;

        // For rest-tuple targets (e.g. `[number, ...string[]]` or
        // `[number, ...string[], number]`), element positions can only be
        // reliably matched against the leading fixed section. Rest-covered and
        // trailing fixed positions have no fixed source index, so tsc emits
        // element-level errors only for leading fixed failures and falls back
        // to the whole-tuple relation (`Type at position(s) … in source …`)
        // for the rest. Cap element drill-in to the leading fixed count.
        let max_elaborate_index: Option<usize> =
            tuple_target_elements.as_deref().and_then(|elements| {
                crate::query_boundaries::common::tuple_leading_fixed_drill_cap(elements)
            });

        // tsc's `elaborateElementwise` reports an unelaborated element mismatch
        // with the *source tuple's* element type
        // (`getIndexedAccessTypeOrUndefined(source, getNumberLiteralType(i))`,
        // which reduces to the fixed slot's type), not a context-free retype
        // of the element expression. Recover that tuple once; element
        // index i maps to slot i only when the literal is hole/spread-free
        // (equal lengths, all fixed slots — checked per element below).
        let source_tuple_elements = crate::query_boundaries::common::tuple_elements(
            self.ctx.types,
            self.get_type_of_node(arg_idx),
        )
        .filter(|elements| elements.len() == arr.elements.nodes.len());

        let mut elaborated = false;

        for (index, &elem_idx) in arr.elements.nodes.iter().enumerate() {
            if let Some(max_idx) = max_elaborate_index
                && index >= max_idx
            {
                break;
            }
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                // Elided slot (a "hole"): tsc's `elaborateArrayLiteral` skips it
                // (no expression to anchor) but records that later present
                // elements are now optional-with-`| undefined` (issue #15612).
                saw_elision = true;
                continue;
            };

            // Spread elements: when the target is a plain array element type
            // (not a tuple), check the spread's iterated element type against
            // the contextual element type. This matches tsc's behavior of
            // reporting `Type 'X' is not assignable to type 'Y'` at the
            // spread expression for cases like
            //   `var arr: number[] = [0, 1, ...new SymbolIterator]`.
            //
            // Custom array subtypes (e.g., `interface Foo extends Array<T>`)
            // keep tsc's whole-assignment TS2322; we only handle plain
            // `T[]` / `readonly T[]` here.
            if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                if tuple_target_elements.is_some() {
                    continue;
                }
                if !self.target_is_plain_array_for_spread_elaboration(effective_param_type) {
                    continue;
                }
                if let Some(target_element_type) =
                    ctx_helper.get_array_element_type().or_else(|| {
                        crate::query_boundaries::common::array_element_type(
                            self.ctx.types,
                            effective_param_type,
                        )
                    })
                    && self
                        .try_elaborate_spread_element_array_mismatch(elem_idx, target_element_type)
                {
                    elaborated = true;
                }
                continue;
            }

            // Get the expected element type from the parameter array/tuple type
            let target_element_type = if let Some(elements) = tuple_target_elements.as_deref() {
                let Some(t) = self.elaboration_tuple_element_type_at(elements, index) else {
                    continue;
                };
                t
            } else if let Some(t) = ctx_helper.get_tuple_element_type(index) {
                t
            } else if let Some(t) = ctx_helper.get_array_element_type() {
                t
            } else if let Some(t) = crate::query_boundaries::common::array_element_type(
                self.ctx.types,
                effective_param_type,
            ) {
                t
            } else {
                continue;
            };
            // For diagnostic display, use the unevaluated element type from
            // `param_type` (the declared array type) rather than `target_element_type`
            // (which comes from `effective_param_type` after `evaluate_type_with_env`
            // has resolved `keyof T` to the constraint's keys union `"a" | "b"`).
            // tsc preserves `keyof T` in TS2322 messages for free type parameters,
            // so we anchor on the raw element type stored in the declared array type.
            let display_target_element_type = if tuple_target_elements.is_none() {
                crate::query_boundaries::common::array_element_type(self.ctx.types, param_type)
                    .unwrap_or(target_element_type)
            } else {
                target_element_type
            };

            // Present element following an elision under `exactOptionalPropertyTypes`
            // in a tuple context: widen its type and union `undefined`, mirroring
            // tsc's `addOptionality(widen(type), /*isProperty*/ true, true)`
            // (issue #15612). When that `T | undefined` is exactly what the target
            // slot rejects — a *required* target element — report it at this
            // element. An optional target slot would make the whole assignment
            // succeed, so this elaborator never runs for it.
            if elision_widening_applies
                && saw_elision
                && elem_node.kind != syntax_kind_ext::SPREAD_ELEMENT
            {
                let widened = crate::query_boundaries::common::widen_literal_type(
                    self.ctx.types,
                    self.elaboration_source_expression_type(elem_idx),
                );
                let src_elem_type = self.ctx.types.union2(widened, TypeId::UNDEFINED);
                if src_elem_type != target_element_type
                    && target_element_type != TypeId::ERROR
                    && target_element_type != TypeId::ANY
                    && !self
                        .call_arg_relation_outcome(src_elem_type, target_element_type)
                        .related
                {
                    // Render the widened source (`T | undefined`) and the target
                    // slot verbatim, anchored at the element. The role-based
                    // emitters re-derive an `AssignmentSource` from the enclosing
                    // assignment's RHS (surfacing the whole source tuple), which
                    // would defeat the per-element drill; the raw emitter keeps
                    // the element types.
                    self.error_type_not_assignable_at_with_raw_display_types(
                        src_elem_type,
                        display_target_element_type,
                        elem_idx,
                    );
                    elaborated = true;
                    continue;
                }
            }

            let elem_type = self.elaboration_source_expression_type(elem_idx);
            let contextual_request =
                crate::context::TypingRequest::with_contextual_type(target_element_type);
            let contextual_elem_type =
                self.get_type_of_node_with_request(elem_idx, &contextual_request);
            let contextual_elem_assignable = contextual_elem_type != TypeId::ERROR
                && contextual_elem_type != TypeId::ANY
                && target_element_type != TypeId::ERROR
                && target_element_type != TypeId::ANY
                && self
                    .call_arg_relation_outcome(contextual_elem_type, target_element_type)
                    .related;

            // When the target element type is an index-signature-only type
            // (e.g., `NamedTransform { [name: string]: Transform3D }`),
            // don't drill into per-property errors for object literal elements.
            // Report at the element level instead:
            //   "Type '{ ry: null }' is not assignable to type 'NamedTransform'"
            // rather than the confusing inner error:
            //   "Type 'null' is not assignable to type 'Transform3D'"
            // This only applies to array element context — direct call argument
            // and variable assignment elaboration still drills into properties.
            let skip_deep_elaboration = elem_node.kind
                == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                && !self
                    .target_has_named_property_for_any_source_prop(elem_idx, target_element_type);

            if contextual_elem_assignable {
                continue;
            }

            // An array literal written for a *tuple-typed element slot* is
            // cached as the widened array (`{ kp: number }[]`) for exactly the
            // reason an array literal written for a tuple-typed *property* is:
            // its own check ran before the slot's contextual type was
            // available. Handing that widened form to the relation loses the
            // element count, so the failure is classified as the
            // unbounded-source arity gap (`TS2620` "Target requires N
            // element(s) but source may have fewer") — false on its face for a
            // literal that wrote exactly N elements — and it hides the real
            // per-element failure underneath.
            //
            // `contextual_tuple_recovers_elementwise_failure` is the same
            // predicate the object-literal property path already applies for
            // this shape; the element slot is the other half of `tsc`'s
            // `elaborateElementwise`, which recurses through array and object
            // literals alike. Restricted identically: the cached form lost
            // tuple-ness, the contextual form regained it, and the target slot
            // is a tuple, so the swap can only turn a whole-array arity reason
            // into the element-wise one.
            let elem_type = if self.contextual_tuple_recovers_elementwise_failure(
                elem_type,
                contextual_elem_type,
                target_element_type,
            ) {
                contextual_elem_type
            } else {
                elem_type
            };

            // For object/array literal elements, use contextually-typed type
            // to decide whether to elaborate (avoids false positives from widening).
            // Pass the target element type as contextual type so literal types
            // are preserved (e.g., `"bluray"` stays as `"bluray"` instead of
            // widening to `string` when checked against a discriminated union).
            if matches!(
                elem_node.kind,
                syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    | syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            ) && !skip_deep_elaboration
                && self.try_elaborate_assignment_source_error(elem_idx, target_element_type)
            {
                elaborated = true;
                continue;
            }
            // Fall through to the non-object element check below.

            // For function/conditional elements, try to elaborate without a guard.
            if matches!(
                elem_node.kind,
                syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::FUNCTION_EXPRESSION
                    | syntax_kind_ext::CONDITIONAL_EXPRESSION
            ) && self.try_elaborate_assignment_source_error(elem_idx, target_element_type)
            {
                elaborated = true;
                continue;
            }

            // Skip if types are unresolved
            if elem_type == TypeId::ERROR
                || elem_type == TypeId::ANY
                || target_element_type == TypeId::ERROR
                || target_element_type == TypeId::ANY
            {
                continue;
            }

            if !self
                .call_arg_relation_outcome(elem_type, target_element_type)
                .related
            {
                let widen_source_display = self.array_elaboration_widening_required_for_display(
                    elem_type,
                    target_element_type,
                );

                if !widen_source_display
                    && !skip_deep_elaboration
                    && self.try_elaborate_assignment_source_error(elem_idx, target_element_type)
                {
                    elaborated = true;
                    continue;
                }

                // When the element is an object literal and property-level elaboration
                // found no issues (returned false above), the widened type (e.g.,
                // `{ kind: string }`) fails assignability but the literal types of all
                // properties actually match the target. This happens with discriminated
                // unions where the literal property types are preserved contextually but
                // the overall element type gets widened. Suppress the false TS2322.
                if elem_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    && self.all_object_literal_properties_assignable_with_literals(
                        elem_idx,
                        elem_type,
                        target_element_type,
                    )
                {
                    continue;
                }

                // For a nested (possibly parenthesized) array literal the
                // context-free retype is an open array, which mis-classifies
                // the sub-reason (arity family instead of the positional
                // chain), so use the source tuple's slot type recovered above.
                // tsc applies the slot-type rule to *every* reported element;
                // gating on array literals is conservative staging — other
                // element kinds keep the display baselines tuned to
                // `elem_type` until their parity witnesses surface.
                let elem_is_array_literal = self
                    .ctx
                    .arena
                    .get(self.ctx.arena.skip_parenthesized(elem_idx))
                    .is_some_and(|node| node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION);
                let relation_elem_type = if elem_is_array_literal {
                    source_tuple_elements
                        .as_deref()
                        .and_then(|elements| elements.get(index))
                        .filter(|element| element.is_required())
                        .map(|element| element.type_id)
                        .filter(|&source_element| !source_element.is_any_unknown_or_error())
                        .unwrap_or(elem_type)
                } else {
                    elem_type
                };
                // The recovered source tuple slot can itself be the widened
                // array form when the *enclosing* literal was the one cached
                // before its contextual type arrived (a tuple of tuples). Apply
                // the same recovery to the slot so the nested case cannot
                // reintroduce the arity reason the rebind above removed.
                let relation_elem_type = if self.contextual_tuple_recovers_elementwise_failure(
                    relation_elem_type,
                    contextual_elem_type,
                    target_element_type,
                ) {
                    contextual_elem_type
                } else {
                    relation_elem_type
                };
                tracing::debug!(
                    "try_elaborate_array_literal_elements: elem_type = {:?}, target_element_type = {:?}, file = {}",
                    elem_type,
                    target_element_type,
                    self.ctx.file_name
                );
                if widen_source_display {
                    self.error_type_not_assignable_at_with_widened_source_display(
                        relation_elem_type,
                        display_target_element_type,
                        elem_idx,
                    );
                } else {
                    self.error_type_not_assignable_at_with_anchor(
                        relation_elem_type,
                        display_target_element_type,
                        elem_idx,
                    );
                }
                elaborated = true;
            }
        }

        elaborated
    }

    /// Returns true when `target` evaluates to a plain `T[]` or
    /// `readonly T[]` type, suitable for per-element spread elaboration.
    /// Custom interfaces/classes that extend `Array<T>` keep tsc's
    /// whole-assignment TS2322, so we exclude them here.
    fn target_is_plain_array_for_spread_elaboration(&mut self, target: TypeId) -> bool {
        let target = self.resolve_lazy_type(target);
        if crate::query_boundaries::common::is_array_type(self.ctx.types, target) {
            return true;
        }
        // readonly T[] — accept by recursing into the inner type.
        if let Some(inner) =
            crate::query_boundaries::common::get_readonly_inner(self.ctx.types, target)
        {
            return self.target_is_plain_array_for_spread_elaboration(inner);
        }
        false
    }

    /// Elaborate a spread element inside an array literal whose contextual
    /// element type doesn't match the spread's iterated element type.
    ///
    /// For `var arr: number[] = [0, 1, ...new SymbolIterator]`, tsc reports
    /// `TS2322 'symbol' is not assignable to 'number'` at the spread
    /// element span (`...new SymbolIterator`), anchoring the diagnostic on
    /// the spread argument rather than on the assignment target.
    ///
    /// Returns true when an elaborated diagnostic is emitted at the spread
    /// expression, so the caller can suppress the outer assignment error.
    fn try_elaborate_spread_element_array_mismatch(
        &mut self,
        spread_idx: NodeIndex,
        target_element_type: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(spread_node) = self.ctx.arena.get(spread_idx) else {
            return false;
        };
        if spread_node.kind != syntax_kind_ext::SPREAD_ELEMENT {
            return false;
        }
        let Some(spread_data) = self.ctx.arena.get_spread(spread_node).cloned() else {
            return false;
        };

        if target_element_type.is_any_unknown_or_error() {
            return false;
        }

        // Compute the spread argument's iterated element type. If the
        // argument is itself a tuple, use array element semantics so the
        // resulting type compares like the union of element types.
        let spread_expr_type = self.get_type_of_node(spread_data.expression);
        let spread_expr_type = self.resolve_lazy_type(spread_expr_type);
        if spread_expr_type.is_any_unknown_or_error() {
            return false;
        }

        let iterated_element_type = self.for_of_element_type(spread_expr_type, false);
        if iterated_element_type.is_any_unknown_or_error() {
            return false;
        }
        if iterated_element_type == target_element_type {
            return false;
        }
        // Don't elaborate when the iterated type is the same as the spread
        // expression type — that means we couldn't actually compute an
        // iterator element type (e.g., the iterator protocol resolution
        // failed).  Falling through to the outer assignment error is a
        // better message than printing the spread type itself.
        if iterated_element_type == spread_expr_type {
            return false;
        }
        if self
            .call_arg_relation_outcome(iterated_element_type, target_element_type)
            .related
        {
            return false;
        }

        // Format types directly (do not route through the assignability
        // diagnostic pipeline, which would recompute the source display
        // from the spread expression's own type and end up printing the
        // spread receiver — e.g. `'SymbolIterator'` — instead of the
        // iterated element type — `'symbol'`).
        let source_display = self.format_type(iterated_element_type);
        let target_display = self.format_type(target_element_type);
        let message = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_display, &target_display],
        );
        // Anchor at the spread element itself (which spans the `...` prefix
        // plus the inner expression), matching tsc's column. tsc reports at
        // the start of the `...` token rather than the inner expression
        // (e.g. column 30 of `[0, 1, ...new SymbolIterator]`, not column 33).
        self.error_at_node(
            spread_idx,
            &message,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        );
        true
    }

    /// Returns true if `idx` resolves to an `OBJECT_LITERAL_EXPRESSION` after
    /// peeling parenthesized and comma-expression wrappers. Used to gate the
    /// var-decl elaboration entry so unrelated initializers (`null as any`,
    /// identifiers, ...) skip the elaboration path entirely. Calling
    /// `is_assignable_to` on those has cache side-effects that perturb
    /// downstream JSX/contextual-typing decisions.
    pub fn initializer_reaches_object_literal_through_wrappers(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let mut current = idx;
        for _ in 0..16 {
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return true;
            }
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.ctx.arena.get_parenthesized(node)
            {
                current = paren.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(bin) = self.ctx.arena.get_binary_expr(node)
                && bin.operator_token == SyntaxKind::CommaToken as u16
            {
                current = bin.right;
                continue;
            }
            return false;
        }
        false
    }

    /// Elaborate object literal property mismatches for variable declarations.
    ///
    /// Walks through parentheses and comma expressions to find the inner
    /// object literal: `var x: T = (1, 2, { ... })` and `var x: T = ({...})`
    /// both still drill into the trailing object literal. tsc anchors the
    /// per-property TS2322 to the deepest offending leaf inside the
    /// initializer's object literal regardless of these wrappers.
    pub fn try_elaborate_object_literal_properties_for_var_init(
        &mut self,
        init_idx: NodeIndex,
        declared_type: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = init_idx;
        for _ in 0..16 {
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                // Inner per-property diagnostics suppress the outer whole-object error (tsc parity).
                if self.object_literal_has_inner_property_diagnostics(current) {
                    return true;
                }
                return self.try_elaborate_object_literal_properties(current, declared_type);
            }
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.ctx.arena.get_parenthesized(node)
            {
                current = paren.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(bin) = self.ctx.arena.get_binary_expr(node)
                && bin.operator_token == SyntaxKind::CommaToken as u16
            {
                current = bin.right;
                continue;
            }
            return false;
        }
        false
    }

    /// True if a TS2322/TS2353/TS1360 diagnostic is anchored inside any of this object literal's property spans.
    fn object_literal_has_inner_property_diagnostics(&self, obj_idx: NodeIndex) -> bool {
        let Some(obj_node) = self.ctx.arena.get(obj_idx) else {
            return false;
        };
        let Some(obj) = self.ctx.arena.get_literal_expr(obj_node) else {
            return false;
        };
        for &elem_idx in &obj.elements.nodes {
            let Some((start, end)) = self.ctx.get_node_span(elem_idx) else {
                continue;
            };
            if self.ctx.diagnostics.iter().any(|diag| {
                matches!(
                    diag.code,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                        | diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
                        | diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_BUT_DOES_NOT_EXIST_IN_TYPE_DID
                        | diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_EXPECTED_TYPE
                ) && diag.start >= start
                    && diag.start < end
            }) {
                return true;
            }
        }
        false
    }

    /// Elaborate array literal element mismatches for variable declarations.
    pub fn try_elaborate_initializer_elements(
        &mut self,
        init_type: TypeId,
        declared_type: TypeId,
        init_idx: NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let init_node = match self.ctx.arena.get(init_idx) {
            Some(node) if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => node,
            _ => return false,
        };

        // Only elaborate when the overall assignment fails.
        if self
            .variable_initializer_relation_outcome(init_type, declared_type)
            .related
        {
            return false;
        }

        // Arity mismatch — report at whole-assignment level, not per-element.
        if let Some(arr) = self.ctx.arena.get_literal_expr(init_node) {
            let source_count = arr.elements.nodes.len();
            if let Some(target_count) = crate::query_boundaries::common::get_fixed_tuple_length(
                self.ctx.types,
                declared_type,
            ) && source_count > target_count
            {
                return false;
            }
        }

        // Delegate to array literal element elaboration
        self.try_elaborate_array_literal_elements(init_idx, declared_type)
    }
}
