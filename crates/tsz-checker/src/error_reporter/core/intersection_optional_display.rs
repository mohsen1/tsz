use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn format_collapsed_object_for_assignability_display(
        &mut self,
        type_id: TypeId,
    ) -> String {
        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_display_properties()
            .with_skip_object_display_alias()
            .with_preserve_optional_parameter_surface_syntax(true);
        formatter.format(type_id).into_owned()
    }

    pub(in crate::error_reporter) fn collapsed_anonymous_object_intersection_for_assignability_display(
        &mut self,
        ty: TypeId,
    ) -> Option<TypeId> {
        if !self.is_collapsible_anonymous_object_intersection_for_assignability_display(ty) {
            return None;
        }

        crate::query_boundaries::intersection_display::collected_properties_object_type(
            self.ctx.types,
            &self.ctx,
            ty,
        )
    }

    fn is_collapsible_anonymous_object_intersection_for_assignability_display(
        &self,
        ty: TypeId,
    ) -> bool {
        if self
            .anonymous_object_intersection_for_assignability_display(ty)
            .is_none()
            || self.named_type_display_name(ty).is_some()
        {
            return false;
        }

        crate::query_boundaries::intersection_display::collected_properties_object_type(
            self.ctx.types,
            &self.ctx,
            ty,
        )
        .is_some()
    }

    fn anonymous_object_intersection_for_assignability_display(
        &self,
        ty: TypeId,
    ) -> Option<TypeId> {
        let db = self.ctx.types.as_type_database();
        let intersection_ty = if crate::query_boundaries::common::is_intersection_type(db, ty) {
            ty
        } else {
            let alias = self.ctx.types.get_display_alias(ty)?;
            if !crate::query_boundaries::common::is_intersection_type(db, alias) {
                return None;
            }
            alias
        };
        let members =
            crate::query_boundaries::common::intersection_members(self.ctx.types, intersection_ty)?;
        if members.is_empty() {
            return None;
        }
        for &member in members.iter() {
            if self.named_type_display_name(member).is_some()
                || !crate::query_boundaries::common::is_object_like_type(db, member)
            {
                return None;
            }
        }
        Some(intersection_ty)
    }

    pub(in crate::error_reporter) fn line_rhs_declared_intersection_annotation(
        &self,
        anchor_idx: NodeIndex,
    ) -> Option<String> {
        let node = self.ctx.arena.get(anchor_idx)?;
        let source = self.ctx.arena.source_files.first()?.text.as_ref();
        let pos = node.pos as usize;
        if pos >= source.len() {
            return None;
        }
        let line_start = source[..pos].rfind('\n').map(|idx| idx + 1).unwrap_or(0);
        let line_end = source[pos..]
            .find('\n')
            .map(|idx| pos + idx)
            .unwrap_or(source.len());
        let line = &source[line_start..line_end];
        let rhs = line.split_once('=')?.1;
        let rhs_name = rhs
            .split([';', '/', ','])
            .next()
            .map(str::trim)?
            .trim_matches(|ch: char| ch == '(' || ch == ')');
        if rhs_name.is_empty()
            || !rhs_name
                .chars()
                .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
        {
            return None;
        }

        for decl_line in source.lines() {
            let trimmed = decl_line.trim_start();
            let Some(rest) = trimmed
                .strip_prefix("declare let ")
                .or_else(|| trimmed.strip_prefix("let "))
                .or_else(|| trimmed.strip_prefix("const "))
                .or_else(|| trimmed.strip_prefix("var "))
            else {
                continue;
            };
            let Some((name, annotation_and_more)) = rest.split_once(':') else {
                continue;
            };
            if name.trim() != rhs_name {
                continue;
            }
            let Some(annotation) = annotation_and_more.split(';').next().map(str::trim) else {
                continue;
            };
            if annotation.contains('&') && !annotation.starts_with("keyof ") {
                return Some(annotation.to_string());
            }
        }

        None
    }
}
