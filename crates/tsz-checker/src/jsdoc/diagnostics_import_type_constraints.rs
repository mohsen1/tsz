//! JSDoc import type constraint diagnostics.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::jsdoc::types::JsdocTypedefInfo;
use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    pub(super) fn report_jsdoc_import_type_constraint_error(
        &mut self,
        expr: &str,
        angle_idx: usize,
        arg_strs: &[&str],
        module_specifier: &str,
        member_name: &str,
        typedef_info: &JsdocTypedefInfo,
        comment_pos: u32,
        comment_end: u32,
        source_text: &str,
    ) {
        let factory = self.ctx.types.factory();
        let mut scope_updates = Vec::new();
        for tp in &typedef_info.template_params {
            let constraint = tp
                .constraint
                .as_deref()
                .and_then(|c| self.resolve_jsdoc_type_str(c));
            let atom = self.ctx.types.intern_string(&tp.name);
            let param = tsz_solver::TypeParamInfo {
                name: atom,
                constraint,
                default: None,
                is_const: false,
            };
            let type_id = factory.type_param(param);
            let previous = self
                .ctx
                .type_parameter_scope
                .insert(tp.name.clone(), type_id);
            scope_updates.push((tp.name.clone(), previous));
        }

        let mut type_params = self
            .resolve_jsdoc_import_member(module_specifier, member_name)
            .map(|sym_id| self.type_reference_symbol_type_with_params(sym_id).1)
            .unwrap_or_default();
        if type_params.is_empty() {
            type_params = self.resolve_import_typedef_type_params(module_specifier, member_name);
        }

        if !type_params.is_empty() {
            let comment_text =
                &source_text[comment_pos as usize..(comment_end as usize).min(source_text.len())];
            let type_expr_offset = comment_text.find(expr).unwrap_or(0);
            let mut arg_search_offset = angle_idx + 1;
            for (arg_str, param) in arg_strs.iter().zip(type_params.iter()) {
                let Some(constraint) = param.constraint else {
                    arg_search_offset += arg_str.len() + 1;
                    continue;
                };
                let Some(type_arg) = self.resolve_jsdoc_type_str(arg_str.trim()) else {
                    arg_search_offset += arg_str.len() + 1;
                    continue;
                };
                if type_arg == tsz_solver::TypeId::ERROR {
                    arg_search_offset += arg_str.len() + 1;
                    continue;
                }
                if self.diagnostic_relation_boolean_guard(type_arg, constraint) {
                    arg_search_offset += arg_str.len() + 1;
                    continue;
                }

                let widened_arg =
                    crate::query_boundaries::common::widen_literal_type(self.ctx.types, type_arg);
                let message = format_message(
                    diagnostic_messages::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
                    &[
                        &self.format_type_diagnostic(widened_arg),
                        &self.format_type_diagnostic(constraint),
                    ],
                );
                let arg_rel = expr[arg_search_offset..].find(arg_str.trim()).unwrap_or(0);
                let arg_pos = comment_pos as usize + type_expr_offset + arg_search_offset + arg_rel;
                self.ctx.error(
                    arg_pos as u32,
                    arg_str.trim().len() as u32,
                    message,
                    diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
                );
                break;
            }
        }

        for (name, previous) in scope_updates.into_iter().rev() {
            if let Some(previous) = previous {
                self.ctx.type_parameter_scope.insert(name, previous);
            } else {
                self.ctx.type_parameter_scope.remove(&name);
            }
        }
    }
}
