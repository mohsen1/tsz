//! Helper methods for assignability error reporting.
//! Extracted from `assignability.rs` for maintainability.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::error_reporter::fingerprint_policy::{
    DiagnosticAnchorKind, DiagnosticRenderRequest, RelatedInformationPolicy,
};
use crate::state::{CheckerState, MemberAccessLevel};
use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

use crate::query_boundaries::type_checking_utilities as query_utils;

impl<'a> CheckerState<'a> {
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
        self.diagnose_assignment_failure_with_anchor(source, target, anchor_idx);
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
    pub(super) fn anchor_target_has_intersection_annotation(&self, anchor_idx: NodeIndex) -> bool {
        self.anchor_target_intersection_check_inner(anchor_idx)
            .unwrap_or(false)
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
        use tsz_solver::objects::index_signatures::{IndexKind, IndexSignatureResolver};

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
                tsz_solver::type_queries::get_object_shape(self.ctx.types, *candidate).is_some()
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
                tsz_solver::type_queries::get_object_shape(self.ctx.types, candidate)
            })
        };
        let target_shape =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, target_with_shape)?;

        if target_shape.string_index.is_some() || target_shape.number_index.is_some() {
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

        tsz_solver::type_queries::get_object_shape(self.ctx.types, target_type)
            .and_then(|shape| find_member(&shape.properties))
            .or_else(|| {
                tsz_solver::type_queries::get_callable_shape(self.ctx.types, target_type)
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
        let mut property_ranks: FxHashMap<tsz_common::interner::Atom, (u32, usize)> =
            FxHashMap::default();

        let mut collect_ranks = |ty: TypeId| {
            if let Some(shape) = tsz_solver::type_queries::get_object_shape(self.ctx.types, ty) {
                for (index, prop) in shape.properties.iter().enumerate() {
                    property_ranks
                        .entry(prop.name)
                        .or_insert((prop.declaration_order, index));
                }
            }
            if let Some(shape) = tsz_solver::type_queries::get_callable_shape(self.ctx.types, ty) {
                for (index, prop) in shape.properties.iter().enumerate() {
                    property_ranks
                        .entry(prop.name)
                        .or_insert((prop.declaration_order, index));
                }
            }
        };

        collect_ranks(target_type);
        let resolved = self.resolve_type_for_property_access(target_type);
        if resolved != target_type {
            collect_ranks(resolved);
        }
        let evaluated = self.evaluate_type_for_assignability(target_type);
        if evaluated != target_type && evaluated != resolved {
            collect_ranks(evaluated);
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

            let left_rank = property_ranks.get(left_name).copied();
            let right_rank = property_ranks.get(right_name).copied();
            match (left_rank, right_rank) {
                (Some((left_order, left_pos)), Some((right_order, right_pos))) => {
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
