//! Variable declaration inferred type text helpers

#[allow(unused_imports)]
use super::super::{DeclarationEmitter, ImportPlan, PlannedImportModule, PlannedImportSymbol};
#[allow(unused_imports)]
use crate::emitter::type_printer::TypePrinter;
#[allow(unused_imports)]
use crate::output::source_writer::{SourcePosition, SourceWriter, source_position_from_offset};
#[allow(unused_imports)]
use rustc_hash::{FxHashMap, FxHashSet};
#[allow(unused_imports)]
use std::sync::Arc;
#[allow(unused_imports)]
use tracing::debug;
#[allow(unused_imports)]
use tsz_binder::{BinderState, SymbolId, symbol_flags};
#[allow(unused_imports)]
use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
#[allow(unused_imports)]
use tsz_parser::parser::ParserState;
#[allow(unused_imports)]
use tsz_parser::parser::node::{Node, NodeAccess, NodeArena};
#[allow(unused_imports)]
use tsz_parser::parser::syntax_kind_ext;
#[allow(unused_imports)]
use tsz_parser::parser::{NodeIndex, NodeList};
#[allow(unused_imports)]
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn widen_mutable_call_initializer_literal_type_text(
        &self,
        initializer: NodeIndex,
    ) -> Option<String> {
        let call = self
            .arena
            .get(initializer)
            .and_then(|node| self.arena.get_call_expr(node))?;
        if call.type_arguments.is_some()
            || self
                .arena
                .get(call.expression)
                .and_then(|node| self.arena.get_expr_type_args(node))
                .is_some_and(|expr| expr.type_arguments.is_some())
        {
            return None;
        }
        let interner = self.type_interner?;
        let type_id = self.get_node_type_or_names(&[initializer])?;
        let widened = tsz_solver::operations::widening::widen_literal_type(interner, type_id);
        (widened != type_id).then(|| self.print_type_id_for_inferred_declaration(widened))
    }

    pub(in crate::declaration_emitter) fn preserve_spread_argument_tuple_labels_in_call_return_type(
        &self,
        initializer: NodeIndex,
        type_text: &str,
    ) -> Option<String> {
        let call = self
            .arena
            .get(initializer)
            .and_then(|node| self.arena.get_call_expr(node))?;
        let args = call.arguments.as_ref()?;
        let [arg_idx] = args.nodes.as_slice() else {
            return None;
        };
        let arg_node = self.arena.get(*arg_idx)?;
        if arg_node.kind != syntax_kind_ext::SPREAD_ELEMENT {
            return None;
        }
        let spread = self.arena.get_spread(arg_node)?;
        let spread_type = self.get_node_type_or_names(&[spread.expression])?;
        let spread_text = self.print_type_id_for_inferred_declaration(spread_type);
        if !spread_text.contains(':') || !Self::is_tuple_type_text(&spread_text) {
            return None;
        }

        let unlabelled_spread = Self::strip_tuple_labels_from_type_text(&spread_text)?;
        let normalized_return = Self::normalize_tuple_type_text(type_text)?;
        (unlabelled_spread == normalized_return)
            .then(|| Self::compact_tuple_type_text(&spread_text))
            .flatten()
    }

    fn is_tuple_type_text(type_text: &str) -> bool {
        let trimmed = type_text.trim();
        trimmed.starts_with('[') && trimmed.ends_with(']')
    }

    fn normalize_tuple_type_text(type_text: &str) -> Option<String> {
        let trimmed = type_text.trim();
        let inner = trimmed.strip_prefix('[')?.strip_suffix(']')?.trim();
        if inner.is_empty() {
            return Some("[]".to_string());
        }
        let parts = Self::split_top_level_commas(inner)
            .into_iter()
            .map(str::trim)
            .collect::<Vec<_>>();
        Some(format!("[{}]", parts.join(", ")))
    }

    fn compact_tuple_type_text(type_text: &str) -> Option<String> {
        Self::normalize_tuple_type_text(type_text)
    }

    pub(in crate::declaration_emitter) fn expand_parameters_utility_tuple_type_text(
        type_text: &str,
    ) -> Option<String> {
        let trimmed = type_text.trim();
        let inner = trimmed.strip_prefix('[')?.strip_suffix(']')?.trim();
        if inner.is_empty() {
            return None;
        }

        let mut changed = false;
        let parts = Self::split_top_level_commas(inner)
            .into_iter()
            .map(|part| {
                let part = part.trim();
                if let Some(expanded) = Self::expand_parameters_utility_type_text(part) {
                    changed = true;
                    expanded
                } else {
                    part.to_string()
                }
            })
            .collect::<Vec<_>>();

        changed.then(|| format!("[{}]", parts.join(", ")))
    }

    fn expand_parameters_utility_type_text(type_text: &str) -> Option<String> {
        let inner = type_text
            .trim()
            .strip_prefix("Parameters<")?
            .strip_suffix('>')?
            .trim();
        let arrow_idx = Self::find_top_level_arrow(inner)?;
        let head = inner.get(..arrow_idx)?.trim_end();
        let open_idx = head.rfind('(')?;
        let params_text = head.get(open_idx + 1..)?.strip_suffix(')')?.trim();
        if params_text.is_empty() {
            return Some("[]".to_string());
        }
        let params = Self::split_top_level_commas(params_text)
            .into_iter()
            .map(str::trim)
            .collect::<Vec<_>>();
        Some(format!("[{}]", params.join(", ")))
    }

    fn strip_tuple_labels_from_type_text(type_text: &str) -> Option<String> {
        let trimmed = type_text.trim();
        let inner = trimmed.strip_prefix('[')?.strip_suffix(']')?.trim();
        if inner.is_empty() {
            return Some("[]".to_string());
        }

        let parts = Self::split_top_level_commas(inner)
            .into_iter()
            .map(|part| {
                let part = part.trim();
                let Some(colon_idx) = Self::find_top_level_byte(part, b':') else {
                    return part.to_string();
                };
                let before_colon = part.get(..colon_idx).unwrap_or_default().trim();
                let after_colon = part.get(colon_idx + 1..).unwrap_or_default().trim();
                if after_colon.is_empty() || !Self::looks_like_tuple_label(before_colon) {
                    return part.to_string();
                }
                let rest_prefix = if before_colon.strip_prefix("...").is_some() {
                    "..."
                } else {
                    ""
                };
                format!("{rest_prefix}{after_colon}")
            })
            .collect::<Vec<_>>();

        Some(format!("[{}]", parts.join(", ")))
    }

    fn looks_like_tuple_label(label: &str) -> bool {
        let label = label.trim();
        let label = label.strip_prefix("...").unwrap_or(label).trim();
        let label = label.strip_suffix('?').unwrap_or(label).trim();
        !label.is_empty()
            && label
                .chars()
                .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
    }

    pub(in crate::declaration_emitter) fn insert_import_for_reused_static_call_type(
        &mut self,
        initializer: NodeIndex,
        type_text: &str,
    ) {
        let Some(init_idx) = self.skip_parenthesized_expression(initializer) else {
            return;
        };
        let Some(init_node) = self.arena.get(init_idx) else {
            return;
        };
        if init_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return;
        }
        let Some(call) = self.arena.get_call_expr(init_node) else {
            return;
        };
        let Some(callee_node) = self.arena.get(call.expression) else {
            return;
        };
        let Some(access) = self.arena.get_access_expr(callee_node) else {
            return;
        };
        let Some(receiver_name) = self.get_identifier_text(access.expression) else {
            return;
        };
        if !type_text
            .trim_start()
            .starts_with(&format!("{receiver_name}<"))
        {
            return;
        }
        let Some((specifier, module)) = self
            .named_import_specifier_for_local(&receiver_name)
            .or_else(|| {
                self.imported_value_module_specifier_from_syntax(access.expression)
                    .map(|module| (receiver_name.clone(), module))
            })
            .or_else(|| {
                self.source_file_text.as_deref().and_then(|text| {
                    self.named_import_module_from_text(text, &receiver_name)
                        .map(|module| (receiver_name.clone(), module))
                })
            })
        else {
            return;
        };
        let import_line = format!("import {{ {specifier} }} from \"{module}\";");
        if self.writer.get_output().contains(&import_line) {
            return;
        }
        self.writer.insert_line_at(0, 0, &import_line);
        self.emitted_module_indicator = true;
    }

    pub(in crate::declaration_emitter) fn insert_import_for_unqualified_imported_type(
        &mut self,
        type_text: &str,
    ) {
        let Some(type_name) = Self::leading_type_reference_name(type_text) else {
            return;
        };
        let Some((specifier, module)) =
            self.named_import_specifier_for_local(type_name)
                .or_else(|| {
                    self.source_file_text.as_deref().and_then(|text| {
                        self.named_import_module_from_text(text, type_name)
                            .map(|module| (type_name.to_string(), module))
                    })
                })
        else {
            return;
        };
        let import_line = format!("import {{ {specifier} }} from \"{module}\";");
        if self.writer.get_output().contains(&import_line) {
            return;
        }
        self.writer.insert_line_at(0, 0, &import_line);
        self.emitted_module_indicator = true;
    }

    fn named_import_specifier_for_local(&self, local_name: &str) -> Option<(String, String)> {
        let source_file = self
            .current_source_file_idx
            .and_then(|source_file_idx| self.arena.get(source_file_idx))
            .and_then(|node| self.arena.get_source_file(node))
            .or_else(|| self.arena_source_file(self.arena))?;
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let Some(import) = self.arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_node) = self.arena.get(import.module_specifier) else {
                continue;
            };
            let Some(module_lit) = self.arena.get_literal(module_node) else {
                continue;
            };
            let Some(clause_node) = self.arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = self.arena.get_import_clause(clause_node) else {
                continue;
            };
            if clause.named_bindings.is_some()
                && let Some(bindings_node) = self.arena.get(clause.named_bindings)
                && let Some(bindings) = self.arena.get_named_imports(bindings_node)
            {
                for &spec_idx in &bindings.elements.nodes {
                    let Some(spec_node) = self.arena.get(spec_idx) else {
                        continue;
                    };
                    let Some(specifier) = self.arena.get_specifier(spec_node) else {
                        continue;
                    };
                    if self.get_identifier_text(specifier.name).as_deref() == Some(local_name) {
                        let imported_name = specifier
                            .property_name
                            .is_some()
                            .then(|| self.get_identifier_text(specifier.property_name))
                            .flatten();
                        let specifier_text = imported_name.map_or_else(
                            || local_name.to_string(),
                            |imported_name| format!("{imported_name} as {local_name}"),
                        );
                        return Some((specifier_text, module_lit.text.clone()));
                    }
                }
            }
        }
        None
    }

    pub(in crate::declaration_emitter) fn widen_literal_initializer_result_type_text(
        &self,
        initializer: NodeIndex,
    ) -> Option<String> {
        let interner = self.type_interner?;
        let type_id = self.get_node_type_or_names(&[initializer])?;
        if let Some(lit) = tsz_solver::visitor::literal_value(interner, type_id) {
            return Self::literal_primitive_kind_text(&lit).map(str::to_string);
        }
        let union_id = tsz_solver::visitor::union_list_id(interner, type_id)?;
        let members = interner.type_list(union_id);
        let mut kind: Option<&'static str> = None;
        for &member in members.iter() {
            let member_lit = tsz_solver::visitor::literal_value(interner, member)?;
            let member_kind = Self::literal_primitive_kind_text(&member_lit)?;
            if let Some(existing) = kind {
                if existing != member_kind {
                    return None;
                }
            } else {
                kind = Some(member_kind);
            }
        }
        kind.map(str::to_string)
    }

    pub(in crate::declaration_emitter) const fn literal_primitive_kind_text(
        lit: &tsz_solver::types::LiteralValue,
    ) -> Option<&'static str> {
        match lit {
            tsz_solver::types::LiteralValue::String(_) => Some("string"),
            tsz_solver::types::LiteralValue::Number(_) => Some("number"),
            tsz_solver::types::LiteralValue::Boolean(_) => Some("boolean"),
            tsz_solver::types::LiteralValue::BigInt(_) => Some("bigint"),
        }
    }

    pub(in crate::declaration_emitter) fn is_literal_type_text_for_const_call(
        type_text: &str,
    ) -> bool {
        let trimmed = type_text.trim();
        matches!(trimmed, "true" | "false" | "null" | "undefined")
            || trimmed.starts_with('"')
            || trimmed.starts_with('\'')
            || tsz_solver::utils::is_numeric_literal_name(trimmed.trim_end_matches('n'))
    }

    pub(in crate::declaration_emitter) fn call_expression_single_literal_type_argument_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(expr_node)?;
        let type_arguments = call.type_arguments.as_ref().or_else(|| {
            self.arena
                .get(call.expression)
                .and_then(|node| self.arena.get_expr_type_args(node))
                .and_then(|expr_type_args| expr_type_args.type_arguments.as_ref())
        })?;
        let &[type_arg] = type_arguments.nodes.as_slice() else {
            return None;
        };
        let type_text = self
            .emit_type_node_text(type_arg)
            .or_else(|| self.source_slice_from_arena(self.arena, type_arg))?;
        let type_text = type_text.trim().to_string();
        Self::is_literal_type_text_for_const_call(&type_text).then_some(type_text)
    }

    pub(in crate::declaration_emitter) fn call_contains_unannotated_function_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        match expr_node.kind {
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
            {
                self.arena
                    .get_function(expr_node)
                    .is_some_and(|func| func.type_annotation.is_none())
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                let Some(call) = self.arena.get_call_expr(expr_node) else {
                    return false;
                };
                self.call_contains_unannotated_function_expression(call.expression)
                    || call.arguments.as_ref().is_some_and(|args| {
                        args.nodes
                            .iter()
                            .any(|&arg| self.call_contains_unannotated_function_expression(arg))
                    })
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                let Some(object) = self.arena.get_literal_expr(expr_node) else {
                    return false;
                };
                object.elements.nodes.iter().any(|&member_idx| {
                    let Some(member_node) = self.arena.get(member_idx) else {
                        return false;
                    };
                    if let Some(prop) = self.arena.get_property_assignment(member_node) {
                        self.call_contains_unannotated_function_expression(prop.initializer)
                    } else if let Some(method) = self.arena.get_method_decl(member_node) {
                        method.type_annotation.is_none()
                    } else {
                        false
                    }
                })
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => self
                .arena
                .get_parenthesized(expr_node)
                .is_some_and(|paren| {
                    self.call_contains_unannotated_function_expression(paren.expression)
                }),
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                self.arena
                    .get_type_assertion(expr_node)
                    .is_some_and(|assertion| {
                        self.call_contains_unannotated_function_expression(assertion.expression)
                    })
            }
            _ => false,
        }
    }

    pub(in crate::declaration_emitter) fn variable_declaration_has_effective_export(
        &self,
        decl_idx: NodeIndex,
    ) -> bool {
        let mut current = decl_idx;
        for _ in 0..4 {
            if self.statement_has_effective_export(current) {
                return true;
            }
            let Some(parent) = self.arena.get_extended(current).map(|ext| ext.parent) else {
                return false;
            };
            current = parent;
        }
        false
    }

    pub(in crate::declaration_emitter) fn normalize_inferred_array_any_text(
        type_text: &str,
    ) -> String {
        if type_text.trim() == "Array<any>" {
            "any[]".to_string()
        } else {
            type_text.to_string()
        }
    }
}
