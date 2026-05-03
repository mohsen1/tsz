use crate::class_checker::ClassMemberInfo;
use crate::state::CheckerState;
use tsz_parser::NodeIndex;
use tsz_solver::{TypeDatabase, TypeId};

fn collect_signature_return_types(db: &dyn TypeDatabase, type_id: TypeId) -> Vec<TypeId> {
    if let Some(signatures) = crate::query_boundaries::common::call_signatures_for_type(db, type_id)
    {
        return signatures
            .into_iter()
            .map(|signature| signature.return_type)
            .collect();
    }
    if let Some(shape_id) = tsz_solver::function_shape_id(db, type_id) {
        return vec![db.function_shape(shape_id).return_type];
    }
    if let Some(shape_id) = tsz_solver::callable_shape_id(db, type_id) {
        return db
            .callable_shape(shape_id)
            .call_signatures
            .iter()
            .map(|signature| signature.return_type)
            .collect();
    }
    if let Some(shape) = crate::query_boundaries::common::object_shape_for_type(db, type_id)
        && shape.properties.len() == 1
    {
        let prop = &shape.properties[0];
        if prop.is_method {
            return collect_signature_return_types(db, prop.type_id);
        }
    }
    Vec::new()
}

fn has_polymorphic_this_return_mismatch(
    checker: &CheckerState<'_>,
    source: TypeId,
    target: TypeId,
) -> bool {
    let source_returns = collect_signature_return_types(checker.ctx.types, source);
    let target_returns = collect_signature_return_types(checker.ctx.types, target);
    if source_returns.is_empty() || target_returns.is_empty() {
        return false;
    }
    // Only apply this fast mismatch rule for single-signature comparisons.
    // Overload sets need full compatibility checks (including trailing overload
    // matching) and this shortcut would produce false TS2430s.
    if source_returns.len() != 1 || target_returns.len() != 1 {
        return false;
    }

    let source_has_polymorphic_this = source_returns
        .iter()
        .any(|&ret| tsz_solver::is_this_type(checker.ctx.types, ret));
    let target_has_polymorphic_this = target_returns
        .iter()
        .any(|&ret| tsz_solver::is_this_type(checker.ctx.types, ret));

    target_has_polymorphic_this && !source_has_polymorphic_this
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

fn callable_mentions_nonlocal_type_params(checker: &CheckerState<'_>, type_id: TypeId) -> bool {
    let signature_mentions_nonlocal = |type_params: &[tsz_solver::types::TypeParamInfo],
                                       params: &[tsz_solver::types::ParamInfo],
                                       this_type: Option<TypeId>,
                                       return_type: TypeId| {
        let local_tp_ids: Vec<TypeId> = type_params
            .iter()
            .map(|tp| checker.ctx.types.type_param(*tp))
            .collect();
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

fn source_this_parameter_is_acceptable_for_target_without_this(
    checker: &mut CheckerState<'_>,
    source: TypeId,
    target: TypeId,
) -> bool {
    fn generic_head(display: &str) -> Option<&str> {
        display.split_once('<').map(|(head, _)| head.trim())
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
            && generic_head(&checker.format_type(source_shape.return_type))
                == generic_head(&checker.format_type(target_shape.return_type))
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
        return checker.is_assignable_to_no_erase_generics(stripped_source, target)
            || checker.is_assignable_to_no_erase_generics(target, stripped_source)
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
    checker.is_assignable_to_no_erase_generics(stripped_source, target)
        || checker.is_assignable_to_no_erase_generics(target, stripped_source)
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
    if has_polymorphic_this_return_mismatch(checker, source, target) {
        return true;
    }
    if checker.should_suppress_assignability_diagnostic(source, target) {
        return false;
    }
    if checker.should_suppress_assignability_for_parse_recovery(node_idx, node_idx) {
        return false;
    }
    if checker.is_assignable_to_no_erase_generics(source, target) {
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
    if has_polymorphic_this_return_mismatch(checker, source, target) {
        return true;
    }
    if checker.should_suppress_member_assignability(source, target) {
        return false;
    }
    if checker.should_suppress_assignability_for_parse_recovery(node_idx, node_idx) {
        return false;
    }
    if checker.is_assignable_to_no_erase_generics(source, target) {
        return false;
    }
    // Fallback: when the strict no-erase-generics relation rejects but the
    // standard assignable-to relation accepts, the rejection is generic-arity
    // bookkeeping rather than a real mismatch (e.g. `IteratorResult<T, any>`
    // source vs `IteratorResult<T, void>` target — `any` is a universal sink
    // for the standard relation but the strict path keeps the args nominally
    // distinct). tsc does not emit TS2416 here.
    if checker.is_assignable_to(source, target) {
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
    // Suppress TS2416 when the only incompatibility is a type predicate that tsc
    // would infer.  tsc infers type predicates for methods without explicit return
    // type annotations whose bodies consist of narrowing expressions (e.g.
    // `typeof x === 'number'`).  We don't implement type predicate inference, so
    // a class method returning `boolean` appears incompatible with an interface
    // method returning `x is T`.  Suppress the diagnostic when:
    //   1. The target (interface) function has a type predicate.
    //   2. The source (class method) has no type predicate and returns `boolean`.
    //   3. The class member AST node has no explicit return type annotation.
    //   4. The parameter types are compatible (the only difference is the predicate).
    if is_type_predicate_inference_suppressed(checker, source, target, node_idx) {
        return false;
    }
    true
}

/// Check if a TS2416 incompatibility is caused solely by a missing type predicate
/// that tsc would infer from the method body.
///
/// tsc infers type predicates for methods without explicit return type annotations
/// (TS 5.5+ inferred type predicates). That inference only ever produces a
/// parameter predicate (`x is T`) — `this is T` predicates are never inferred
/// from a method body. So the suppression below is restricted to parameter
/// predicates: a `this is T` mismatch must always be reported as TS2416 because
/// tsc would also report it.
///
/// Since we don't implement type predicate inference, suppress the diagnostic
/// when the only difference between the source and target signatures is a
/// parameter type predicate that tsc would have inferred.
fn is_type_predicate_inference_suppressed(
    checker: &CheckerState<'_>,
    source: TypeId,
    target: TypeId,
    node_idx: NodeIndex,
) -> bool {
    // Get target function shape and check for type predicate
    let Some(target_predicate) = get_type_predicate_from_signature(checker, target) else {
        return false;
    };

    // tsc never infers `this is T` predicates from method bodies, so a `this`
    // predicate target must always emit TS2416.
    if matches!(
        target_predicate.target,
        tsz_solver::types::TypePredicateTarget::This
    ) {
        return false;
    }

    // Source must NOT have a type predicate (it returns plain boolean)
    let source_has_predicate = get_type_predicate_from_signature(checker, source).is_some();
    if source_has_predicate {
        return false;
    }

    // Source must return boolean
    let source_returns_boolean = tsz_solver::function_shape_id(checker.ctx.types, source)
        .map(|id| checker.ctx.types.function_shape(id).return_type)
        .or_else(|| {
            tsz_solver::callable_shape_id(checker.ctx.types, source).and_then(|id| {
                checker
                    .ctx
                    .types
                    .callable_shape(id)
                    .call_signatures
                    .first()
                    .map(|s| s.return_type)
            })
        })
        .is_some_and(|ret| ret == TypeId::BOOLEAN);
    if !source_returns_boolean {
        return false;
    }

    // The class member must not have an explicit return type annotation.
    // If the developer wrote `: boolean` explicitly, tsc won't infer a predicate.
    let node = checker.ctx.arena.get(node_idx);
    let has_explicit_return_type = node
        .and_then(|n| {
            checker
                .ctx
                .arena
                .get_method_decl(n)
                .map(|m| m.type_annotation.is_some())
                .or_else(|| {
                    checker
                        .ctx
                        .arena
                        .get_function(n)
                        .map(|f| f.type_annotation.is_some())
                })
        })
        .unwrap_or(true); // If we can't determine, assume explicit (don't suppress)
    if has_explicit_return_type {
        return false;
    }

    true
}

/// Extract a type predicate from a function or callable type's signature.
fn get_type_predicate_from_signature(
    checker: &CheckerState<'_>,
    type_id: TypeId,
) -> Option<tsz_solver::types::TypePredicate> {
    tsz_solver::function_shape_id(checker.ctx.types, type_id)
        .and_then(|id| checker.ctx.types.function_shape(id).type_predicate)
        .or_else(|| {
            tsz_solver::callable_shape_id(checker.ctx.types, type_id).and_then(|id| {
                checker
                    .ctx
                    .types
                    .callable_shape(id)
                    .call_signatures
                    .first()
                    .and_then(|s| s.type_predicate)
            })
        })
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
            if sp.type_id != tp.type_id && !checker.is_assignable_to(tp.type_id, sp.type_id) {
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
    if has_polymorphic_this_return_mismatch(checker, narrowed_source, target) {
        return true;
    }
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
        if needs_strict_generic_target_callable_recheck(checker, relation_source, relation_target) {
            let strict_source = unwrap_single_property_value_type(checker, relation_source);
            let strict_target = unwrap_single_property_value_type(checker, relation_target);
            return !checker.is_assignable_to_no_erase_generics(strict_source, strict_target);
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
fn is_incomplete_class_type(checker: &mut CheckerState<'_>, type_id: TypeId) -> bool {
    match checker.ctx.types.lookup(type_id) {
        Some(tsz_solver::TypeData::Object(shape_id))
        | Some(tsz_solver::TypeData::ObjectWithIndex(shape_id)) => checker
            .ctx
            .types
            .object_shape(shape_id)
            .properties
            .is_empty(),
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
        Some(tsz_solver::TypeData::Lazy(_)) => {
            // Lazy types that haven't been resolved yet — check the resolved form
            let evaluated = checker.evaluate_type_for_assignability(type_id);
            if evaluated != type_id {
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
