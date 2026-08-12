//! Type-specific infer pattern matching helpers (signature/function/callable).
//!
//! Contains specialized pattern matchers for:
//! - Function type patterns
//! - Constructor type patterns
//! - Callable type patterns
//! - Signature parameter / rest matching and template-capture binding helpers
//!
//! Object, object-with-index, and union matchers live in
//! `infer_pattern_object_match.rs`; template-literal matchers live in
//! `infer_pattern_template_match.rs`. These split modules stay under the
//! file-size ceiling while sharing the same `impl TypeEvaluator` module tree.

use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::types::{
    CallableShapeId, FunctionShape, FunctionShapeId, IntrinsicKind, LiteralValue, ParamInfo,
    TupleElement, TypeData, TypeId, TypeParamInfo, TypePredicate, TypePredicateTarget,
};
use crate::visitor::array_element_type;
use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;

use super::super::evaluate::TypeEvaluator;
use super::infer_pattern::InferPatternVisited;

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

    /// Mirror of tsc `inferFromSignature`'s type-predicate handling: when both
    /// the source signature and the inference pattern carry a type predicate
    /// that names a type (`x is T` / `asserts x is T`), the inference for the
    /// return position is taken from the predicate target types rather than the
    /// boolean return type. Returns the source predicate's target type
    /// (instantiated with the source signature's generic constraints) when the
    /// predicate kinds are compatible, else `None` (which makes the pattern fail
    /// so the conditional takes its false branch — a non-guard source does not
    /// match a `value is infer R` pattern).
    fn source_predicate_target_for_infer(
        &self,
        source_predicate: Option<TypePredicate>,
        pattern_predicate: TypePredicate,
        source_type_params: &[TypeParamInfo],
    ) -> Option<TypeId> {
        let source_predicate = source_predicate?;
        // Predicate "kinds" must match (tsc compares predicate kind): the
        // `asserts` modifier and the target shape (`this` vs an identifier).
        if source_predicate.asserts != pattern_predicate.asserts {
            return None;
        }
        let same_target_kind = matches!(
            (source_predicate.target, pattern_predicate.target),
            (TypePredicateTarget::This, TypePredicateTarget::This)
                | (
                    TypePredicateTarget::Identifier(_),
                    TypePredicateTarget::Identifier(_)
                )
        );
        if !same_target_kind {
            return None;
        }
        let target_type = source_predicate.type_id?;
        Some(self.erase_return_type_for_infer(target_type, source_type_params))
    }

    /// Extract a source `Function`/`Callable` signature's predicate target type
    /// (when present and kind-compatible with `pattern_predicate`) for
    /// predicate-`infer` matching, plus its instantiated params — but only when
    /// `needs_params` (the pattern's params carry an infer). The dominant
    /// predicate pattern `(value: any) => value is infer R` has no param infer,
    /// so skipping the param instantiation avoids a wasted `Vec` clone on this
    /// hot inference path. `None` when `source` is not a callable signature.
    fn source_sig_for_predicate_infer(
        &self,
        source: TypeId,
        pattern_predicate: TypePredicate,
        needs_params: bool,
    ) -> Option<(Vec<ParamInfo>, Option<TypeId>)> {
        let instantiate_params = |params: &[ParamInfo], return_type, type_params: &[_]| {
            if needs_params {
                self.instantiate_signature_for_infer(params, return_type, type_params)
                    .0
            } else {
                Vec::new()
            }
        };
        match self.interner().lookup(source)? {
            TypeData::Function(source_fn_id) => {
                let source_fn = self.interner().function_shape(source_fn_id);
                let params = instantiate_params(
                    &source_fn.params,
                    source_fn.return_type,
                    &source_fn.type_params,
                );
                let predicate_type = self.source_predicate_target_for_infer(
                    source_fn.type_predicate,
                    pattern_predicate,
                    &source_fn.type_params,
                );
                Some((params, predicate_type))
            }
            TypeData::Callable(source_shape_id) => {
                let source_shape = self.interner().callable_shape(source_shape_id);
                let source_sig = source_shape.call_signatures.last()?;
                let params = instantiate_params(
                    &source_sig.params,
                    source_sig.return_type,
                    &source_sig.type_params,
                );
                let predicate_type = self.source_predicate_target_for_infer(
                    source_sig.type_predicate,
                    pattern_predicate,
                    &source_sig.type_params,
                );
                Some((params, predicate_type))
            }
            _ => None,
        }
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
                type_id: instantiate_type(self.interner(), param.type_id, &subst),
                ..*param
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
        let mut local_visited = InferPatternVisited::default();
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

        // Arity gate, mirroring `compareSignaturesRelated`'s parameter-count
        // check (see `check_params_compatible`): for the extends relation to
        // hold, the source must be callable with the pattern's parameter list.
        // A source signature with no rest parameter whose required-argument
        // count exceeds a fixed-arity (no trailing rest) pattern's parameter
        // count demands more arguments than the pattern supplies, so tsc fails
        // the relation and the conditional takes the false branch. Without this
        // guard tsz truncates the source to the pattern prefix and wrongly
        // matches a higher-arity source (e.g. `(a, b) => {}` against
        // `(p0: infer P0) => any`). A trailing rest in the pattern absorbs extra
        // source params, so the cap only applies to fixed-arity patterns.
        if trailing_rest_param.is_none() && !source_params.last().is_some_and(|param| param.rest) {
            let source_required = checker.required_param_count(source_params);
            // tsc's `getMinArgumentCount` treats trailing parameters whose type
            // includes `void` as optional for arity, so a source like
            // `(a: string, b: void) => …` still satisfies a 1-arg pattern; only
            // a non-void extra required parameter fails the relation.
            if source_required > fixed_param_count
                && source_params
                    .iter()
                    .skip(fixed_param_count)
                    .take(source_required - fixed_param_count)
                    .any(|param| !checker.param_type_contains_void(param.type_id))
            {
                return false;
            }
        }

        // A source callable with fewer parameters is still assignable to the
        // inference pattern (extra trailing positions are ignored at the call
        // site); tsc takes the true branch and defaults the unmatched `infer`
        // slots to `unknown`. Match the overlapping prefix, default the rest.
        let matched_count = source_params.len().min(fixed_param_count);

        let mut local_visited = InferPatternVisited::default();
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
        visited: &mut InferPatternVisited,
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
        // A type-guard pattern `(v) => v is infer R` carries its infer variable
        // in the predicate target type, not the (boolean) return type. The
        // param/return-infer branches below never see it, so handle it here.
        let has_predicate_infer = pattern_fn
            .type_predicate
            .and_then(|predicate| predicate.type_id)
            .is_some_and(|type_id| self.type_contains_infer(type_id));

        if pattern_fn.this_type.is_none() && has_predicate_infer && !has_return_infer {
            // The boolean return holds no infer var; the inference target is the
            // predicate's asserted type, and (per tsc) it binds only when the
            // SOURCE is itself a type guard of the same predicate kind.
            let Some(pattern_predicate) = pattern_fn.type_predicate else {
                return false;
            };
            let Some(pattern_predicate_type) = pattern_predicate.type_id else {
                return false;
            };

            let mut match_predicate_sig = |source_params: &[ParamInfo],
                                           source_predicate_type: Option<TypeId>,
                                           bindings: &mut FxHashMap<Atom, TypeId>|
             -> bool {
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
                let Some(source_predicate_type) = source_predicate_type else {
                    return false;
                };
                self.match_infer_pattern(
                    source_predicate_type,
                    pattern_predicate_type,
                    bindings,
                    visited,
                    checker,
                )
            };

            // A union source (`Guard1 | Guard2`) binds the infer to the union of
            // each member's narrowed type; a single callable binds directly.
            if let Some(TypeData::Union(members)) = self.interner().lookup(source) {
                let members = self.interner().type_list(members);
                let mut combined = FxHashMap::default();
                for &member in members.iter() {
                    let Some((params, source_predicate_type)) = self
                        .source_sig_for_predicate_infer(member, pattern_predicate, has_param_infer)
                    else {
                        return false;
                    };
                    let mut member_bindings = FxHashMap::default();
                    if !match_predicate_sig(&params, source_predicate_type, &mut member_bindings) {
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
                return true;
            }

            let Some((params, source_predicate_type)) =
                self.source_sig_for_predicate_infer(source, pattern_predicate, has_param_infer)
            else {
                return false;
            };
            return match_predicate_sig(&params, source_predicate_type, bindings);
        }

        if pattern_fn.this_type.is_none() && has_param_infer && has_return_infer {
            let mut match_params_and_return = |_source_type: TypeId,
                                               source_params: &[ParamInfo],
                                               source_return: TypeId,
                                               bindings: &mut FxHashMap<Atom, TypeId>|
             -> bool {
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
                    visited,
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
                        arity_only_optional: false,
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
                        arity_only_optional: false,
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
                            visited,
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
                    self.match_infer_pattern(tuple_ty, infer_ty, bindings, visited, checker)
                };

                return match self.interner().lookup(source) {
                    Some(TypeData::Intrinsic(crate::types::IntrinsicKind::Function)) => {
                        // Function intrinsic is structurally (...args: any[]) => any
                        let function_params = vec![crate::types::ParamInfo {
                            name: None,
                            type_id: TypeId::ANY,
                            optional: false,
                            rest: true,
                            arity_only_optional: false,
                        }];
                        match_params_tuple(&function_params, &[], bindings)
                    }
                    Some(TypeData::Function(source_fn_id)) => {
                        let source_fn = self.interner().function_shape(source_fn_id);
                        match_params_tuple(&source_fn.params, &source_fn.type_params, bindings)
                    }
                    Some(TypeData::Callable(source_shape_id)) => {
                        let source_shape = self.interner().callable_shape(source_shape_id);
                        let Some(source_sig) = source_shape.call_signatures.last() else {
                            return false;
                        };
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
                                    let Some(source_sig) = source_shape.call_signatures.last()
                                    else {
                                        return false;
                                    };
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
                            arity_only_optional: false,
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
                    let Some(source_sig) = source_shape.call_signatures.last() else {
                        return false;
                    };
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
                if !self.match_infer_pattern(
                    source_return,
                    pattern_fn.return_type,
                    bindings,
                    visited,
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
                if !self.match_infer_pattern(source_this, pattern_this, bindings, visited, checker)
                {
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

            if has_return_infer
                && !self.match_infer_pattern(
                    source_return,
                    pattern_fn.return_type,
                    bindings,
                    visited,
                    checker,
                )
            {
                return false;
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
                let Some(source_sig) = source_shape.call_signatures.last() else {
                    return false;
                };
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
                            let Some(source_sig) = source_shape.call_signatures.last() else {
                                return false;
                            };
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
                let mut local_visited = InferPatternVisited::default();
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
                let mut local_visited = InferPatternVisited::default();
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
        visited: &mut InferPatternVisited,
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

        // Multi-overload infer capture: a pattern with two or more call (or
        // construct) signatures that each carry `infer` holes — e.g. the zustand
        // `StoreDevtools` shape
        //   S extends {
        //     (...args: infer Sa1): infer Sr1
        //     (...args: infer Sa2): infer Sr2
        //   } ? ... : never
        // matched against a two-overload `setState` callable. tsc binds each
        // `infer` by pairing pattern signature `i` against the source signature at
        // the corresponding position (the last `min(n, m)` overloads). The
        // single-signature paths below only handle `call_signatures.len() == 1`;
        // without this branch a multi-overload pattern falls through to the
        // `is_subtype_of` fallback, which cannot bind `infer` holes, so the
        // conditional wrongly collapses to its false branch (`never`).
        if pattern_shape.properties.is_empty()
            && let Some(result) = self.match_infer_multi_signature_pattern(
                source,
                pattern_shape_id,
                bindings,
                checker,
            )
        {
            return result;
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
                    visited,
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
                if !self.match_infer_pattern(
                    source_return,
                    pattern_sig.return_type,
                    bindings,
                    visited,
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

    /// Match a multi-overload pattern callable (two or more call or construct
    /// signatures, each bearing `infer` holes) against the `source` callable by
    /// pairing signatures positionally.
    ///
    /// Returns `Some(true)` when every paired signature binds its infer holes,
    /// `Some(false)` when the structural prerequisites hold but a pairing fails,
    /// and `None` when this is not a multi-overload infer pattern (so the caller
    /// continues with the single-signature paths / `is_subtype_of` fallback).
    ///
    /// tsc pairs the trailing `min(pattern_len, source_len)` overloads (the most
    /// specific signatures sit last); we mirror that by aligning both lists from
    /// the end. Candidates for the same `infer` name across signatures are
    /// unioned, matching how tsc accumulates inferences across overloads.
    fn match_infer_multi_signature_pattern(
        &self,
        source: TypeId,
        pattern_shape_id: CallableShapeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> Option<bool> {
        let pattern_shape = self.interner().callable_shape(pattern_shape_id);
        // Only the homogeneous multi-call or multi-construct shapes are handled
        // here; mixed call+construct patterns keep the existing fallback.
        let is_construct = pattern_shape.call_signatures.is_empty()
            && pattern_shape.construct_signatures.len() >= 2;
        let is_call = pattern_shape.construct_signatures.is_empty()
            && pattern_shape.call_signatures.len() >= 2;
        if !is_call && !is_construct {
            return None;
        }
        let pattern_sigs = if is_construct {
            &pattern_shape.construct_signatures
        } else {
            &pattern_shape.call_signatures
        };
        // Every pattern signature must carry an `infer` hole. A concrete overload
        // sitting beside infer-bearing ones would need a real subtype relation
        // (not infer binding), so defer the whole pattern to the existing
        // single-signature / `is_subtype_of` fallback instead of guessing.
        if !pattern_sigs
            .iter()
            .all(|sig| self.signature_contains_infer(sig))
        {
            return None;
        }

        let Some(source_shape_id) = self.source_callable_shape_id(source) else {
            return Some(false);
        };
        let source_shape = self.interner().callable_shape(source_shape_id);
        let source_sigs = if is_construct {
            &source_shape.construct_signatures
        } else {
            &source_shape.call_signatures
        };
        if source_sigs.is_empty() {
            return Some(false);
        }

        // Pair from the end: pattern signature `pattern_len-1-k` against source
        // signature `source_len-1-k` for k in 0..min(len).
        let pair_count = pattern_sigs.len().min(source_sigs.len());
        let mut combined: FxHashMap<Atom, TypeId> = bindings.clone();
        for k in 0..pair_count {
            let pattern_sig = &pattern_sigs[pattern_sigs.len() - 1 - k];
            let source_sig = &source_sigs[source_sigs.len() - 1 - k];
            let mut pair_bindings = bindings.clone();
            if !self.match_infer_signature_pair(
                pattern_sig,
                source_sig,
                &mut pair_bindings,
                checker,
            ) {
                return Some(false);
            }
            for (name, ty) in pair_bindings {
                combined
                    .entry(name)
                    .and_modify(|existing| {
                        if *existing != ty {
                            *existing = self.interner().union2(*existing, ty);
                        }
                    })
                    .or_insert(ty);
            }
        }
        *bindings = combined;
        Some(true)
    }

    /// Whether any param/return position of a call signature carries an `infer`.
    fn signature_contains_infer(&self, sig: &crate::types::CallSignature) -> bool {
        sig.params
            .iter()
            .any(|param| self.type_contains_infer(param.type_id))
            || self.type_contains_infer(sig.return_type)
    }

    /// Bind the `infer` holes of a single pattern call signature against a single
    /// source call signature (params + return), reusing the same param/return
    /// matchers as the single-signature paths.
    fn match_infer_signature_pair(
        &self,
        pattern_sig: &crate::types::CallSignature,
        source_sig: &crate::types::CallSignature,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let has_param_infer = pattern_sig
            .params
            .iter()
            .any(|param| self.type_contains_infer(param.type_id));
        let has_return_infer = self.type_contains_infer(pattern_sig.return_type);
        let has_single_rest_infer = pattern_sig.params.len() == 1
            && pattern_sig.params[0].rest
            && self.type_contains_infer(pattern_sig.params[0].type_id);

        let (source_params, source_return) = self.instantiate_signature_for_infer(
            &source_sig.params,
            source_sig.return_type,
            &source_sig.type_params,
        );

        if has_param_infer {
            if has_single_rest_infer {
                if !self.match_rest_infer_tuple(
                    &source_params,
                    pattern_sig.params[0].type_id,
                    bindings,
                    checker,
                ) {
                    return false;
                }
            } else if !self.match_signature_params_for_infer(
                &source_params,
                &pattern_sig.params,
                bindings,
                checker,
            ) {
                return false;
            }
        }

        if has_return_infer {
            let mut visited = InferPatternVisited::default();
            if !self.match_infer_pattern(
                source_return,
                pattern_sig.return_type,
                bindings,
                &mut visited,
                checker,
            ) {
                return false;
            }
        }

        // Every signature reaching here carries at least one infer hole (the
        // driver only pairs all-infer patterns), so a successful param/return
        // match is sufficient. The final subtype check is intentionally skipped
        // to avoid contravariance false-negatives, mirroring the single-signature
        // paths above.
        true
    }
}
