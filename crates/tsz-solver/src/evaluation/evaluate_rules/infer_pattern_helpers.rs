//! Type-specific infer pattern matching helpers (signature/function/callable).
//!
//! Contains specialized pattern matchers for:
//! - Function type patterns
//! - Constructor type patterns
//! - Callable type patterns
//! - Signature parameter / rest matching and template-capture binding helpers
//!
//! Object, object-with-index, union, and template-literal pattern matchers live
//! in `infer_pattern_object_helpers.rs` (split to stay under the file-size
//! ceiling); both are `impl TypeEvaluator` blocks in the same module tree.

use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::types::{
    CallableShapeId, FunctionShape, FunctionShapeId, IntrinsicKind, LiteralValue, ParamInfo,
    TupleElement, TypeData, TypeId, TypeParamInfo,
};
use crate::visitor::array_element_type;
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

    pub(crate) fn template_capture_for_constraint(
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

    pub(crate) fn bind_template_infer_capture(
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

        // A source callable with fewer parameters is still assignable to the
        // inference pattern (extra trailing positions are ignored at the call
        // site); tsc takes the true branch and defaults the unmatched `infer`
        // slots to `unknown`. Match the overlapping prefix, default the rest.
        let matched_count = source_params.len().min(fixed_param_count);

        let mut local_visited = FxHashSet::default();
        // Function/callable parameters are contravariant: co-located same-name
        // infer slots intersect their candidates instead of failing the
        // second match through `bind_infer`'s mutual subtype check. Route
        // both the fixed-param loop and any non-infer trailing-rest fan-out
        // through the shared co-located merge helper so the rest case keeps
        // its own contravariant semantics.
        let mut fixed_pairs: Vec<(TypeId, TypeId)> = Vec::with_capacity(matched_count);
        for (source_param, pattern_param) in source_params
            .iter()
            .take(matched_count)
            .zip(pattern_params.iter().take(matched_count))
        {
            let source_param_type = if source_param.optional {
                crate::narrowing::remove_nullish(self.interner(), source_param.type_id)
            } else {
                source_param.type_id
            };
            fixed_pairs.push((source_param_type, pattern_param.type_id));
        }

        // Fixed pattern positions the source never supplies: default their
        // infer vars to `unknown`, filled only where still unbound so a
        // candidate from a matched position always wins.
        for pattern_param in &pattern_params[matched_count..fixed_param_count] {
            self.fill_unbound_infer_defaults(pattern_param.type_id, TypeId::UNKNOWN, bindings);
        }

        if let Some(rest_param) = trailing_rest_param {
            let remaining_params = source_params.get(fixed_param_count..).unwrap_or(&[]);
            if self.type_contains_infer(rest_param.type_id) {
                if !self.match_co_located_intersect_pairs(
                    &fixed_pairs,
                    bindings,
                    &mut local_visited,
                    checker,
                ) {
                    return false;
                }
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
                    fixed_pairs.push((source_param_type, pattern_type));
                }
                return self.match_co_located_intersect_pairs(
                    &fixed_pairs,
                    bindings,
                    &mut local_visited,
                    checker,
                );
            }
            return true;
        }

        self.match_co_located_intersect_pairs(&fixed_pairs, bindings, &mut local_visited, checker)
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
                    // A Callable like DateConstructor carries both call and construct
                    // signatures; select by the pattern's kind for the return type.
                    let Some(source_sig) = source_shape.last_sig_for(pattern_fn.is_constructor)
                    else {
                        return false;
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
                                let Some(source_sig) =
                                    source_shape.last_sig_for(pattern_fn.is_constructor)
                                else {
                                    return false;
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

        if pattern_shape
            .properties
            .iter()
            .any(|prop| self.type_contains_infer(prop.type_id))
            && self.match_infer_callable_pattern_properties(
                source,
                pattern_shape_id,
                bindings,
                checker,
            )
        {
            return true;
        }

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
                    let Some(source_sig) = source_shape.last_sig_for(is_construct_pattern) else {
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
                                let Some(source_sig) =
                                    source_shape.last_sig_for(is_construct_pattern)
                                else {
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
                    let Some(source_sig) = source_shape.last_sig_for(is_construct_pattern) else {
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
                                let Some(source_sig) =
                                    source_shape.last_sig_for(is_construct_pattern)
                                else {
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
                    let Some(source_sig) = source_shape.last_sig_for(is_construct_pattern) else {
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
                                let Some(source_sig) =
                                    source_shape.last_sig_for(is_construct_pattern)
                                else {
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

    fn match_infer_callable_pattern_properties(
        &self,
        source: TypeId,
        pattern_shape_id: CallableShapeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let pattern_shape = self.interner().callable_shape(pattern_shape_id);
        let Some(source_shape_id) = self.source_callable_shape_id(source) else {
            return false;
        };
        let source_shape = self.interner().callable_shape(source_shape_id);
        if pattern_shape.call_signatures.len() > source_shape.call_signatures.len()
            || pattern_shape.construct_signatures.len() > source_shape.construct_signatures.len()
        {
            return false;
        }

        for pattern_prop in &pattern_shape.properties {
            let source_prop = source_shape
                .properties
                .iter()
                .find(|prop| prop.name == pattern_prop.name);
            let Some(source_prop) = source_prop else {
                if pattern_prop.optional {
                    if self.type_contains_infer(pattern_prop.type_id) {
                        let mut visited = FxHashSet::default();
                        if !self.match_infer_pattern(
                            TypeId::UNDEFINED,
                            pattern_prop.type_id,
                            bindings,
                            &mut visited,
                            checker,
                        ) {
                            return false;
                        }
                    }
                    continue;
                }
                return false;
            };

            if self.type_contains_infer(pattern_prop.type_id) {
                let mut visited = FxHashSet::default();
                if !self.match_infer_pattern(
                    source_prop.type_id,
                    pattern_prop.type_id,
                    bindings,
                    &mut visited,
                    checker,
                ) {
                    return false;
                }
            } else if !checker.is_subtype_of(
                self.optional_property_type(source_prop),
                self.optional_property_type(pattern_prop),
            ) {
                return false;
            }
        }
        true
    }

    fn source_callable_shape_id(&self, source: TypeId) -> Option<CallableShapeId> {
        match self.interner().lookup(source) {
            Some(TypeData::Callable(shape_id)) => Some(shape_id),
            Some(TypeData::ReadonlyType(inner)) => self.source_callable_shape_id(inner),
            Some(TypeData::Intersection(members)) => self
                .interner()
                .type_list(members)
                .iter()
                .find_map(|&member| self.source_callable_shape_id(member)),
            _ => None,
        }
    }
}
