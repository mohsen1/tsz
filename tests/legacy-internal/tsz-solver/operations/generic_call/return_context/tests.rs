use super::sort_type_params_by_name;
use crate::TypeInterner;
use crate::caches::query_cache::QueryCache;
use crate::def::{DefId, DefinitionStore};
use crate::instantiation::instantiate::flags::InstResolverRereduceFlagGuard;
use crate::operations::{AssignabilityChecker, CallEvaluator, CallResult, GenericCallRequest};
use crate::types::{
    FunctionShape, ParamInfo, PropertyInfo, TypeId, TypeParamInfo, TypePredicate,
    TypePredicateTarget,
};
use tsz_common::interner::Atom;

const fn tp(name: u32) -> TypeParamInfo {
    TypeParamInfo {
        name: Atom(name),
        constraint: Some(TypeId::UNKNOWN),
        default: Some(TypeId::ERROR),
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    }
}

fn object_with_value(interner: &TypeInterner, value_name: Atom, value_type: TypeId) -> TypeId {
    interner.object(vec![PropertyInfo::new(value_name, value_type)])
}

struct StoreBackedReturnChecker<'eval, 'cache> {
    db: &'eval QueryCache<'cache>,
}

impl AssignabilityChecker for StoreBackedReturnChecker<'_, '_> {
    fn is_assignable_to(&mut self, _source: TypeId, _target: TypeId) -> bool {
        true
    }

    fn evaluate_type_for_return_context_substitution(&mut self, type_id: TypeId) -> TypeId {
        self.db
            .store_backed_rereduce_evaluator()
            .map_or(type_id, |mut evaluator| evaluator.evaluate(type_id))
    }
}

fn return_context_substitution_for_lazy_pair(
    interner: &TypeInterner,
    db: &QueryCache<'_>,
    source_def: DefId,
    contextual_def: DefId,
    call_param: TypeParamInfo,
) -> crate::instantiation::instantiate::TypeSubstitution {
    let func = FunctionShape {
        type_params: vec![call_param],
        params: Vec::new(),
        this_type: None,
        return_type: interner.lazy(source_def),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };
    let mut checker = StoreBackedReturnChecker { db };
    let mut evaluator = CallEvaluator::new(interner, &mut checker);
    evaluator.compute_return_context_substitution(&func, Some(interner.lazy(contextual_def)))
}

#[derive(Clone, Copy)]
struct LazyWrapPair {
    wrap_def: DefId,
    source_def: DefId,
    contextual_def: DefId,
    wrap_param: TypeParamInfo,
    call_param: TypeParamInfo,
    property_name: Atom,
    contextual_value: TypeId,
}

fn publish_lazy_wrap_pair(interner: &TypeInterner, store: &DefinitionStore, case: LazyWrapPair) {
    let wrap_param_id = interner.type_param(case.wrap_param);
    store.set_body_with_params(
        case.wrap_def,
        object_with_value(interner, case.property_name, wrap_param_id),
        Some(vec![case.wrap_param]),
    );
    let wrap_base = interner.lazy(case.wrap_def);
    let call_param_id = interner.type_param(case.call_param);
    store.set_body(
        case.source_def,
        interner.application(wrap_base, vec![call_param_id]),
    );
    store.set_body(
        case.contextual_def,
        interner.application(wrap_base, vec![case.contextual_value]),
    );
}

#[derive(Clone, Copy)]
struct NestedLazyWrapPair {
    inner_def: DefId,
    outer_def: DefId,
    source_def: DefId,
    contextual_def: DefId,
    inner_param: TypeParamInfo,
    outer_param: TypeParamInfo,
    call_param: TypeParamInfo,
    inner_property_name: Atom,
    outer_property_name: Atom,
    contextual_value: TypeId,
}

fn publish_nested_lazy_wrap_pair(
    interner: &TypeInterner,
    store: &DefinitionStore,
    case: NestedLazyWrapPair,
) {
    let inner_param_id = interner.type_param(case.inner_param);
    store.set_body_with_params(
        case.inner_def,
        object_with_value(interner, case.inner_property_name, inner_param_id),
        Some(vec![case.inner_param]),
    );
    let outer_param_id = interner.type_param(case.outer_param);
    store.set_body_with_params(
        case.outer_def,
        object_with_value(interner, case.outer_property_name, outer_param_id),
        Some(vec![case.outer_param]),
    );
    let inner_base = interner.lazy(case.inner_def);
    let outer_base = interner.lazy(case.outer_def);
    let call_param_id = interner.type_param(case.call_param);
    store.set_body(
        case.source_def,
        interner.application(
            outer_base,
            vec![interner.application(inner_base, vec![call_param_id])],
        ),
    );
    store.set_body(
        case.contextual_def,
        interner.application(
            outer_base,
            vec![interner.application(inner_base, vec![case.contextual_value])],
        ),
    );
}

#[derive(Clone, Copy)]
struct LazyPairApplication {
    pair_def: DefId,
    source_def: DefId,
    contextual_def: DefId,
    fixed_param: TypeParamInfo,
    value_param: TypeParamInfo,
    call_param: TypeParamInfo,
    fixed_property_name: Atom,
    value_property_name: Atom,
    fixed_value: TypeId,
    contextual_value: TypeId,
}

fn publish_lazy_pair_application(
    interner: &TypeInterner,
    store: &DefinitionStore,
    case: LazyPairApplication,
) {
    let fixed_param_id = interner.type_param(case.fixed_param);
    let value_param_id = interner.type_param(case.value_param);
    store.set_body_with_params(
        case.pair_def,
        interner.object(vec![
            PropertyInfo::new(case.fixed_property_name, fixed_param_id),
            PropertyInfo::new(case.value_property_name, value_param_id),
        ]),
        Some(vec![case.fixed_param, case.value_param]),
    );
    let pair_base = interner.lazy(case.pair_def);
    let call_param_id = interner.type_param(case.call_param);
    store.set_body(
        case.source_def,
        interner.application(pair_base, vec![case.fixed_value, call_param_id]),
    );
    store.set_body(
        case.contextual_def,
        interner.application(pair_base, vec![case.fixed_value, case.contextual_value]),
    );
}

#[derive(Clone, Copy)]
struct TransparentLazyAlias {
    alias_def: DefId,
    source_def: DefId,
    contextual_def: DefId,
    alias_param: TypeParamInfo,
    call_param: TypeParamInfo,
    contextual_value: TypeId,
}

fn publish_transparent_lazy_alias(
    interner: &TypeInterner,
    store: &DefinitionStore,
    case: TransparentLazyAlias,
) {
    let alias_param_id = interner.type_param(case.alias_param);
    store.set_body_with_params(case.alias_def, alias_param_id, Some(vec![case.alias_param]));
    let alias_base = interner.lazy(case.alias_def);
    let call_param_id = interner.type_param(case.call_param);
    store.set_body(
        case.source_def,
        interner.application(alias_base, vec![call_param_id]),
    );
    store.set_body(case.contextual_def, case.contextual_value);
}

#[test]
fn sort_type_params_by_name_orders_ascending_atom_ids() {
    let mut type_params = vec![tp(7), tp(1), tp(3)];
    sort_type_params_by_name(&mut type_params);

    let names: Vec<_> = type_params
        .iter()
        .map(|type_param| type_param.name)
        .collect();
    assert_eq!(names, vec![Atom(1), Atom(3), Atom(7)]);
}

#[test]
fn return_context_substitution_resolves_store_backed_lazy_application_bodies() {
    let interner = TypeInterner::new();
    let store = DefinitionStore::new();
    let wrap_def = DefId(143_510);
    let source_def = DefId(143_511);
    let contextual_def = DefId(143_512);
    let call_param = tp(303);
    publish_lazy_wrap_pair(
        &interner,
        &store,
        LazyWrapPair {
            wrap_def,
            source_def,
            contextual_def,
            wrap_param: tp(101),
            call_param,
            property_name: Atom(202),
            contextual_value: TypeId::STRING,
        },
    );
    let db = QueryCache::new(&interner).with_definition_store(&store);

    let _flag = InstResolverRereduceFlagGuard::new(true);
    let substitution = return_context_substitution_for_lazy_pair(
        &interner,
        &db,
        source_def,
        contextual_def,
        call_param,
    );

    assert_eq!(substitution.get(call_param.name), Some(TypeId::STRING));
}

#[test]
fn return_context_substitution_resolves_nested_store_backed_lazy_application_bodies() {
    let interner = TypeInterner::new();
    let store = DefinitionStore::new();
    let inner_def = DefId(143_540);
    let outer_def = DefId(143_541);
    let source_def = DefId(143_542);
    let contextual_def = DefId(143_543);
    let call_param = tp(1_103);
    publish_nested_lazy_wrap_pair(
        &interner,
        &store,
        NestedLazyWrapPair {
            inner_def,
            outer_def,
            source_def,
            contextual_def,
            inner_param: tp(901),
            outer_param: tp(902),
            call_param,
            inner_property_name: Atom(1_001),
            outer_property_name: Atom(1_002),
            contextual_value: TypeId::STRING,
        },
    );
    let db = QueryCache::new(&interner).with_definition_store(&store);

    let _flag = InstResolverRereduceFlagGuard::new(true);
    let substitution = return_context_substitution_for_lazy_pair(
        &interner,
        &db,
        source_def,
        contextual_def,
        call_param,
    );

    assert_eq!(substitution.get(call_param.name), Some(TypeId::STRING));
}

#[test]
fn return_context_substitution_matches_store_backed_lazy_pair_fixed_argument() {
    let interner = TypeInterner::new();
    let store = DefinitionStore::new();
    let pair_def = DefId(143_550);
    let source_def = DefId(143_551);
    let contextual_def = DefId(143_552);
    let call_param = tp(1_403);
    publish_lazy_pair_application(
        &interner,
        &store,
        LazyPairApplication {
            pair_def,
            source_def,
            contextual_def,
            fixed_param: tp(1_201),
            value_param: tp(1_202),
            call_param,
            fixed_property_name: Atom(1_301),
            value_property_name: Atom(1_302),
            fixed_value: TypeId::NUMBER,
            contextual_value: TypeId::STRING,
        },
    );
    let db = QueryCache::new(&interner).with_definition_store(&store);

    let _flag = InstResolverRereduceFlagGuard::new(true);
    let substitution = return_context_substitution_for_lazy_pair(
        &interner,
        &db,
        source_def,
        contextual_def,
        call_param,
    );

    assert_eq!(substitution.get(call_param.name), Some(TypeId::STRING));
}

#[test]
fn return_context_substitution_resolves_transparent_store_backed_lazy_alias_body() {
    let interner = TypeInterner::new();
    let store = DefinitionStore::new();
    let alias_def = DefId(143_560);
    let source_def = DefId(143_561);
    let contextual_def = DefId(143_562);
    let call_param = tp(1_703);
    publish_transparent_lazy_alias(
        &interner,
        &store,
        TransparentLazyAlias {
            alias_def,
            source_def,
            contextual_def,
            alias_param: tp(1_501),
            call_param,
            contextual_value: TypeId::STRING,
        },
    );
    let db = QueryCache::new(&interner).with_definition_store(&store);

    {
        let _flag = InstResolverRereduceFlagGuard::new(false);
        let substitution = return_context_substitution_for_lazy_pair(
            &interner,
            &db,
            source_def,
            contextual_def,
            call_param,
        );
        assert!(substitution.get(call_param.name).is_none());
    }

    let _flag = InstResolverRereduceFlagGuard::new(true);
    let substitution = return_context_substitution_for_lazy_pair(
        &interner,
        &db,
        source_def,
        contextual_def,
        call_param,
    );

    assert_eq!(substitution.get(call_param.name), Some(TypeId::STRING));
}

#[test]
fn return_context_substitution_keeps_lazy_bodies_deferred_without_rereduce_flag() {
    let interner = TypeInterner::new();
    let store = DefinitionStore::new();
    let wrap_def = DefId(143_520);
    let source_def = DefId(143_521);
    let contextual_def = DefId(143_522);
    let call_param = tp(603);
    publish_lazy_wrap_pair(
        &interner,
        &store,
        LazyWrapPair {
            wrap_def,
            source_def,
            contextual_def,
            wrap_param: tp(401),
            call_param,
            property_name: Atom(502),
            contextual_value: TypeId::STRING,
        },
    );
    let db = QueryCache::new(&interner).with_definition_store(&store);

    let _flag = InstResolverRereduceFlagGuard::new(false);
    let substitution = return_context_substitution_for_lazy_pair(
        &interner,
        &db,
        source_def,
        contextual_def,
        call_param,
    );

    assert!(substitution.get(call_param.name).is_none());
}

#[test]
fn return_context_substitution_requires_store_and_published_lazy_bodies() {
    let interner = TypeInterner::new();
    let source_def = DefId(143_531);
    let contextual_def = DefId(143_532);
    let call_param = tp(803);
    let no_store_db = QueryCache::new(&interner);
    let empty_store = DefinitionStore::new();
    let missing_body_db = QueryCache::new(&interner).with_definition_store(&empty_store);

    let _flag = InstResolverRereduceFlagGuard::new(true);
    let no_store_substitution = return_context_substitution_for_lazy_pair(
        &interner,
        &no_store_db,
        source_def,
        contextual_def,
        call_param,
    );
    let missing_body_substitution = return_context_substitution_for_lazy_pair(
        &interner,
        &missing_body_db,
        source_def,
        contextual_def,
        call_param,
    );

    assert!(no_store_substitution.get(call_param.name).is_none());
    assert!(missing_body_substitution.get(call_param.name).is_none());
}

#[derive(Default)]
struct BoundaryChecker;

impl AssignabilityChecker for BoundaryChecker {
    fn is_assignable_to(&mut self, _source: TypeId, _target: TypeId) -> bool {
        true
    }
}

#[test]
fn return_context_does_not_bind_foreign_same_named_scoped_parameter() {
    let interner = TypeInterner::new();
    let file = interner.intern_string("return-context-domain.ts");
    let name = interner.intern_string("U");
    let owned = TypeParamInfo {
        name,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::DeclScoped { file, node: 1 },
    };
    let foreign = interner.fresh_type_param(TypeParamInfo {
        origin: crate::types::TypeParamOrigin::DeclScoped { file, node: 2 },
        ..owned
    });
    let func = FunctionShape {
        type_params: vec![owned],
        params: Vec::new(),
        this_type: None,
        return_type: foreign,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };
    let mut checker = BoundaryChecker;
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let substitution = evaluator.compute_return_context_substitution(&func, Some(TypeId::STRING));

    assert!(substitution.get(name).is_none());
}

#[test]
fn resolve_with_request_returns_instantiated_side_channel_data() {
    let interner = TypeInterner::new();
    let type_param = tp(11);
    let param_name = Atom(23);
    let type_param_id = interner.type_param(type_param);
    let func = FunctionShape {
        type_params: vec![type_param],
        params: vec![ParamInfo::required(param_name, type_param_id)],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: Some(TypePredicate {
            asserts: false,
            target: TypePredicateTarget::Identifier(param_name),
            type_id: Some(type_param_id),
            parameter_index: Some(0),
        }),
        is_constructor: false,
        is_method: false,
    };
    let arg_types = [TypeId::STRING];
    let mut checker = BoundaryChecker;
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let mut result = evaluator.resolve_with_request(GenericCallRequest::new(&func, &arg_types));

    assert!(evaluator.last_instantiated_predicate.is_none());
    assert!(evaluator.last_instantiated_params.is_none());

    let (predicate, predicate_params) = result
        .take_instantiated_predicate()
        .expect("generic call should return the instantiated predicate");
    assert_eq!(predicate.type_id, Some(TypeId::STRING));
    assert_eq!(predicate_params[0].type_id, TypeId::STRING);
    assert!(result.take_instantiated_predicate().is_none());

    let params = result
        .take_instantiated_params()
        .expect("generic call should return instantiated params");
    assert_eq!(params[0].type_id, TypeId::STRING);
    assert!(result.take_instantiated_params().is_none());

    assert!(matches!(
        result.into_call_result(),
        CallResult::Success(TypeId::BOOLEAN)
    ));
}
