//! Finalization helpers for call/new expressions.
//!
//! Split out of `call_helpers.rs` to keep per-file LOC under the checker
//! architecture guardrail (2000 lines). Contains:
//! - CommonJS `require` runtime-shim detection,
//! - contextual call param normalization and generic-call result finalization,
//! - `this.property<T>(...)` TS2347 suppression helpers.

use crate::query_boundaries::checkers::call::is_type_parameter_type;
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
        if let Some(source) = self.homomorphic_readonly_contextual_source_type(param_type) {
            return source;
        }

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

    fn homomorphic_readonly_contextual_source_type(&mut self, type_id: TypeId) -> Option<TypeId> {
        let mapped = common::mapped_type_info(self.ctx.types, type_id)?;
        if mapped.name_type.is_some()
            || mapped.optional_modifier.is_some()
            || mapped.readonly_modifier != Some(tsz_solver::MappedModifier::Add)
        {
            return None;
        }

        let source = common::keyof_inner_type(self.ctx.types, mapped.constraint)?;
        let (template_object, template_index) =
            common::index_access_parts(self.ctx.types, mapped.template)?;
        if template_object != source {
            return None;
        }
        let template_param = common::type_param_info(self.ctx.types, template_index)?;
        if template_param.name != mapped.type_param.name {
            return None;
        }

        if common::object_shape_for_type(self.ctx.types, source).is_some() {
            return Some(source);
        }

        if common::type_param_info(self.ctx.types, source).is_some()
            && let Some(alias) = self.ctx.types.get_display_alias(type_id)
            && let Some((_, args)) = common::application_info(self.ctx.types, alias)
            && let Some(source_arg) = args.first().copied()
            && common::object_shape_for_type(self.ctx.types, source_arg).is_some()
        {
            return Some(source_arg);
        }

        None
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
                        let original_is_spread_marker = arg_types.get(index).is_some_and(|&ty| {
                            common::is_spread_marker_tuple(self.ctx.types.as_type_database(), ty)
                        });
                        let aggregate_rest_mismatch = param.rest
                            && (common::tuple_elements(self.ctx.types, actual).is_some()
                                || original_is_spread_marker)
                            && arg_types
                                .get(index)
                                .copied()
                                .is_none_or(|original| original != actual);
                        if aggregate_rest_mismatch {
                            let evaluated_param = self.evaluate_type_with_env(param.type_id);
                            let aggregate_assignable = self
                                .is_assignable_to_with_env(actual, expected)
                                || self.is_assignable_to_with_env(actual, evaluated_param);
                            if aggregate_assignable {
                                recovered_argument_mismatch = true;
                                (
                                    CallResult::ArgumentTypeMismatch {
                                        index,
                                        expected: evaluated_param,
                                        actual,
                                        fallback_return,
                                    },
                                    true,
                                )
                            } else {
                                allow_contextual_mismatch_deferral = false;
                                (
                                    CallResult::ArgumentTypeMismatch {
                                        index,
                                        expected,
                                        actual,
                                        fallback_return,
                                    },
                                    false,
                                )
                            }
                        } else {
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
                                    .and_then(|&arg_ty| {
                                        common::tuple_elements(self.ctx.types, arg_ty)
                                    })
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
                                            self.rest_argument_element_type_with_env(
                                                evaluated_param,
                                            )
                                        } else {
                                            evaluated_param
                                        }
                                    })
                            };
                            let expected_param =
                                if is_type_parameter_type(self.ctx.types, expected_param) {
                                    common::type_parameter_constraint(
                                        self.ctx.types,
                                        expected_param,
                                    )
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
                            // Use the substituted parameter type (with `unknown` for
                            // unresolved type parameters) to match tsc's diagnostic
                            // output. tsc displays the post-inference parameter type;
                            // when inference fails for an unconstrained type parameter
                            // it defaults to `unknown` and tsc surfaces that explicitly
                            // (e.g. `A<unknown>` rather than the raw `A<T>` from the
                            // signature source text).
                            let reported_expected_param = expected_param;
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
                                || self.is_assignable_via_contextual_signatures(
                                    arg_type,
                                    expected_param,
                                );
                            let excess_property_recovery = if !fresh_assignable {
                                args.get(index)
                                    .copied()
                                    .filter(|&arg_idx| {
                                        self.ctx.arena.get(arg_idx).is_some_and(|arg_node| {
                                            arg_node.kind
                                                == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                        })
                                    })
                                    .is_some_and(|arg_idx| {
                                        if self
                                            .ctx
                                            .generic_excess_skip
                                            .as_ref()
                                            .is_some_and(|skip| index < skip.len() && skip[index])
                                        {
                                            if self
                                                .check_object_literal_named_property_values_against_any_target(
                                                    arg_idx,
                                                    expected_param,
                                                )
                                            {
                                                return true;
                                            }
                                            if param.rest {
                                                return self
                                                    .check_generic_rest_object_literal_values_against_sibling_annotation(
                                                        arg_idx,
                                                        index,
                                                        args,
                                                    );
                                            }
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
                            // Let the central deferral policy decide contextual/generic
                            // constructor mismatches. The expected parameter can be fully
                            // concrete after outer call inference, while the source argument
                            // still has its own generic construct signature.
                            let should_consult_deferral_policy =
                                common::contains_type_parameters(self.ctx.types, expected_param)
                                    || self.generic_construct_argument_mismatch_may_be_contextual(
                                        arg_type,
                                        expected_param,
                                    );
                            let defer_mismatch = should_consult_deferral_policy
                                && self.should_defer_contextual_argument_mismatch(
                                    arg_type,
                                    expected_param,
                                );
                            if !fresh_assignable && !excess_property_recovery && !defer_mismatch {
                                allow_contextual_mismatch_deferral = false;
                            }
                            recovered_argument_mismatch =
                                fresh_assignable || excess_property_recovery;
                            (
                                CallResult::ArgumentTypeMismatch {
                                    index,
                                    expected: reported_expected_param,
                                    actual: arg_type,
                                    fallback_return,
                                },
                                fresh_assignable || excess_property_recovery,
                            )
                        }
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

    fn generic_construct_argument_mismatch_may_be_contextual(
        &mut self,
        actual: TypeId,
        expected: TypeId,
    ) -> bool {
        let (actual_has_construct, actual_has_generic_construct) =
            self.construct_signature_flags(actual);
        let (expected_has_construct, expected_has_generic_construct) =
            self.construct_signature_flags(expected);

        actual_has_construct
            && expected_has_construct
            && (actual_has_generic_construct || expected_has_generic_construct)
    }

    fn construct_signature_flags(&mut self, type_id: TypeId) -> (bool, bool) {
        self.construct_signature_flags_for_type(type_id)
            .or_else(|| {
                let evaluated = self.evaluate_type_with_env(type_id);
                (evaluated != type_id)
                    .then(|| self.construct_signature_flags_for_type(evaluated))
                    .flatten()
            })
            .unwrap_or((false, false))
    }

    fn construct_signature_flags_for_type(&self, type_id: TypeId) -> Option<(bool, bool)> {
        if let Some(shape) = common::callable_shape_for_type_extended(self.ctx.types, type_id)
            && !shape.construct_signatures.is_empty()
        {
            return Some((
                true,
                shape
                    .construct_signatures
                    .iter()
                    .any(|signature| !signature.type_params.is_empty()),
            ));
        }

        if let Some(shape) = common::function_shape_for_type(self.ctx.types, type_id)
            && shape.is_constructor
        {
            return Some((true, !shape.type_params.is_empty()));
        }

        None
    }

    fn check_generic_rest_object_literal_values_against_sibling_annotation(
        &mut self,
        obj_literal_idx: NodeIndex,
        current_arg_index: usize,
        args: &[NodeIndex],
    ) -> bool {
        for (arg_index, &arg_idx) in args.iter().enumerate() {
            if arg_index == current_arg_index {
                continue;
            }
            let Some(annotation_type) = self.declared_type_of_identifier_argument(arg_idx) else {
                continue;
            };
            if self.check_object_literal_named_property_values_against_any_target(
                obj_literal_idx,
                annotation_type,
            ) {
                return true;
            }
        }
        false
    }

    fn declared_type_of_identifier_argument(&mut self, arg_idx: NodeIndex) -> Option<TypeId> {
        let idx = self.ctx.arena.skip_parenthesized(arg_idx);
        let node = self.ctx.arena.get(idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self.resolve_identifier_symbol(idx)?;
        let stable_declarations = {
            let symbol = self
                .get_cross_file_symbol(sym_id)
                .or_else(|| self.ctx.binder.get_symbol(sym_id))?;
            symbol
                .stable_declarations
                .iter()
                .copied()
                .chain(std::iter::once(symbol.stable_value_declaration))
                .filter(|loc| loc.is_known())
                .collect::<Vec<_>>()
        };

        for stable_location in stable_declarations {
            let Some((decl_idx, arena)) = self.ctx.node_at_stable_location(stable_location) else {
                continue;
            };
            let Some(decl_node) = arena.get(decl_idx) else {
                continue;
            };
            let Some(decl) = arena.get_variable_declaration(decl_node) else {
                continue;
            };
            if decl.type_annotation.is_some() {
                if stable_location.has_file_idx()
                    && stable_location.file_idx != self.ctx.current_file_idx as u32
                {
                    return Some(self.type_of_value_declaration_for_cross_file_symbol(
                        sym_id,
                        decl_idx,
                        stable_location.file_idx as usize,
                    ));
                }
                if !std::ptr::eq(arena, self.ctx.arena)
                    && let Some(target_file_idx) = self.ctx.get_file_idx_for_arena(arena)
                    && target_file_idx != self.ctx.current_file_idx
                {
                    return Some(self.type_of_value_declaration_for_cross_file_symbol(
                        sym_id,
                        decl_idx,
                        target_file_idx,
                    ));
                }
                return Some(self.type_of_value_declaration_for_symbol(sym_id, decl_idx));
            }
        }

        None
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CheckerOptions;
    use crate::module_resolution::build_module_resolution_maps;
    use std::sync::Arc;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;
    use tsz_parser::parser::syntax_kind_ext;
    use tsz_solver::TypeInterner;

    #[test]
    fn declared_type_of_identifier_argument_resolves_cross_file_stable_declaration() {
        let files = [
            (
                "consumer.ts",
                r#"
declare function f<T>(...items: T[]): T;
f(data, { a: 2 });
"#,
            ),
            (
                "shared.ts",
                r#"
declare let data: { a: 1, b: "abc", c: true };
"#,
            ),
        ];

        let mut arenas = Vec::with_capacity(files.len());
        let mut binders = Vec::with_capacity(files.len());
        let mut roots = Vec::with_capacity(files.len());
        let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();
        for (file_idx, (name, source)) in files.iter().enumerate() {
            let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
            let root = parser.parse_source_file();
            let mut binder = BinderState::new();
            binder.set_file_idx(file_idx as u32);
            binder.bind_source_file(parser.get_arena(), root);
            arenas.push(Arc::new(parser.get_arena().clone()));
            binders.push(Arc::new(binder));
            roots.push(root);
        }

        let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);
        let all_arenas = Arc::new(arenas);
        let all_binders = Arc::new(binders);
        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            all_arenas[0].as_ref(),
            all_binders[0].as_ref(),
            &types,
            file_names[0].clone(),
            CheckerOptions::default(),
        );
        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        checker.ctx.set_current_file_idx(0);
        checker.ctx.set_lib_contexts(Vec::new());
        checker
            .ctx
            .set_resolved_module_paths(Arc::new(resolved_module_paths));
        checker.ctx.set_resolved_modules(resolved_modules);

        checker.check_source_file(roots[0]);

        let data_arg_idx = checker
            .ctx
            .arena
            .nodes
            .iter()
            .find_map(|node| {
                if node.kind != syntax_kind_ext::CALL_EXPRESSION {
                    return None;
                }
                let call = checker.ctx.arena.get_call_expr(node)?;
                let callee_node = checker.ctx.arena.get(call.expression)?;
                let callee_ident = checker.ctx.arena.get_identifier(callee_node)?;
                if callee_ident.escaped_text != "f" {
                    return None;
                }
                call.arguments.as_ref()?.nodes.first().copied()
            })
            .expect("expected to find f(data, ...) call in consumer.ts");

        let declared = checker.declared_type_of_identifier_argument(data_arg_idx);
        assert!(
            declared.is_some(),
            "cross-file typed identifier argument should resolve a declared type"
        );
    }
}
