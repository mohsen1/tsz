//! Generic JSX class-component signature synthesis.
//!
//! Split out of `resolution.rs` to satisfy the source-file line cap: the
//! construct-signature synthesis (props parameter recovery through
//! `JSX.ElementAttributesProperty`) and the default-instantiated props
//! fallback for generic class components.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    pub(in crate::checkers_domain::jsx) fn get_default_instantiated_generic_class_props_type(
        &mut self,
        component_type: TypeId,
    ) -> Option<TypeId> {
        use crate::query_boundaries::common::PropertyAccessResult;

        let sigs = crate::query_boundaries::common::construct_signatures_for_type(
            self.ctx.types,
            component_type,
        )?;
        let generic: Vec<_> = sigs
            .iter()
            .filter(|sig| !sig.type_params.is_empty())
            .collect();
        if generic.len() != 1 {
            return None;
        }

        let sig = generic[0];
        let props = sig.params.first().map(|param| param.type_id).or_else(|| {
            let evaluated_return_type = self.evaluate_type_with_env(sig.return_type);
            match self.get_element_attributes_property_name_with_check(None) {
                None => match self.resolve_property_access_with_env(sig.return_type, "props") {
                    PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                    _ => {
                        match self.resolve_property_access_with_env(evaluated_return_type, "props")
                        {
                            PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                            _ => None,
                        }
                    }
                },
                Some(name) if name.is_empty() => Some(sig.return_type),
                Some(name) => match self.resolve_property_access_with_env(sig.return_type, &name) {
                    PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                    _ => {
                        match self.resolve_property_access_with_env(evaluated_return_type, &name) {
                            PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                            _ => None,
                        }
                    }
                },
            }
        })?;

        let type_args: Vec<_> = sig
            .type_params
            .iter()
            .map(|param| {
                param
                    .default
                    .or(param.constraint)
                    .unwrap_or(TypeId::UNKNOWN)
            })
            .collect();
        let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
            self.ctx.types,
            &sig.type_params,
            &type_args,
        );
        let instantiated =
            crate::query_boundaries::common::instantiate_type(self.ctx.types, props, &substitution);
        let evaluated =
            if crate::query_boundaries::common::is_union_type(self.ctx.types, instantiated)
                || crate::computation::call_inference::should_preserve_contextual_application_shape(
                    self.ctx.types,
                    instantiated,
                )
            {
                instantiated
            } else {
                self.evaluate_type_with_env(instantiated)
            };
        let managed = self.apply_jsx_library_managed_attributes(component_type, evaluated);
        if managed == TypeId::ANY
            || managed == TypeId::UNKNOWN
            || managed == TypeId::ERROR
            || crate::query_boundaries::common::contains_type_parameters(self.ctx.types, managed)
        {
            None
        } else {
            Some(managed)
        }
    }

    pub(super) fn compact_jsx_readonly_display(display: String) -> String {
        let mut out = String::with_capacity(display.len());
        let mut rest = display.as_str();
        while let Some(pos) = rest.find("Readonly<") {
            out.push_str(&rest[..pos]);
            out.push_str("Readonly<...>");
            let mut depth = 0i32;
            let mut end = pos;
            for (offset, ch) in rest[pos..].char_indices() {
                match ch {
                    '<' => depth += 1,
                    '>' => {
                        depth -= 1;
                        if depth == 0 {
                            end = pos + offset + ch.len_utf8();
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if end <= pos {
                rest = &rest[pos + "Readonly<".len()..];
            } else {
                rest = &rest[end..];
            }
        }
        out.push_str(rest);
        out
    }

    pub(in crate::checkers_domain::jsx) fn infer_jsx_generic_class_component_signature(
        &mut self,
        _element_idx: NodeIndex,
        component_type: TypeId,
    ) -> Option<tsz_solver::FunctionShape> {
        let call_sig = crate::query_boundaries::common::construct_signatures_for_type(
            self.ctx.types,
            component_type,
        )?
        .first()?
        .clone();
        let mut function_shape =
            crate::query_boundaries::checkers::jsx::construct_signature_function_shape(call_sig);
        if function_shape.type_params.is_empty() {
            return None;
        }

        if function_shape.params.is_empty() {
            use crate::query_boundaries::common::PropertyAccessResult;

            let evaluated_return_type = self.evaluate_type_with_env(function_shape.return_type);

            // Bind the construct signature's own type parameters while
            // resolving the instance props member: this signature IS the
            // binding scope for them, so the property-access layer's
            // unbound-parameter fallback (`resolve_unbound_property_member_defaults`)
            // must not collapse `Props & BaseProps<Values>` to the parameter
            // defaults before attribute inference has run (issue #15687).
            // The exact interned param `TypeId`s referenced by the instance
            // members are collected by name from the return type; a freshly
            // interned `TypeParamInfo` copy can carry a distinct `TypeId` the
            // scope check would not match.
            let own_param_names: std::collections::HashSet<tsz_common::Atom> = function_shape
                .type_params
                .iter()
                .map(|info| info.name)
                .collect();
            let free_params = crate::query_boundaries::common::free_type_params_named(
                self.ctx.types,
                [function_shape.return_type, evaluated_return_type],
                &own_param_names,
            );
            let scope_bindings: Vec<(String, Option<TypeId>)> = free_params
                .into_iter()
                .map(|(name_atom, param_type)| {
                    let name = self.ctx.types.resolve_atom(name_atom);
                    let previous = self.ctx.type_parameter_scope.get(&name).copied();
                    self.ctx
                        .type_parameter_scope
                        .insert(name.clone(), param_type);
                    (name, previous)
                })
                .collect();
            let synthesized_param_type = match self
                .get_element_attributes_property_name_with_check(None)
            {
                None => {
                    match self.resolve_property_access_with_env(function_shape.return_type, "props")
                    {
                        PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                        _ => match self
                            .resolve_property_access_with_env(evaluated_return_type, "props")
                        {
                            PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                            _ => None,
                        },
                    }
                }
                Some(name) if name.is_empty() => Some(function_shape.return_type),
                Some(name) => {
                    match self.resolve_property_access_with_env(function_shape.return_type, &name) {
                        PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                        _ => match self
                            .resolve_property_access_with_env(evaluated_return_type, &name)
                        {
                            PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                            _ => None,
                        },
                    }
                }
            }
            .filter(|type_id| !matches!(*type_id, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN));

            // Restore in reverse so that if two distinct param TypeIds share a
            // name, the first-saved `previous` (the caller's binding) wins.
            for (name, previous) in scope_bindings.into_iter().rev() {
                match previous {
                    Some(previous) => {
                        self.ctx.type_parameter_scope.insert(name, previous);
                    }
                    None => {
                        self.ctx.type_parameter_scope.remove(&name);
                    }
                }
            }

            if let Some(type_id) = synthesized_param_type {
                let props_name = self.ctx.types.intern_string("props");
                crate::query_boundaries::checkers::jsx::push_required_param(
                    &mut function_shape,
                    props_name,
                    type_id,
                );
            }
        }

        if function_shape.params.is_empty() {
            return None;
        }

        Some(function_shape)
    }
}
