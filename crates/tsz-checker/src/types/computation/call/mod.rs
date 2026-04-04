//! Call expression type computation for `CheckerState`.
//!
//! Handles call expression type resolution including overload resolution,
//! argument type checking, type argument validation, and call result processing.
//! Identifier resolution is in `identifier.rs` and tagged
//! template expression handling is in `tagged_template.rs`.
//!
//! Split into submodules:
//! - `inner` — the main `get_type_of_call_expression_inner` implementation

mod inner;

use crate::context::TypingRequest;
use crate::state::CheckerState;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn callee_suppresses_contextual_any(
        &self,
        callee_idx: NodeIndex,
        snap: &crate::context::speculation::DiagnosticSnapshot,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let callee_idx = self.ctx.arena.skip_parenthesized_and_assertions(callee_idx);
        let Some(callee_node) = self.ctx.arena.get(callee_idx) else {
            return false;
        };

        let is_simple_error_path = matches!(
            callee_node.kind,
            k if k == tsz_scanner::SyntaxKind::Identifier as u16
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        );
        if !is_simple_error_path {
            return false;
        }

        let has_callee_side_failure =
            self.ctx.speculative_diagnostics_since(snap).iter().any(|diag| {
                diag.start >= callee_node.pos
                    && diag.start < callee_node.end
                    && matches!(
                        diag.code,
                        diagnostic_codes::CANNOT_FIND_NAME
                            | diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN
                            | diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_STATIC_MEMBER
                            | diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_INSTANCE_MEMBER_THIS
                            | diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
                            | diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN
                            | diagnostic_codes::CANNOT_USE_NAMESPACE_AS_A_VALUE
                            | diagnostic_codes::VALUE_OF_TYPE_IS_NOT_CALLABLE_DID_YOU_MEAN_TO_INCLUDE_NEW
                            | diagnostic_codes::THIS_EXPRESSION_IS_NOT_CALLABLE
                            | diagnostic_codes::TYPE_HAS_NO_CALL_SIGNATURES
                    )
            });

        has_callee_side_failure || self.property_access_base_is_error_symbol(callee_idx)
    }

    fn property_access_base_is_error_symbol(&self, callee_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let callee_idx = self.ctx.arena.skip_parenthesized_and_assertions(callee_idx);
        let Some(callee_node) = self.ctx.arena.get(callee_idx) else {
            return false;
        };
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && callee_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };
        let base_expr = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(access.expression);
        let Some(base_node) = self.ctx.arena.get(base_expr) else {
            return false;
        };
        if base_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return false;
        }

        self.resolve_identifier_symbol(base_expr)
            .and_then(|sym_id| self.ctx.symbol_types.get(&sym_id).copied())
            == Some(TypeId::ERROR)
    }

    fn reemit_namespace_value_error_for_call_callee(&mut self, callee_idx: NodeIndex) {
        use tsz_parser::parser::syntax_kind_ext;

        let callee_idx = self.ctx.arena.skip_parenthesized_and_assertions(callee_idx);
        let Some(callee_node) = self.ctx.arena.get(callee_idx) else {
            return;
        };

        let base_expr = match callee_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                self.ctx
                    .arena
                    .get_access_expr(callee_node)
                    .map(|access| access.expression)
            }
            _ => None,
        };

        let Some(base_expr) = base_expr else {
            return;
        };
        let base_expr = self.ctx.arena.skip_parenthesized_and_assertions(base_expr);

        let _ = self.report_namespace_value_access_for_type_only_import_equals_expr(base_expr);
    }

    /// Get the type of a call expression (e.g., `foo()`, `obj.method()`).
    ///
    /// Computes the return type of function/method calls.
    /// Handles:
    /// - Dynamic imports (returns `Promise<any>`)
    /// - Super calls (returns `void`)
    /// - Optional chaining (`obj?.method()`)
    /// - Overload resolution
    /// - Argument type checking
    /// - Type argument validation (TS2344)
    #[allow(dead_code)]
    pub(crate) fn get_type_of_call_expression(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_call_expression_with_request(idx, &TypingRequest::NONE)
    }

    pub(crate) fn get_type_of_call_expression_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        // Check call depth limit to prevent infinite recursion
        if !self.ctx.call_depth.borrow_mut().enter() {
            return TypeId::ERROR;
        }

        let result = self.get_type_of_call_expression_inner(idx, request);

        // TS2590: Check if the call produced a union type that is too complex.
        // The solver sets a flag during union normalization when the constituent
        // count exceeds the threshold. We check and clear it here to emit the
        // diagnostic at the call expression that triggered it.
        if self.ctx.types.take_union_too_complex() {
            use crate::diagnostics::diagnostic_messages;
            self.error_at_node(
                idx,
                diagnostic_messages::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT,
                diagnostic_codes::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT,
            );
        }

        self.ctx.call_depth.borrow_mut().leave();
        result
    }

    /// Check if a call is a dynamic import and handle all associated diagnostics.
    /// Returns `Some(type_id)` if this is a dynamic import (the caller should return it),
    /// or `None` if this is not a dynamic import.
    fn check_and_resolve_dynamic_import(
        &mut self,
        idx: NodeIndex,
        call: &tsz_parser::parser::node::CallExprData,
    ) -> Option<TypeId> {
        if !self.is_dynamic_import(call) {
            return None;
        }

        // TS1323: Dynamic imports require a module kind that supports them
        if !self.ctx.compiler_options.module.supports_dynamic_import() {
            self.error_at_node(
                idx,
                crate::diagnostics::diagnostic_messages::DYNAMIC_IMPORTS_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_FLAG_IS_SET_TO_ES2020_ES2022,
                diagnostic_codes::DYNAMIC_IMPORTS_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_FLAG_IS_SET_TO_ES2020_ES2022,
            );
        }

        // TS1325: Check for spread elements in import arguments
        if let Some(ref args_list) = call.arguments {
            for &arg_idx in &args_list.nodes {
                if let Some(arg_node) = self.ctx.arena.get(arg_idx)
                    && arg_node.kind == tsz_parser::parser::syntax_kind_ext::SPREAD_ELEMENT
                {
                    self.error_at_node(
                        arg_idx,
                        crate::diagnostics::diagnostic_messages::ARGUMENT_OF_DYNAMIC_IMPORT_CANNOT_BE_SPREAD_ELEMENT,
                        diagnostic_codes::ARGUMENT_OF_DYNAMIC_IMPORT_CANNOT_BE_SPREAD_ELEMENT,
                    );
                }
            }
        }

        // TS1324: Second argument only supported for certain module kinds.
        // Only emit when dynamic imports are supported (TS1323 not emitted),
        // otherwise TS1323 already covers the unsupported case.
        if let Some(ref args_list) = call.arguments
            && args_list.nodes.len() >= 2
            && self.ctx.compiler_options.module.supports_dynamic_import()
            && !self
                .ctx
                .compiler_options
                .module
                .supports_dynamic_import_options()
        {
            self.error_at_node(
                args_list.nodes[1],
                crate::diagnostics::diagnostic_messages::DYNAMIC_IMPORTS_ONLY_SUPPORT_A_SECOND_ARGUMENT_WHEN_THE_MODULE_OPTION_IS_SET_TO,
                diagnostic_codes::DYNAMIC_IMPORTS_ONLY_SUPPORT_A_SECOND_ARGUMENT_WHEN_THE_MODULE_OPTION_IS_SET_TO,
            );
        }

        // TS7036: Check specifier type is assignable to `string`
        self.check_dynamic_import_specifier_type(call);
        // TS2322/TS2559: Check options arg against ImportCallOptions
        self.check_dynamic_import_options_type(call);
        self.check_dynamic_import_module_specifier(call);

        // TS2712: Dynamic import requires Promise constructor support from the
        // active libs / declarations. This is lib-driven, not target-driven:
        // `@target: es2015` with `@lib: es5` still needs the diagnostic.
        if self.ctx.promise_constructor_diagnostics_required() {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                idx,
                diagnostic_messages::A_DYNAMIC_IMPORT_CALL_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YOU_HAVE,
                diagnostic_codes::A_DYNAMIC_IMPORT_CALL_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YOU_HAVE,
            );
        }

        // Dynamic imports return Promise<typeof module>
        // This creates Promise<ModuleNamespace> where ModuleNamespace contains all exports
        Some(self.get_dynamic_import_type(call))
    }

    /// Handle `unknown` and `never` callee types with appropriate diagnostics.
    /// Returns `Some(type_id)` if the callee type was handled (caller should return),
    /// or `None` to continue with normal call resolution.
    fn check_callee_unknown_or_never(
        &mut self,
        callee_type: TypeId,
        callee_expr: NodeIndex,
        args: &[NodeIndex],
    ) -> Option<TypeId> {
        use crate::call_checker::CallableContext;
        use tsz_parser::parser::syntax_kind_ext;

        // TS18046: Calling an expression of type `unknown` is not allowed.
        // tsc emits TS18046 instead of TS2349 when the callee is `unknown`.
        // Without strictNullChecks, unknown is treated like any (callable, returns any).
        if callee_type == TypeId::UNKNOWN {
            if self.error_is_of_type_unknown(callee_expr) {
                // Still need to check arguments for definite assignment (TS2454)
                let check_excess_properties = false;
                self.collect_call_argument_types_with_context(
                    args,
                    |_i, _arg_count| None,
                    check_excess_properties,
                    None,
                    CallableContext::none(),
                );
                return Some(TypeId::ERROR);
            }
            // Without strictNullChecks, treat unknown like any: callable, returns any
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| None,
                check_excess_properties,
                None,
                CallableContext::none(),
            );
            return Some(TypeId::ANY);
        }

        // Calling `never` returns `never` (bottom type propagation).
        // tsc treats `never` as having no call signatures.
        // For method calls (e.g., `a.toFixed()` where `a: never`), TS2339 is already
        // emitted by the property access check, so we suppress the redundant TS2349.
        // For direct calls on `never` (e.g., `f()` where `f: never`), emit TS2349.
        if callee_type == TypeId::NEVER {
            let is_method_call = matches!(
                self.ctx.arena.get(callee_expr).map(|n| n.kind),
                Some(
                    syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        | syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                )
            );
            if !is_method_call {
                self.error_not_callable_at(callee_type, callee_expr);
            }
            return Some(TypeId::NEVER);
        }

        None
    }
}

// Identifier resolution is in `identifier.rs`.
// Tagged template expression handling is in `tagged_template.rs`.
// TDZ checking, value declaration resolution, and other helpers are in
// `call_helpers.rs`.
