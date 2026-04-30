//! Helper methods for assignability error reporting.
//! Extracted from `assignability.rs` for maintainability.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::error_reporter::assignability::is_object_prototype_method;
use crate::error_reporter::fingerprint_policy::{
    DiagnosticAnchorKind, DiagnosticRenderRequest, RelatedInformationPolicy,
};
use crate::error_reporter::type_display_policy::DiagnosticTypeDisplayRole;
use crate::state::{CheckerState, MemberAccessLevel};
use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

use crate::query_boundaries::type_checking_utilities as query_utils;

impl<'a> CheckerState<'a> {
    pub(crate) fn recover_unknown_array_source_type_for_display(
        &mut self,
        source: TypeId,
        idx: NodeIndex,
        depth: u32,
    ) -> TypeId {
        if depth != 0
            || crate::query_boundaries::common::array_element_type(self.ctx.types, source).is_none()
        {
            return source;
        }

        let Some(expr_idx) = self.assignment_source_expression(idx) else {
            return source;
        };
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return source;
        };

        if node.kind == tsz_parser::parser::syntax_kind_ext::CALL_EXPRESSION
            || node.kind == tsz_parser::parser::syntax_kind_ext::NEW_EXPRESSION
        {
            let Some(call) = self.ctx.arena.get_call_expr(node) else {
                return source;
            };
            let Some(args) = call.arguments.as_ref() else {
                return source;
            };
            let Some(&first_arg) = args.nodes.first() else {
                return source;
            };

            let first_arg_type = self.get_type_of_node(first_arg);
            if matches!(first_arg_type, TypeId::ERROR | TypeId::UNKNOWN) {
                return source;
            }

            let element_type =
                crate::query_boundaries::common::array_element_type(self.ctx.types, first_arg_type)
                    .or_else(|| {
                        tsz_solver::operations::get_iterator_info(
                            self.ctx.types,
                            first_arg_type,
                            false,
                        )
                        .map(|info| info.yield_type)
                    });
            let Some(element_type) = element_type else {
                return source;
            };
            if matches!(element_type, TypeId::ERROR | TypeId::UNKNOWN) {
                return source;
            }

            let recovered = self
                .ctx
                .types
                .array(self.widen_type_for_display(element_type));
            if recovered != source {
                return recovered;
            }
        }

        source
    }

    /// Report a type not assignable error with detailed elaboration.
    ///
    /// This method uses the solver's "explain" API to determine WHY the types
    /// are incompatible (e.g., missing property, incompatible property types,
    /// etc.) and produces a richer diagnostic with that information.
    ///
    /// **Architecture Note**: This follows the "Check Fast, Explain Slow" pattern.
    /// The `is_assignable_to` check is fast (boolean). This explain call is slower
    /// but produces better error messages. Only call this after a failed check.
    pub fn error_type_not_assignable_with_reason_at(
        &mut self,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
    ) {
        if self.is_assignable_to(source, target)
            || self.is_nested_same_wrapper_application_assignment(source, target)
            || self.type_contains_invalid_mapped_key_type(target)
            || Self::looks_like_invalid_optional_mapped_display(
                &self.format_type_diagnostic(target),
            )
        {
            return;
        }
        self.diagnose_assignment_failure(source, target, idx);
    }

    /// Report a type not assignable error with detailed elaboration, preserving
    /// the provided anchor exactly instead of walking to an assignment anchor.
    pub fn error_type_not_assignable_with_reason_at_anchor(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) {
        if self.is_assignable_to(source, target)
            || self.is_nested_same_wrapper_application_assignment(source, target)
            || self.type_contains_invalid_mapped_key_type(target)
            || Self::looks_like_invalid_optional_mapped_display(
                &self.format_type_diagnostic(target),
            )
        {
            return;
        }
        self.diagnose_assignment_failure_with_anchor(source, target, anchor_idx);
    }

    fn looks_like_invalid_optional_mapped_display(display: &str) -> bool {
        // Recognise mapped-type displays of the shape `{ [<name> in <key>]?: <…> | undefined; }`
        // regardless of the iteration variable name. The previous version
        // hardcoded `[P in ` and missed every other valid name (`K`, `key`,
        // `X`, etc.) that the printer emits — `Readonly<T> = { [K in keyof
        // T]: T[K] }` would slip past, defeating this carve-out.
        let Some(rest) = display.strip_prefix("{ [") else {
            return false;
        };
        let Some(space_idx) = rest.find(' ') else {
            return false;
        };
        let var_name = &rest[..space_idx];
        if var_name.is_empty()
            || !var_name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
            || !var_name
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        {
            return false;
        }
        let after_var = &rest[space_idx + 1..];
        after_var.starts_with("in ")
            && display.contains("]?: ")
            && display.ends_with(" | undefined; }")
    }

    /// Report a type not assignable error using pre-computed display types.
    /// This is used for callback return type errors where we want to show the full
    /// function types in the error message (e.g., "Type '() => string' is not assignable
    /// to type '{ (): number; (i: number): number; }'") instead of just the return types.
    pub(crate) fn error_type_not_assignable_at_with_display_types(
        &mut self,
        source_for_display: TypeId,
        target_for_display: TypeId,
        anchor_idx: NodeIndex,
    ) {
        let (start, length) = self
            .resolve_diagnostic_anchor(
                anchor_idx,
                super::fingerprint_policy::DiagnosticAnchorKind::Exact,
            )
            .map(|anchor| (anchor.start, anchor.length))
            .unwrap_or_else(|| {
                let (pos, end) = self.get_node_span(anchor_idx).unwrap_or((0, 0));
                self.normalized_anchor_span(anchor_idx, pos, end.saturating_sub(pos))
            });
        let source_is_function_like = crate::query_boundaries::common::callable_shape_for_type(
            self.ctx.types,
            source_for_display,
        )
        .is_some()
            || crate::query_boundaries::common::function_shape_for_type(
                self.ctx.types,
                source_for_display,
            )
            .is_some();
        let target_is_function_like = crate::query_boundaries::common::callable_shape_for_type(
            self.ctx.types,
            target_for_display,
        )
        .is_some()
            || crate::query_boundaries::common::function_shape_for_type(
                self.ctx.types,
                target_for_display,
            )
            .is_some();
        let (source_str, target_str) = if source_is_function_like || target_is_function_like {
            (
                self.format_type_diagnostic(source_for_display),
                self.format_type_diagnostic(target_for_display),
            )
        } else {
            (
                self.format_type_for_diagnostic_role(
                    source_for_display,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target: target_for_display,
                        anchor_idx,
                    },
                ),
                self.format_type_for_diagnostic_role(
                    target_for_display,
                    DiagnosticTypeDisplayRole::AssignmentTarget {
                        source: source_for_display,
                        anchor_idx,
                    },
                ),
            )
        };
        let message = crate::diagnostics::format_message(
            crate::diagnostics::diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        );
        self.ctx
            .push_diagnostic(crate::diagnostics::Diagnostic::error(
                self.ctx.file_name.clone(),
                start,
                length,
                message,
                crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            ));
    }

    /// Report a type not assignable error using a pre-computed failure reason.
    /// This renders the failure reason with the provided display types and pushes the diagnostic.
    pub(crate) fn error_type_not_assignable_with_reason_and_display(
        &mut self,
        source_for_display: TypeId,
        target_for_display: TypeId,
        reason: &tsz_solver::SubtypeFailureReason,
        anchor_idx: NodeIndex,
    ) {
        let diag = self.render_failure_reason(
            reason,
            source_for_display,
            target_for_display,
            anchor_idx,
            0,
        );
        self.ctx.push_diagnostic(diag);
    }

    /// Report constructor accessibility mismatch error.
    pub(crate) fn error_constructor_accessibility_not_assignable(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_level: Option<MemberAccessLevel>,
        target_level: Option<MemberAccessLevel>,
        idx: NodeIndex,
    ) {
        let source_type = self.format_type_diagnostic(source);
        let target_type = self.format_type_diagnostic(target);
        let message = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_type, &target_type],
        );
        let detail = format!(
            "Cannot assign a '{}' constructor type to a '{}' constructor type.",
            Self::constructor_access_name(source_level),
            Self::constructor_access_name(target_level),
        );

        // Build related info referencing the anchor span — since we don't know
        // the span yet, use a placeholder (0, 0) and let emit_render_request
        // fill it in via the anchor. Actually, the related info needs the span.
        // Resolve anchor first to get the span for the related item.
        let Some(anchor) = self.resolve_diagnostic_anchor(idx, DiagnosticAnchorKind::Exact) else {
            return;
        };

        let related = vec![crate::diagnostics::DiagnosticRelatedInformation {
            category: crate::diagnostics::DiagnosticCategory::Error,
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
    }

    /// Check if the diagnostic anchor node traces back to an assignment target
    /// whose variable declaration has an intersection type annotation.
    ///
    /// For `y = a;` where `y: { a: string } & { b: string }`:
    ///   anchor (`ExpressionStatement`) → expression (`BinaryExpression`) → left (Identifier)
    ///   → symbol → `value_declaration` (`VariableDeclaration`) → `type_annotation` (`IntersectionType`)
    /// Check if the SOURCE (RHS) of the assignment at the anchor has an intersection type
    /// annotation on its declaration. This is needed because intersection types may be
    /// flattened into Object types by the solver, losing the intersection structure.
    pub(super) fn anchor_source_has_intersection_annotation(&self, anchor_idx: NodeIndex) -> bool {
        self.anchor_source_intersection_check_inner(anchor_idx)
            .unwrap_or(false)
    }

    fn anchor_source_intersection_check_inner(&self, anchor_idx: NodeIndex) -> Option<bool> {
        use tsz_parser::parser::syntax_kind_ext;

        let anchor_node = self.ctx.arena.get(anchor_idx)?;

        // Walk from anchor to the assignment source (RHS) expression
        let source_expr_idx = if anchor_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
            let expr_stmt = self.ctx.arena.get_expression_statement(anchor_node)?;
            let expr_node = self.ctx.arena.get(expr_stmt.expression)?;
            if expr_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                let binary = self.ctx.arena.get_binary_expr(expr_node)?;
                binary.right
            } else {
                return Some(false);
            }
        } else if anchor_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            // For `let x: T = source`, the source is the initializer
            let var_decl = self.ctx.arena.get_variable_declaration(anchor_node)?;
            var_decl.initializer
        } else {
            return Some(false);
        };

        // Check if the source is an identifier
        let ident_node = self.ctx.arena.get(source_expr_idx)?;
        if ident_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return Some(false);
        }

        // Resolve identifier to symbol
        let sym_id = self.resolve_identifier_symbol(source_expr_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;

        // Get value declaration
        let decl_node = self.ctx.arena.get(symbol.value_declaration)?;

        // Check if it's a variable declaration with an intersection type annotation
        if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
            if var_decl.type_annotation.is_some() {
                let type_node = self.ctx.arena.get(var_decl.type_annotation)?;
                return Some(type_node.kind == syntax_kind_ext::INTERSECTION_TYPE);
            }
        }

        Some(false)
    }

    pub(super) fn anchor_target_has_intersection_annotation(&self, anchor_idx: NodeIndex) -> bool {
        self.anchor_target_intersection_check_inner(anchor_idx)
            .unwrap_or(false)
    }

    pub(super) fn anchor_jsdoc_type_tag_targets_intersection_alias(
        &self,
        anchor_idx: NodeIndex,
    ) -> bool {
        self.anchor_jsdoc_type_tag_targets_intersection_alias_inner(anchor_idx)
            .unwrap_or(false)
    }

    fn anchor_jsdoc_type_tag_targets_intersection_alias_inner(
        &self,
        anchor_idx: NodeIndex,
    ) -> Option<bool> {
        let sf = self.source_file_data_for_node(anchor_idx)?;
        let source_text = sf.text.to_string();
        let comments = sf.comments.clone();
        let jsdoc = self.try_jsdoc_with_ancestor_walk(anchor_idx, &comments, &source_text)?;
        let type_expr = Self::extract_jsdoc_type_expression(&jsdoc)?;
        let base_name = if let Some(angle_idx) = Self::find_top_level_char(type_expr, '<') {
            type_expr[..angle_idx].trim()
        } else {
            type_expr.trim()
        };
        if base_name.is_empty() {
            return Some(false);
        }

        for comment in &comments {
            if !tsz_common::comments::is_jsdoc_comment(comment, &source_text) {
                continue;
            }
            let content = tsz_common::comments::get_jsdoc_content(comment, &source_text);
            for (name, typedef_info) in Self::parse_jsdoc_typedefs(&content) {
                if name != base_name {
                    continue;
                }
                let Some(base_type) = typedef_info.base_type.as_deref() else {
                    continue;
                };
                if Self::split_top_level_binary(base_type, '&').is_some() {
                    return Some(true);
                }
            }
        }

        Some(false)
    }

    /// Inner helper returning `Option` so we can use `?` for early returns.
    fn anchor_target_intersection_check_inner(&self, anchor_idx: NodeIndex) -> Option<bool> {
        use tsz_parser::parser::syntax_kind_ext;

        let anchor_node = self.ctx.arena.get(anchor_idx)?;

        // Walk from anchor to the assignment target identifier
        let target_ident_idx = if anchor_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
            let expr_stmt = self.ctx.arena.get_expression_statement(anchor_node)?;
            let expr_node = self.ctx.arena.get(expr_stmt.expression)?;
            if expr_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                let binary = self.ctx.arena.get_binary_expr(expr_node)?;
                binary.left
            } else {
                return Some(false);
            }
        } else {
            return Some(false);
        };

        // Check if the target is an identifier
        let ident_node = self.ctx.arena.get(target_ident_idx)?;
        if ident_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return Some(false);
        }

        // Resolve identifier to symbol
        let sym_id = self.resolve_identifier_symbol(target_ident_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;

        // Get value declaration
        let decl_node = self.ctx.arena.get(symbol.value_declaration)?;

        // Check if it's a variable declaration with an intersection type annotation
        if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
            if var_decl.type_annotation.is_some() {
                let type_node = self.ctx.arena.get(var_decl.type_annotation)?;
                return Some(type_node.kind == syntax_kind_ext::INTERSECTION_TYPE);
            }
        }

        Some(false)
    }

    pub(super) fn missing_required_properties_from_index_signature_source(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<Vec<tsz_common::interner::Atom>> {
        use crate::query_boundaries::common::{IndexKind, IndexSignatureResolver};

        if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, source) {
            return None;
        }

        let source_env_evaluated = self.evaluate_type_with_env(source);
        let source_evaluated = self.evaluate_type_for_assignability(source);
        let target_env_evaluated = self.evaluate_type_with_env(target);
        let target_evaluated = self.evaluate_type_for_assignability(target);

        let resolver = IndexSignatureResolver::new(self.ctx.types);
        let source_has_index = [source, source_env_evaluated, source_evaluated]
            .into_iter()
            .any(|candidate| {
                resolver.has_index_signature(candidate, IndexKind::String)
                    || resolver.has_index_signature(candidate, IndexKind::Number)
            });
        if !source_has_index {
            return None;
        }

        let target_with_shape = {
            let direct = target;
            let resolved = self.resolve_type_for_property_access(direct);
            let judged = self.judge_evaluate(resolved);
            [
                direct,
                resolved,
                judged,
                target_env_evaluated,
                target_evaluated,
            ]
            .into_iter()
            .find(|candidate| {
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, *candidate)
                    .is_some()
            })?
        };

        let source_shape = {
            let direct = source;
            let resolved = self.resolve_type_for_property_access(direct);
            let judged = self.judge_evaluate(resolved);
            [
                direct,
                resolved,
                judged,
                source_env_evaluated,
                source_evaluated,
            ]
            .into_iter()
            .find_map(|candidate| {
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, candidate)
            })
        };
        let target_shape = crate::query_boundaries::common::object_shape_for_type(
            self.ctx.types,
            target_with_shape,
        )?;

        // Check if target has index signature using the resolver (more reliable than shape check)
        let target_has_index = [target, target_env_evaluated, target_evaluated]
            .into_iter()
            .any(|candidate| {
                resolver.has_index_signature(candidate, IndexKind::String)
                    || resolver.has_index_signature(candidate, IndexKind::Number)
            });

        if target_has_index
            || target_shape.string_index.is_some()
            || target_shape.number_index.is_some()
        {
            return None;
        }

        let mut missing: Vec<_> = target_shape
            .properties
            .iter()
            .filter(|prop| !prop.optional)
            .filter(|prop| {
                !source_shape.as_ref().is_some_and(|shape| {
                    shape
                        .properties
                        .iter()
                        .any(|source_prop| source_prop.name == prop.name)
                })
            })
            .map(|prop| prop.name)
            .collect();
        missing.sort_by(|left, right| {
            self.ctx
                .types
                .resolve_atom_ref(*left)
                .cmp(&self.ctx.types.resolve_atom_ref(*right))
        });

        (!missing.is_empty()).then_some(missing)
    }

    pub(super) fn private_or_protected_brand_backing_member_display(
        &self,
        target_type: TypeId,
        required_property_name: Option<tsz_common::interner::Atom>,
    ) -> Option<(String, String, tsz_solver::Visibility)> {
        let find_member = |props: &[tsz_solver::PropertyInfo]| {
            props.iter().find_map(|prop| {
                let prop_name = self.ctx.types.resolve_atom(prop.name);
                if prop_name.starts_with("__private_brand_")
                    || required_property_name.is_some_and(|required| prop.name != required)
                    || prop.visibility == tsz_solver::Visibility::Public
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
            .and_then(|shape| find_member(&shape.properties))
            .or_else(|| {
                crate::query_boundaries::common::callable_shape_for_type(
                    self.ctx.types,
                    target_type,
                )
                .and_then(|shape| find_member(&shape.properties))
            })
    }

    pub(super) fn nominal_mismatch_detail(
        &self,
        source_type: TypeId,
        target_type: TypeId,
        property_name: tsz_common::interner::Atom,
    ) -> Option<String> {
        let source_prop = self.property_info_for_display(source_type, property_name)?;
        let target_prop = self.property_info_for_display(target_type, property_name)?;
        if source_prop.visibility != target_prop.visibility
            || target_prop.visibility == tsz_solver::Visibility::Public
        {
            return None;
        }
        let prop_name = self.ctx.types.resolve_atom_ref(property_name);
        match target_prop.visibility {
            tsz_solver::Visibility::Private => Some(format_message(
                diagnostic_messages::TYPES_HAVE_SEPARATE_DECLARATIONS_OF_A_PRIVATE_PROPERTY,
                &[&prop_name],
            )),
            tsz_solver::Visibility::Protected => Some(format!(
                "Types have separate declarations of a protected property '{prop_name}'."
            )),
            tsz_solver::Visibility::Public => None,
        }
    }

    pub(super) fn canonical_array_display_rank(name: &str) -> Option<usize> {
        match name {
            "length" => Some(0),
            "pop" => Some(1),
            "push" => Some(2),
            "concat" => Some(3),
            "join" => Some(4),
            "reverse" => Some(5),
            "shift" => Some(6),
            "slice" => Some(7),
            "sort" => Some(8),
            "splice" => Some(9),
            "unshift" => Some(10),
            "indexOf" => Some(11),
            "lastIndexOf" => Some(12),
            "every" => Some(13),
            "some" => Some(14),
            "forEach" => Some(15),
            "map" => Some(16),
            "filter" => Some(17),
            "reduce" => Some(18),
            "reduceRight" => Some(19),
            _ => None,
        }
    }

    pub(super) fn private_or_protected_assignability_message(
        &self,
        source_str: &str,
        target_str: &str,
        prop_name: &str,
        owner_name: &str,
        visibility: tsz_solver::Visibility,
        source_visibility: Option<tsz_solver::Visibility>,
    ) -> String {
        let source_side = source_visibility
            .filter(|_| !source_str.trim_start().starts_with('{'))
            .map(Self::visibility_name)
            .map(|visibility| format!("{visibility} in type '{source_str}'"))
            .unwrap_or_else(|| format!("not in type '{source_str}'"));
        let detail = match visibility {
            tsz_solver::Visibility::Private => {
                format!(
                    "Property '{prop_name}' is private in type '{owner_name}' but {source_side}."
                )
            }
            tsz_solver::Visibility::Protected => {
                format!(
                    "Property '{prop_name}' is protected in type '{owner_name}' but {source_side}."
                )
            }
            _ => format!(
                "Property '{prop_name}' is not accessible in type '{owner_name}' from type '{source_str}'."
            ),
        };
        format!("Type '{source_str}' is not assignable to type '{target_str}'.\n  {detail}")
    }

    pub(super) const fn visibility_name(visibility: tsz_solver::Visibility) -> &'static str {
        match visibility {
            tsz_solver::Visibility::Private => "private",
            tsz_solver::Visibility::Protected => "protected",
            tsz_solver::Visibility::Public => "public",
        }
    }

    pub(super) fn property_visibility_assignability_message(
        &self,
        source_str: &str,
        target_str: &str,
        prop_name: &str,
        source_visibility: tsz_solver::Visibility,
        target_visibility: tsz_solver::Visibility,
    ) -> String {
        let source_visibility = Self::visibility_name(source_visibility);
        let target_visibility = Self::visibility_name(target_visibility);
        format!(
            "Type '{source_str}' is not assignable to type '{target_str}'.\n  Property '{prop_name}' is {target_visibility} in type '{target_str}' but {source_visibility} in type '{source_str}'."
        )
    }

    pub(super) fn sort_missing_property_names_for_display(
        &mut self,
        target_type: TypeId,
        property_names: &[tsz_common::interner::Atom],
    ) -> Vec<tsz_common::interner::Atom> {
        // Track (declaration_order, shape_index, is_own_property) for each property.
        // `is_own_property` = true when the property's parent_id matches the target
        // type's symbol, meaning it was declared directly on the target type (not
        // inherited). tsc lists own properties before inherited ones in TS2739/TS2741.
        let target_symbol =
            crate::query_boundaries::common::get_object_symbol(self.ctx.types, target_type);
        let mut property_ranks: FxHashMap<tsz_common::interner::Atom, (u32, usize, bool)> =
            FxHashMap::default();

        let mut collect_ranks = |ty: TypeId, tgt_sym: Option<tsz_binder::SymbolId>| {
            if let Some(shape) =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty)
            {
                for (index, prop) in shape.properties.iter().enumerate() {
                    let is_own = tgt_sym.is_some() && prop.parent_id == tgt_sym;
                    property_ranks.entry(prop.name).or_insert((
                        prop.declaration_order,
                        index,
                        is_own,
                    ));
                }
            }
            if let Some(shape) =
                crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, ty)
            {
                for (index, prop) in shape.properties.iter().enumerate() {
                    let is_own = tgt_sym.is_some() && prop.parent_id == tgt_sym;
                    property_ranks.entry(prop.name).or_insert((
                        prop.declaration_order,
                        index,
                        is_own,
                    ));
                }
            }
        };

        collect_ranks(target_type, target_symbol);
        let resolved = self.resolve_type_for_property_access(target_type);
        if resolved != target_type {
            collect_ranks(resolved, target_symbol);
        }
        let evaluated = self.evaluate_type_for_assignability(target_type);
        if evaluated != target_type && evaluated != resolved {
            collect_ranks(evaluated, target_symbol);
        }

        let array_like_target = matches!(
            query_utils::classify_array_like(self.ctx.types, target_type),
            query_utils::ArrayLikeKind::Array(_)
                | query_utils::ArrayLikeKind::Tuple
                | query_utils::ArrayLikeKind::Readonly(_)
        ) || matches!(
            query_utils::classify_array_like(self.ctx.types, resolved),
            query_utils::ArrayLikeKind::Array(_)
                | query_utils::ArrayLikeKind::Tuple
                | query_utils::ArrayLikeKind::Readonly(_)
        ) || matches!(
            query_utils::classify_array_like(self.ctx.types, evaluated),
            query_utils::ArrayLikeKind::Array(_)
                | query_utils::ArrayLikeKind::Tuple
                | query_utils::ArrayLikeKind::Readonly(_)
        );

        let mut ordered: Vec<(usize, tsz_common::interner::Atom)> =
            property_names.iter().copied().enumerate().collect();
        let named_target = self.named_type_display_name(target_type).is_some();
        let date_target = self.named_type_display_name(target_type).as_deref() == Some("Date");
        ordered.sort_by(|(left_index, left_name), (right_index, right_name)| {
            if array_like_target {
                let left_text = self.ctx.types.resolve_atom_ref(*left_name);
                let right_text = self.ctx.types.resolve_atom_ref(*right_name);
                match (
                    Self::canonical_array_display_rank(&left_text),
                    Self::canonical_array_display_rank(&right_text),
                ) {
                    (Some(left_rank), Some(right_rank)) => {
                        let rank_ord = left_rank.cmp(&right_rank);
                        if rank_ord != std::cmp::Ordering::Equal {
                            return rank_ord;
                        }
                    }
                    (Some(_), None) => return std::cmp::Ordering::Less,
                    (None, Some(_)) => return std::cmp::Ordering::Greater,
                    (None, None) => {}
                }
            }

            if date_target {
                let date_rank = |name: &str| match name {
                    "toDateString" => Some(0_u8),
                    "toTimeString" => Some(1),
                    "toLocaleDateString" => Some(2),
                    "toLocaleTimeString" => Some(3),
                    _ => None,
                };
                let left_text = self.ctx.types.resolve_atom_ref(*left_name);
                let right_text = self.ctx.types.resolve_atom_ref(*right_name);
                match (date_rank(&left_text), date_rank(&right_text)) {
                    (Some(left_rank), Some(right_rank)) => {
                        let rank_ord = left_rank.cmp(&right_rank);
                        if rank_ord != std::cmp::Ordering::Equal {
                            return rank_ord;
                        }
                    }
                    (Some(_), None) => return std::cmp::Ordering::Less,
                    (None, Some(_)) => return std::cmp::Ordering::Greater,
                    (None, None) => {}
                }
            }

            if named_target {
                let left_text = self.ctx.types.resolve_atom_ref(*left_name);
                let right_text = self.ctx.types.resolve_atom_ref(*right_name);
                match (
                    is_object_prototype_method(&left_text),
                    is_object_prototype_method(&right_text),
                ) {
                    (false, true) => return std::cmp::Ordering::Less,
                    (true, false) => return std::cmp::Ordering::Greater,
                    _ => {}
                }
            }

            let left_rank = property_ranks.get(left_name).copied();
            let right_rank = property_ranks.get(right_name).copied();
            match (left_rank, right_rank) {
                (
                    Some((left_order, left_pos, left_own)),
                    Some((right_order, right_pos, right_own)),
                ) => {
                    // Own properties (declared directly on the target type) come
                    // before inherited ones, matching tsc behavior for TS2739/TS2741.
                    match (left_own, right_own) {
                        (true, false) => return std::cmp::Ordering::Less,
                        (false, true) => return std::cmp::Ordering::Greater,
                        _ => {}
                    }
                    match (
                        left_order > 0,
                        right_order > 0,
                        left_order.cmp(&right_order),
                        left_pos.cmp(&right_pos),
                    ) {
                        (true, true, std::cmp::Ordering::Equal, pos_ord)
                            if pos_ord != std::cmp::Ordering::Equal =>
                        {
                            pos_ord
                        }
                        (true, true, ord, _) if ord != std::cmp::Ordering::Equal => ord,
                        (true, false, _, _) => std::cmp::Ordering::Less,
                        (false, true, _, _) => std::cmp::Ordering::Greater,
                        _ => left_index.cmp(right_index),
                    }
                }
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => left_index.cmp(right_index),
            }
        });

        ordered.into_iter().map(|(_, name)| name).collect()
    }
}
