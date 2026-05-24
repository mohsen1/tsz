use crate::call_checker::CallableContext;
use crate::context::TypingRequest;
use crate::state::CheckerState;
use tracing::trace;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_parser::parser::node::{CallExprData, Node};
use tsz_parser::parser::{NodeIndex, NodeList, syntax_kind_ext};
use tsz_solver::TypeId;

pub(super) struct PreparedCallCallee {
    pub(super) callee_type: TypeId,
    pub(super) is_super_call: bool,
    pub(super) explicit_call_type_arguments: Option<NodeList>,
    pub(super) nullish_cause: Option<TypeId>,
}

pub(super) enum CallCalleePrep {
    Continue(PreparedCallCallee),
    Return(TypeId),
}

impl<'a> CheckerState<'a> {
    pub(super) fn prepare_call_callee(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
        node: &Node,
        call: &CallExprData,
        contextual_type: Option<TypeId>,
        args: &[NodeIndex],
    ) -> CallCalleePrep {
        if self.is_unshadowed_commonjs_require_identifier(call.expression)
            && let Some(args) = &call.arguments
            && let Some(first_arg) = args.nodes.first().copied()
            && let Some(module_specifier) = self.get_require_module_specifier(first_arg)
        {
            let module_type =
                self.commonjs_module_value_type(&module_specifier, Some(self.ctx.current_file_idx));
            if let Some(module_type) = module_type {
                return CallCalleePrep::Return(module_type);
            }
            self.emit_module_not_found_error(&module_specifier, first_arg);
            return CallCalleePrep::Return(TypeId::ANY);
        }

        let early_args: &[NodeIndex] = call
            .arguments
            .as_ref()
            .map(|a| a.nodes.as_slice())
            .unwrap_or(&[]);

        // For IIFEs, wrap the contextual type into a callable type so the
        // function expression resolver can extract the return type.
        let iife_info = self.setup_iife_contextual_type(call.expression, contextual_type);
        let higher_order_callee_context = if iife_info.is_none() {
            self.setup_higher_order_callee_contextual_type(
                call.expression,
                contextual_type,
                early_args,
            )
        } else {
            None
        };
        let callee_request = iife_info
            .map(|(wrapper_fn, _)| request.read().contextual(wrapper_fn))
            .or_else(|| {
                higher_order_callee_context.map(|wrapper_fn| request.read().contextual(wrapper_fn))
            })
            .unwrap_or(*request);
        if iife_info.is_some() || higher_order_callee_context.is_some() {
            self.invalidate_expression_for_contextual_retry(call.expression);
        }

        let callee_diag_snap = self.ctx.snapshot_diagnostics();
        let mut callee_type = if let Some(callee_node) = self.ctx.arena.get(call.expression) {
            if callee_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
                let identifier_text = self
                    .ctx
                    .arena
                    .get_identifier(callee_node)
                    .map(|ident| ident.escaped_text.as_str())
                    .unwrap_or_default();
                let direct_symbol = self
                    .ctx
                    .binder
                    .node_symbols
                    .get(&call.expression.0)
                    .copied();
                let fast_symbol = direct_symbol
                    .or_else(|| self.resolve_identifier_symbol(call.expression))
                    .filter(|&sym_id| {
                        self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                            let decl_idx = if symbol.value_declaration.is_some() {
                                Some(symbol.value_declaration)
                            } else if symbol.declarations.len() == 1 {
                                symbol.declarations.first().copied()
                            } else {
                                None
                            };
                            self.is_fast_path_function_decl(
                                sym_id,
                                symbol,
                                decl_idx,
                                direct_symbol,
                                identifier_text,
                            )
                        })
                    });
                if let Some(sym_id) = fast_symbol {
                    // Fast path intentionally skips identifier-side diagnostic probes
                    // (e.g. type-only import/value checks). The guard allows local,
                    // non-aliased function declarations in two cases:
                    // - implementation declarations without explicit return annotations
                    // - current-file direct ambient/overload signatures (no body)
                    self.ctx.referenced_symbols.borrow_mut().insert(sym_id);
                    let callee_ty = self.get_type_of_symbol(sym_id);
                    // Cache in node_types so flow narrowing can retrieve callee
                    // type predicates during type guard analysis.
                    if callee_request.is_empty() {
                        self.ctx.node_types.insert(call.expression.0, callee_ty);
                    }
                    callee_ty
                } else {
                    self.get_type_of_node_with_request(call.expression, &callee_request)
                }
            } else {
                self.get_type_of_node_with_request(call.expression, &callee_request)
            }
        } else {
            self.get_type_of_node_with_request(call.expression, &callee_request)
        };

        trace!(
            callee_type = ?callee_type,
            callee_expr = ?call.expression,
            "Call expression callee type resolved"
        );
        self.report_checked_js_nullable_this_property_method_call(call.expression);
        let callee_missing_value = callee_type == TypeId::ERROR
            && self.callee_suppresses_contextual_any(call.expression, &callee_diag_snap);

        // When the callee identifier resolves through a type-only alias chain,
        // `report_wrong_meaning` has just emitted TS1361/TS1362 at the callee
        // site. Even if the resolved callee_type still happens to be callable
        // (because the alias merges a namespace value with a function type
        // from the type-only-imported side), tsc treats `typeof <name>` as
        // having no call signatures in this position and emits TS2349 in
        // addition to TS1361/TS1362. Match that so the call site picks up the
        // companion "not callable" diagnostic. See `typeOnlyMerge3.ts`.
        let callee_emitted_type_only_value_error = self
            .ctx
            .speculative_diagnostics_since(&callee_diag_snap)
            .iter()
            .any(|diag| {
                self.ctx
                    .arena
                    .get(call.expression)
                    .is_some_and(|callee_node| {
                        diag.start >= callee_node.pos && diag.start < callee_node.end
                    })
                    && matches!(
                        diag.code,
                        diagnostic_codes::CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_EXPORTED_USING_EXPORT_TYPE
                            | diagnostic_codes::CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_IMPORTED_USING_IMPORT_TYPE
                    )
            });

        // Check for dynamic import module resolution (TS2307)
        if let Some(dynamic_import_type) = self.check_and_resolve_dynamic_import(idx, call) {
            return CallCalleePrep::Return(dynamic_import_type);
        }

        // Special handling for super() calls - treat as construct call
        let is_super_call = self.is_super_expression(call.expression);

        let explicit_call_type_arguments = call.type_arguments.clone().or_else(|| {
            self.ctx
                .arena
                .get(call.expression)
                .and_then(|node| self.ctx.arena.get_expr_type_args(node))
                .and_then(|expr_type_args| expr_type_args.type_arguments.clone())
        });

        if self.callee_name_conflicts_with_namespace_module(call.expression) {
            self.error_not_callable_at(callee_type, call.expression);
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| Some(TypeId::ANY),
                check_excess_properties,
                None,
                CallableContext::none(),
            );
            return CallCalleePrep::Return(TypeId::ERROR);
        }

        // Check if callee is any/error (don't report for those)
        if callee_type == TypeId::ANY {
            if let Some(ref type_args_list) = explicit_call_type_arguments
                && !type_args_list.nodes.is_empty()
            {
                // When the callee is a property access on `this` inside a class and
                // the property doesn't exist, tsc emits TS2339 (property not found)
                // instead of TS2347 (untyped function calls). The ANY here came from
                // this_type_stack suppression; check if the property genuinely doesn't
                // exist and emit TS2339 in that case.
                let suppressed_ts2347 = self
                    .try_emit_ts2339_for_missing_this_property(call.expression)
                    || self.is_this_property_access_in_class_context(call.expression);
                if !suppressed_ts2347 {
                    self.error_at_node(
                        idx,
                        crate::diagnostics::diagnostic_messages::UNTYPED_FUNCTION_CALLS_MAY_NOT_ACCEPT_TYPE_ARGUMENTS,
                        crate::diagnostics::diagnostic_codes::UNTYPED_FUNCTION_CALLS_MAY_NOT_ACCEPT_TYPE_ARGUMENTS,
                    );
                }
                // Resolve type arguments even though the call is untyped. Without
                // this, unresolved type names in arguments (e.g.
                // `g<InvalidReference>()`) silently succeed — tsc still emits
                // TS2304 for them. Mirrors the matching block in generic_checker.
                for &type_arg_idx in &type_args_list.nodes {
                    self.get_type_of_node(type_arg_idx);
                }
            }
            // Untyped calls accept ordinary args; callbacks still get their own context for TS7006.
            let cb_args: Vec<_> = args
                .iter()
                .map(|&idx| self.is_callback_like_argument(idx))
                .collect();
            self.collect_call_argument_types_with_context(
                args,
                |i, _arg_count| (!matches!(cb_args.get(i), Some(true))).then_some(TypeId::ANY),
                false,
                None, // No skipping needed
                CallableContext::none(),
            );
            return CallCalleePrep::Return(TypeId::ANY);
        }
        if callee_type == TypeId::ERROR
            && let Some(recovered_type) = self.recover_declared_type_for_tdz_callee(call.expression)
        {
            callee_type = recovered_type;
        }

        if callee_type == TypeId::ERROR {
            self.reemit_namespace_value_error_for_call_callee(call.expression);
            // Still evaluate type arguments to catch TS2304 for unresolved type names
            // (e.g., `this.super<T>(0)` where T is undeclared)
            if let Some(ref type_args_list) = explicit_call_type_arguments {
                for &arg_idx in &type_args_list.nodes {
                    self.get_type_from_type_node(arg_idx);
                }
            }
            // Still need to check arguments for definite assignment (TS2454) and other
            // errors. When the callee itself failed name/value resolution, avoid
            // fabricating contextual `any` for callback arguments because that would
            // suppress real TS7006 diagnostics. Other callee errors still preserve the
            // historical `any` fallback to avoid broader conformance regressions.
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |i, _arg_count| {
                    if !callee_missing_value {
                        return Some(TypeId::ANY);
                    }
                    args.get(i)
                        .copied()
                        .and_then(|arg_idx| self.ctx.arena.get(arg_idx))
                        .filter(|arg_node| arg_node.kind == syntax_kind_ext::SPREAD_ELEMENT)
                        .map(|_| TypeId::ANY)
                },
                check_excess_properties,
                None, // No skipping needed
                CallableContext::none(),
            );
            return CallCalleePrep::Return(TypeId::ERROR); // Return ERROR instead of ANY to expose type errors
        }

        // Handle unknown/never callee types as early returns.
        if let Some(early_return) =
            self.check_callee_unknown_or_never(callee_type, call.expression, args)
        {
            return CallCalleePrep::Return(early_return);
        }

        // tsc companion-emits TS2349 ("This expression is not callable. Type
        // 'typeof X' has no call signatures.") alongside TS1361/TS1362 when a
        // type-only-aliased identifier is used as a call target. tsz keeps
        // the underlying callable on the resolved type (because the alias
        // chain merged a namespace value with a function-typed type-only
        // import), so the call would otherwise resolve to Success and the
        // accompanying TS2349 would be missing. See `typeOnlyMerge3.ts`.
        if callee_emitted_type_only_value_error {
            // Still evaluate arguments so downstream definite-assignment /
            // unresolved-name diagnostics still fire on argument sites.
            return CallCalleePrep::Return(self.error_not_callable_and_collect_any_args(
                callee_type,
                call.expression,
                args,
            ));
        }

        if self.callee_name_conflicts_with_namespace_module(call.expression) {
            self.error_not_callable_at(callee_type, call.expression);
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| Some(TypeId::ANY),
                check_excess_properties,
                None,
                CallableContext::none(),
            );
            return CallCalleePrep::Return(TypeId::ERROR);
        }

        let mut nullish_cause = None;
        if self.call_expression_is_optional_chain(node, call.expression) {
            // Evaluate the callee type to resolve Application/Lazy types before
            // splitting nullish members. Without this, `Transform1<T>` stays as an
            // unevaluated Application and split_nullish_type can't see its union members.
            let callee_for_split = self.evaluate_type_with_env(callee_type);
            let (non_nullish, cause) = self.split_nullish_type(callee_for_split);
            nullish_cause = cause;
            let Some(non_nullish) = non_nullish else {
                return CallCalleePrep::Return(TypeId::UNDEFINED);
            };
            callee_type = non_nullish;
            if callee_type == TypeId::ANY {
                return CallCalleePrep::Return(TypeId::ANY);
            }
            if callee_type == TypeId::ERROR {
                return CallCalleePrep::Return(TypeId::ERROR); // Return ERROR instead of ANY to expose type errors
            }
        }

        CallCalleePrep::Continue(PreparedCallCallee {
            callee_type,
            is_super_call,
            explicit_call_type_arguments,
            nullish_cause,
        })
    }
}
