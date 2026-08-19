//! Instantiated property-access annotation display for call-error diagnostics.
//!
//! Extracted from `display_formatting.rs` as pure code motion to keep that file
//! under the 2000-LOC arch cap.

use crate::query_boundaries::common as query_common;
use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn instantiated_property_access_annotation_display(
        &mut self,
        raw_shape: &tsz_solver::FunctionShape,
        call_args: &[NodeIndex],
        annotation_type_node: NodeIndex,
    ) -> Option<String> {
        // With no signature-owned type parameters there is no call-level
        // inference to reconstruct: the checked parameter type is already the
        // correct display. Re-resolving the declaration's annotation here
        // would drop enclosing type parameters (a method of a generic class
        // rendered `(a: T) => U` through this path resolves `T`/`U` outside
        // their scope to `any`), where tsc shows the receiver-instantiated
        // parameter type.
        if raw_shape.type_params.is_empty() {
            return None;
        }
        let mut replacements = FxHashMap::default();
        for (raw_param, &call_arg_idx) in raw_shape.params.iter().zip(call_args.iter()) {
            let actual_arg_type = self.elaboration_source_expression_type(call_arg_idx);
            self.collect_type_param_display_replacements(
                raw_param.type_id,
                actual_arg_type,
                &mut replacements,
            );
        }

        let mut display = self.sanitized_type_node_display(annotation_type_node)?;
        let mut replaced_any = false;
        for raw_tp in &raw_shape.type_params {
            let Some(&replacement_type) = replacements.get(&raw_tp.name) else {
                continue;
            };
            let replacement = self.format_type_for_assignability_message(replacement_type);
            let tp_name = self.ctx.types.resolve_atom_ref(raw_tp.name);
            display = Self::replace_type_param_name_in_display(&display, &tp_name, &replacement);
            replaced_any = true;
        }

        if replaced_any {
            return Some(self.format_annotation_like_type(&display));
        }

        let annotation_type = self.get_type_from_type_node(annotation_type_node);
        let type_args: Vec<_> = raw_shape
            .type_params
            .iter()
            .filter_map(|raw_tp| replacements.get(&raw_tp.name).copied())
            .collect();
        if type_args.len() == raw_shape.type_params.len() {
            let subst = crate::query_boundaries::common::TypeSubstitution::from_signature_args(
                self.ctx.types,
                &raw_shape.type_params,
                &type_args,
            );
            let instantiated = crate::query_boundaries::common::instantiate_type(
                self.ctx.types,
                annotation_type,
                &subst,
            );
            return Some(self.format_type_for_assignability_message(instantiated));
        }

        None
    }

    pub(in crate::error_reporter::call_errors) fn explicit_type_argument_callback_parameter_display(
        &mut self,
        _param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        let parent_idx = self.ctx.arena.get_extended(arg_idx)?.parent;
        let parent = self.ctx.arena.get(parent_idx)?;
        let (callee_expr, args, type_args): (
            NodeIndex,
            &[NodeIndex],
            &tsz_parser::parser::NodeList,
        ) = match parent.kind {
            k if k == syntax_kind_ext::CALL_EXPRESSION || k == syntax_kind_ext::NEW_EXPRESSION => {
                let call = self.ctx.arena.get_call_expr(parent)?;
                (
                    call.expression,
                    &call.arguments.as_ref()?.nodes,
                    call.type_arguments.as_ref()?,
                )
            }
            _ => return None,
        };
        if type_args.nodes.is_empty() {
            return None;
        }

        let arg_index = args.iter().position(|&candidate| candidate == arg_idx)?;
        let concrete_callee_type = self.get_type_of_node(callee_expr);
        let raw_callee_type = self
            .resolve_qualified_symbol(callee_expr)
            .or_else(|| self.resolve_identifier_symbol(callee_expr))
            .map(|sym| self.get_type_of_symbol(sym))
            .unwrap_or(concrete_callee_type);
        let raw_sigs = query_common::get_call_signatures(self.ctx.types, raw_callee_type)?;
        let accepts_arg_count = |sig: &&tsz_solver::CallSignature| {
            let required_count = sig.params.iter().filter(|p| !p.optional).count();
            let has_rest = sig.params.iter().any(|p| p.rest);
            if has_rest {
                args.len() >= required_count
            } else {
                args.len() >= required_count && args.len() <= sig.params.len()
            }
        };
        let raw_sig = raw_sigs
            .iter()
            .filter(|sig| sig.type_params.len() == type_args.nodes.len())
            .find(accepts_arg_count)
            .or_else(|| {
                raw_sigs
                    .iter()
                    .find(|sig| sig.type_params.len() == type_args.nodes.len())
            })?;

        let raw_param_type = raw_sig
            .params
            .get(arg_index)
            .map(|param| param.type_id)
            .or_else(|| {
                let last = raw_sig.params.last()?;
                last.rest.then_some(last.type_id)
            })?;
        let mut display = self.format_type_for_assignability_message(raw_param_type);
        for (tp, &arg_type_node) in raw_sig.type_params.iter().zip(type_args.nodes.iter()) {
            let replacement = self.sanitized_type_node_display(arg_type_node)?;
            let tp_name = self.ctx.types.resolve_atom_ref(tp.name);
            display = Self::replace_type_param_name_in_display(&display, &tp_name, &replacement);
        }

        Some(display)
    }

    pub(in crate::error_reporter::call_errors) fn contextual_function_parameter_display_with_annotation_fallback(
        &mut self,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        if !crate::query_boundaries::common::contains_type_by_id(
            self.ctx.types,
            param_type,
            TypeId::ERROR,
        ) {
            return None;
        }

        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(arg_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        let func = self.ctx.arena.get_function(node)?;
        if !matches!(
            node.kind,
            k if k == tsz_parser::parser::syntax_kind_ext::ARROW_FUNCTION
                || k == tsz_parser::parser::syntax_kind_ext::FUNCTION_EXPRESSION
        ) || !crate::query_boundaries::common::is_callable_type(self.ctx.types, param_type)
        {
            return None;
        }

        let expected = self.evaluate_application_type(param_type);
        let expected = self.normalize_contextual_signature_with_env(expected);
        let shape = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            expected,
        )
        .or_else(|| {
            crate::query_boundaries::checkers::call::get_contextual_signature(
                self.ctx.types,
                param_type,
            )
        })?;

        let mut rendered = Vec::with_capacity(shape.params.len());
        for (index, param) in shape.params.iter().enumerate() {
            let name = param
                .name
                .map(|name| self.ctx.types.resolve_atom_ref(name).to_string())
                .unwrap_or_else(|| format!("arg{index}"));
            let mut type_display = self.format_type_for_assignability_message(param.type_id);
            if crate::query_boundaries::common::is_genuine_error_type(self.ctx.types, param.type_id)
                && let Some(&actual_param_idx) = func.parameters.nodes.get(index)
                && let Some(actual_param_node) = self.ctx.arena.get(actual_param_idx)
                && let Some(actual_param) = self.ctx.arena.get_parameter(actual_param_node)
                && actual_param.type_annotation.is_some()
            {
                type_display = self
                    .sanitized_type_node_display(actual_param.type_annotation)
                    .unwrap_or(type_display);
            }
            if param.optional {
                type_display = self.optional_parameter_type_display(type_display, param.type_id);
            }
            rendered.push(format!(
                "{}{}{}: {}",
                if param.rest { "..." } else { "" },
                name,
                if param.optional { "?" } else { "" },
                type_display
            ));
        }

        let mut return_display = self.format_type_for_assignability_message(shape.return_type);
        if crate::query_boundaries::common::is_genuine_error_type(self.ctx.types, shape.return_type)
        {
            return_display = self
                .explicit_callback_return_display_from_parameter(func)
                .unwrap_or(return_display);
        }

        let type_param_prefix = if shape.type_params.is_empty() {
            String::new()
        } else {
            let names = shape
                .type_params
                .iter()
                .map(|tp| self.ctx.types.resolve_atom_ref(tp.name).to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("<{names}>")
        };

        Some(format!(
            "{}({}) => {}",
            type_param_prefix,
            rendered.join(", "),
            return_display
        ))
    }

    /// The structural display for the type substituted into a naked
    /// type-parameter call-parameter surface when that substituted type is the
    /// canonical primitive key union `string | number | symbol` **and** the
    /// type parameter's constraint was written structurally (`keyof any`,
    /// `keyof unknown`, `keyof never`, or the longhand `string | number |
    /// symbol`).
    ///
    /// tsz interns one `TypeId` for every spelling of the key union and, via
    /// the reverse type-to-def lookup, repaints it with whatever coincidentally
    /// shaped alias is in scope (the lib `PropertyKey`, a user alias). That
    /// reverse lookup already yields the correct name for a constraint written
    /// *as* an alias (`PropertyKey`, `type Zed = …`), so those are left
    /// untouched — this only intercepts the two structural spellings, which
    /// `tsc` prints by their members. The written spelling is read from the
    /// callee's type-parameter constraint node in the AST because every
    /// spelling collapses to the same interned `TypeId` (the type level cannot
    /// tell them apart). This is the TS2345 call-argument counterpart of the
    /// TS2344 explicit-type-argument recovery.
    fn key_union_type_param_replacement_display(
        &mut self,
        callee_expr: NodeIndex,
        tp_name: tsz_common::interner::Atom,
        replacement_type: TypeId,
    ) -> Option<String> {
        let evaluated_replacement = self.evaluate_type_for_assignability(replacement_type);
        if !self.is_primitive_key_union_type(evaluated_replacement) {
            return None;
        }
        let constraint_idx = self.callee_type_parameter_constraint_node(callee_expr, tp_name)?;
        let structural =
            Self::annotation_is_keyof_over_degenerate_operand(self.ctx.arena, constraint_idx)
                || Self::annotation_is_longhand_primitive_keyword_union(
                    self.ctx.arena,
                    constraint_idx,
                );
        structural.then(|| self.format_type_diagnostic_constraint(evaluated_replacement))
    }

    /// The AST constraint node (`extends …`) of the callee's type parameter
    /// named `tp_name`, when the callee resolves to a function/method-like
    /// declaration whose type-parameter list declares it. Covers a plain
    /// function (`f`), a method (`obj.m`, resolved through the qualified-symbol
    /// path), and a value whose declared type is a function/constructor type
    /// (`declare const f: <K extends …>(…) => …`, whose variable declaration's
    /// type annotation carries the parameters). Returns `None` for an
    /// unconstrained parameter or a callee with no resolvable declaration — the
    /// ordinary display path then stands.
    fn callee_type_parameter_constraint_node(
        &self,
        callee_expr: NodeIndex,
        tp_name: tsz_common::interner::Atom,
    ) -> Option<NodeIndex> {
        let sym_id = self
            .resolve_qualified_symbol(callee_expr)
            .or_else(|| self.resolve_identifier_symbol(callee_expr))?;
        let symbol = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .or_else(|| self.get_cross_file_symbol(sym_id))?;
        let wanted = self.ctx.types.resolve_atom_ref(tp_name);
        for &decl_idx in &symbol.declarations {
            let Some(type_parameters) = self.node_type_parameters(decl_idx) else {
                continue;
            };
            for &tp_idx in &type_parameters.nodes {
                let Some(tp_node) = self.ctx.arena.get(tp_idx) else {
                    continue;
                };
                let Some(tp_data) = self.ctx.arena.get_type_parameter(tp_node) else {
                    continue;
                };
                if tp_data.constraint.is_none() {
                    continue;
                }
                if self
                    .ctx
                    .arena
                    .get(tp_data.name)
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .is_some_and(|ident| ident.escaped_text == *wanted)
                {
                    return Some(tp_data.constraint);
                }
            }
        }
        None
    }

    /// The type-parameter list declared directly on a function/method-like
    /// declaration node, across the node kinds that can own one. Returns `None`
    /// for any other node kind (the callee is then displayed the ordinary way).
    fn node_type_parameters(&self, node_idx: NodeIndex) -> Option<tsz_parser::parser::NodeList> {
        let node = self.ctx.arena.get(node_idx)?;
        match node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION =>
            {
                self.ctx.arena.get_function(node)?.type_parameters.clone()
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_method_decl(node)?
                .type_parameters
                .clone(),
            k if k == syntax_kind_ext::METHOD_SIGNATURE
                || k == syntax_kind_ext::CALL_SIGNATURE
                || k == syntax_kind_ext::CONSTRUCT_SIGNATURE =>
            {
                self.ctx.arena.get_signature(node)?.type_parameters.clone()
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                self.ctx
                    .arena
                    .get_function_type(node)?
                    .type_parameters
                    .clone()
            }
            // `declare const f: <K extends …>(…) => …`: the type parameters live
            // on the variable's function/constructor-type annotation.
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                // A null `type_annotation` resolves to `None` one frame down
                // (`arena.get` returns `None` for a null `NodeIndex`).
                let annotation = self
                    .ctx
                    .arena
                    .get_variable_declaration(node)?
                    .type_annotation;
                self.node_type_parameters(annotation)
            }
            _ => None,
        }
    }

    pub(in crate::error_reporter::call_errors) fn generic_call_parameter_alias_display(
        &mut self,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        let parent_idx = self.ctx.arena.get_extended(arg_idx)?.parent;
        let parent = self.ctx.arena.get(parent_idx)?;
        let (callee_expr, args, has_explicit_type_args): (NodeIndex, &[NodeIndex], bool) =
            match parent.kind {
                k if k == syntax_kind_ext::CALL_EXPRESSION
                    || k == syntax_kind_ext::NEW_EXPRESSION =>
                {
                    let call = self.ctx.arena.get_call_expr(parent)?;
                    let args = call.arguments.as_ref()?;
                    let has_explicit_type_args = call
                        .type_arguments
                        .as_ref()
                        .is_some_and(|type_args| !type_args.nodes.is_empty());
                    (call.expression, &args.nodes, has_explicit_type_args)
                }
                _ => return None,
            };

        let arg_index = args.iter().position(|&candidate| candidate == arg_idx)?;
        let callee_type = self.get_type_of_node(callee_expr);
        let raw_sig = crate::query_boundaries::checkers::call::get_contextual_signature_for_arity(
            self.ctx.types,
            callee_type,
            args.len(),
        )?;
        let raw_param_type = raw_sig
            .params
            .get(arg_index)
            .map(|param| param.type_id)
            .or_else(|| {
                let last = raw_sig.params.last()?;
                last.rest.then_some(last.type_id)
            })?;

        if !crate::query_boundaries::common::contains_type_parameters(
            self.ctx.types,
            raw_param_type,
        ) {
            return None;
        }

        // Direct arguments later in the same generic call can fix a callback's
        // return type more specifically than the instantiated parameter surface.
        // Only when the call actually infers that type parameter: an explicit
        // type-argument list fixes every type parameter before inference, and
        // tsc renders the fixed instantiation, not the later argument's literal.
        let callback_return_tp = if has_explicit_type_args {
            None
        } else {
            self.raw_callback_return_type_param_name(raw_param_type)
        };
        let mut replacements = FxHashMap::default();
        self.collect_type_param_display_replacements(raw_param_type, param_type, &mut replacements);

        if !replacements.is_empty() {
            let mut display = self.format_type_for_assignability_message(raw_param_type);
            let raw_display = display.clone();
            let mut replaced_any = false;
            for tp in &raw_sig.type_params {
                let Some(&replacement_type) = replacements.get(&tp.name) else {
                    continue;
                };
                let replacement_type = if callback_return_tp == Some(tp.name) {
                    self.later_literal_argument_replacement_for_type_param(
                        &raw_sig, args, arg_index, tp.name,
                    )
                    .unwrap_or(replacement_type)
                } else {
                    replacement_type
                };
                let replacement = self
                    .key_union_type_param_replacement_display(
                        callee_expr,
                        tp.name,
                        replacement_type,
                    )
                    .unwrap_or_else(|| {
                        self.format_type_for_assignability_message(replacement_type)
                    });
                let tp_name = self.ctx.types.resolve_atom_ref(tp.name);
                display =
                    Self::replace_type_param_name_in_display(&display, &tp_name, &replacement);
                replaced_any = true;
            }
            if replaced_any
                && display != raw_display
                && (display.contains('<') || !raw_display.contains('<'))
            {
                return Some(
                    Self::widen_member_literals_in_display_text(&display)
                        .replace("new(", "new (")
                        .replace("?: unknown | undefined", "?: unknown"),
                );
            }
        }

        let raw_display = self.format_type_for_assignability_message(raw_param_type);
        let raw_shape = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            raw_param_type,
        )?;
        let concrete_shape = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            param_type,
        )?;

        let mut display = raw_display;
        let original_display = display.clone();
        let mut replaced_any = false;
        for tp in &raw_sig.type_params {
            let tp_name = self.ctx.types.resolve_atom_ref(tp.name);
            let replacement = replacements
                .get(&tp.name)
                .map(|&replacement_type| {
                    let widened = crate::query_boundaries::common::widen_type(
                        self.ctx.types,
                        replacement_type,
                    );
                    self.format_type_for_assignability_message(widened)
                })
                .or_else(|| {
                    raw_shape
                        .params
                        .iter()
                        .zip(concrete_shape.params.iter())
                        .find_map(|(raw_param, concrete_param)| {
                            let raw_tp = crate::query_boundaries::common::type_param_info(
                                self.ctx.types.as_type_database(),
                                raw_param.type_id,
                            )?;
                            (raw_tp.name == tp.name).then(|| {
                                let widened = crate::query_boundaries::common::widen_type(
                                    self.ctx.types,
                                    concrete_param.type_id,
                                );
                                self.format_type_for_assignability_message(widened)
                            })
                        })
                })
                .or_else(|| {
                    let raw_tp = crate::query_boundaries::common::type_param_info(
                        self.ctx.types.as_type_database(),
                        raw_shape.return_type,
                    )?;
                    (raw_tp.name == tp.name).then(|| {
                        let widened = crate::query_boundaries::common::widen_type(
                            self.ctx.types,
                            concrete_shape.return_type,
                        );
                        self.format_type_for_assignability_message(widened)
                    })
                })?;

            display = Self::replace_type_param_name_in_display(&display, &tp_name, &replacement);
            replaced_any = true;
        }

        (replaced_any
            && display != original_display
            && (display.contains('<') || !original_display.contains('<')))
        .then_some(display)
    }
}
