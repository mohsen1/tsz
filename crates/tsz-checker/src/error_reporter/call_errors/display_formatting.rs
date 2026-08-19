//! Type display formatting helpers for call error diagnostics.

use crate::context::TypingRequest;
use crate::query_boundaries::assignability::{
    get_function_return_type, replace_function_return_type,
};
use crate::query_boundaries::{common as query_common, diagnostics as query_diagnostics};
use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn strip_synthetic_optional_from_display(display: String) -> String {
        if let Some(stripped) = display.strip_suffix(" | undefined")
            && stripped != "null"
        {
            stripped.to_string()
        } else {
            display
        }
    }

    pub(in crate::error_reporter::call_errors) fn strip_synthetic_optional_from_display_for_arg(
        &self,
        display: String,
        arg_type: TypeId,
    ) -> String {
        if arg_type == TypeId::NULL
            || query_common::union_list_id(self.ctx.types, arg_type)
                .is_some_and(|list_id| self.ctx.types.type_list(list_id).contains(&TypeId::NULL))
        {
            display
        } else {
            Self::strip_synthetic_optional_from_display(display)
        }
    }

    pub(in crate::error_reporter::call_errors) fn optional_parameter_type_display(
        &self,
        display: String,
        type_id: TypeId,
    ) -> String {
        if query_common::type_contains_undefined(self.ctx.types, type_id) {
            display
        } else {
            format!("{display} | undefined")
        }
    }

    pub(crate) fn sanitized_type_node_display(&mut self, type_node: NodeIndex) -> Option<String> {
        self.node_text(type_node)
            .and_then(|text| self.sanitize_type_annotation_text_for_diagnostic(text, true))
            .map(|text| self.format_annotation_like_type(&text))
    }

    pub(in crate::error_reporter::call_errors) fn explicit_callback_return_display_from_parameter(
        &mut self,
        func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<String> {
        let body_node = self.ctx.arena.get(func.body)?;
        let return_expr = if body_node.kind == syntax_kind_ext::BLOCK {
            let block = self.ctx.arena.get_block(body_node)?;
            block.statements.nodes.iter().rev().find_map(|&stmt_idx| {
                let stmt = self.ctx.arena.get(stmt_idx)?;
                let ret = self.ctx.arena.get_return_statement(stmt)?;
                ret.expression.into_option()
            })?
        } else {
            func.body
        };

        let return_name = self.ctx.arena.get_identifier_text(return_expr)?;
        func.parameters.nodes.iter().find_map(|&param_idx| {
            let param_node = self.ctx.arena.get(param_idx)?;
            let param = self.ctx.arena.get_parameter(param_node)?;
            let param_name = self.ctx.arena.get_identifier_text(param.name)?;
            if param_name != return_name {
                return None;
            }
            if param.type_annotation.is_none() {
                return None;
            }
            let type_node = param.type_annotation;
            self.sanitized_type_node_display(type_node)
        })
    }

    pub(super) fn replace_type_param_name_in_display(
        display: &str,
        param_name: &str,
        replacement: &str,
    ) -> String {
        let chars: Vec<char> = display.chars().collect();
        let needle: Vec<char> = param_name.chars().collect();
        let mut out = String::with_capacity(display.len() + replacement.len());
        let mut i = 0usize;

        while i < chars.len() {
            let matches = i + needle.len() <= chars.len()
                && chars[i..i + needle.len()] == needle[..]
                && (i == 0 || !chars[i - 1].is_alphanumeric() && chars[i - 1] != '_')
                && (i + needle.len() == chars.len()
                    || !chars[i + needle.len()].is_alphanumeric()
                        && chars[i + needle.len()] != '_');

            if matches {
                out.push_str(replacement);
                i += needle.len();
            } else {
                out.push(chars[i]);
                i += 1;
            }
        }

        out
    }

    pub(super) fn collect_type_param_display_replacements(
        &mut self,
        raw_type: TypeId,
        concrete_type: TypeId,
        replacements: &mut FxHashMap<tsz_common::interner::Atom, TypeId>,
    ) {
        self.collect_type_param_display_replacements_inner(
            raw_type,
            concrete_type,
            replacements,
            0,
        );
    }

    fn collect_type_param_display_replacements_inner(
        &mut self,
        raw_type: TypeId,
        concrete_type: TypeId,
        replacements: &mut FxHashMap<tsz_common::interner::Atom, TypeId>,
        depth: usize,
    ) {
        if depth > 32 || raw_type == concrete_type {
            return;
        }

        if let Some(raw_tp) = crate::query_boundaries::common::type_param_info(
            self.ctx.types.as_type_database(),
            raw_type,
        ) {
            replacements.entry(raw_tp.name).or_insert_with(|| {
                if crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    concrete_type,
                ) {
                    crate::query_boundaries::common::widen_type(self.ctx.types, concrete_type)
                } else if crate::query_boundaries::common::object_shape_for_type(
                    self.ctx.types,
                    concrete_type,
                )
                .is_some()
                {
                    self.widen_object_property_literals_for_call_parameter_display(concrete_type)
                } else {
                    concrete_type
                }
            });
            return;
        }

        let raw_unwrapped = query_common::unwrap_readonly(self.ctx.types, raw_type);
        let concrete_unwrapped = query_common::unwrap_readonly(self.ctx.types, concrete_type);
        if raw_unwrapped != raw_type || concrete_unwrapped != concrete_type {
            self.collect_type_param_display_replacements_inner(
                raw_unwrapped,
                concrete_unwrapped,
                replacements,
                depth + 1,
            );
            return;
        }

        if let (Some(raw_elem), Some(concrete_elem)) = (
            query_common::array_element_type(self.ctx.types, raw_type),
            query_common::array_element_type(self.ctx.types, concrete_type),
        ) {
            self.collect_type_param_display_replacements_inner(
                raw_elem,
                concrete_elem,
                replacements,
                depth + 1,
            );
            return;
        }

        if let (Some(raw_shape), Some(concrete_shape)) = (
            query_common::object_shape_for_type(self.ctx.types, raw_type),
            query_common::object_shape_for_type(self.ctx.types, concrete_type),
        ) {
            for raw_prop in &raw_shape.properties {
                if let Some(concrete_prop) = concrete_shape
                    .properties
                    .iter()
                    .find(|prop| prop.name == raw_prop.name)
                {
                    self.collect_type_param_display_replacements_inner(
                        raw_prop.type_id,
                        concrete_prop.type_id,
                        replacements,
                        depth + 1,
                    );
                }
            }
            if let (Some(raw_index), Some(concrete_index)) =
                (&raw_shape.string_index, &concrete_shape.string_index)
            {
                self.collect_type_param_display_replacements_inner(
                    raw_index.value_type,
                    concrete_index.value_type,
                    replacements,
                    depth + 1,
                );
            }
            if let (Some(raw_index), Some(concrete_index)) =
                (&raw_shape.number_index, &concrete_shape.number_index)
            {
                self.collect_type_param_display_replacements_inner(
                    raw_index.value_type,
                    concrete_index.value_type,
                    replacements,
                    depth + 1,
                );
            }
        }

        if let (Some(raw_shape), Some(concrete_shape)) = (
            query_common::callable_shape_for_type(self.ctx.types, raw_type),
            query_common::callable_shape_for_type(self.ctx.types, concrete_type),
        ) {
            for (raw_sig, concrete_sig) in raw_shape
                .call_signatures
                .iter()
                .zip(concrete_shape.call_signatures.iter())
            {
                for (raw_param, concrete_param) in
                    raw_sig.params.iter().zip(concrete_sig.params.iter())
                {
                    self.collect_type_param_display_replacements_inner(
                        raw_param.type_id,
                        concrete_param.type_id,
                        replacements,
                        depth + 1,
                    );
                }
                self.collect_type_param_display_replacements_inner(
                    raw_sig.return_type,
                    concrete_sig.return_type,
                    replacements,
                    depth + 1,
                );
            }
            for (raw_sig, concrete_sig) in raw_shape
                .construct_signatures
                .iter()
                .zip(concrete_shape.construct_signatures.iter())
            {
                for (raw_param, concrete_param) in
                    raw_sig.params.iter().zip(concrete_sig.params.iter())
                {
                    self.collect_type_param_display_replacements_inner(
                        raw_param.type_id,
                        concrete_param.type_id,
                        replacements,
                        depth + 1,
                    );
                }
                self.collect_type_param_display_replacements_inner(
                    raw_sig.return_type,
                    concrete_sig.return_type,
                    replacements,
                    depth + 1,
                );
            }
        }

        if let (Some(raw_shape), Some(concrete_shape)) = (
            query_common::function_shape_for_type(self.ctx.types, raw_type),
            query_common::function_shape_for_type(self.ctx.types, concrete_type),
        ) {
            for (raw_param, concrete_param) in
                raw_shape.params.iter().zip(concrete_shape.params.iter())
            {
                self.collect_type_param_display_replacements_inner(
                    raw_param.type_id,
                    concrete_param.type_id,
                    replacements,
                    depth + 1,
                );
            }
            self.collect_type_param_display_replacements_inner(
                raw_shape.return_type,
                concrete_shape.return_type,
                replacements,
                depth + 1,
            );
        }

        if let (Some(raw_app), Some(concrete_app)) = (
            query_common::type_application(self.ctx.types, raw_type),
            query_common::type_application(self.ctx.types, concrete_type),
        ) && raw_app.base == concrete_app.base
            && raw_app.args.len() == concrete_app.args.len()
        {
            for (&raw_arg, &concrete_arg) in raw_app.args.iter().zip(concrete_app.args.iter()) {
                self.collect_type_param_display_replacements_inner(
                    raw_arg,
                    concrete_arg,
                    replacements,
                    depth + 1,
                );
            }
        }
    }

    pub(super) fn raw_callback_return_type_param_name(
        &self,
        raw_param_type: TypeId,
    ) -> Option<tsz_common::interner::Atom> {
        let shape = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            raw_param_type,
        )?;
        crate::query_boundaries::common::type_param_info(
            self.ctx.types.as_type_database(),
            shape.return_type,
        )
        .map(|tp| tp.name)
    }

    pub(super) fn later_literal_argument_replacement_for_type_param(
        &mut self,
        raw_sig: &tsz_solver::FunctionShape,
        args: &[NodeIndex],
        arg_index: usize,
        type_param_name: tsz_common::interner::Atom,
    ) -> Option<TypeId> {
        raw_sig
            .params
            .iter()
            .zip(args.iter().copied())
            .enumerate()
            .skip(arg_index + 1)
            .find_map(|(_, (raw_param, call_arg_idx))| {
                let raw_tp = crate::query_boundaries::common::type_param_info(
                    self.ctx.types.as_type_database(),
                    raw_param.type_id,
                )?;
                (raw_tp.name == type_param_name)
                    .then(|| self.literal_type_from_initializer(call_arg_idx))
                    .flatten()
            })
    }

    /// The check-time (unwidened) parameter type that the TS2345 head display
    /// restores for a generic call argument, when it differs from the
    /// relation's final (widened) instantiated parameter.
    ///
    /// `generic_call_parameter_alias_display` renders the head against the raw
    /// signature with the callback's return-position type parameter replaced by
    /// a LATER literal argument's type: `m(1, function (a) { return '' }, 1)`
    /// against `m<T, U>(x: T, cb: (a: T) => U, y: U)` displays the parameter as
    /// `(a: number) => 1` even though the stored instantiation widened `U` to
    /// `number`. tsc runs the whole argument relation against that unwidened
    /// check-time instantiation, so the nested elaboration must derive from the
    /// same pair the head shows (issue #17686). This rebuilds that restored
    /// parameter at the type level — same raw signature, same replacement map,
    /// instantiated instead of string-substituted — for the emission sink to
    /// re-derive the failure reason against. `None` when no later-literal
    /// restore applies (the displayed and related parameters already agree).
    pub(crate) fn later_literal_restored_param_type_for_argument(
        &mut self,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> Option<TypeId> {
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
        // An explicit type-argument list fixes every type parameter before
        // inference, so the head display (`generic_call_parameter_alias_display`)
        // renders the fixed instantiation, not the later argument's literal
        // (#17745). Mirror that gate here so the re-derived elaboration never
        // restores a literal the head does not.
        if has_explicit_type_args {
            return None;
        }
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
        if !query_common::contains_type_parameters(self.ctx.types, raw_param_type) {
            return None;
        }
        let callback_return_tp = self.raw_callback_return_type_param_name(raw_param_type)?;
        let restored = self.later_literal_argument_replacement_for_type_param(
            &raw_sig,
            args,
            arg_index,
            callback_return_tp,
        )?;
        let mut replacements = FxHashMap::default();
        self.collect_type_param_display_replacements(raw_param_type, param_type, &mut replacements);
        replacements.insert(callback_return_tp, restored);
        let mut subst =
            crate::query_boundaries::generic_instantiation::signature_domain_substitution(
                &raw_sig.type_params,
            );
        for tp in &raw_sig.type_params {
            if let Some(&replacement) = replacements.get(&tp.name) {
                subst.insert(tp.name, replacement);
            }
        }
        let instantiated = query_common::instantiate_type(self.ctx.types, raw_param_type, &subst);
        // A leftover free type parameter (no replacement recovered for it)
        // would turn the re-derived relation into a bare-type-parameter check;
        // stand down and keep the original reason pair.
        if query_common::contains_type_parameters(self.ctx.types, instantiated) {
            return None;
        }
        (instantiated != param_type).then_some(instantiated)
    }

    pub(in crate::error_reporter::call_errors) fn instantiated_call_parameter_display(
        &mut self,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        let parent_idx = self.ctx.arena.get_extended(arg_idx)?.parent;
        let parent = self.ctx.arena.get(parent_idx)?;
        let call = match parent.kind {
            k if k == syntax_kind_ext::CALL_EXPRESSION || k == syntax_kind_ext::NEW_EXPRESSION => {
                self.ctx.arena.get_call_expr(parent)?
            }
            _ => return None,
        };
        let args = call.arguments.as_ref()?;
        let arg_index = args
            .nodes
            .iter()
            .position(|&candidate| candidate == arg_idx)?;

        let raw_callee_type = self
            .resolve_qualified_symbol(call.expression)
            .or_else(|| self.resolve_identifier_symbol(call.expression))
            .map(|sym| self.get_type_of_symbol(sym))
            .unwrap_or_else(|| self.get_type_of_node(call.expression));
        let raw_sig = crate::query_boundaries::checkers::call::get_call_signature(
            self.ctx.types,
            raw_callee_type,
            args.nodes.len(),
        )?;
        if raw_sig.type_params.is_empty() {
            return None;
        }

        let (raw_param_type, param_is_rest) = raw_sig
            .params
            .get(arg_index)
            .map(|param| (param.type_id, param.rest))
            .or_else(|| {
                let last = raw_sig.params.last()?;
                last.rest.then_some((last.type_id, true))
            })?;
        if param_is_rest && self.rest_tuple_parameter_reports_per_position(raw_param_type) {
            return None;
        }

        let mut replacements = FxHashMap::default();
        for (raw_param, &call_arg_idx) in raw_sig.params.iter().zip(args.nodes.iter()) {
            let actual_arg_type = self.elaboration_display_type_of(call_arg_idx);
            self.collect_type_param_display_replacements(
                raw_param.type_id,
                actual_arg_type,
                &mut replacements,
            );
        }

        let mut type_args = Vec::with_capacity(raw_sig.type_params.len());
        for tp in &raw_sig.type_params {
            if let Some(&replacement) = replacements.get(&tp.name) {
                type_args.push(replacement);
            } else {
                let constraint = tp.constraint?;
                type_args.push(self.evaluate_type_for_assignability(constraint));
            }
        }

        let subst = crate::query_boundaries::common::TypeSubstitution::from_signature_args(
            self.ctx.types,
            &raw_sig.type_params,
            &type_args,
        );
        let instantiated = crate::query_boundaries::common::instantiate_type(
            self.ctx.types,
            raw_param_type,
            &subst,
        );
        let evaluated = self.evaluate_type_for_assignability(instantiated);
        let display_type = if evaluated != TypeId::ERROR {
            evaluated
        } else {
            instantiated
        };
        if matches!(display_type, TypeId::ANY | TypeId::UNKNOWN) {
            return None;
        }
        if crate::query_boundaries::common::contains_type_parameters(self.ctx.types, display_type) {
            let mut display = self.format_type_for_assignability_message(raw_param_type);
            let mut replaced_any = false;
            for (tp, &type_arg) in raw_sig.type_params.iter().zip(type_args.iter()) {
                let replacement = self.format_type_for_assignability_message(type_arg);
                let tp_name = self.ctx.types.resolve_atom_ref(tp.name);
                display =
                    Self::replace_type_param_name_in_display(&display, &tp_name, &replacement);
                replaced_any = true;
            }
            return replaced_any.then(|| {
                Self::widen_member_literals_in_display_text(&display)
                    .replace("new(", "new (")
                    .replace("?: unknown | undefined", "?: unknown")
            });
        }

        let display_type =
            if crate::query_boundaries::common::object_shape_for_type(self.ctx.types, display_type)
                .is_some()
            {
                self.widen_object_property_literals_for_call_parameter_display(display_type)
            } else {
                display_type
            };

        let display = if query_common::is_callable_type(self.ctx.types, display_type) {
            self.format_type_for_assignability_message(display_type)
        } else {
            let widened = self.widen_annotation_literals_for_display(
                display_type,
                crate::query_boundaries::diagnostics::AnnotationLiteralWideningPolicy::ALL,
            );
            if widened.display_residue {
                self.format_type_diagnostic_widened(widened.type_id)
            } else {
                self.format_type_for_assignability_message(widened.type_id)
            }
        };
        Some(
            display
                .replace("new(", "new (")
                .replace("?: unknown | undefined", "?: unknown"),
        )
    }

    fn widen_object_property_literals_for_call_parameter_display(&mut self, ty: TypeId) -> TypeId {
        crate::query_boundaries::diagnostics::shallow_object_property_literals_widened_for_call_parameter_display(
            self.ctx.types,
            ty,
        )
    }

    pub(in crate::error_reporter::call_errors) fn property_access_call_parameter_annotation_display(
        &mut self,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        let parent_idx = self.ctx.arena.get_extended(arg_idx)?.parent;
        let parent = self.ctx.arena.get(parent_idx)?;
        let call = match parent.kind {
            k if k == syntax_kind_ext::CALL_EXPRESSION || k == syntax_kind_ext::NEW_EXPRESSION => {
                self.ctx.arena.get_call_expr(parent)?
            }
            _ => return None,
        };
        let callee_node = self.ctx.arena.get(call.expression)?;
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }

        let access = self.ctx.arena.get_access_expr(callee_node)?;
        let member_name = self
            .ctx
            .arena
            .get_identifier_at(access.name_or_argument)?
            .escaped_text
            .clone();
        let arg_index = call
            .arguments
            .as_ref()?
            .nodes
            .iter()
            .position(|&n| n == arg_idx)?;

        // When explicit type arguments are provided (e.g., `_.map<number, string, Date>(...)`),
        // the annotation-based display cannot fully substitute type parameters that are only
        // determined by the explicit type args (not inferable from call arguments). For example,
        // in `map<T, U, V>(c: Collection<T, U>, f: (x: T, y: U) => V)`, V is only known from
        // the explicit type arg `Date`, not from any call argument. Returning None lets the
        // general type display format the correctly instantiated param_type TypeId directly.
        if call
            .type_arguments
            .as_ref()
            .is_some_and(|ta| !ta.nodes.is_empty())
        {
            return None;
        }

        let raw_callee_type = self
            .resolve_qualified_symbol(call.expression)
            .or_else(|| self.resolve_identifier_symbol(call.expression))
            .map(|sym| self.get_type_of_symbol(sym))
            .unwrap_or_else(|| self.get_type_of_node(call.expression));
        let raw_shape = crate::query_boundaries::checkers::call::get_call_signature(
            self.ctx.types,
            raw_callee_type,
            call.arguments.as_ref()?.nodes.len(),
        )?;
        crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            param_type,
        )?;

        let mut declaration_owners = Vec::new();

        if let Some(base_sym) = self.resolve_identifier_symbol(access.expression)
            && let Some(base_symbol) = self.ctx.binder.get_symbol(base_sym)
            && let base_decl = base_symbol.value_declaration
            && let Some(base_decl_node) = self.ctx.arena.get(base_decl)
            && let Some(var_decl) = self.ctx.arena.get_variable_declaration(base_decl_node)
            && var_decl.type_annotation.is_some()
            && let Some(type_node) = self.ctx.arena.get(var_decl.type_annotation)
        {
            let type_sym = if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                self.ctx.arena.get_type_ref(type_node).and_then(|type_ref| {
                    match self.resolve_qualified_symbol_in_type_position(type_ref.type_name) {
                        TypeSymbolResolution::Type(sym_id)
                        | TypeSymbolResolution::ValueOnly(sym_id) => Some(sym_id),
                        TypeSymbolResolution::NotFound => None,
                    }
                })
            } else {
                None
            };
            if let Some(type_sym) = type_sym {
                declaration_owners.push(type_sym);
            }

            let annotated_base_type = self.get_type_from_type_node(var_decl.type_annotation);
            if let Some(type_sym) = crate::query_boundaries::common::type_shape_symbol(
                self.ctx.types,
                annotated_base_type,
            )
            .or_else(|| {
                let def_id = crate::query_boundaries::common::lazy_def_id(
                    self.ctx.types,
                    annotated_base_type,
                )?;
                self.ctx.def_to_symbol_id_with_fallback(def_id)
            }) && !declaration_owners.contains(&type_sym)
            {
                declaration_owners.push(type_sym);
            }
        }

        let base_type = self.get_type_of_node(access.expression);
        if let Some(base_sym) = crate::query_boundaries::common::type_shape_symbol(
            self.ctx.types,
            base_type,
        )
        .or_else(|| {
            let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, base_type)?;
            self.ctx.def_to_symbol_id_with_fallback(def_id)
        }) && !declaration_owners.contains(&base_sym)
        {
            declaration_owners.push(base_sym);
        }

        for owner_sym in declaration_owners {
            let Some(base_symbol) = self.ctx.binder.get_symbol(owner_sym) else {
                continue;
            };
            for &decl_idx in &base_symbol.declarations {
                let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                let member_lists: Option<&tsz_parser::parser::NodeList> =
                    if decl_node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                        self.ctx
                            .arena
                            .get_interface(decl_node)
                            .map(|iface| &iface.members)
                    } else if decl_node.kind == syntax_kind_ext::CLASS_DECLARATION
                        || decl_node.kind == syntax_kind_ext::CLASS_EXPRESSION
                    {
                        self.ctx
                            .arena
                            .get_class(decl_node)
                            .map(|class| &class.members)
                    } else {
                        None
                    };
                let Some(member_lists) = member_lists else {
                    continue;
                };

                for &member_idx in &member_lists.nodes {
                    let Some(member_node) = self.ctx.arena.get(member_idx) else {
                        continue;
                    };
                    if member_node.kind != syntax_kind_ext::METHOD_SIGNATURE
                        && member_node.kind != syntax_kind_ext::METHOD_DECLARATION
                    {
                        continue;
                    }
                    let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                        if member_node.kind == syntax_kind_ext::METHOD_SIGNATURE {
                            let Some(method) = self.ctx.arena.get_signature(member_node) else {
                                continue;
                            };
                            let Some(name) = self.get_property_name(method.name) else {
                                continue;
                            };
                            if name != member_name {
                                continue;
                            }
                            let Some(parameters) = method.parameters.as_ref() else {
                                continue;
                            };
                            let Some(param_idx) = parameters.nodes.get(arg_index).copied() else {
                                continue;
                            };
                            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                                continue;
                            };
                            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                                continue;
                            };
                            if param.type_annotation.is_none() {
                                continue;
                            }

                            if let Some(display) = self
                                .instantiated_property_access_annotation_display(
                                    &raw_shape,
                                    &call.arguments.as_ref()?.nodes,
                                    param.type_annotation,
                                )
                            {
                                return Some(display);
                            }
                            continue;
                        } else {
                            continue;
                        }
                    };
                    let Some(name) = self.get_property_name(method.name) else {
                        continue;
                    };
                    if name != member_name {
                        continue;
                    }
                    let Some(param_idx) = method.parameters.nodes.get(arg_index).copied() else {
                        continue;
                    };
                    let Some(param_node) = self.ctx.arena.get(param_idx) else {
                        continue;
                    };
                    let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                        continue;
                    };
                    if param.type_annotation.is_none() {
                        continue;
                    }

                    if let Some(display) = self.instantiated_property_access_annotation_display(
                        &raw_shape,
                        &call.arguments.as_ref()?.nodes,
                        param.type_annotation,
                    ) {
                        return Some(display);
                    }
                }
            }
        }

        None
    }

    pub(in crate::error_reporter::call_errors) fn widen_function_like_call_source(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
        let type_id = self.evaluate_type_with_env(type_id);
        let type_id = self.resolve_type_for_property_access(type_id);
        let type_id = self.resolve_lazy_type(type_id);
        let type_id = self.evaluate_application_type(type_id);

        let widened = crate::query_boundaries::common::widen_type(self.ctx.types, type_id);
        if widened != type_id {
            return widened;
        }

        if let Some(return_type) = get_function_return_type(self.ctx.types, type_id) {
            let widened_return =
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, return_type);
            if widened_return != return_type {
                let replaced =
                    replace_function_return_type(self.ctx.types, type_id, widened_return);
                if replaced != type_id {
                    return replaced;
                }
            }
        }

        type_id
    }

    fn should_prefer_property_target_type(
        &self,
        current: Option<TypeId>,
        candidate: TypeId,
    ) -> bool {
        if matches!(candidate, TypeId::ERROR | TypeId::ANY) {
            return false;
        }

        let Some(current) = current else {
            return true;
        };

        if matches!(current, TypeId::ERROR | TypeId::ANY | TypeId::UNKNOWN) {
            return true;
        }

        let current_has_type_params =
            crate::query_boundaries::common::contains_type_parameters(self.ctx.types, current);
        let candidate_has_type_params =
            crate::query_boundaries::common::contains_type_parameters(self.ctx.types, candidate);

        current_has_type_params && !candidate_has_type_params
    }

    pub(in crate::error_reporter) fn elaboration_source_expression_type(
        &mut self,
        expr_idx: NodeIndex,
    ) -> TypeId {
        let snap = crate::context::speculation::DiagnosticSpeculationSnapshot::new(&self.ctx);

        let ty = self.compute_type_of_node_with_request(expr_idx, &TypingRequest::NONE);

        snap.rollback(&mut self.ctx.diagnostic_state());
        ty
    }

    /// Like [`Self::elaboration_source_expression_type`], but typed under a
    /// contextual expected type. Elementwise elaboration needs this: a nested
    /// array literal re-typed with no context degrades to `(A | B)[]` and
    /// spuriously fails against a tuple target it actually satisfies.
    pub(in crate::error_reporter) fn elaboration_source_expression_type_with_context(
        &mut self,
        expr_idx: NodeIndex,
        expected: TypeId,
    ) -> TypeId {
        let snap = crate::context::speculation::DiagnosticSpeculationSnapshot::new(&self.ctx);

        let ty = self.compute_type_of_node_with_request(
            expr_idx,
            &TypingRequest::with_contextual_type(expected),
        );

        snap.rollback(&mut self.ctx.diagnostic_state());
        ty
    }

    /// The display type of a call argument in elaboration paths: its own
    /// fresh literal type when the argument is a literal (tsc renders the
    /// unwidened checked type in diagnostics), otherwise the elaboration
    /// source expression type.
    pub(in crate::error_reporter) fn elaboration_display_type_of(
        &mut self,
        expr_idx: NodeIndex,
    ) -> TypeId {
        self.literal_type_from_initializer(expr_idx)
            .unwrap_or_else(|| self.elaboration_source_expression_type(expr_idx))
    }

    fn finite_mapped_target_property_type(
        &mut self,
        target_type: TypeId,
        prop_name: &str,
    ) -> Option<TypeId> {
        if let Some(members) = query_common::union_members(self.ctx.types, target_type) {
            let property_types: Vec<TypeId> = members
                .iter()
                .copied()
                .filter(|&member| member != TypeId::NULL && member != TypeId::UNDEFINED)
                .filter_map(|member| self.finite_mapped_target_property_type(member, prop_name))
                .collect();
            return match property_types.as_slice() {
                [] => None,
                [single] => Some(*single),
                _ => Some(query_diagnostics::display_union_type(
                    self.ctx.types,
                    property_types,
                )),
            };
        }

        if let Some(mapped_id) = query_common::mapped_type_id(self.ctx.types, target_type) {
            return crate::query_boundaries::state::checking::get_finite_mapped_property_type(
                self.ctx.types,
                mapped_id,
                prop_name,
            );
        }

        let (base, args) = query_common::application_info(self.ctx.types, target_type)?;
        let sym_id = self.ctx.resolve_type_to_symbol_id(base)?;
        let (body_type, type_params) = self.type_reference_symbol_type_with_params(sym_id);
        let mapped_id = query_common::mapped_type_id(self.ctx.types, body_type)?;
        let substitution =
            query_common::TypeSubstitution::from_args(self.ctx.types, &type_params, &args);
        let instantiated = query_common::instantiate_type(self.ctx.types, body_type, &substitution);
        let instantiated_mapped_id =
            query_common::mapped_type_id(self.ctx.types, instantiated).unwrap_or(mapped_id);

        crate::query_boundaries::state::checking::get_finite_mapped_property_type(
            self.ctx.types,
            instantiated_mapped_id,
            prop_name,
        )
    }

    fn finite_mapped_target_property_display_type(
        &mut self,
        target_type: TypeId,
        prop_name: &str,
    ) -> Option<TypeId> {
        if let Some(members) = query_common::union_members(self.ctx.types, target_type) {
            let property_types: Vec<TypeId> = members
                .iter()
                .copied()
                .filter(|&member| member != TypeId::NULL && member != TypeId::UNDEFINED)
                .filter_map(|member| {
                    self.finite_mapped_target_property_display_type(member, prop_name)
                })
                .collect();
            return match property_types.as_slice() {
                [] => None,
                [single] => Some(*single),
                _ => Some(query_diagnostics::display_union_type(
                    self.ctx.types,
                    property_types,
                )),
            };
        }

        if let Some(mapped_id) = query_common::mapped_type_id(self.ctx.types, target_type) {
            return crate::query_boundaries::state::checking::get_finite_mapped_property_display_type(
                self.ctx.types,
                mapped_id,
                prop_name,
            );
        }

        let (base, args) = query_common::application_info(self.ctx.types, target_type)?;
        let sym_id = self.ctx.resolve_type_to_symbol_id(base)?;
        let (body_type, type_params) = self.type_reference_symbol_type_with_params(sym_id);
        let mapped_id = query_common::mapped_type_id(self.ctx.types, body_type)?;
        let substitution =
            query_common::TypeSubstitution::from_args(self.ctx.types, &type_params, &args);
        let instantiated = query_common::instantiate_type(self.ctx.types, body_type, &substitution);
        let instantiated_mapped_id =
            query_common::mapped_type_id(self.ctx.types, instantiated).unwrap_or(mapped_id);

        crate::query_boundaries::state::checking::get_finite_mapped_property_display_type(
            self.ctx.types,
            instantiated_mapped_id,
            prop_name,
        )
    }

    pub(in crate::error_reporter) fn mapped_target_property_display_type(
        &mut self,
        target_type: TypeId,
        prop_name: &str,
    ) -> Option<TypeId> {
        let mapped =
            if let Some(mapped_id) = query_common::mapped_type_id(self.ctx.types, target_type) {
                self.ctx.types.mapped_type(mapped_id)
            } else {
                let (base, args) = query_common::application_info(self.ctx.types, target_type)?;
                let sym_id = self.ctx.resolve_type_to_symbol_id(base)?;
                let (body_type, type_params) = self.type_reference_symbol_type_with_params(sym_id);
                let mapped_id = query_common::mapped_type_id(self.ctx.types, body_type)?;
                let substitution =
                    query_common::TypeSubstitution::from_args(self.ctx.types, &type_params, &args);
                let instantiated =
                    query_common::instantiate_type(self.ctx.types, body_type, &substitution);
                let instantiated_mapped_id =
                    query_common::mapped_type_id(self.ctx.types, instantiated).unwrap_or(mapped_id);
                self.ctx.types.mapped_type(instantiated_mapped_id)
            };

        if mapped.name_type.is_some()
            && !crate::query_boundaries::state::checking::is_identity_name_mapping(
                self.ctx.types,
                &mapped,
            )
        {
            return None;
        }

        let key_literal = query_diagnostics::display_string_literal_type(self.ctx.types, prop_name);
        let mut display_type =
            crate::query_boundaries::state::checking::instantiate_mapped_template_for_property(
                self.ctx.types,
                mapped.template,
                mapped.type_param.name,
                key_literal,
            );
        let has_optional_property =
            crate::query_boundaries::common::index_access_types(self.ctx.types, display_type)
                .and_then(|(object_type, index_type)| {
                    let prop_atom = crate::query_boundaries::common::string_literal_value(
                        self.ctx.types,
                        index_type,
                    )?;
                    let object_members = crate::query_boundaries::common::intersection_members(
                        self.ctx.types,
                        object_type,
                    )
                    .unwrap_or_else(|| vec![object_type].into());
                    Some(object_members.into_iter().any(|member| {
                        crate::query_boundaries::common::find_property_in_object(
                            self.ctx.types,
                            member,
                            prop_atom,
                        )
                        .is_some_and(|prop| prop.optional)
                            || crate::query_boundaries::class_type::type_includes_undefined(
                                self.ctx.types,
                                self.evaluate_type_with_env(
                                    query_diagnostics::display_index_access_type(
                                        self.ctx.types,
                                        member,
                                        index_type,
                                    ),
                                ),
                            )
                    }))
                })
                .unwrap_or(false);
        if self.ctx.strict_null_checks()
            && (has_optional_property
                || crate::query_boundaries::class_type::type_includes_undefined(
                    self.ctx.types,
                    self.evaluate_type_with_env(display_type),
                ))
        {
            display_type =
                query_diagnostics::display_union_with_undefined(self.ctx.types, display_type);
        }
        Some(display_type)
    }

    pub(in crate::error_reporter) fn object_literal_target_property_type(
        &mut self,
        target_type: TypeId,
        prop_name_idx: NodeIndex,
        prop_name: &str,
    ) -> Option<(TypeId, TypeId)> {
        // Mirror tsc `getBestMatchIndexedAccessTypeOrUndefined` /
        // `findBestTypeForObjectLiteral`: object-literal elaboration only drills
        // into a property when the whole union — or the best-matching union
        // member — exposes it. For a fresh object-literal source against a union
        // target that contains an array-like member (e.g. the recursive JSON
        // alias `string | number | ... | Json[] | { [k: string]: Json }`), the
        // best match is the FIRST non-array-like member in union order. When a
        // primitive member (or any member that lacks the key) is the best match,
        // `tsc` skips the property drill-in and reports the outer relation error
        // with its per-member chain instead of an inner property TS2322. Without
        // this gate, tsz resolves the key through an arbitrary index-signature
        // member and elaborates into the property, losing the outer frame.
        let prefer_number_index = self
            .ctx
            .arena
            .get(prop_name_idx)
            .is_some_and(|node| node.kind == SyntaxKind::NumericLiteral as u16);
        let prefer_symbol_index = self.is_symbol_property_name(prop_name_idx);
        if self.object_literal_union_skips_property_drill_in(
            target_type,
            prop_name,
            prefer_number_index,
            prefer_symbol_index,
        ) {
            return None;
        }

        let resolved_target = self.resolve_type_for_property_access(target_type);
        let evaluated_target = self.judge_evaluate(resolved_target);
        let contextual_target = self.evaluate_contextual_type(target_type);
        let mut contextual_property_type = None;
        let mut env_property_type = None;
        let mut mapped_property_display_type = None;
        for candidate in [
            contextual_target,
            evaluated_target,
            resolved_target,
            target_type,
        ] {
            if let Some(property_type) =
                self.contextual_object_literal_property_type(candidate, prop_name)
                && self.should_prefer_property_target_type(contextual_property_type, property_type)
            {
                contextual_property_type = Some(property_type);
            }

            if let tsz_solver::operations::property::PropertyAccessResult::Success {
                type_id, ..
            } = self.resolve_property_access_with_env(candidate, prop_name)
                && self.should_prefer_property_target_type(env_property_type, type_id)
            {
                env_property_type = Some(type_id);
            }

            if let Some(type_id) = self.finite_mapped_target_property_type(candidate, prop_name)
                && self.should_prefer_property_target_type(env_property_type, type_id)
            {
                env_property_type = Some(type_id);
            }

            if mapped_property_display_type.is_none() {
                mapped_property_display_type =
                    self.finite_mapped_target_property_display_type(candidate, prop_name);
            }
        }

        if let Some(type_id) = env_property_type.or(contextual_property_type) {
            let prop_atom = self.ctx.types.intern_string(prop_name);
            let declared_optional_type = [
                contextual_target,
                evaluated_target,
                resolved_target,
                target_type,
            ]
            .into_iter()
            .filter_map(|candidate| {
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, candidate)
            })
            .find_map(|shape| {
                shape
                        .properties
                        .iter()
                        .find(|p| p.name == prop_atom && p.optional)
                        .map(|p| {
                            // tsc displays optional property types with `| undefined`
                            // in error messages only when strictNullChecks is enabled.
                            // Without strictNullChecks, undefined is implicit in all types
                            // and tsc shows the declared type without `| undefined`.
                            if !self.ctx.strict_null_checks() {
                                return p.type_id;
                            }
                            // Create a union with undefined if not already present.
                            if p.type_id == TypeId::UNDEFINED {
                                p.type_id
                            } else if let Some(list_id) =
                                crate::query_boundaries::common::union_list_id(
                                    self.ctx.types,
                                    p.type_id,
                                )
                            {
                                let members = self.ctx.types.type_list(list_id);
                                if members.contains(&TypeId::UNDEFINED) {
                                    p.type_id
                                } else {
                                    query_diagnostics::display_union_with_undefined(
                                        self.ctx.types,
                                        p.type_id,
                                    )
                                }
                            } else {
                                query_diagnostics::display_union_with_undefined(
                                    self.ctx.types,
                                    p.type_id,
                                )
                            }
                        })
            });

            let effective_type =
                if self.should_prefer_property_target_type(contextual_property_type, type_id) {
                    type_id
                } else {
                    contextual_property_type.unwrap_or(type_id)
                };
            // When strictNullChecks is off, strip synthetic `| undefined` from both
            // the effective type and the diagnostic type. Without strictNullChecks,
            // `undefined` is implicit in all types and tsc does not display it.
            let effective_type = if !self.ctx.strict_null_checks() {
                crate::query_boundaries::common::remove_undefined(
                    self.ctx.types.as_type_database(),
                    effective_type,
                )
            } else {
                effective_type
            };
            let declared_optional_type = declared_optional_type.map(|t| {
                if !self.ctx.strict_null_checks() {
                    crate::query_boundaries::common::remove_undefined(
                        self.ctx.types.as_type_database(),
                        t,
                    )
                } else {
                    t
                }
            });
            let mapped_property_display_type = mapped_property_display_type.map(|t| {
                if !self.ctx.strict_null_checks() {
                    crate::query_boundaries::common::remove_undefined(
                        self.ctx.types.as_type_database(),
                        t,
                    )
                } else {
                    t
                }
            });
            return Some((
                effective_type,
                mapped_property_display_type
                    .or(declared_optional_type)
                    .unwrap_or(effective_type),
            ));
        }

        let prop_node = self.ctx.arena.get(prop_name_idx)?;

        let prefer_number_index = prop_node.kind == SyntaxKind::NumericLiteral as u16;
        let prefer_symbol_index = self.is_symbol_property_name(prop_name_idx);
        // A number index signature only ever covers a canonical numeric name —
        // tsc's `isNumericLiteralName`. A plain non-numeric name (`b`, `"d"`)
        // is never constrained by a target's number index just because no
        // string index exists, and a numeric-*looking* but non-canonical
        // spelling (`"3.0"`, `"4.0"`) isn't either — falling back to the
        // number index unconditionally manufactured a spurious per-property
        // mismatch against every unrelated property once excess checking
        // deferred to a genuine index-signature violation elsewhere in the
        // same literal (`numericIndexerConstrainsPropertyDeclarations.ts`/`2.ts`).
        let key_is_numeric_like = tsz_solver::utils::is_numeric_literal_name(prop_name);

        // For type parameters, also check the constraint for index signatures
        let constraint_target =
            crate::query_boundaries::common::type_parameter_constraint(self.ctx.types, target_type);

        let candidates: Vec<TypeId> = [target_type, resolved_target, evaluated_target]
            .into_iter()
            .chain(constraint_target)
            .chain(constraint_target.map(|c| self.resolve_type_for_property_access(c)))
            .chain(constraint_target.map(|c| self.judge_evaluate(c)))
            .collect();

        let index_value_type = candidates
            .into_iter()
            .filter_map(|candidate| {
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, candidate)
            })
            .find_map(|shape| {
                let string_index = shape.string_index_signature();
                let symbol_index = shape.symbol_index_signature();
                if prefer_number_index {
                    shape
                        .number_index
                        .as_ref()
                        .map(|sig| sig.value_type)
                        .or_else(|| string_index.map(|sig| sig.value_type))
                } else if prefer_symbol_index {
                    symbol_index.map(|sig| sig.value_type)
                } else {
                    string_index.map(|sig| sig.value_type).or_else(|| {
                        key_is_numeric_like
                            .then(|| shape.number_index.as_ref().map(|sig| sig.value_type))
                            .flatten()
                    })
                }
            })?;

        Some((index_value_type, index_value_type))
    }

    /// tsc `findBestTypeForObjectLiteral` gate for object-literal elaboration,
    /// shared by the property drill-in resolver and the union index-signature
    /// per-property check.
    ///
    /// `tsc` resolves a drilled-in property type through
    /// `getBestMatchIndexedAccessTypeOrUndefined`: indexed access over the whole
    /// union, and — when that fails — indexed access into the best-matching
    /// union member. For a fresh object-literal source,
    /// `findBestTypeForObjectLiteral` picks the FIRST non-array-like member
    /// whenever the union contains an array-like member (so a recursive alias
    /// like `Json = string | number | ... | Json[] | { [k: string]: Json }`
    /// resolves the key against a leading primitive, not the trailing index
    /// signature).
    ///
    /// Returns `true` when object-literal elaboration must SKIP drilling into
    /// `prop_name` — i.e. the union has an array-like member and its best-match
    /// member (the first non-array-like member) does not expose the key — so the
    /// outer relation error is reported instead of an inner property TS2322.
    /// Returns `false` when the gate does not apply (no array-like member, or
    /// every member already exposes the key) or the best-match member exposes
    /// the key, leaving the existing drill-in behavior intact.
    pub(crate) fn object_literal_union_skips_property_drill_in(
        &mut self,
        target_type: TypeId,
        prop_name: &str,
        prefer_number_index: bool,
        prefer_symbol_index: bool,
    ) -> bool {
        use crate::query_boundaries::common::union_members;
        // Reveal a union hidden behind a type-alias/application (e.g. `Json`)
        // before testing for the array-like member pattern. Resolve/evaluate
        // lazily so a target that is already a union avoids the extra work.
        let members = if let Some(members) = union_members(self.ctx.types, target_type) {
            members
        } else {
            let resolved = self.resolve_type_for_property_access(target_type);
            if let Some(members) = union_members(self.ctx.types, resolved) {
                members
            } else if let Some(members) =
                union_members(self.ctx.types, self.judge_evaluate(resolved))
            {
                members
            } else {
                return false;
            }
        };
        // `members` is an owned `TypeIdList` (`Arc<[TypeId]>`) that derefs to a
        // slice and does not borrow `self`, so no copy is needed here.
        if !members
            .iter()
            .any(|&member| self.is_array_like_type(member))
        {
            return false;
        }
        // `tsc` first tries indexed access over the whole union, which succeeds
        // only when every member exposes the key; then the drill-in is valid.
        if members.iter().all(|&member| {
            self.union_member_exposes_object_literal_key(
                member,
                prop_name,
                prefer_number_index,
                prefer_symbol_index,
            )
        }) {
            return false;
        }
        // findBestTypeForObjectLiteral: the first non-array-like member. Skip the
        // drill-in unless that member genuinely exposes the key; otherwise the
        // best match is a primitive (or a lacking object) and the outer relation
        // error stands.
        match members
            .iter()
            .copied()
            .find(|&member| !self.is_array_like_type(member))
        {
            Some(best_member) => !self.union_member_exposes_object_literal_key(
                best_member,
                prop_name,
                prefer_number_index,
                prefer_symbol_index,
            ),
            None => true,
        }
    }

    /// Whether a union member exposes `prop_name` the way tsc's
    /// `getIndexedAccessTypeOrUndefined` would resolve it for a fresh
    /// object-literal source: a named property, or an index signature applicable
    /// to the key's kind. Primitive members (`string`, `number`, `boolean`,
    /// `null`, `undefined`, literals) never expose a key here — tsc returns
    /// `undefined` for `primitive["a"]` even though the apparent `String`/
    /// `Number` interfaces carry numeric index signatures.
    fn union_member_exposes_object_literal_key(
        &mut self,
        member: TypeId,
        prop_name: &str,
        prefer_number_index: bool,
        prefer_symbol_index: bool,
    ) -> bool {
        if member == TypeId::NULL
            || member == TypeId::UNDEFINED
            || crate::query_boundaries::common::is_primitive_type(self.ctx.types, member)
        {
            return false;
        }
        let prop_atom = self.ctx.types.intern_string(prop_name);
        let resolved = self.resolve_type_for_property_access(member);
        let evaluated = self.judge_evaluate(resolved);
        for candidate in [member, resolved, evaluated] {
            let Some(shape) =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, candidate)
            else {
                continue;
            };
            if shape.properties.iter().any(|p| p.name == prop_atom) {
                return true;
            }
            let has_applicable_index = if prefer_symbol_index {
                shape.symbol_index_signature().is_some()
            } else if prefer_number_index {
                shape.number_index.is_some() || shape.string_index_signature().is_some()
            } else {
                shape.string_index_signature().is_some()
            };
            if has_applicable_index {
                return true;
            }
        }
        false
    }

    /// Check whether a target type has a named property matching ANY property
    /// in a source object literal.  Used to detect "index-signature-only"
    /// target types where per-property elaboration would produce confusing
    /// diagnostics.  Returns `true` if at least one source property matches a
    /// named target property (not an index signature).
    pub(in crate::error_reporter::call_errors) fn target_has_named_property_for_any_source_prop(
        &mut self,
        source_obj_idx: NodeIndex,
        target_type: TypeId,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(source_obj_idx) else {
            return true; // conservative: assume named properties exist
        };
        let Some(obj) = self.ctx.arena.get_literal_expr(node) else {
            return true;
        };
        let obj = obj.clone();
        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            let prop_name = match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .and_then(|p| self.get_property_name(p.name)),
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_shorthand_property(elem_node)
                    .and_then(|p| self.get_property_name(p.name)),
                _ => continue,
            };
            if let Some(name) = prop_name
                && self.target_has_named_property(&name, target_type)
            {
                return true;
            }
        }
        false
    }

    /// Check whether a target type has a named (non-index-signature) property
    /// with the given name.  Returns `true` when the target resolves to an
    /// object shape that contains a property entry whose name matches
    /// `prop_name`.  Returns `false` when the only path to `prop_name` goes
    /// through a string/number index signature.
    ///
    /// For union types, returns `true` if any member has the named property.
    fn target_has_named_property(&mut self, prop_name: &str, target_type: TypeId) -> bool {
        let prop_atom = self.ctx.types.intern_string(prop_name);
        let resolved = self.resolve_type_for_property_access(target_type);
        let evaluated = self.judge_evaluate(resolved);
        for candidate in [target_type, resolved, evaluated] {
            // Check union members individually
            if let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, candidate)
            {
                for member in members {
                    if let Some(shape) = crate::query_boundaries::common::object_shape_for_type(
                        self.ctx.types,
                        member,
                    ) && shape.properties.iter().any(|p| p.name == prop_atom)
                    {
                        return true;
                    }
                }
            }
            if let Some(shape) =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, candidate)
                && shape.properties.iter().any(|p| p.name == prop_atom)
            {
                return true;
            }
        }
        false
    }

    pub(in crate::error_reporter) fn object_literal_property_name_text(
        &self,
        prop_name_idx: NodeIndex,
    ) -> Option<String> {
        self.get_property_name(prop_name_idx)
    }

    pub(in crate::error_reporter::call_errors) fn literal_call_argument_display(
        &self,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        self.literal_expression_display(arg_idx)
    }

    fn object_literal_call_argument_display_with_target_literals(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        if !self.object_literal_is_missing_required_target_property(arg_idx, param_type) {
            return None;
        }

        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(arg_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }

        let literal = self.ctx.arena.get_literal_expr(node)?;
        let elements = literal.elements.nodes.to_vec();
        let mut literal_overrides = FxHashMap::default();

        for elem_idx in elements {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            let (prop_name_idx, prop_value_idx) = match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) else {
                        continue;
                    };
                    (prop.name, prop.initializer)
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.ctx.arena.get_shorthand_property(elem_node) else {
                        continue;
                    };
                    (prop.name, prop.name)
                }
                _ => continue,
            };
            let Some(prop_name) = self
                .object_literal_property_name_text(prop_name_idx)
                .or_else(|| self.get_property_name_resolved(prop_name_idx))
            else {
                continue;
            };
            let Some((target_prop_type, _)) =
                self.object_literal_target_property_type(param_type, prop_name_idx, &prop_name)
            else {
                continue;
            };
            if !self.is_literal_sensitive_assignment_target(target_prop_type) {
                continue;
            }
            let Some(literal_type) = self.literal_type_from_initializer(prop_value_idx) else {
                continue;
            };
            literal_overrides.insert(self.ctx.types.intern_string(&prop_name), literal_type);
        }

        if literal_overrides.is_empty() {
            return None;
        }

        let display_type = crate::query_boundaries::diagnostics::widen_argument_type_for_display(
            self.ctx.types,
            arg_type,
        );
        let shape =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, display_type)?;
        let mut props = shape.properties.clone();
        props.sort_by_key(|prop| prop.declaration_order);

        let mut rendered = Vec::new();
        for prop in props {
            let name = self.ctx.types.resolve_atom(prop.name);
            let ty_display = if let Some(&literal_type) = literal_overrides.get(&prop.name) {
                self.format_type_for_assignability_message(literal_type)
            } else {
                let widened =
                    crate::query_boundaries::common::widen_type(self.ctx.types, prop.type_id);
                let mut formatter = self
                    .ctx
                    .create_diagnostic_type_formatter()
                    .with_preserve_optional_parameter_surface_syntax(true);
                formatter.format(widened).into_owned()
            };
            let optional = if prop.optional { "?" } else { "" };
            rendered.push(format!("{name}{optional}: {ty_display};"));
        }

        Some(format!("{{ {} }}", rendered.join(" ")))
    }

    pub(in crate::error_reporter::call_errors) fn object_literal_is_missing_required_target_property(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
    ) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(arg_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(literal) = self.ctx.arena.get_literal_expr(node) else {
            return false;
        };
        let elements = literal.elements.nodes.to_vec();
        let mut source_names = rustc_hash::FxHashSet::default();
        for elem_idx in elements {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            let name_idx = match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .map(|p| p.name),
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_shorthand_property(elem_node)
                    .map(|p| p.name),
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    self.ctx.arena.get_method_decl(elem_node).map(|m| m.name)
                }
                _ => None,
            };
            let Some(name_idx) = name_idx else {
                continue;
            };
            if let Some(name) = self
                .object_literal_property_name_text(name_idx)
                .or_else(|| self.get_property_name_resolved(name_idx))
            {
                source_names.insert(name);
            }
        }

        let resolved = self.resolve_type_for_property_access(param_type);
        let evaluated = self.evaluate_type_with_env(resolved);
        let evaluated = self.resolve_lazy_type(evaluated);
        let evaluated = self.evaluate_application_type(evaluated);
        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, evaluated)
        else {
            return false;
        };

        static OBJECT_PROTO_METHODS: &[&str] = &[
            "constructor",
            "toString",
            "toLocaleString",
            "valueOf",
            "hasOwnProperty",
            "isPrototypeOf",
            "propertyIsEnumerable",
        ];

        shape.properties.iter().any(|prop| {
            !prop.optional && {
                let name = self.ctx.types.resolve_atom(prop.name);
                !source_names.contains(name.as_str())
                    && !OBJECT_PROTO_METHODS.contains(&name.as_str())
            }
        })
    }

    fn jsdoc_constructor_identifier_source_display(
        &mut self,
        expr_idx: NodeIndex,
        arg_type: TypeId,
    ) -> Option<String> {
        let arg_type = self.evaluate_type_with_env(arg_type);
        let arg_type = self.resolve_type_for_property_access(arg_type);
        if !crate::query_boundaries::common::is_constructor_like_type(self.ctx.types, arg_type) {
            return None;
        }
        let expr_node = self.ctx.arena.get(expr_idx)?;
        let ident = self.ctx.arena.get_identifier(expr_node)?;
        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        self.symbol_has_js_constructor_evidence(sym_id)
            .then(|| format!("typeof {}", ident.escaped_text))
    }

    fn zero_argument_call_list_display(&self, arg_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(arg_idx)?;
        if node.kind != syntax_kind_ext::CALL_EXPRESSION
            && node.kind != syntax_kind_ext::NEW_EXPRESSION
        {
            return None;
        }
        let call = self.ctx.arena.get_call_expr(node)?;
        if call
            .arguments
            .as_ref()
            .is_none_or(|args| args.nodes.is_empty())
        {
            Some("[]".to_string())
        } else {
            None
        }
    }

    pub(in crate::error_reporter) fn format_call_argument_type_for_diagnostic(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) -> String {
        // A plain `expr as T` / `<T>expr` assertion argument yields the asserted
        // type `T` as written. `tsc` reports it with its literal element /
        // property types intact (a regular, non-fresh type) rather than widening
        // them as for a fresh array/object literal argument. Detect the assertion
        // before the `skip_parenthesized_and_assertions` below peels it away to
        // the inner literal (which would otherwise route the operand through the
        // fresh-literal widening). `format_type_diagnostic` renders the asserted
        // type with literals preserved. `as const` and `satisfies` are excluded.
        if self.expression_is_plain_type_assertion(arg_idx) {
            return self.format_type_diagnostic(arg_type);
        }

        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(arg_idx);
        let is_array_literal_arg = self
            .ctx
            .arena
            .get(expr_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION);
        if let Some(expr_node) = self.ctx.arena.get(expr_idx)
            && let Some(ident) = self.ctx.arena.get_identifier(expr_node)
            && ident.escaped_text == "arguments"
            && self.has_enclosing_regular_function(expr_idx)
        {
            return "IArguments".to_string();
        }
        // Preserve the declared alias display for readonly array/tuple
        // arguments before structural fallbacks collapse it.
        if let Some(alias) = self.readonly_array_alias_source_display(expr_idx, arg_type) {
            return alias;
        }

        if query_common::tuple_elements(self.ctx.types, arg_type).is_some()
            && self
                .ctx
                .arena
                .get(expr_idx)
                .is_none_or(|node| node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
        {
            return self.format_type_diagnostic(arg_type);
        }

        // When the only literal-sensitive member of the parameter type is
        // `undefined` contributed by an optional parameter (`b?: T`), tsc
        // strips the synthetic `| undefined` and widens the argument display
        // for the underlying target. Skip the literal-preserving branch in
        // that case so the argument widens to its widened display type
        // (e.g. `string` instead of `'"hello"'`).
        //
        // Additionally, only preserve the source literal when the target's
        // primitive structure makes the literal display informative — for a
        // mixed-primitive target like `string | "hello"` whose unique base
        // appears in plain primitive form, the source widens to its base to
        // match tsc's output. See `literal_widening_policy` for the full
        // rule.
        if self.is_literal_sensitive_assignment_target(param_type)
            && !self.literal_sensitivity_is_only_synthetic_optional_undefined(param_type, arg_idx)
            && self.source_literal_primitive_matches_target_literal(arg_type, arg_idx, param_type)
            && let Some(display) = self.literal_call_argument_display(arg_idx)
        {
            return display;
        }

        if self
            .ctx
            .arena
            .get(expr_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION)
            && let Some(display) = self.conditional_callable_union_argument_display(arg_type)
        {
            return display;
        }

        if let Some(display) =
            self.contextual_function_argument_display(arg_type, param_type, arg_idx)
        {
            return display;
        }

        if let Some(display) = self.object_literal_call_argument_display_with_target_literals(
            arg_type, param_type, arg_idx,
        ) {
            return display;
        }

        if let Some(display) =
            self.identifier_array_object_literal_source_display(expr_idx, param_type)
        {
            return display;
        }
        if let Some(display) = self.jsdoc_constructor_identifier_source_display(expr_idx, arg_type)
        {
            return display;
        }
        if !is_array_literal_arg
            && let Some(display) = self.rebuilt_array_source_display(arg_type, param_type)
        {
            return display;
        }
        if let Some(display) =
            self.declared_identifier_source_display(expr_idx, param_type, arg_type)
        {
            return display;
        }

        if self.call_target_preserves_literal_argument_surface(param_type, arg_idx)
            && self.source_literal_primitive_matches_target_literal(arg_type, arg_idx, param_type)
            && let Some(display) = self.literal_call_argument_display(arg_idx)
        {
            if (display == "true" || display == "false")
                && self.call_target_should_widen_boolean_literal_display(param_type)
            {
                return "boolean".to_string();
            }
            return display;
        }

        let mut display_type = if param_type == TypeId::NEVER {
            if let Some(display) = self.zero_argument_call_list_display(arg_idx) {
                return display;
            }
            let direct_arg_type = self.elaboration_source_expression_type(arg_idx);
            if direct_arg_type == TypeId::ERROR || direct_arg_type == arg_type {
                arg_type
            } else {
                direct_arg_type
            }
        } else {
            crate::query_boundaries::diagnostics::widen_argument_type_for_display(
                self.ctx.types,
                arg_type,
            )
        };

        if crate::query_boundaries::common::is_mapped_type(self.ctx.types, display_type) {
            let evaluated_display = self.evaluate_type_for_assignability(display_type);
            if crate::query_boundaries::common::object_shape_for_type(
                self.ctx.types,
                evaluated_display,
            )
            .is_some()
            {
                display_type = evaluated_display;
            }
        }

        let should_widen_display = self
            .materialize_finite_mapped_call_parameter_display_type(param_type)
            .is_some()
            && crate::query_boundaries::common::object_shape_for_type(self.ctx.types, display_type)
                .is_some()
            || (is_array_literal_arg && !self.is_literal_sensitive_assignment_target(param_type));

        let display = if should_widen_display {
            self.format_type_diagnostic_widened(display_type)
        } else {
            self.format_type_for_assignability_message(display_type)
        };
        self.rewrite_source_display_for_non_literal_target_assignability(
            arg_type, param_type, display,
        )
    }
}
