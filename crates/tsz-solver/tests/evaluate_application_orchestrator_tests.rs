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
use crate::evaluation::request::EvaluationRequest;
use crate::evaluation::result::{Termination, TerminationKind};
use crate::relations::subtype::TypeEnvironment;

fn unconstrained_param(interner: &TypeInterner, name: &str) -> TypeParamInfo {
    TypeParamInfo {
        name: interner.intern_string(name),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
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

/// Phase 5 — GENUINE `unknown` body. When the alias body is genuinely
/// registered as `unknown` (`type C<T> = unknown`, or a utility alias that
/// reduces to `unknown`), the application MUST reduce to the canonical
/// `unknown` `TypeId`. Keeping it opaque mints an identity-distinct
/// `Application` that the relation layer cannot recognize as `unknown`,
/// producing a false `unknown` ≠ `unknown` / `unknown` ≰ `C<...>`
/// (TS2719 / TS2322) in member position (issue #13212).
#[test]
fn evaluate_application_genuine_unknown_body_reduces_to_canonical_unknown() {
    let interner = TypeInterner::new();
    let t_param = unconstrained_param(&interner, "T");

    let mut env = TypeEnvironment::new();
    // A registered `unknown` body — `get_def_raw_body` sees it, so it is the
    // genuine case, not a registration-window placeholder.
    let app = alias_application(
        &interner,
        &mut env,
        DefId(202),
        DefKind::TypeAlias,
        TypeId::UNKNOWN,
        vec![t_param],
        vec![TypeId::STRING],
    );

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(app);

    assert_eq!(
        result,
        TypeId::UNKNOWN,
        "a genuine `unknown` alias body must reduce `C<Args>` to canonical `unknown`"
    );
}

/// A resolver that reports a `DefId` as a generic type alias whose body
/// resolves to `unknown` (via `resolve_lazy`) but has **no** body registered
/// at alias-registration time (`get_def_raw_body` is `None`). This is the
/// cross-file registration-window race: the declaring file has not published
/// the alias body to the shared `DefinitionStore` yet, and the consuming
/// file's `unknown` is an unresolved symbol-type fallback, not a finalized
/// body.
struct PlaceholderUnknownResolver {
    def_id: DefId,
    params: Vec<TypeParamInfo>,
    name: tsz_common::interner::Atom,
}

impl TypeResolver for PlaceholderUnknownResolver {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn resolve_lazy(&self, def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        (def_id == self.def_id).then_some(TypeId::UNKNOWN)
    }

    fn get_lazy_type_params(&self, def_id: DefId) -> Option<Vec<TypeParamInfo>> {
        (def_id == self.def_id).then(|| self.params.clone())
    }

    fn get_def_kind(&self, def_id: DefId) -> Option<DefKind> {
        (def_id == self.def_id).then_some(DefKind::TypeAlias)
    }

    fn get_def_name(&self, def_id: DefId) -> Option<tsz_common::interner::Atom> {
        (def_id == self.def_id).then_some(self.name)
    }

    // No registered body: `get_def_raw_body` keeps the default `None`, marking
    // this as a registration-window placeholder rather than a genuine body.
}

/// Phase 5 — PLACEHOLDER `unknown`. When `resolve_lazy` yields `unknown` but
/// no body was registered at alias-registration time (`get_def_raw_body` is
/// `None`), the `unknown` is a cross-file race placeholder. The orchestrator
/// must keep the `Application` opaque so a later pass with the populated body
/// can expand it — never collapse `C<Args>` to bare `unknown`.
#[test]
fn evaluate_application_placeholder_unknown_body_stays_opaque() {
    let interner = TypeInterner::new();
    let t_param = unconstrained_param(&interner, "T");

    let resolver = PlaceholderUnknownResolver {
        def_id: DefId(303),
        params: vec![t_param],
        name: interner.intern_string("C"),
    };
    let app = interner.application(interner.lazy(DefId(303)), vec![TypeId::STRING]);

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &resolver);
    let result = evaluator.evaluate(app);

    assert_eq!(
        result, app,
        "a placeholder `unknown` (no registered body) must keep `C<Args>` opaque"
    );
}

/// Type-argument deferral taint (#14347). When an application's base resolves
/// but a type *argument* is a `Lazy(DefId)` whose body is not registered on this
/// query (a cross-file alias whose declaring file has not published it yet), the
/// argument stays opaque — and the evaluator must record `unresolved_def_seen`
/// so the enclosing application's under-expanded result is kept out of the
/// `TypeId`-keyed evaluation caches. This is the argument-side mirror of the
/// application-base deferrals in `evaluate/application.rs`.
#[test]
fn evaluate_application_unresolved_arg_alias_taints_unresolved_def_seen() {
    let interner = TypeInterner::new();
    let t_param = unconstrained_param(&interner, "T");
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let value_name = interner.intern_string("value");
    let body = interner.object(vec![PropertyInfo::new(value_name, t_type)]);

    // `Box<Unregistered>` where `Unregistered`'s body is absent on this query.
    let unresolved_arg = interner.lazy(DefId(909));
    let mut env = TypeEnvironment::new();
    let app = alias_application(
        &interner,
        &mut env,
        DefId(101),
        DefKind::TypeAlias,
        body,
        vec![t_param],
        vec![unresolved_arg],
    );

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let _ = evaluator.evaluate(app);

    assert!(
        evaluator.is_unresolved_def_seen(),
        "an application argument whose alias body is unresolved must taint \
         `unresolved_def_seen` so the result is not cached as authoritative"
    );
}

/// Negative control for #14347: a type argument whose alias body *is* registered
/// expands cleanly and must NOT taint `unresolved_def_seen`. This pins the taint
/// to genuine registration-window deferrals, never a steady-state expansion.
#[test]
fn evaluate_application_resolved_arg_alias_does_not_taint() {
    let interner = TypeInterner::new();
    let t_param = unconstrained_param(&interner, "T");
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let value_name = interner.intern_string("value");
    let body = interner.object(vec![PropertyInfo::new(value_name, t_type)]);

    let mut env = TypeEnvironment::new();
    // `type StringAlias = string` — a fully registered, resolvable alias arg.
    env.insert_def_with_params(DefId(808), TypeId::STRING, vec![]);
    env.insert_def_kind(DefId(808), DefKind::TypeAlias);
    let resolved_arg = interner.lazy(DefId(808));

    let app = alias_application(
        &interner,
        &mut env,
        DefId(101),
        DefKind::TypeAlias,
        body,
        vec![t_param],
        vec![resolved_arg],
    );

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(app);

    assert!(
        !evaluator.is_unresolved_def_seen(),
        "a resolvable argument alias must not taint `unresolved_def_seen`"
    );
    let expected = interner.object(vec![PropertyInfo::new(value_name, TypeId::STRING)]);
    assert_eq!(
        result, expected,
        "Box<StringAlias> must expand to {{ value: string }}"
    );
}

fn tuple_elem(type_id: TypeId) -> TupleElement {
    TupleElement {
        type_id,
        name: None,
        optional: false,
        rest: false,
    }
}

fn rest_tuple_elem(type_id: TypeId) -> TupleElement {
    TupleElement {
        type_id,
        name: None,
        optional: false,
        rest: true,
    }
}

#[test]
fn evaluate_application_variadic_prepend_flattens_tail_application() {
    let interner = TypeInterner::new();

    let head_param = unconstrained_param(&interner, "Head");
    let tail_param = unconstrained_param(&interner, "Tail");
    let source_param = unconstrained_param(&interner, "Source");
    let ignored_head = interner.intern(TypeData::Infer(unconstrained_param(&interner, "Ignored")));
    let rest = interner.intern(TypeData::Infer(unconstrained_param(&interner, "Rest")));

    let head_type = interner.intern(TypeData::TypeParameter(head_param));
    let tail_type = interner.intern(TypeData::TypeParameter(tail_param));
    let source_type = interner.intern(TypeData::TypeParameter(source_param));

    let prepend_body = interner.tuple(vec![tuple_elem(head_type), rest_tuple_elem(tail_type)]);
    let tail_body = interner.conditional(ConditionalType {
        check_type: source_type,
        extends_type: interner.tuple(vec![tuple_elem(ignored_head), rest_tuple_elem(rest)]),
        true_type: rest,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });

    let mut env = TypeEnvironment::new();
    let tail_source = interner.tuple(vec![
        tuple_elem(TypeId::NUMBER),
        tuple_elem(TypeId::BOOLEAN),
    ]);
    let tail_app = alias_application(
        &interner,
        &mut env,
        DefId(301),
        DefKind::TypeAlias,
        tail_body,
        vec![source_param],
        vec![tail_source],
    );
    let prepend_app = alias_application(
        &interner,
        &mut env,
        DefId(302),
        DefKind::TypeAlias,
        prepend_body,
        vec![head_param, tail_param],
        vec![TypeId::STRING, tail_app],
    );

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(prepend_app);
    // `Tail<[number, boolean]> = [number, boolean] extends [infer _, ...infer R]
    // ? R : never` drops the head, yielding `[boolean]`, so
    // `Prepend<string, Tail<...>> = [string, ...[boolean]] = [string, boolean]`.
    // The flattened spread inlines the resolved tail application exactly,
    // matching `tsc`'s `createNormalizedTupleType`.
    let expected = interner.tuple(vec![
        tuple_elem(TypeId::STRING),
        tuple_elem(TypeId::BOOLEAN),
    ]);

    assert_eq!(
        result, expected,
        "Prepend<Head, Tail<Source>> must inline the resolved tail application's exact arity"
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

/// Phase 6 — dropped-nominal-symbol guard (issue #16055). A class whose
/// declared constructor body already carries a nominal `symbol` describes a
/// real, named class declaration. If evaluating that declaration's
/// construct-signature return type yields a structural object that no
/// longer carries that symbol, the result is a degraded, partially built
/// instance body (e.g. a circular import forcing resolution before the
/// class publishes its final type) rather than the class's true instance
/// type, so the application must stay opaque instead of surfacing it.
#[test]
fn evaluate_application_class_result_dropping_declared_nominal_symbol_stays_opaque() {
    let interner = TypeInterner::new();
    let class_symbol = tsz_binder::SymbolId(4050);

    let value_name = interner.intern_string("value");
    // The construct signature's return type is a structural object that has
    // LOST the class's nominal symbol — the signature of a partial body
    // materialized before the class finished building.
    let degraded_instance = interner.object_with_flags_and_symbol(
        vec![PropertyInfo::new(value_name, TypeId::STRING)],
        crate::types::ObjectFlags::empty(),
        None,
    );

    let construct_sig = CallSignature {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: degraded_instance,
        type_predicate: None,
        is_method: false,
    };
    let class_body = interner.callable(CallableShape {
        symbol: Some(class_symbol),
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
        DefId(405),
        DefKind::Class,
        class_body,
        vec![],
        vec![],
    );

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(app);

    assert_eq!(
        result, app,
        "a class application whose declared body carries a nominal symbol \
         must stay opaque when the evaluated instance dropped it, instead \
         of surfacing the degraded structural object"
    );
}

/// Same trigger as above with a renamed binder (`Widget`/`data` in spirit —
/// distinct `DefId`, symbol id, and property name) to prove the guard keys
/// off structure (symbol presence/absence), not a specific identifier.
#[test]
fn evaluate_application_class_result_dropping_declared_nominal_symbol_stays_opaque_renamed() {
    let interner = TypeInterner::new();
    let class_symbol = tsz_binder::SymbolId(7070);

    let payload_name = interner.intern_string("payload");
    let degraded_instance = interner.object_with_flags_and_symbol(
        vec![PropertyInfo::new(payload_name, TypeId::NUMBER)],
        crate::types::ObjectFlags::empty(),
        None,
    );

    let construct_sig = CallSignature {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: degraded_instance,
        type_predicate: None,
        is_method: false,
    };
    let class_body = interner.callable(CallableShape {
        symbol: Some(class_symbol),
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
        DefId(406),
        DefKind::Class,
        class_body,
        vec![],
        vec![],
    );

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(app);

    assert_eq!(result, app, "renamed-binder variant must also stay opaque");
}

/// Negative/fallback case: when the evaluated instance KEEPS the class's
/// nominal symbol, the guard must not fire — the result is a genuine,
/// complete instance and must be returned normally, not held opaque.
/// Mirrors `evaluate_application_class_uses_construct_signature_return_type`
/// (generic substitution via a real type parameter) with a `symbol` added to
/// both the class body and the instance shape, and inspects the resolved
/// shape directly rather than assuming a specific post-instantiation
/// `TypeId` — only the substitution + `evaluate_application` orchestration
/// decide that identity, not this guard.
#[test]
fn evaluate_application_class_result_keeping_declared_nominal_symbol_resolves_normally() {
    let interner = TypeInterner::new();
    let class_symbol = tsz_binder::SymbolId(4051);
    let t_param = unconstrained_param(&interner, "T");
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let value_name = interner.intern_string("value");
    let instance_shape = interner.object_with_flags_and_symbol(
        vec![PropertyInfo::new(value_name, t_type)],
        crate::types::ObjectFlags::empty(),
        Some(class_symbol),
    );

    let construct_sig = CallSignature {
        type_params: vec![],
        params: vec![ParamInfo::required(value_name, t_type)],
        this_type: None,
        return_type: instance_shape,
        type_predicate: None,
        is_method: false,
    };
    let class_body = interner.callable(CallableShape {
        symbol: Some(class_symbol),
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
        DefId(407),
        DefKind::Class,
        class_body,
        vec![t_param],
        vec![TypeId::STRING],
    );

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate(app);

    assert_ne!(
        result, app,
        "a class application whose evaluated instance keeps the declared \
         nominal symbol must resolve normally, not be held opaque"
    );
    let (properties, symbol) = match interner.lookup(result) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            (shape.properties.clone(), shape.symbol)
        }
        Some(TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            (shape.properties.clone(), shape.symbol)
        }
        other => panic!("expected a resolved instance object, got {other:?}"),
    };
    assert_eq!(
        symbol,
        Some(class_symbol),
        "the substituted instance must keep the class's nominal symbol"
    );
    assert_eq!(properties.len(), 1);
    assert_eq!(properties[0].name, value_name);
    assert_eq!(
        properties[0].type_id,
        TypeId::STRING,
        "T must substitute to string in the instantiated instance"
    );
}

/// Phase 4 — authoritative application-eval cache read.
///
/// The per-file application-eval cache lives on the `QueryCache`. Evaluators
/// only consume it when handed an explicit `query_db`; resolver-less evaluators
/// must not read it through the interner alone because the key does not encode
/// resolver strength, and limited/noop resolvers need their normal opaque
/// fallback behavior for recursive and inference parity.
///
/// Structural axis (CLAUDE.md §25/§26): the rule is keyed on the `(DefId, args)`
/// identity, not on a spelling, so the test varies both the def id and the
/// argument type to prove the lookup and the `query_db` gate are structural.
#[test]
fn evaluate_application_reads_cache_only_with_query_db() {
    use crate::caches::db::TypeApplicationEvalCache;
    use crate::caches::query_cache::QueryCache;

    for (def_raw, arg, cached) in [
        (501u32, TypeId::STRING, TypeId::NUMBER),
        (777u32, TypeId::BOOLEAN, TypeId::STRING),
    ] {
        let interner = TypeInterner::new();
        let def_id = DefId(def_raw);
        let app = interner.application(interner.lazy(def_id), vec![arg]);

        // A `NoopResolver` evaluator cannot resolve the alias body, so without a
        // cache entry the application stays opaque.
        {
            let qc = QueryCache::new(&interner);
            let mut evaluator = TypeEvaluator::new(&qc);
            assert_eq!(
                evaluator.evaluate(app),
                app,
                "without a cache entry the unresolvable application must stay opaque"
            );
        }

        // With an authoritative entry seeded on the per-file cache, the same
        // resolver-less evaluator still must not read through its interner
        // alone.
        {
            let qc = QueryCache::new(&interner);
            qc.insert_application_eval_cache(def_id, &[arg], false, cached);
            let mut evaluator = TypeEvaluator::new(&qc);
            assert_eq!(
                evaluator.evaluate(app),
                app,
                "evaluator without query_db must preserve the opaque fallback"
            );
        }

        // The cache is reusable once the evaluator is connected to an explicit
        // query database.
        {
            let qc = QueryCache::new(&interner);
            qc.insert_application_eval_cache(def_id, &[arg], false, cached);
            let mut evaluator = TypeEvaluator::new(&interner).with_query_db(&qc);
            assert_eq!(
                evaluator.evaluate(app),
                cached,
                "evaluator with query_db should read the per-file app-eval cache"
            );
        }
    }
}

/// Phase 5 — application-eval cache WRITE happens for a converging alias.
///
/// A normal, terminating alias application (`Box<T> = { value: T }`) must
/// populate the per-file `application_eval_cache` so repeat use sites reuse the
/// memoized expansion. This is the positive control for the limit-gated write
/// below: the gate must not suppress healthy memoization.
#[test]
fn evaluate_application_caches_converging_alias_result() {
    use crate::caches::db::TypeApplicationEvalCache;
    use crate::caches::query_cache::QueryCache;

    // Vary the def id and the type-parameter spelling so the rule is pinned to
    // the structural `(DefId, args)` identity, not a name.
    for (def_raw, param_name) in [(611u32, "T"), (733u32, "Element")] {
        let interner = TypeInterner::new();
        let def_id = DefId(def_raw);
        let t_param = unconstrained_param(&interner, param_name);
        let t_type = interner.intern(TypeData::TypeParameter(t_param));
        let value_name = interner.intern_string("value");
        let body = interner.object(vec![PropertyInfo::new(value_name, t_type)]);

        let mut env = TypeEnvironment::new();
        let app = alias_application(
            &interner,
            &mut env,
            def_id,
            DefKind::TypeAlias,
            body,
            vec![t_param],
            vec![TypeId::STRING],
        );

        let qc = QueryCache::new(&interner);
        let mut evaluator = TypeEvaluator::with_resolver(&interner, &env).with_query_db(&qc);
        let result = evaluator.evaluate(app);

        let expected = interner.object(vec![PropertyInfo::new(value_name, TypeId::STRING)]);
        assert_eq!(
            result, expected,
            "Box<string> must expand to {{ value: string }}"
        );
        assert_eq!(
            qc.lookup_application_eval_cache(def_id, &[TypeId::STRING], false),
            Some(expected),
            "a converging alias application must populate the application-eval cache"
        );
    }
}

#[test]
fn evaluate_application_finalization_defers_seeded_fifth_identity_without_cache_write() {
    use crate::caches::db::TypeApplicationEvalCache;
    use crate::caches::query_cache::QueryCache;

    let interner = TypeInterner::new();
    let def_id = DefId(143_517);
    let t_param = unconstrained_param(&interner, "Source");
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let value_name = interner.intern_string("value");
    let body = interner.object(vec![PropertyInfo::new(value_name, t_type)]);

    let mut env = TypeEnvironment::new();
    let app = alias_application(
        &interner,
        &mut env,
        def_id,
        DefKind::TypeAlias,
        body,
        vec![t_param],
        vec![TypeId::STRING],
    );
    let expected = interner.object(vec![PropertyInfo::new(value_name, TypeId::STRING)]);

    let qc = QueryCache::new(&interner);
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env).with_query_db(&qc);
    evaluator.seed_meta_rereduce_recursion_identity_for_test(app, 4);
    let result = evaluator.evaluate(app);

    assert_eq!(
        result, app,
        "fifth same-root application finalization should preserve the deferred Application"
    );
    assert_ne!(
        result, expected,
        "identity cutoff must prevent another eager structural application expansion"
    );
    assert!(
        evaluator.has_incomplete_request_verdict(),
        "application identity bailout must mark the request partial"
    );
    assert_eq!(
        qc.lookup_application_eval_cache(def_id, &[TypeId::STRING], false),
        None,
        "a recursion-identity bailout must not enter the application-eval cache"
    );
}

#[test]
fn evaluate_application_finalization_allows_seeded_fourth_identity_and_caches() {
    use crate::caches::db::TypeApplicationEvalCache;
    use crate::caches::query_cache::QueryCache;

    let interner = TypeInterner::new();
    let def_id = DefId(143_518);
    let t_param = unconstrained_param(&interner, "Element");
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let value_name = interner.intern_string("payload");
    let body = interner.object(vec![PropertyInfo::new(value_name, t_type)]);

    let mut env = TypeEnvironment::new();
    let app = alias_application(
        &interner,
        &mut env,
        def_id,
        DefKind::TypeAlias,
        body,
        vec![t_param],
        vec![TypeId::NUMBER],
    );
    let expected = interner.object(vec![PropertyInfo::new(value_name, TypeId::NUMBER)]);

    let qc = QueryCache::new(&interner);
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env).with_query_db(&qc);
    evaluator.seed_meta_rereduce_recursion_identity_for_test(app, 3);
    let result = evaluator.evaluate(app);

    assert_eq!(result, expected);
    assert!(
        !evaluator.has_incomplete_request_verdict(),
        "below-cutoff application finalization must not mark the request partial"
    );
    assert_eq!(
        qc.lookup_application_eval_cache(def_id, &[TypeId::NUMBER], false),
        Some(expected),
        "below-cutoff application finalization should still populate the cache"
    );
}

#[test]
fn evaluate_application_finalization_preserves_direct_same_application_growth_path_without_cache_write()
 {
    use crate::caches::db::TypeApplicationEvalCache;
    use crate::caches::query_cache::QueryCache;

    let interner = TypeInterner::new();
    let def_id = DefId(143_519);
    let t_param = unconstrained_param(&interner, "Node");
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let grown_arg = interner.array(t_type);
    let body = interner.application(interner.lazy(def_id), vec![grown_arg]);

    let mut env = TypeEnvironment::new();
    let app = alias_application(
        &interner,
        &mut env,
        def_id,
        DefKind::TypeAlias,
        body,
        vec![t_param],
        vec![TypeId::STRING],
    );

    let qc = QueryCache::new(&interner);
    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env).with_query_db(&qc);
    evaluator.seed_meta_rereduce_recursion_identity_for_test(app, 4);
    let result = evaluator.evaluate_request_result(EvaluationRequest::new(app));

    assert_eq!(
        result.into_type_id(),
        TypeId::ERROR,
        "direct same-alias argument growth must keep using the depth/divergence path"
    );
    assert_eq!(
        result.termination(),
        Termination::Incomplete {
            kind: TerminationKind::DepthExceeded,
            partial: TypeId::ERROR,
        }
    );
    assert_eq!(
        qc.lookup_application_eval_cache(def_id, &[TypeId::STRING], false),
        None,
        "direct same-alias argument growth bails must not be cached"
    );
}

/// Phase 5 — a depth-bounded (divergent) alias application must NOT poison the
/// per-file `application_eval_cache`.
///
/// `Rec<T> = Rec<T[]>` grows its argument on every step and never terminates,
/// so evaluation bails out (TS2589-class depth/divergence) and leaves the
/// recursion guard exceeded. The bail result is a function of the *ambient
/// stack depth* at this use site, not of `(DefId, args)`. Persisting it would
/// poison every other use of the alias — the "alias fan-out regression" the
/// fix targets: a sibling `Rec<string>` evaluated on its own shallower stack
/// would converge differently and must never read back a stale bail artifact.
///
/// Structural axis (CLAUDE.md §25/§26): the rule is keyed on recursion state,
/// not a spelling, so the def id and the type-parameter name both vary.
#[test]
fn evaluate_application_divergent_alias_does_not_poison_cache() {
    use crate::caches::db::TypeApplicationEvalCache;
    use crate::caches::query_cache::QueryCache;

    for (def_raw, param_name, arg) in [
        (821u32, "T", TypeId::STRING),
        (947u32, "Item", TypeId::NUMBER),
    ] {
        let interner = TypeInterner::new();
        let def_id = DefId(def_raw);
        let t_param = unconstrained_param(&interner, param_name);
        let t_type = interner.intern(TypeData::TypeParameter(t_param));
        // Body: `Rec<T[]>` — the alias re-applies itself to an ever-growing
        // argument, so the recursion diverges and must bail.
        let grown_arg = interner.array(t_type);
        let body = interner.application(interner.lazy(def_id), vec![grown_arg]);

        let mut env = TypeEnvironment::new();
        let app = alias_application(
            &interner,
            &mut env,
            def_id,
            DefKind::TypeAlias,
            body,
            vec![t_param],
            vec![arg],
        );

        let qc = QueryCache::new(&interner);
        let mut evaluator = TypeEvaluator::with_resolver(&interner, &env).with_query_db(&qc);
        // Evaluation must terminate (the guards bound it) rather than hang.
        let _ = evaluator.evaluate(app);

        assert_eq!(
            qc.lookup_application_eval_cache(def_id, &[arg], false),
            None,
            "a depth-bounded alias bail must not be persisted under (DefId, args); \
             caching it would poison every sibling use of the alias"
        );
    }
}

#[test]
fn evaluate_application_divergent_alias_reports_incomplete_request_result() {
    let interner = TypeInterner::new();
    let def_id = DefId(1081);
    let t_param = unconstrained_param(&interner, "Item");
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let grown_arg = interner.array(t_type);
    let body = interner.application(interner.lazy(def_id), vec![grown_arg]);

    let mut env = TypeEnvironment::new();
    let app = alias_application(
        &interner,
        &mut env,
        def_id,
        DefKind::TypeAlias,
        body,
        vec![t_param],
        vec![TypeId::STRING],
    );

    let mut evaluator = TypeEvaluator::with_resolver(&interner, &env);
    let result = evaluator.evaluate_request_result(EvaluationRequest::new(app));

    assert_eq!(result.into_type_id(), TypeId::ERROR);
    assert_eq!(
        result.termination(),
        Termination::Incomplete {
            kind: TerminationKind::DepthExceeded,
            partial: TypeId::ERROR,
        },
        "application depth/divergence bails must surface through the typed request result"
    );
}

/// Phase 5 — an earlier, unrelated recursion bail must not disable caching for a
/// fully-converging alias evaluated afterwards on the same evaluator.
///
/// `deep_recursion_seen` / `silent_depth_bailed` are sticky for the evaluator's
/// lifetime, so the previous behavior — gating cache writes on
/// `!recursion_limit_hit()` — turned every later `application_eval_cache` write
/// off the moment the first unrelated alias bailed (a cycle / silent-depth
/// bail that leaves the evaluator usable, unlike the divergent
/// `guard.mark_exceeded` path covered by
/// `evaluate_application_divergent_alias_does_not_poison_cache`). A
/// schema-library shape such as TypeBox/zod `Static<TObject<…>>` re-instantiates
/// the *same* finite inner application across each intersection branch and
/// nesting level; with caching globally disabled after one unrelated bail it is
/// recomputed combinatorially, turning a terminating type into an effective hang
/// (#10834). Cacheability is now keyed on the per-application epoch: a converging
/// alias whose own body subtree fires no new limit event is still persisted.
///
/// Structural axis (CLAUDE.md §25/§26): keyed on recursion state, not a
/// spelling, so the def id and the type-parameter / field names both vary.
#[test]
fn evaluate_application_clean_alias_caches_after_unrelated_recursion_bail() {
    use crate::caches::db::TypeApplicationEvalCache;
    use crate::caches::query_cache::QueryCache;

    for (good_raw, good_param, good_field, good_arg) in [
        (832u32, "U", "value", TypeId::NUMBER),
        (954u32, "Elem", "payload", TypeId::STRING),
    ] {
        let interner = TypeInterner::new();
        let mut env = TypeEnvironment::new();

        // Converging alias `Good<U> = { <field>: U }` — its body fires no limit
        // event, so its result is a complete function of `(DefId, args)`.
        let good_def = DefId(good_raw);
        let good_param = unconstrained_param(&interner, good_param);
        let good_param_ty = interner.intern(TypeData::TypeParameter(good_param));
        let good_field_atom = interner.intern_string(good_field);
        let good_body = interner.object(vec![PropertyInfo::new(good_field_atom, good_param_ty)]);
        let good_app = alias_application(
            &interner,
            &mut env,
            good_def,
            DefKind::TypeAlias,
            good_body,
            vec![good_param],
            vec![good_arg],
        );
        let expected = interner.object(vec![PropertyInfo::new(good_field_atom, good_arg)]);

        let qc = QueryCache::new(&interner);
        let mut evaluator = TypeEvaluator::with_resolver(&interner, &env).with_query_db(&qc);

        // An earlier, unrelated alias bailed (a cycle / silent-depth bail),
        // latching the sticky recursion-limit state without poisoning the guard.
        evaluator.simulate_unrelated_recursion_bail_for_test();

        // The unrelated converging alias must still expand correctly AND be
        // persisted — the sticky bail flag must not disable its write.
        let result = evaluator.evaluate(good_app);
        assert_eq!(result, expected, "Good<arg> must expand structurally");
        assert_eq!(
            qc.lookup_application_eval_cache(good_def, &[good_arg], false),
            Some(expected),
            "a fully-converging alias must still populate the cache even after an \
             unrelated alias bailed earlier on the same evaluator (#10834)"
        );
    }
}
