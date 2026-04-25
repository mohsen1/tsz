//! Finalization helpers for call/new expressions.
//!
//! Split out of `call_helpers.rs` to keep per-file LOC under the checker
//! architecture guardrail (2000 lines). Contains:
//! - CommonJS `require` runtime-shim detection,
//! - contextual call param normalization and generic-call result finalization,
//! - `this.property<T>(...)` TS2347 suppression helpers.

use crate::query_boundaries::checkers::call::{
    get_contextual_signature_for_arity, is_type_parameter_type,
};
use crate::query_boundaries::common;
use crate::query_boundaries::common::CallResult;
use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

use super::call_inference::should_preserve_contextual_application_shape;

impl<'a> CheckerState<'a> {
    fn symbol_has_nonambient_local_declaration(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        symbol.declarations.iter().any(|&decl_idx| {
            self.ctx.arena.get(decl_idx).is_some()
                && !self.ctx.arena.is_in_ambient_context(decl_idx)
        })
    }

    fn is_declaration_file_runtime_shim_symbol(&self, sym_id: SymbolId, name: &str) -> bool {
        if self.is_cross_file_declaration_runtime_shim(sym_id, name) {
            return true;
        }

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        if symbol.escaped_name != name || symbol.decl_file_idx == u32::MAX {
            return false;
        }

        self.ctx
            .get_arena_for_file(symbol.decl_file_idx)
            .source_files
            .first()
            .is_some_and(|source_file| source_file.is_declaration_file)
    }

    fn is_cross_file_declaration_runtime_shim(&self, sym_id: SymbolId, name: &str) -> bool {
        let Some(symbol) = self.get_cross_file_symbol(sym_id) else {
            return false;
        };
        if symbol.escaped_name != name
            || symbol.decl_file_idx == u32::MAX
            || symbol.decl_file_idx == self.ctx.current_file_idx as u32
        {
            return false;
        }

        self.ctx
            .get_arena_for_file(symbol.decl_file_idx)
            .source_files
            .first()
            .is_some_and(|source_file| source_file.is_declaration_file)
    }

    pub(crate) fn is_unshadowed_commonjs_require_identifier(&mut self, idx: NodeIndex) -> bool {
        // JavaScript/checkJs files use CommonJS-style `require(...)` value resolution
        // even when the `module` compiler option stays at its default script mode.
        // Keep the special module-value path available there so property presence,
        // assignment compatibility, and call diagnostics all see the same module type.
        if !self.ctx.compiler_options.module.is_commonjs() && !self.is_js_file() {
            return false;
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(node) else {
            return false;
        };
        if ident.escaped_text != "require" {
            return false;
        }

        let resolved_symbol = self
            .ctx
            .binder
            .node_symbols
            .get(&idx.0)
            .copied()
            .or_else(|| self.resolve_identifier_symbol(idx));
        if resolved_symbol
            .is_some_and(|sym_id| self.is_declaration_file_runtime_shim_symbol(sym_id, "require"))
        {
            return true;
        }

        if let Some(sym_id) = resolved_symbol
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && (symbol.decl_file_idx == self.ctx.current_file_idx as u32
                || symbol
                    .declarations
                    .iter()
                    .any(|&decl_idx| self.ctx.arena.get(decl_idx).is_some()))
            && self.symbol_has_nonambient_local_declaration(sym_id)
        {
            return false;
        }

        if self.is_js_file() {
            if let Some(sym_id) = self.ctx.binder.file_locals.get("require")
                && self.is_declaration_file_runtime_shim_symbol(sym_id, "require")
            {
                return true;
            }
            if let Some(sym_id) = self.ctx.binder.file_locals.get("require")
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && (symbol.decl_file_idx == self.ctx.current_file_idx as u32
                    || symbol
                        .declarations
                        .iter()
                        .any(|&decl_idx| self.ctx.arena.get(decl_idx).is_some()))
                && self.symbol_has_nonambient_local_declaration(sym_id)
            {
                return false;
            }
            return true;
        }

        let Some(sym_id) = resolved_symbol else {
            return true;
        };

        if self.is_js_file() && self.is_cross_file_declaration_runtime_shim(sym_id, "require") {
            return true;
        }

        let lib_binders = self.get_lib_binders();
        let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
            return true;
        };

        !symbol
            .declarations
            .iter()
            .any(|decl_idx| self.ctx.binder.node_symbols.contains_key(&decl_idx.0))
    }

    pub(crate) fn normalize_contextual_call_param_type(&mut self, param_type: TypeId) -> TypeId {
        if common::is_callable_type(self.ctx.types, param_type)
            || should_preserve_contextual_application_shape(self.ctx.types, param_type)
        {
            return param_type;
        }

        if let Some(members) = common::union_members(self.ctx.types, param_type) {
            let evaluated_members: Vec<_> = members
                .iter()
                .map(|&member| {
                    if should_preserve_contextual_application_shape(self.ctx.types, member) {
                        member
                    } else {
                        self.evaluate_type_with_env(member)
                    }
                })
                .collect();
            if evaluated_members
                .iter()
                .zip(members.iter())
                .all(|(evaluated, original)| evaluated == original)
            {
                return param_type;
            }

            let reduced = self.ctx.types.union_literal_reduce(evaluated_members);
            if reduced != param_type
                && let Some(def_id) = self.ctx.definition_store.find_def_for_type(param_type)
            {
                self.ctx
                    .definition_store
                    .register_type_to_def(reduced, def_id);
            }
            return reduced;
        }

        self.evaluate_type_with_env(param_type)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn finalize_generic_call_result(
        &mut self,
        callee_type_for_call: TypeId,
        generic_instantiated_params: Option<&Vec<tsz_solver::ParamInfo>>,
        args: &[NodeIndex],
        arg_types: &[TypeId],
        result: CallResult,
        sanitized_generic_inference: bool,
        needs_real_type_recheck: bool,
        _shape_this_type: Option<TypeId>,
    ) -> (CallResult, bool) {
        if let Some(instantiated_params) = generic_instantiated_params {
            self.propagate_generic_constructor_display_defs(
                callee_type_for_call,
                args.len(),
                instantiated_params,
            );
        }

        let mut allow_contextual_mismatch_deferral = true;
        let result = if let Some(instantiated_params) = generic_instantiated_params {
            let expected_param_types = self.contextual_param_types_from_instantiated_params(
                instantiated_params,
                arg_types.len(),
            );
            let mut recovered_argument_mismatch = false;
            let result = if sanitized_generic_inference || needs_real_type_recheck {
                self.recheck_generic_call_arguments_with_real_types(
                    result,
                    instantiated_params,
                    args,
                    arg_types,
                )
            } else {
                result
            };
            let (result, should_epc) = match result {
                CallResult::Success(return_type) => (CallResult::Success(return_type), true),
                CallResult::ArgumentTypeMismatch {
                    index,
                    actual,
                    expected,
                    fallback_return,
                } => {
                    if let Some(param) = instantiated_params.get(index).or_else(|| {
                        let last = instantiated_params.last()?;
                        last.rest.then_some(last)
                    }) {
                        let evaluated_param = self.evaluate_type_with_env(param.type_id);
                        // Detect variadic tuple spread markers: when a generic type
                        // parameter spread `...u` (where u: U extends SomeArray[])
                        // is collected, the call checker wraps it as `[...U]` (a
                        // single-rest-element tuple).  For the post-inference
                        // assignability check we need to compare the spread marker
                        // against the full rest parameter array type, not the
                        // element type, because `[...U]` represents the whole
                        // spread, not an individual element.
                        let arg_is_variadic_spread_marker = param.rest
                            && arg_types
                                .get(index)
                                .and_then(|&arg_ty| common::tuple_elements(self.ctx.types, arg_ty))
                                .is_some_and(|elems| elems.len() == 1 && elems[0].rest);
                        let expected_param = if arg_is_variadic_spread_marker {
                            // Use the full rest parameter type (the array itself),
                            // not its element type. The arg `[...U]` will be
                            // compared structurally against this array type.
                            evaluated_param
                        } else {
                            expected_param_types
                                .get(index)
                                .copied()
                                .flatten()
                                .unwrap_or_else(|| {
                                    if param.rest {
                                        self.rest_argument_element_type_with_env(evaluated_param)
                                    } else {
                                        evaluated_param
                                    }
                                })
                        };
                        let expected_param =
                            if is_type_parameter_type(self.ctx.types, expected_param) {
                                common::type_parameter_constraint(self.ctx.types, expected_param)
                                    .filter(|constraint| {
                                        !common::contains_type_parameters(
                                            self.ctx.types,
                                            *constraint,
                                        )
                                    })
                                    .unwrap_or(expected_param)
                            } else {
                                expected_param
                            };
                        let reported_expected_param = get_contextual_signature_for_arity(
                            self.ctx.types,
                            callee_type_for_call,
                            args.len(),
                        )
                        .and_then(|shape| {
                            shape
                                .params
                                .get(index)
                                .map(|param| param.type_id)
                                .or_else(|| {
                                    let last = shape.params.last()?;
                                    last.rest.then_some(last.type_id)
                                })
                        })
                        .filter(|raw_expected| {
                            common::contains_type_parameters(self.ctx.types, *raw_expected)
                                && common::contains_type_by_id(
                                    self.ctx.types,
                                    expected_param,
                                    TypeId::UNKNOWN,
                                )
                                && self.evaluate_type_with_env(*raw_expected) == expected_param
                        })
                        .unwrap_or(expected_param);
                        let arg_type = args
                            .get(index)
                            .copied()
                            .map(|arg_idx| {
                                self.refreshed_generic_call_arg_type_with_context(
                                    arg_idx,
                                    arg_types.get(index).copied().unwrap_or(TypeId::UNKNOWN),
                                    Some(expected_param),
                                )
                            })
                            .unwrap_or(TypeId::UNKNOWN);
                        let fresh_assignable = self
                            .is_assignable_to_with_env(arg_type, expected_param)
                            || self
                                .is_assignable_via_contextual_signatures(arg_type, expected_param);
                        let excess_property_recovery = if !fresh_assignable {
                            args.get(index)
                                .copied()
                                .filter(|&arg_idx| {
                                    self.ctx.arena.get(arg_idx).is_some_and(|arg_node| {
                                        arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                    })
                                })
                                .is_some_and(|arg_idx| {
                                    if self
                                        .ctx
                                        .generic_excess_skip
                                        .as_ref()
                                        .is_some_and(|skip| index < skip.len() && skip[index])
                                    {
                                        return false;
                                    }
                                    if is_type_parameter_type(self.ctx.types, expected_param) {
                                        return false;
                                    }
                                    if self.contextual_type_is_unresolved_for_argument_refresh(
                                        expected_param,
                                    ) {
                                        return false;
                                    }
                                    let excess_snap = self.ctx.snapshot_diagnostics();
                                    self.check_object_literal_excess_properties(
                                        arg_type,
                                        expected_param,
                                        arg_idx,
                                    );
                                    self.ctx.has_speculative_diagnostics(&excess_snap)
                                })
                        } else {
                            false
                        };
                        if !fresh_assignable
                            && !excess_property_recovery
                            && !self
                                .should_defer_contextual_argument_mismatch(arg_type, expected_param)
                        {
                            allow_contextual_mismatch_deferral = false;
                        }
                        recovered_argument_mismatch = fresh_assignable || excess_property_recovery;
                        (
                            CallResult::ArgumentTypeMismatch {
                                index,
                                expected: reported_expected_param,
                                actual: arg_type,
                                fallback_return,
                            },
                            fresh_assignable || excess_property_recovery,
                        )
                    } else {
                        (
                            CallResult::ArgumentTypeMismatch {
                                index,
                                actual,
                                expected,
                                fallback_return,
                            },
                            false,
                        )
                    }
                }
                other => (other, false),
            };
            if should_epc {
                for (i, &arg_idx) in args.iter().enumerate() {
                    if let Some(arg_node) = self.ctx.arena.get(arg_idx)
                        && arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        && let Some(param) = instantiated_params.get(i)
                        && param.type_id != TypeId::ANY
                        && param.type_id != TypeId::UNKNOWN
                    {
                        if self
                            .ctx
                            .generic_excess_skip
                            .as_ref()
                            .is_some_and(|skip| i < skip.len() && skip[i])
                        {
                            continue;
                        }
                        let evaluated_param = self.evaluate_type_with_env(param.type_id);
                        if !is_type_parameter_type(self.ctx.types, evaluated_param)
                            && !self
                                .contextual_type_is_unresolved_for_argument_refresh(evaluated_param)
                        {
                            // Use the unevaluated parameter type as the contextual
                            // type for object literal refresh so that ThisType<T>
                            // markers inside intersection type aliases (e.g.,
                            // `Props & ThisType<Instance>`) are preserved. Evaluating
                            // the intersection can strip the ThisType marker, causing
                            // false TS2339 when `this` is used in method bodies.
                            let arg_type = self.refreshed_generic_call_arg_type_with_context(
                                arg_idx,
                                arg_types.get(i).copied().unwrap_or(TypeId::UNKNOWN),
                                Some(param.type_id),
                            );
                            // Snapshot diagnostics before the excess property check.
                            // The re-evaluation may produce spurious TS18046 ("is of
                            // type 'unknown'") from expression-body callbacks in nested
                            // object literals when the contextual parameter type uses
                            // a constraint (e.g., Record<string, unknown>) rather than
                            // the final inferred type. Filter these out after the check.
                            let epc_snap = self.ctx.snapshot_diagnostics();
                            self.check_object_literal_excess_properties(
                                arg_type,
                                evaluated_param,
                                arg_idx,
                            );
                            self.ctx.rollback_diagnostics_filtered(&epc_snap, |diag| {
                                !matches!(
                                    diag.code,
                                    crate::diagnostics::diagnostic_codes::IS_OF_TYPE_UNKNOWN
                                        | crate::diagnostics::diagnostic_codes::OBJECT_IS_OF_TYPE_UNKNOWN
                                )
                            });
                        }
                    }
                }
                if recovered_argument_mismatch {
                    if let CallResult::ArgumentTypeMismatch {
                        fallback_return, ..
                    } = &result
                    {
                        CallResult::Success(*fallback_return)
                    } else {
                        result
                    }
                } else {
                    result
                }
            } else {
                result
            }
        } else {
            result
        };

        (result, allow_contextual_mismatch_deferral)
    }

    pub(crate) fn try_emit_ts2339_for_missing_this_property(
        &mut self,
        callee_expr: NodeIndex,
    ) -> bool {
        if self.ctx.enclosing_class.is_none() {
            return false;
        }

        let Some(callee_node) = self.ctx.arena.get(callee_expr) else {
            return false;
        };
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };

        let Some(expr_node) = self.ctx.arena.get(access.expression) else {
            return false;
        };
        if expr_node.kind != tsz_scanner::SyntaxKind::ThisKeyword as u16 {
            return false;
        }

        let Some(property_name) = self.get_property_name(access.name_or_argument) else {
            return false;
        };

        let this_type = self.get_type_of_node(access.expression);

        // When `this` resolves to ANY (common in static methods where the constructor
        // type isn't fully resolved), fall back to checking the class symbol's member
        // table directly via the binder. If the property exists as a class member,
        // suppress TS2347 — the call target is typed, not genuinely untyped.
        if this_type == TypeId::ANY || this_type == TypeId::ERROR {
            if let Some(ref class_info) = self.ctx.enclosing_class.clone()
                && let Some(&class_sym) = self.ctx.binder.node_symbols.get(&class_info.class_idx.0)
                && let Some(class_symbol) = self.ctx.binder.get_symbol(class_sym)
            {
                let found_in_exports = class_symbol
                    .exports
                    .as_ref()
                    .and_then(|e| e.get(&property_name))
                    .is_some();
                let found_in_members = class_symbol
                    .members
                    .as_ref()
                    .and_then(|m| m.get(&property_name))
                    .is_some();
                if found_in_exports || found_in_members {
                    return true; // suppress TS2347
                }
            }
            return false;
        }

        let result = self.resolve_property_access_with_env(this_type, &property_name);
        match result {
            crate::query_boundaries::common::PropertyAccessResult::PropertyNotFound { .. } => {
                self.error_property_not_exist_at(
                    &property_name,
                    this_type,
                    access.name_or_argument,
                );
                true
            }
            crate::query_boundaries::common::PropertyAccessResult::Success { type_id, .. }
                if type_id == TypeId::ANY =>
            {
                // Property exists but is explicitly typed as `any` — the call target
                // is genuinely untyped. Do NOT suppress TS2347.
                // e.g., `private foo: any; this.foo<string>()` should emit TS2347.
                false
            }
            _ => {
                // Property exists on a concrete `this` type — the callee resolved to ANY
                // due to generic instantiation limitations, not because it's genuinely untyped.
                // Suppress TS2347 (e.g., `this.one<T>(...)` in static generic methods).
                true
            }
        }
    }

    /// Suppress TS2347 for `this.property<T>(...)` inside a class.
    /// When an enclosing class exists and the property is a known member that is NOT
    /// explicitly typed as `any`, suppress — the callee is typed, ANY came from
    /// resolution limitations. When the property is explicitly `any`, do NOT suppress
    /// because the call target is genuinely untyped.
    pub(crate) fn is_this_property_access_in_class_context(
        &mut self,
        callee_expr: NodeIndex,
    ) -> bool {
        let Some(callee_node) = self.ctx.arena.get(callee_expr) else {
            return false;
        };
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };
        let Some(expr_node) = self.ctx.arena.get(access.expression) else {
            return false;
        };
        if expr_node.kind != tsz_scanner::SyntaxKind::ThisKeyword as u16 {
            return false;
        }

        if self.nearest_enclosing_class(callee_expr).is_none() {
            return false;
        }

        // Check if the property resolves to `any` — if so, the call target is genuinely
        // untyped and TS2347 should fire.
        let this_type = self.get_type_of_node(access.expression);
        if this_type != TypeId::ANY
            && this_type != TypeId::ERROR
            && let Some(property_name) = self.get_property_name(access.name_or_argument)
        {
            let result = self.resolve_property_access_with_env(this_type, &property_name);
            if let crate::query_boundaries::common::PropertyAccessResult::Success {
                type_id, ..
            } = result
                && type_id == TypeId::ANY
            {
                return false; // genuinely `any` — don't suppress TS2347
            }
        }

        true
    }
}
