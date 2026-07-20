use crate::TypeInterner;
use crate::caches::query_cache_evaluation::StoreOnlyResolver;
use crate::construction::QueryDatabase;
use crate::def::{DefId, DefinitionStore};
use crate::instantiation::instantiate::TypeSubstitution;
use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::relations::subtype::TypeResolver;
use crate::types::{FunctionShape, PropertyInfo, TypeId, TypeParamInfo};
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

struct ReturnContextChecker<'a> {
    resolver: &'a dyn TypeResolver,
}

impl AssignabilityChecker for ReturnContextChecker<'_> {
    fn is_assignable_to(&mut self, _source: TypeId, _target: TypeId) -> bool {
        true
    }

    fn type_resolver(&self) -> Option<&dyn TypeResolver> {
        Some(self.resolver)
    }
}

fn substitution(
    interner: &TypeInterner,
    resolver: &dyn TypeResolver,
    type_params: Vec<TypeParamInfo>,
    return_type: TypeId,
    contextual_type: TypeId,
) -> TypeSubstitution {
    let func = FunctionShape {
        type_params,
        params: Vec::new(),
        this_type: None,
        return_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };
    CallEvaluator::new(interner, &mut ReturnContextChecker { resolver })
        .compute_return_context_substitution(&func, Some(contextual_type))
}

fn foreign_composite(interner: &TypeInterner, outer: TypeParamInfo) -> TypeId {
    let mapped_alias = interner.lazy(DefId(90_002));
    let mapped = interner.application(mapped_alias, vec![interner.type_param(outer)]);
    interner.union(vec![mapped, TypeId::NULL])
}

#[test]
fn same_base_aligned_return_argument_keeps_foreign_composite_whole() {
    let interner = TypeInterner::new();
    let resolver = interner.as_type_resolver();
    let call_param = tp(11);
    let target_arg = foreign_composite(&interner, tp(12));
    let base = interner.lazy(DefId(90_001));

    let result = substitution(
        &interner,
        resolver,
        vec![call_param],
        interner.application(base, vec![interner.type_param(call_param)]),
        interner.application(base, vec![target_arg]),
    );

    assert_eq!(result.get(call_param.name), Some(target_arg));
}

#[test]
fn canonical_base_definitions_align_return_arguments() {
    let interner = TypeInterner::new();
    let store = DefinitionStore::new();
    let alias_base = DefId(90_010);
    let declared_base = DefId(90_011);
    store.set_alias_forward(alias_base, declared_base);
    let resolver = StoreOnlyResolver::new(&store);
    let call_param = tp(21);
    let target_arg = foreign_composite(&interner, tp(22));

    let result = substitution(
        &interner,
        &resolver,
        vec![call_param],
        interner.application(
            interner.lazy(alias_base),
            vec![interner.type_param(call_param)],
        ),
        interner.application(interner.lazy(declared_base), vec![target_arg]),
    );

    assert_eq!(result.get(call_param.name), Some(target_arg));
}

#[test]
fn aligned_return_argument_rejects_invalid_and_other_tracked_targets() {
    let interner = TypeInterner::new();
    let resolver = interner.as_type_resolver();
    let first = tp(31);
    let second = tp(32);
    let base = interner.lazy(DefId(90_020));
    let source = interner.application(base, vec![interner.type_param(first)]);

    for invalid in [TypeId::ANY, TypeId::UNKNOWN, TypeId::ERROR] {
        let result = substitution(
            &interner,
            resolver,
            vec![first],
            source,
            interner.application(base, vec![invalid]),
        );
        assert_eq!(result.get(first.name), None, "invalid target {invalid:?}");
    }

    let target_with_other = interner.application(
        interner.lazy(DefId(90_021)),
        vec![interner.type_param(second)],
    );
    let result = substitution(
        &interner,
        resolver,
        vec![first, second],
        source,
        interner.application(base, vec![target_with_other]),
    );
    assert_eq!(result.get(first.name), None);
}

#[test]
fn different_base_and_nested_member_paths_keep_untracked_guard() {
    let interner = TypeInterner::new();
    let resolver = interner.as_type_resolver();
    let call_param = tp(41);
    let target_arg = foreign_composite(&interner, tp(42));
    let source_base = interner.lazy(DefId(90_030));
    let target_base = interner.lazy(DefId(90_031));
    let source_app = interner.application(source_base, vec![interner.type_param(call_param)]);
    let target_app = interner.application(target_base, vec![target_arg]);

    let different_base = substitution(
        &interner,
        resolver,
        vec![call_param],
        source_app,
        target_app,
    );
    assert_eq!(different_base.get(call_param.name), None);

    let member = Atom(90_032);
    let nested = substitution(
        &interner,
        resolver,
        vec![call_param],
        interner.object(vec![PropertyInfo::new(member, source_app)]),
        interner.object(vec![PropertyInfo::new(
            member,
            interner.application(source_base, vec![target_arg]),
        )]),
    );
    assert_eq!(nested.get(call_param.name), None);
}

#[test]
fn union_context_with_disagreeing_same_base_arms_stays_ambiguous() {
    let interner = TypeInterner::new();
    let resolver = interner.as_type_resolver();
    let call_param = tp(51);
    let base = interner.lazy(DefId(90_040));
    let source = interner.application(base, vec![interner.type_param(call_param)]);
    let contextual = interner.union(vec![
        interner.application(base, vec![TypeId::STRING]),
        interner.application(base, vec![TypeId::NUMBER]),
    ]);

    let result = substitution(&interner, resolver, vec![call_param], source, contextual);
    assert_eq!(result.get(call_param.name), None);
}
