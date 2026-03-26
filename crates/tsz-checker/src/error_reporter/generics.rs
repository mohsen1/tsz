//! Generic type and comparison error reporting (TS2314, TS2344, TS2367, TS2352).

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::error_reporter::fingerprint_policy::{DiagnosticAnchorKind, DiagnosticRenderRequest};
use crate::query_boundaries::assignability::{
    get_function_return_type, replace_function_return_type,
};
use crate::query_boundaries::common;
use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};
use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;
use tsz_solver::{CallSignature, CallableShape};

impl<'a> CheckerState<'a> {
    fn widen_function_like_assertion_source(&self, type_id: TypeId) -> TypeId {
        if let Some(return_type) = get_function_return_type(self.ctx.types, type_id) {
            let widened_return = tsz_solver::widen_literal_type(self.ctx.types, return_type);
            if widened_return != return_type {
                let replaced =
                    replace_function_return_type(self.ctx.types, type_id, widened_return);
                if replaced != type_id {
                    return replaced;
                }
            }
        }

        if let Some(shape_id) = tsz_solver::callable_shape_id(self.ctx.types, type_id) {
            let shape = self.ctx.types.callable_shape(shape_id);
            let mut changed = false;

            let call_signatures = shape
                .call_signatures
                .iter()
                .map(|sig| {
                    let widened_return =
                        tsz_solver::widen_literal_type(self.ctx.types, sig.return_type);
                    if widened_return != sig.return_type {
                        changed = true;
                        let mut next = sig.clone();
                        next.return_type = widened_return;
                        next
                    } else {
                        sig.clone()
                    }
                })
                .collect();

            let construct_signatures = shape
                .construct_signatures
                .iter()
                .map(|sig| {
                    let widened_return =
                        tsz_solver::widen_literal_type(self.ctx.types, sig.return_type);
                    if widened_return != sig.return_type {
                        changed = true;
                        let mut next = sig.clone();
                        next.return_type = widened_return;
                        next
                    } else {
                        sig.clone()
                    }
                })
                .collect();

            if changed {
                return self.ctx.types.callable(tsz_solver::CallableShape {
                    call_signatures,
                    construct_signatures,
                    properties: shape.properties.clone(),
                    string_index: shape.string_index.clone(),
                    number_index: shape.number_index.clone(),
                    symbol: shape.symbol,
                    is_abstract: shape.is_abstract,
                });
            }
        }

        type_id
    }

    fn instantiate_call_signature_for_display(
        &self,
        sig: &CallSignature,
        type_args: &[TypeId],
    ) -> Option<CallSignature> {
        if sig.type_params.len() != type_args.len() {
            return None;
        }
        let subst = TypeSubstitution::from_args(self.ctx.types, &sig.type_params, type_args);
        Some(CallSignature {
            type_params: Vec::new(),
            params: sig
                .params
                .iter()
                .map(|param| tsz_solver::ParamInfo {
                    name: param.name,
                    type_id: instantiate_type(self.ctx.types, param.type_id, &subst),
                    optional: param.optional,
                    rest: param.rest,
                })
                .collect(),
            this_type: sig
                .this_type
                .map(|this_type| instantiate_type(self.ctx.types, this_type, &subst)),
            return_type: instantiate_type(self.ctx.types, sig.return_type, &subst),
            type_predicate: sig.type_predicate.clone(),
            is_method: sig.is_method,
        })
    }

    fn symbol_type_parameter_count(&self, sym_id: SymbolId) -> usize {
        let def_id = self.ctx.get_or_create_def_id(sym_id);
        if let Some(type_params) = self.ctx.get_def_type_params(def_id) {
            return type_params.len();
        }

        self.ctx
            .binder
            .get_symbol(sym_id)
            .and_then(|symbol| {
                symbol.declarations.iter().find_map(|decl| {
                    let node = self.ctx.arena.get(*decl)?;
                    let class = self.ctx.arena.get_class(node)?;
                    Some(class.type_parameters.as_ref().map_or(0, |p| p.nodes.len()))
                })
            })
            .unwrap_or(0)
    }

    fn try_format_type_query_instantiation_overlap_display(
        &mut self,
        type_id: TypeId,
    ) -> Option<String> {
        let app = tsz_solver::type_queries::get_type_application(self.ctx.types, type_id)?;
        let sym = tsz_solver::type_query_symbol(self.ctx.types, app.base)?;
        let symbol_type = self.get_type_of_symbol(SymbolId(sym.0));
        let shape = tsz_solver::type_queries::get_callable_shape(self.ctx.types, symbol_type)?;
        let call_sig = shape
            .call_signatures
            .iter()
            .find_map(|sig| self.instantiate_call_signature_for_display(sig, &app.args))?;
        let prototype_prop = shape
            .properties
            .iter()
            .find(|prop| self.ctx.types.resolve_atom_ref(prop.name).as_ref() == "prototype")?;
        let prototype_shape =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, prototype_prop.type_id)?;
        let prototype_sym_id = prototype_shape.symbol?;
        let prototype_symbol = self.ctx.binder.get_symbol(prototype_sym_id)?;
        let type_param_count = self.symbol_type_parameter_count(prototype_sym_id);
        if type_param_count != 1 {
            return None;
        }

        let prototype_symbol_name = prototype_symbol.escaped_name.as_str();
        let prototype_display = format!("{prototype_symbol_name}<any>");
        let call_return_type = call_sig.return_type;
        let call_display =
            self.format_type_for_assignability_message(self.ctx.types.callable(CallableShape {
                call_signatures: vec![call_sig],
                construct_signatures: Vec::new(),
                properties: Vec::new(),
                string_index: None,
                number_index: None,
                symbol: None,
                is_abstract: false,
            }));
        let construct_display = format!(
            "{prototype_symbol_name}<{}>",
            self.format_type_for_assignability_message(call_return_type)
        );
        Some(format!(
            "{{ new (): {construct_display}; prototype: {prototype_display}; }} & ({call_display})"
        ))
    }

    fn try_format_constructor_call_intersection_display(
        &mut self,
        type_id: TypeId,
    ) -> Option<String> {
        if let Some(display) = self.try_format_type_query_instantiation_overlap_display(type_id) {
            return Some(display);
        }
        let shape_id = tsz_solver::callable_shape_id(self.ctx.types, type_id)?;
        let shape = self.ctx.types.callable_shape(shape_id);
        if shape.call_signatures.len() != 1
            || !shape.construct_signatures.is_empty()
            || shape.string_index.is_some()
            || shape.number_index.is_some()
        {
            return None;
        }

        let prototype_prop = shape
            .properties
            .iter()
            .find(|prop| self.ctx.types.resolve_atom_ref(prop.name).as_ref() == "prototype")?;
        let prototype_shape =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, prototype_prop.type_id)?;
        let sym_id = prototype_shape.symbol?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let type_param_count = self.symbol_type_parameter_count(sym_id);
        if type_param_count != 1 {
            return None;
        }

        let symbol_name = symbol.escaped_name.as_str();
        let call_sig = &shape.call_signatures[0];
        let call_display = self.format_type_for_assignability_message(self.ctx.types.callable(
            tsz_solver::CallableShape {
                call_signatures: vec![call_sig.clone()],
                construct_signatures: Vec::new(),
                properties: Vec::new(),
                string_index: None,
                number_index: None,
                symbol: None,
                is_abstract: false,
            },
        ));
        let call_return_display = self.format_type_for_assignability_message(call_sig.return_type);
        let prototype_display = format!("{symbol_name}<any>");
        let construct_display = format!("{symbol_name}<{call_return_display}>");
        Some(format!(
            "{{ new (): {construct_display}; prototype: {prototype_display}; }} & ({call_display})"
        ))
    }

    fn try_format_type_assertion_overlap_special_display(
        &mut self,
        type_id: TypeId,
        widen_source: bool,
    ) -> Option<String> {
        let type_id = if widen_source {
            self.widen_function_like_assertion_source(type_id)
        } else {
            type_id
        };
        let evaluated = self.evaluate_type_with_env(type_id);
        self.try_format_constructor_call_intersection_display(evaluated)
    }

    fn format_type_assertion_overlap_display(
        &mut self,
        type_id: TypeId,
        widen_source: bool,
    ) -> String {
        if let Some(display) =
            self.try_format_type_assertion_overlap_special_display(type_id, widen_source)
        {
            return display;
        }
        let type_id = if widen_source {
            self.widen_function_like_assertion_source(type_id)
        } else {
            type_id
        };
        let evaluated = self.evaluate_type_with_env(type_id);
        if tsz_solver::type_queries::get_type_application(self.ctx.types, type_id).is_some() {
            return self.format_type_for_assignability_message(type_id);
        }
        self.format_type_for_assignability_message(evaluated)
    }

    fn assertion_declared_type_texts(&self, idx: NodeIndex) -> Option<(String, String)> {
        fn sanitize_type_text(text: String) -> Option<String> {
            let mut text = text.trim().trim_start_matches(':').trim().to_string();
            while matches!(text.chars().last(), Some(',') | Some(';')) {
                text.pop();
                text = text.trim_end().to_string();
            }
            // Strip trailing `>` leaked from angle-bracket assertion syntax `<T>expr`.
            // Only strip when angle brackets are unbalanced (more `>` than `<`).
            if text.ends_with('>') {
                let open = text.chars().filter(|&c| c == '<').count();
                let close = text.chars().filter(|&c| c == '>').count();
                if close > open {
                    text.pop();
                    text = text.trim_end().to_string();
                }
            }
            (!text.is_empty()).then_some(text)
        }

        let node = self.ctx.arena.get(idx)?;
        let assertion = self.ctx.arena.get_type_assertion(node)?;
        let source = self.declared_type_annotation_text_for_expression(assertion.expression)?;
        let mut target = self
            .node_text(assertion.type_node)
            .and_then(sanitize_type_text)?;
        // For angle-bracket assertions `<T>expr`, the parser's type_node span
        // may include the closing `>`. Strip it if the node is TYPE_ASSERTION.
        if node.kind == syntax_kind_ext::TYPE_ASSERTION
            && let Some(stripped) = target.strip_suffix('>')
        {
            // Only strip if brackets are unbalanced (more `>` than `<`),
            // so legitimate generic types like `Array<T>` are preserved.
            let open = stripped.chars().filter(|&c| c == '<').count();
            let close = stripped.chars().filter(|&c| c == '>').count();
            if close < open || (open == 0 && close == 0) {
                target = stripped.to_string();
            }
        }
        Some((source, target))
    }

    // =========================================================================
    // Generic Type Errors
    // =========================================================================

    /// Report TS2314: Generic type 'X' requires N type argument(s).
    pub fn error_generic_type_requires_type_arguments_at(
        &mut self,
        name: &str,
        required_count: usize,
        idx: NodeIndex,
    ) {
        let count_str = required_count.to_string();
        self.error_at_node_msg(
            idx,
            diagnostic_codes::GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S,
            &[name, &count_str],
        );
    }

    /// Report TS2314 at an explicit source location.
    pub fn error_generic_type_requires_type_arguments_at_span(
        &mut self,
        name: &str,
        required_count: usize,
        start: u32,
        length: u32,
    ) {
        let message = format_message(
            diagnostic_messages::GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S,
            &[name, &required_count.to_string()],
        );
        self.ctx.error(
            start,
            length,
            message,
            diagnostic_codes::GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S,
        );
    }

    /// Report TS2344: Type does not satisfy constraint.
    pub fn error_type_constraint_not_satisfied(
        &mut self,
        type_arg: TypeId,
        constraint: TypeId,
        idx: NodeIndex,
    ) {
        // Suppress cascade errors from unresolved types
        if type_arg == TypeId::ERROR
            || constraint == TypeId::ERROR
            || type_arg == TypeId::UNKNOWN
            || constraint == TypeId::UNKNOWN
            || type_arg == TypeId::ANY
            || constraint == TypeId::ANY
        {
            return;
        }

        // Also suppress when either side CONTAINS error types (e.g., { new(): error }).
        // This happens when a forward-referenced class hasn't been fully resolved yet.
        if common::contains_error_type(self.ctx.types, type_arg)
            || common::contains_error_type(self.ctx.types, constraint)
        {
            return;
        }

        // tsc widens literal types to their base types in TS2344 messages:
        // e.g., `42` → `number`, `"hello"` → `string`. This matches
        // tsc's getBaseTypeOfLiteralType applied before typeToString.
        let widened_arg = tsz_solver::widen_literal_type(self.ctx.types, type_arg);
        let type_str = self.format_type_diagnostic(widened_arg);
        let constraint_str = self.format_type_diagnostic(constraint);
        self.error_at_node_msg(
            idx,
            diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
            &[&type_str, &constraint_str],
        );
    }

    /// Report TS2559: Type has no properties in common with constraint.
    ///
    /// Emitted instead of TS2344 when the constraint is a "weak type" (all-optional
    /// properties) and the type argument shares no common properties with it. tsc
    /// emits TS2559 in this case because the failure is specifically about weak type
    /// detection, not a general constraint violation.
    pub fn error_no_common_properties_constraint(
        &mut self,
        type_arg: TypeId,
        constraint: TypeId,
        idx: NodeIndex,
    ) {
        if type_arg == TypeId::ERROR
            || constraint == TypeId::ERROR
            || type_arg == TypeId::ANY
            || constraint == TypeId::ANY
        {
            return;
        }

        let type_str = self.format_type_diagnostic(type_arg);
        let constraint_str = self.format_type_diagnostic(constraint);
        self.error_at_node_msg(
            idx,
            diagnostic_codes::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
            &[&type_str, &constraint_str],
        );
    }

    /// Report TS2352: Conversion of type 'X' to type 'Y' may be a mistake because neither type
    /// sufficiently overlaps with the other. If this was intentional, convert the expression to
    /// 'unknown' first.
    pub fn error_type_assertion_no_overlap(
        &mut self,
        source_type: TypeId,
        target_type: TypeId,
        idx: NodeIndex,
    ) {
        let source_special =
            self.try_format_type_assertion_overlap_special_display(source_type, true);
        let target_special =
            self.try_format_type_assertion_overlap_special_display(target_type, false);
        let source_str = self.format_type_assertion_overlap_display(source_type, true);
        let target_str = self.format_type_assertion_overlap_display(target_type, false);
        let (source_str, target_str) = if source_special.is_some() || target_special.is_some() {
            (source_str, target_str)
        } else if let Some((declared_source, declared_target)) =
            self.assertion_declared_type_texts(idx)
        {
            (declared_source, declared_target)
        } else {
            (source_str, target_str)
        };
        let source_str = source_str.trim_end_matches(';').to_string();
        let target_str = target_str.trim_end_matches(';').to_string();
        let message = format_message(
            diagnostic_messages::CONVERSION_OF_TYPE_TO_TYPE_MAY_BE_A_MISTAKE_BECAUSE_NEITHER_TYPE_SUFFICIENTLY_OV,
            &[&source_str, &target_str],
        );
        if let Some((start, len)) = self.jsdoc_type_tag_expr_span_for_node_direct(idx) {
            self.ctx.error(
                start,
                len,
                message,
                diagnostic_codes::CONVERSION_OF_TYPE_TO_TYPE_MAY_BE_A_MISTAKE_BECAUSE_NEITHER_TYPE_SUFFICIENTLY_OV,
            );
            return;
        }
        self.emit_render_request(
            idx,
            DiagnosticRenderRequest::simple(
                DiagnosticAnchorKind::TypeAssertionOverlap { target_type },
                diagnostic_codes::CONVERSION_OF_TYPE_TO_TYPE_MAY_BE_A_MISTAKE_BECAUSE_NEITHER_TYPE_SUFFICIENTLY_OV,
                message,
            ),
        );
    }

    // =========================================================================
    // Diagnostic Utilities
    // =========================================================================

    /// Create a diagnostic collector for batch error reporting.
    pub fn create_diagnostic_collector(&self) -> tsz_solver::DiagnosticCollector<'_> {
        tsz_solver::DiagnosticCollector::new(self.ctx.types, self.ctx.file_name.as_str())
    }

    /// Merge diagnostics from a collector into the checker's diagnostics.
    pub fn merge_diagnostics(&mut self, collector: &tsz_solver::DiagnosticCollector) {
        for diag in collector.to_checker_diagnostics() {
            self.ctx.diagnostics.push(diag);
        }
    }
}
