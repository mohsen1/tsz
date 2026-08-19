use super::relation_policy;
use crate::class_checker::ClassMemberInfo;
use crate::state::CheckerState;
use tsz_parser::NodeIndex;
use tsz_solver::TypeId;
use tsz_solver::construction::{QueryDatabase, TypeDatabase};

pub(crate) fn maybe_substitute_this_type(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    self_type: Option<TypeId>,
) -> TypeId {
    let Some(st) = self_type else {
        return type_id;
    };
    if crate::query_boundaries::common::contains_this_type(db.as_type_database(), type_id) {
        crate::query_boundaries::common::substitute_this_type(db, type_id, st)
    } else {
        type_id
    }
}

/// Build a `name -> member type` map from a class/interface instance type's
/// object shape.
///
/// The instance-type shape already merges method overloads into a single
/// callable (and hides the implementation signature), so it is the canonical
/// source for an externally-visible member type. Both the own-class and the
/// inherited-member `implements` paths use this so they aggregate overloads
/// identically. Returns an empty map when the type has no object shape.
pub(crate) fn instance_member_types_by_name(
    db: &dyn QueryDatabase,
    instance_type: TypeId,
) -> rustc_hash::FxHashMap<String, TypeId> {
    crate::query_boundaries::common::object_shape_for_type(db.as_type_database(), instance_type)
        .map(|shape| {
            shape
                .properties
                .iter()
                .map(|prop| (db.resolve_atom(prop.name), prop.type_id))
                .collect()
        })
        .unwrap_or_default()
}

/// Collect a type's call signatures, handling both `CallableShape`
/// (overloaded / object-with-call-sigs) and the single-signature `FunctionShape`
/// that an interface method declaration lowers to. `get_call_signatures` alone
/// misses the `FunctionShape` case and would yield an empty signature list.
pub(crate) fn member_call_signatures(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Vec<tsz_solver::CallSignature> {
    if let Some(sigs) = tsz_solver::type_queries::get_call_signatures(db, type_id) {
        return sigs;
    }
    if let Some(fs) = tsz_solver::type_queries::get_function_shape(db, type_id) {
        return vec![tsz_solver::CallSignature {
            type_params: fs.type_params.clone(),
            params: fs.params.clone(),
            this_type: fs.this_type,
            return_type: fs.return_type,
            type_predicate: fs.type_predicate,
            is_method: fs.is_method,
            declaration_group: 0,
        }];
    }
    Vec::new()
}

fn has_own_signature_type_params(checker: &CheckerState<'_>, type_id: TypeId) -> bool {
    if let Some(shape) = tsz_solver::type_queries::get_callable_shape(checker.ctx.types, type_id) {
        return shape
            .call_signatures
            .iter()
            .chain(shape.construct_signatures.iter())
            .any(|sig| !sig.type_params.is_empty());
    }
    if let Some(shape) = tsz_solver::type_queries::get_function_shape(checker.ctx.types, type_id) {
        return !shape.type_params.is_empty();
    }
    false
}

/// Returns true when the standard (generic-erasing) assignability relation is
/// safe to use as a fallback for member-override compatibility checks.
///
/// tsc's `compareSignaturesRelated` only canonicalizes (erases) the target's
/// method-local type parameters when the *source* signature carries its own.
/// Otherwise the target's type parameters stay universally quantified, so the
/// standard tsz relation (which erases method-local generics) must not undo
/// the strict relation's correct rejection. The fallback remains safe — and
/// active for `any`-propagation cases like `IteratorResult<T, any>` vs
/// `IteratorResult<T, void>` — when target has no method-local type parameters
/// or when source has its own.
pub(crate) fn generic_erasure_fallback_is_safe(
    checker: &CheckerState<'_>,
    source: TypeId,
    target: TypeId,
) -> bool {
    let source = unwrap_single_property_value_type(checker, source);
    let target = unwrap_single_property_value_type(checker, target);
    !has_method_local_type_params(checker.ctx.types, target)
        || has_method_local_type_params(checker.ctx.types, source)
}

/// Convenience wrapper around `callable_signature_is_generic` that treats
/// non-callable types as "no method-local generics" — the right default for
/// member-override gating where a non-callable member can never leak
/// universal quantification.
fn has_method_local_type_params(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    callable_signature_is_generic(db, type_id).unwrap_or(false)
}

/// Resolve a signature's method-local type parameters to their `TypeId`s, the
/// form they take when reached through `collect_all_types`. Shared by the
/// local/nonlocal type-parameter membership predicates below.
fn signature_local_type_param_ids(
    checker: &CheckerState<'_>,
    type_params: &[tsz_solver::types::TypeParamInfo],
) -> Vec<TypeId> {
    type_params
        .iter()
        .map(|tp| checker.ctx.types.type_param(*tp))
        .collect()
}

fn callable_mentions_nonlocal_type_params(checker: &CheckerState<'_>, type_id: TypeId) -> bool {
    let signature_mentions_nonlocal = |type_params: &[tsz_solver::types::TypeParamInfo],
                                       params: &[tsz_solver::types::ParamInfo],
                                       this_type: Option<TypeId>,
                                       return_type: TypeId| {
        let local_tp_ids = signature_local_type_param_ids(checker, type_params);
        let mentions_nonlocal = |referenced: TypeId| {
            tsz_solver::visitor::collect_all_types(checker.ctx.types, referenced)
                .into_iter()
                .any(|ty| {
                    tsz_solver::type_param_info(checker.ctx.types, ty).is_some()
                        && !local_tp_ids.contains(&ty)
                })
        };

        params.iter().any(|param| mentions_nonlocal(param.type_id))
            || this_type.is_some_and(mentions_nonlocal)
            || mentions_nonlocal(return_type)
    };

    if let Some(shape) = tsz_solver::type_queries::get_callable_shape(checker.ctx.types, type_id) {
        return shape
            .call_signatures
            .iter()
            .chain(shape.construct_signatures.iter())
            .any(|sig| {
                signature_mentions_nonlocal(
                    &sig.type_params,
                    &sig.params,
                    sig.this_type,
                    sig.return_type,
                )
            });
    }
    if let Some(shape) = tsz_solver::type_queries::get_function_shape(checker.ctx.types, type_id) {
        return signature_mentions_nonlocal(
            &shape.type_params,
            &shape.params,
            shape.this_type,
            shape.return_type,
        );
    }
    false
}

/// True when the base method's single call signature uses one of its own
/// method-local type parameters in its return type.
///
/// Interface-heritage override checking (`TS2430`) drops a base method's
/// method-local generics to decide whether a non-generic override is a valid
/// specialization. The strict `no_erase` return relation keeps those generics
/// opaque to reject a covariant misuse (`m(): string` overriding `m<T>(): T`,
/// where the dropped `T` appears in the return). But when the return does NOT
/// reference the dropped generic — e.g. a self-returning method
/// `with<K extends string>(...): Base<T>` — the no-erase mode is spuriously
/// strict on generics reached through the named return type, and the return is
/// an ordinary covariant position. This boundary lets the checker make that
/// distinction structurally (keyed on signature shape, not identifiers).
///
/// Conservative (`true`, keeping the strict relation) for overloaded or
/// constructor members and for non-callable shapes without a single call
/// signature.
pub(crate) fn callable_return_mentions_own_method_local_generic(
    checker: &CheckerState<'_>,
    base: TypeId,
) -> bool {
    // Resolve the local type-param ids and return type without cloning the
    // type-parameter list out of the `Arc`-backed shape.
    let (local_tp_ids, return_type) = if let Some(shape) =
        crate::query_boundaries::common::function_shape_for_type(checker.ctx.types, base)
    {
        (
            signature_local_type_param_ids(checker, &shape.type_params),
            shape.return_type,
        )
    } else if let Some(shape) =
        crate::query_boundaries::common::callable_shape_for_type(checker.ctx.types, base)
    {
        if shape.call_signatures.len() != 1 || !shape.construct_signatures.is_empty() {
            return true;
        }
        let sig = &shape.call_signatures[0];
        (
            signature_local_type_param_ids(checker, &sig.type_params),
            sig.return_type,
        )
    } else {
        return true;
    };
    if local_tp_ids.is_empty() {
        return false;
    }
    tsz_solver::visitor::collect_all_types(checker.ctx.types, return_type)
        .into_iter()
        .any(|ty| local_tp_ids.contains(&ty))
}

fn unwrap_single_property_value_type(checker: &CheckerState<'_>, type_id: TypeId) -> TypeId {
    if let Some(shape) =
        crate::query_boundaries::common::object_shape_for_type(checker.ctx.types, type_id)
        && shape.properties.len() == 1
        && !shape.properties[0].is_method
    {
        return shape.properties[0].type_id;
    }
    type_id
}

fn needs_strict_generic_target_callable_recheck(
    checker: &CheckerState<'_>,
    source: TypeId,
    target: TypeId,
) -> bool {
    let source = unwrap_single_property_value_type(checker, source);
    let target = unwrap_single_property_value_type(checker, target);
    let is_callable_like = |type_id: TypeId| {
        tsz_solver::type_queries::get_callable_shape(checker.ctx.types, type_id).is_some()
            || tsz_solver::type_queries::get_function_shape(checker.ctx.types, type_id).is_some()
    };

    is_callable_like(source)
        && is_callable_like(target)
        && callable_mentions_nonlocal_type_params(checker, source)
        && has_own_signature_type_params(checker, target)
}

fn construct_signature_source_captures_nonlocal_type_param(
    checker: &CheckerState<'_>,
    source: TypeId,
    target: TypeId,
) -> bool {
    let source = unwrap_single_property_value_type(checker, source);
    let target = unwrap_single_property_value_type(checker, target);
    let Some(source_signatures) =
        crate::query_boundaries::common::construct_signatures_for_type(checker.ctx.types, source)
    else {
        return false;
    };
    let Some(target_signatures) =
        crate::query_boundaries::common::construct_signatures_for_type(checker.ctx.types, target)
    else {
        return false;
    };
    if !target_signatures
        .iter()
        .any(|target_sig| !target_sig.type_params.is_empty())
    {
        return false;
    }

    source_signatures.iter().any(|source_sig| {
        source_sig.type_params.is_empty()
            && (source_sig.params.iter().any(|param| {
                crate::query_boundaries::common::contains_type_parameters(
                    checker.ctx.types,
                    param.type_id,
                )
            }) || source_sig.this_type.is_some_and(|this_type| {
                crate::query_boundaries::common::contains_type_parameters(
                    checker.ctx.types,
                    this_type,
                )
            }) || crate::query_boundaries::common::contains_type_parameters(
                checker.ctx.types,
                source_sig.return_type,
            ))
    })
}

fn generic_construct_requires_optional_target_param_recheck(
    checker: &CheckerState<'_>,
    source: TypeId,
    target: TypeId,
) -> bool {
    let source = unwrap_single_property_value_type(checker, source);
    let target = unwrap_single_property_value_type(checker, target);
    let Some(source_signatures) =
        crate::query_boundaries::common::construct_signatures_for_type(checker.ctx.types, source)
    else {
        return false;
    };
    let Some(target_signatures) =
        crate::query_boundaries::common::construct_signatures_for_type(checker.ctx.types, target)
    else {
        return false;
    };

    source_signatures.iter().any(|source_sig| {
        target_signatures.iter().any(|target_sig| {
            construct_signature_required_param_against_optional_target(
                checker, source_sig, target_sig,
            )
        })
    })
}

fn construct_signature_required_param_against_optional_target(
    checker: &CheckerState<'_>,
    source_sig: &tsz_solver::types::CallSignature,
    target_sig: &tsz_solver::types::CallSignature,
) -> bool {
    if source_sig.type_params.is_empty()
        || source_sig.type_params.len() != target_sig.type_params.len()
        || source_sig.params.len() != target_sig.params.len()
    {
        return false;
    }

    source_sig
        .params
        .iter()
        .zip(target_sig.params.iter())
        .any(|(source_param, target_param)| {
            if source_param.optional || !target_param.optional {
                return false;
            }

            source_sig
                .type_params
                .iter()
                .zip(target_sig.type_params.iter())
                .any(|(source_tp, target_tp)| {
                    let source_tp_type = checker.ctx.types.type_param(*source_tp);
                    let target_tp_type = checker.ctx.types.type_param(*target_tp);
                    crate::query_boundaries::common::contains_type_by_id(
                        checker.ctx.types,
                        source_param.type_id,
                        source_tp_type,
                    ) && crate::query_boundaries::common::contains_type_by_id(
                        checker.ctx.types,
                        target_param.type_id,
                        target_tp_type,
                    ) && crate::query_boundaries::common::contains_type_by_id(
                        checker.ctx.types,
                        source_sig.return_type,
                        source_tp_type,
                    ) && crate::query_boundaries::common::contains_type_by_id(
                        checker.ctx.types,
                        target_sig.return_type,
                        target_tp_type,
                    )
                })
        })
}

fn source_this_parameter_is_acceptable_for_target_without_this(
    checker: &mut CheckerState<'_>,
    source: TypeId,
    target: TypeId,
) -> bool {
    fn return_types_have_matching_application_base(
        checker: &CheckerState<'_>,
        source_return: TypeId,
        target_return: TypeId,
    ) -> bool {
        let source_base =
            crate::query_boundaries::common::application_info(checker.ctx.types, source_return)
                .map(|(base, _)| base);
        let target_base =
            crate::query_boundaries::common::application_info(checker.ctx.types, target_return)
                .map(|(base, _)| base);
        source_base.is_some() && source_base == target_base
    }

    fn signatures_have_matching_generic_shape(
        checker: &CheckerState<'_>,
        source_shape: &tsz_solver::FunctionShape,
        target_shape: &tsz_solver::FunctionShape,
    ) -> bool {
        source_shape.this_type.is_some()
            && target_shape.this_type.is_none()
            && !source_shape.type_params.is_empty()
            && !target_shape.type_params.is_empty()
            && source_shape.params.len() == target_shape.params.len()
            && return_types_have_matching_application_base(
                checker,
                source_shape.return_type,
                target_shape.return_type,
            )
    }

    if let (Some(source_shape), Some(target_shape)) = (
        crate::query_boundaries::common::function_shape_for_type(checker.ctx.types, source),
        crate::query_boundaries::common::function_shape_for_type(checker.ctx.types, target),
    ) {
        if source_shape.this_type.is_none() || target_shape.this_type.is_some() {
            return false;
        }

        let mut stripped = (*source_shape).clone();
        stripped.this_type = None;
        let stripped_source = checker.ctx.types.factory().function(stripped);
        return checker
            .no_erase_generics_relation_outcome(stripped_source, target)
            .related
            || checker
                .no_erase_generics_relation_outcome(target, stripped_source)
                .related
            || signatures_have_matching_generic_shape(checker, &source_shape, &target_shape);
    }

    let (Some(source_shape), Some(target_shape)) = (
        crate::query_boundaries::common::callable_shape_for_type(checker.ctx.types, source),
        crate::query_boundaries::common::callable_shape_for_type(checker.ctx.types, target),
    ) else {
        return false;
    };
    if source_shape.call_signatures.is_empty()
        || source_shape
            .call_signatures
            .iter()
            .all(|sig| sig.this_type.is_none())
        || target_shape
            .call_signatures
            .iter()
            .any(|sig| sig.this_type.is_some())
    {
        return false;
    }

    let mut stripped = (*source_shape).clone();
    for sig in &mut stripped.call_signatures {
        sig.this_type = None;
    }
    let stripped_source = checker.ctx.types.factory().callable(stripped);
    checker
        .no_erase_generics_relation_outcome(stripped_source, target)
        .related
        || checker
            .no_erase_generics_relation_outcome(target, stripped_source)
            .related
        || source_shape.call_signatures.iter().any(|source_sig| {
            target_shape.call_signatures.iter().any(|target_sig| {
                let source_fn = tsz_solver::FunctionShape {
                    type_params: source_sig.type_params.clone(),
                    params: source_sig.params.clone(),
                    this_type: source_sig.this_type,
                    return_type: source_sig.return_type,
                    type_predicate: source_sig.type_predicate,
                    is_constructor: false,
                    is_method: source_sig.is_method,
                };
                let target_fn = tsz_solver::FunctionShape {
                    type_params: target_sig.type_params.clone(),
                    params: target_sig.params.clone(),
                    this_type: target_sig.this_type,
                    return_type: target_sig.return_type,
                    type_predicate: target_sig.type_predicate,
                    is_constructor: false,
                    is_method: target_sig.is_method,
                };
                signatures_have_matching_generic_shape(checker, &source_fn, &target_fn)
            })
        })
}

// =============================================================================
// Relation boundary helpers (thin wrappers over assignability)
// =============================================================================

/// Check if a member type mismatch should be reported (TS2416).
///
/// Uses `no_erase_generics` mode to match tsc's `compareSignaturesRelated`
/// behavior for implements/extends member checking: a non-generic function
/// like `(x: string) => string` is NOT assignable to a generic function
/// like `<T>(x: T) => T`, ensuring TS2416 is correctly emitted.
pub(crate) fn should_report_member_type_mismatch(
    checker: &mut CheckerState<'_>,
    source: TypeId,
    target: TypeId,
    node_idx: NodeIndex,
) -> bool {
    let source = checker.narrow_this_from_enclosing_typeof_guard(node_idx, source);
    if checker.should_suppress_assignability_diagnostic(source, target) {
        return false;
    }
    if checker.should_suppress_assignability_for_parse_recovery(node_idx, node_idx) {
        return false;
    }
    if implementation_signature_has_incompatible_erased_overload_return(checker, source, target) {
        return true;
    }
    if checker
        .no_erase_generics_relation_outcome(source, target)
        .related
    {
        return false;
    }
    if source_this_parameter_is_acceptable_for_target_without_this(checker, source, target) {
        return false;
    }
    if checker.should_skip_weak_union_error(source, target, node_idx) {
        return false;
    }

    // Coinductive suppression: when checking class member compatibility (TS2416),
    // the class instance type may have been computed during circular resolution,
    // resulting in an incomplete type (0 properties). If the source is a function
    // whose return type has 0 properties but the return type is a class that extends
    // the class being checked (which implements the target interface), suppress the
    // diagnostic. This matches tsc's coinductive cycle handling for recursive class
    // hierarchies like:
    //   interface I { foo(): I; }
    //   class A implements I { foo(): B { ... } }
    //   class B extends A { }
    if is_coinductive_return_type_cycle(checker, source, target) {
        return false;
    }

    true
}

/// Check whether a derived interface member's overload set is assignable to a
/// base member's overload set, through the class relation boundary.
///
/// `source` and `target` are the full overload callables for the member (see
/// `build_method_overload_callable`), so the strict `no_erase_generics` probe
/// applies tsc's N×M `signaturesRelatedTo` rule. The optional retry mirrors
/// `tsc`'s fresh generic instantiation for equivalent method-local generic
/// overload shapes.
pub(crate) fn interface_overload_set_assignable(
    checker: &mut CheckerState<'_>,
    source: TypeId,
    target: TypeId,
    allow_fresh_generic_retry: bool,
) -> bool {
    let strict_related = checker
        .no_erase_generics_relation_outcome(source, target)
        .related;
    if strict_related {
        tracing::trace!(?source, ?target, "interface overload set related strictly");
        return true;
    }
    let has_generic_signature = checker.callable_has_own_generic_signatures(source)
        || checker.callable_has_own_generic_signatures(target);
    if !allow_fresh_generic_retry || !has_generic_signature {
        return false;
    }
    let retry_related = checker
        .interface_heritage_generic_method_relation_outcome(source, target)
        .related;
    tracing::trace!(
        ?source,
        ?target,
        retry_related,
        "interface overload set fresh-generic retry"
    );
    retry_related
}

fn call_signature_function_type(
    checker: &mut CheckerState<'_>,
    sig: &tsz_solver::CallSignature,
) -> TypeId {
    checker
        .ctx
        .types
        .factory()
        .function(tsz_solver::FunctionShape {
            type_params: sig.type_params.clone(),
            params: sig.params.clone(),
            this_type: sig.this_type,
            return_type: sig.return_type,
            type_predicate: sig.type_predicate,
            is_constructor: false,
            is_method: sig.is_method,
        })
}

/// True when a single class implementation-style method covers an overloaded
/// interface method. This mirrors `tsc`'s overload implementation compatibility:
/// each exposed overload must be compatible with the implementation signature,
/// rather than requiring the implementation signature to be a subtype of the
/// whole overload set.
pub(crate) fn implementation_signature_covers_interface_overloads(
    checker: &mut CheckerState<'_>,
    source: TypeId,
    target: TypeId,
) -> bool {
    let source = unwrap_single_property_value_type(checker, source);
    let target = unwrap_single_property_value_type(checker, target);
    let source_sigs = member_call_signatures(checker.ctx.types, source);
    let target_sigs = member_call_signatures(checker.ctx.types, target);
    // Require a single source signature and a genuinely overloaded target
    // (multiple call signatures). Single-target comparisons are ordinary
    // override compatibility — the strict relation must govern there so
    // target's universally-quantified method-local type parameters stay
    // opaque (tsc rejects `(x: number) => Box<string>` as an override of
    // `<T extends string>(x: number) => Box<T>`).
    if source_sigs.len() != 1 || target_sigs.len() < 2 {
        return false;
    }

    let source_type = call_signature_function_type(checker, &source_sigs[0]);
    target_sigs.iter().all(|target_sig| {
        let target_type = call_signature_function_type(checker, target_sig);
        // Do not reuse the broad overload-implementation relation here: Array's
        // callback overload surface needs a real TS2416 when the implementation
        // narrows callback returns. This helper is only for builder-style returns
        // that share an application base after erasing local type params.
        overload_return_base_matches_and_params_cover(checker, source_type, target_type)
    })
}

fn overload_return_base_matches_and_params_cover(
    checker: &mut CheckerState<'_>,
    source_type: TypeId,
    target_type: TypeId,
) -> bool {
    let policy = relation_policy::from_checker_flags_u16(checker.ctx.pack_relation_flags());
    let context = tsz_solver::relations::relation_queries::RelationContext {
        query_db: Some(checker.ctx.types),
        evaluation_session: Some(checker.ctx.eval_session.as_ref()),
        inheritance_graph: Some(&checker.ctx.inheritance_graph),
        class_check: None,
    };
    tsz_solver::relations::relation_queries::query_erased_overload_params_with_matching_return_base(
        checker.ctx.types.as_type_database(),
        &checker.ctx,
        source_type,
        target_type,
        policy,
        context,
    )
    .is_related()
}

/// True when an implementation/overload pair has a determinate erased
/// conditional return with a proven `any`/`never` variance mismatch.
///
/// The regular coverage helper intentionally returns only a boolean, so a false
/// result can mean either an ordinary parameter mismatch or this stronger return
/// rejection. Preserve that distinction before the whole-member compatibility
/// relation structurally expands the return applications and loses their generic
/// identity.
pub(crate) fn implementation_signature_has_incompatible_erased_overload_return(
    checker: &mut CheckerState<'_>,
    source: TypeId,
    target: TypeId,
) -> bool {
    let source = unwrap_single_property_value_type(checker, source);
    let target = unwrap_single_property_value_type(checker, target);
    let source_sigs = member_call_signatures(checker.ctx.types, source);
    let target_sigs = member_call_signatures(checker.ctx.types, target);
    if source_sigs.len() != 1 || target_sigs.len() < 2 {
        return false;
    }

    let source_type = call_signature_function_type(checker, &source_sigs[0]);
    target_sigs.iter().any(|target_sig| {
        let target_type = call_signature_function_type(checker, target_sig);
        let policy = relation_policy::from_checker_flags_u16(checker.ctx.pack_relation_flags());
        let context = tsz_solver::relations::relation_queries::RelationContext {
            query_db: Some(checker.ctx.types),
            evaluation_session: Some(checker.ctx.eval_session.as_ref()),
            inheritance_graph: Some(&checker.ctx.inheritance_graph),
            class_check: None,
        };
        tsz_solver::relations::relation_queries::query_erased_overload_return_variance_rejects(
            checker.ctx.types.as_type_database(),
            &checker.ctx,
            source_type,
            target_type,
            policy,
            context,
        )
    })
}

/// Check if a DIRECT (own) member type mismatch should be reported (TS2416).
///
/// Unlike `should_report_member_type_mismatch`, this variant uses a targeted
/// suppression that does NOT suppress callable types whose source contains
/// type parameters from the class scope. For class's own members, the type
/// parameters are fully declared and their constraints must be checked
/// eagerly against the interface member types, matching tsc behavior.
///
/// The regular `should_report_member_type_mismatch` should still be used for
/// inherited members, where base class type parameters may not have been
/// instantiated and the callable suppression is needed.
pub(crate) fn should_report_own_member_type_mismatch(
    checker: &mut CheckerState<'_>,
    source: TypeId,
    target: TypeId,
    node_idx: NodeIndex,
) -> bool {
    let source = checker.narrow_this_from_enclosing_typeof_guard(node_idx, source);
    if checker.should_suppress_member_assignability(source, target) {
        return false;
    }
    if checker.should_suppress_assignability_for_parse_recovery(node_idx, node_idx) {
        return false;
    }
    if implementation_signature_covers_interface_overloads(checker, source, target) {
        return false;
    }
    if implementation_signature_has_incompatible_erased_overload_return(checker, source, target) {
        return true;
    }
    if checker
        .no_erase_generics_relation_outcome(source, target)
        .related
    {
        return false;
    }
    // Fallback for any-propagation cases (e.g. `IteratorResult<T, any>` vs
    // `IteratorResult<T, void>`) where the strict path keeps args nominally
    // distinct but tsc accepts. Gated by `generic_erasure_fallback_is_safe`
    // so the fallback does not leak universal quantification when target has
    // method-local type parameters and source does not.
    if generic_erasure_fallback_is_safe(checker, source, target)
        && checker
            .class_implements_whole_type_relation_outcome(source, target)
            .related
    {
        return false;
    }
    if source_this_parameter_is_acceptable_for_target_without_this(checker, source, target) {
        return false;
    }
    if checker.should_skip_weak_union_error(source, target, node_idx) {
        return false;
    }
    if is_coinductive_return_type_cycle(checker, source, target) {
        return false;
    }
    true
}

/// Check if two function types differ only in return types that form a coinductive
/// cycle through the class hierarchy (class extends another class that implements
/// the interface defining the target return type).
fn is_coinductive_return_type_cycle(
    checker: &mut CheckerState<'_>,
    source: TypeId,
    target: TypeId,
) -> bool {
    // Get source return type from Function shape
    let source_ret = tsz_solver::function_shape_id(checker.ctx.types, source)
        .map(|id| checker.ctx.types.function_shape(id).return_type);

    // Get target return type from Function or Callable shape
    let target_ret = tsz_solver::function_shape_id(checker.ctx.types, target)
        .map(|id| checker.ctx.types.function_shape(id).return_type)
        .or_else(|| {
            tsz_solver::callable_shape_id(checker.ctx.types, target).and_then(|id| {
                checker
                    .ctx
                    .types
                    .callable_shape(id)
                    .call_signatures
                    .first()
                    .map(|s| s.return_type)
            })
        });

    let (Some(s_ret), Some(_t_ret)) = (source_ret, target_ret) else {
        return false;
    };

    // Check if the source return type is an incomplete class type from circular
    // resolution. This can be:
    // 1. An Object/ObjectWithIndex with 0 properties (non-generic case)
    // 2. An Application type whose evaluated form has 0 properties (generic case)
    let source_ret_is_incomplete = is_incomplete_class_type(checker, s_ret);

    if !source_ret_is_incomplete {
        return false;
    }

    // Check parameter compatibility (everything except return type).
    // If parameters are incompatible, this isn't a coinductive cycle issue.
    let source_fn = tsz_solver::function_shape_id(checker.ctx.types, source)
        .map(|id| checker.ctx.types.function_shape(id));
    let target_fn = tsz_solver::function_shape_id(checker.ctx.types, target)
        .map(|id| checker.ctx.types.function_shape(id));
    let target_callable = tsz_solver::callable_shape_id(checker.ctx.types, target)
        .map(|id| checker.ctx.types.callable_shape(id));

    // Get source params
    let source_params = source_fn.as_ref().map(|f| &f.params);
    // Get target params
    let target_params = target_fn.as_ref().map(|f| &f.params).or_else(|| {
        target_callable
            .as_ref()
            .and_then(|c| c.call_signatures.first().map(|s| &s.params))
    });

    if let (Some(s_params), Some(t_params)) = (source_params, target_params) {
        // Quick check: if param count differs significantly, not a cycle issue
        if s_params.len() != t_params.len() {
            return false;
        }
        // Check each param for assignability
        for (sp, tp) in s_params.iter().zip(t_params.iter()) {
            if sp.type_id != tp.type_id
                && !checker
                    .function_type_compatibility_relation_outcome(tp.type_id, sp.type_id)
                    .related
            {
                return false;
            }
        }
    }

    // Parameters are compatible but return types differ. The source return type is
    // an empty class instance type. This is likely a coinductive cycle where the
    // class implementing the interface returns a subclass, and the subclass's
    // instance type was computed during circular resolution (resulting in an empty
    // object shape). Suppress the TS2416 diagnostic.
    true
}

/// Check if a property type mismatch should be reported (TS2430).
///
/// Uses regular `is_assignable_to` (NOT `no_erase_generics`) because property
/// types in interface extends are compared with standard assignability in tsc.
/// This allows generic function types like `<T>(a: T) => T` to be correctly
/// recognized as assignable to concrete function types like `(a: Derived) => Derived`
/// through generic instantiation, matching tsc's `isTypeRelatedTo` behavior
/// for property type checking (as opposed to `compareSignaturesRelated` used
/// for method signatures).
pub(crate) fn should_report_property_type_mismatch(
    checker: &mut CheckerState<'_>,
    source: TypeId,
    target: TypeId,
    node_idx: NodeIndex,
) -> bool {
    let narrowed_source = checker.narrow_this_from_enclosing_typeof_guard(node_idx, source);
    let relation_source = unwrap_single_property_value_type(checker, narrowed_source);
    let relation_target = unwrap_single_property_value_type(checker, target);
    // TS2430 property compatibility is still a member-compatibility check, even
    // though it uses regular assignability instead of no-erase-generics. Using
    // the broader TS2322 suppression here hides real interface-extends failures
    // when the derived property mentions outer type parameters.
    if checker.should_suppress_member_assignability(relation_source, relation_target) {
        return false;
    }
    if checker.should_suppress_assignability_for_parse_recovery(node_idx, node_idx) {
        return false;
    }

    let request = {
        use crate::query_boundaries::assignability::RelationRequest;
        let (prepared_source, prepared_target) =
            checker.prepare_assignability_inputs(relation_source, relation_target);
        RelationRequest::assign(prepared_source, prepared_target)
            .with_erased_generic_signature_retry()
    };
    let outcome = checker.execute_relation_request(&request);

    if outcome.related {
        if generic_construct_requires_optional_target_param_recheck(
            checker,
            relation_source,
            relation_target,
        ) {
            return true;
        }
        if construct_signature_source_captures_nonlocal_type_param(
            checker,
            relation_source,
            relation_target,
        ) {
            return true;
        }
        if needs_strict_generic_target_callable_recheck(checker, relation_source, relation_target) {
            let strict_source = unwrap_single_property_value_type(checker, relation_source);
            let strict_target = unwrap_single_property_value_type(checker, relation_target);
            return !checker
                .no_erase_generics_relation_outcome(strict_source, strict_target)
                .related;
        }
        return false;
    }
    if outcome.weak_union_violation
        || checker.should_skip_weak_union_error_with_outcome(
            relation_source,
            relation_target,
            node_idx,
            Some(&outcome),
        )
    {
        return false;
    }
    if is_coinductive_return_type_cycle(checker, relation_source, relation_target) {
        return false;
    }
    true
}

pub(crate) fn should_report_member_type_mismatch_bivariant(
    checker: &mut CheckerState<'_>,
    source: TypeId,
    target: TypeId,
    node_idx: NodeIndex,
) -> bool {
    checker.should_report_assignability_mismatch_bivariant(source, target, node_idx)
}

/// Check if a type is an incomplete class instance type that resulted from
/// circular resolution (0 properties, likely because inherited members from
/// a base class that was still being resolved were dropped).
pub(crate) fn is_incomplete_class_type(checker: &mut CheckerState<'_>, type_id: TypeId) -> bool {
    match checker.ctx.types.lookup(type_id) {
        Some(tsz_solver::TypeData::Object(shape_id))
        | Some(tsz_solver::TypeData::ObjectWithIndex(shape_id)) => {
            // Only a *class* instance type that came back empty during circular
            // resolution counts as incomplete. A class instance carries its class
            // symbol even when its property set is transiently empty; a symbol-less
            // empty object is a genuine, fully-resolved `{}` type (e.g. an explicit
            // `(): {}` annotation), not a resolution artifact. Treating the latter
            // as "incomplete" would wrongly suppress a real member-type mismatch
            // (TS2416) — e.g. an override returning `{}` against a base member with
            // required members.
            let shape = checker.ctx.types.object_shape(shape_id);
            shape.properties.is_empty() && shape.symbol.is_some()
        }
        Some(tsz_solver::TypeData::Application(app_id)) => {
            // For Application types like B<T>, evaluate the application to check
            // if the resulting object has 0 properties.
            let evaluated = checker.evaluate_type_for_assignability(type_id);
            if evaluated == type_id {
                // Couldn't evaluate — check the base type
                let app = checker.ctx.types.type_application(app_id);
                is_incomplete_class_type(checker, app.base)
            } else {
                is_incomplete_class_type(checker, evaluated)
            }
        }
        Some(tsz_solver::TypeData::Lazy(def_id)) => {
            // A self-referential class Lazy(def_id) whose own instance type is
            // still mid-build (its symbol is in `class_instance_resolution_set`)
            // is incomplete *at this identity*, regardless of what it evaluates
            // to. Evaluating it first (as the branch below does) can resolve a
            // reentrant class-body Lazy to a degraded stand-in — e.g. the
            // class's own constructor/`Callable` shape — that carries no arm
            // here and is wrongly treated as a complete, unrelated type. Check
            // the in-flight set before evaluating away that identity.
            if checker
                .ctx
                .def_to_symbol_id(def_id)
                .is_some_and(|sym_id| checker.ctx.class_instance_resolution_set.contains(&sym_id))
            {
                return true;
            }
            // Lazy types that haven't been resolved yet — check the resolved form
            let evaluated = checker.evaluate_type_for_assignability(type_id);
            if evaluated != type_id {
                // A type-position `Lazy` of a `DefKind::Class` def denotes the
                // class's INSTANCE type, which is always an object — never
                // `unknown`/`any`/`error`. So if evaluation only degrades it to
                // one of those, the instance type is simply not available yet on
                // this entry path (the class self-reference resolved through the
                // unresolved-def taint rather than `symbol_instance_types`), not
                // a genuine resolution to a constraint-failing type. Treat it as
                // incomplete so the deferred self-reference does not fail a
                // constraint the real instance satisfies — mirroring, for the
                // direct-`CheckerState` entry path, the CLI driver's deferral of
                // an unbuilt class self-reference (#17743; #17629 family). A
                // resolved instance (the CLI's normal case) is a real object and
                // is unaffected.
                if evaluated.is_any_unknown_or_error()
                    && checker.ctx.definition_store.get_kind(def_id)
                        == Some(tsz_solver::def::DefKind::Class)
                {
                    return true;
                }
                is_incomplete_class_type(checker, evaluated)
            } else {
                // Can't evaluate — might be unresolvable during circular resolution
                // Treat as potentially incomplete
                true
            }
        }
        _ => false,
    }
}

// =============================================================================
// OwnMemberSummary — single-pass class member extraction
// =============================================================================

/// Summary of a single class's own members, extracted in one pass.
///
/// Contains ALL members (including private). Consumers filter by visibility
/// as needed. Only instance and static member vectors are populated; other
/// derived views (display names, kinds, parameter properties) were removed
/// as they had no callers.
#[derive(Clone, Default)]
pub(crate) struct OwnMemberSummary {
    /// All instance members (including private).
    pub(crate) all_instance_members: Vec<ClassMemberInfo>,
    /// All static members (including private).
    pub(crate) all_static_members: Vec<ClassMemberInfo>,
}

// =============================================================================
// Construction boundary function
// =============================================================================

/// Build the own-member summary for a class via single-pass extraction.
///
/// Extracts each member once (with `skip_private=false`) and records it
/// into the instance or static member vector.
pub(crate) fn build_own_member_summary(
    checker: &mut CheckerState<'_>,
    class_data: &tsz_parser::parser::node::ClassData,
) -> OwnMemberSummary {
    let mut summary = OwnMemberSummary::default();

    for &member_idx in &class_data.members.nodes {
        // Extract member info once (skip_private=false -> all members)
        if let Some(info) = checker.extract_class_member_info(member_idx, false) {
            if info.is_static {
                summary.all_static_members.push(info);
            } else {
                summary.all_instance_members.push(info);
            }
        }
    }

    summary
}

/// Check if a type is a valid base class type (for `extends` clause validation).
pub(crate) fn is_valid_base_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::data::is_valid_base_type(db, type_id)
}

/// Check if a type is a valid interface heritage base.
pub(crate) fn is_valid_interface_base_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::data::is_valid_interface_base_type(db, type_id)
}

/// True if `type_id` is callable (function or callable shape) and any of its
/// call signatures carries method-local type parameters. `None` when not
/// callable, so callers fall back to the strict relation.
pub(crate) fn callable_signature_is_generic(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<bool> {
    if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(db, type_id) {
        return Some(!shape.type_params.is_empty());
    }
    if let Some(shape) = crate::query_boundaries::common::callable_shape_for_type(db, type_id) {
        if shape.call_signatures.is_empty() {
            return None;
        }
        return Some(
            shape
                .call_signatures
                .iter()
                .any(|sig| !sig.type_params.is_empty()),
        );
    }
    None
}

/// Combine several object-wrapped single-signature method members (one per
/// overload of `name`) into one `Callable` carrying all of their call
/// signatures. Returns `None` unless at least two signatures were gathered, so
/// callers only use the combined form for genuinely overloaded members.
pub(crate) fn combine_overloaded_method_callable(
    db: &dyn QueryDatabase,
    member_object_types: &[TypeId],
    name: &str,
) -> Option<TypeId> {
    build_method_overload_callable(db, member_object_types.iter().copied(), name, 2)
}

/// Build a single `Callable` carrying the call signatures contributed by each
/// member-wrapper object type in `member_object_types`. Each entry is a
/// `{ name(...): ... }` wrapper (or already a function/callable), so the result
/// is the full overload set for the method `name`.
///
/// `min_signatures` is the floor below which `None` is returned:
///   - `2` builds a callable only for genuinely overloaded members
///     (`combine_overloaded_method_callable`).
///   - `1` keeps a single-signature member as a one-signature callable, which
///     the interface-heritage overload-coverage check needs whenever only one
///     side is overloaded: a derived interface may legitimately collapse a
///     base overload set, and a derived overload set must still be checked
///     against a single base signature.
pub(crate) fn build_method_overload_callable(
    db: &dyn QueryDatabase,
    member_object_types: impl IntoIterator<Item = TypeId>,
    name: &str,
    min_signatures: usize,
) -> Option<TypeId> {
    use tsz_solver::types::{CallSignature, CallableShape};
    let tdb = db.as_type_database();
    let mut call_signatures: Vec<CallSignature> = Vec::new();
    for object_ty in member_object_types {
        let fn_ty = crate::query_boundaries::common::find_property_by_str(tdb, object_ty, name)
            .map(|p| p.type_id)
            .unwrap_or(object_ty);
        if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(tdb, fn_ty) {
            call_signatures.push(CallSignature {
                type_params: shape.type_params.clone(),
                params: shape.params.clone(),
                this_type: shape.this_type,
                return_type: shape.return_type,
                type_predicate: shape.type_predicate,
                is_method: shape.is_method,
                declaration_group: 0,
            });
        } else {
            let shape = crate::query_boundaries::common::callable_shape_for_type(tdb, fn_ty)?;
            call_signatures.extend(shape.call_signatures.iter().cloned());
        }
    }
    if call_signatures.len() < min_signatures {
        return None;
    }
    Some(db.factory().callable(CallableShape {
        call_signatures,
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    }))
}

/// Instantiate a base interface member type by substituting the base's type
/// parameters with the heritage type arguments. A no-op when `base_params` is
/// empty or the member contains none of those parameters. The cross-file
/// interface heritage path relies on this because the lowered base member can
/// still reference the base's own parameter (e.g. `AliasedExpression<T, A>`).
pub(crate) fn instantiate_member_with_heritage_args(
    db: &dyn QueryDatabase,
    member_type: TypeId,
    base_params: &[tsz_solver::TypeParamInfo],
    heritage_args: &[TypeId],
) -> TypeId {
    if base_params.is_empty() {
        return member_type;
    }
    let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
        db.as_type_database(),
        base_params,
        heritage_args,
    );
    crate::query_boundaries::common::instantiate_type(db, member_type, &substitution)
}
