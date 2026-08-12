use crate::construction::QueryDatabase;
use crate::def::{DefKind, resolver::TypeResolver};
use crate::instantiation::instantiate::instantiate_generic_cached;
use crate::types::{ParamInfo, TypeParamInfo};
use crate::{TypeData, TypeId};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Rest-binder surface queries are deliberately broader than ordinary shallow
/// shape probes: declaration aliases can be nested deeply, while recursive
/// aliases can mint a fresh `Application` on every expansion. Keep a named
/// operation budget and report exhaustion instead of silently treating a deep
/// declared binder as concrete.
pub(crate) const MAX_REST_BINDER_QUERY_STEPS: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestBinderQuery<T> {
    Complete(T),
    Incomplete,
}

#[derive(Clone, Default)]
struct RestBinderWalk {
    seen_types: FxHashSet<TypeId>,
    /// One operation-wide budget shared by every cloned path state.
    ///
    /// `seen_types` remains path-local so a shared DAG node can be revisited
    /// from an independent branch, while the shared counter keeps that fan-out
    /// globally bounded.
    steps: Rc<Cell<usize>>,
    /// Positive binder classifications are independent of the current path.
    /// Sharing them across cloned branch states prevents identity-conditional
    /// DAGs from rewalking the same finite subtree exponentially.
    known_binders: Rc<RefCell<FxHashMap<TypeId, TypeParamInfo>>>,
}

impl RestBinderWalk {
    fn enter_type(&mut self, type_id: TypeId) -> RestBinderQuery<bool> {
        let steps = self.steps.get().saturating_add(1);
        self.steps.set(steps);
        if steps > MAX_REST_BINDER_QUERY_STEPS {
            return RestBinderQuery::Incomplete;
        }
        RestBinderQuery::Complete(!type_id.is_intrinsic() && self.seen_types.insert(type_id))
    }
}

struct AliasDefinition {
    args: Option<Vec<TypeId>>,
    type_params: Vec<TypeParamInfo>,
    body: TypeId,
}

#[derive(Clone, Copy)]
enum ConditionalIdentityPolicy {
    Direct,
    AliasBody,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestCallableSignature {
    pub params: Vec<ParamInfo>,
    pub is_method: bool,
}

#[derive(Clone, Copy)]
enum CallSignatureQueryPolicy {
    AnyCallable,
    DirectProvisional,
}

fn alias_definition<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    type_id: TypeId,
) -> Option<AliasDefinition> {
    let (def_id, args) = match db.lookup(type_id)? {
        TypeData::Lazy(def_id) => (def_id, None),
        TypeData::Application(app_id) => {
            let app = db.type_application(app_id);
            let def_id = crate::type_queries::application_base_def_id(
                db.as_type_database(),
                resolver,
                app.base,
            )?;
            (def_id, Some(app.args.clone()))
        }
        _ => return None,
    };
    let canonical_def_id = resolver.canonical_def_id(def_id);
    let alias_def_id = if resolver.get_def_kind(canonical_def_id) == Some(DefKind::TypeAlias) {
        canonical_def_id
    } else if resolver.get_def_kind(def_id) == Some(DefKind::TypeAlias) {
        def_id
    } else {
        return None;
    };
    let body = resolver
        .get_def_raw_body(alias_def_id, db.as_type_database())
        .or_else(|| resolver.resolve_lazy_lookup_only(alias_def_id, db.as_type_database()))?;
    let type_params = resolver
        .get_lazy_type_params(alias_def_id)
        .unwrap_or_default();
    Some(AliasDefinition {
        args,
        type_params,
        body,
    })
}

fn constraints_are_identity_equivalent<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    constraint: TypeId,
    extends_type: TypeId,
    policy: ConditionalIdentityPolicy,
) -> RestBinderQuery<bool> {
    if constraint == TypeId::ERROR || extends_type == TypeId::ERROR {
        return RestBinderQuery::Complete(false);
    }
    if constraint == extends_type {
        return RestBinderQuery::Complete(true);
    }
    if matches!(policy, ConditionalIdentityPolicy::AliasBody)
        && crate::relations::subtype::are_types_structurally_identical(
            db.as_type_database(),
            resolver,
            constraint,
            extends_type,
        )
    {
        return RestBinderQuery::Complete(true);
    }
    let evaluated_constraint = match evaluate_with_resolver_query(db, resolver, constraint) {
        RestBinderQuery::Complete(value) => value,
        RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
    };
    let evaluated_extends = match evaluate_with_resolver_query(db, resolver, extends_type) {
        RestBinderQuery::Complete(value) => value,
        RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
    };
    RestBinderQuery::Complete(
        evaluated_constraint != TypeId::ERROR
            && evaluated_extends != TypeId::ERROR
            && evaluated_constraint == evaluated_extends,
    )
}

fn evaluate_with_resolver_query<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    type_id: TypeId,
) -> RestBinderQuery<TypeId> {
    let result = crate::evaluation::evaluate::evaluate_type_result_with_resolver(
        db.as_type_database(),
        resolver,
        type_id,
    );
    if result.is_complete() {
        RestBinderQuery::Complete(result.type_id())
    } else {
        RestBinderQuery::Incomplete
    }
}

fn identity_conditional_binder<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    conditional_id: crate::types::ConditionalTypeId,
    state: &RestBinderWalk,
    policy: ConditionalIdentityPolicy,
) -> RestBinderQuery<Option<TypeParamInfo>> {
    let conditional = db.conditional_type(conditional_id);
    if conditional.false_type != TypeId::NEVER {
        return RestBinderQuery::Complete(None);
    }
    let mut check_state = state.clone();
    let check_info = match bare_rest_type_parameter_inner(
        db,
        resolver,
        conditional.check_type,
        &mut check_state,
    ) {
        RestBinderQuery::Complete(value) => value,
        RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
    };
    let mut true_state = state.clone();
    let true_info = match bare_rest_type_parameter_inner(
        db,
        resolver,
        conditional.true_type,
        &mut true_state,
    ) {
        RestBinderQuery::Complete(value) => value,
        RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
    };
    let (Some(check_info), Some(true_info)) = (check_info, true_info) else {
        return RestBinderQuery::Complete(None);
    };
    if !check_info.is_same_binder(true_info) {
        return RestBinderQuery::Complete(None);
    }
    let constraint = check_info.constraint.unwrap_or(TypeId::UNKNOWN);
    match constraints_are_identity_equivalent(
        db,
        resolver,
        constraint,
        conditional.extends_type,
        policy,
    ) {
        RestBinderQuery::Complete(equivalent) => {
            RestBinderQuery::Complete(equivalent.then_some(check_info))
        }
        RestBinderQuery::Incomplete => RestBinderQuery::Incomplete,
    }
}

fn transparent_alias_surface<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    type_id: TypeId,
) -> RestBinderQuery<Option<TypeId>> {
    let Some(alias) = alias_definition(db, resolver, type_id) else {
        return RestBinderQuery::Complete(None);
    };
    let Some(args) = alias.args else {
        return RestBinderQuery::Complete(Some(alias.body));
    };
    if let Some(TypeData::Conditional(conditional_id)) = db.lookup(alias.body) {
        let conditional = db.conditional_type(conditional_id);
        if conditional.false_type == TypeId::NEVER
            && let (
                Some(TypeData::TypeParameter(check_info)),
                Some(TypeData::TypeParameter(true_info)),
            ) = (
                db.lookup(conditional.check_type),
                db.lookup(conditional.true_type),
            )
            && !check_info.is_infer_placeholder()
            && check_info.is_same_binder(true_info)
            && let Some(index) = alias
                .type_params
                .iter()
                .position(|param| param.is_same_binder(check_info))
            && let Some(argument) = args.get(index)
        {
            let constraint = alias.type_params[index]
                .constraint
                .unwrap_or(TypeId::UNKNOWN);
            match constraints_are_identity_equivalent(
                db,
                resolver,
                constraint,
                conditional.extends_type,
                ConditionalIdentityPolicy::AliasBody,
            ) {
                RestBinderQuery::Complete(true) => {
                    return RestBinderQuery::Complete(Some(*argument));
                }
                RestBinderQuery::Complete(false) => {}
                RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
            }
        }
    }
    RestBinderQuery::Complete(Some(instantiate_generic_cached(
        db.as_type_database(),
        Some(db),
        alias.body,
        &alias.type_params,
        &args,
    )))
}

fn bare_rest_type_parameter_inner<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    type_id: TypeId,
    state: &mut RestBinderWalk,
) -> RestBinderQuery<Option<TypeParamInfo>> {
    if let Some(info) = state.known_binders.borrow().get(&type_id).copied() {
        return RestBinderQuery::Complete(Some(info));
    }
    match state.enter_type(type_id) {
        RestBinderQuery::Complete(true) => {}
        RestBinderQuery::Complete(false) => return RestBinderQuery::Complete(None),
        RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
    }
    let result = match db.lookup(type_id) {
        Some(TypeData::TypeParameter(info)) if !info.is_infer_placeholder() => {
            RestBinderQuery::Complete(Some(info))
        }
        Some(TypeData::NoInfer(inner)) => {
            bare_rest_type_parameter_inner(db, resolver, inner, state)
        }
        Some(TypeData::Substitution { base_type, .. }) => {
            bare_rest_type_parameter_inner(db, resolver, base_type, state)
        }
        Some(TypeData::Application(_) | TypeData::Lazy(_)) => {
            match transparent_alias_surface(db, resolver, type_id) {
                RestBinderQuery::Complete(Some(body)) => {
                    bare_rest_type_parameter_inner(db, resolver, body, state)
                }
                RestBinderQuery::Complete(None) => RestBinderQuery::Complete(None),
                RestBinderQuery::Incomplete => RestBinderQuery::Incomplete,
            }
        }
        Some(TypeData::Conditional(conditional_id)) => {
            match identity_conditional_binder(
                db,
                resolver,
                conditional_id,
                state,
                ConditionalIdentityPolicy::Direct,
            ) {
                RestBinderQuery::Complete(Some(info)) => RestBinderQuery::Complete(Some(info)),
                RestBinderQuery::Complete(None) => {
                    let evaluated = match evaluate_with_resolver_query(db, resolver, type_id) {
                        RestBinderQuery::Complete(value) => value,
                        RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
                    };
                    if evaluated == type_id || evaluated == TypeId::ERROR {
                        RestBinderQuery::Complete(None)
                    } else {
                        bare_rest_type_parameter_inner(db, resolver, evaluated, state)
                    }
                }
                RestBinderQuery::Incomplete => RestBinderQuery::Incomplete,
            }
        }
        _ => RestBinderQuery::Complete(None),
    };
    if let RestBinderQuery::Complete(Some(info)) = result {
        state.known_binders.borrow_mut().insert(type_id, info);
    }
    result
}

pub fn transparent_bare_rest_type_parameter_with_resolver_query<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    type_id: TypeId,
) -> RestBinderQuery<Option<TypeParamInfo>> {
    bare_rest_type_parameter_inner(db, resolver, type_id, &mut RestBinderWalk::default())
}

fn single_variadic_tuple_rest_type_parameter_inner<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    type_id: TypeId,
    state: &mut RestBinderWalk,
) -> RestBinderQuery<Option<TypeParamInfo>> {
    match state.enter_type(type_id) {
        RestBinderQuery::Complete(true) => {}
        RestBinderQuery::Complete(false) => return RestBinderQuery::Complete(None),
        RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
    }
    match db.lookup(type_id) {
        Some(TypeData::Tuple(elements_id)) => {
            let elements = db.tuple_list(elements_id);
            let [element] = &*elements else {
                return RestBinderQuery::Complete(None);
            };
            if !element.rest || element.optional {
                return RestBinderQuery::Complete(None);
            }
            bare_rest_type_parameter_inner(db, resolver, element.type_id, state)
        }
        Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
            single_variadic_tuple_rest_type_parameter_inner(db, resolver, inner, state)
        }
        Some(TypeData::Substitution { base_type, .. }) => {
            single_variadic_tuple_rest_type_parameter_inner(db, resolver, base_type, state)
        }
        Some(TypeData::Application(_) | TypeData::Lazy(_)) => {
            match transparent_alias_surface(db, resolver, type_id) {
                RestBinderQuery::Complete(Some(body)) => {
                    single_variadic_tuple_rest_type_parameter_inner(db, resolver, body, state)
                }
                RestBinderQuery::Complete(None) => RestBinderQuery::Complete(None),
                RestBinderQuery::Incomplete => RestBinderQuery::Incomplete,
            }
        }
        Some(TypeData::Conditional(_)) => {
            let evaluated = match evaluate_with_resolver_query(db, resolver, type_id) {
                RestBinderQuery::Complete(value) => value,
                RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
            };
            if evaluated == type_id || evaluated == TypeId::ERROR {
                RestBinderQuery::Complete(None)
            } else {
                single_variadic_tuple_rest_type_parameter_inner(db, resolver, evaluated, state)
            }
        }
        _ => RestBinderQuery::Complete(None),
    }
}

/// Return the binder of a transparent single variadic tuple surface
/// (`[...Pack]`), without treating an ordinary array (`Pack[]`) as equivalent.
pub fn single_variadic_tuple_rest_type_parameter_with_resolver_query<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    type_id: TypeId,
) -> RestBinderQuery<Option<TypeParamInfo>> {
    single_variadic_tuple_rest_type_parameter_inner(
        db,
        resolver,
        type_id,
        &mut RestBinderWalk::default(),
    )
}

fn push_structural_children(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    state: &RestBinderWalk,
    stack: &mut Vec<(TypeId, RestBinderWalk)>,
) {
    let mut push = |child| stack.push((child, state.clone()));
    match db.lookup(type_id) {
        Some(TypeData::Function(shape_id)) => {
            let shape = db.function_shape(shape_id);
            shape.params.iter().for_each(|param| push(param.type_id));
            if let Some(this_type) = shape.this_type {
                push(this_type);
            }
            push(shape.return_type);
        }
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            for signature in shape
                .call_signatures
                .iter()
                .chain(shape.construct_signatures.iter())
            {
                signature
                    .params
                    .iter()
                    .for_each(|param| push(param.type_id));
                if let Some(this_type) = signature.this_type {
                    push(this_type);
                }
                push(signature.return_type);
            }
            for property in &shape.properties {
                push(property.type_id);
                push(property.write_type);
            }
            for index in [shape.string_index, shape.number_index]
                .into_iter()
                .flatten()
            {
                push(index.value_type);
            }
        }
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            for property in &shape.properties {
                push(property.type_id);
                push(property.write_type);
            }
            for index in [shape.string_index, shape.number_index, shape.symbol_index]
                .into_iter()
                .flatten()
            {
                push(index.value_type);
            }
        }
        Some(TypeData::Tuple(list_id)) => db
            .tuple_list(list_id)
            .iter()
            .for_each(|element| push(element.type_id)),
        Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => db
            .type_list(list_id)
            .iter()
            .for_each(|&member| push(member)),
        Some(TypeData::Array(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
            push(inner);
        }
        _ => {}
    }
}

pub fn contains_declared_bare_function_rest_with_resolver_query<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    type_id: TypeId,
) -> RestBinderQuery<bool> {
    let mut stack = vec![(type_id, RestBinderWalk::default())];
    while let Some((current, mut state)) = stack.pop() {
        match state.enter_type(current) {
            RestBinderQuery::Complete(true) => {}
            RestBinderQuery::Complete(false) => continue,
            RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
        }
        match db.lookup(current) {
            Some(TypeData::Function(shape_id)) => {
                let shape = db.function_shape(shape_id);
                for param in &shape.params {
                    if param.rest {
                        let mut rest_state = state.clone();
                        match bare_rest_type_parameter_inner(
                            db,
                            resolver,
                            param.type_id,
                            &mut rest_state,
                        ) {
                            RestBinderQuery::Complete(Some(_)) => {
                                return RestBinderQuery::Complete(true);
                            }
                            RestBinderQuery::Complete(None) => {}
                            RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
                        }
                    }
                }
                push_structural_children(db, current, &state, &mut stack);
            }
            Some(TypeData::Callable(shape_id)) => {
                let shape = db.callable_shape(shape_id);
                for signature in shape
                    .call_signatures
                    .iter()
                    .chain(shape.construct_signatures.iter())
                {
                    for param in &signature.params {
                        if param.rest {
                            let mut rest_state = state.clone();
                            match bare_rest_type_parameter_inner(
                                db,
                                resolver,
                                param.type_id,
                                &mut rest_state,
                            ) {
                                RestBinderQuery::Complete(Some(_)) => {
                                    return RestBinderQuery::Complete(true);
                                }
                                RestBinderQuery::Complete(None) => {}
                                RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
                            }
                        }
                    }
                }
                push_structural_children(db, current, &state, &mut stack);
            }
            Some(TypeData::Application(_) | TypeData::Lazy(_)) => {
                match transparent_alias_surface(db, resolver, current) {
                    RestBinderQuery::Complete(Some(body)) => stack.push((body, state)),
                    RestBinderQuery::Complete(None) => {}
                    RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
                }
            }
            Some(TypeData::Conditional(_) | TypeData::IndexAccess(_, _)) => {
                let evaluated = match evaluate_with_resolver_query(db, resolver, current) {
                    RestBinderQuery::Complete(value) => value,
                    RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
                };
                if evaluated != current && evaluated != TypeId::ERROR {
                    stack.push((evaluated, state));
                }
            }
            _ => push_structural_children(db, current, &state, &mut stack),
        }
    }
    RestBinderQuery::Complete(false)
}

/// Resolver-aware structural visitor for declared callable rest binders.
///
/// Query exhaustion is conservatively treated as raw-sensitive: callers must
/// not project an unclassified declaration surface through a constraint.
pub fn contains_declared_bare_function_rest_with_resolver<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    type_id: TypeId,
) -> bool {
    match contains_declared_bare_function_rest_with_resolver_query(db, resolver, type_id) {
        RestBinderQuery::Complete(value) => value,
        RestBinderQuery::Incomplete => true,
    }
}

pub fn contains_declared_bare_function_rest(db: &dyn QueryDatabase, type_id: TypeId) -> bool {
    contains_declared_bare_function_rest_with_resolver(db, &db, type_id)
}

pub fn call_signatures_with_resolver<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    type_id: TypeId,
) -> RestBinderQuery<Option<Vec<RestCallableSignature>>> {
    call_signatures_with_resolver_inner(
        db,
        resolver,
        type_id,
        &mut RestBinderWalk::default(),
        CallSignatureQueryPolicy::AnyCallable,
    )
}

/// Call signatures on a direct aggregate-callback surface.
///
/// The provisional generic-call relation is currently one boolean scoped to
/// the first function comparison. It is therefore sound only for a call-only
/// value (possibly below aliases, `NoInfer`, or one nullish shell). Callable
/// properties, constructs, and intersections need a branch-local relation
/// plan; reject them here instead of letting the escape reach sibling paths.
pub fn direct_provisional_call_signatures_with_resolver<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    type_id: TypeId,
) -> RestBinderQuery<Option<Vec<RestCallableSignature>>> {
    call_signatures_with_resolver_inner(
        db,
        resolver,
        type_id,
        &mut RestBinderWalk::default(),
        CallSignatureQueryPolicy::DirectProvisional,
    )
}

fn call_signatures_with_resolver_inner<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    type_id: TypeId,
    state: &mut RestBinderWalk,
    policy: CallSignatureQueryPolicy,
) -> RestBinderQuery<Option<Vec<RestCallableSignature>>> {
    match state.enter_type(type_id) {
        RestBinderQuery::Complete(true) => {}
        RestBinderQuery::Complete(false) => return RestBinderQuery::Complete(None),
        RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
    }
    match db.lookup(type_id) {
        Some(TypeData::Function(shape_id)) => {
            let shape = db.function_shape(shape_id);
            if matches!(policy, CallSignatureQueryPolicy::DirectProvisional) && shape.is_constructor
            {
                return RestBinderQuery::Complete(None);
            }
            RestBinderQuery::Complete(Some(vec![RestCallableSignature {
                params: shape.params.clone(),
                is_method: shape.is_method,
            }]))
        }
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            if matches!(policy, CallSignatureQueryPolicy::DirectProvisional)
                && (!shape.construct_signatures.is_empty()
                    || !shape.properties.is_empty()
                    || shape.string_index.is_some()
                    || shape.number_index.is_some())
            {
                return RestBinderQuery::Complete(None);
            }
            RestBinderQuery::Complete((!shape.call_signatures.is_empty()).then(|| {
                shape
                    .call_signatures
                    .iter()
                    .map(|signature| RestCallableSignature {
                        params: signature.params.clone(),
                        is_method: signature.is_method,
                    })
                    .collect()
            }))
        }
        Some(TypeData::NoInfer(inner)) => {
            call_signatures_with_resolver_inner(db, resolver, inner, state, policy)
        }
        Some(TypeData::Application(_) | TypeData::Lazy(_)) => {
            let next = match transparent_alias_surface(db, resolver, type_id) {
                RestBinderQuery::Complete(Some(body)) => body,
                RestBinderQuery::Complete(None) => {
                    // Generic callable interfaces are `Application`s whose
                    // definitions are not aliases. Resolve their typed
                    // application surface before deciding that the value is
                    // non-callable; callers still need an explicit
                    // `Incomplete` result when evaluation exhausts fuel.
                    match evaluate_with_resolver_query(db, resolver, type_id) {
                        RestBinderQuery::Complete(value) => value,
                        RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
                    }
                }
                RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
            };
            if next == type_id || next == TypeId::ERROR {
                RestBinderQuery::Complete(None)
            } else {
                call_signatures_with_resolver_inner(db, resolver, next, state, policy)
            }
        }
        Some(TypeData::Conditional(_)) => {
            let evaluated = match evaluate_with_resolver_query(db, resolver, type_id) {
                RestBinderQuery::Complete(value) => value,
                RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
            };
            if evaluated == type_id || evaluated == TypeId::ERROR {
                RestBinderQuery::Complete(None)
            } else {
                call_signatures_with_resolver_inner(db, resolver, evaluated, state, policy)
            }
        }
        Some(TypeData::Union(list_id)) => {
            // Only a nullish wrapper is transparent here. A genuine union of
            // value surfaces cannot share one branch-local provenance decision.
            let members = db.type_list(list_id);
            let mut non_nullish = members
                .iter()
                .copied()
                .filter(|member| !member.is_nullish() && *member != TypeId::NEVER);
            let Some(member) = non_nullish.next() else {
                return RestBinderQuery::Complete(None);
            };
            if non_nullish.next().is_some() {
                return RestBinderQuery::Complete(None);
            }
            call_signatures_with_resolver_inner(db, resolver, member, state, policy)
        }
        Some(TypeData::Intersection(list_id)) => {
            if matches!(policy, CallSignatureQueryPolicy::DirectProvisional) {
                return RestBinderQuery::Complete(None);
            }
            // A callable intersected with a non-callable decoration keeps one
            // direct signature surface. Multiple callable constituents are
            // rejected because flattening them would lose signature-path scope.
            let members = db.type_list(list_id);
            let mut found = None;
            for &member in members.iter() {
                let mut branch_state = state.clone();
                match call_signatures_with_resolver_inner(
                    db,
                    resolver,
                    member,
                    &mut branch_state,
                    policy,
                ) {
                    RestBinderQuery::Complete(Some(signatures)) => {
                        if found.is_some() {
                            return RestBinderQuery::Complete(None);
                        }
                        found = Some(signatures);
                    }
                    RestBinderQuery::Complete(None) => {}
                    RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
                }
            }
            RestBinderQuery::Complete(found)
        }
        _ => RestBinderQuery::Complete(None),
    }
}

pub fn rest_type_has_union_surface_with_resolver_query<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    type_id: TypeId,
) -> RestBinderQuery<bool> {
    let mut current = type_id;
    let mut state = RestBinderWalk::default();
    loop {
        match state.enter_type(current) {
            RestBinderQuery::Complete(true) => {}
            RestBinderQuery::Complete(false) => return RestBinderQuery::Complete(false),
            RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
        }
        match db.lookup(current) {
            Some(TypeData::Union(_)) => return RestBinderQuery::Complete(true),
            Some(TypeData::NoInfer(inner)) => current = inner,
            Some(TypeData::Substitution { constraint, .. }) => current = constraint,
            Some(TypeData::Application(_) | TypeData::Lazy(_)) => {
                match transparent_alias_surface(db, resolver, current) {
                    RestBinderQuery::Complete(Some(body)) => current = body,
                    RestBinderQuery::Complete(None) => {
                        return RestBinderQuery::Complete(false);
                    }
                    RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
                }
            }
            Some(TypeData::Conditional(_)) => {
                let evaluated = match evaluate_with_resolver_query(db, resolver, current) {
                    RestBinderQuery::Complete(value) => value,
                    RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
                };
                if evaluated == current || evaluated == TypeId::ERROR {
                    return RestBinderQuery::Complete(false);
                }
                current = evaluated;
            }
            _ => return RestBinderQuery::Complete(false),
        }
    }
}

fn bare_rest_index_with_resolver<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    params: &[ParamInfo],
) -> RestBinderQuery<Option<usize>> {
    let Some(index) = params.iter().position(|param| param.rest) else {
        return RestBinderQuery::Complete(None);
    };
    match transparent_bare_rest_type_parameter_with_resolver_query(
        db,
        resolver,
        params[index].type_id,
    ) {
        RestBinderQuery::Complete(Some(_)) => RestBinderQuery::Complete(Some(index)),
        RestBinderQuery::Complete(None) => RestBinderQuery::Complete(None),
        RestBinderQuery::Incomplete => RestBinderQuery::Incomplete,
    }
}

fn has_union_rest_with_resolver<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    params: &[ParamInfo],
) -> RestBinderQuery<bool> {
    let Some(param) = params.last().filter(|param| param.rest) else {
        return RestBinderQuery::Complete(false);
    };
    rest_type_has_union_surface_with_resolver_query(db, resolver, param.type_id)
}

fn fixed_or_union_rest_mismatch<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    source_params: &[ParamInfo],
    target_params: &[ParamInfo],
) -> RestBinderQuery<bool> {
    let source_rest_index = match bare_rest_index_with_resolver(db, resolver, source_params) {
        RestBinderQuery::Complete(Some(index)) => index,
        RestBinderQuery::Complete(None) => return RestBinderQuery::Complete(false),
        RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
    };
    let target_fixed_count = target_params.iter().take_while(|param| !param.rest).count();
    if target_fixed_count > source_rest_index {
        return RestBinderQuery::Complete(true);
    }
    has_union_rest_with_resolver(db, resolver, target_params)
}

/// Whether a direct callable relation is the generic-call aggregate escape: a
/// bare source variadic against a target variadic with a union surface.
pub fn bare_source_rest_targets_union_with_resolver_query<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    source: TypeId,
    target: TypeId,
) -> RestBinderQuery<bool> {
    let source_lists = match call_signatures_with_resolver(db, resolver, source) {
        RestBinderQuery::Complete(Some(value)) => value,
        RestBinderQuery::Complete(None) => return RestBinderQuery::Complete(false),
        RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
    };
    let target_lists = match call_signatures_with_resolver(db, resolver, target) {
        RestBinderQuery::Complete(Some(value)) => value,
        RestBinderQuery::Complete(None) => return RestBinderQuery::Complete(false),
        RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
    };
    let mut source_has_bare_rest = false;
    for source in &source_lists {
        match bare_rest_index_with_resolver(db, resolver, &source.params) {
            RestBinderQuery::Complete(Some(_)) => {
                source_has_bare_rest = true;
                break;
            }
            RestBinderQuery::Complete(None) => {}
            RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
        }
    }
    if !source_has_bare_rest {
        return RestBinderQuery::Complete(false);
    }
    for target in &target_lists {
        match has_union_rest_with_resolver(db, resolver, &target.params) {
            RestBinderQuery::Complete(true) => return RestBinderQuery::Complete(true),
            RestBinderQuery::Complete(false) => {}
            RestBinderQuery::Incomplete => return RestBinderQuery::Incomplete,
        }
    }
    RestBinderQuery::Complete(false)
}

/// Whether a failed callable relation involving a bare source variadic must
/// remain visible to checker diagnostics.
///
/// The relation has already failed when this query is consumed, so any
/// source/target signature pair carrying the rigid mismatch is sufficient.
pub fn bare_source_rest_requires_visible_relation_failure<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    source: TypeId,
    target: TypeId,
) -> bool {
    let source_lists = match call_signatures_with_resolver(db, resolver, source) {
        RestBinderQuery::Complete(Some(value)) => value,
        RestBinderQuery::Complete(None) => return false,
        RestBinderQuery::Incomplete => return true,
    };
    let target_lists = match call_signatures_with_resolver(db, resolver, target) {
        RestBinderQuery::Complete(Some(value)) => value,
        RestBinderQuery::Complete(None) => return false,
        RestBinderQuery::Incomplete => return true,
    };
    for source_signature in &source_lists {
        for target_signature in &target_lists {
            match fixed_or_union_rest_mismatch(
                db,
                resolver,
                &source_signature.params,
                &target_signature.params,
            ) {
                RestBinderQuery::Complete(true) | RestBinderQuery::Incomplete => return true,
                RestBinderQuery::Complete(false) => {}
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::TypeDatabase;
    use crate::construction::TypeInterner;
    use crate::def::DefId;
    use crate::relations::subtype::NoopResolver;
    use crate::types::{
        ConditionalType, FunctionShape, PropertyInfo, SymbolRef, TupleElement, TypeParamOrigin,
    };

    struct IdentityAliasResolver {
        def_id: DefId,
        body: TypeId,
        type_param: TypeParamInfo,
    }

    impl TypeResolver for IdentityAliasResolver {
        fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
            None
        }

        fn resolve_lazy(&self, def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
            (def_id == self.def_id).then_some(self.body)
        }

        fn get_lazy_type_params(&self, def_id: DefId) -> Option<Vec<TypeParamInfo>> {
            (def_id == self.def_id).then(|| vec![self.type_param])
        }

        fn get_def_kind(&self, def_id: DefId) -> Option<DefKind> {
            (def_id == self.def_id).then_some(DefKind::TypeAlias)
        }

        fn get_def_raw_body(&self, def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
            (def_id == self.def_id).then_some(self.body)
        }
    }

    fn declared_pack(interner: &TypeInterner, node: u32) -> (TypeParamInfo, TypeId) {
        let info = TypeParamInfo {
            name: interner.intern_string("Pack"),
            constraint: Some(interner.array(TypeId::UNKNOWN)),
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped {
                file: interner.intern_string("deep-rest-query.ts"),
                node,
            },
        };
        (info, interner.fresh_type_param(info))
    }

    #[test]
    fn deep_no_infer_chain_has_no_sixteen_wrapper_cliff() {
        let interner = TypeInterner::new();
        let (info, binder) = declared_pack(&interner, 1);
        let mut wrapped = binder;
        for _ in 0..64 {
            wrapped = interner.no_infer(wrapped);
        }

        assert!(matches!(
            transparent_bare_rest_type_parameter_with_resolver_query(
                &interner,
                &NoopResolver,
                wrapped,
            ),
            RestBinderQuery::Complete(Some(found)) if found.is_same_binder(info)
        ));
    }

    #[test]
    fn repeated_identity_alias_chain_has_no_256_reentrance_cliff() {
        let interner = TypeInterner::new();
        let (pack_info, pack) = declared_pack(&interner, 4);
        let alias_param = TypeParamInfo {
            name: interner.intern_string("AliasPack"),
            constraint: Some(interner.array(TypeId::UNKNOWN)),
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped {
                file: interner.intern_string("deep-rest-query.ts"),
                node: 5,
            },
        };
        let alias_body = interner.fresh_type_param(alias_param);
        let def_id = DefId(9_001);
        let alias = interner.lazy(def_id);
        let resolver = IdentityAliasResolver {
            def_id,
            body: alias_body,
            type_param: alias_param,
        };
        let mut nested = pack;
        for _ in 0..300 {
            nested = interner.application(alias, vec![nested]);
        }

        assert!(matches!(
            transparent_bare_rest_type_parameter_with_resolver_query(
                &interner,
                &resolver,
                nested,
            ),
            RestBinderQuery::Complete(Some(found)) if found.is_same_binder(pack_info)
        ));
    }

    #[test]
    fn conditional_identity_requires_the_declared_constraint_surface() {
        let interner = TypeInterner::new();
        let (info, binder) = declared_pack(&interner, 2);
        let constraint = info.constraint.expect("declared pack has a constraint");
        let identity = interner.conditional(ConditionalType {
            check_type: binder,
            extends_type: constraint,
            true_type: binder,
            false_type: TypeId::NEVER,
            is_distributive: true,
        });
        assert!(matches!(
            transparent_bare_rest_type_parameter_with_resolver_query(
                &interner,
                &NoopResolver,
                identity,
            ),
            RestBinderQuery::Complete(Some(found)) if found.is_same_binder(info)
        ));

        let non_identity = interner.conditional(ConditionalType {
            check_type: binder,
            extends_type: interner.tuple(vec![]),
            true_type: binder,
            false_type: TypeId::NEVER,
            is_distributive: true,
        });
        assert!(matches!(
            transparent_bare_rest_type_parameter_with_resolver_query(
                &interner,
                &NoopResolver,
                non_identity,
            ),
            RestBinderQuery::Complete(None)
        ));

        let different_false_branch = interner.conditional(ConditionalType {
            check_type: binder,
            extends_type: constraint,
            true_type: binder,
            false_type: TypeId::STRING,
            is_distributive: true,
        });
        assert!(matches!(
            transparent_bare_rest_type_parameter_with_resolver_query(
                &interner,
                &NoopResolver,
                different_false_branch,
            ),
            RestBinderQuery::Complete(None)
        ));
    }

    #[test]
    fn single_variadic_tuple_query_distinguishes_spread_from_array_and_fixed_tuple() {
        let interner = TypeInterner::new();
        let (info, binder) = declared_pack(&interner, 7);
        let spread_tuple = interner.tuple(vec![TupleElement {
            type_id: binder,
            name: None,
            optional: false,
            rest: true,
        }]);
        let fixed_tuple = interner.tuple(vec![TupleElement {
            type_id: binder,
            name: None,
            optional: false,
            rest: false,
        }]);

        assert!(matches!(
            single_variadic_tuple_rest_type_parameter_with_resolver_query(
                &interner,
                &NoopResolver,
                spread_tuple,
            ),
            RestBinderQuery::Complete(Some(found)) if found.is_same_binder(info)
        ));
        for non_spread in [interner.array(binder), fixed_tuple] {
            assert_eq!(
                single_variadic_tuple_rest_type_parameter_with_resolver_query(
                    &interner,
                    &NoopResolver,
                    non_spread,
                ),
                RestBinderQuery::Complete(None),
            );
        }
    }

    #[test]
    fn shared_identity_conditional_dag_reuses_completed_binder_results() {
        let interner = TypeInterner::new();
        let (info, binder) = declared_pack(&interner, 6);
        let constraint = info.constraint.expect("declared pack has a constraint");
        let mut identity = binder;
        for _ in 0..32 {
            identity = interner.conditional(ConditionalType {
                check_type: identity,
                extends_type: constraint,
                true_type: identity,
                false_type: TypeId::NEVER,
                is_distributive: true,
            });
        }

        assert!(matches!(
            transparent_bare_rest_type_parameter_with_resolver_query(
                &interner,
                &NoopResolver,
                identity,
            ),
            RestBinderQuery::Complete(Some(found)) if found.is_same_binder(info)
        ));
    }

    #[test]
    fn declared_rest_visitor_crosses_deep_structural_wrappers() {
        let interner = TypeInterner::new();
        let (_, binder) = declared_pack(&interner, 3);
        let mut nested = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                suppress_display_optional: false,
                name: None,
                type_id: binder,
                optional: false,
                rest: true,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        for level in 0..64 {
            nested = interner.object(vec![PropertyInfo::new(
                interner.intern_string(&format!("level{level}")),
                nested,
            )]);
        }

        assert_eq!(
            contains_declared_bare_function_rest_with_resolver_query(
                &interner,
                &NoopResolver,
                nested,
            ),
            RestBinderQuery::Complete(true)
        );
    }

    #[test]
    fn structural_fanout_uses_one_operation_wide_budget() {
        let interner = TypeInterner::new();
        let properties = (0..MAX_REST_BINDER_QUERY_STEPS)
            .map(|index| {
                PropertyInfo::new(
                    interner.intern_string(&format!("branch{index}")),
                    TypeId::STRING,
                )
            })
            .collect();
        let wide = interner.object(properties);

        assert_eq!(
            contains_declared_bare_function_rest_with_resolver_query(
                &interner,
                &NoopResolver,
                wide,
            ),
            RestBinderQuery::Incomplete,
            "cloned branch states must share one global traversal budget"
        );
    }
}
