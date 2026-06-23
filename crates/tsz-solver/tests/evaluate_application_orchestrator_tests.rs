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
