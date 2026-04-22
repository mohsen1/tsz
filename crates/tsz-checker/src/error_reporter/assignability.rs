//! Type assignability error reporting (TS2322 and related).

use crate::diagnostics::{
    DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes, diagnostic_messages,
    format_message,
};
use crate::error_reporter::fingerprint_policy::{
    DiagnosticAnchorKind, DiagnosticRenderRequest, RelatedInformationPolicy,
};
use crate::state::CheckerState;
use tracing::{Level, trace};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

/// Returns true if the formatted type name matches a built-in wrapper type
/// (Boolean, Number, String, Object). These types inherit properties from Object
/// and missing-property diagnostics should be suppressed in favor of TS2322.
pub(super) fn is_builtin_wrapper_name(name: &str) -> bool {
    matches!(name, "Boolean" | "Number" | "String" | "Object")
}

/// Returns true if the formatted type name represents a TypeScript primitive type.
/// This catches cases where a complex type (e.g., homomorphic mapped type over a
/// primitive) evaluates/displays as a primitive, even if the solver's TypeId doesn't
/// directly represent the primitive.
pub(crate) fn is_primitive_type_name(name: &str) -> bool {
    matches!(
        name,
        "string"
            | "number"
            | "boolean"
            | "bigint"
            | "symbol"
            | "void"
            | "undefined"
            | "null"
            | "never"
    )
}

/// Returns true if the name is a reserved type name that cannot be used as
/// an interface or class name (TS2427/TS2414). Matches tsc's
/// `checkTypeNameIsReserved` which checks the `typeNames` set.
pub(crate) fn is_reserved_type_name(name: &str) -> bool {
    matches!(
        name,
        "any"
            | "unknown"
            | "never"
            | "string"
            | "number"
            | "boolean"
            | "symbol"
            | "bigint"
            | "void"
            | "undefined"
            | "null"
            | "object"
    )
}

/// Returns true if the display string looks like a function/callable type.
/// Used as a fallback when TypeId-level detection fails due to TypeQuery/Lazy wrapping.
/// Function types display as `(params) => ReturnType`.
pub(super) fn is_function_type_display(name: &str) -> bool {
    // A function type display always starts with `(` and contains `) => `.
    name.starts_with('(') && name.contains(") => ")
}

/// Returns true if the property name is a standard Object.prototype method.
/// These are implicitly available on all interfaces/objects through the Object
/// prototype chain. When such a property appears as "missing" in a subtype check,
/// it typically means the source type inherits it implicitly but its `ObjectShape`
/// doesn't include it. In this case, the mismatch is a type compatibility issue
/// (TS2322), not a missing property issue (TS2741).
pub(super) fn is_object_prototype_method(name: impl AsRef<str>) -> bool {
    matches!(
        name.as_ref(),
        "valueOf"
            | "toString"
            | "toLocaleString"
            | "hasOwnProperty"
            | "isPrototypeOf"
            | "propertyIsEnumerable"
            | "constructor"
    )
}

/// Subset of Object.prototype methods that should still be reported as missing
/// when the target is an array-like type. Array types override `toString` and
/// `toLocaleString` with their own signatures, so these should NOT be filtered
/// out from TS2739/TS2740 missing property lists for array targets.
pub(super) fn is_object_prototype_method_for_array_target(name: impl AsRef<str>) -> bool {
    matches!(
        name.as_ref(),
        "valueOf" | "hasOwnProperty" | "isPrototypeOf" | "propertyIsEnumerable" | "constructor"
    )
}

/// Check if a type is a callable application type.
/// This checks if it's an Application type whose base is a callable/function type,
/// or if it's directly a callable/function type.
fn is_callable_application_type(db: &dyn tsz_solver::TypeDatabase, type_id: TypeId) -> bool {
    // Check if it's an application of a callable type
    if let Some(app) = crate::query_boundaries::common::type_application(db, type_id) {
        crate::query_boundaries::common::callable_shape_for_type(db, app.base).is_some()
            || crate::query_boundaries::common::function_shape_for_type(db, app.base).is_some()
    } else {
        // Also check if it's directly a callable/function type
        crate::query_boundaries::common::callable_shape_for_type(db, type_id).is_some()
            || crate::query_boundaries::common::function_shape_for_type(db, type_id).is_some()
    }
}

/// Check if a callable/function type has its own signature-level type parameters.
fn has_own_signature_type_params(db: &dyn tsz_solver::TypeDatabase, type_id: TypeId) -> bool {
    if let Some(shape) = crate::query_boundaries::common::callable_shape_for_type(db, type_id) {
        return shape
            .call_signatures
            .iter()
            .chain(shape.construct_signatures.iter())
            .any(|sig| !sig.type_params.is_empty());
    }
    if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(db, type_id) {
        return !shape.type_params.is_empty();
    }
    false
}

impl<'a> CheckerState<'a> {
    /// Check if the assignment failure is due to exact optional property types.
    ///
    /// When `exactOptionalPropertyTypes` is enabled, optional properties don't
    /// implicitly include `undefined`. If the source has `undefined` for properties
    /// that are optional in the target, this is an exact optional property mismatch
    /// and should produce TS2375 instead of TS2322.
    ///
    /// Mirrors tsc's `getExactOptionalUnassignableProperties` + `isExactOptionalPropertyMismatch`.
    pub(super) fn has_exact_optional_property_mismatch(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if !self.ctx.compiler_options.exact_optional_property_types {
            return false;
        }
        // Evaluate types to resolve Lazy(DefId) references to concrete Object shapes
        let target_eval = self.evaluate_type_for_assignability(target);
        let source_eval = self.evaluate_type_for_assignability(source);
        let Some(target_shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, target_eval)
        else {
            return false;
        };
        let source_shape =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, source_eval);
        for target_prop in &target_shape.properties {
            if !target_prop.optional {
                continue;
            }
            // Check if the source has a property with the same name that includes undefined
            let source_prop_type = source_shape
                .as_ref()
                .and_then(|s| s.properties.iter().find(|p| p.name == target_prop.name))
                .map(|p| p.type_id);
            if let Some(src_type) = source_prop_type
                && crate::query_boundaries::class_type::type_includes_undefined(
                    self.ctx.types,
                    src_type,
                )
            {
                return true;
            }
        }
        false
    }

    /// Detect assignment-to-optional-slot mismatches that should produce TS2412.
    ///
    /// TS2412 applies for exact-optional write targets (e.g. `obj.a = value`)
    /// where the write type excludes `undefined` but the source includes it.
    pub(super) fn has_exact_optional_write_target_mismatch(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> bool {
        if !self.ctx.compiler_options.exact_optional_property_types {
            return false;
        }
        if !crate::query_boundaries::class_type::type_includes_undefined(self.ctx.types, source) {
            return false;
        }
        if crate::query_boundaries::class_type::type_includes_undefined(self.ctx.types, target) {
            return false;
        }

        let anchor_idx = self.ctx.arena.skip_parenthesized_and_assertions(anchor_idx);
        let Some(anchor_node) = self.ctx.arena.get(anchor_idx) else {
            return false;
        };

        let mut write_target_idx = if anchor_node.kind
            == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || anchor_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            Some(anchor_idx)
        } else if anchor_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
            self.ctx
                .arena
                .get_expression_statement(anchor_node)
                .and_then(|stmt| self.ctx.arena.get(stmt.expression))
                .and_then(|expr_node| {
                    if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                        return None;
                    }
                    let binary = self.ctx.arena.get_binary_expr(expr_node)?;
                    if !self.is_assignment_operator(binary.operator_token) {
                        return None;
                    }
                    let lhs = self
                        .ctx
                        .arena
                        .skip_parenthesized_and_assertions(binary.left);
                    let lhs_node = self.ctx.arena.get(lhs)?;
                    (lhs_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        || lhs_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                        .then_some(lhs)
                })
        } else if anchor_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            self.ctx
                .arena
                .get_binary_expr(anchor_node)
                .and_then(|binary| {
                    if !self.is_assignment_operator(binary.operator_token) {
                        return None;
                    }
                    let lhs = self
                        .ctx
                        .arena
                        .skip_parenthesized_and_assertions(binary.left);
                    let lhs_node = self.ctx.arena.get(lhs)?;
                    (lhs_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        || lhs_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                        .then_some(lhs)
                })
        } else {
            None
        };

        if write_target_idx.is_none()
            && let Some(ext) = self.ctx.arena.get_extended(anchor_idx)
            && let Some(parent_node) = self.ctx.arena.get(ext.parent)
            && (parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
            && access.name_or_argument == anchor_idx
        {
            write_target_idx = Some(ext.parent);
        }

        let Some(write_target_idx) = write_target_idx else {
            return false;
        };
        let Some(write_target_node) = self.ctx.arena.get(write_target_idx) else {
            return false;
        };

        let is_property_like_write =
            if write_target_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                true
            } else if write_target_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
                self.ctx
                    .arena
                    .get_access_expr(write_target_node)
                    .is_some_and(|access| {
                        self.get_literal_index_from_node(access.name_or_argument)
                            .is_some()
                            || self
                                .get_literal_string_from_node(access.name_or_argument)
                                .is_some()
                    })
            } else {
                false
            };
        if !is_property_like_write {
            return false;
        }

        let read_target = self
            .get_type_of_node_with_request(write_target_idx, &crate::context::TypingRequest::NONE);
        crate::query_boundaries::class_type::type_includes_undefined(self.ctx.types, read_target)
    }

    /// Get the declaring type name for a property in a target type.
    /// For inherited properties (e.g., from a base class), returns the base class name.
    /// Falls back to formatting the target type if no parent info is available.
    pub(super) fn property_declaring_type_name(
        &self,
        target_type: TypeId,
        property_name: tsz_common::interner::Atom,
    ) -> Option<String> {
        let prop_info = self.property_info_for_display(target_type, property_name)?;
        prop_info
            .parent_id
            .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
            .map(|sym| sym.escaped_name.clone())
    }

    pub(super) fn property_info_for_display(
        &self,
        ty: TypeId,
        name: tsz_common::interner::Atom,
    ) -> Option<tsz_solver::PropertyInfo> {
        crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty)
            .and_then(|shape| {
                shape
                    .properties
                    .iter()
                    .find(|candidate| candidate.name == name)
                    .cloned()
            })
            .or_else(|| {
                crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, ty)
                    .and_then(|shape| {
                        shape
                            .properties
                            .iter()
                            .find(|candidate| candidate.name == name)
                            .cloned()
                    })
            })
    }

    fn callback_initializer_for_assignability_anchor(
        &self,
        anchor_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let callback_argument_in_call_like = |expr_idx: NodeIndex| {
            let expr_node = self.ctx.arena.get(expr_idx)?;
            let args = if matches!(
                expr_node.kind,
                syntax_kind_ext::CALL_EXPRESSION | syntax_kind_ext::NEW_EXPRESSION
            ) {
                self.ctx
                    .arena
                    .get_call_expr(expr_node)?
                    .arguments
                    .as_ref()?
            } else {
                return None;
            };
            args.nodes.iter().find_map(|&arg_idx| {
                let arg_idx = self.ctx.arena.skip_parenthesized_and_assertions(arg_idx);
                let arg_node = self.ctx.arena.get(arg_idx)?;
                (arg_node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || arg_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION)
                    .then_some(arg_idx)
            })
        };

        let mut current = anchor_idx;
        for _ in 0..8 {
            let anchor_node = self.ctx.arena.get(current)?;
            if anchor_node.kind == syntax_kind_ext::ARROW_FUNCTION
                || anchor_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            {
                return Some(current);
            }
            if anchor_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                let property = self.ctx.arena.get_property_assignment(anchor_node)?;
                let initializer = self
                    .ctx
                    .arena
                    .skip_parenthesized_and_assertions(property.initializer);
                let initializer_node = self.ctx.arena.get(initializer)?;
                return (initializer_node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || initializer_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION)
                    .then_some(initializer)
                    .or_else(|| callback_argument_in_call_like(initializer));
            }

            let parent = self.ctx.arena.get_extended(current)?.parent;
            if parent.is_none() {
                return None;
            }
            current = parent;
        }

        None
    }

    fn should_suppress_outer_callback_return_assignability(
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

    fn should_suppress_assignment_after_overload_failure(&self, anchor_idx: NodeIndex) -> bool {
        let Some(anchor_node) = self.ctx.arena.get(anchor_idx) else {
            return false;
        };
        if anchor_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            return false;
        }
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
        self.ctx.diagnostics.iter().any(|diag| {
            diag.code == diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL
                && diag.start >= rhs_node.pos
                && diag.start < rhs_node.end
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
                if prop_name.starts_with("__private_brand_")
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

        crate::query_boundaries::common::object_shape_for_type(self.ctx.types, target_type)
            .and_then(|shape| find_missing(&shape.properties))
            .or_else(|| {
                crate::query_boundaries::common::callable_shape_for_type(
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

        // When a value anchor is supplied, reposition missing-property codes
        // (TS2741/TS2739/TS2740) to anchor on the property value — matching
        // tsc's `elaborateElementwise` behavior that uses the initializer as
        // the error node for missing-property elaborations.
        if let Some(value_anchor_src) = value_anchor_idx {
            let resolved_value_anchor =
                self.resolve_diagnostic_anchor_node(value_anchor_src, DiagnosticAnchorKind::Exact);
            let value_span = self
                .resolve_diagnostic_anchor(resolved_value_anchor, DiagnosticAnchorKind::Exact)
                .map(|anchor| (anchor.start, anchor.length))
                .or_else(|| {
                    self.get_node_span(resolved_value_anchor).map(|(pos, end)| {
                        self.normalized_anchor_span(
                            resolved_value_anchor,
                            pos,
                            end.saturating_sub(pos),
                        )
                    })
                });
            if let Some((start, length)) = value_span {
                for diag in &mut self.ctx.diagnostics[diag_count_before..] {
                    if matches!(
                        diag.code,
                        diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
                            | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
                            | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
                    ) {
                        diag.start = start;
                        diag.length = length;
                    }
                }
            }
        }

        if !downgrade_missing_to_2322 {
            return;
        }

        let needs_downgrade = self.ctx.diagnostics[diag_count_before..].iter().any(|d| {
            matches!(
                d.code,
                diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
                    | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
                    | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
            )
        });
        if needs_downgrade {
            let src_str = self.format_type_for_assignability_message(source);
            let tgt_str = self.format_type_for_assignability_message(target);
            let new_message = crate::diagnostics::format_message(
                crate::diagnostics::diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            for diag in &mut self.ctx.diagnostics[diag_count_before..] {
                if matches!(
                    diag.code,
                    diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
                        | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
                        | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
                ) {
                    diag.code = diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE;
                    diag.message_text = new_message.clone();
                }
            }
        }
    }
    pub fn error_type_does_not_satisfy_the_expected_type(
        &mut self,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        keyword_pos: Option<u32>,
    ) {
        if self.should_suppress_assignability_diagnostic(source, target) {
            return;
        }

        let reason = self
            .analyze_assignability_failure(source, target)
            .failure_reason;

        // For TS1360, point the diagnostic at the `satisfies` keyword position
        // when available, rather than walking up to the enclosing statement.
        let anchor_idx = if keyword_pos.is_some() {
            self.resolve_diagnostic_anchor_node(idx, DiagnosticAnchorKind::Exact)
        } else {
            self.resolve_diagnostic_anchor_node(idx, DiagnosticAnchorKind::RewriteAssignment)
        };

        let mut base_diag = match reason {
            Some(reason) => self.render_failure_reason(&reason, source, target, anchor_idx, 0),
            None => {
                let Some(anchor) =
                    self.resolve_diagnostic_anchor(anchor_idx, DiagnosticAnchorKind::Exact)
                else {
                    return;
                };
                let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
                    self.ctx.types,
                    &self.ctx.binder.symbols,
                    self.ctx.file_name.as_str(),
                )
                .with_def_store(&self.ctx.definition_store)
                .with_namespace_module_names(&self.ctx.namespace_module_names);
                let diag = builder.type_not_assignable(source, target, anchor.start, anchor.length);
                diag.to_checker_diagnostic(&self.ctx.file_name)
            }
        };

        // Mutate the top-level diagnostic to be TS1360
        let src_str = self.format_type_for_assignability_message(source);
        let tgt_str = self.format_type_for_assignability_message(target);
        use tsz_common::diagnostics::data::diagnostic_codes;
        use tsz_common::diagnostics::data::diagnostic_messages;
        use tsz_common::diagnostics::format_message;

        let msg = format_message(
            diagnostic_messages::TYPE_DOES_NOT_SATISFY_THE_EXPECTED_TYPE,
            &[&src_str, &tgt_str],
        );

        if base_diag.code != diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_EXPECTED_TYPE {
            let new_related = self
                .related_from_diagnostic(&base_diag, RelatedInformationPolicy::WRAPPED_DIAGNOSTIC);
            base_diag.code = diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_EXPECTED_TYPE;
            base_diag.message_text = msg;
            base_diag.related_information = new_related;
        }

        // Override the diagnostic start position to the `satisfies` keyword
        // when available. tsc points TS1360 at the keyword, not the expression.
        if let Some(kw_pos) = keyword_pos {
            base_diag.start = kw_pos;
            // "satisfies" is 9 characters long
            base_diag.length = 9;
        }

        self.ctx.push_diagnostic(base_diag);
    }

    /// Diagnose why an assignment failed and report a detailed error.
    pub fn diagnose_assignment_failure(&mut self, source: TypeId, target: TypeId, idx: NodeIndex) {
        let anchor_idx =
            self.resolve_diagnostic_anchor_node(idx, DiagnosticAnchorKind::RewriteAssignment);
        self.diagnose_assignment_failure_with_anchor(source, target, anchor_idx);
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
        // Centralized suppression for TS2322 cascades on unresolved escape-hatch types.
        if self.should_suppress_assignability_diagnostic(source, target) {
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
        if self.should_suppress_assignment_after_overload_failure(anchor_idx) {
            return;
        }

        let has_callable_shape = |this: &mut Self, ty: TypeId| {
            crate::query_boundaries::common::function_shape_for_type(this.ctx.types, ty).is_some()
                || crate::query_boundaries::common::callable_shape_for_type(this.ctx.types, ty)
                    .is_some()
                || {
                    let evaluated = this.evaluate_type_with_env(ty);
                    crate::query_boundaries::common::function_shape_for_type(
                        this.ctx.types,
                        evaluated,
                    )
                    .is_some()
                        || crate::query_boundaries::common::callable_shape_for_type(
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

            let source_type = self.format_type_diagnostic(source);
            let target_type = self.format_type_diagnostic(target);
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

        // TS2375: exactOptionalPropertyTypes — undefined assigned to optional property without undefined.
        if self.has_exact_optional_property_mismatch(source, target) {
            let src_str =
                self.format_assignment_source_type_for_diagnostic(source, target, anchor_idx);
            let tgt_str = self.format_assignability_type_for_message(target, source);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_WITH_EXACTOPTIONALPROPERTYTYPES_TRUE_CONSIDER_ADD,
                &[&src_str, &tgt_str],
            );
            if !self.emit_render_request(
                anchor_idx,
                DiagnosticRenderRequest::simple(
                    DiagnosticAnchorKind::Exact,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_WITH_EXACTOPTIONALPROPERTYTYPES_TRUE_CONSIDER_ADD,
                    message,
                ),
            ) {
                return;
            }
            return;
        }

        // TS2412: exactOptionalPropertyTypes write target mismatch (property/element write).
        if self.has_exact_optional_write_target_mismatch(source, target, anchor_idx) {
            let src_str =
                self.format_assignment_source_type_for_diagnostic(source, target, anchor_idx);
            let tgt_str = self.format_assignability_type_for_message(target, source);
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

        // Trace what's happening with contextualTyping33
        if self.ctx.file_name.contains("contextualTyping33") {
            let _src_str = self.format_type_diagnostic(source);
            let _tgt_str = self.format_type_diagnostic(target);
            tracing::trace!(
                source = %_src_str, target = %_tgt_str, ?reason,
                "diagnose_assignment"
            );
        }

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
                // ExcessProperty errors need special handling: emit at the property position,
                // not the statement position. Find the object literal and call the excess
                // property checker to emit at the correct position.
                if matches!(
                    failure_reason,
                    tsz_solver::SubtypeFailureReason::ExcessProperty { .. }
                ) {
                    // Walk through statements and binary expressions to find the object literal
                    let start_idx = if let Some(node) = self.ctx.arena.get(anchor_idx) {
                        // If anchor is a return statement, start from its expression
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
                    let literal_idx = self.find_rhs_object_literal(start_idx);
                    if let Some(obj_idx) = literal_idx {
                        self.check_object_literal_excess_properties(source, target, obj_idx);
                    }
                    // If we can't find an object literal, the solver's excess property
                    // check may be from a non-literal fresh type (shouldn't happen in
                    // typical code, but fallback to avoid silent suppression).
                    return;
                }
                // Skip MissingProperty for computed symbol expressions (TS2339 emitted separately).
                if let tsz_solver::SubtypeFailureReason::MissingProperty { property_name, .. } =
                    &failure_reason
                {
                    let pn = self.ctx.types.resolve_atom_ref(*property_name);
                    if pn.starts_with("[Symbol.") || pn.starts_with("__js_ctor_brand_") {
                        return;
                    }
                }
                if is_callable_application_type(self.ctx.types, source)
                    && is_callable_application_type(self.ctx.types, target)
                    && self.should_suppress_outer_callback_return_assignability(target, anchor_idx)
                {
                    return;
                }
                let diag =
                    self.render_failure_reason(failure_reason, source, target, anchor_idx, 0);
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
                    let src_str = self
                        .format_assignment_source_type_for_diagnostic(source, target, anchor_idx);
                    let tgt_str = self
                        .format_assignment_target_type_for_diagnostic(target, source, anchor_idx);
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
                        if prop_name.starts_with("__private_brand") {
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

    pub(super) fn format_top_level_assignability_message_types(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> (String, String) {
        let source_str = self.format_assignability_type_for_message(source, target);
        let mut source_str = self.rewrite_source_display_for_non_literal_target_assignability(
            source, target, source_str,
        );
        let target_str = self.format_assignability_type_for_message(target, source);
        let mut target_str =
            self.rewrite_target_display_for_non_literal_assignability(target, target_str);

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

        // For non-generic type aliases whose evaluated form has a display_alias
        // (i.e., the alias wraps a generic Application like `type Foo = Id<{...}>`),
        // tsc shows the Application form in TS2322 messages. Replace the alias name
        // with the display_alias-based formatter output.
        let rewrite_application_alias =
            |state: &Self, ty: TypeId, display: &str| -> Option<String> {
                if display.contains('<') || display.contains('{') || display.contains('|') {
                    return None; // Already expanded
                }
                // Only for types that have a display_alias (were produced by Application eval)
                state.ctx.types.get_display_alias(ty)?;
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
        if let Some(display) = self.evaluated_literal_alias_source_display(source) {
            source_str = display;
        }
        if let Some(display) = self.evaluated_literal_alias_source_display(target) {
            target_str = display;
        }
        source_str = self.canonicalize_assignment_numeric_literal_union_display(source_str);
        target_str = self.canonicalize_assignment_numeric_literal_union_display(target_str);
        if let Some(widened) = self.rewrite_standalone_literal_source_for_keyof_display(
            &source_str,
            &target_str,
            target,
        ) {
            source_str = widened;
        }
        (source_str, target_str)
    }

    pub(in crate::error_reporter) fn rewrite_standalone_literal_source_for_keyof_display(
        &mut self,
        source_display: &str,
        target_display: &str,
        target: TypeId,
    ) -> Option<String> {
        let evaluated_target = self.evaluate_type_for_assignability(target);
        let target_alias_origin = self
            .ctx
            .types
            .get_display_alias(target)
            .or_else(|| self.ctx.types.get_display_alias(evaluated_target));
        let target_is_generic_keyof =
            crate::query_boundaries::common::contains_type_parameters(self.ctx.types, target)
                || crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    evaluated_target,
                )
                || target_alias_origin
                    .and_then(|alias| {
                        crate::query_boundaries::common::keyof_inner_type(self.ctx.types, alias)
                    })
                    .is_some_and(|operand| {
                        crate::query_boundaries::common::contains_type_parameters(
                            self.ctx.types,
                            operand,
                        ) || crate::query_boundaries::common::contains_type_parameters(
                            self.ctx.types,
                            self.evaluate_type_for_assignability(operand),
                        )
                    });
        if !target_display.starts_with("keyof ") || !target_is_generic_keyof {
            return None;
        }

        if source_display == "true" || source_display == "false" {
            return Some("boolean".to_string());
        }
        if source_display.starts_with('"') && source_display.ends_with('"') {
            return Some("string".to_string());
        }
        if source_display.parse::<f64>().is_ok() {
            return Some("number".to_string());
        }

        None
    }

    pub(super) fn format_top_level_assignability_message_types_at(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> (String, String) {
        let (source_str, _) = self.format_top_level_assignability_message_types(source, target);
        let target_str =
            self.format_assignment_target_type_for_diagnostic(target, source, anchor_idx);
        (source_str, target_str)
    }

    pub(super) fn rewrite_source_display_for_non_literal_target_assignability(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_display: String,
    ) -> String {
        let target_is_constructor_like =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, target)
                .is_some_and(|shape| shape.is_constructor)
                || crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, target)
                    .is_some_and(|shape| !shape.construct_signatures.is_empty());

        if self.is_literal_sensitive_assignment_target(target)
            || (source_display.contains("=>") && !target_is_constructor_like)
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
        let evaluated_source = self.evaluate_type_for_assignability(source);
        let source_has_display_props = self.ctx.types.get_display_properties(source).is_some()
            || self
                .ctx
                .types
                .get_display_properties(evaluated_source)
                .is_some();
        let source_is_array =
            crate::query_boundaries::common::array_element_type(self.ctx.types, source).is_some()
                || crate::query_boundaries::common::array_element_type(
                    self.ctx.types,
                    evaluated_source,
                )
                .is_some();
        if !source_has_display_props && !source_is_array {
            let has_direct_literal_prop = crate::query_boundaries::common::object_shape_for_type(
                self.ctx.types,
                evaluated_source,
            )
            .is_some_and(|shape| {
                shape.properties.iter().any(|p| {
                    crate::query_boundaries::common::is_literal_type(self.ctx.types, p.type_id)
                })
            });
            if has_direct_literal_prop {
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
                crate::query_boundaries::common::is_intersection_type(self.ctx.types, candidate)
                    && self.ctx.types.get_display_properties(candidate).is_some()
            });
        if is_intersection_source && self.target_has_literal_typed_properties(target) {
            return source_display;
        }

        let evaluated = self.evaluate_type_for_assignability(source);
        let widened = crate::query_boundaries::common::widen_type(self.ctx.types, evaluated);
        let widened = self.widen_function_like_display_type(widened);
        let widened_display = self.format_type_diagnostic_widened(widened);
        if Self::display_has_member_literals_assignability(&widened_display) {
            Self::widen_member_literals_in_display_text(&widened_display)
        } else {
            widened_display
        }
    }

    pub(super) fn rewrite_target_display_for_non_literal_assignability(
        &mut self,
        target: TypeId,
        target_display: String,
    ) -> String {
        // Callable types use syntax like `{ (x: "foo"): number; }` which has `: "` pattern
        // but these are parameter literals that should be preserved, not object property
        // literals that should be widened. Skip rewriting for callable types.
        let is_callable_type =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, target)
                .is_some();
        if target_display.contains("=>")
            || is_callable_type
            || !Self::display_has_member_literals_assignability(&target_display)
        {
            return target_display;
        }

        // Application types carry literals in type arguments — preserve them.
        if Self::type_displays_as_application(self.ctx.types, target) {
            return target_display;
        }
        let evaluated = self.evaluate_type_for_assignability(target);
        let widened = crate::query_boundaries::common::widen_type(self.ctx.types, evaluated);
        let widened = self.widen_function_like_display_type(widened);
        let widened_display = self.format_type_diagnostic_widened(widened);
        if Self::display_has_member_literals_assignability(&widened_display) {
            Self::widen_member_literals_in_display_text(&widened_display)
        } else {
            widened_display
        }
    }

    /// Returns true when `ty` would be formatted as an Application type (e.g. `Foo<{...}>`).
    ///
    /// Application types carry their type arguments from annotations — the literals in those
    /// args represent declared types, not fresh expression values, and must never be text-widened
    /// in `rewrite_{source,target}_display_for_non_literal_*` calls.
    fn type_displays_as_application(db: &dyn tsz_solver::TypeDatabase, ty: TypeId) -> bool {
        // Direct Application: Application(Lazy(Foo), [args])
        if crate::query_boundaries::common::is_generic_application(db, ty) {
            return true;
        }
        // Evaluated Application: concrete Object that carries display_alias → Application
        if let Some(alias) = db.get_display_alias(ty)
            && crate::query_boundaries::common::is_generic_application(db, alias)
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
        let shape = crate::query_boundaries::common::object_shape_for_type(self.ctx.types, target)
            .or_else(|| {
                // For intersection/union targets, check members.
                crate::query_boundaries::common::intersection_members(self.ctx.types, target)
                    .and_then(|members| {
                        members.iter().find_map(|&m| {
                            crate::query_boundaries::common::object_shape_for_type(
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
            if rest.starts_with('"') || rest.starts_with("true") || rest.starts_with("false") {
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

    /// Internal generic error reporting for type assignability failures.
    pub(crate) fn error_type_not_assignable_generic_at(
        &mut self,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
    ) {
        let anchor_idx =
            self.resolve_diagnostic_anchor_node(idx, DiagnosticAnchorKind::RewriteAssignment);
        self.error_type_not_assignable_generic_with_anchor(source, target, anchor_idx);
    }

    fn error_type_not_assignable_generic_with_anchor(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) {
        if source == target {
            return;
        }

        // Suppress cascade errors from unresolved types
        if source == TypeId::ERROR
            || target == TypeId::ERROR
            // any is assignable to everything except never — tsc reports TS2322 for any→never
            || (source == TypeId::ANY && target != TypeId::NEVER)
            || target == TypeId::ANY
            || source == TypeId::UNKNOWN
            || target == TypeId::UNKNOWN
        {
            return;
        }

        // Suppress TS2322 for callable types with generic type parameters from outer
        // context. Skip the suppression when both sides have their own signature-level
        // type params — the solver handles generic-to-generic comparison correctly.
        let src_callable = is_callable_application_type(self.ctx.types, source);
        let tgt_callable = is_callable_application_type(self.ctx.types, target);
        let has_type_params =
            crate::query_boundaries::common::contains_type_parameters(self.ctx.types, source);
        let both_have_own_sig_params = has_own_signature_type_params(self.ctx.types, source)
            && has_own_signature_type_params(self.ctx.types, target);
        if src_callable && tgt_callable && has_type_params && !both_have_own_sig_params {
            return;
        }

        if let Some(anchor) =
            self.resolve_diagnostic_anchor(anchor_idx, DiagnosticAnchorKind::Exact)
        {
            if is_callable_application_type(self.ctx.types, source)
                && is_callable_application_type(self.ctx.types, target)
                && self.should_suppress_outer_callback_return_assignability(target, anchor_idx)
            {
                return;
            }

            // Precedence gate: suppress fallback TS2322 when a more specific
            // diagnostic is already present at the same span.
            if self.has_more_specific_diagnostic_at_span(anchor.start, anchor.length) {
                return;
            }

            if let Some(missing_props) =
                self.missing_required_properties_from_index_signature_source(source, target)
            {
                let src_str =
                    self.format_assignment_source_type_for_diagnostic(source, target, anchor_idx);
                let tgt_str =
                    self.format_assignment_target_type_for_diagnostic(target, source, anchor_idx);
                let (message, code) = if missing_props.len() == 1 {
                    let prop_name = self
                        .ctx
                        .types
                        .resolve_atom_ref(missing_props[0])
                        .to_string();
                    if prop_name.starts_with("__js_ctor_brand_") {
                        // Synthetic brand from JS constructor functions — TSC
                        // doesn't report these as missing properties.
                        return;
                    }
                    if prop_name.starts_with("__private_brand") {
                        if let Some((display_prop, owner_name, visibility)) =
                            self.private_or_protected_brand_backing_member_display(target, None)
                        {
                            (
                                self.private_or_protected_assignability_message(
                                    &src_str,
                                    &tgt_str,
                                    &display_prop,
                                    &owner_name,
                                    visibility,
                                    self.property_info_for_display(
                                        source,
                                        self.ctx.types.intern_string(&display_prop),
                                    )
                                    .map(|prop| prop.visibility),
                                ),
                                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                            )
                        } else {
                            (
                                format_message(
                                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                                    &[&src_str, &tgt_str],
                                ),
                                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                            )
                        }
                    } else {
                        (
                            format_message(
                                diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                                &[&prop_name, &src_str, &tgt_str],
                            ),
                            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                        )
                    }
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

            let src_str =
                self.format_assignment_source_type_for_diagnostic(source, target, anchor_idx);
            let tgt_str =
                self.format_assignment_target_type_for_diagnostic(target, source, anchor_idx);
            // TS2719: when both types display identically but are different,
            // emit "Two different types with this name exist" instead of TS2322.
            let authoritative_src = self.authoritative_assignability_def_name(source);
            let authoritative_tgt = self.authoritative_assignability_def_name(target);
            let authoritative_names_differ = authoritative_src
                .as_ref()
                .zip(authoritative_tgt.as_ref())
                .is_some_and(|(src, tgt)| src != tgt);

            let (message, code) = if src_str == tgt_str && !authoritative_names_differ {
                (
                    format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_TWO_DIFFERENT_TYPES_WITH_THIS_NAME_EXIST_BUT_THEY,
                        &[&src_str, &tgt_str],
                    ),
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_TWO_DIFFERENT_TYPES_WITH_THIS_NAME_EXIST_BUT_THEY,
                )
            } else {
                let source_generic_base = src_str.split_once('<').map(|(base, _)| base);
                let target_generic_base = tgt_str.split_once('<').map(|(base, _)| base);
                let preserve_generic_nominal_pair = src_str.contains('<')
                    && tgt_str.contains('<')
                    && authoritative_src == authoritative_tgt
                    && source_generic_base == target_generic_base
                    && authoritative_src.as_deref() == source_generic_base;
                let source_name = if src_str.starts_with("typeof ") || preserve_generic_nominal_pair
                {
                    src_str.as_str()
                } else {
                    authoritative_src.as_deref().unwrap_or(&src_str)
                };
                let target_name = if tgt_str.starts_with("typeof ") || preserve_generic_nominal_pair
                {
                    tgt_str.as_str()
                } else {
                    authoritative_tgt.as_deref().unwrap_or(&tgt_str)
                };
                (
                    format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[source_name, target_name],
                    ),
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                )
            };
            self.emit_render_request_at_anchor(
                anchor,
                DiagnosticRenderRequest::simple(DiagnosticAnchorKind::Exact, code, message),
            );
        }
    }

    // `render_failure_reason` has been moved to `render_failure.rs`.
    // The `format_top_level_assignability_message_types` helper remains here
    // because it is used by both assignability entry points and the render module.
}
