//! Alias display helpers for assignability diagnostics.

use crate::diagnostics::{diagnostic_messages, format_message};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn generic_alias_name_from_display(display: &str) -> Option<&str> {
        let display = display.trim_start();
        let (name, _) = display.split_once('<')?;
        let name = name.trim();
        (!name.is_empty()
            && name
                .chars()
                .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
        .then_some(name)
    }

    fn declared_generic_alias_annotation_matches_target_display(
        annotation: &str,
        target_display: &str,
    ) -> bool {
        let Some(annotation_name) = Self::generic_alias_name_from_display(annotation) else {
            return false;
        };
        let Some(target_name) = Self::generic_alias_name_from_display(target_display) else {
            return false;
        };
        annotation_name == target_name
    }

    fn bare_type_parameter_annotation_for_assignment_identifier(
        &mut self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }
        let annotation = self.declared_type_annotation_text_for_expression(expr_idx)?;
        let annotation = annotation.trim();
        if annotation.is_empty()
            || !annotation
                .chars()
                .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
        {
            return None;
        }
        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::VARIABLE) {
            return None;
        }
        let declared_type = self.get_type_of_symbol(sym_id);
        crate::query_boundaries::common::is_type_parameter(self.ctx.types, declared_type)
            .then(|| annotation.to_string())
    }

    pub(in crate::error_reporter) fn declared_generic_alias_source_display_for_target_display(
        &self,
        anchor_idx: NodeIndex,
        source_display: &str,
        target_display: &str,
    ) -> Option<String> {
        let expr_idx = self
            .direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx))?;
        let annotation_text = self.declared_type_annotation_text_for_expression(expr_idx)?;
        if annotation_text.contains('<')
            && let Some(annotation_name) = Self::generic_alias_name_from_display(&annotation_text)
            && Self::generic_alias_name_from_display(source_display) == Some(annotation_name)
            && Self::generic_alias_name_from_display(target_display) == Some(annotation_name)
        {
            if source_display.contains(" & ") || source_display.contains('{') {
                return Some(self.format_declared_annotation_for_diagnostic(&annotation_text));
            }
            return Some(source_display.to_string());
        }
        if !source_display.contains(" extends ")
            && !source_display.contains("infer ")
            && !source_display.contains("=>")
        {
            return None;
        }
        Self::declared_generic_alias_annotation_matches_target_display(
            &annotation_text,
            target_display,
        )
        .then(|| self.format_declared_annotation_for_diagnostic(&annotation_text))
    }

    pub(in crate::error_reporter) fn declared_generic_alias_assignment_pair_display(
        &mut self,
        anchor_idx: NodeIndex,
        source_display: &str,
        target_display: &str,
    ) -> Option<(String, String)> {
        if let Some(expr_idx) = self
            .direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx))
            && let Some(source_display) =
                self.bare_type_parameter_annotation_for_assignment_identifier(expr_idx)
        {
            return Some((source_display, target_display.to_string()));
        }
        if let Some(expr_idx) = self.assignment_target_expression(anchor_idx)
            && let Some(target_display) =
                self.bare_type_parameter_annotation_for_assignment_identifier(expr_idx)
        {
            return Some((source_display.to_string(), target_display));
        }
        if let Some(expr_idx) = self.assignment_target_expression(anchor_idx)
            && let Some(annotation_text) =
                self.declared_type_annotation_text_for_expression(expr_idx)
            && annotation_text.contains('<')
            && target_display.contains('<')
            && let Some(annotation_name) = Self::generic_alias_name_from_display(&annotation_text)
            && let Some(target_name) = Self::generic_alias_name_from_display(target_display)
            && annotation_name != target_name
        {
            let target_display = self.format_declared_annotation_for_diagnostic(&annotation_text);
            return Some((source_display.to_string(), target_display));
        }
        if let Some(source_display) = self.declared_generic_alias_source_display_for_target_display(
            anchor_idx,
            source_display,
            target_display,
        ) {
            return Some((source_display, target_display.to_string()));
        }

        let Some(expr_idx) = self
            .direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx))
        else {
            return None;
        };
        let Some(annotation_text) = self.declared_type_annotation_text_for_expression(expr_idx)
        else {
            return None;
        };
        if annotation_text == source_display
            || annotation_text.trim_start().starts_with("typeof ")
            || source_display.starts_with("import(")
            || (source_display.contains('{') && !annotation_text.contains('{'))
            || (!annotation_text.contains('<')
                && source_display.contains('<')
                && target_display.contains('<'))
            || annotation_text.contains(" | ")
            || annotation_text.contains(" & ")
            || annotation_text.contains('<')
            || annotation_text.contains('.')
            || (source_display.contains("| undefined") && !annotation_text.contains("| undefined"))
            || crate::error_reporter::assignability::display_is_literal_value(source_display)
        {
            return None;
        }
        let source_display = self.format_declared_annotation_for_diagnostic(&annotation_text);
        Some((source_display, target_display.to_string()))
    }

    pub(in crate::error_reporter) fn rewrite_declared_generic_alias_source_in_ts2322_message(
        &mut self,
        anchor_idx: NodeIndex,
        message: String,
    ) -> String {
        let Some(rest) = message.strip_prefix("Type '") else {
            return message;
        };
        let Some((source_display, target_part)) = rest.split_once("' is not assignable to type '")
        else {
            return message;
        };
        let Some(target_display) = target_part.strip_suffix("'.") else {
            return message;
        };
        if let Some((source_display, target_display)) = self
            .declared_generic_alias_assignment_pair_display(
                anchor_idx,
                source_display,
                target_display,
            )
        {
            return format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_display, &target_display],
            );
        }
        message
    }

    pub(in crate::error_reporter) fn direct_type_param_alias_application_pair_display(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> Option<(String, String)> {
        let (source_base, source_args) = self.application_info_or_display_alias(source)?;
        let (target_base, target_args) = self.application_info_or_display_alias(target)?;
        if source_base != target_base || source_args.len() != target_args.len() {
            return None;
        }
        let (source_arg, target_arg) = self.direct_type_param_alias_application_pair_args(
            source_base,
            &source_args,
            &target_args,
            0,
        )?;
        Some((
            self.format_type_diagnostic(source_arg),
            self.format_type_diagnostic(target_arg),
        ))
    }

    fn direct_type_param_alias_application_pair_args(
        &self,
        base: TypeId,
        source_args: &[TypeId],
        target_args: &[TypeId],
        depth: usize,
    ) -> Option<(TypeId, TypeId)> {
        if depth > 8 || source_args.len() != target_args.len() {
            return None;
        }

        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, base)?;
        let def = self.ctx.definition_store.get(def_id)?;
        if def.kind != tsz_solver::def::DefKind::TypeAlias {
            return None;
        }
        let body = def.body?;
        if let Some(param) = crate::query_boundaries::common::type_param_info(self.ctx.types, body)
        {
            let arg_idx = def
                .type_params
                .iter()
                .position(|type_param| type_param.name == param.name)?;
            return Some((*source_args.get(arg_idx)?, *target_args.get(arg_idx)?));
        }

        let (next_base, body_args) =
            crate::query_boundaries::common::application_info(self.ctx.types, body)?;
        if next_base == base {
            return None;
        }
        let source_args = self.instantiate_alias_application_display_args(
            &def.type_params,
            source_args,
            &body_args,
        )?;
        let target_args = self.instantiate_alias_application_display_args(
            &def.type_params,
            target_args,
            &body_args,
        )?;
        self.direct_type_param_alias_application_pair_args(
            next_base,
            &source_args,
            &target_args,
            depth + 1,
        )
    }

    fn instantiate_alias_application_display_args(
        &self,
        type_params: &[tsz_solver::TypeParamInfo],
        alias_args: &[TypeId],
        body_args: &[TypeId],
    ) -> Option<Vec<TypeId>> {
        if alias_args.len() < type_params.len() {
            return None;
        }
        let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
            self.ctx.types,
            type_params,
            &alias_args[..type_params.len()],
        );
        Some(
            body_args
                .iter()
                .map(|&arg| {
                    crate::query_boundaries::common::instantiate_type(
                        self.ctx.types,
                        arg,
                        &substitution,
                    )
                })
                .collect(),
        )
    }
}
