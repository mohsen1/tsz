//! Type assignability error reporting (TS2322 and related).

use crate::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, RelatedInformationKind,
    diagnostic_codes, diagnostic_messages, format_message,
};
use crate::error_reporter::assignability_literal_display::display_has_boolean_member_literal_assignability;
use crate::error_reporter::fingerprint_policy::{
    DiagnosticAnchorKind, DiagnosticRenderRequest, RelatedInformationPolicy,
};
use crate::error_reporter::type_display_policy::DiagnosticTypeDisplayRole;
use crate::state::CheckerState;
use tracing::{Level, trace};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

pub(crate) use super::assignability_type_helpers::{
    display_is_literal_value, is_primitive_type_name, is_reserved_type_name,
};
pub(super) use super::assignability_type_helpers::{
    is_builtin_wrapper_name, is_callable_application_type,
    is_function_like_for_literal_member_widening, is_object_prototype_method,
    is_object_prototype_method_for_array_target,
};

impl<'a> CheckerState<'a> {
    /// Get the declaring type name for a property in a target type.
    /// For inherited properties (e.g., from a base class), returns the base class name.
    /// Falls back to formatting the target type if no parent info is available.
    ///
    /// Returns `None` when the property is declared on the target type ITSELF:
    /// tsc then renders the full target display (`A<unknown>`, not the bare
    /// class symbol `A`) — the declaring-type shortcut is only for genuinely
    /// inherited members.
    pub(super) fn property_declaring_type_name(
        &self,
        target_type: TypeId,
        property_name: tsz_common::interner::Atom,
    ) -> Option<String> {
        let prop_info = self.property_info_for_display(target_type, property_name)?;
        let parent_id = prop_info.parent_id?;
        let own_symbol = crate::query_boundaries::diagnostics::object_shape_for_type(
            self.ctx.types,
            target_type,
        )
        .and_then(|shape| shape.symbol)
        .or_else(|| {
            crate::query_boundaries::diagnostics::callable_shape_for_type(
                self.ctx.types,
                target_type,
            )
            .and_then(|shape| shape.symbol)
        })
        .or_else(|| {
            crate::query_boundaries::diagnostics::lazy_def_id(self.ctx.types, target_type)
                .and_then(|def_id| self.ctx.def_symbol_identity(def_id))
                .map(|(sym_id, _)| sym_id)
        });
        if own_symbol == Some(parent_id) {
            return None;
        }
        self.ctx
            .binder
            .get_symbol(parent_id)
            .map(|sym| sym.escaped_name.clone())
    }

    pub(super) fn property_info_for_display(
        &self,
        ty: TypeId,
        name: tsz_common::interner::Atom,
    ) -> Option<tsz_solver::PropertyInfo> {
        crate::query_boundaries::diagnostics::object_shape_for_type(self.ctx.types, ty)
            .and_then(|shape| {
                shape
                    .properties
                    .iter()
                    .find(|candidate| candidate.name == name)
                    .cloned()
            })
            .or_else(|| {
                crate::query_boundaries::diagnostics::callable_shape_for_type(self.ctx.types, ty)
                    .and_then(|shape| {
                        shape
                            .properties
                            .iter()
                            .find(|candidate| candidate.name == name)
                            .cloned()
                    })
            })
            .or_else(|| {
                crate::query_boundaries::diagnostics::intersection_members(self.ctx.types, ty)
                    .and_then(|members| {
                        members
                            .iter()
                            .find_map(|member| self.property_info_for_display(*member, name))
                    })
            })
    }

    pub(super) fn should_suppress_outer_callback_return_assignability(
        &mut self,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> bool {
        let Some(callback_idx) = self.callback_initializer_for_assignability_anchor(anchor_idx)
        else {
            return false;
        };
        if self.callback_has_explicit_param_type_conflict(callback_idx, target) {
            return false;
        }

        let Some(callback_node) = self.ctx.arena.get(callback_idx) else {
            return false;
        };
        let Some(function) = self.ctx.arena.get_function(callback_node) else {
            return false;
        };
        let Some(body_node) = self.ctx.arena.get(function.body) else {
            return false;
        };
        if body_node.kind == syntax_kind_ext::BLOCK {
            return false;
        }

        self.has_diagnostic_code_within_span(
            body_node.pos,
            body_node.end,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        ) || self.has_diagnostic_code_within_span(
            body_node.pos,
            body_node.end,
            diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE,
        )
    }

    fn should_suppress_assignment_after_overload_failure(
        &self,
        source: TypeId,
        anchor_idx: NodeIndex,
    ) -> bool {
        if source != TypeId::NEVER && source != TypeId::ERROR {
            return false;
        }

        let Some(anchor_node) = self.ctx.arena.get(anchor_idx) else {
            return false;
        };

        // Case 1: `x = fn(true);` — anchor is EXPRESSION_STATEMENT
        if anchor_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
            let Some(expr_stmt) = self.ctx.arena.get_expression_statement(anchor_node) else {
                return false;
            };
            let expr_idx = self.ctx.arena.skip_parenthesized(expr_stmt.expression);
            let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
                return false;
            };
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                return false;
            }
            let Some(binary) = self.ctx.arena.get_binary_expr(expr_node) else {
                return false;
            };
            if !self.is_assignment_operator(binary.operator_token) {
                return false;
            }
            let rhs_idx = self
                .ctx
                .arena
                .skip_parenthesized_and_assertions(binary.right);
            let Some(rhs_node) = self.ctx.arena.get(rhs_idx) else {
                return false;
            };
            if rhs_node.kind != syntax_kind_ext::CALL_EXPRESSION
                && rhs_node.kind != syntax_kind_ext::NEW_EXPRESSION
            {
                return false;
            }
            return self.call_or_new_expr_emitted_no_overload_failure(rhs_idx, rhs_node);
        }

        // Case 2: `const x: T = fn(true);` — anchor is the variable name IDENTIFIER.
        // Walk up to the VARIABLE_DECLARATION and check the initializer.
        if anchor_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let Some(ext) = self.ctx.arena.get_extended(anchor_idx) else {
                return false;
            };
            let parent_idx = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                return false;
            }
            let Some(vd) = self.ctx.arena.get_variable_declaration(parent_node) else {
                return false;
            };
            let init_idx = self
                .ctx
                .arena
                .skip_parenthesized_and_assertions(vd.initializer);
            let Some(init_node) = self.ctx.arena.get(init_idx) else {
                return false;
            };
            if init_node.kind != syntax_kind_ext::CALL_EXPRESSION
                && init_node.kind != syntax_kind_ext::NEW_EXPRESSION
            {
                return false;
            }
            return self.call_or_new_expr_emitted_no_overload_failure(init_idx, init_node);
        }

        false
    }

    fn call_or_new_expr_emitted_no_overload_failure(
        &self,
        expr_idx: NodeIndex,
        expr_node: &tsz_parser::parser::node::Node,
    ) -> bool {
        self.ctx.no_overload_call_nodes.contains(&expr_idx.0)
            && self.ctx.diagnostics.iter().any(|diag| {
                diag.code == diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL
                    && diag.start >= expr_node.pos
                    && diag.start < expr_node.end
            })
    }

    pub(super) fn private_or_protected_member_missing_display(
        &self,
        source_type: TypeId,
        target_type: TypeId,
        required_property_name: Option<tsz_common::interner::Atom>,
    ) -> Option<(String, String, tsz_solver::Visibility)> {
        let source_has_prop = |name| self.property_info_for_display(source_type, name).is_some();

        let find_missing = |props: &[tsz_solver::PropertyInfo]| {
            props.iter().find_map(|prop| {
                let prop_name = self.ctx.types.resolve_atom(prop.name);
                if tsz_solver::utils::is_synthetic_private_brand_name(&prop_name)
                    || required_property_name.is_some_and(|required| prop.name != required)
                    || prop.visibility == tsz_solver::Visibility::Public
                    || source_has_prop(prop.name)
                {
                    return None;
                }

                let owner_name = prop
                    .parent_id
                    .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                    .map(|sym| sym.escaped_name.clone())
                    .unwrap_or_else(|| self.format_type_diagnostic(target_type));
                Some((prop_name, owner_name, prop.visibility))
            })
        };

        crate::query_boundaries::diagnostics::object_shape_for_type(self.ctx.types, target_type)
            .and_then(|shape| find_missing(&shape.properties))
            .or_else(|| {
                crate::query_boundaries::diagnostics::callable_shape_for_type(
                    self.ctx.types,
                    target_type,
                )
                .and_then(|shape| find_missing(&shape.properties))
            })
    }

    // =========================================================================
    // Type Assignability Errors
    // =========================================================================

    /// Report a type not assignable error (delegates to `diagnose_assignment_failure`).
    pub fn error_type_not_assignable_at(&mut self, source: TypeId, target: TypeId, idx: NodeIndex) {
        let anchor_idx =
            self.resolve_diagnostic_anchor_node(idx, DiagnosticAnchorKind::RewriteAssignment);
        self.diagnose_assignment_failure_with_anchor(source, target, anchor_idx);
    }

    /// Report a type not assignable error at an exact AST node anchor.
    pub fn error_type_not_assignable_at_with_anchor(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) {
        let anchor_idx =
            self.resolve_diagnostic_anchor_node(anchor_idx, DiagnosticAnchorKind::Exact);
        self.diagnose_assignment_failure_with_anchor(source, target, anchor_idx);
    }

    /// Like `error_type_not_assignable_at_with_anchor`, but for object literal
    /// property-value elaboration contexts. TSC's `elaborateElementwise` reports
    /// TS2322 at the property name for property-value type mismatches, not
    /// TS2741/TS2739/TS2740 (missing property codes). This variant uses full
    /// failure analysis for accurate message formatting (e.g., union best-match),
    /// then downgrades any "missing property" code to TS2322.
    /// Like `error_type_not_assignable_at_with_anchor`, but for object literal
    /// property-value elaboration contexts. TSC's `elaborateElementwise` reports
    /// TS2322 at the property name for property-value type mismatches, not
    /// TS2741/TS2739/TS2740 (missing property codes). This variant uses full
    /// failure analysis for accurate message formatting (e.g., union best-match),
    /// then downgrades any "missing property" code to TS2322.
    ///
    /// NOTE: For empty object literals `{}` that are missing required properties,
    /// we should NOT downgrade TS2741 to TS2322 - we should keep TS2741 because
    /// the issue is missing properties, not type mismatch. Only downgrade when
    /// there are actual property-value type mismatches.
    pub fn error_type_not_assignable_at_with_anchor_elaboration(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) {
        self.error_type_not_assignable_at_with_anchor_elaboration_inner(
            source, target, anchor_idx, false,
        );
    }

    /// Like `error_type_not_assignable_at_with_anchor_elaboration`, but when
    /// `downgrade_missing_to_2322` is true, converts TS2741/TS2739/TS2740
    /// (missing-property) diagnostics to TS2322 ("Type X is not assignable to
    /// type Y"). tsc's `elaborateElementwise` uses TS2322 for `this` keyword
    /// property values instead of the more specific missing-property codes.
    pub fn error_type_not_assignable_at_with_anchor_elaboration_inner(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
        downgrade_missing_to_2322: bool,
    ) {
        self.error_type_not_assignable_at_with_anchor_elaboration_inner_with_value_anchor(
            source,
            target,
            anchor_idx,
            None,
            downgrade_missing_to_2322,
        );
    }

    /// Like [`error_type_not_assignable_at_with_anchor_elaboration_inner`], but
    /// also relocates any emitted missing-property diagnostics (TS2741/TS2739/
    /// TS2740) to `value_anchor_idx` when provided. tsc's
    /// `elaborateElementwise` anchors missing-property elaborations on the
    /// property initializer (the value), while plain TS2322 assignability
    /// diagnostics remain anchored on the property name — so callers pass the
    /// value anchor only when they want missing-property codes repositioned.
    pub fn error_type_not_assignable_at_with_anchor_elaboration_inner_with_value_anchor(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
        value_anchor_idx: Option<NodeIndex>,
        downgrade_missing_to_2322: bool,
    ) {
        let anchor_idx =
            self.resolve_diagnostic_anchor_node(anchor_idx, DiagnosticAnchorKind::Exact);
        let diag_count_before = self.ctx.diagnostics.len();
        self.diagnose_assignment_failure_with_anchor(source, target, anchor_idx);

        use crate::diagnostics::diagnostic_codes;

        // The missing-property elaboration codes (TS2741/TS2739/TS2740) whose
        // span and code are finalized below once the value anchor and downgrade
        // decision are known.
        let is_missing_property = |code: u32| {
            matches!(
                code,
                diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
                    | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
                    | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
            )
        };

        // When a value anchor is supplied, missing-property codes are
        // repositioned to anchor on the property value — matching tsc's
        // `elaborateElementwise` behavior that uses the initializer as the error
        // node for missing-property elaborations.
        let value_span = value_anchor_idx.and_then(|value_anchor_src| {
            let resolved_value_anchor =
                self.resolve_diagnostic_anchor_node(value_anchor_src, DiagnosticAnchorKind::Exact);
            self.resolve_diagnostic_anchor(resolved_value_anchor, DiagnosticAnchorKind::Exact)
                .map(|anchor| (anchor.start, anchor.length))
                .or_else(|| {
                    self.get_node_span(resolved_value_anchor).map(|(pos, end)| {
                        self.normalized_anchor_span(
                            resolved_value_anchor,
                            pos,
                            end.saturating_sub(pos),
                        )
                    })
                })
        });

        // Downgrade missing-property elaborations to TS2322 when the caller asks
        // for it and at least one such diagnostic was emitted. The replacement
        // message is built before the buffer tail is finalized.
        let downgrade_message = (downgrade_missing_to_2322
            && self
                .ctx
                .recent_diagnostics(diag_count_before)
                .iter()
                .any(|d| is_missing_property(d.code)))
        .then(|| {
            let src_str = "this".to_string();
            let tgt_str = self.format_type_for_assignability_message(target);
            let (src_str, tgt_str) =
                self.finalize_pair_display_for_diagnostic(source, target, src_str, tgt_str);
            crate::diagnostics::format_message(
                crate::diagnostics::diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            )
        });

        if value_span.is_none() && downgrade_message.is_none() {
            return;
        }

        self.ctx
            .finalize_recent_diagnostics(diag_count_before, |diag| {
                if !is_missing_property(diag.code) {
                    return;
                }
                if let Some((start, length)) = value_span {
                    diag.start = start;
                    diag.length = length;
                }
                if let Some(new_message) = &downgrade_message {
                    diag.code = diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE;
                    diag.message_text = new_message.clone();
                }
            });
    }
    /// Diagnose why an assignment failed and report a detailed error.
    pub fn diagnose_assignment_failure(&mut self, source: TypeId, target: TypeId, idx: NodeIndex) {
        let anchor_idx =
            self.resolve_diagnostic_anchor_node(idx, DiagnosticAnchorKind::RewriteAssignment);
        self.diagnose_assignment_failure_with_anchor(source, target, anchor_idx);
    }

    /// Report a fresh-object **union** source whose excess belongs to tsc's
    /// single-`TS2322` shape.
    ///
    /// When a fresh object literal flows through `?:`/`??`/`||`/return and the
    /// assignment source stays a union of distinct members (e.g.
    /// `cond ? { a: 1, b: 2 } : { a: 3 }` → `{ … } | { … }`), tsc reports ONE
    /// `TS2322` — `Type '<union>' is not assignable to type '<target>'.` — with
    /// the first offending member's excess-property message attached as a nested
    /// elaboration, anchored at the excess property. (When the branches widen to
    /// one shape the conditional collapses to a single object type,
    /// `union_members` is `None`, and the caller keeps the standalone `TS2353`.)
    ///
    /// `display_anchor_idx` selects the source/target display; `walk_start_idx`
    /// is the wrapper expression to descend for branch literals. Returns `true`
    /// when it emitted the union `TS2322` (the caller must then stop).
    pub(crate) fn report_fresh_object_union_excess(
        &mut self,
        source: TypeId,
        target: TypeId,
        display_anchor_idx: NodeIndex,
        walk_start_idx: NodeIndex,
    ) -> bool {
        if crate::query_boundaries::diagnostics::union_members(self.ctx.types, source).is_none() {
            return false;
        }
        // Run the canonical per-literal excess detector on the branch literals
        // reachable through the wrapper, stopping at the first offending member
        // (tsc reports only the first). The detector keys off the literal AST
        // node, so it still surfaces excess after contextual typing has stripped
        // freshness from the cached branch type — which a freshness-gated type
        // relation would miss.
        let diags_before = self.ctx.diagnostics.len();
        for obj_idx in self.collect_rhs_object_literals(walk_start_idx) {
            let literal_type = self.get_type_of_node(obj_idx);
            self.check_object_literal_excess_properties(literal_type, target, obj_idx);
            if self.ctx.diagnostics.len() > diags_before {
                break;
            }
        }
        if self.ctx.diagnostics.len() == diags_before {
            return false;
        }
        // Refold the offending member's excess diagnostic into tsc's single union
        // shape: the same property-anchored excess message becomes a nested
        // elaboration beneath a `Type '<union>' is not assignable to type '<T>'.`
        // head. Only a plain excess emit (TS2353/TS2561) refolds; anything else
        // is left as the caller emitted it.
        let captured = self.ctx.diagnostics.split_off(diags_before);
        let head = &captured[0];
        if !matches!(
            head.code,
            diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
                | diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_BUT_DOES_NOT_EXIST_IN_TYPE_DID
        ) {
            self.ctx.diagnostics.extend(captured);
            return false;
        }
        let (source_str, target_str) = self.format_top_level_assignability_message_types_at(
            source,
            target,
            display_anchor_idx,
        );
        let main_message = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        );
        let mut diag = Diagnostic::error(
            head.file.clone(),
            head.start,
            head.length,
            main_message,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        );
        diag.push_elaboration(head.message_text.clone(), head.code, 0);
        // Preserve any deeper chain the excess diagnostic already carried (nested
        // excess produces its own related-information trail).
        for related in &head.related_information {
            diag.push_elaboration_at(
                related.file.clone(),
                related.start,
                related.length,
                related.message_text.clone(),
                related.code,
                u32::from(related.depth).saturating_add(1),
            );
        }
        // The excess diagnostic was removed from the buffer; rebuild the auxiliary
        // dedup index so its recorded excess-property position no longer suppresses
        // the overlapping TS2322 we are about to push.
        self.ctx.rebuild_diagnostic_aux_indices();
        self.ctx.push_diagnostic(diag);
        true
    }

    /// Emit a TS2375 exact-optional-property assignment diagnostic, attaching
    /// the property-incompatibility elaboration that `tsc` renders beneath it.
    ///
    /// The top line is already formatted by the caller; this runs the shared
    /// assignability failure analysis on the same `source`/`target` pair so the
    /// relation reason (`PropertyTypeMismatch`) populates the nested
    /// `Types of property 'X' are incompatible. / Type 'S' is not assignable to
    /// type 'T'.` related-information chain — identical to the TS2379 argument
    /// path. When no structural reason is captured the diagnostic degrades to a
    /// flat line, matching the prior behavior.
    fn emit_exact_optional_assignment_diagnostic(
        &mut self,
        anchor_idx: NodeIndex,
        code: u32,
        message: String,
        source: TypeId,
        target: TypeId,
    ) {
        let reason = self
            .analyze_assignability_failure(source, target)
            .failure_reason;
        let request = if let Some(reason) = reason {
            DiagnosticRenderRequest::with_failure_reason(
                DiagnosticAnchorKind::Exact,
                code,
                message,
                reason,
                source,
                target,
            )
        } else {
            DiagnosticRenderRequest::simple(DiagnosticAnchorKind::Exact, code, message)
        };
        self.emit_render_request(anchor_idx, request);
    }

    fn rewrite_variadic_tuple_structural_ts2322(
        &mut self,
        diag: &mut Diagnostic,
        failure_reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) {
        if diag.code != diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
            || !matches!(
                failure_reason,
                tsz_solver::SubtypeFailureReason::TupleElementTypeMismatch { .. }
                    | tsz_solver::SubtypeFailureReason::TupleVariadicPositionMismatch { .. }
            )
        {
            return;
        }

        // A target annotated by a type REFERENCE keeps its name only when the
        // referenced alias body is already in tuple normal form:
        // `type Rested = [...number[], boolean]; const bad: Rested = ["a"]`
        // renders `'[string]' is not assignable to type 'Rested'` in tsc,
        // while `type Unbounded = [...Numbers, boolean]` (spread of a NAMED
        // array alias) normalizes the variadic element away, mints a fresh
        // tuple without the alias identity, and renders the structural form
        // (variadicTuples1.ts line 415). Both alias bodies EVALUATE to the
        // same `TypeId` as an anonymous annotation of the same shape, so the
        // alias declaration's AST is the only authoritative signal. The probe
        // is resolution-free (syntax reads plus the binder's memoized
        // read-only scope walk): mid-render SYMBOL resolution perturbs the
        // checked program (the hazard `chain_rendering_does_not_leak_
        // diagnostics_into_enclosing_call` guards). Spread-FLATTENED aliases
        // never reach this hook — their evaluation has no rest element, so
        // the structural-display probe below declines them.
        if self
            .direct_assignment_target_annotation_node(anchor_idx)
            .is_some_and(|annotation_idx| {
                self.annotation_references_normal_form_tuple_alias(annotation_idx)
            })
        {
            return;
        }
        let Some(target_str) = self.variadic_tuple_alias_structural_display(target, source) else {
            return;
        };
        let source_str = self.format_type_for_diagnostic_role(
            source,
            DiagnosticTypeDisplayRole::AssignmentSource { target, anchor_idx },
        );
        let (source_str, target_str) =
            self.finalize_pair_display_for_diagnostic(source, target, source_str, target_str);
        diag.message_text = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        );
    }

    /// Whether `annotation_idx` is a plain (argument-free) type reference to
    /// a type alias whose declared tuple body is in syntactic normal form:
    /// every spread element's operand is written as an ARRAY type
    /// (`...number[]`). tsc keeps the alias identity for such tuples because
    /// `getTupleElementFlags` classifies an array-operand spread as `Rest`
    /// (no normalization runs), while a named-operand spread (`...Numbers`)
    /// is `Variadic` and `createNormalizedTupleType` mints a fresh tuple
    /// without the alias symbol, so tsc displays the structural form there.
    /// Resolution-free: the name lookup is `Binder::resolve_identifier`, a
    /// memoized read-only scope walk. Lookup failures (qualified names,
    /// generic references, imported aliases) return `false`, falling back to
    /// the structural rewrite.
    fn annotation_references_normal_form_tuple_alias(&self, annotation_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(annotation_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return false;
        };
        if type_ref
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty())
        {
            return false;
        }
        let Some(sym_id) = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, type_ref.type_name)
        else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        if !symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS) {
            return false;
        }
        let decl_idx = symbol.primary_declaration().unwrap_or(NodeIndex::NONE);
        if decl_idx.is_none() {
            return false;
        }
        let Some(alias) = self.ctx.arena.get_type_alias_at(decl_idx) else {
            return false;
        };
        self.tuple_type_node_is_syntactic_normal_form(alias.type_node)
    }

    /// Whether a tuple type NODE is already in tsc's tuple normal form: each
    /// spread element (`RestType` or a `...`-marked `NamedTupleMember`) has a
    /// syntactic ARRAY-type operand. Purely syntactic — no symbol or type
    /// resolution.
    fn tuple_type_node_is_syntactic_normal_form(&self, type_node_idx: NodeIndex) -> bool {
        let Some(body) = self.ctx.arena.get(type_node_idx) else {
            return false;
        };
        if body.kind != syntax_kind_ext::TUPLE_TYPE {
            return false;
        }
        let Some(tuple) = self.ctx.arena.get_tuple_type(body) else {
            return false;
        };
        tuple.elements.nodes.iter().all(|&elem_idx| {
            let Some(elem) = self.ctx.arena.get(elem_idx) else {
                return true;
            };
            let spread_operand = if elem.kind == syntax_kind_ext::REST_TYPE {
                self.ctx
                    .arena
                    .get_wrapped_type(elem)
                    .map(|wrapped| wrapped.type_node)
            } else if elem.kind == syntax_kind_ext::NAMED_TUPLE_MEMBER {
                self.ctx
                    .arena
                    .get_named_tuple_member(elem)
                    .filter(|member| member.dot_dot_dot_token)
                    .map(|member| member.type_node)
            } else {
                None
            };
            spread_operand.is_none_or(|operand_idx| {
                self.ctx
                    .arena
                    .get(operand_idx)
                    .is_some_and(|operand| operand.kind == syntax_kind_ext::ARRAY_TYPE)
            })
        })
    }

    /// Internal helper that reports a detailed assignability failure using an
    /// already-resolved diagnostic anchor.
    pub(super) fn diagnose_assignment_failure_with_anchor(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) {
        // Same TypeId → no actual type mismatch (failure at a higher structural level).
        if source == target {
            return;
        }
        // A type alias flagged as unconditionally-infinite (TS2589 at its
        // definition) collapses to the error type in tsc, which is assignable in
        // both directions. When either side involves such a poisoned alias, the
        // structural relation is meaningless, so suppress the TS2322 cascade.
        // Gated on the poison set so the common case pays nothing.
        if self.ctx.definition_store.has_any_depth_poisoned()
            && (self.ctx.type_involves_depth_poisoned_def(source)
                || self.ctx.type_involves_depth_poisoned_def(target))
        {
            return;
        }
        // Centralized suppression for TS2322 cascades on unresolved escape-hatch types.
        if !self.has_exact_optional_property_mismatch(source, target)
            && self.should_suppress_assignability_diagnostic(source, target)
        {
            if tracing::enabled!(Level::TRACE) {
                trace!(
                    source = source.0,
                    target = target.0,
                    node_idx = anchor_idx.0,
                    file = %self.ctx.file_name,
                    "suppressing TS2322 for non-actionable source/target types"
                );
            }
            return;
        }
        if self.should_suppress_assignment_after_overload_failure(source, anchor_idx) {
            return;
        }

        let has_callable_shape = |this: &mut Self, ty: TypeId| {
            crate::query_boundaries::diagnostics::function_shape_for_type(this.ctx.types, ty)
                .is_some()
                || crate::query_boundaries::diagnostics::callable_shape_for_type(this.ctx.types, ty)
                    .is_some()
                || {
                    let evaluated = this.evaluate_type_with_env(ty);
                    crate::query_boundaries::diagnostics::function_shape_for_type(
                        this.ctx.types,
                        evaluated,
                    )
                    .is_some()
                        || crate::query_boundaries::diagnostics::callable_shape_for_type(
                            this.ctx.types,
                            evaluated,
                        )
                        .is_some()
                }
        };
        if has_callable_shape(self, source)
            && has_callable_shape(self, target)
            && let Some(arg_node) = self.ctx.arena.get(anchor_idx)
            && matches!(arg_node.kind, k if k == syntax_kind_ext::ARROW_FUNCTION || k == syntax_kind_ext::FUNCTION_EXPRESSION)
            && let Some(func) = self.ctx.arena.get_function(arg_node)
            && let Some(body_node) = self.ctx.arena.get(func.body)
            && body_node.kind != syntax_kind_ext::BLOCK
            && self.has_diagnostic_code_within_span(
                body_node.pos,
                body_node.end,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            )
        {
            return;
        }

        // Check for constructor accessibility mismatch
        if let Some((source_level, target_level)) =
            self.constructor_accessibility_mismatch(source, target, None)
        {
            self.error_constructor_accessibility_not_assignable(
                source,
                target,
                source_level,
                target_level,
                anchor_idx,
            );
            return;
        }

        // Check for private brand mismatch
        if let Some(detail) = self.private_brand_mismatch_error(source, target) {
            let Some(anchor) =
                self.resolve_diagnostic_anchor(anchor_idx, DiagnosticAnchorKind::Exact)
            else {
                return;
            };

            let (source_type, target_type) =
                self.format_top_level_assignability_message_types_at(source, target, anchor_idx);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_type, &target_type],
            );

            let related = vec![DiagnosticRelatedInformation {
                category: DiagnosticCategory::Error,
                code: diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                file: self.ctx.file_name.clone(),
                start: anchor.start,
                length: anchor.length,
                message_text: detail,
                depth: 0,
                kind: RelatedInformationKind::ChainLink,
            }];

            self.emit_render_request_at_anchor(
                anchor,
                DiagnosticRenderRequest::with_related(
                    DiagnosticAnchorKind::Exact,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    message,
                    related,
                    RelatedInformationPolicy::ELABORATION,
                ),
            );
            return;
        }

        // Exact-optional presence checks make `obj.a = obj.a` safe in the present branch.
        if self.ctx.compiler_options.exact_optional_property_types
            && self.same_property_self_assignment_in_presence_true_branch_for_anchor(anchor_idx)
        {
            return;
        }

        // TS2375: exactOptionalPropertyTypes — undefined assigned to optional property without undefined.
        if self.has_exact_optional_property_mismatch(source, target) {
            let src_str = self.format_type_for_diagnostic_role(
                source,
                DiagnosticTypeDisplayRole::AssignmentSource { target, anchor_idx },
            );
            let tgt_str = self.format_exact_optional_target_type_for_message(target);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_WITH_EXACTOPTIONALPROPERTYTYPES_TRUE_CONSIDER_ADD,
                &[&src_str, &tgt_str],
            );
            // tsc routes this assignment through `checkTypeAssignableTo`, whose
            // failure reason supplies the nested `Types of property 'X' are
            // incompatible. / Type 'S' is not assignable to type 'T'.` chain
            // beneath the TS2375 top line. The sibling TS2379 argument path
            // already attaches this via `with_failure_reason`; mirror it here so
            // the assignment-context diagnostic carries the same elaboration
            // instead of a flat single line.
            self.emit_exact_optional_assignment_diagnostic(
                anchor_idx,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_WITH_EXACTOPTIONALPROPERTYTYPES_TRUE_CONSIDER_ADD,
                message,
                source,
                target,
            );
            return;
        }

        // TS2412: exactOptionalPropertyTypes write target mismatch (property/element write).
        if self.has_exact_optional_write_target_mismatch(source, target, anchor_idx) {
            // tsc reports the offending portion of the source — when the source
            // is `T | undefined` and the target is `T`, the diagnostic narrows
            // the source to `undefined` because `T` is assignable but
            // `undefined` is not under `exactOptionalPropertyTypes`. Surface
            // that narrowed display when the union strip leaves the target's
            // shape intact.
            let narrowed_source =
                self.exact_optional_source_for_message(source, target, anchor_idx);
            let src_str = if narrowed_source == TypeId::UNDEFINED {
                self.format_type_diagnostic(narrowed_source)
            } else {
                self.format_type_for_diagnostic_role(
                    narrowed_source,
                    DiagnosticTypeDisplayRole::AssignmentSource { target, anchor_idx },
                )
            };
            let tgt_str = self.format_exact_optional_target_type_for_message(target);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_WITH_EXACTOPTIONALPROPERTYTYPES_TRUE_CONSIDER_ADD_2,
                &[&src_str, &tgt_str],
            );
            if !self.emit_render_request(
                anchor_idx,
                DiagnosticRenderRequest::simple(
                    DiagnosticAnchorKind::Exact,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_WITH_EXACTOPTIONALPROPERTYTYPES_TRUE_CONSIDER_ADD_2,
                    message,
                ),
            ) {
                return;
            }
            return;
        }

        // Use one solver-boundary analysis path for TS2322 metadata.
        let analysis = self.analyze_assignability_failure(source, target);
        let reason = analysis.failure_reason;

        if tracing::enabled!(Level::TRACE) {
            let source_type = self.format_type_diagnostic(source);
            let target_type = self.format_type_diagnostic(target);
            let reason_ref = reason.as_ref();
            trace!(
                source = %source_type,
                target = %target_type,
                reason = ?reason_ref,
                node_idx = anchor_idx.0,
                file = %self.ctx.file_name,
                "assignability failure diagnostics"
            );
        }
        match reason {
            Some(ref failure_reason) => {
                if matches!(
                    failure_reason,
                    tsz_solver::SubtypeFailureReason::ExcessProperty { .. }
                ) {
                    let start_idx = if let Some(node) = self.ctx.arena.get(anchor_idx) {
                        if node.kind == syntax_kind_ext::RETURN_STATEMENT {
                            self.ctx
                                .arena
                                .get_return_statement(node)
                                .and_then(|ret| {
                                    if ret.expression.is_some() {
                                        Some(ret.expression)
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or(anchor_idx)
                        } else {
                            anchor_idx
                        }
                    } else {
                        anchor_idx
                    };
                    // A fresh object literal flowing through `?:`/`??`/`||`/return
                    // is a union for differing branches (tsc folds the excess into a
                    // single TS2322) but collapses to one object type for branches
                    // that widen alike (tsc keeps the standalone TS2353). The helper
                    // emits the union shape and returns `true`; otherwise the
                    // per-branch walk below emits the property-anchored TS2353. See
                    // `report_fresh_object_union_excess`.
                    if self.report_fresh_object_union_excess(source, target, anchor_idx, start_idx)
                    {
                        return;
                    }
                    let diags_before = self.ctx.diagnostics.len();
                    for obj_idx in self.collect_rhs_object_literals(start_idx) {
                        let literal_type = self.get_type_of_node(obj_idx);
                        self.check_object_literal_excess_properties(literal_type, target, obj_idx);
                    }
                    if self.ctx.diagnostics.len() > diags_before {
                        return;
                    }
                    if crate::query_boundaries::diagnostics::union_members(self.ctx.types, source)
                        .is_none()
                    {
                        return;
                    }
                }
                // Skip MissingProperty for synthetic internal keys that have no
                // user-facing spelling (e.g. the `__js_ctor_brand_*` constructor
                // brand). User-facing well-known symbol members (`[Symbol.dispose]`,
                // `[Symbol.iterator]`, …) are NOT skipped: tsc lists them in
                // TS2741/TS2739 on non-array targets, and the explain layer already
                // omits them for array-like targets where tsc treats them as
                // implicitly satisfied.
                if let tsz_solver::SubtypeFailureReason::MissingProperty {
                    property_name,
                    source_type,
                    target_type,
                } = &failure_reason
                {
                    let pn = self.ctx.types.resolve_atom_ref(*property_name);
                    if pn.starts_with("__js_ctor_brand_") {
                        return;
                    }
                    if self.missing_property_is_satisfied_by_source(
                        &[source, *source_type],
                        &[target, *target_type],
                        *property_name,
                    ) {
                        return;
                    }
                }
                if is_callable_application_type(self.ctx.types, source)
                    && is_callable_application_type(self.ctx.types, target)
                    && self.should_suppress_outer_callback_return_assignability(target, anchor_idx)
                {
                    return;
                }
                let mut diag =
                    self.render_failure_reason(failure_reason, source, target, anchor_idx, 0);
                self.rewrite_variadic_tuple_structural_ts2322(
                    &mut diag,
                    failure_reason,
                    source,
                    target,
                    anchor_idx,
                );
                if diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE {
                    diag.message_text = self.rewrite_static_schema_array_target_in_ts2322_message(
                        diag.message_text,
                        source,
                    );
                }
                let has_static_schema_display = self
                    .static_schema_array_structural_display(source, target)
                    .is_some()
                    || self
                        .static_schema_array_structural_display(target, source)
                        .is_some();
                if diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                    && !has_static_schema_display
                {
                    diag.message_text = self
                        .rewrite_declared_generic_alias_source_in_ts2322_message(
                            anchor_idx,
                            source,
                            target,
                            diag.message_text,
                        );
                }
                self.ctx.push_diagnostic(diag);
            }
            None => {
                // Before falling back to generic TS2322, check if there are missing
                // properties from index signature source. If so, emit TS2741 instead.
                if let Some(anchor) =
                    self.resolve_diagnostic_anchor(anchor_idx, DiagnosticAnchorKind::Exact)
                    && let Some(missing_props) =
                        self.missing_required_properties_from_index_signature_source(source, target)
                {
                    // For TS2739, when the source is a non-generic type alias
                    // whose body is a generic Application (`type B = A<X1, X2, ...>`),
                    // tsc unfolds one level to display the application form
                    // `A<X1, X2, ...>` rather than the wrapper alias name `B`.
                    // See `compiler/objectTypeWithStringAndNumberIndexSignatureToAny.ts`
                    // line 91, which expects `Type 'NumberTo<number>'` for
                    // `type NumberToNumber = NumberTo<number>` source. The unfold
                    // is scoped to the missing-properties source only — TS2322
                    // target context and TS2339 receiver keep the alias name.
                    let src_str = if let Some(display) =
                        self.ts2739_alias_of_application_source_display_text(source)
                    {
                        display
                    } else {
                        self.format_type_for_diagnostic_role(
                            source,
                            DiagnosticTypeDisplayRole::AssignmentSource { target, anchor_idx },
                        )
                    };
                    let tgt_str = self
                        .checked_js_global_element_access_fallback_target_display(anchor_idx)
                        .unwrap_or_else(|| {
                            self.format_type_for_diagnostic_role(
                                target,
                                DiagnosticTypeDisplayRole::AssignmentTarget { source, anchor_idx },
                            )
                        });
                    let (message, code) = if missing_props.len() == 1 {
                        let prop_name = self
                            .ctx
                            .types
                            .resolve_atom_ref(missing_props[0])
                            .to_string();
                        if prop_name.starts_with("__js_ctor_brand_") {
                            // Synthetic brand from JS constructor functions — TSC
                            // doesn't report these as missing properties.
                            self.error_type_not_assignable_generic_with_anchor(
                                source, target, anchor_idx,
                            );
                            return;
                        }
                        if tsz_solver::utils::is_synthetic_private_brand_name(&prop_name) {
                            // Private brand mismatch
                            self.error_type_not_assignable_generic_with_anchor(
                                source, target, anchor_idx,
                            );
                            return;
                        }
                        (
                                format_message(
                                    diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                                    &[&prop_name, &src_str, &tgt_str],
                                ),
                                diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                            )
                    } else {
                        let prop_list: Vec<String> = missing_props
                            .iter()
                            .take(4)
                            .map(|name| self.ctx.types.resolve_atom_ref(*name).to_string())
                            .collect();
                        let props_joined = prop_list.join(", ");
                        if missing_props.len() > 4 {
                            let more_count = (missing_props.len() - 4).to_string();
                            (
                                    format_message(
                                        diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                                        &[&src_str, &tgt_str, &props_joined, &more_count],
                                    ),
                                    diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                                )
                        } else {
                            (
                                    format_message(
                                        diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                                        &[&src_str, &tgt_str, &props_joined],
                                    ),
                                    diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                                )
                        }
                    };
                    self.emit_render_request_at_anchor(
                        anchor,
                        DiagnosticRenderRequest::simple(DiagnosticAnchorKind::Exact, code, message),
                    );
                    return;
                }
                // Fallback to generic message
                self.error_type_not_assignable_generic_with_anchor(source, target, anchor_idx);
            }
        }
    }

    /// Narrow the TS2412 source display to the offending member when the
    /// source is a union that contains the target type's shape. In that case
    /// only the `null` / `undefined` (or other non-overlapping) members are
    /// the actual mismatch, and tsc reports just those rather than the full
    /// source union.
    fn exact_optional_source_for_message(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> TypeId {
        if self.same_property_self_assignment_in_presence_false_branch(anchor_idx) {
            return TypeId::UNDEFINED;
        }

        let source_eval = self.evaluate_type_for_assignability(source);
        let target_eval = self.evaluate_type_for_assignability(target);
        let Some(members) =
            crate::query_boundaries::diagnostics::union_members(self.ctx.types, source_eval)
        else {
            return source;
        };
        let mismatched: Vec<TypeId> = members
            .iter()
            .copied()
            .filter(|&m| {
                !self
                    .exact_optional_source_filter_relation_outcome(m, target_eval)
                    .related
            })
            .collect();
        if mismatched.len() == members.len()
            && members.len() == 2
            && members.contains(&TypeId::UNDEFINED)
            && !crate::query_boundaries::class_type::type_includes_undefined(self.ctx.types, target)
        {
            return TypeId::UNDEFINED;
        }
        source
    }

    fn format_exact_optional_target_type_for_message(&mut self, target: TypeId) -> String {
        // Honor any display-alias attached during type construction (e.g.
        // JSDoc `@typedef {object} A` stores `body_type → lazy(def_for_A)`).
        // tsc reports the alias name `A` in TS2375 messages instead of
        // expanding to the body's structural form `{ value?: number; }`.
        if let Some(alias_id) = self.ctx.types.get_display_alias(target)
            && let Some(name) = self.authoritative_assignability_def_name(alias_id)
        {
            return name;
        }
        let target = self.ctx.types.intersection_reduced_for_display(target);
        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_display_properties()
            .with_preserve_optional_parameter_surface_syntax(true)
            .with_preserve_optional_property_surface_syntax(true);
        formatter.format(target).into_owned()
    }

    pub(super) fn format_top_level_assignability_message_types(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> (String, String) {
        let source_str = self
            .related_generic_indexed_access_source_display(source, target)
            .unwrap_or_else(|| self.format_assignability_type_for_message(source, target));
        let mut source_str = self.rewrite_source_display_for_non_literal_target_assignability(
            source, target, source_str,
        );
        let target_str = self.format_assignability_type_for_message(target, source);
        let mut target_str =
            self.rewrite_target_display_for_non_literal_assignability(target, target_str);

        source_str = self.apply_ts2739_nonliteral(source, source_str);
        if target_str.trim() != "{}"
            && let Some(unfolded) = self.ts2739_alias_target_display(target, &target_str)
        {
            target_str = self.format_type_diagnostic(unfolded);
        }

        let should_prefer_authoritative_name = |display: &str| {
            display.starts_with("{ ")
                || display.starts_with("typeof import(")
                || display.contains("& typeof import(")
        };

        if should_prefer_authoritative_name(&source_str)
            && let Some(authoritative) = self.authoritative_assignability_def_name(source)
        {
            source_str = authoritative;
        }
        if should_prefer_authoritative_name(&target_str)
            && let Some(authoritative) = self.authoritative_assignability_def_name(target)
        {
            target_str = authoritative;
        }

        // Non-generic aliases that wrap applications display the application.
        let rewrite_application_alias =
            |state: &Self, ty: TypeId, display: &str| -> Option<String> {
                if display.contains('<') || display.contains('{') || display.contains('|') {
                    return None; // Already expanded
                }
                if display.starts_with('"')
                    || display.starts_with('`')
                    || display == "true"
                    || display == "false"
                {
                    return None; // Keep concrete literal displays instead of repainting alias provenance.
                }
                // JSDoc typedef lazy aliases must not trigger this rewrite.
                let alias = state.ctx.types.get_display_alias(ty)?;
                crate::query_boundaries::diagnostics::application_info(state.ctx.types, alias)?;
                let mut formatter = state
                    .ctx
                    .create_diagnostic_type_formatter()
                    .with_display_properties()
                    .with_skip_application_alias_names();
                Some(formatter.format(ty).into_owned())
            };
        if let Some(rewritten) = rewrite_application_alias(self, source, &source_str) {
            source_str = rewritten;
        }
        if let Some(rewritten) = rewrite_application_alias(self, target, &target_str) {
            target_str = rewritten;
        }
        source_str = self.apply_eval_alias_nonliteral(source, source_str);
        if let Some(display) = self.evaluated_literal_alias_source_display(target) {
            target_str = display;
        }
        source_str =
            self.canonicalize_assignment_numeric_literal_union_display(source, target, source_str);
        target_str =
            self.canonicalize_assignment_numeric_literal_union_display(target, source, target_str);
        if let Some(widened) =
            self.rewrite_standalone_literal_source_for_keyof_display(source, target)
        {
            source_str = widened;
        }
        let (source_str, mut target_str) =
            self.finalize_pair_display_for_diagnostic(source, target, source_str, target_str);
        let mut source_str = source_str;
        let mut static_schema_display = false;
        if let Some(display) = self.static_schema_array_structural_display(source, target) {
            source_str = display;
            static_schema_display = true;
        }
        if let Some(display) = self.static_schema_array_structural_display(target, source) {
            target_str = display;
            static_schema_display = true;
        }
        if let Some(display) = self.static_schema_array_structural_display_text(&target_str, source)
        {
            target_str = display;
            static_schema_display = true;
        }
        if !static_schema_display
            && let Some((direct_source, direct_target)) =
                self.direct_type_param_alias_application_pair_display(source, target)
        {
            source_str = direct_source;
            target_str = direct_target;
        }
        if let Some(display) =
            self.contextual_callable_application_target_display(target, source, &target_str)
        {
            target_str = display;
        }
        source_str =
            self.canonicalize_assignment_numeric_literal_union_display(source, target, source_str);
        target_str =
            self.canonicalize_assignment_numeric_literal_union_display(target, source, target_str);
        (source_str, target_str)
    }

    pub(in crate::error_reporter) fn rewrite_standalone_literal_source_for_keyof_display(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<String> {
        if !self.target_is_generic_keyof_display(target) {
            return None;
        }

        crate::query_boundaries::diagnostics::literal_value(self.ctx.types, source)?;
        match crate::query_boundaries::diagnostics::widen_literal_to_primitive(
            self.ctx.types,
            source,
        ) {
            TypeId::BOOLEAN => Some("boolean".to_string()),
            TypeId::STRING => Some("string".to_string()),
            TypeId::NUMBER => Some("number".to_string()),
            _ => None,
        }
    }

    fn target_is_generic_keyof_display(&mut self, target: TypeId) -> bool {
        if let Some(alias) = self.ctx.types.get_display_alias(target)
            && self.type_is_generic_keyof(alias)
        {
            return true;
        }
        false
    }

    fn type_is_generic_keyof(&mut self, type_id: TypeId) -> bool {
        let Some(operand) =
            crate::query_boundaries::diagnostics::keyof_inner_type(self.ctx.types, type_id)
        else {
            return false;
        };
        crate::query_boundaries::diagnostics::contains_type_parameters(self.ctx.types, operand)
            || crate::query_boundaries::diagnostics::contains_type_parameters(
                self.ctx.types,
                self.evaluate_type_for_assignability(operand),
            )
    }

    pub(super) fn format_top_level_assignability_message_types_at(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> (String, String) {
        let (mut source_str, _) = self.format_top_level_assignability_message_types(source, target);
        if self
            .array_literal_element_source_widening_required_for_display(anchor_idx, source, target)
        {
            let widened = self.widen_type_for_display(source);
            source_str = self.format_assignability_type_for_message(widened, target);
        }
        let mut source_from_annotation = false;
        let mut source_from_array_literal_tuple = false;
        if let Some(expr_idx) = self
            .direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx))
            && let Some(annotation_text) =
                self.declared_type_annotation_text_for_expression(expr_idx)
            && annotation_text.contains('&')
            && !annotation_text.trim_start().starts_with("keyof ")
            && self.should_prefer_declared_source_annotation_display(
                expr_idx,
                source,
                &annotation_text,
            )
        {
            source_str = self
                .declared_intersection_annotation_display_for_expression(expr_idx)
                .unwrap_or_else(|| {
                    self.format_declared_annotation_for_diagnostic(&annotation_text)
                });
            source_from_annotation = true;
        }
        if self
            .collapsed_anonymous_object_intersection_for_assignability_display(source)
            .is_some()
            && let Some(annotation_text) =
                self.line_rhs_declared_intersection_annotation(anchor_idx)
        {
            source_str = self.format_declared_annotation_for_diagnostic(&annotation_text);
            source_from_annotation = true;
        }
        if !source_from_annotation
            && let Some(object_display) =
                self.object_literal_source_type_display(anchor_idx, Some(target))
        {
            source_str = self.rewrite_source_display_for_non_literal_target_assignability(
                source,
                target,
                object_display,
            );
        }
        let expr_idx = self
            .direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx));
        if !source_from_annotation
            && let Some(expr_idx) = expr_idx
            && let Some(display) = self.direct_type_query_primitive_source_display(expr_idx, source)
        {
            source_str = display;
            source_from_annotation = true;
        }
        if !source_from_annotation
            && let Some(expr_idx) = expr_idx
            && let Some(display) =
                self.declared_numeric_literal_union_alias_source_display(expr_idx, source)
        {
            source_str = display;
            source_from_annotation = true;
        }
        // A longhand primitive-keyword union source annotation
        // (`string | number | symbol`, `string | number`) carries no
        // `aliasSymbol`, so tsc renders it by its members rather than repainting
        // the whole union with a coincidentally-shaped non-generic alias
        // (`PropertyKey`, a user `type`) reached through the reverse type-to-def
        // lookup. A written-through reference (`: Zed`) is a `TYPE_REFERENCE`,
        // not a longhand union, so it keeps its name.
        if !source_from_annotation
            && let Some(display) = self.longhand_primitive_union_source_display(anchor_idx, source)
        {
            source_str = display;
            source_from_annotation = true;
        }
        // An inline tuple / function / constructor source annotation
        // (`[number, string]`, `(a: number) => void`, `new () => T`) carries no
        // `aliasSymbol`, so tsc renders its expanded structural form rather than
        // a coincidentally-shaped alias name reached through the reverse
        // type-to-def lookup (#17119) — the head-line-renderer twin of the guard
        // in `format_assignment_source_type_for_diagnostic`.
        if !source_from_annotation
            && let Some(display) =
                self.inline_structural_type_annotation_source_display(anchor_idx, source)
        {
            source_str = display;
            source_from_annotation = true;
        }
        if !source_from_annotation
            && let Some(expr_idx) = expr_idx
            && !self.declared_identifier_has_literal_only_alias_source(expr_idx)
            && let Some(display) = self.declared_identifier_source_display(expr_idx, target, source)
            && self.declared_identifier_candidate_preserves_source_surface(&source_str, &display)
        {
            source_str = display;
            source_from_annotation = true;
        }
        if !source_from_annotation
            && self.target_is_normalized_object_literal_union(target)
            && let Some(expr_idx) = expr_idx
            && let Some(object_display) =
                self.object_literal_source_type_display(expr_idx, Some(target))
        {
            source_str = object_display;
        }
        if !source_from_annotation
            && let Some(expr_idx) = expr_idx
            && let Some(tuple_display) =
                self.array_literal_tuple_source_type_display(expr_idx, source, target)
        {
            source_str = tuple_display;
            source_from_array_literal_tuple = true;
        }
        if self
            .array_literal_element_source_widening_required_for_display(anchor_idx, source, target)
        {
            let widened = self.widen_type_for_display(source);
            source_str = self.format_assignability_type_for_message(widened, target);
        }
        if let Some(display) = self.literal_assignment_source_display_for_target(target, anchor_idx)
        {
            source_str = display;
        }
        let target_str = self.format_type_for_diagnostic_role(
            target,
            DiagnosticTypeDisplayRole::AssignmentTarget { source, anchor_idx },
        );
        if !source_from_annotation
            && let Some(display) = self.declared_generic_alias_source_display_for_target_display(
                anchor_idx,
                source,
                &source_str,
                &target_str,
            )
        {
            source_str = display;
            source_from_annotation = true;
        }
        let (source_str, mut target_str) =
            self.finalize_pair_display_for_diagnostic(source, target, source_str, target_str);
        let mut source_str = source_str;
        // Preserve the literal surface of a plain `as T` / `<T>` assertion
        // source. This `_at` path is distinct from the `AssignmentSource` role
        // path, so both funnel through the shared
        // `assertion_source_literal_display`.
        if !source_from_annotation
            && !source_from_array_literal_tuple
            && let Some(display) = self.assertion_source_literal_display(anchor_idx, source, target)
        {
            source_str = display;
        }
        if !source_from_annotation && !source_from_array_literal_tuple {
            source_str = self.apply_ts2739_nonliteral(source, source_str);
        }
        if target_str.trim() != "{}"
            && let Some(unfolded) = self.ts2739_alias_target_display(target, &target_str)
        {
            target_str = self.format_type_diagnostic(unfolded);
        }
        if let Some(display) = self.static_schema_array_structural_display(source, target) {
            source_str = display;
        }
        if let Some(display) = self.static_schema_array_structural_display(target, source) {
            target_str = display;
        }
        if let Some(display) = self.static_schema_array_structural_display_text(&target_str, source)
        {
            target_str = display;
        }
        if let Some(display) =
            self.contextual_callable_application_target_display(target, source, &target_str)
        {
            target_str = display;
        }
        if !source_from_annotation {
            source_str = self
                .canonicalize_assignment_numeric_literal_union_display(source, target, source_str);
        }
        target_str =
            self.canonicalize_assignment_numeric_literal_union_display(target, source, target_str);
        (source_str, target_str)
    }

    /// Whether `ty` (or its assignability-evaluated form `evaluated`) carries
    /// fresh object-literal `display_properties` — the provenance marker that
    /// distinguishes a fresh expression literal (whose canonical shape is
    /// widened, so it is text-widened for display) from a declared annotation /
    /// named type (rendered verbatim). Shared by the source- and target-side
    /// non-literal display rewrites so both apply the same fresh-vs-declared
    /// discipline.
    fn type_or_evaluated_has_display_properties(&self, ty: TypeId, evaluated: TypeId) -> bool {
        self.ctx.types.get_display_properties(ty).is_some()
            || self.ctx.types.get_display_properties(evaluated).is_some()
    }

    pub(super) fn rewrite_source_display_for_non_literal_target_assignability(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_display: String,
    ) -> String {
        let target_is_constructor_like =
            crate::query_boundaries::diagnostics::function_shape_for_type(self.ctx.types, target)
                .is_some_and(|shape| shape.is_constructor)
                || crate::query_boundaries::diagnostics::callable_shape_for_type(
                    self.ctx.types,
                    target,
                )
                .is_some_and(|shape| !shape.construct_signatures.is_empty());
        let evaluated_source = self.evaluate_type_for_assignability(source);
        let source_has_display_props =
            self.type_or_evaluated_has_display_properties(source, evaluated_source);
        if let Some(display) = self.typeof_result_source_display(evaluated_source, target) {
            return display.to_string();
        }
        if source_has_display_props
            && self.target_is_normalized_object_literal_union(target)
            && display_has_boolean_member_literal_assignability(&source_display)
        {
            return source_display;
        }
        if self.is_literal_sensitive_assignment_target(target)
            || self.target_preserves_literal_surface(target)
            || ([source, evaluated_source].into_iter().any(|candidate| {
                is_function_like_for_literal_member_widening(self.ctx.types, candidate)
            }) && !target_is_constructor_like)
            || !Self::display_has_member_literals_assignability(&source_display)
        {
            return source_display;
        }

        // Application types (generic instantiations like `Foo<{ b?: 1; x: 1 }>`)
        // carry literals in their type arguments — these come from type annotations,
        // not from fresh expression literals, and must NOT be text-widened.
        // tsc always shows literal type args as-is in assignability messages.
        if Self::type_displays_as_application(self.ctx.types, source) {
            return source_display;
        }

        // Declared type annotations (e.g. `var z: { length: 2; }`) store literal
        // property types canonically with no display_properties. Only fresh object
        // literal expressions carry display_properties (canonical=widened, display=literal).
        // tsc preserves the annotation's literal property types in error messages.
        //
        // Skip widening when source has no display_properties AND has at least one direct
        // canonical property of literal type. The "direct" check prevents false positives
        // from outer types like `{ a: inner_fresh }` where the outer is not fresh but inner
        // properties contain fresh types — their outer canonical properties are object types
        // (not literals), so they correctly fall through to the widening path.
        let source_is_array =
            crate::query_boundaries::diagnostics::array_element_type(self.ctx.types, source)
                .is_some()
                || crate::query_boundaries::diagnostics::array_element_type(
                    self.ctx.types,
                    evaluated_source,
                )
                .is_some();
        if !source_has_display_props && !source_is_array {
            // A non-fresh source (no fresh-object-literal display provenance —
            // e.g. produced by `as const`, a declared annotation, or a named
            // type) carries canonical literal members that tsc preserves
            // verbatim at every nesting depth. Only genuinely fresh object
            // literals (which intern a widened canonical shape) are widened for
            // non-literal targets. Detect a literal member at ANY depth, not
            // just the top level, so nested const-asserted literals like
            // `{ p: { q: 1 } }` are preserved instead of text-widened.
            if self.source_carries_canonical_literal_member(evaluated_source) {
                return source_display;
            }
        }

        // For intersection types with display properties (fresh object literal in an
        // intersection), check whether the *target* type has literal-typed properties.
        // tsc preserves literal display when the target expects literals (e.g.
        // `fooProp: "hello" | "world"`), but widens to primitives when the target
        // has non-literal property types (e.g. `fooProp: boolean`).
        let is_intersection_source = [source, self.evaluate_type_for_assignability(source)]
            .into_iter()
            .any(|candidate| {
                crate::query_boundaries::diagnostics::is_intersection_type(
                    self.ctx.types,
                    candidate,
                ) && self.ctx.types.get_display_properties(candidate).is_some()
            });
        if is_intersection_source && self.target_has_literal_typed_properties(target) {
            return source_display;
        }

        let evaluated = self.evaluate_type_for_assignability(source);
        let widened = crate::query_boundaries::diagnostics::widen_type(self.ctx.types, evaluated);
        let widened = self.widen_function_like_display_type(widened);
        let widened = self
            .widen_annotation_literals_for_display(
                widened,
                crate::query_boundaries::diagnostics::AnnotationLiteralWideningPolicy::ALL,
            )
            .type_id;
        self.format_type_for_diagnostic_role(widened, DiagnosticTypeDisplayRole::WidenedDiagnostic)
    }

    /// Returns `true` when a non-fresh source has canonical literal members.
    pub(super) fn source_carries_canonical_literal_member(&self, ty: TypeId) -> bool {
        let mut visiting = rustc_hash::FxHashSet::default();
        self.source_carries_canonical_literal_member_inner(ty, &mut visiting, 0)
    }

    fn source_carries_canonical_literal_member_inner(
        &self,
        ty: TypeId,
        visiting: &mut rustc_hash::FxHashSet<TypeId>,
        depth: usize,
    ) -> bool {
        const MAX_DEPTH: usize = 8;
        if depth > MAX_DEPTH || !visiting.insert(ty) {
            return false;
        }
        let db = self.ctx.types;
        let recurse = |child: TypeId, visiting: &mut rustc_hash::FxHashSet<TypeId>| -> bool {
            crate::query_boundaries::diagnostics::is_literal_type(db, child)
                || self.source_carries_canonical_literal_member_inner(child, visiting, depth + 1)
        };

        // A hybrid object that also carries call/construct signatures
        // (`{ x: number; (): 1 }`) falls through to the signature descent below.
        if let Some(shape) = crate::query_boundaries::diagnostics::object_shape_for_type(db, ty)
            && shape
                .properties
                .iter()
                .any(|p| recurse(p.type_id, visiting))
        {
            return true;
        }
        // Intersections carry their members' canonical literal properties just
        // like unions do — a declared `number & { tag: "x" }` source keeps its
        // `"x"` member in tsc diagnostics. This must run before the
        // `array_element_type`/`tuple_elements` probes below: unlike
        // `object_shape_for_type`, `get_tuple_elements` reduces a *tuple-typed*
        // intersection member down to just that tuple's own elements (picking
        // the most specific tuple for contextual typing), so an intersection
        // whose OTHER member is an object literal (`{ z: 1 } & [string, ...]`)
        // would short-circuit through the tuple arm and never see the object's
        // `z` property — silently widening it in the non-literal-target
        // fallback below (`{ z: number } & [...]`) instead of preserving `1`.
        if let Some(members) = crate::query_boundaries::diagnostics::intersection_members(db, ty) {
            return members.iter().any(|&m| recurse(m, visiting));
        }
        if let Some(elem) = crate::query_boundaries::diagnostics::array_element_type(db, ty) {
            return recurse(elem, visiting);
        }
        if let Some(elements) = crate::query_boundaries::diagnostics::tuple_elements(db, ty) {
            return elements.iter().any(|e| recurse(e.type_id, visiting));
        }
        if let Some(members) = crate::query_boundaries::diagnostics::union_members(db, ty) {
            return members.iter().any(|&m| recurse(m, visiting));
        }
        super::assignability_type_helpers::signature_carries_canonical_literal_member(
            db, ty, visiting, recurse,
        )
    }

    pub(super) fn rewrite_target_display_for_non_literal_assignability(
        &mut self,
        target: TypeId,
        target_display: String,
    ) -> String {
        // Conditional types separate the false branch with `:` (`C extends E ? X : Y`),
        // which the text-based member-literal widener mistakes for an object member and
        // widens (`... : 2` → `... : number`, `... : "no"` → `... : string`). tsc renders
        // deferred conditional branches verbatim, so never text-widen a conditional target.
        if crate::query_boundaries::diagnostics::is_conditional_type(self.ctx.types, target) {
            return target_display;
        }
        let evaluated = self.evaluate_type_for_assignability(target);
        let target_is_function_like = [target, evaluated].into_iter().any(|candidate| {
            is_function_like_for_literal_member_widening(self.ctx.types, candidate)
        });
        if target_is_function_like
            || !Self::display_has_member_literals_assignability(&target_display)
        {
            return target_display;
        }

        // Application types carry literals in type arguments — preserve them.
        if Self::type_displays_as_application(self.ctx.types, target) {
            return target_display;
        }

        // A *declared* target — an annotation or named type with no fresh
        // object-literal display provenance — carries canonical literal members
        // that tsc renders verbatim at every nesting depth. Only a genuinely
        // fresh object-literal target (which interns a widened canonical shape
        // and therefore carries `display_properties`) is text-widened. Mirror
        // the fresh-vs-declared discipline the source side already applies in
        // `rewrite_source_display_for_non_literal_target_assignability`, so a
        // non-object source (e.g. a union) assigned to an anonymous object
        // annotation like `{ a: 1 }` keeps the declared literal target instead
        // of leaking a widened `{ a: number }` (#12179). Fresh-literal targets
        // (with `display_properties`) still widen, preserving the existing
        // role-swapped behavior.
        let target_has_display_props =
            self.type_or_evaluated_has_display_properties(target, evaluated);
        if !target_has_display_props && self.source_carries_canonical_literal_member(evaluated) {
            return target_display;
        }

        let widened = crate::query_boundaries::diagnostics::widen_type(self.ctx.types, evaluated);
        let widened = self.widen_function_like_display_type(widened);
        let widened = self
            .widen_annotation_literals_for_display(
                widened,
                crate::query_boundaries::diagnostics::AnnotationLiteralWideningPolicy::ALL,
            )
            .type_id;
        self.format_type_for_diagnostic_role(widened, DiagnosticTypeDisplayRole::WidenedDiagnostic)
    }

    /// Returns true when `ty` would be formatted as an Application type (e.g. `Foo<{...}>`).
    ///
    /// Application types carry their type arguments from annotations — the literals in those
    /// args represent declared types, not fresh expression values, and must never be text-widened
    /// in `rewrite_{source,target}_display_for_non_literal_*` calls.
    fn type_displays_as_application(
        db: &dyn tsz_solver::construction::TypeDatabase,
        ty: TypeId,
    ) -> bool {
        // Direct Application: Application(Lazy(Foo), [args])
        if crate::query_boundaries::diagnostics::is_generic_application(db, ty) {
            return true;
        }
        // Evaluated Application: concrete Object that carries display_alias → Application
        if let Some(alias) = db.get_display_alias(ty)
            && crate::query_boundaries::diagnostics::is_generic_application(db, alias)
        {
            return true;
        }
        false
    }

    /// Check if the target type has any properties whose types contain literal
    /// types.  Used to decide whether to preserve source literal display in
    /// intersection contexts: tsc shows `"frizzlebizzle"` when the target expects
    /// `"hello" | "world"`, but widens to `string` when the target expects `boolean`.
    fn target_has_literal_typed_properties(&mut self, target: TypeId) -> bool {
        let target = self.evaluate_type_for_assignability(target);
        let shape =
            crate::query_boundaries::diagnostics::object_shape_for_type(self.ctx.types, target)
                .or_else(|| {
                    // For intersection/union targets, check members.
                    crate::query_boundaries::diagnostics::intersection_members(
                        self.ctx.types,
                        target,
                    )
                    .and_then(|members| {
                        members.iter().find_map(|&m| {
                            crate::query_boundaries::diagnostics::object_shape_for_type(
                                self.ctx.types,
                                m,
                            )
                        })
                    })
                });
        let Some(shape) = shape else {
            return false;
        };
        shape
            .properties
            .iter()
            .any(|prop| self.is_literal_sensitive_assignment_target(prop.type_id))
    }

    pub(super) fn display_has_member_literals_assignability(display: &str) -> bool {
        let bytes = display.as_bytes();
        if bytes.len() < 3 {
            return false;
        }
        for i in 0..(bytes.len() - 2) {
            if bytes[i] != b':' || bytes[i + 1] != b' ' {
                continue;
            }
            let rest = &display[i + 2..];
            if rest.starts_with('"')
                || rest.starts_with('\'')
                || rest.starts_with("true")
                || rest.starts_with("false")
            {
                return true;
            }
            if rest
                .as_bytes()
                .first()
                .is_some_and(|b| b.is_ascii_digit() || *b == b'-')
            {
                return true;
            }
        }
        false
    }

    /// Check if a type display string contains duplicate type names in a
    /// union (`Yep | Yep`) or tuple (`[Yep, Yep]`) context.
    pub(super) fn has_duplicate_union_member_names(display: &str) -> bool {
        // Try union split first
        if display.contains(" | ") {
            let members: Vec<&str> = display.split(" | ").collect();
            if members.len() >= 2 {
                for i in 0..members.len() {
                    for j in (i + 1)..members.len() {
                        if members[i] == members[j] {
                            return true;
                        }
                    }
                }
            }
        }
        // Try tuple split (e.g., "[Yep, Yep]")
        let inner = display.strip_prefix('[').and_then(|s| s.strip_suffix(']'));
        if let Some(inner) = inner {
            let members: Vec<&str> = inner.split(", ").collect();
            if members.len() >= 2 {
                for i in 0..members.len() {
                    for j in (i + 1)..members.len() {
                        if members[i] == members[j] {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Text-level literal-annotation widening for generic-signature displays
    /// spliced from multiple rendered types; single-`TypeId` displays use the
    /// type-level widening boundary instead.
    pub(super) fn widen_member_literals_in_display_text(display: &str) -> String {
        let bytes = display.as_bytes();
        let mut out = String::with_capacity(display.len());
        let mut i = 0usize;
        let is_boundary = |b: u8| {
            matches!(
                b,
                b';' | b',' | b'}' | b'>' | b')' | b'|' | b'&' | b']' | b' '
            )
        };
        while i < bytes.len() {
            if i + 2 < bytes.len() && bytes[i] == b':' && bytes[i + 1] == b' ' {
                out.push(':');
                out.push(' ');
                i += 2;

                if i < bytes.len() && bytes[i] == b'"' {
                    i += 1;
                    while i < bytes.len() {
                        if bytes[i] == b'\\' && i + 1 < bytes.len() {
                            i += 2;
                            continue;
                        }
                        if bytes[i] == b'"' {
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                    out.push_str("string");
                    continue;
                }

                if display[i..].starts_with("true")
                    && (i + 4 >= bytes.len() || is_boundary(bytes[i + 4]))
                {
                    out.push_str("boolean");
                    i += 4;
                    continue;
                }
                if display[i..].starts_with("false")
                    && (i + 5 >= bytes.len() || is_boundary(bytes[i + 5]))
                {
                    out.push_str("boolean");
                    i += 5;
                    continue;
                }

                if i < bytes.len() && (bytes[i] == b'-' || bytes[i].is_ascii_digit()) {
                    let mut j = i;
                    if bytes[j] == b'-' {
                        j += 1;
                    }
                    let mut saw_digit = false;
                    while j < bytes.len() && bytes[j].is_ascii_digit() {
                        j += 1;
                        saw_digit = true;
                    }
                    if j < bytes.len() && bytes[j] == b'.' {
                        j += 1;
                        while j < bytes.len() && bytes[j].is_ascii_digit() {
                            j += 1;
                            saw_digit = true;
                        }
                    }
                    if saw_digit && (j >= bytes.len() || is_boundary(bytes[j])) {
                        out.push_str("number");
                        i = j;
                        continue;
                    }
                }
            }

            out.push(bytes[i] as char);
            i += 1;
        }
        out
    }
}
