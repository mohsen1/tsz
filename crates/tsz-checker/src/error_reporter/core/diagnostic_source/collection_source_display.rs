//! Collection-shaped source display helpers for assignment diagnostics.

use crate::query_boundaries::diagnostics as diagnostic_query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn declared_numeric_literal_union_alias_source_display(
        &mut self,
        expr_idx: NodeIndex,
        declared_type: TypeId,
    ) -> Option<String> {
        let evaluated = self.evaluate_type_for_assignability(declared_type);
        if !diagnostic_query::is_number_literal_union(self.ctx.types, evaluated) {
            return None;
        }
        let annotation_text = self.declared_type_annotation_text_for_expression(expr_idx)?;
        Self::annotation_text_is_plain_type_reference(&annotation_text)
            .then(|| self.format_declared_annotation_for_diagnostic(&annotation_text))
    }

    /// Returns `true` when `narrowed`'s union members are a strict subset of
    /// `declared`'s union members. Single non-union types are treated as a
    /// one-element membership set against the declared union.
    ///
    /// This is the "narrowing eliminated some union members" check used by
    /// `declared_identifier_source_display` to recognise that flow narrowing
    /// produced a strictly smaller type even when the surviving member is
    /// structurally compatible with the eliminated ones (so plain
    /// `is_assignable_to(declared, narrowed)` returns true).
    pub(in crate::error_reporter) fn is_strict_union_member_subset(
        &mut self,
        narrowed: TypeId,
        declared: TypeId,
    ) -> bool {
        let Some(declared_members) =
            crate::query_boundaries::common::union_members(self.ctx.types, declared)
        else {
            return false;
        };
        if declared_members.len() < 2 {
            return false;
        }
        let narrowed_members =
            crate::query_boundaries::common::union_members(self.ctx.types, narrowed)
                .unwrap_or_else(|| vec![narrowed].into());
        if narrowed_members.is_empty() || narrowed_members.len() >= declared_members.len() {
            return false;
        }
        narrowed_members
            .iter()
            .all(|m| declared_members.contains(m))
    }

    pub(in crate::error_reporter) fn narrowed_string_literal_residual_union_display(
        &mut self,
        declared_type: TypeId,
        expr_display_type: TypeId,
        target: TypeId,
    ) -> Option<String> {
        if target != TypeId::NEVER || declared_type == expr_display_type {
            return None;
        }
        let source_members =
            crate::query_boundaries::common::union_members(self.ctx.types, expr_display_type)?;
        let declared_members =
            crate::query_boundaries::common::union_members(self.ctx.types, declared_type)?;
        if source_members.len() < 2 || source_members.len() >= declared_members.len() {
            return None;
        }
        if !source_members.iter().all(|&member| {
            crate::query_boundaries::common::string_literal_value(self.ctx.types, member).is_some()
        }) || !declared_members.iter().all(|&member| {
            crate::query_boundaries::common::string_literal_value(self.ctx.types, member).is_some()
        }) {
            return None;
        }
        if !source_members
            .iter()
            .all(|member| declared_members.contains(member))
        {
            return None;
        }

        let mut ordered = source_members.to_vec();
        ordered.sort_by_key(|member| {
            std::cmp::Reverse(
                declared_members
                    .iter()
                    .position(|declared| declared == member)
                    .unwrap_or(usize::MAX),
            )
        });
        Some(
            ordered
                .into_iter()
                .map(|member| self.format_assignability_type_for_message(member, target))
                .collect::<Vec<_>>()
                .join(" | "),
        )
    }

    pub(in crate::error_reporter) fn rebuilt_array_source_display(
        &mut self,
        source_type: TypeId,
        target: TypeId,
    ) -> Option<String> {
        if let Some(display) = self.static_schema_array_structural_display(source_type, target) {
            return Some(display);
        }
        let element_type =
            crate::query_boundaries::common::array_element_type(self.ctx.types, source_type)?;
        if matches!(element_type, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }
        // `tsc` renders a non-fresh array source's element types verbatim: an
        // annotated `Array<1>` / `1[]` / `(1 | 2)[]` source (or `ReadonlyArray<1>`)
        // keeps `1` / `1 | 2` in its assignability message, because
        // `getWidenedType` widens only types carrying the fresh-literal flag.
        // A *fresh* array literal source is already widened to its primitive
        // element type at expression typing (`const y: string = [1, 2]` types
        // `[1, 2]` as `number[]`), so it reaches this display already widened and
        // needs no further widening here. Widening the element unconditionally
        // therefore only mangled the non-fresh case, rendering `number[]` where
        // `tsc` shows `1[]`. Keep the element as written and let the normalizer
        // handle display canonicalization; non-fresh nested object/tuple members
        // are likewise preserved by `tsc`.
        let display_element = self.normalize_assignability_display_type(element_type);
        let rebuilt = diagnostic_query::rebuilt_array_source_display_type(
            self.ctx.types,
            source_type,
            display_element,
        );
        Some(self.format_assignability_type_for_message(rebuilt, target))
    }

    pub(in crate::error_reporter) fn call_object_literal_intersection_source_display(
        &mut self,
        expr_idx: NodeIndex,
        source_type: TypeId,
        target: TypeId,
    ) -> Option<String> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.ctx.arena.get_call_expr(node)?;
        let first_arg = *call.arguments.as_ref()?.nodes.first()?;
        let object_display = self.object_literal_source_type_display(first_arg, Some(target))?;

        let members =
            crate::query_boundaries::common::intersection_members(self.ctx.types, source_type)?;
        let mut displays = Vec::with_capacity(members.len());
        let mut replaced_object_member = false;

        for &member in members.iter() {
            let evaluated = self.evaluate_type_for_assignability(member);
            let is_object_like_member =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, evaluated)
                    .is_some()
                    || crate::query_boundaries::common::get_merged_object_shape_for_type(
                        self.ctx.types,
                        evaluated,
                    )
                    .is_some();
            if !replaced_object_member && is_object_like_member {
                displays.push(object_display.clone());
                replaced_object_member = true;
            } else {
                displays.push(self.format_assignability_type_for_message(member, target));
            }
        }

        replaced_object_member.then(|| displays.join(" & "))
    }
}
