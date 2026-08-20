//! Instantiated generic-call parameter display for call diagnostics.
//!
//! The TS2345 target half of a generic call whose failing parameter mentions
//! signature type parameters: structural replacement matching between the raw
//! signature and the call's arguments, the later-literal check-time restore
//! (#17686), and the arm-wise mixed-union display that recovers instantiated
//! arms from the relation's final parameter type.

use crate::query_boundaries::common as query_common;
use crate::query_boundaries::diagnostics as query_diagnostics;
use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
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

    /// Arm-wise display for a generic-call union parameter mixing concrete
    /// arms with type-parameter-dependent arms (`u: U | T[]`): tsc
    /// instantiates such a union arm-wise, so a concrete alias arm keeps its
    /// written reference while a type-parameter arm renders through the
    /// call's actual inference — widened when the inference source was a
    /// fresh literal, unwidened under explicit type arguments. The
    /// instantiated arms are recovered from the relation's own final
    /// parameter type (the members left once every concrete arm's evaluation
    /// is accounted for), so this display never re-decides widening. A
    /// recovered member is repainted through its raw arm's substitution when
    /// that round-trips (keeping `Application` spellings like `Box<number>`).
    /// `None` when the raw/final member sets do not line up; the monolithic
    /// instantiated display owns those.
    fn mixed_union_parameter_arm_wise_display(
        &mut self,
        raw_param_type: TypeId,
        final_param_type: TypeId,
        type_params: &[tsz_solver::TypeParamInfo],
    ) -> Option<String> {
        let raw_members = query_common::union_members(self.ctx.types, raw_param_type)?;
        let (generic_arms, concrete_arms): (Vec<TypeId>, Vec<TypeId>) = raw_members
            .iter()
            .copied()
            .partition(|&arm| query_common::contains_type_parameters(self.ctx.types, arm));
        if generic_arms.is_empty() || concrete_arms.is_empty() {
            return None;
        }

        let final_eval = self.evaluate_type_for_assignability(final_param_type);
        if final_eval == TypeId::ERROR {
            return None;
        }
        let final_members: Vec<TypeId> =
            query_common::union_members(self.ctx.types, final_eval)?.to_vec();

        // Every concrete arm's evaluation must be present verbatim in the
        // final union; whatever remains is the image of the generic arms.
        let mut covered = vec![false; final_members.len()];
        for &arm in &concrete_arms {
            let evaluated = self.evaluate_type_for_assignability(arm);
            let arm_members = query_common::union_members(self.ctx.types, evaluated)
                .map_or_else(|| vec![evaluated], |members| members.to_vec());
            for member in arm_members {
                let slot = final_members
                    .iter()
                    .position(|&candidate| candidate == member)?;
                covered[slot] = true;
            }
        }
        let remainder: Vec<TypeId> = final_members
            .iter()
            .zip(covered.iter())
            .filter_map(|(&member, &was_covered)| (!was_covered).then_some(member))
            .collect();
        if remainder.is_empty() {
            return None;
        }

        // Repaint each recovered member through the raw generic arm whose
        // substitution round-trips to it, so alias applications keep their
        // written head; a member no arm explains renders as itself.
        let mut display_members = concrete_arms;
        for &member in &remainder {
            let repainted = generic_arms.iter().find_map(|&arm| {
                let mut replacements = FxHashMap::default();
                self.collect_type_param_display_replacements(arm, member, &mut replacements);
                let mut subst =
                    crate::query_boundaries::generic_instantiation::signature_domain_substitution(
                        type_params,
                    );
                for tp in type_params {
                    if let Some(&replacement) = replacements.get(&tp.name) {
                        subst.insert(tp.name, replacement);
                    }
                }
                let instantiated = query_common::instantiate_type(self.ctx.types, arm, &subst);
                if query_common::contains_type_parameters(self.ctx.types, instantiated) {
                    return None;
                }
                (instantiated == member
                    || self.evaluate_type_for_assignability(instantiated) == member)
                    .then_some(instantiated)
            });
            display_members.push(repainted.unwrap_or(member));
        }

        let display_type =
            query_diagnostics::display_union_literal_reduce_type(self.ctx.types, display_members);
        if query_common::contains_type_parameters(self.ctx.types, display_type) {
            return None;
        }
        Some(
            self.format_type_for_assignability_message(display_type)
                .replace("new(", "new (")
                .replace("?: unknown | undefined", "?: unknown"),
        )
    }

    pub(in crate::error_reporter::call_errors) fn instantiated_call_parameter_display(
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
        // Instantiation is the identity on a parameter type that mentions none
        // of the signature's type parameters: tsc renders the written form
        // (alias reference included) in the TS2345 target, so the
        // alias-preserving fallback owns the display. Mirrors the same guard
        // in `generic_call_parameter_alias_display`.
        if !crate::query_boundaries::common::contains_type_parameters(
            self.ctx.types,
            raw_param_type,
        ) {
            return None;
        }
        if param_is_rest && self.rest_tuple_parameter_reports_per_position(raw_param_type) {
            return None;
        }

        // A rest argument relates against the rest parameter's element type,
        // so the arm-wise union display reads the element too.
        let raw_union_source = if param_is_rest {
            query_common::array_element_type(self.ctx.types, raw_param_type)
                .unwrap_or(raw_param_type)
        } else {
            raw_param_type
        };
        if let Some(display) = self.mixed_union_parameter_arm_wise_display(
            raw_union_source,
            param_type,
            &raw_sig.type_params,
        ) {
            return Some(display);
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
}
