//! Tests pinning the phase-level contract of
//! [`TypeEvaluator::evaluate_application`] after the orchestrator split.
//!
//! Each test exercises one of the documented phases (callee normalization,
//! per-DefId depth guard, body-aware shortcut paths, instantiation +
//! display-alias bookkeeping) so a future regression that violates the
//! contract surfaces here rather than only inside the broad conformance
//! suite.

use super::*;
use crate::construction::TypeInterner;
use crate::def::{DefId, DefKind};
use crate::evaluation::evaluate::TypeEvaluator;
use crate::relations::subtype::TypeEnvironment;

fn unconstrained_param(interner: &TypeInterner, name: &str) -> TypeParamInfo {
    TypeParamInfo {
        name: interner.intern_string(name),
        constraint: None,
        default: None,
        is_const: false,
    }
}

/// Register `def_id` against `kind` and `body` (with the given `params`) in
/// `env` and produce `app = body<args>` as a `Lazy(def_id)` application.
fn alias_application(
    interner: &TypeInterner,
    env: &mut TypeEnvironment,
    def_id: DefId,
    kind: DefKind,
    body: TypeId,
    params: Vec<TypeParamInfo>,
    args: Vec<TypeId>,
) -> TypeId {
    env.insert_def_with_params(def_id, body, params);
    env.insert_def_kind(def_id, kind);
    interner.application(interner.lazy(def_id), args)
}

/// Phase 1 — callee normalization. An application whose base does not
/// normalize to a `DefId` must stay opaque rather than collapse to its
/// body, so later resolver passes can expand it correctly.
#[test]
fn evaluate_application_base_without_def_id_stays_opaque() {
    let interner = TypeInterner::new();
    // `Application(Array<...>, [string])` — base is a structural array,
    // not a `Lazy(DefId)`, so no `DefId` can be recovered.
    let array_t = interner.array(TypeId::NUMBER);
    let app = interner.application(array_t, vec![TypeId::STRING]);

    let env = TypeEnvironment::new();
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(app);

    assert_eq!(
        result, app,
        "an application whose base lacks a DefId must remain interned as-is"
    );
}

/// Phase 5 — known-params path. `Box<string>` with body `{ value: T }`
/// must instantiate to `{ value: string }`.
#[test]
fn evaluate_application_known_params_instantiates_alias_body() {
    let interner = TypeInterner::new();
    let t_param = unconstrained_param(&interner, "T");
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let value_name = interner.intern_string("value");
    let body = interner.object(vec![PropertyInfo::new(value_name, t_type)]);

    let mut env = TypeEnvironment::new();
    let app = alias_application(
        &interner,
        &mut env,
        DefId(101),
        DefKind::TypeAlias,
        body,
        vec![t_param],
        vec![TypeId::STRING],
    );

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(app);

    let expected = interner.object(vec![PropertyInfo::new(value_name, TypeId::STRING)]);
    assert_eq!(
        result, expected,
        "Box<string> must instantiate to {{ value: string }}"
    );
}

/// Phase 5 — UNKNOWN body. When the resolver returns `unknown` (because
/// the declaring file is still being processed in parallel checking),
/// the orchestrator must bail and keep the `Application` opaque so a
/// later pass with a populated body can expand it.
#[test]
fn evaluate_application_unknown_body_keeps_application_opaque() {
    let interner = TypeInterner::new();
    let t_param = unconstrained_param(&interner, "T");

    let mut env = TypeEnvironment::new();
    let app = alias_application(
        &interner,
        &mut env,
        DefId(202),
        DefKind::TypeAlias,
        // Unknown sentinel mirrors the cross-file race condition.
        TypeId::UNKNOWN,
        vec![t_param],
        vec![TypeId::STRING],
    );

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(app);

    assert_eq!(
        result, app,
        "unknown alias body must not collapse `Foo<Args>` to bare `unknown`"
    );
}

/// Phase 5 — homomorphic mapped-type passthrough. `Box<number>` where
/// `Box<T> = { [P in keyof T]: T[P] }` returns the primitive argument
/// directly without expanding the mapped body, matching tsc.
///
/// Per-name-rename axis (CLAUDE.md §25): the type-parameter and
/// iteration-variable names vary so the test pins the structural rule,
/// not a specific spelling.
#[test]
fn evaluate_application_homomorphic_passthrough_returns_primitive() {
    for (param_name, iter_name) in [("T", "P"), ("U", "K"), ("Source", "Key")] {
        let interner = TypeInterner::new();
        let t_param = unconstrained_param(&interner, param_name);
        let p_param = unconstrained_param(&interner, iter_name);
        let t_type = interner.intern(TypeData::TypeParameter(t_param));
        let p_type = interner.intern(TypeData::TypeParameter(p_param));
        let keyof_t = interner.intern(TypeData::KeyOf(t_type));
        let t_index_p = interner.intern(TypeData::IndexAccess(t_type, p_type));

        let mapped_body = interner.mapped(MappedType {
            type_param: p_param,
            constraint: keyof_t,
            name_type: None,
            template: t_index_p,
            optional_modifier: None,
            readonly_modifier: None,
        });

        let mut env = TypeEnvironment::new();
        let app = alias_application(
            &interner,
            &mut env,
            DefId(303),
            DefKind::TypeAlias,
            mapped_body,
            vec![t_param],
            vec![TypeId::NUMBER],
        );

        let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
        let result = evaluator.evaluate(app);

        assert_eq!(
            result,
            TypeId::NUMBER,
            "homomorphic passthrough must return the primitive argument directly \
             for params named ({param_name}, {iter_name})"
        );
    }
}

/// Phase 5 — class instance extraction. When `DefKind::Class` resolves
/// to a `Callable` with construct signatures, the application must
/// return the construct signature's RETURN type (the instance), not the
/// constructor itself.
#[test]
fn evaluate_application_class_uses_construct_signature_return_type() {
    let interner = TypeInterner::new();
    let t_param = unconstrained_param(&interner, "T");
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let value_name = interner.intern_string("value");
    let instance_shape = interner.object(vec![PropertyInfo::new(value_name, t_type)]);

    let construct_sig = CallSignature {
        type_params: vec![],
        params: vec![ParamInfo::required(value_name, t_type)],
        this_type: None,
        return_type: instance_shape,
        type_predicate: None,
        is_method: false,
    };
    let class_body = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![construct_sig],
        properties: vec![],
        ..Default::default()
    });

    let mut env = TypeEnvironment::new();
    let app = alias_application(
        &interner,
        &mut env,
        DefId(404),
        DefKind::Class,
        class_body,
        vec![t_param],
        vec![TypeId::STRING],
    );

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(app);

    let expected = interner.object(vec![PropertyInfo::new(value_name, TypeId::STRING)]);
    assert_eq!(
        result, expected,
        "class application must reduce to the instance type produced by the construct signature"
    );
}
