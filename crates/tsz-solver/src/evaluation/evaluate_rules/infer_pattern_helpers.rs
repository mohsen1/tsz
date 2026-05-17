//! Type-specific infer pattern matching helpers.
//!
//! Contains specialized pattern matchers for different type structures:
//! - Function type patterns
//! - Constructor type patterns
//! - Callable type patterns
//! - Object type patterns
//! - Object with index patterns
//! - Union type patterns
//! - Template literal patterns

use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::types::{
    CallableShapeId, FunctionShape, FunctionShapeId, IntrinsicKind, LiteralValue, ObjectShapeId,
    ParamInfo, TemplateSpan, TupleElement, TypeData, TypeId, TypeListId, TypeParamInfo,
};
use crate::utils;
use crate::visitor::array_element_type;
use crate::{TypeSubstitution, instantiate_type};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::interner::Atom;

use super::super::evaluate::TypeEvaluator;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    pub(crate) fn implicit_sequence_property_type(
        &self,
        source: TypeId,
        prop_name: Atom,
    ) -> Option<TypeId> {
        if self.interner().resolve_atom_ref(prop_name).as_ref() != "length" {
            return None;
        }

        let source = match self.interner().lookup(source) {
            Some(TypeData::ReadonlyType(inner)) => inner,
            _ => source,
        };

        match self.interner().lookup(source) {
            Some(TypeData::Tuple(elements_id)) => {
                let elements = self.interner().tuple_list(elements_id);
                if elements.iter().any(|element| element.rest) {
                    Some(TypeId::NUMBER)
                } else {
                    Some(self.interner().literal_number(elements.len() as f64))
                }
            }
            // Arrays and string types all have `length: number`. String.prototype.length
            // is typed as `number`, so tsc infers `number` even for concrete string literals.
            Some(
                TypeData::Array(_)
                | TypeData::Intrinsic(IntrinsicKind::String)
                | TypeData::Literal(LiteralValue::String(_))
                | TypeData::TemplateLiteral(_),
            ) => Some(TypeId::NUMBER),
            _ => None,
        }
    }

    fn parse_template_number_capture(&self, captured: &str) -> Option<TypeId> {
        let value = if let Some(digits) = captured.strip_prefix("0x") {
            u64::from_str_radix(digits, 16).ok().map(|n| n as f64)?
        } else if let Some(digits) = captured.strip_prefix("0X") {
            u64::from_str_radix(digits, 16).ok().map(|n| n as f64)?
        } else if let Some(digits) = captured.strip_prefix("0o") {
            u64::from_str_radix(digits, 8).ok().map(|n| n as f64)?
        } else if let Some(digits) = captured.strip_prefix("0O") {
            u64::from_str_radix(digits, 8).ok().map(|n| n as f64)?
        } else if let Some(digits) = captured.strip_prefix("0b") {
            u64::from_str_radix(digits, 2).ok().map(|n| n as f64)?
        } else if let Some(digits) = captured.strip_prefix("0B") {
            u64::from_str_radix(digits, 2).ok().map(|n| n as f64)?
        } else {
            captured.parse::<f64>().ok()?
        };

        if !value.is_finite() {
            return None;
        }

        let literal = self.interner().literal_number(value);
        let round_trips = match value {
            v if v.fract() == 0.0 && v.abs() < 1e15 => (v as i64).to_string() == captured,
            v => format!("{v}") == captured,
        };
        Some(if round_trips { literal } else { TypeId::NUMBER })
    }

    fn parse_template_bigint_capture(&self, captured: &str) -> Option<TypeId> {
        let (negative, digits) = captured
            .strip_prefix('-')
            .map_or((false, captured), |rest| (true, rest));
        if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }

        Some(self.interner().literal_bigint_with_sign(negative, digits))
    }

    fn template_capture_for_constraint(
        &self,
        captured: &str,
        captured_type: TypeId,
        constraint: TypeId,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> Option<TypeId> {
        if checker.is_subtype_of(captured_type, constraint) {
            return Some(captured_type);
        }

        match self.interner().lookup(constraint) {
            Some(TypeData::Intrinsic(IntrinsicKind::Number)) => self
                .parse_template_number_capture(captured)
                .filter(|&ty| checker.is_subtype_of(ty, constraint)),
            Some(TypeData::Intrinsic(IntrinsicKind::Bigint)) => self
                .parse_template_bigint_capture(captured)
                .filter(|&ty| checker.is_subtype_of(ty, constraint)),
            Some(TypeData::Intrinsic(IntrinsicKind::Boolean)) => match captured {
                "true" => Some(self.interner().literal_boolean(true)),
                "false" => Some(self.interner().literal_boolean(false)),
                _ => None,
            },
            Some(TypeData::Intrinsic(IntrinsicKind::Null)) if captured == "null" => {
                Some(TypeId::NULL)
            }
            Some(TypeData::Intrinsic(IntrinsicKind::Undefined)) if captured == "undefined" => {
                Some(TypeId::UNDEFINED)
            }
            Some(TypeData::Union(members_id)) => {
                let members = self.interner().type_list(members_id);
                members.iter().find_map(|&member| {
                    self.template_capture_for_constraint(captured, captured_type, member, checker)
                        .filter(|&ty| checker.is_subtype_of(ty, constraint))
                })
            }
            _ => None,
        }
    }

    fn bind_template_infer_capture(
        &self,
        info: &TypeParamInfo,
        captured: &str,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let captured_type = self.interner().literal_string(captured);
        let inferred = if let Some(constraint) = info.constraint {
            let Some(converted) =
                self.template_capture_for_constraint(captured, captured_type, constraint, checker)
            else {
                return false;
            };
            converted
        } else {
            captured_type
        };

        self.bind_infer(info, inferred, bindings, checker)
    }

    fn erase_type_params_to_constraints(
        &self,
        type_params: &[TypeParamInfo],
    ) -> Option<TypeSubstitution> {
        if type_params.is_empty() {
            return None;
        }

        let mut subst = TypeSubstitution::new();
        for tp in type_params {
            subst.insert(tp.name, tp.constraint.unwrap_or(TypeId::UNKNOWN));
        }
        Some(subst)
    }

    fn erase_return_type_for_infer(
        &self,
        return_type: TypeId,
        type_params: &[TypeParamInfo],
    ) -> TypeId {
        let Some(subst) = self.erase_type_params_to_constraints(type_params) else {
            return return_type;
        };
        instantiate_type(self.interner(), return_type, &subst)
    }

    fn instantiate_signature_for_infer(
        &self,
        params: &[ParamInfo],
        return_type: TypeId,
        type_params: &[TypeParamInfo],
    ) -> (Vec<ParamInfo>, TypeId) {
        let Some(subst) = self.erase_type_params_to_constraints(type_params) else {
            return (params.to_vec(), return_type);
        };

        let params = params
            .iter()
            .map(|param| ParamInfo {
                name: param.name,
                type_id: instantiate_type(self.interner(), param.type_id, &subst),
                optional: param.optional,
                rest: param.rest,
            })
            .collect();
        let return_type = instantiate_type(self.interner(), return_type, &subst);
        (params, return_type)
    }

    fn match_rest_infer_tuple(
        &self,
        source_params: &[ParamInfo],
        infer_ty: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        // Cases (left side is the source signature, right side is the pattern
        // `(...args: infer R)`):
        //
        // 1. `(...args: T)` — single rest param. Bind R = T directly.
        // 2. `(a: A, b: B)` — only fixed params. Bind R = [A, B] (a tuple).
        // 3. `(head: V, ...args: T)` — mixed fixed+rest. Build a variadic
        //    tuple `[V, ...T]` (preserving each param's `rest` flag) and
        //    recurse so `Length<R>` and tuple-traversal queries correctly
        //    walk into the rest element.
        let source_tuple_or_array = if source_params.len() == 1 && source_params[0].rest {
            source_params[0].type_id
        } else {
            // Build a tuple preserving each param's `rest` flag so variadic
            // elements remain spreadable and `fixed_length()` traverses into
            // them. This handles both the all-fixed case and the mixed
            // fixed+rest case in one branch.
            let tuple_elems: Vec<TupleElement> = source_params
                .iter()
                .map(|p| TupleElement {
                    type_id: p.type_id,
                    name: p.name,
                    optional: p.optional,
                    rest: p.rest,
                })
                .collect();
            self.interner().tuple(tuple_elems)
        };
        let mut local_visited = FxHashSet::default();
        self.match_infer_pattern(
            source_tuple_or_array,
            infer_ty,
            bindings,
            &mut local_visited,
            checker,
        )
    }

    fn match_signature_params_for_infer(
        &self,
        source_params: &[ParamInfo],
        pattern_params: &[ParamInfo],
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        // A source function `(...args: [A, B]) => R` is structurally equivalent
        // to `(a: A, b: B) => R` for infer matching.  Expand before the
        // per-param loop so `(first: infer F, ...rest: infer Rest)` correctly
        // binds F = A and Rest = [B] instead of F = [A, B] and Rest = [].
        // Guard on get_tuple_elements to skip the Vec allocation for non-tuple
        // rest params such as `...args: string[]`.
        let expanded: Vec<ParamInfo>;
        let source_params = if source_params.len() == 1
            && source_params[0].rest
            && crate::type_queries::get_tuple_elements(self.interner(), source_params[0].type_id)
                .is_some()
        {
            expanded = crate::type_queries::unpack_tuple_rest_parameter(
                self.interner(),
                &source_params[0],
            );
            &expanded
        } else {
            source_params
        };

        let trailing_rest_param = pattern_params.last().filter(|param| param.rest);
        let fixed_param_count = if trailing_rest_param.is_some() {
            pattern_params.len().saturating_sub(1)
        } else {
            pattern_params.len()
        };

        if source_params.len() < fixed_param_count {
            return false;
        }

        let mut local_visited = FxHashSet::default();
        for (source_param, pattern_param) in source_params
            .iter()
            .take(fixed_param_count)
            .zip(pattern_params.iter().take(fixed_param_count))
        {
            let source_param_type = if source_param.optional {
                crate::narrowing::remove_nullish(self.interner(), source_param.type_id)
            } else {
                source_param.type_id
            };
            if !self.match_infer_pattern(
                source_param_type,
                pattern_param.type_id,
                bindings,
                &mut local_visited,
                checker,
            ) {
                return false;
            }
        }

        if let Some(rest_param) = trailing_rest_param {
            let remaining_params = &source_params[fixed_param_count..];
            if self.type_contains_infer(rest_param.type_id) {
                if !self.match_rest_infer_tuple(
                    remaining_params,
                    rest_param.type_id,
                    bindings,
                    checker,
                ) {
                    return false;
                }
            } else {
                // Fixed source params match against the element type of the rest array
                // (e.g. `number` vs element of `unknown[]`); rest source params match
                // array-to-array since those slots align at the rest level.
                let rest_elem_type = array_element_type(self.interner(), rest_param.type_id)
                    .unwrap_or(rest_param.type_id);
                let mut local_visited = FxHashSet::default();
                for source_param in remaining_params {
                    let source_param_type = if source_param.optional {
                        crate::narrowing::remove_nullish(self.interner(), source_param.type_id)
                    } else {
                        source_param.type_id
                    };
                    let pattern_type = if source_param.rest {
                        rest_param.type_id
                    } else {
                        rest_elem_type
                    };
                    if !self.match_infer_pattern(
                        source_param_type,
                        pattern_type,
                        bindings,
                        &mut local_visited,
                        checker,
                    ) {
                        return false;
                    }
                }
            }
        }

        true
    }

    pub(crate) fn match_infer_function_pattern(
        &self,
        source: TypeId,
        pattern_fn_id: FunctionShapeId,
        pattern: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        _visited: &mut FxHashSet<(TypeId, TypeId)>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let pattern_fn = self.interner().function_shape(pattern_fn_id);
        let has_param_infer = pattern_fn
            .params
            .iter()
            .any(|param| self.type_contains_infer(param.type_id));
        let has_return_infer = self.type_contains_infer(pattern_fn.return_type);
        let has_single_rest_infer = pattern_fn.params.len() == 1
            && pattern_fn.params[0].rest
            && self.type_contains_infer(pattern_fn.params[0].type_id);

        if pattern_fn.this_type.is_none() && has_param_infer && has_return_infer {
            let mut match_params_and_return = |_source_type: TypeId,
                                               source_params: &[ParamInfo],
                                               source_return: TypeId,
                                               bindings: &mut FxHashMap<Atom, TypeId>|
             -> bool {
                let mut local_visited = FxHashSet::default();
                if has_single_rest_infer {
                    if !self.match_rest_infer_tuple(
                        source_params,
                        pattern_fn.params[0].type_id,
                        bindings,
                        checker,
                    ) {
                        return false;
                    }
                } else if !self.match_signature_params_for_infer(
                    source_params,
                    &pattern_fn.params,
                    bindings,
                    checker,
                ) {
                    return false;
                }
                if !self.match_infer_pattern(
                    source_return,
                    pattern_fn.return_type,
                    bindings,
                    &mut local_visited,
                    checker,
                ) {
                    return false;
                }
                // For infer pattern matching, once parameters and return type match successfully,
                // the pattern is considered successful. The final subtype check is too strict
                // because of function parameter contravariance (e.g., any vs concrete type).
                // We've already matched the signature components above, which is sufficient.
                true
            };

            return match self.interner().lookup(source) {
                Some(TypeData::Intrinsic(crate::types::IntrinsicKind::Function)) => {
                    // Function intrinsic is structurally (...args: any[]) => any
                    let function_params = vec![crate::types::ParamInfo {
                        name: None,
                        type_id: TypeId::ANY,
                        optional: false,
                        rest: true,
                    }];
                    match_params_and_return(source, &function_params, TypeId::ANY, bindings)
                }
                Some(TypeData::Function(source_fn_id)) => {
                    let source_fn = self.interner().function_shape(source_fn_id);
                    let (params, return_type) = self.instantiate_signature_for_infer(
                        &source_fn.params,
                        source_fn.return_type,
                        &source_fn.type_params,
                    );
                    match_params_and_return(source, &params, return_type, bindings)
                }
                Some(TypeData::Callable(source_shape_id)) => {
                    // Match against the last call signature (TypeScript behavior)
                    let source_shape = self.interner().callable_shape(source_shape_id);
                    if source_shape.call_signatures.is_empty() {
                        return false;
                    }
                    // Use the last call signature (TypeScript's behavior for overloads)
                    // Safe to use last() here as we've verified the vector is not empty
                    let source_sig = match source_shape.call_signatures.last() {
                        Some(sig) => sig,
                        None => return false,
                    };
                    let (params, return_type) = self.instantiate_signature_for_infer(
                        &source_sig.params,
                        source_sig.return_type,
                        &source_sig.type_params,
                    );
                    match_params_and_return(source, &params, return_type, bindings)
                }
                Some(TypeData::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let mut member_bindings = FxHashMap::default();
                        match self.interner().lookup(member) {
                            Some(TypeData::Function(source_fn_id)) => {
                                let source_fn = self.interner().function_shape(source_fn_id);
                                let (params, return_type) = self.instantiate_signature_for_infer(
                                    &source_fn.params,
                                    source_fn.return_type,
                                    &source_fn.type_params,
                                );
                                if !match_params_and_return(
                                    member,
                                    &params,
                                    return_type,
                                    &mut member_bindings,
                                ) {
                                    return false;
                                }
                            }
                            Some(TypeData::Callable(source_shape_id)) => {
                                let source_shape = self.interner().callable_shape(source_shape_id);
                                if source_shape.call_signatures.is_empty() {
                                    return false;
                                }
                                // Safe to use last() here as we've verified the vector is not empty
                                let source_sig = match source_shape.call_signatures.last() {
                                    Some(sig) => sig,
                                    None => return false,
                                };
                                let (params, return_type) = self.instantiate_signature_for_infer(
                                    &source_sig.params,
                                    source_sig.return_type,
                                    &source_sig.type_params,
                                );
                                if !match_params_and_return(
                                    member,
                                    &params,
                                    return_type,
                                    &mut member_bindings,
                                ) {
                                    return false;
                                }
                            }
                            _ => return false,
                        }
                        for (name, ty) in member_bindings {
                            combined
                                .entry(name)
                                .and_modify(|existing| {
                                    *existing = self.interner().union2(*existing, ty);
                                })
                                .or_insert(ty);
                        }
                    }
                    bindings.extend(combined);
                    true
                }
                Some(TypeData::Object(_)) | Some(TypeData::ObjectWithIndex(_))
                    if crate::type_queries::is_function_interface_structural(
                        self.interner(),
                        source,
                    ) =>
                {
                    let function_params = vec![crate::types::ParamInfo {
                        name: None,
                        type_id: TypeId::ANY,
                        optional: false,
                        rest: true,
                    }];
                    match_params_and_return(source, &function_params, TypeId::ANY, bindings)
                }
                _ => false,
            };
        }

        if pattern_fn.this_type.is_none() && has_param_infer && !has_return_infer {
            if pattern_fn.is_constructor {
                return self.match_infer_constructor_pattern(
                    source,
                    &pattern_fn,
                    bindings,
                    checker,
                );
            }

            let has_single_rest_infer = pattern_fn.params.len() == 1
                && pattern_fn.params[0].rest
                && self.type_contains_infer(pattern_fn.params[0].type_id);

            if has_single_rest_infer {
                let infer_ty = pattern_fn.params[0].type_id;
                let mut match_params_tuple = |source_params: &[ParamInfo],
                                              source_type_params: &[TypeParamInfo],
                                              bindings: &mut FxHashMap<Atom, TypeId>|
                 -> bool {
                    let mut local_visited = FxHashSet::default();
                    let erased_subst = self.erase_type_params_to_constraints(source_type_params);

                    if source_params.len() == 1 && source_params[0].rest {
                        let source_param = &source_params[0];
                        let source_param_type = if let Some(subst) = &erased_subst {
                            instantiate_type(self.interner(), source_param.type_id, subst)
                        } else {
                            source_param.type_id
                        };
                        let source_param_type = if source_param.optional {
                            self.interner().union2(source_param_type, TypeId::UNDEFINED)
                        } else {
                            source_param_type
                        };
                        return self.match_infer_pattern(
                            source_param_type,
                            infer_ty,
                            bindings,
                            &mut local_visited,
                            checker,
                        );
                    }

                    let tuple_elems: Vec<TupleElement> = source_params
                        .iter()
                        .map(|param| TupleElement {
                            type_id: if let Some(subst) = &erased_subst {
                                instantiate_type(self.interner(), param.type_id, subst)
                            } else {
                                param.type_id
                            },
                            name: param.name,
                            optional: param.optional,
                            rest: param.rest,
                        })
                        .collect();
                    let tuple_ty = self.interner().tuple(tuple_elems);
                    self.match_infer_pattern(
                        tuple_ty,
                        infer_ty,
                        bindings,
                        &mut local_visited,
                        checker,
                    )
                };

                return match self.interner().lookup(source) {
                    Some(TypeData::Intrinsic(crate::types::IntrinsicKind::Function)) => {
                        // Function intrinsic is structurally (...args: any[]) => any
                        let function_params = vec![crate::types::ParamInfo {
                            name: None,
                            type_id: TypeId::ANY,
                            optional: false,
                            rest: true,
                        }];
                        match_params_tuple(&function_params, &[], bindings)
                    }
                    Some(TypeData::Function(source_fn_id)) => {
                        let source_fn = self.interner().function_shape(source_fn_id);
                        match_params_tuple(&source_fn.params, &source_fn.type_params, bindings)
                    }
                    Some(TypeData::Callable(source_shape_id)) => {
                        let source_shape = self.interner().callable_shape(source_shape_id);
                        if source_shape.call_signatures.is_empty() {
                            return false;
                        }
                        let source_sig = source_shape
                            .call_signatures
                            .last()
                            .expect("call_signatures checked non-empty above");
                        match_params_tuple(&source_sig.params, &source_sig.type_params, bindings)
                    }
                    Some(TypeData::Union(members)) => {
                        let members = self.interner().type_list(members);
                        let mut combined = FxHashMap::default();
                        for &member in members.iter() {
                            let mut member_bindings = FxHashMap::default();
                            match self.interner().lookup(member) {
                                Some(TypeData::Function(source_fn_id)) => {
                                    let source_fn = self.interner().function_shape(source_fn_id);
                                    if !match_params_tuple(
                                        &source_fn.params,
                                        &source_fn.type_params,
                                        &mut member_bindings,
                                    ) {
                                        return false;
                                    }
                                }
                                Some(TypeData::Callable(source_shape_id)) => {
                                    let source_shape =
                                        self.interner().callable_shape(source_shape_id);
                                    if source_shape.call_signatures.is_empty() {
                                        return false;
                                    }
                                    let source_sig = source_shape
                                        .call_signatures
                                        .last()
                                        .expect("call_signatures checked non-empty above");
                                    if !match_params_tuple(
                                        &source_sig.params,
                                        &source_sig.type_params,
                                        &mut member_bindings,
                                    ) {
                                        return false;
                                    }
                                }
                                _ => return false,
                            }
                            for (name, ty) in member_bindings {
                                combined
                                    .entry(name)
                                    .and_modify(|existing| {
                                        *existing = self.interner().union2(*existing, ty);
                                    })
                                    .or_insert(ty);
                            }
                        }
                        bindings.extend(combined);
                        true
                    }
                    Some(TypeData::Object(_)) | Some(TypeData::ObjectWithIndex(_))
                        if crate::type_queries::is_function_interface_structural(
                            self.interner(),
                            source,
                        ) =>
                    {
                        let function_params = vec![crate::types::ParamInfo {
                            name: None,
                            type_id: TypeId::ANY,
                            optional: false,
                            rest: true,
                        }];
                        match_params_tuple(&function_params, &[], bindings)
                    }
                    _ => false,
                };
            }

            // Regular function parameter inference
            let mut match_function_params = |_source_type: TypeId,
                                             source_fn_id: FunctionShapeId,
                                             bindings: &mut FxHashMap<Atom, TypeId>|
             -> bool {
                let source_fn = self.interner().function_shape(source_fn_id);
                if has_single_rest_infer {
                    return self.match_rest_infer_tuple(
                        &source_fn.params,
                        pattern_fn.params[0].type_id,
                        bindings,
                        checker,
                    );
                }
                self.match_signature_params_for_infer(
                    &source_fn.params,
                    &pattern_fn.params,
                    bindings,
                    checker,
                )
            };

            return match self.interner().lookup(source) {
                Some(TypeData::Function(source_fn_id)) => {
                    match_function_params(source, source_fn_id, bindings)
                }
                Some(TypeData::Callable(source_shape_id)) => {
                    // Match against the last call signature (TypeScript behavior for overloads)
                    let source_shape = self.interner().callable_shape(source_shape_id);
                    if source_shape.call_signatures.is_empty() {
                        return false;
                    }
                    let source_sig = source_shape
                        .call_signatures
                        .last()
                        .expect("call_signatures checked non-empty above");
                    if has_single_rest_infer {
                        return self.match_rest_infer_tuple(
                            &source_sig.params,
                            pattern_fn.params[0].type_id,
                            bindings,
                            checker,
                        );
                    }
                    self.match_signature_params_for_infer(
                        &source_sig.params,
                        &pattern_fn.params,
                        bindings,
                        checker,
                    )
                }
                Some(TypeData::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let Some(TypeData::Function(source_fn_id)) = self.interner().lookup(member)
                        else {
                            return false;
                        };
                        let mut member_bindings = FxHashMap::default();
                        if !match_function_params(member, source_fn_id, &mut member_bindings) {
                            return false;
                        }
                        for (name, ty) in member_bindings {
                            combined
                                .entry(name)
                                .and_modify(|existing| {
                                    *existing = self.interner().union2(*existing, ty);
                                })
                                .or_insert(ty);
                        }
                    }
                    bindings.extend(combined);
                    true
                }
                _ => false,
            };
        }
        if pattern_fn.this_type.is_none() && !has_param_infer && has_return_infer {
            let mut match_return = |_source_type: TypeId,
                                    source_return: TypeId,
                                    bindings: &mut FxHashMap<Atom, TypeId>|
             -> bool {
                let mut local_visited = FxHashSet::default();
                if !self.match_infer_pattern(
                    source_return,
                    pattern_fn.return_type,
                    bindings,
                    &mut local_visited,
                    checker,
                ) {
                    return false;
                }
                // For return-only infer patterns, the return type match is sufficient.
                // Skipping the final subtype check avoids issues with contravariance.
                true
            };

            return match self.interner().lookup(source) {
                Some(TypeData::Function(source_fn_id)) => {
                    let source_fn = self.interner().function_shape(source_fn_id);
                    let return_type = self
                        .erase_return_type_for_infer(source_fn.return_type, &source_fn.type_params);
                    match_return(source, return_type, bindings)
                }
                Some(TypeData::Callable(source_shape_id)) => {
                    let source_shape = self.interner().callable_shape(source_shape_id);
                    let source_sig = match source_shape.call_signatures.last() {
                        Some(sig) => sig,
                        None => return false,
                    };
                    let return_type = self.erase_return_type_for_infer(
                        source_sig.return_type,
                        &source_sig.type_params,
                    );
                    match_return(source, return_type, bindings)
                }
                Some(TypeData::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let mut member_bindings = FxHashMap::default();
                        match self.interner().lookup(member) {
                            Some(TypeData::Function(source_fn_id)) => {
                                let source_fn = self.interner().function_shape(source_fn_id);
                                let return_type = self.erase_return_type_for_infer(
                                    source_fn.return_type,
                                    &source_fn.type_params,
                                );
                                if !match_return(member, return_type, &mut member_bindings) {
                                    return false;
                                }
                            }
                            Some(TypeData::Callable(source_shape_id)) => {
                                let source_shape = self.interner().callable_shape(source_shape_id);
                                let source_sig = match source_shape.call_signatures.last() {
                                    Some(sig) => sig,
                                    None => return false,
                                };
                                let return_type = self.erase_return_type_for_infer(
                                    source_sig.return_type,
                                    &source_sig.type_params,
                                );
                                if !match_return(member, return_type, &mut member_bindings) {
                                    return false;
                                }
                            }
                            _ => return false,
                        }
                        for (name, ty) in member_bindings {
                            combined
                                .entry(name)
                                .and_modify(|existing| {
                                    *existing = self.interner().union2(*existing, ty);
                                })
                                .or_insert(ty);
                        }
                    }
                    bindings.extend(combined);
                    true
                }
                _ => false,
            };
        }

        let Some(pattern_this) = pattern_fn.this_type else {
            return checker.is_subtype_of(source, pattern);
        };
        let has_this_infer = self.type_contains_infer(pattern_this);
        if !has_this_infer && !has_param_infer && !has_return_infer {
            return checker.is_subtype_of(source, pattern);
        }

        let mut match_signature_with_this = |source_params: &[ParamInfo],
                                             source_return: TypeId,
                                             source_this: Option<TypeId>,
                                             bindings: &mut FxHashMap<Atom, TypeId>|
         -> bool {
            // Use Unknown instead of Any for stricter type checking
            // When this parameter type is not specified, use Unknown
            let source_this = source_this.unwrap_or(TypeId::UNKNOWN);
            if has_this_infer {
                let mut local_visited = FxHashSet::default();
                if !self.match_infer_pattern(
                    source_this,
                    pattern_this,
                    bindings,
                    &mut local_visited,
                    checker,
                ) {
                    return false;
                }
            } else if !checker.is_subtype_of(source_this, pattern_this) {
                return false;
            }

            if has_param_infer {
                if has_single_rest_infer {
                    if !self.match_rest_infer_tuple(
                        source_params,
                        pattern_fn.params[0].type_id,
                        bindings,
                        checker,
                    ) {
                        return false;
                    }
                } else if !self.match_signature_params_for_infer(
                    source_params,
                    &pattern_fn.params,
                    bindings,
                    checker,
                ) {
                    return false;
                }
            }

            if has_return_infer {
                let mut local_visited = FxHashSet::default();
                if !self.match_infer_pattern(
                    source_return,
                    pattern_fn.return_type,
                    bindings,
                    &mut local_visited,
                    checker,
                ) {
                    return false;
                }
            }

            // For explicit-this infer patterns, matched signature components are
            // sufficient. The final function subtype check can fail on parameter
            // contravariance even after successful infer binding.
            true
        };

        match self.interner().lookup(source) {
            Some(TypeData::Function(source_fn_id)) => {
                let source_fn = self.interner().function_shape(source_fn_id);
                let (params, return_type) = self.instantiate_signature_for_infer(
                    &source_fn.params,
                    source_fn.return_type,
                    &source_fn.type_params,
                );
                match_signature_with_this(&params, return_type, source_fn.this_type, bindings)
            }
            Some(TypeData::Callable(source_shape_id)) => {
                let source_shape = self.interner().callable_shape(source_shape_id);
                if source_shape.call_signatures.is_empty() {
                    return false;
                }
                let source_sig = source_shape
                    .call_signatures
                    .last()
                    .expect("call_signatures checked non-empty above");
                let (params, return_type) = self.instantiate_signature_for_infer(
                    &source_sig.params,
                    source_sig.return_type,
                    &source_sig.type_params,
                );
                match_signature_with_this(&params, return_type, source_sig.this_type, bindings)
            }
            Some(TypeData::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut combined = FxHashMap::default();
                for &member in members.iter() {
                    let mut member_bindings = FxHashMap::default();
                    match self.interner().lookup(member) {
                        Some(TypeData::Function(source_fn_id)) => {
                            let source_fn = self.interner().function_shape(source_fn_id);
                            let (params, return_type) = self.instantiate_signature_for_infer(
                                &source_fn.params,
                                source_fn.return_type,
                                &source_fn.type_params,
                            );
                            if !match_signature_with_this(
                                &params,
                                return_type,
                                source_fn.this_type,
                                &mut member_bindings,
                            ) {
                                return false;
                            }
                        }
                        Some(TypeData::Callable(source_shape_id)) => {
                            let source_shape = self.interner().callable_shape(source_shape_id);
                            if source_shape.call_signatures.is_empty() {
                                return false;
                            }
                            let source_sig = source_shape
                                .call_signatures
                                .last()
                                .expect("call_signatures checked non-empty above");
                            let (params, return_type) = self.instantiate_signature_for_infer(
                                &source_sig.params,
                                source_sig.return_type,
                                &source_sig.type_params,
                            );
                            if !match_signature_with_this(
                                &params,
                                return_type,
                                source_sig.this_type,
                                &mut member_bindings,
                            ) {
                                return false;
                            }
                        }
                        _ => return false,
                    }
                    for (name, ty) in member_bindings {
                        combined
                            .entry(name)
                            .and_modify(|existing| {
                                *existing = self.interner().union2(*existing, ty);
                            })
                            .or_insert(ty);
                    }
                }
                bindings.extend(combined);
                true
            }
            _ => false,
        }
    }

    /// Helper for matching constructor function patterns.
    pub(crate) fn match_infer_constructor_pattern(
        &self,
        source: TypeId,
        pattern_fn: &FunctionShape,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        // Check if pattern has a single rest parameter with infer type
        // e.g., new (...args: infer P) => any
        let has_single_rest_infer = pattern_fn.params.len() == 1
            && pattern_fn.params[0].rest
            && self.type_contains_infer(pattern_fn.params[0].type_id);

        if has_single_rest_infer {
            let infer_ty = pattern_fn.params[0].type_id;
            let mut match_construct_params_tuple = |source_params: &[ParamInfo],
                                                    bindings: &mut FxHashMap<Atom, TypeId>|
             -> bool {
                // Build a tuple type from all source parameters
                let tuple_elems: Vec<TupleElement> = source_params
                    .iter()
                    .map(|p| TupleElement {
                        type_id: p.type_id,
                        name: p.name,
                        optional: p.optional,
                        rest: false,
                    })
                    .collect();
                let tuple_ty = self.interner().tuple(tuple_elems);

                // Match the tuple against the infer type
                let mut local_visited = FxHashSet::default();
                self.match_infer_pattern(tuple_ty, infer_ty, bindings, &mut local_visited, checker)
            };

            return match self.interner().lookup(source) {
                Some(TypeData::Function(source_fn_id)) => {
                    let source_fn = self.interner().function_shape(source_fn_id);
                    if !source_fn.is_constructor {
                        return false;
                    }
                    match_construct_params_tuple(&source_fn.params, bindings)
                }
                Some(TypeData::Callable(source_shape_id)) => {
                    let source_shape = self.interner().callable_shape(source_shape_id);
                    if source_shape.construct_signatures.is_empty() {
                        return false;
                    }
                    let source_sig = &source_shape.construct_signatures[0];
                    match_construct_params_tuple(&source_sig.params, bindings)
                }
                Some(TypeData::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let mut member_bindings = FxHashMap::default();
                        match self.interner().lookup(member) {
                            Some(TypeData::Function(source_fn_id)) => {
                                let source_fn = self.interner().function_shape(source_fn_id);
                                if !source_fn.is_constructor
                                    || !match_construct_params_tuple(
                                        &source_fn.params,
                                        &mut member_bindings,
                                    )
                                {
                                    return false;
                                }
                            }
                            Some(TypeData::Callable(source_shape_id)) => {
                                let source_shape = self.interner().callable_shape(source_shape_id);
                                if source_shape.construct_signatures.is_empty() {
                                    return false;
                                }
                                let source_sig = &source_shape.construct_signatures[0];
                                if !match_construct_params_tuple(
                                    &source_sig.params,
                                    &mut member_bindings,
                                ) {
                                    return false;
                                }
                            }
                            _ => return false,
                        }
                        for (name, ty) in member_bindings {
                            combined
                                .entry(name)
                                .and_modify(|existing| {
                                    *existing = self.interner().union2(*existing, ty);
                                })
                                .or_insert(ty);
                        }
                    }
                    bindings.extend(combined);
                    true
                }
                _ => false,
            };
        }

        // General case: match parameters individually
        let mut match_construct_params =
            |source_params: &[ParamInfo], bindings: &mut FxHashMap<Atom, TypeId>| -> bool {
                let mut local_visited = FxHashSet::default();
                self.match_signature_params(
                    source_params,
                    &pattern_fn.params,
                    bindings,
                    &mut local_visited,
                    checker,
                )
            };

        match self.interner().lookup(source) {
            Some(TypeData::Function(source_fn_id)) => {
                let source_fn = self.interner().function_shape(source_fn_id);
                if !source_fn.is_constructor {
                    return false;
                }
                match_construct_params(&source_fn.params, bindings)
            }
            Some(TypeData::Callable(source_shape_id)) => {
                let source_shape = self.interner().callable_shape(source_shape_id);
                if source_shape.construct_signatures.is_empty() {
                    return false;
                }
                let source_sig = &source_shape.construct_signatures[0];
                match_construct_params(&source_sig.params, bindings)
            }
            Some(TypeData::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut combined = FxHashMap::default();
                for &member in members.iter() {
                    let mut member_bindings = FxHashMap::default();
                    match self.interner().lookup(member) {
                        Some(TypeData::Function(source_fn_id)) => {
                            let source_fn = self.interner().function_shape(source_fn_id);
                            if !source_fn.is_constructor
                                || !match_construct_params(&source_fn.params, &mut member_bindings)
                            {
                                return false;
                            }
                        }
                        Some(TypeData::Callable(source_shape_id)) => {
                            let source_shape = self.interner().callable_shape(source_shape_id);
                            if source_shape.construct_signatures.is_empty() {
                                return false;
                            }
                            let source_sig = &source_shape.construct_signatures[0];
                            if !match_construct_params(&source_sig.params, &mut member_bindings) {
                                return false;
                            }
                        }
                        _ => return false,
                    }
                    for (name, ty) in member_bindings {
                        combined
                            .entry(name)
                            .and_modify(|existing| {
                                *existing = self.interner().union2(*existing, ty);
                            })
                            .or_insert(ty);
                    }
                }
                bindings.extend(combined);
                true
            }
            _ => false,
        }
    }

    /// Helper for matching callable type patterns.
    pub(crate) fn match_infer_callable_pattern(
        &self,
        source: TypeId,
        pattern_shape_id: CallableShapeId,
        pattern: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        _visited: &mut FxHashSet<(TypeId, TypeId)>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let pattern_shape = self.interner().callable_shape(pattern_shape_id);

        // Determine which signature to use: call or construct.
        // Pattern `new (...) => infer P` has construct_signatures, not call_signatures.
        let is_construct_pattern = pattern_shape.call_signatures.is_empty()
            && pattern_shape.construct_signatures.len() == 1
            && pattern_shape.properties.is_empty();
        let is_call_pattern = pattern_shape.construct_signatures.is_empty()
            && pattern_shape.call_signatures.len() == 1
            && pattern_shape.properties.is_empty();

        if !is_call_pattern && !is_construct_pattern {
            return checker.is_subtype_of(source, pattern);
        }
        let pattern_sig = if is_construct_pattern {
            &pattern_shape.construct_signatures[0]
        } else {
            &pattern_shape.call_signatures[0]
        };
        let has_param_infer = pattern_sig
            .params
            .iter()
            .any(|param| self.type_contains_infer(param.type_id));
        let has_return_infer = self.type_contains_infer(pattern_sig.return_type);
        let has_single_rest_infer = pattern_sig.params.len() == 1
            && pattern_sig.params[0].rest
            && self.type_contains_infer(pattern_sig.params[0].type_id);
        if pattern_sig.this_type.is_none() && has_param_infer && has_return_infer {
            let mut match_params_and_return = |_source_type: TypeId,
                                               source_params: &[ParamInfo],
                                               source_return: TypeId,
                                               bindings: &mut FxHashMap<Atom, TypeId>|
             -> bool {
                let mut local_visited = FxHashSet::default();
                if has_single_rest_infer {
                    if !self.match_rest_infer_tuple(
                        source_params,
                        pattern_sig.params[0].type_id,
                        bindings,
                        checker,
                    ) {
                        return false;
                    }
                } else if !self.match_signature_params_for_infer(
                    source_params,
                    &pattern_sig.params,
                    bindings,
                    checker,
                ) {
                    return false;
                }
                if !self.match_infer_pattern(
                    source_return,
                    pattern_sig.return_type,
                    bindings,
                    &mut local_visited,
                    checker,
                ) {
                    return false;
                }
                // For infer pattern matching, once parameters and return type match successfully,
                // the pattern is considered successful. Skipping the final subtype check avoids
                // contravariance issues.
                true
            };

            return match self.interner().lookup(source) {
                Some(TypeData::Callable(source_shape_id)) => {
                    let source_shape = self.interner().callable_shape(source_shape_id);
                    let source_sigs = if is_construct_pattern {
                        &source_shape.construct_signatures
                    } else {
                        &source_shape.call_signatures
                    };
                    let other_sigs = if is_construct_pattern {
                        &source_shape.call_signatures
                    } else {
                        &source_shape.construct_signatures
                    };
                    if source_sigs.is_empty() || !other_sigs.is_empty() {
                        return false;
                    }
                    let Some(source_sig) = source_sigs.last() else {
                        return false;
                    };
                    let (params, return_type) = self.instantiate_signature_for_infer(
                        &source_sig.params,
                        source_sig.return_type,
                        &source_sig.type_params,
                    );
                    match_params_and_return(source, &params, return_type, bindings)
                }
                Some(TypeData::Function(source_fn_id)) => {
                    let source_fn = self.interner().function_shape(source_fn_id);
                    // For construct patterns, only match constructor Functions
                    if is_construct_pattern && !source_fn.is_constructor {
                        return false;
                    }
                    let (params, return_type) = self.instantiate_signature_for_infer(
                        &source_fn.params,
                        source_fn.return_type,
                        &source_fn.type_params,
                    );
                    match_params_and_return(source, &params, return_type, bindings)
                }
                Some(TypeData::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let mut member_bindings = FxHashMap::default();
                        match self.interner().lookup(member) {
                            Some(TypeData::Callable(source_shape_id)) => {
                                let source_shape = self.interner().callable_shape(source_shape_id);
                                let source_sigs = if is_construct_pattern {
                                    &source_shape.construct_signatures
                                } else {
                                    &source_shape.call_signatures
                                };
                                let other_sigs = if is_construct_pattern {
                                    &source_shape.call_signatures
                                } else {
                                    &source_shape.construct_signatures
                                };
                                if source_sigs.is_empty() || !other_sigs.is_empty() {
                                    return false;
                                }
                                let Some(source_sig) = source_sigs.last() else {
                                    return false;
                                };
                                let (params, return_type) = self.instantiate_signature_for_infer(
                                    &source_sig.params,
                                    source_sig.return_type,
                                    &source_sig.type_params,
                                );
                                if !match_params_and_return(
                                    member,
                                    &params,
                                    return_type,
                                    &mut member_bindings,
                                ) {
                                    return false;
                                }
                            }
                            Some(TypeData::Function(source_fn_id)) => {
                                let source_fn = self.interner().function_shape(source_fn_id);
                                if is_construct_pattern && !source_fn.is_constructor {
                                    return false;
                                }
                                let (params, return_type) = self.instantiate_signature_for_infer(
                                    &source_fn.params,
                                    source_fn.return_type,
                                    &source_fn.type_params,
                                );
                                if !match_params_and_return(
                                    member,
                                    &params,
                                    return_type,
                                    &mut member_bindings,
                                ) {
                                    return false;
                                }
                            }
                            _ => return false,
                        }
                        for (name, ty) in member_bindings {
                            combined
                                .entry(name)
                                .and_modify(|existing| {
                                    *existing = self.interner().union2(*existing, ty);
                                })
                                .or_insert(ty);
                        }
                    }
                    bindings.extend(combined);
                    true
                }
                _ => false,
            };
        }
        if pattern_sig.this_type.is_none() && has_param_infer && !has_return_infer {
            let mut match_params =
                |source_params: &[ParamInfo], bindings: &mut FxHashMap<Atom, TypeId>| -> bool {
                    if has_single_rest_infer {
                        return self.match_rest_infer_tuple(
                            source_params,
                            pattern_sig.params[0].type_id,
                            bindings,
                            checker,
                        );
                    }
                    // Match params and infer types. Skip subtype check since pattern matching
                    // success implies compatibility. The subtype check can fail for optional
                    // params due to contravariance issues with undefined.
                    self.match_signature_params_for_infer(
                        source_params,
                        &pattern_sig.params,
                        bindings,
                        checker,
                    )
                };

            return match self.interner().lookup(source) {
                Some(TypeData::Callable(source_shape_id)) => {
                    let source_shape = self.interner().callable_shape(source_shape_id);
                    let source_sigs = if is_construct_pattern {
                        &source_shape.construct_signatures
                    } else {
                        &source_shape.call_signatures
                    };
                    let other_sigs = if is_construct_pattern {
                        &source_shape.call_signatures
                    } else {
                        &source_shape.construct_signatures
                    };
                    if source_sigs.is_empty() || !other_sigs.is_empty() {
                        return false;
                    }
                    let Some(source_sig) = source_sigs.last() else {
                        return false;
                    };
                    match_params(&source_sig.params, bindings)
                }
                Some(TypeData::Function(source_fn_id)) => {
                    let source_fn = self.interner().function_shape(source_fn_id);
                    if is_construct_pattern && !source_fn.is_constructor {
                        return false;
                    }
                    match_params(&source_fn.params, bindings)
                }
                Some(TypeData::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let mut member_bindings = FxHashMap::default();
                        match self.interner().lookup(member) {
                            Some(TypeData::Callable(source_shape_id)) => {
                                let source_shape = self.interner().callable_shape(source_shape_id);
                                let source_sigs = if is_construct_pattern {
                                    &source_shape.construct_signatures
                                } else {
                                    &source_shape.call_signatures
                                };
                                let other_sigs = if is_construct_pattern {
                                    &source_shape.call_signatures
                                } else {
                                    &source_shape.construct_signatures
                                };
                                if source_sigs.is_empty() || !other_sigs.is_empty() {
                                    return false;
                                }
                                let Some(source_sig) = source_sigs.last() else {
                                    return false;
                                };
                                if !match_params(&source_sig.params, &mut member_bindings) {
                                    return false;
                                }
                            }
                            Some(TypeData::Function(source_fn_id)) => {
                                let source_fn = self.interner().function_shape(source_fn_id);
                                if is_construct_pattern && !source_fn.is_constructor {
                                    return false;
                                }
                                if !match_params(&source_fn.params, &mut member_bindings) {
                                    return false;
                                }
                            }
                            _ => return false,
                        }
                        for (name, ty) in member_bindings {
                            combined
                                .entry(name)
                                .and_modify(|existing| {
                                    *existing = self.interner().union2(*existing, ty);
                                })
                                .or_insert(ty);
                        }
                    }
                    bindings.extend(combined);
                    true
                }
                _ => false,
            };
        }

        if pattern_sig.this_type.is_none() && !has_param_infer && has_return_infer {
            let mut match_return = |_source_type: TypeId,
                                    source_return: TypeId,
                                    bindings: &mut FxHashMap<Atom, TypeId>|
             -> bool {
                let mut local_visited = FxHashSet::default();
                if !self.match_infer_pattern(
                    source_return,
                    pattern_sig.return_type,
                    bindings,
                    &mut local_visited,
                    checker,
                ) {
                    return false;
                }
                // For return-only infer patterns, the return type match is sufficient.
                // Skipping the final subtype check avoids contravariance issues.
                true
            };

            return match self.interner().lookup(source) {
                Some(TypeData::Callable(source_shape_id)) => {
                    let source_shape = self.interner().callable_shape(source_shape_id);
                    let source_sigs = if is_construct_pattern {
                        &source_shape.construct_signatures
                    } else {
                        &source_shape.call_signatures
                    };
                    let other_sigs = if is_construct_pattern {
                        &source_shape.call_signatures
                    } else {
                        &source_shape.construct_signatures
                    };
                    if source_sigs.is_empty() || !other_sigs.is_empty() {
                        return false;
                    }
                    let Some(source_sig) = source_sigs.last() else {
                        return false;
                    };
                    let erased_return = self.erase_return_type_for_infer(
                        source_sig.return_type,
                        &source_sig.type_params,
                    );
                    match_return(source, erased_return, bindings)
                }
                Some(TypeData::Function(source_fn_id)) => {
                    let source_fn = self.interner().function_shape(source_fn_id);
                    if is_construct_pattern && !source_fn.is_constructor {
                        return false;
                    }
                    let erased_return = self
                        .erase_return_type_for_infer(source_fn.return_type, &source_fn.type_params);
                    match_return(source, erased_return, bindings)
                }
                Some(TypeData::Union(members)) => {
                    let members = self.interner().type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let mut member_bindings = FxHashMap::default();
                        match self.interner().lookup(member) {
                            Some(TypeData::Callable(source_shape_id)) => {
                                let source_shape = self.interner().callable_shape(source_shape_id);
                                let source_sigs = if is_construct_pattern {
                                    &source_shape.construct_signatures
                                } else {
                                    &source_shape.call_signatures
                                };
                                let other_sigs = if is_construct_pattern {
                                    &source_shape.call_signatures
                                } else {
                                    &source_shape.construct_signatures
                                };
                                if source_sigs.is_empty() || !other_sigs.is_empty() {
                                    return false;
                                }
                                let Some(source_sig) = source_sigs.last() else {
                                    return false;
                                };
                                let erased_return = self.erase_return_type_for_infer(
                                    source_sig.return_type,
                                    &source_sig.type_params,
                                );
                                if !match_return(member, erased_return, &mut member_bindings) {
                                    return false;
                                }
                            }
                            Some(TypeData::Function(source_fn_id)) => {
                                let source_fn = self.interner().function_shape(source_fn_id);
                                if is_construct_pattern && !source_fn.is_constructor {
                                    return false;
                                }
                                let erased_return = self.erase_return_type_for_infer(
                                    source_fn.return_type,
                                    &source_fn.type_params,
                                );
                                if !match_return(member, erased_return, &mut member_bindings) {
                                    return false;
                                }
                            }
                            _ => return false,
                        }
                        for (name, ty) in member_bindings {
                            combined
                                .entry(name)
                                .and_modify(|existing| {
                                    *existing = self.interner().union2(*existing, ty);
                                })
                                .or_insert(ty);
                        }
                    }
                    bindings.extend(combined);
                    true
                }
                _ => false,
            };
        }

        checker.is_subtype_of(source, pattern)
    }

    /// Helper for matching object type patterns.
    pub(crate) fn match_infer_object_pattern(
        &self,
        source: TypeId,
        pattern_shape_id: ObjectShapeId,
        pattern: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        match self.interner().lookup(source) {
            Some(
                TypeData::Object(source_shape_id) | TypeData::ObjectWithIndex(source_shape_id),
            ) => {
                let source_shape = self.interner().object_shape(source_shape_id);
                let pattern_shape = self.interner().object_shape(pattern_shape_id);
                for pattern_prop in &pattern_shape.properties {
                    let source_prop = source_shape
                        .properties
                        .iter()
                        .find(|prop| prop.name == pattern_prop.name);
                    let Some(source_prop) = source_prop else {
                        if pattern_prop.optional {
                            if self.type_contains_infer(pattern_prop.type_id)
                                && !self.match_infer_pattern(
                                    TypeId::UNDEFINED,
                                    pattern_prop.type_id,
                                    bindings,
                                    visited,
                                    checker,
                                )
                            {
                                return false;
                            }
                            continue;
                        }
                        return false;
                    };
                    let source_type = self.optional_property_type(source_prop);
                    if !self.match_infer_pattern(
                        source_type,
                        pattern_prop.type_id,
                        bindings,
                        visited,
                        checker,
                    ) {
                        return false;
                    }
                }
                true
            }
            Some(TypeData::Callable(callable_shape_id)) => {
                // Callable types (class constructors) have properties (static members)
                // that can match object patterns with infer. For example:
                // `typeof MyClass extends { defaultProps: infer D }` should match
                // when MyClass has a static `defaultProps` property.
                let callable_shape = self.interner().callable_shape(callable_shape_id);
                let pattern_shape = self.interner().object_shape(pattern_shape_id);
                for pattern_prop in &pattern_shape.properties {
                    let source_prop = callable_shape
                        .properties
                        .iter()
                        .find(|prop| prop.name == pattern_prop.name);
                    let Some(source_prop) = source_prop else {
                        if pattern_prop.optional {
                            if self.type_contains_infer(pattern_prop.type_id)
                                && !self.match_infer_pattern(
                                    TypeId::UNDEFINED,
                                    pattern_prop.type_id,
                                    bindings,
                                    visited,
                                    checker,
                                )
                            {
                                return false;
                            }
                            continue;
                        }
                        return false;
                    };
                    let source_type = self.optional_property_type(source_prop);
                    if !self.match_infer_pattern(
                        source_type,
                        pattern_prop.type_id,
                        bindings,
                        visited,
                        checker,
                    ) {
                        return false;
                    }
                }
                true
            }
            Some(TypeData::Intersection(members)) => {
                let members = self.interner().type_list(members);
                let pattern_shape = self.interner().object_shape(pattern_shape_id);
                for pattern_prop in &pattern_shape.properties {
                    let mut merged_type = None;
                    for &member in members.iter() {
                        let found_type =
                            self.find_property_type_in_structural(member, pattern_prop.name);
                        if found_type.is_none() && !pattern_prop.optional {
                            // Non-optional pattern prop not found in this intersection
                            // member — if the member isn't Object/Callable, fail.
                            if !matches!(
                                self.interner().lookup(member),
                                Some(
                                    TypeData::Object(_)
                                        | TypeData::ObjectWithIndex(_)
                                        | TypeData::Callable(_)
                                )
                            ) {
                                return false;
                            }
                        }
                        if let Some(source_type) = found_type {
                            merged_type = Some(match merged_type {
                                Some(existing) => {
                                    self.interner().intersection2(existing, source_type)
                                }
                                None => source_type,
                            });
                        }
                    }

                    let Some(source_type) = merged_type else {
                        if pattern_prop.optional {
                            if self.type_contains_infer(pattern_prop.type_id)
                                && !self.match_infer_pattern(
                                    TypeId::UNDEFINED,
                                    pattern_prop.type_id,
                                    bindings,
                                    visited,
                                    checker,
                                )
                            {
                                return false;
                            }
                            continue;
                        }
                        return false;
                    };

                    if !self.match_infer_pattern(
                        source_type,
                        pattern_prop.type_id,
                        bindings,
                        visited,
                        checker,
                    ) {
                        return false;
                    }
                }
                true
            }
            Some(TypeData::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut combined = FxHashMap::default();
                for &member in members.iter() {
                    let mut member_bindings = FxHashMap::default();
                    let mut local_visited = FxHashSet::default();
                    if !self.match_infer_pattern(
                        member,
                        pattern,
                        &mut member_bindings,
                        &mut local_visited,
                        checker,
                    ) {
                        return false;
                    }
                    for (name, ty) in member_bindings {
                        combined
                            .entry(name)
                            .and_modify(|existing| {
                                *existing = self.interner().union2(*existing, ty);
                            })
                            .or_insert(ty);
                    }
                }
                bindings.extend(combined);
                true
            }
            Some(
                TypeData::Tuple(_)
                | TypeData::Array(_)
                | TypeData::ReadonlyType(_)
                | TypeData::Intrinsic(IntrinsicKind::String)
                | TypeData::Literal(LiteralValue::String(_))
                | TypeData::TemplateLiteral(_),
            ) => {
                let pattern_shape = self.interner().object_shape(pattern_shape_id);
                for pattern_prop in &pattern_shape.properties {
                    let Some(source_type) =
                        self.implicit_sequence_property_type(source, pattern_prop.name)
                    else {
                        if pattern_prop.optional {
                            if self.type_contains_infer(pattern_prop.type_id)
                                && !self.match_infer_pattern(
                                    TypeId::UNDEFINED,
                                    pattern_prop.type_id,
                                    bindings,
                                    visited,
                                    checker,
                                )
                            {
                                return false;
                            }
                            continue;
                        }
                        return false;
                    };
                    if !self.match_infer_pattern(
                        source_type,
                        pattern_prop.type_id,
                        bindings,
                        visited,
                        checker,
                    ) {
                        return false;
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// Find a named property's type in a structural type (`Object`, `ObjectWithIndex`, or `Callable`).
    /// Returns `Some(type_id)` if the property is found, respecting optional property unwrapping.
    fn find_property_type_in_structural(&self, type_id: TypeId, prop_name: Atom) -> Option<TypeId> {
        match self.interner().lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner().object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .find(|p| p.name == prop_name)
                    .map(|p| self.optional_property_type(p))
            }
            Some(TypeData::Callable(callable_id)) => {
                let shape = self.interner().callable_shape(callable_id);
                shape
                    .properties
                    .iter()
                    .find(|p| p.name == prop_name)
                    .map(|p| self.optional_property_type(p))
            }
            _ => None,
        }
    }

    /// Helper for matching object with index type patterns.
    pub(crate) fn match_infer_object_with_index_pattern(
        &self,
        source: TypeId,
        pattern_shape_id: ObjectShapeId,
        pattern: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let pattern_shape = self.interner().object_shape(pattern_shape_id);
        if let Some(source_elem) =
            crate::type_queries::get_array_element_type(self.interner(), source)
            && let Some(pattern_index) = &pattern_shape.number_index
        {
            let mut key_visited = FxHashSet::default();
            if !self.match_infer_pattern(
                TypeId::NUMBER,
                pattern_index.key_type,
                bindings,
                &mut key_visited,
                checker,
            ) {
                return false;
            }
            let mut value_visited = FxHashSet::default();
            return self.match_infer_pattern(
                source_elem,
                pattern_index.value_type,
                bindings,
                &mut value_visited,
                checker,
            );
        }

        match self.interner().lookup(source) {
            Some(
                TypeData::Object(source_shape_id) | TypeData::ObjectWithIndex(source_shape_id),
            ) => {
                let source_shape = self.interner().object_shape(source_shape_id);
                for pattern_prop in &pattern_shape.properties {
                    let source_prop = source_shape
                        .properties
                        .iter()
                        .find(|prop| prop.name == pattern_prop.name);
                    let Some(source_prop) = source_prop else {
                        if pattern_prop.optional {
                            if self.type_contains_infer(pattern_prop.type_id)
                                && !self.match_infer_pattern(
                                    TypeId::UNDEFINED,
                                    pattern_prop.type_id,
                                    bindings,
                                    visited,
                                    checker,
                                )
                            {
                                return false;
                            }
                            continue;
                        }
                        return false;
                    };
                    let source_type = self.optional_property_type(source_prop);
                    if !self.match_infer_pattern(
                        source_type,
                        pattern_prop.type_id,
                        bindings,
                        visited,
                        checker,
                    ) {
                        return false;
                    }
                }

                if let Some(pattern_index) = &pattern_shape.string_index {
                    if let Some(source_index) = &source_shape.string_index {
                        if !self.match_infer_pattern(
                            source_index.key_type,
                            pattern_index.key_type,
                            bindings,
                            visited,
                            checker,
                        ) {
                            return false;
                        }
                        if !self.match_infer_pattern(
                            source_index.value_type,
                            pattern_index.value_type,
                            bindings,
                            visited,
                            checker,
                        ) {
                            return false;
                        }
                    } else {
                        let mut local_visited = FxHashSet::default();
                        if !self.match_infer_pattern(
                            TypeId::STRING,
                            pattern_index.key_type,
                            bindings,
                            &mut local_visited,
                            checker,
                        ) {
                            return false;
                        }
                        let values: Vec<TypeId> = source_shape
                            .properties
                            .iter()
                            .map(|prop| self.optional_property_type(prop))
                            .collect();
                        let value_type = if values.is_empty() {
                            TypeId::NEVER
                        } else if values.len() == 1 {
                            values[0]
                        } else {
                            self.interner().union(values)
                        };
                        let mut local_visited = FxHashSet::default();
                        if !self.match_infer_pattern(
                            value_type,
                            pattern_index.value_type,
                            bindings,
                            &mut local_visited,
                            checker,
                        ) {
                            return false;
                        }
                    }
                }

                if let Some(pattern_index) = &pattern_shape.number_index {
                    if let Some(source_index) = &source_shape.number_index {
                        if !self.match_infer_pattern(
                            source_index.key_type,
                            pattern_index.key_type,
                            bindings,
                            visited,
                            checker,
                        ) {
                            return false;
                        }
                        if !self.match_infer_pattern(
                            source_index.value_type,
                            pattern_index.value_type,
                            bindings,
                            visited,
                            checker,
                        ) {
                            return false;
                        }
                    } else {
                        let mut local_visited = FxHashSet::default();
                        if !self.match_infer_pattern(
                            TypeId::NUMBER,
                            pattern_index.key_type,
                            bindings,
                            &mut local_visited,
                            checker,
                        ) {
                            return false;
                        }
                        let values: Vec<TypeId> = source_shape
                            .properties
                            .iter()
                            .filter(|prop| {
                                utils::is_numeric_property_name(self.interner(), prop.name)
                            })
                            .map(|prop| self.optional_property_type(prop))
                            .collect();
                        let value_type = if values.is_empty() {
                            TypeId::NEVER
                        } else if values.len() == 1 {
                            values[0]
                        } else {
                            self.interner().union(values)
                        };
                        let mut local_visited = FxHashSet::default();
                        if !self.match_infer_pattern(
                            value_type,
                            pattern_index.value_type,
                            bindings,
                            &mut local_visited,
                            checker,
                        ) {
                            return false;
                        }
                    }
                }

                true
            }
            Some(TypeData::Union(members)) => {
                let members = self.interner().type_list(members);
                let mut combined = FxHashMap::default();
                for &member in members.iter() {
                    let mut member_bindings = FxHashMap::default();
                    let mut local_visited = FxHashSet::default();
                    if !self.match_infer_pattern(
                        member,
                        pattern,
                        &mut member_bindings,
                        &mut local_visited,
                        checker,
                    ) {
                        return false;
                    }
                    for (name, ty) in member_bindings {
                        combined
                            .entry(name)
                            .and_modify(|existing| {
                                *existing = self.interner().union2(*existing, ty);
                            })
                            .or_insert(ty);
                    }
                }
                bindings.extend(combined);
                true
            }
            _ => false,
        }
    }

    /// Helper for matching union type patterns containing infer.
    pub(crate) fn match_infer_union_pattern(
        &self,
        source: TypeId,
        pattern_members: TypeListId,
        pattern: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let pattern_members = self.interner().type_list(pattern_members);

        // Find infer members and non-infer members in the pattern
        let mut infer_members: Vec<(Atom, Option<TypeId>)> = Vec::new();
        let mut non_infer_pattern_members: Vec<TypeId> = Vec::new();

        for &pattern_member in pattern_members.iter() {
            if let Some(TypeData::Infer(info)) = self.interner().lookup(pattern_member) {
                infer_members.push((info.name, info.constraint));
            } else {
                non_infer_pattern_members.push(pattern_member);
            }
        }

        // If no infer members, just do subtype check
        if infer_members.is_empty() {
            return checker.is_subtype_of(source, pattern);
        }

        // Currently only handle single infer in union pattern
        if infer_members.len() != 1 {
            return checker.is_subtype_of(source, pattern);
        }

        let (infer_name, infer_constraint) = infer_members[0];

        // Handle both union and non-union sources
        match self.interner().lookup(source) {
            Some(TypeData::Union(source_members)) => {
                let source_members = self.interner().type_list(source_members);

                // Find source members that DON'T match non-infer pattern members
                let mut remaining_source_members: Vec<TypeId> = Vec::new();

                for &source_member in source_members.iter() {
                    let mut matched = false;
                    for &non_infer in &non_infer_pattern_members {
                        if checker.is_subtype_of(source_member, non_infer)
                            && checker.is_subtype_of(non_infer, source_member)
                        {
                            matched = true;
                            break;
                        }
                    }
                    if !matched {
                        remaining_source_members.push(source_member);
                    }
                }

                // Bind infer to the remaining source members
                let inferred_type = if remaining_source_members.is_empty() {
                    TypeId::NEVER
                } else if remaining_source_members.len() == 1 {
                    remaining_source_members[0]
                } else {
                    self.interner().union(remaining_source_members)
                };

                self.bind_infer(
                    &TypeParamInfo {
                        is_const: false,
                        name: infer_name,
                        constraint: infer_constraint,
                        default: None,
                    },
                    inferred_type,
                    bindings,
                    checker,
                )
            }
            _ => {
                // Source is not a union - check if source matches any non-infer pattern member
                for &non_infer in &non_infer_pattern_members {
                    if checker.is_subtype_of(source, non_infer)
                        && checker.is_subtype_of(non_infer, source)
                    {
                        // Source is exactly a non-infer member, so infer gets never
                        return self.bind_infer(
                            &TypeParamInfo {
                                is_const: false,
                                name: infer_name,
                                constraint: infer_constraint,
                                default: None,
                            },
                            TypeId::NEVER,
                            bindings,
                            checker,
                        );
                    }
                }
                // Source doesn't match non-infer members, so infer = source
                self.bind_infer(
                    &TypeParamInfo {
                        is_const: false,
                        name: infer_name,
                        constraint: infer_constraint,
                        default: None,
                    },
                    source,
                    bindings,
                    checker,
                )
            }
        }
    }

    /// Match a template literal string against a pattern.
    pub(crate) fn match_template_literal_string(
        &self,
        source: &str,
        pattern: &[TemplateSpan],
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        self.match_template_literal_string_from(source, pattern, 0, 0, bindings, checker)
    }

    fn match_template_segment_prefix(
        &self,
        source: &str,
        pos: usize,
        type_id: TypeId,
    ) -> Option<usize> {
        match self.interner().lookup(type_id)? {
            TypeData::Literal(LiteralValue::String(atom)) => {
                let text = self.interner().resolve_atom(atom);
                source
                    .get(pos..)?
                    .starts_with(&text)
                    .then_some(pos + text.len())
            }
            TypeData::Union(list_id) => self
                .interner()
                .type_list(list_id)
                .iter()
                .find_map(|member| self.match_template_segment_prefix(source, pos, *member)),
            TypeData::TemplateLiteral(template_id) => {
                let spans = self.interner().template_list(template_id);
                let mut text = String::new();
                for span in spans.iter() {
                    let TemplateSpan::Text(atom) = span else {
                        return None;
                    };
                    text.push_str(&self.interner().resolve_atom(*atom));
                }
                source
                    .get(pos..)?
                    .starts_with(&text)
                    .then_some(pos + text.len())
            }
            _ => None,
        }
    }

    fn is_template_infer_span(&self, span: Option<&TemplateSpan>) -> bool {
        span.is_some_and(|span| {
            matches!(span, TemplateSpan::Type(type_id) if matches!(self.interner().lookup(*type_id), Some(TypeData::Infer(_))))
        })
    }

    fn next_char_end(source: &str, pos: usize) -> Option<usize> {
        if pos >= source.len() {
            return None;
        }
        Some(
            source[pos..]
                .char_indices()
                .nth(1)
                .map_or(source.len(), |(idx, _)| pos + idx),
        )
    }

    fn candidate_template_capture_ends(
        &self,
        source: &str,
        pos: usize,
        pattern: &[TemplateSpan],
        index: usize,
    ) -> Vec<usize> {
        if index + 1 >= pattern.len() {
            return vec![source.len()];
        }

        if self.is_template_infer_span(pattern.get(index))
            && matches!(
                pattern.get(index + 1),
                Some(TemplateSpan::Type(
                    TypeId::STRING | TypeId::ANY | TypeId::UNKNOWN
                ))
            )
        {
            if self.is_template_infer_span(pattern.get(index + 2)) {
                return Self::next_char_end(source, pos).into_iter().collect();
            }

            return Self::next_char_end(source, pos)
                .or(Some(pos))
                .into_iter()
                .collect();
        }

        if pattern
            .get(index + 1)
            .is_some_and(|s| matches!(s, TemplateSpan::Type(type_id) if matches!(self.interner().lookup(*type_id), Some(TypeData::Infer(_)))))
        {
            return Self::next_char_end(source, pos).into_iter().collect();
        }

        if let Some(next_text) = pattern[index + 1..].iter().find_map(|span| match span {
            TemplateSpan::Text(text) => Some(*text),
            TemplateSpan::Type(_) => None,
        }) {
            let next_value = self.interner().resolve_atom_ref(next_text);
            let remaining = &source[pos..];
            return remaining
                .match_indices(next_value.as_ref())
                .map(|(offset, _)| pos + offset)
                .collect();
        }

        source[pos..]
            .char_indices()
            .map(|(offset, _)| pos + offset)
            .chain(std::iter::once(source.len()))
            .collect()
    }

    /// Match an intrinsic-typed span at position `pos` in the infer-pattern path.
    ///
    /// Returns `Some(true/false)` when the span is a recognized intrinsic kind
    /// (number, bigint, boolean, null, undefined) and dispatches length-aware
    /// matching for it.  Returns `None` for wildcard intrinsics (string/any/
    /// unknown) so the caller falls through to generic handling.
    fn match_intrinsic_span_from(
        &self,
        source: &str,
        pattern: &[TemplateSpan],
        pos: usize,
        index: usize,
        type_id: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> Option<bool> {
        use crate::relations::subtype::rules::literals::{
            find_integer_length, find_number_length, is_valid_number,
        };

        let remaining = &source[pos..];

        match self.interner().lookup(type_id)? {
            TypeData::Intrinsic(kind) => match kind {
                IntrinsicKind::Number => {
                    let num_len = find_number_length(remaining);
                    if num_len == 0 {
                        return Some(false);
                    }
                    // Try shortest valid number first — matches tsc's non-greedy
                    // behaviour for ambiguous infer+number patterns.
                    for len in 1..=num_len {
                        if is_valid_number(&remaining[..len])
                            && self.match_template_literal_string_from(
                                source,
                                pattern,
                                pos + len,
                                index + 1,
                                bindings,
                                checker,
                            )
                        {
                            return Some(true);
                        }
                    }
                    Some(false)
                }
                IntrinsicKind::Bigint => {
                    let int_len = find_integer_length(remaining);
                    if int_len == 0 {
                        return Some(false);
                    }
                    // Try shortest valid integer first — consistent with tsc.
                    for len in 1..=int_len {
                        if self.match_template_literal_string_from(
                            source,
                            pattern,
                            pos + len,
                            index + 1,
                            bindings,
                            checker,
                        ) {
                            return Some(true);
                        }
                    }
                    Some(false)
                }
                IntrinsicKind::Boolean => {
                    if remaining.starts_with("true")
                        && self.match_template_literal_string_from(
                            source,
                            pattern,
                            pos + 4,
                            index + 1,
                            bindings,
                            checker,
                        )
                    {
                        return Some(true);
                    }
                    if remaining.starts_with("false")
                        && self.match_template_literal_string_from(
                            source,
                            pattern,
                            pos + 5,
                            index + 1,
                            bindings,
                            checker,
                        )
                    {
                        return Some(true);
                    }
                    Some(false)
                }
                IntrinsicKind::Null => {
                    if remaining.starts_with("null")
                        && self.match_template_literal_string_from(
                            source,
                            pattern,
                            pos + 4,
                            index + 1,
                            bindings,
                            checker,
                        )
                    {
                        Some(true)
                    } else {
                        Some(false)
                    }
                }
                IntrinsicKind::Undefined => {
                    if remaining.starts_with("undefined")
                        && self.match_template_literal_string_from(
                            source,
                            pattern,
                            pos + 9,
                            index + 1,
                            bindings,
                            checker,
                        )
                    {
                        Some(true)
                    } else {
                        Some(false)
                    }
                }
                // Wildcards and other intrinsics fall through to generic handling.
                _ => None,
            },
            _ => None,
        }
    }

    fn match_template_literal_string_from(
        &self,
        source: &str,
        pattern: &[TemplateSpan],
        pos: usize,
        index: usize,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        if index == pattern.len() {
            return pos == source.len();
        }

        match pattern[index] {
            TemplateSpan::Text(text) => {
                let text_value = self.interner().resolve_atom_ref(text);
                let text_value = text_value.as_ref();
                if !source[pos..].starts_with(text_value) {
                    return false;
                }
                self.match_template_literal_string_from(
                    source,
                    pattern,
                    pos + text_value.len(),
                    index + 1,
                    bindings,
                    checker,
                )
            }
            TemplateSpan::Type(type_id) => {
                if let Some(TypeData::Infer(info)) = self.interner().lookup(type_id) {
                    for end in self.candidate_template_capture_ends(source, pos, pattern, index) {
                        let mut next_bindings = bindings.clone();
                        let captured = &source[pos..end];
                        if !self.bind_template_infer_capture(
                            &info,
                            captured,
                            &mut next_bindings,
                            checker,
                        ) {
                            continue;
                        }
                        if self.match_template_literal_string_from(
                            source,
                            pattern,
                            end,
                            index + 1,
                            &mut next_bindings,
                            checker,
                        ) {
                            *bindings = next_bindings;
                            return true;
                        }
                    }
                    return false;
                }

                if let Some(next_pos) = self.match_template_segment_prefix(source, pos, type_id) {
                    return self.match_template_literal_string_from(
                        source,
                        pattern,
                        next_pos,
                        index + 1,
                        bindings,
                        checker,
                    );
                }

                if let Some(result) = self.match_intrinsic_span_from(
                    source, pattern, pos, index, type_id, bindings, checker,
                ) {
                    return result;
                }

                for end in self.candidate_template_capture_ends(source, pos, pattern, index) {
                    let captured = &source[pos..end];
                    let captured_type = self.interner().literal_string(captured);
                    if self
                        .template_capture_for_constraint(captured, captured_type, type_id, checker)
                        .is_some()
                        && self.match_template_literal_string_from(
                            source,
                            pattern,
                            end,
                            index + 1,
                            bindings,
                            checker,
                        )
                    {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Match template literal spans against a pattern.
    pub(crate) fn match_template_literal_spans(
        &self,
        source: TypeId,
        source_spans: &[TemplateSpan],
        pattern_spans: &[TemplateSpan],
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        if pattern_spans.len() == 1
            && let TemplateSpan::Type(type_id) = pattern_spans[0]
        {
            if let Some(TypeData::Infer(info)) = self.interner().lookup(type_id) {
                let inferred = if source_spans
                    .iter()
                    .all(|span| matches!(span, TemplateSpan::Type(_)))
                {
                    TypeId::STRING
                } else {
                    source
                };
                return self.bind_infer(&info, inferred, bindings, checker);
            }
            return checker.is_subtype_of(source, type_id);
        }

        if source_spans.len() != pattern_spans.len() {
            return false;
        }

        for (source_span, pattern_span) in source_spans.iter().zip(pattern_spans.iter()) {
            match pattern_span {
                TemplateSpan::Text(text) => match source_span {
                    TemplateSpan::Text(source_text) if source_text == text => {}
                    _ => return false,
                },
                TemplateSpan::Type(type_id) => {
                    let inferred = match source_span {
                        TemplateSpan::Text(text) => {
                            let text_value = self.interner().resolve_atom_ref(*text);
                            self.interner().literal_string(text_value.as_ref())
                        }
                        TemplateSpan::Type(source_type) => *source_type,
                    };
                    if let Some(TypeData::Infer(info)) = self.interner().lookup(*type_id) {
                        if !self.bind_infer(&info, inferred, bindings, checker) {
                            return false;
                        }
                    } else if !checker.is_subtype_of(inferred, *type_id) {
                        return false;
                    }
                }
            }
        }

        true
    }
}
