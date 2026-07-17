//! Tests pinning union simplification's treatment of generic-dependent
//! `Application` members.
//!
//! `evaluate_union` removes a member when the bypass-evaluation
//! `SubtypeChecker` judges it a subtype of a sibling. A generic application
//! (`Alias<DB, TB>` with type-parameter arguments) expands to a type that
//! checker cannot judge soundly — in the Kysely witness (#10663),
//! `AnyColumn<DB, TB>` (= `keyof DB[TB] & string`) looked string-like and was
//! absorbed by the object-shaped `Expression<unknown>` member, collapsing the
//! alias union and cascading into false `TS2416` on every implementing
//! method. tsc performs no subtype reduction on members that depend on
//! unresolved type parameters, so the member must survive evaluation.
//!
//! Concrete applications remain simplifiable: a fully-instantiated alias
//! member that *is* redundant must still be absorbed.

use super::*;
use crate::construction::TypeInterner;
use crate::def::{DefId, DefKind};
use crate::evaluation::evaluate::TypeEvaluator;
use crate::relations::subtype::{MAX_SUBTYPE_DEPTH, SubtypeChecker, TypeEnvironment};

fn optional_unknown_property_object(interner: &TypeInterner, prop: &str) -> TypeId {
    let mut info = PropertyInfo::new(
        interner.intern_string(prop),
        interner.union2(TypeId::UNKNOWN, TypeId::UNDEFINED),
    );
    info.optional = true;
    interner.object(vec![info])
}

fn value_property_object(interner: &TypeInterner, value: TypeId) -> TypeId {
    interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        value,
    )])
}

/// `type AnyColumn<DB, TB extends keyof DB> = keyof DB[TB] & string` —
/// registered in `env` under `def_id`; returns the generic application
/// `AnyColumn<DB, TB>` with the *type parameters themselves* as arguments
/// (the open-generic context of a method signature annotation).
fn generic_alias_application(
    interner: &TypeInterner,
    env: &mut TypeEnvironment,
    def_id: DefId,
) -> TypeId {
    let db_param = TypeParamInfo {
        name: interner.intern_string("DB"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let db_type = interner.intern(TypeData::TypeParameter(db_param));
    let tb_param = TypeParamInfo {
        name: interner.intern_string("TB"),
        constraint: Some(interner.keyof(db_type)),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let tb_type = interner.intern(TypeData::TypeParameter(tb_param));

    let body = interner.intersection(vec![
        interner.keyof(interner.index_access(db_type, tb_type)),
        TypeId::STRING,
    ]);
    env.insert_def_with_params(def_id, body, vec![db_param, tb_param]);
    env.insert_def_kind(def_id, DefKind::TypeAlias);

    interner.application(interner.lazy(def_id), vec![db_type, tb_type])
}

#[test]
fn generic_dependent_application_member_survives_union_evaluation() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();
    let app = generic_alias_application(&interner, &mut env, DefId(301));
    let expression_like = optional_unknown_property_object(&interner, "expressionType");

    let union = interner.union(vec![app, expression_like]);
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(union);

    let Some(TypeData::Union(list_id)) = interner.lookup(result) else {
        panic!(
            "expected the union to survive evaluation, got {:?}",
            interner.lookup(result)
        );
    };
    let members = interner.type_list(list_id);
    assert_eq!(
        members.len(),
        2,
        "the generic-dependent alias member must not be absorbed by the object member",
    );
}

#[test]
fn renamed_binders_member_survives_union_evaluation() {
    // Same shape with different binder names and member order — the rule
    // must follow the type shape, not the identifier spelling.
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let schema_param = TypeParamInfo {
        name: interner.intern_string("Schema"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let schema_type = interner.intern(TypeData::TypeParameter(schema_param));
    let table_param = TypeParamInfo {
        name: interner.intern_string("Table"),
        constraint: Some(interner.keyof(schema_type)),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let table_type = interner.intern(TypeData::TypeParameter(table_param));

    let body = interner.intersection(vec![
        interner.keyof(interner.index_access(schema_type, table_type)),
        TypeId::STRING,
    ]);
    env.insert_def_with_params(DefId(302), body, vec![schema_param, table_param]);
    env.insert_def_kind(DefId(302), DefKind::TypeAlias);
    let app = interner.application(interner.lazy(DefId(302)), vec![schema_type, table_type]);

    let operand_like = optional_unknown_property_object(&interner, "operandType");

    let union = interner.union(vec![operand_like, app]);
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(union);

    let Some(TypeData::Union(list_id)) = interner.lookup(result) else {
        panic!(
            "expected the union to survive evaluation, got {:?}",
            interner.lookup(result)
        );
    };
    assert_eq!(interner.type_list(list_id).len(), 2);
}

#[test]
fn concrete_application_member_still_simplifiable() {
    // Negative control: a fully-concrete alias application that evaluates to
    // a subtype of a sibling must still be absorbed (the canonicalizer-backed
    // simplification stays enabled for non-generic members).
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    // type Boxed<T> = { value: T }
    let body = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        t_type,
    )]);
    env.insert_def_with_params(DefId(303), body, vec![t_param]);
    env.insert_def_kind(DefId(303), DefKind::TypeAlias);
    // Boxed<"a"> | { value: string } — the literal instantiation is a strict
    // subtype of the wider object member and is redundant in the union.
    let lit_a = interner.literal_string("a");
    let app = interner.application(interner.lazy(DefId(303)), vec![lit_a]);
    let wider = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::STRING,
    )]);

    let union = interner.union(vec![app, wider]);
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(union);

    assert_eq!(
        result, wider,
        "a concrete redundant application member must still be absorbed",
    );
}

fn compound_simplification_checker(
    interner: &TypeInterner,
) -> SubtypeChecker<'_, crate::relations::subtype::NoopResolver> {
    let mut checker = SubtypeChecker::new(interner);
    checker.bypass_evaluation = true;
    // Mirror the production reduction checker (see `remove_redundant_members`):
    // member removal only acts on definitive verdicts, so the probe key must
    // carry the same not-coinductive relation mode.
    checker.assume_related_on_cycle = false;
    checker.max_depth = MAX_SUBTYPE_DEPTH;
    checker
}

#[test]
fn compound_simplification_subtype_cache_records_pair_results() {
    let interner = TypeInterner::new();
    let lit_a = interner.literal_string("a");
    let narrow = value_property_object(&interner, lit_a);
    let wide = value_property_object(&interner, TypeId::STRING);
    let union = interner.union(vec![narrow, wide]);
    let mut evaluator = TypeEvaluator::new(&interner);

    assert_eq!(evaluator.cache_statistics().compound_subtype_entries, 0);
    assert_eq!(
        evaluator.evaluate(union),
        wide,
        "the narrower object member should be absorbed by the wider object member",
    );

    let stats = evaluator.cache_statistics();
    assert_eq!(
        stats.compound_subtype_entries, 1,
        "compound simplification should memoize the decisive subtype pair",
    );
    assert!(stats.estimated_size_bytes() > 0);
}

#[test]
fn compound_simplification_subtype_cache_hit_controls_reduction() {
    let interner = TypeInterner::new();
    let lit_a = interner.literal_string("a");
    let narrow = value_property_object(&interner, lit_a);
    let wide = value_property_object(&interner, TypeId::STRING);
    let union = interner.union(vec![narrow, wide]);
    let checker = compound_simplification_checker(&interner);
    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.seed_compound_subtype_cache_for_test(&checker, narrow, wide, false);

    let result = evaluator.evaluate(union);
    let Some(TypeData::Union(list_id)) = interner.lookup(result) else {
        panic!(
            "seeded false subtype memo should keep the union intact, got {:?}",
            interner.lookup(result)
        );
    };
    let members = interner.type_list(list_id);
    assert_eq!(members.len(), 2);
    assert!(members.contains(&narrow));
    assert!(members.contains(&wide));
    assert_eq!(
        evaluator.cache_statistics().compound_subtype_entries,
        2,
        "the seeded ordered-pair entry and reverse miss should remain separate",
    );
}

#[test]
fn compound_simplification_subtype_cache_partitions_bypass_mode() {
    let interner = TypeInterner::new();
    let lit_a = interner.literal_string("a");
    let narrow = value_property_object(&interner, lit_a);
    let wide = value_property_object(&interner, TypeId::STRING);
    let union = interner.union(vec![narrow, wide]);
    let checker = SubtypeChecker::new(&interner);
    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.seed_compound_subtype_cache_for_test(&checker, narrow, wide, false);

    assert_eq!(
        evaluator.evaluate(union),
        wide,
        "a seed from the non-bypass relation mode must not affect compound simplification",
    );
    assert_eq!(
        evaluator.cache_statistics().compound_subtype_entries,
        2,
        "the non-bypass seed and production bypass key must live in separate slots",
    );
}

#[test]
fn compound_simplification_subtype_cache_clears_on_reset_and_option_flips() {
    let interner = TypeInterner::new();
    let lit_a = interner.literal_string("a");
    let narrow = value_property_object(&interner, lit_a);
    let wide = value_property_object(&interner, TypeId::STRING);
    let checker = compound_simplification_checker(&interner);
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.seed_compound_subtype_cache_for_test(&checker, narrow, wide, false);
    assert_eq!(evaluator.cache_statistics().compound_subtype_entries, 1);
    evaluator.reset();
    assert_eq!(
        evaluator.cache_statistics().compound_subtype_entries,
        0,
        "reset must clear operation-local compound subtype results",
    );

    evaluator.seed_compound_subtype_cache_for_test(&checker, narrow, wide, false);
    assert_eq!(evaluator.cache_statistics().compound_subtype_entries, 1);
    evaluator.set_no_unchecked_indexed_access(true);
    assert_eq!(
        evaluator.cache_statistics().compound_subtype_entries,
        0,
        "option-sensitive evaluator mode changes must clear compound subtype results",
    );

    evaluator.seed_compound_subtype_cache_for_test(&checker, narrow, wide, false);
    assert_eq!(evaluator.cache_statistics().compound_subtype_entries, 1);
    evaluator.set_exact_optional_property_types(true);
    assert_eq!(
        evaluator.cache_statistics().compound_subtype_entries,
        0,
        "exact-optional mode changes must clear compound subtype results",
    );
}

#[test]
fn compound_simplification_subtype_cache_partitions_max_depth() {
    let interner = TypeInterner::new();
    let lit_a = interner.literal_string("a");
    let narrow = value_property_object(&interner, lit_a);
    let wide = value_property_object(&interner, TypeId::STRING);
    let union = interner.union(vec![narrow, wide]);
    let mut seeded_checker = compound_simplification_checker(&interner);
    seeded_checker.max_depth = MAX_SUBTYPE_DEPTH - 1;
    let mut evaluator = TypeEvaluator::new(&interner);
    evaluator.seed_compound_subtype_cache_for_test(&seeded_checker, narrow, wide, false);

    assert_eq!(
        evaluator.evaluate(union),
        wide,
        "a seed from a different subtype max-depth must not affect production simplification",
    );
    assert_eq!(
        evaluator.cache_statistics().compound_subtype_entries,
        2,
        "the shallow-depth seed and production-depth key must live in separate slots",
    );
}
