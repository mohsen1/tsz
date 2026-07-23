//! Apparent-constraint coverage for deferred distributive extraction conditionals.
//!
//! A constrained check parameter may point at a resolver-owned alias application.
//! The relation must expose that alias before substituting the constraint into the
//! conditional, otherwise the evaluator sees the application as one opaque check
//! and loses distribution over the alias body's union.

use crate::def::{DefId, DefKind};
use crate::evaluation::evaluate::evaluate_type_with_resolver;
use crate::intern::TypeInterner;
use crate::relations::subtype::{SubtypeChecker, TypeEnvironment};
use crate::types::{ConditionalType, PropertyInfo, TypeId, TypeParamInfo, Variance};
use std::sync::Arc;

fn register_input_alias(
    interner: &TypeInterner,
    env: &mut TypeEnvironment,
    def_id: DefId,
    argument: TypeId,
) -> TypeId {
    // `type Input<Payload> = string | { value: Payload }`.
    let payload = TypeParamInfo::simple(interner.intern_string("Payload"));
    let payload_type = interner.type_param(payload);
    let value_name = interner.intern_string("value");
    let carrier = interner.object(vec![PropertyInfo::new(value_name, payload_type)]);
    let body = interner.union2(TypeId::STRING, carrier);

    env.insert_def_with_params(def_id, body, vec![payload]);
    env.insert_def_kind(def_id, DefKind::TypeAlias);
    interner.application(interner.lazy(def_id), vec![argument])
}

fn register_never_alias(
    interner: &TypeInterner,
    env: &mut TypeEnvironment,
    def_id: DefId,
) -> TypeId {
    // Keep this generic so the constraint remains an alias Application rather
    // than simplifying to the intrinsic before the resolver-backed path runs.
    let unused = TypeParamInfo::simple(interner.intern_string("Unused"));
    env.insert_def_with_params(def_id, TypeId::NEVER, vec![unused]);
    env.insert_def_kind(def_id, DefKind::TypeAlias);
    interner.application(interner.lazy(def_id), vec![TypeId::ANY])
}

fn constrained_subject(interner: &TypeInterner, name: &str, constraint: TypeId) -> TypeId {
    let mut subject = TypeParamInfo::simple(interner.intern_string(name));
    subject.constraint = Some(constraint);
    interner.type_param(subject)
}

fn extraction_conditional(interner: &TypeInterner, constraint: TypeId) -> TypeId {
    // `Subject extends { value: infer Extracted } ? Extracted : unknown`.
    let subject = constrained_subject(interner, "Subject", constraint);
    let extracted = interner.infer(TypeParamInfo::simple(interner.intern_string("Extracted")));
    let pattern = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        extracted,
    )]);
    interner.conditional(ConditionalType {
        check_type: subject,
        extends_type: pattern,
        true_type: extracted,
        false_type: TypeId::UNKNOWN,
        is_distributive: true,
    })
}

fn optional_value_extraction_conditional(interner: &TypeInterner, constraint: TypeId) -> TypeId {
    // `Subject extends { value: infer Extracted | undefined }
    //   ? Extracted : unknown`.
    let subject = constrained_subject(interner, "OptionalSubject", constraint);
    let extracted = interner.infer(TypeParamInfo::simple(interner.intern_string("Extracted")));
    let pattern = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        interner.union2(extracted, TypeId::UNDEFINED),
    )]);
    interner.conditional(ConditionalType {
        check_type: subject,
        extends_type: pattern,
        true_type: extracted,
        false_type: TypeId::UNKNOWN,
        is_distributive: true,
    })
}

#[test]
fn resolver_exposed_any_alias_constraint_is_apparently_any() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();
    let constraint = register_input_alias(&interner, &mut env, DefId(158_560), TypeId::ANY);
    let conditional = extraction_conditional(&interner, constraint);
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    assert_eq!(
        checker.infer_extraction_conditional_constraint(conditional),
        Some(TypeId::ANY),
        "Input<any> must expose `string | {{ value: any }}`, distribute the extraction, and reduce `unknown | any` to any"
    );
    for target in [
        TypeId::BOOLEAN,
        TypeId::STRING,
        TypeId::NUMBER,
        TypeId::OBJECT,
        TypeId::UNKNOWN,
    ] {
        assert!(
            checker.is_subtype_of(conditional, target),
            "an extraction with apparent constraint any must be assignable to {target:?}"
        );
    }
    assert!(
        !checker.is_subtype_of(conditional, TypeId::NEVER),
        "any remains unassignable to never"
    );
}

#[test]
fn any_remains_infer_evidence_next_to_a_fixed_union_member() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();
    let constraint = register_input_alias(&interner, &mut env, DefId(158_569), TypeId::ANY);
    let conditional = optional_value_extraction_conditional(&interner, constraint);
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    assert_eq!(
        checker.infer_extraction_conditional_constraint(conditional),
        Some(TypeId::ANY),
        "`any` is residual evidence for `infer V | undefined`, not an exact `undefined` match"
    );
    assert!(checker.is_subtype_of(conditional, TypeId::BOOLEAN));
}

#[test]
fn resolver_exposed_number_alias_constraint_is_apparently_unknown() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();
    let constraint = register_input_alias(&interner, &mut env, DefId(158_561), TypeId::NUMBER);
    let conditional = extraction_conditional(&interner, constraint);
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    assert_eq!(
        checker.infer_extraction_conditional_constraint(conditional),
        Some(TypeId::UNKNOWN),
        "Input<number> must distribute to `unknown | number`, whose apparent type is unknown"
    );
    assert!(checker.is_subtype_of(conditional, TypeId::UNKNOWN));
    for target in [
        TypeId::BOOLEAN,
        TypeId::STRING,
        TypeId::NUMBER,
        TypeId::OBJECT,
        TypeId::NEVER,
    ] {
        assert!(
            !checker.is_subtype_of(conditional, target),
            "an extraction with apparent constraint unknown must not be assignable to {target:?}"
        );
    }
}

#[test]
fn infer_free_alias_predicate_keeps_the_default_constraint() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();
    let constraint = register_input_alias(&interner, &mut env, DefId(158_562), TypeId::ANY);
    let subject = constrained_subject(&interner, "PredicateSubject", constraint);
    let pattern = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::ANY,
    )]);
    let conditional = interner.conditional(ConditionalType {
        check_type: subject,
        extends_type: pattern,
        true_type: TypeId::NUMBER,
        false_type: TypeId::UNKNOWN,
        is_distributive: true,
    });
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    assert_eq!(
        checker.infer_extraction_conditional_constraint(conditional),
        None,
        "an infer-free predicate must stay behind the ordinary determinism gate"
    );
    assert!(checker.is_subtype_of(conditional, TypeId::UNKNOWN));
    assert!(
        !checker.is_subtype_of(conditional, TypeId::NUMBER),
        "the predicate's true branch is not a sound apparent constraint"
    );
}

#[test]
fn resolver_exposed_never_constraint_falls_back_to_default_unknown() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();
    let constraint = register_never_alias(&interner, &mut env, DefId(158_563));
    let conditional = extraction_conditional(&interner, constraint);
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    assert_eq!(
        checker.infer_extraction_conditional_constraint(conditional),
        None,
        "tsc discards a distributive constraint that instantiates to never"
    );
    assert!(checker.is_subtype_of(conditional, TypeId::UNKNOWN));
    assert!(!checker.is_subtype_of(conditional, TypeId::BOOLEAN));
    assert!(
        !checker.is_subtype_of(conditional, TypeId::NEVER),
        "discarding the never constraint must preserve the default apparent constraint unknown"
    );
}

#[test]
fn non_infer_zeroof_empty_constraint_keeps_never_subtype_witness() {
    let interner = TypeInterner::new();
    let empty_object = interner.object(vec![]);
    let subject = constrained_subject(&interner, "Value", empty_object);
    let zero = interner.literal_number(0.0);
    let empty_string = interner.literal_string("");
    let branch = |extends_type, true_type, false_type| {
        interner.conditional(ConditionalType {
            check_type: subject,
            extends_type,
            true_type,
            false_type,
            is_distributive: true,
        })
    };

    // `type ZeroOf<T> =
    //   T extends null ? null :
    //   T extends undefined ? undefined :
    //   T extends string ? "" :
    //   T extends number ? 0 :
    //   T extends boolean ? false :
    //   never`.
    let boolean_case = branch(TypeId::BOOLEAN, TypeId::BOOLEAN_FALSE, TypeId::NEVER);
    let number_case = branch(TypeId::NUMBER, zero, boolean_case);
    let string_case = branch(TypeId::STRING, empty_string, number_case);
    let undefined_case = branch(TypeId::UNDEFINED, TypeId::UNDEFINED, string_case);
    let zero_of = branch(TypeId::NULL, TypeId::NULL, undefined_case);
    let target = interner.union(vec![empty_string, zero, TypeId::BOOLEAN_FALSE]);
    let mut checker = SubtypeChecker::new(&interner);

    assert!(
        checker.is_subtype_of(zero_of, target),
        "`ZeroOf<T>` for `T extends {{}}` evaluates its distributive constraint to `never`, which remains a valid subtype witness"
    );
}

fn assert_reference_extraction_relation(outer_name: &str) {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();
    let input = register_input_alias(&interner, &mut env, DefId(158_564), TypeId::ANY);

    // `type ExtractReferenceValue<Reference> =
    //   Reference extends { value: infer Value } ? Value : unknown`.
    let alias_param = TypeParamInfo::simple(interner.intern_string("Reference"));
    let alias_param_type = interner.type_param(alias_param);
    let extracted = interner.infer(TypeParamInfo::simple(interner.intern_string("Value")));
    let pattern = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        extracted,
    )]);
    let body = interner.conditional(ConditionalType {
        check_type: alias_param_type,
        extends_type: pattern,
        true_type: extracted,
        false_type: TypeId::UNKNOWN,
        is_distributive: true,
    });
    let extract_def = DefId(158_565);
    env.insert_def_with_params(extract_def, body, vec![alias_param]);
    env.insert_def_kind(extract_def, DefKind::TypeAlias);

    let argument = constrained_subject(&interner, outer_name, input);
    let application = interner.application(interner.lazy(extract_def), vec![argument]);
    let evaluated = evaluate_type_with_resolver(&interner, &env, application);
    assert!(
        crate::visitor::conditional_type_id(&interner, evaluated).is_some(),
        "a generic extraction remains deferred until its relation is queried"
    );
    let source_value = interner.union2(evaluated, TypeId::UNDEFINED);
    let application_source_value = interner.union2(application, TypeId::UNDEFINED);
    let target_value = interner.union2(TypeId::BOOLEAN, TypeId::UNDEFINED);
    let value_name = interner.intern_string("expressionType");
    let source_box = interner.object(vec![PropertyInfo::new(value_name, source_value)]);
    let target_box = interner.object(vec![PropertyInfo::new(value_name, target_value)]);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    assert_eq!(
        checker.infer_extraction_conditional_constraint(evaluated),
        Some(TypeId::ANY)
    );
    assert!(
        checker.is_subtype_of(evaluated, TypeId::BOOLEAN),
        "the resolver-exposed Input<any> constraint makes the extraction apparently any"
    );
    assert!(
        checker.is_subtype_of(application, TypeId::BOOLEAN),
        "the application expansion path must reach the extraction's apparent constraint"
    );
    assert!(
        checker.is_subtype_of(source_value, target_value),
        "the apparent extraction constraint must survive union member comparison"
    );
    assert!(
        checker.is_subtype_of(application_source_value, target_value),
        "an unevaluated extraction application must survive union member comparison"
    );
    assert!(
        checker.is_subtype_of(source_box, target_box),
        "the apparent extraction constraint must survive object property comparison"
    );
}

#[test]
fn application_extraction_uses_the_caller_constraint_in_relations() {
    assert_reference_extraction_relation("Reference");
    assert_reference_extraction_relation("OuterReference");
}

#[test]
fn generic_container_relation_exposes_nested_extraction_constraint() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // `interface ContextualBox<Scope, Key, Value> {
    //   expressionType: Value | undefined
    // }`.
    let scope = TypeParamInfo::simple(interner.intern_string("Scope"));
    let key = TypeParamInfo::simple(interner.intern_string("Key"));
    let value = TypeParamInfo::simple(interner.intern_string("Value"));
    let scope_type = interner.type_param(scope);
    let key_type = interner.type_param(key);
    let value_type = interner.type_param(value);
    let expression_name = interner.intern_string("expressionType");
    let box_def = DefId(158_566);
    let box_body = interner.object(vec![PropertyInfo::readonly(
        expression_name,
        interner.union2(value_type, TypeId::UNDEFINED),
    )]);
    env.insert_def_with_params(box_def, box_body, vec![scope, key, value]);
    env.insert_def_kind(box_def, DefKind::Interface);
    env.insert_declared_variances(
        box_def,
        Arc::from([
            Variance::empty(),
            Variance::empty(),
            Variance::COVARIANT | Variance::DIRECT_USAGE,
        ]),
    );

    // `type ReferenceInput<S, K> = string | ContextualBox<S, K, any>`.
    let input_def = DefId(158_567);
    let input_box = interner.application(
        interner.lazy(box_def),
        vec![scope_type, key_type, TypeId::ANY],
    );
    let input_body = interner.union2(TypeId::STRING, input_box);
    env.insert_def_with_params(input_def, input_body, vec![scope, key]);
    env.insert_def_kind(input_def, DefKind::TypeAlias);

    // `type ExtractValue<S, K, Ref> =
    //   Ref extends ContextualBox<any, any, infer V> ? V : unknown`.
    let reference = TypeParamInfo::simple(interner.intern_string("Reference"));
    let reference_type = interner.type_param(reference);
    let extracted = interner.infer(TypeParamInfo::simple(interner.intern_string("Extracted")));
    let extract_pattern = interner.application(
        interner.lazy(box_def),
        vec![TypeId::ANY, TypeId::ANY, extracted],
    );
    let extract_body = interner.conditional(ConditionalType {
        check_type: reference_type,
        extends_type: extract_pattern,
        true_type: extracted,
        false_type: TypeId::UNKNOWN,
        is_distributive: true,
    });
    let extract_def = DefId(158_568);
    env.insert_def_with_params(extract_def, extract_body, vec![scope, key, reference]);
    env.insert_def_kind(extract_def, DefKind::TypeAlias);

    let outer_input = interner.application(interner.lazy(input_def), vec![scope_type, key_type]);
    let outer_reference = constrained_subject(&interner, "OuterReference", outer_input);
    let extraction = interner.application(
        interner.lazy(extract_def),
        vec![scope_type, key_type, outer_reference],
    );
    let source = interner.application(
        interner.lazy(box_def),
        vec![scope_type, key_type, extraction],
    );
    let target = interner.application(
        interner.lazy(box_def),
        vec![scope_type, key_type, TypeId::BOOLEAN],
    );

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    assert!(checker.is_subtype_of(extraction, TypeId::BOOLEAN));
    assert!(
        checker.is_subtype_of(source, target),
        "same-base generic variance must use the nested extraction's resolver-backed apparent constraint"
    );
}
