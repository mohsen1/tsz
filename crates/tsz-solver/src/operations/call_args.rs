//! Argument checking, parameter analysis, and inference helpers for `CallEvaluator`.
//!
//! This module contains the argument-matching utilities used during function call
//! resolution and generic inference:
//! - Parameter/argument type checking (`check_argument_types`)
//! - Argument count bounds and rest parameter expansion
//! - Tuple rest pattern handling (`expand_tuple_rest`, `tuple_rest_element_type`)
//! - Placeholder/inference variable detection (`type_contains_placeholder`)
//! - Contextual sensitivity analysis (`is_contextually_sensitive`)

use super::{AssignabilityChecker, CallEvaluator, CallResult};
use crate::operations::iterators::get_iterator_info;
use crate::types::{
    IntrinsicKind, LiteralValue, ParamInfo, TemplateSpan, TupleElement, TypeData, TypeId,
};
use crate::utils::{self, TupleRestExpansion};
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::trace;

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    fn is_string_like_type(&self, type_id: TypeId) -> bool {
        matches!(
            self.interner.lookup(type_id),
            Some(TypeData::Intrinsic(IntrinsicKind::String))
                | Some(TypeData::TemplateLiteral(_))
                | Some(TypeData::Literal(LiteralValue::String(_)))
        )
    }

    fn extract_iterable_yield_type(&mut self, target: TypeId) -> Option<TypeId> {
        use crate::visitor::{
            application_id, callable_shape_id, object_shape_id, object_with_index_shape_id,
        };

        if let Some(TypeData::Application(app_id)) = self.interner.lookup(target) {
            let app = self.interner.type_application(app_id);
            if let Some(&first_arg) = app.args.first() {
                let evaluated = self.checker.evaluate_type(target);
                if self.is_iterable_like_call_target(evaluated) {
                    return Some(first_arg);
                }
            }
        }

        if let Some(iter_info) = get_iterator_info(self.interner, target, false) {
            return Some(iter_info.yield_type);
        }

        let shape_id = object_shape_id(self.interner, target)
            .or_else(|| object_with_index_shape_id(self.interner, target))?;
        let shape = self.interner.object_shape(shape_id);
        let sym_iter_atom = self.interner.intern_string("[Symbol.iterator]");
        let iter_prop = shape
            .properties
            .binary_search_by_key(&sym_iter_atom, |p| p.name)
            .ok()
            .map(|idx| &shape.properties[idx])?;
        let callable_id = callable_shape_id(self.interner, iter_prop.type_id)?;
        let callable = self.interner.callable_shape(callable_id);
        let return_type = callable.call_signatures.first()?.return_type;
        let app_id = application_id(self.interner, return_type)?;
        let app = self.interner.type_application(app_id);
        app.args.first().copied()
    }

    fn is_iterable_like_call_target(&self, type_id: TypeId) -> bool {
        match self.interner.lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                if shape.number_index.is_some() {
                    return true;
                }
                shape.properties.iter().any(|prop| {
                    let name = self.interner.resolve_atom(prop.name);
                    name == "__@iterator" || name == "[Symbol.iterator]"
                })
            }
            Some(TypeData::Intersection(members)) => self
                .interner
                .type_list(members)
                .iter()
                .any(|&member| self.is_iterable_like_call_target(member)),
            _ => false,
        }
    }

    pub(crate) fn function_signature_is_contextually_sensitive(
        &self,
        params: &[ParamInfo],
    ) -> bool {
        params.iter().any(|param| {
            param.type_id == TypeId::ANY || self.type_uses_inference_placeholders(param.type_id)
        })
    }

    fn is_assignable_via_contextual_signatures_strict(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let normalize = |shape: crate::types::FunctionShape| {
            use crate::type_queries::unpack_tuple_rest_parameter;

            let mut normalized = shape.clone();
            normalized.params = shape
                .params
                .iter()
                .flat_map(|param| unpack_tuple_rest_parameter(self.interner, param))
                .collect();
            normalized
        };
        let source = self.instantiate_generic_function_argument_against_target(source, target);
        let Some(source_fn) =
            Self::get_contextual_signature(self.interner.as_type_database(), source)
        else {
            return false;
        };
        let Some(target_fn) =
            Self::get_contextual_signature(self.interner.as_type_database(), target)
        else {
            return false;
        };
        let source_fn = normalize(source_fn);
        let target_fn = normalize(target_fn);

        self.checker.is_assignable_to_strict(
            self.interner.function(source_fn),
            self.interner.function(target_fn),
        )
    }

    /// Check if a callback argument has more required parameters than the target
    /// callback can accept. This is a pre-check that runs before bivariant callback
    /// assignability, because bivariance only relaxes parameter TYPE checking, not
    /// parameter COUNT checking.
    ///
    /// In TypeScript, `(items: X) => void` is NOT assignable to `() => any` because
    /// the source requires 1 argument but the target is called with 0.
    /// This mirrors tsc's behavior where function arity is enforced even in bivariant
    /// callback positions.
    fn callback_source_has_excess_required_params(&self, source: TypeId, target: TypeId) -> bool {
        let Some(source_fn) =
            Self::get_contextual_signature(self.interner.as_type_database(), source)
        else {
            return false;
        };
        let Some(target_fn) =
            Self::get_contextual_signature(self.interner.as_type_database(), target)
        else {
            return false;
        };

        // If target has a rest parameter, the arity is effectively unlimited
        // (handled by the existing generic rest check or the full subtype check).
        let target_has_rest = target_fn.params.last().is_some_and(|p| p.rest);
        if target_has_rest {
            return false;
        }

        let source_required = crate::utils::required_param_count(&source_fn.params);
        let target_fixed_count = target_fn.params.len();

        // Extra source params of type `void` are effectively optional in TypeScript
        if source_required > target_fixed_count {
            let extra_are_void = source_fn
                .params
                .iter()
                .skip(target_fixed_count)
                .take(source_required.saturating_sub(target_fixed_count))
                .all(|param| {
                    param.type_id == TypeId::VOID
                        || if let Some(crate::TypeData::Union(list_id)) =
                            self.interner.lookup(param.type_id)
                        {
                            self.interner.type_list(list_id).contains(&TypeId::VOID)
                        } else {
                            false
                        }
                });
            return !extra_are_void;
        }

        false
    }

    fn callback_requires_more_fixed_params_than_generic_rest_allows(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let normalize = |shape: crate::types::FunctionShape| {
            use crate::type_queries::unpack_tuple_rest_parameter;

            let mut normalized = shape.clone();
            normalized.params = shape
                .params
                .iter()
                .flat_map(|param| unpack_tuple_rest_parameter(self.interner, param))
                .collect();
            normalized
        };

        let Some(source_fn) =
            Self::get_contextual_signature(self.interner.as_type_database(), source)
        else {
            return false;
        };
        let Some(target_fn) =
            Self::get_contextual_signature(self.interner.as_type_database(), target)
        else {
            return false;
        };

        let source_fn = normalize(source_fn);
        let target_fn = normalize(target_fn);
        let Some(target_rest) = target_fn.params.last().filter(|param| param.rest) else {
            return false;
        };

        let target_rest_is_generic =
            crate::type_queries::is_type_parameter_like(self.interner, target_rest.type_id)
                || crate::type_queries::contains_type_parameters_db(
                    self.interner,
                    target_rest.type_id,
                );

        if !target_rest_is_generic {
            return false;
        }

        let source_required = crate::utils::required_param_count(&source_fn.params);
        let target_fixed_count = target_fn.params.len().saturating_sub(1);
        source_required > target_fixed_count
    }

    fn type_uses_inference_placeholders(&self, type_id: TypeId) -> bool {
        match self.interner.lookup(type_id) {
            Some(TypeData::TypeParameter(info)) => {
                let name = self.interner.resolve_atom(info.name);
                name.as_str().starts_with("__infer_")
                    || info
                        .constraint
                        .is_some_and(|constraint| self.type_uses_inference_placeholders(constraint))
            }
            Some(TypeData::Infer(info)) => info
                .constraint
                .is_some_and(|constraint| self.type_uses_inference_placeholders(constraint)),
            Some(TypeData::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                self.function_signature_is_contextually_sensitive(&shape.params)
                    || self.type_uses_inference_placeholders(shape.return_type)
            }
            // Callable types represent class constructor values (pre-existing,
            // never contextually sensitive). Merged with default arm below.
            Some(TypeData::Union(members)) | Some(TypeData::Intersection(members)) => self
                .interner
                .type_list(members)
                .iter()
                .any(|&member| self.type_uses_inference_placeholders(member)),
            Some(TypeData::Object(shape_id)) | Some(TypeData::ObjectWithIndex(shape_id)) => self
                .interner
                .object_shape(shape_id)
                .properties
                .iter()
                .any(|prop| self.type_uses_inference_placeholders(prop.type_id)),
            Some(TypeData::Array(elem))
            | Some(TypeData::ReadonlyType(elem))
            | Some(TypeData::NoInfer(elem))
            | Some(TypeData::KeyOf(elem))
            | Some(TypeData::Enum(_, elem)) => self.type_uses_inference_placeholders(elem),
            Some(TypeData::Tuple(elements)) => self
                .interner
                .tuple_list(elements)
                .iter()
                .any(|elem| self.type_uses_inference_placeholders(elem.type_id)),
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                self.type_uses_inference_placeholders(app.base)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.type_uses_inference_placeholders(arg))
            }
            Some(TypeData::IndexAccess(obj, key)) => {
                self.type_uses_inference_placeholders(obj)
                    || self.type_uses_inference_placeholders(key)
            }
            Some(TypeData::Conditional(cond_id)) => {
                let cond = self.interner.get_conditional(cond_id);
                self.type_uses_inference_placeholders(cond.check_type)
                    || self.type_uses_inference_placeholders(cond.extends_type)
                    || self.type_uses_inference_placeholders(cond.true_type)
                    || self.type_uses_inference_placeholders(cond.false_type)
            }
            Some(TypeData::Mapped(mapped_id)) => {
                let mapped = self.interner.get_mapped(mapped_id);
                self.type_uses_inference_placeholders(mapped.constraint)
                    || self.type_uses_inference_placeholders(mapped.template)
            }
            Some(TypeData::StringIntrinsic { type_arg, .. }) => {
                self.type_uses_inference_placeholders(type_arg)
            }
            Some(TypeData::TemplateLiteral(spans)) => self
                .interner
                .template_list(spans)
                .iter()
                .any(|span| match span {
                    crate::types::TemplateSpan::Text(_) => false,
                    crate::types::TemplateSpan::Type(inner) => {
                        self.type_uses_inference_placeholders(*inner)
                    }
                }),
            _ => false,
        }
    }

    /// Expand a `TypeParameter` to its constraint (if it has one).
    /// This is used when a `TypeParameter` from an outer scope is used as an argument.
    pub(super) fn expand_type_param(&self, ty: TypeId) -> TypeId {
        match self.interner.lookup(ty) {
            Some(TypeData::TypeParameter(tp)) => tp.constraint.unwrap_or(ty),
            _ => ty,
        }
    }

    pub(super) fn check_argument_types(
        &mut self,
        params: &[ParamInfo],
        arg_types: &[TypeId],
        allow_bivariant_callbacks: bool,
    ) -> Option<CallResult> {
        self.check_argument_types_with(params, arg_types, false, allow_bivariant_callbacks)
    }

    pub(crate) fn check_argument_types_with(
        &mut self,
        params: &[ParamInfo],
        arg_types: &[TypeId],
        strict: bool,
        allow_bivariant_callbacks: bool,
    ) -> Option<CallResult> {
        let arg_count = arg_types.len();
        for (i, arg_type) in arg_types.iter().enumerate() {
            let Some(param_type) = self.param_type_for_arg_index(params, i, arg_count) else {
                break;
            };
            if *arg_type == param_type {
                continue;
            }

            // In tsc, passing `undefined` to an optional parameter is always
            // accepted — the parameter type implicitly includes `undefined`
            // via the `?` marker.  We check this here (lazily, at the call
            // site) rather than baking `| undefined` into the parameter type
            // at signature build time, because lib signatures are built without
            // strictNullChecks and would otherwise miss it.
            if *arg_type == TypeId::UNDEFINED || *arg_type == TypeId::VOID {
                let param_info = self.param_info_for_arg_index(params, i);
                if param_info.is_some_and(|p| p.optional) {
                    continue;
                }
            }

            // When the parameter is optional (`?`), its effective type includes `undefined`.
            // The fast-path above handles the exact `undefined` case; here we strip
            // `undefined` from the arg type so that `string | undefined` is checked as
            // `string` against the raw param type `string`.  This preserves error
            // reporting paths (TS2322 property-level errors) that would break if we
            // instead widened param_type to a union.
            let arg_type_for_check =
                if let Some(param_info) = self.param_info_for_arg_index(params, i) {
                    if param_info.optional {
                        crate::narrowing::utils::remove_undefined(self.interner, *arg_type)
                    } else {
                        *arg_type
                    }
                } else {
                    *arg_type
                };

            // Expand TypeParameters to their constraints for assignability checking when the
            // *parameter* expects a concrete type (e.g. `object`) but the argument is an outer
            // type parameter with a compatible constraint.
            //
            // IMPORTANT: Do **not** expand when the parameter type is itself a type parameter;
            // otherwise a call like `freeze(obj)` where `obj: T extends object` can incorrectly
            // compare `object` (expanded) against `T` and fail, even though inference would (and
            // tsc does) infer the inner `T` to the outer `T`.
            let expanded_arg_type = match self.interner.lookup(param_type) {
                Some(TypeData::TypeParameter(_) | TypeData::Infer(_)) => arg_type_for_check,
                _ => self.expand_type_param(arg_type_for_check),
            };

            // When the parameter type is an unconstrained type parameter, a concrete
            // argument is NOT assignable to it (T could be anything). However, when the
            // argument itself is also a type parameter (or the same type parameter),
            // we let the normal assignability path handle it. This matches tsc which
            // rejects `foo<U>(42)` but allows `foo<U>(x)` where `x: U`.
            //
            // Note: Previously this skipped ALL non-nullish arguments to unconstrained
            // type-parameter params, which was too lenient and suppressed TS2345 errors
            // for cases like `function outer<T>() { accept<T>(42); }`.
            //
            // Nullish arguments (null/undefined) must still be checked under
            // strictNullChecks to surface real mismatches like `new Box<T>(null)`.

            // When the parameter is optional, implicitly include `undefined`
            // in the parameter type. This ensures `SomeType | undefined` can be
            // passed to an optional parameter of type `SomeType | null`, since
            // `SomeType | undefined <: SomeType | null | undefined`.
            let effective_param_type = {
                let param_info = self.param_info_for_arg_index(params, i);
                if param_info.is_some_and(|p| p.optional) {
                    self.interner.union2(param_type, TypeId::UNDEFINED)
                } else {
                    param_type
                }
            };
            let expanded_arg_type = if Self::get_contextual_signature(
                self.interner.as_type_database(),
                expanded_arg_type,
            )
            .is_some()
                && Self::get_contextual_signature(
                    self.interner.as_type_database(),
                    effective_param_type,
                )
                .is_some()
            {
                // For Callable types with generic call signatures (e.g.,
                // `declare function identity<T>(x: T): T`), convert to Function
                // before instantiation so the generic type params are properly
                // resolved against the target. `instantiate_generic_function_argument_against_target`
                // bails out for Callable types (to preserve class constructor
                // shapes), but for argument checking we need the instantiation.
                let arg_for_instantiation =
                    if let Some(crate::types::TypeData::Callable(shape_id)) =
                        self.interner.lookup(expanded_arg_type)
                    {
                        let shape = self.interner.callable_shape(shape_id);
                        if let Some(sig) = shape.call_signatures.first()
                            && !sig.type_params.is_empty()
                            && shape.call_signatures.len() == 1
                        {
                            self.interner.function(crate::types::FunctionShape {
                                type_params: sig.type_params.clone(),
                                params: sig.params.clone(),
                                this_type: sig.this_type,
                                return_type: sig.return_type,
                                type_predicate: sig.type_predicate,
                                is_constructor: false,
                                is_method: sig.is_method,
                            })
                        } else {
                            expanded_arg_type
                        }
                    } else {
                        expanded_arg_type
                    };
                self.instantiate_generic_function_argument_against_target(
                    arg_for_instantiation,
                    effective_param_type,
                )
            } else {
                expanded_arg_type
            };

            // Fast-path: skip the full assignability check when the arg type
            // matches either the declared or effective param type by identity.
            if expanded_arg_type == effective_param_type || expanded_arg_type == param_type {
                continue;
            }

            let use_bivariant_callbacks = (allow_bivariant_callbacks
                || self.force_bivariant_callbacks)
                && crate::type_queries::is_callable_type(self.interner, expanded_arg_type)
                && crate::type_queries::is_callable_type(self.interner, effective_param_type);
            if self.callback_requires_more_fixed_params_than_generic_rest_allows(
                expanded_arg_type,
                effective_param_type,
            ) {
                return Some(CallResult::ArgumentTypeMismatch {
                    index: i,
                    expected: param_type,
                    actual: *arg_type,
                    fallback_return: TypeId::ERROR,
                });
            }
            // Pre-check: reject callbacks where the source has more required
            // parameters than the target can accept. This must run before the
            // bivariant callback check because bivariance only relaxes parameter
            // TYPE checking, not parameter COUNT (arity) checking.
            if use_bivariant_callbacks
                && self.callback_source_has_excess_required_params(
                    expanded_arg_type,
                    effective_param_type,
                )
            {
                return Some(CallResult::ArgumentTypeMismatch {
                    index: i,
                    expected: param_type,
                    actual: *arg_type,
                    fallback_return: TypeId::ERROR,
                });
            }

            let assignable = if use_bivariant_callbacks {
                self.checker
                    .is_assignable_to_bivariant_callback(expanded_arg_type, effective_param_type)
            } else if strict {
                let result = self
                    .checker
                    .is_assignable_to_strict(expanded_arg_type, effective_param_type);
                if !result {
                    tracing::debug!(
                        "Strict assignability failed at index {}: {:?} <: {:?}",
                        i,
                        self.interner.lookup(expanded_arg_type),
                        self.interner.lookup(effective_param_type)
                    );
                }
                result
                    || self.is_assignable_via_contextual_signatures_strict(
                        expanded_arg_type,
                        effective_param_type,
                    )
            } else {
                self.checker
                    .is_assignable_to(expanded_arg_type, effective_param_type)
            };
            let assignable = assignable
                || (self.is_string_like_type(expanded_arg_type)
                    && self
                        .extract_iterable_yield_type(effective_param_type)
                        .is_some_and(|yield_type| {
                            self.checker.is_assignable_to(TypeId::STRING, yield_type)
                        }));
            if !assignable {
                return Some(CallResult::ArgumentTypeMismatch {
                    index: i,
                    expected: param_type,
                    actual: *arg_type,
                    // NOTE: fallback_return is ERROR here; the caller
                    // (resolve_function_call / resolve_union_call) overrides
                    // it with the actual return type when appropriate.
                    fallback_return: TypeId::ERROR,
                });
            }
        }
        None
    }

    /// Check if a parameter type contains `void` — either is `void` directly
    /// or is a union with `void` as a member (e.g., `number | void`).
    fn param_type_contains_void(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::VOID {
            return true;
        }
        if let Some(TypeData::Union(list_id)) = self.interner.lookup(type_id) {
            let members = self.interner.type_list(list_id);
            return members.contains(&TypeId::VOID);
        }
        false
    }

    fn tuple_element_contains_void(&self, elem: &TupleElement) -> bool {
        if elem.rest {
            return false;
        }
        self.param_type_contains_void(elem.type_id)
    }

    pub(crate) fn arg_count_bounds(&mut self, params: &[ParamInfo]) -> (usize, Option<usize>) {
        // Count required parameters, treating trailing `void`-containing params as optional.
        // In TypeScript, a parameter of type `void` (or union containing void like `number | void`)
        // can be omitted at the call site, but only if all subsequent params are also optional/void.
        // e.g., `f(x: number, y: void): void` → f(1) is valid (trailing void)
        //        `f(x: void, y: number): void` → f() is NOT valid (void before required)
        let non_rest_params: &[ParamInfo] = if params.last().is_some_and(|p| p.rest) {
            &params[..params.len() - 1]
        } else {
            params
        };
        // Find the rightmost required param that does NOT contain void.
        // Everything after it (void-containing or optional) is effectively optional.
        let required = non_rest_params
            .iter()
            .rposition(|p| p.is_required() && !self.param_type_contains_void(p.type_id))
            .map(|pos| pos + 1)
            .unwrap_or(0);
        let rest_param = params.last().filter(|param| param.rest);
        let Some(rest_param) = rest_param else {
            return (required, Some(params.len()));
        };

        let rest_param_type = self.unwrap_readonly(rest_param.type_id);
        // Evaluate Application/Conditional/Mapped types (e.g. Parameters<Fn>) to
        // their concrete Tuple form so arity checking works correctly.
        let rest_param_type = self.evaluate_rest_param_type(rest_param_type);
        match self.interner.lookup(rest_param_type) {
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                let (rest_min, rest_max) = self.tuple_length_bounds(&elements);
                let min = required + rest_min;
                let max = rest_max.map(|max| required + max);
                (min, max)
            }
            _ => (required, None),
        }
    }

    /// Look up the `ParamInfo` for a given argument index (non-rest only).
    /// Returns `None` if the index falls into a rest parameter or is out of bounds.
    fn param_info_for_arg_index<'b>(
        &self,
        params: &'b [ParamInfo],
        arg_index: usize,
    ) -> Option<&'b ParamInfo> {
        let rest_start = if params.last().is_some_and(|p| p.rest) {
            params.len().saturating_sub(1)
        } else {
            params.len()
        };
        if arg_index < rest_start {
            Some(&params[arg_index])
        } else {
            None
        }
    }

    pub(crate) fn param_type_for_arg_index(
        &mut self,
        params: &[ParamInfo],
        arg_index: usize,
        arg_count: usize,
    ) -> Option<TypeId> {
        let rest_param = params.last().filter(|param| param.rest);
        let rest_start = if rest_param.is_some() {
            params.len().saturating_sub(1)
        } else {
            params.len()
        };

        if arg_index < rest_start {
            return Some(params[arg_index].type_id);
        }

        let rest_param = rest_param?;
        let offset = arg_index - rest_start;
        let rest_arg_count = arg_count.saturating_sub(rest_start);

        let rest_param_type = self.unwrap_readonly(rest_param.type_id);
        // Evaluate Application/Mapped types (e.g., TupleMapper<[string, number]>) to
        // their concrete Array/Tuple form so rest parameter spreading works correctly.
        let rest_param_type = self.evaluate_rest_param_type(rest_param_type);
        trace!(
            rest_param_type_id = %rest_param_type.0,
            rest_param_type_key = ?self.interner.lookup(rest_param_type),
            "Extracting element type from rest parameter"
        );
        match self.interner.lookup(rest_param_type) {
            Some(TypeData::Array(elem)) => {
                trace!(
                    elem_type_id = %elem.0,
                    elem_type_key = ?self.interner.lookup(elem),
                    "Extracted array element type"
                );
                Some(elem)
            }
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                self.tuple_rest_element_type(&elements, offset, rest_arg_count)
            }
            Some(TypeData::Union(members)) => {
                let mut member_types = Vec::new();
                for &member in self.interner.type_list(members).iter() {
                    let member = self.unwrap_readonly(member);
                    let member = self.evaluate_rest_param_type(member);
                    match self.interner.lookup(member) {
                        Some(TypeData::Array(elem)) => member_types.push(elem),
                        Some(TypeData::Tuple(elements)) => {
                            let elements = self.interner.tuple_list(elements);
                            if let Some(ty) =
                                self.tuple_rest_element_type(&elements, offset, rest_arg_count)
                            {
                                member_types.push(ty);
                            }
                        }
                        _ => {}
                    }
                }
                if !member_types.is_empty() {
                    return Some(crate::utils::union_or_single(self.interner, member_types));
                }
                let extracted = crate::contextual::rest_argument_element_type(
                    self.interner,
                    self.checker.evaluate_type(rest_param_type),
                );
                if extracted != rest_param_type {
                    return Some(extracted);
                }
                Some(rest_param_type)
            }
            other => {
                let extracted = crate::contextual::rest_argument_element_type(
                    self.interner,
                    self.checker.evaluate_type(rest_param_type),
                );
                if extracted != rest_param_type {
                    trace!(
                        original_id = %rest_param_type.0,
                        extracted_id = %extracted.0,
                        extracted_key = ?self.interner.lookup(extracted),
                        "Extracted element type from rest wrapper fallback"
                    );
                    return Some(extracted);
                }
                trace!(?other, "Rest param is not Array or Tuple, returning as-is");
                Some(rest_param_type)
            }
        }
    }

    fn tuple_length_bounds(&self, elements: &[TupleElement]) -> (usize, Option<usize>) {
        let mut max = 0usize;
        let mut variadic = false;
        let mut fixed_elements = Vec::new();

        for elem in elements {
            if elem.rest {
                let expansion = self.expand_tuple_rest(elem.type_id);
                for fixed in expansion.fixed {
                    max += 1;
                    fixed_elements.push(fixed);
                }
                if expansion.variadic.is_some() {
                    variadic = true;
                }
                // Count tail elements from nested tuple spreads.
                // Required tail elements always count toward min, even
                // after a variadic rest. E.g. [...T[], Required] has min=1.
                for tail_elem in expansion.tail {
                    max += 1;
                    fixed_elements.push(tail_elem);
                }
                continue;
            }
            max += 1;
            fixed_elements.push(*elem);
        }

        let min = fixed_elements
            .iter()
            .rposition(|elem| !elem.optional && !self.tuple_element_contains_void(elem))
            .map(|pos| pos + 1)
            .unwrap_or(0);

        (min, if variadic { None } else { Some(max) })
    }

    fn tuple_rest_element_type(
        &self,
        elements: &[TupleElement],
        offset: usize,
        rest_arg_count: usize,
    ) -> Option<TypeId> {
        let rest_index = elements.iter().position(|elem| elem.rest);
        let Some(rest_index) = rest_index else {
            return elements.get(offset).map(|elem| elem.type_id);
        };

        let (prefix, rest_and_tail) = elements.split_at(rest_index);
        let rest_elem = &rest_and_tail[0];
        let outer_tail = &rest_and_tail[1..];

        let expansion = self.expand_tuple_rest(rest_elem.type_id);
        let prefix_len = prefix.len();
        let rest_fixed_len = expansion.fixed.len();
        let expansion_tail_len = expansion.tail.len();
        let outer_tail_len = outer_tail.len();
        // Total suffix = expansion.tail + outer_tail
        let total_suffix_len = expansion_tail_len + outer_tail_len;

        if let Some(variadic) = expansion.variadic {
            let suffix_start = rest_arg_count.saturating_sub(total_suffix_len);
            if offset >= suffix_start {
                let suffix_index = offset - suffix_start;
                // First check expansion.tail, then outer_tail
                if suffix_index < expansion_tail_len {
                    return Some(expansion.tail[suffix_index].type_id);
                }
                let outer_index = suffix_index - expansion_tail_len;
                return outer_tail.get(outer_index).map(|elem| elem.type_id);
            }
            if offset < prefix_len {
                return Some(prefix[offset].type_id);
            }
            let fixed_end = prefix_len + rest_fixed_len;
            if offset < fixed_end {
                return Some(expansion.fixed[offset - prefix_len].type_id);
            }
            return Some(variadic);
        }

        // No variadic: prefix + expansion.fixed + expansion.tail + outer_tail
        let mut index = offset;
        if index < prefix_len {
            return Some(prefix[index].type_id);
        }
        index -= prefix_len;
        if index < rest_fixed_len {
            return Some(expansion.fixed[index].type_id);
        }
        index -= rest_fixed_len;
        if index < expansion_tail_len {
            return Some(expansion.tail[index].type_id);
        }
        index -= expansion_tail_len;
        outer_tail.get(index).map(|elem| elem.type_id)
    }

    pub(crate) fn rest_element_type(&self, type_id: TypeId) -> TypeId {
        match self.interner.lookup(type_id) {
            Some(TypeData::Array(elem)) => elem,
            _ => type_id,
        }
    }

    /// Maximum iterations for type unwrapping loops to prevent infinite loops.
    const MAX_UNWRAP_ITERATIONS: usize = 1000;

    /// Evaluate a rest parameter type to resolve Application/Mapped types to their
    /// concrete Array/Tuple form. This is needed because after generic instantiation,
    /// the rest parameter type may be an Application like `TupleMapper<[string, number]>`
    /// which needs evaluation to become a Tuple like `[MyMappedType<string>, MyMappedType<number>]`.
    /// Without this, rest parameter spreading doesn't recognize the type as a tuple
    /// and treats it as a single parameter type.
    ///
    /// Uses the checker's `evaluate_type` which has access to the full `TypeResolver`,
    /// unlike `QueryDatabase::evaluate_type` which uses a `NoopResolver`.
    fn evaluate_rest_param_type(&mut self, type_id: TypeId) -> TypeId {
        match self.interner.lookup(type_id) {
            // Application, Mapped, Intersection, or Conditional types may evaluate to Array/Tuple
            Some(
                TypeData::Application(_)
                | TypeData::Mapped(_)
                | TypeData::Intersection(_)
                | TypeData::Conditional(_)
                | TypeData::Lazy(_),
            ) => {
                let evaluated = self.checker.evaluate_type(type_id);
                trace!(
                    original_id = %type_id.0,
                    evaluated_id = %evaluated.0,
                    evaluated_key = ?self.interner.lookup(evaluated),
                    "evaluate_rest_param_type: evaluated complex type"
                );
                evaluated
            }
            _ => type_id,
        }
    }

    pub(super) fn unwrap_readonly(&self, mut type_id: TypeId) -> TypeId {
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > Self::MAX_UNWRAP_ITERATIONS {
                // Safety limit reached - return current type to prevent infinite loop
                return type_id;
            }
            match self.interner.lookup(type_id) {
                Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
                    type_id = inner;
                }
                _ => return type_id,
            }
        }
    }

    pub(super) fn expand_tuple_rest(&self, type_id: TypeId) -> TupleRestExpansion {
        utils::expand_tuple_rest(self.interner, type_id)
    }

    pub(crate) fn rest_tuple_inference_target(
        &mut self,
        params: &[ParamInfo],
        arg_types: &[TypeId],
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
    ) -> Option<(usize, TypeId, TypeId)> {
        let rest_param = params.last().filter(|param| param.rest)?;
        let rest_start = params.len().saturating_sub(1);

        let rest_param_type = self.unwrap_readonly(rest_param.type_id);
        let target = match self.interner.lookup(rest_param_type) {
            Some(TypeData::TypeParameter(_)) if var_map.contains_key(&rest_param_type) => {
                Some((rest_start, rest_param_type, 0))
            }
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                elements.iter().enumerate().find_map(|(i, elem)| {
                    if !elem.rest {
                        return None;
                    }
                    if !var_map.contains_key(&elem.type_id) {
                        return None;
                    }

                    // Count trailing elements after the variadic part, but allow optional
                    // tail elements to be omitted when they don't match.
                    let tail = &elements[i + 1..];
                    let min_index = rest_start + i;
                    let mut trailing_count = 0usize;
                    let mut arg_index = arg_types.len();
                    for tail_elem in tail.iter().rev() {
                        if arg_index <= min_index {
                            break;
                        }
                        let arg_type = arg_types[arg_index - 1];
                        let assignable = self.checker.is_assignable_to(arg_type, tail_elem.type_id);
                        if tail_elem.optional && !assignable {
                            break;
                        }
                        trailing_count += 1;
                        arg_index -= 1;
                    }
                    Some((rest_start + i, elem.type_id, trailing_count))
                })
            }
            // Application rest param: e.g., `...args: TupleMapper<Tuple>` where Tuple
            // is an inference variable and TupleMapper is a mapped type alias.
            // Pack rest args into a tuple and constrain against the Application.
            // The constraint solver's (_, Application) handler will expand the alias
            // to its mapped type body, enabling reverse-mapped tuple inference.
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                let has_infer_arg = app.args.iter().any(|arg| var_map.contains_key(arg));
                if has_infer_arg {
                    Some((rest_start, rest_param_type, 0))
                } else {
                    None
                }
            }
            _ => None,
        }?;

        let (start_index, target_type, trailing_count) = target;
        if start_index >= arg_types.len() {
            return None;
        }

        // Extract the arguments that should be inferred for the variadic type parameter,
        // excluding both prefix fixed elements and trailing fixed elements.
        // For example, for `...args: [number, ...T, boolean]` with call `foo(1, 'a', 'b', true)`:
        //   - rest_start = 0 (rest param index)
        //   - start_index = 1 (after the prefix `number`)
        //   - trailing_count = 1 (the trailing `boolean`)
        //   - we should infer T from ['a', 'b'], not [1, 'a', 'b', true]
        //
        // The variadic arguments start at start_index and end before trailing elements.
        let end_index = arg_types.len().saturating_sub(trailing_count);
        let tuple_elements: Vec<TupleElement> = if start_index < end_index {
            arg_types[start_index..end_index]
                .iter()
                .map(|&ty| TupleElement {
                    type_id: ty,
                    name: None,
                    optional: false,
                    rest: false,
                })
                .collect()
        } else {
            Vec::new()
        };
        Some((
            start_index,
            target_type,
            self.interner.tuple(tuple_elements),
        ))
    }

    /// Check if a type evaluates to or contains a function type.
    /// This includes:
    /// - Direct Function or Callable types
    /// - Union/intersection members that evaluate to functions
    /// - Aliases/applications that only become callable after evaluation
    pub(crate) fn type_evaluates_to_function(&self, type_id: TypeId) -> bool {
        let mut visited = FxHashSet::default();
        self.type_evaluates_to_function_inner(type_id, &mut visited)
    }

    pub(crate) fn should_directly_constrain_same_base_application(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let evaluated_source = self.checker.evaluate_type(source);
        let evaluated_target = self.checker.evaluate_type(target);
        !self.type_evaluates_to_function(evaluated_source)
            && !self.type_evaluates_to_function(evaluated_target)
    }

    fn type_evaluates_to_function_inner(
        &self,
        type_id: TypeId,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if !visited.insert(type_id) {
            return false;
        }

        match self.interner.lookup(type_id) {
            Some(TypeData::Function(_) | TypeData::Callable(_)) => true,
            Some(TypeData::Union(members) | TypeData::Intersection(members)) => self
                .interner
                .type_list(members)
                .iter()
                .any(|&member| self.type_evaluates_to_function_inner(member, visited)),
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
                self.type_evaluates_to_function_inner(inner, visited)
            }
            _ => {
                let evaluated = self.interner.evaluate_type(type_id);
                evaluated != type_id && self.type_evaluates_to_function_inner(evaluated, visited)
            }
        }
    }

    /// Check if a function arg type contains `TypeParameter`s whose names match the
    /// caller's type parameter names (from the substitution). This detects when the
    /// checker's contextual typing leaked unresolved type parameters from overload
    /// signatures into arg types. Only checks function parameter positions, since
    /// those are the ones that cause inference poisoning in Round 1.
    pub(crate) fn arg_contains_callers_type_params(
        &self,
        arg_type: TypeId,
        substitution: &crate::instantiation::instantiate::TypeSubstitution,
    ) -> bool {
        if substitution.map().is_empty() {
            return false;
        }
        // Only check function types - the issue is specifically when contextual typing
        // leaks caller type params into a function arg's parameter types.
        match self.interner.lookup(arg_type) {
            Some(TypeData::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                shape.params.iter().any(|param| {
                    self.type_references_substitution_keys(param.type_id, substitution)
                })
            }
            _ => false,
        }
    }

    /// Recursively check if a type references any `TypeParameter` whose name is a key
    /// in the given substitution (i.e., one of the caller's type parameter names).
    fn type_references_substitution_keys(
        &self,
        ty: TypeId,
        substitution: &crate::instantiation::instantiate::TypeSubstitution,
    ) -> bool {
        match self.interner.lookup(ty) {
            Some(TypeData::TypeParameter(info)) => {
                // Check if this type parameter's name matches one of the caller's type params
                let name = self.interner.resolve_atom(info.name);
                if name.as_str().starts_with("__infer_") {
                    return false; // Inference placeholders are fine
                }
                substitution.map().contains_key(&info.name)
            }
            Some(TypeData::Union(members)) | Some(TypeData::Intersection(members)) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|&m| self.type_references_substitution_keys(m, substitution))
            }
            Some(TypeData::Array(elem)) => {
                self.type_references_substitution_keys(elem, substitution)
            }
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                elements
                    .iter()
                    .any(|e| self.type_references_substitution_keys(e.type_id, substitution))
            }
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                app.args
                    .iter()
                    .any(|&a| self.type_references_substitution_keys(a, substitution))
            }
            _ => false,
        }
    }

    #[inline]
    pub(crate) fn type_contains_placeholder(
        &self,
        ty: TypeId,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if var_map.contains_key(&ty) {
            return true;
        }
        // Fast path: intrinsic types (primitives, never, any, etc.) never contain placeholders
        if ty.is_intrinsic() {
            return false;
        }
        if !visited.insert(ty) {
            return false;
        }

        let key = match self.interner.lookup(ty) {
            Some(key) => key,
            None => return false,
        };

        match key {
            TypeData::Array(elem) => self.type_contains_placeholder(elem, var_map, visited),
            TypeData::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                elements
                    .iter()
                    .any(|elem| self.type_contains_placeholder(elem.type_id, var_map, visited))
            }
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|&member| self.type_contains_placeholder(member, var_map, visited))
            }
            TypeData::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_placeholder(prop.type_id, var_map, visited))
            }
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_placeholder(prop.type_id, var_map, visited))
                    || shape.string_index.as_ref().is_some_and(|idx| {
                        self.type_contains_placeholder(idx.key_type, var_map, visited)
                            || self.type_contains_placeholder(idx.value_type, var_map, visited)
                    })
                    || shape.number_index.as_ref().is_some_and(|idx| {
                        self.type_contains_placeholder(idx.key_type, var_map, visited)
                            || self.type_contains_placeholder(idx.value_type, var_map, visited)
                    })
            }
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.type_contains_placeholder(app.base, var_map, visited)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.type_contains_placeholder(arg, var_map, visited))
            }
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                shape.type_params.iter().any(|tp| {
                    tp.constraint.is_some_and(|constraint| {
                        self.type_contains_placeholder(constraint, var_map, visited)
                    }) || tp.default.is_some_and(|default| {
                        self.type_contains_placeholder(default, var_map, visited)
                    })
                }) || shape
                    .params
                    .iter()
                    .any(|param| self.type_contains_placeholder(param.type_id, var_map, visited))
                    || shape.this_type.is_some_and(|this_type| {
                        self.type_contains_placeholder(this_type, var_map, visited)
                    })
                    || self.type_contains_placeholder(shape.return_type, var_map, visited)
                    || shape.type_predicate.as_ref().is_some_and(|pred| {
                        pred.type_id
                            .is_some_and(|ty| self.type_contains_placeholder(ty, var_map, visited))
                    })
            }
            TypeData::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                let in_call = shape.call_signatures.iter().any(|sig| {
                    sig.type_params.iter().any(|tp| {
                        tp.constraint.is_some_and(|constraint| {
                            self.type_contains_placeholder(constraint, var_map, visited)
                        }) || tp.default.is_some_and(|default| {
                            self.type_contains_placeholder(default, var_map, visited)
                        })
                    }) || sig.params.iter().any(|param| {
                        self.type_contains_placeholder(param.type_id, var_map, visited)
                    }) || sig.this_type.is_some_and(|this_type| {
                        self.type_contains_placeholder(this_type, var_map, visited)
                    }) || self.type_contains_placeholder(sig.return_type, var_map, visited)
                        || sig.type_predicate.as_ref().is_some_and(|pred| {
                            pred.type_id.is_some_and(|ty| {
                                self.type_contains_placeholder(ty, var_map, visited)
                            })
                        })
                });
                if in_call {
                    return true;
                }
                let in_construct = shape.construct_signatures.iter().any(|sig| {
                    sig.type_params.iter().any(|tp| {
                        tp.constraint.is_some_and(|constraint| {
                            self.type_contains_placeholder(constraint, var_map, visited)
                        }) || tp.default.is_some_and(|default| {
                            self.type_contains_placeholder(default, var_map, visited)
                        })
                    }) || sig.params.iter().any(|param| {
                        self.type_contains_placeholder(param.type_id, var_map, visited)
                    }) || sig.this_type.is_some_and(|this_type| {
                        self.type_contains_placeholder(this_type, var_map, visited)
                    }) || self.type_contains_placeholder(sig.return_type, var_map, visited)
                        || sig.type_predicate.as_ref().is_some_and(|pred| {
                            pred.type_id.is_some_and(|ty| {
                                self.type_contains_placeholder(ty, var_map, visited)
                            })
                        })
                });
                if in_construct {
                    return true;
                }
                shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_placeholder(prop.type_id, var_map, visited))
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.get_conditional(cond_id);
                self.type_contains_placeholder(cond.check_type, var_map, visited)
                    || self.type_contains_placeholder(cond.extends_type, var_map, visited)
                    || self.type_contains_placeholder(cond.true_type, var_map, visited)
                    || self.type_contains_placeholder(cond.false_type, var_map, visited)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.get_mapped(mapped_id);
                mapped.type_param.constraint.is_some_and(|constraint| {
                    self.type_contains_placeholder(constraint, var_map, visited)
                }) || mapped.type_param.default.is_some_and(|default| {
                    self.type_contains_placeholder(default, var_map, visited)
                }) || self.type_contains_placeholder(mapped.constraint, var_map, visited)
                    || self.type_contains_placeholder(mapped.template, var_map, visited)
            }
            TypeData::IndexAccess(obj, idx) => {
                self.type_contains_placeholder(obj, var_map, visited)
                    || self.type_contains_placeholder(idx, var_map, visited)
            }
            TypeData::KeyOf(operand)
            | TypeData::ReadonlyType(operand)
            | TypeData::NoInfer(operand) => {
                self.type_contains_placeholder(operand, var_map, visited)
            }
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => {
                        self.type_contains_placeholder(*inner, var_map, visited)
                    }
                })
            }
            TypeData::StringIntrinsic { type_arg, .. } => {
                self.type_contains_placeholder(type_arg, var_map, visited)
            }
            TypeData::Enum(_def_id, member_type) => {
                self.type_contains_placeholder(member_type, var_map, visited)
            }
            TypeData::TypeParameter(_)
            | TypeData::Infer(_)
            | TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::BoundParameter(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ThisType
            | TypeData::ModuleNamespace(_)
            | TypeData::Error => false,
        }
    }

    /// Check if a type is contextually sensitive (requires contextual typing for inference).
    ///
    /// Contextually sensitive types include:
    /// - Function types (lambda expressions)
    /// - Callable types (object with call signatures)
    /// - Union/Intersection types containing contextually sensitive members
    /// - Object literals with callable properties (methods)
    ///
    /// These types need deferred inference in Round 2 after non-contextual
    /// arguments have been processed and type variables have been fixed.
    pub(crate) fn is_contextually_sensitive(&self, type_id: TypeId) -> bool {
        // Check memoization cache to avoid exponential re-traversal on deeply
        // nested type structures (e.g., Application chains where each level
        // references the previous type multiple times via keyof).
        if let Some(&cached) = self.contextual_sensitivity_cache.borrow().get(&type_id) {
            return cached;
        }
        let result = self.is_contextually_sensitive_inner(type_id);
        self.contextual_sensitivity_cache
            .borrow_mut()
            .insert(type_id, result);
        result
    }

    fn is_contextually_sensitive_inner(&self, type_id: TypeId) -> bool {
        let key = match self.interner.lookup(type_id) {
            Some(key) => key,
            None => return false,
        };

        match key {
            // Function types are contextually sensitive only when one of their
            // parameter types still needs contextual typing (has `any` type or
            // inference placeholder). Fully annotated function arguments --
            // including generic function references like `id<T>(x: T) => T` --
            // should participate in Round 1 generic inference.
            //
            // In tsc, contextual sensitivity is an AST-level check
            // (isContextSensitive) that looks at whether the expression is a
            // function expression/arrow with unannotated parameters. A simple
            // identifier referring to a generic function is NOT contextually
            // sensitive. We approximate this by only checking parameter types
            // for placeholder/any markers, not the presence of type_params.
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                self.function_signature_is_contextually_sensitive(&shape.params)
            }
            // Union/Intersection: contextually sensitive if any member is
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|&member| self.is_contextually_sensitive(member))
            }

            // Object types: only fresh object literals can be contextually sensitive.
            // Non-fresh objects (class instances, evaluated generic types like Set<T>)
            // are never contextually sensitive — their types are already determined.
            // This matches tsc's isContextSensitive which checks the AST expression,
            // not the type: variable references are never contextually sensitive.
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .flags
                    .contains(crate::types::ObjectFlags::FRESH_LITERAL)
                    && shape
                        .properties
                        .iter()
                        .any(|prop| self.is_contextually_sensitive(prop.type_id))
            }

            // Array types: check element type
            TypeData::Array(elem) => self.is_contextually_sensitive(elem),

            // Tuple types: check all elements
            TypeData::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                elements
                    .iter()
                    .any(|elem| self.is_contextually_sensitive(elem.type_id))
            }

            // Type applications: check base and arguments
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.is_contextually_sensitive(app.base)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.is_contextually_sensitive(arg))
            }

            // Readonly types: look through to inner type
            TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                self.is_contextually_sensitive(inner)
            }

            // Type parameters with constraints: check constraint
            TypeData::TypeParameter(info) | TypeData::Infer(info) => info
                .constraint
                .is_some_and(|constraint| self.is_contextually_sensitive(constraint)),

            // Index access: check both object and key types
            TypeData::IndexAccess(obj, key) => {
                self.is_contextually_sensitive(obj) || self.is_contextually_sensitive(key)
            }

            // Conditional types: check all branches
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.get_conditional(cond_id);
                self.is_contextually_sensitive(cond.check_type)
                    || self.is_contextually_sensitive(cond.extends_type)
                    || self.is_contextually_sensitive(cond.true_type)
                    || self.is_contextually_sensitive(cond.false_type)
            }

            // Mapped types: check constraint and template
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.get_mapped(mapped_id);
                self.is_contextually_sensitive(mapped.constraint)
                    || self.is_contextually_sensitive(mapped.template)
            }

            // KeyOf, StringIntrinsic: check operand
            TypeData::KeyOf(operand)
            | TypeData::StringIntrinsic {
                type_arg: operand, ..
            } => self.is_contextually_sensitive(operand),

            // Enum types: check member type
            TypeData::Enum(_def_id, member_type) => self.is_contextually_sensitive(member_type),

            // Template literals: check type spans
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => self.is_contextually_sensitive(*inner),
                })
            }

            // Non-contextually sensitive types (Callable = class constructor values)
            TypeData::Callable(_)
            | TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::BoundParameter(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ThisType
            | TypeData::ModuleNamespace(_)
            | TypeData::Error => false,
        }
    }
}
