//! Consumer-side regression matrix for on-demand lazy heritage-base member
//! resolution (issue #13935, the consumer half of #13933 / the #12101 campaign).
//!
//! ## Structural rule
//!
//! When an interface is represented as
//! `Intersection(own_members_object, Lazy(base_def), …)` — i.e. its heritage is
//! kept as deferred `Lazy(DefId)` (or `Application(Lazy(DefId), args)`) bases
//! instead of being flattened into a single `Object` — every consumer that
//! reads members must descend into those lazy bases on demand and apply the
//! base's type-parameter substitution, so inherited members resolve identically
//! to the flattened form. `tsc` likewise resolves base members lazily through
//! `getPropertiesOfType` / `getBaseTypes` rather than eagerly flattening.
//!
//! The three consumer paths this pins (all solver-owned):
//! - `objects::collect::collect_properties` — full property collection.
//! - `operations::property::PropertyAccessEvaluator` — single-property access.
//! - `relations::subtype::SubtypeChecker` — structural relations, both
//!   directions (lazy-heritage type as source and as target).
//!
//! The lazy-heritage representation modelled here is exactly what #13933 will
//! make the producer (`merge_lib_interface_heritage`) emit. These tests are
//! written against synthetic `DefId`s and a `TypeEnvironment` resolver so they
//! exercise the consumer in isolation, with no dependency on the producer flip
//! landing first ("land this first" — #13935). They guard against a future
//! producer change silently regressing inherited-member resolution and against
//! the resolution-mode → type-identity hazard family tracked in #13980.
//!
//! Binder-name independence: each case varies the synthetic `DefId`s and the
//! property/type-parameter names so no result depends on a particular interned
//! name (per the anti-hardcoding contract).

use crate::construction::TypeInterner;
use crate::def::resolver::TypeEnvironment;
use crate::def::{DefId, DefKind};
use crate::objects::collect::{PropertyCollectionResult, collect_properties};
use crate::operations::property::PropertyAccessEvaluator;
use crate::relations::subtype::SubtypeChecker;
use crate::types::{PropertyInfo, TypeData, TypeId, TypeParamInfo};

/// Build an object type from `(name, type)` fields.
fn object(interner: &TypeInterner, fields: &[(&str, TypeId)]) -> TypeId {
    interner.object(
        fields
            .iter()
            .map(|(name, ty)| PropertyInfo::new(interner.intern_string(name), *ty))
            .collect(),
    )
}

/// Register an interface `DefId` resolving to `body`.
fn declare_interface(env: &mut TypeEnvironment, def: DefId, body: TypeId) {
    env.insert_def(def, body);
    env.insert_def_kind(def, DefKind::Interface);
}

/// Register a generic interface `DefId` with type parameters.
fn declare_generic_interface(
    env: &mut TypeEnvironment,
    def: DefId,
    body: TypeId,
    params: Vec<TypeParamInfo>,
) {
    env.insert_def_with_params(def, body, params);
    env.insert_def_kind(def, DefKind::Interface);
}

/// A fresh, user-origin type parameter named `name`.
fn type_param(interner: &TypeInterner, name: &str) -> (TypeParamInfo, TypeId) {
    let info = TypeParamInfo {
        name: interner.intern_string(name),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let ty = interner.intern(TypeData::TypeParameter(info));
    (info, ty)
}

/// Sorted property names of a collection result (empty for `Any`/`NonObject`).
fn collected_names(result: &PropertyCollectionResult, interner: &TypeInterner) -> Vec<String> {
    let mut names = match result {
        PropertyCollectionResult::Properties { properties, .. } => properties
            .iter()
            .map(|p| interner.resolve_atom(p.name))
            .collect(),
        _ => Vec::new(),
    };
    names.sort();
    names
}

/// Type of a collected property by name, if present.
fn collected_type(
    result: &PropertyCollectionResult,
    interner: &TypeInterner,
    name: &str,
) -> Option<TypeId> {
    let interned = interner.intern_string(name);
    match result {
        PropertyCollectionResult::Properties { properties, .. } => properties
            .iter()
            .find(|p| p.name == interned)
            .map(|p| p.type_id),
        _ => None,
    }
}

// ----------------------------------------------------------------------------
// collect_properties
// ----------------------------------------------------------------------------

#[test]
fn collect_resolves_bare_lazy_heritage_base() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let base = DefId(31_001);
    declare_interface(
        &mut env,
        base,
        object(&interner, &[("inherited", TypeId::STRING)]),
    );

    // Derived = { own: number } & Lazy(base)
    let derived = interner.intersection2(
        object(&interner, &[("own", TypeId::NUMBER)]),
        interner.lazy(base),
    );

    let result = collect_properties(derived, &interner, &env);
    assert_eq!(
        collected_names(&result, &interner),
        vec!["inherited".to_string(), "own".to_string()],
        "own and inherited members must both be collected from a lazy heritage base",
    );
    assert_eq!(
        collected_type(&result, &interner, "inherited"),
        Some(TypeId::STRING)
    );
}

#[test]
fn collect_substitutes_application_lazy_heritage_base() {
    // Generic base instantiated as a heritage member must substitute its type
    // parameter into inherited members (#13652: do not leak the bare param).
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let (param, param_ty) = type_param(&interner, "Element");
    let base = DefId(31_002);
    declare_generic_interface(
        &mut env,
        base,
        object(&interner, &[("value", param_ty)]),
        vec![param],
    );

    // Derived = { own: number } & Box<number>
    let derived = interner.intersection2(
        object(&interner, &[("own", TypeId::NUMBER)]),
        interner.application(interner.lazy(base), vec![TypeId::NUMBER]),
    );

    let result = collect_properties(derived, &interner, &env);
    assert_eq!(
        collected_names(&result, &interner),
        vec!["own".to_string(), "value".to_string()],
    );
    assert_eq!(
        collected_type(&result, &interner, "value"),
        Some(TypeId::NUMBER),
        "inherited member must use the substituted base type argument, not the bare param",
    );
}

#[test]
fn collect_descends_transitive_lazy_heritage_chain() {
    // own -> Lazy(mid) -> Lazy(base): the descent must be transitive.
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let base = DefId(31_003);
    let mid = DefId(31_004);
    declare_interface(
        &mut env,
        base,
        object(&interner, &[("grand", TypeId::STRING)]),
    );
    let mid_body = interner.intersection2(
        object(&interner, &[("middle", TypeId::BOOLEAN)]),
        interner.lazy(base),
    );
    declare_interface(&mut env, mid, mid_body);

    let derived = interner.intersection2(
        object(&interner, &[("own", TypeId::NUMBER)]),
        interner.lazy(mid),
    );

    let result = collect_properties(derived, &interner, &env);
    assert_eq!(
        collected_names(&result, &interner),
        vec!["grand".to_string(), "middle".to_string(), "own".to_string()],
        "members from every level of a transitive lazy heritage chain must be collected",
    );
}

#[test]
fn collect_terminates_on_cyclic_lazy_heritage() {
    // Self-referential heritage (DOM-style): base body extends itself. The
    // collector must terminate via its cycle guard and still surface own members.
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let base = DefId(31_005);
    let base_body = interner.intersection2(
        object(&interner, &[("self_member", TypeId::STRING)]),
        interner.lazy(base),
    );
    declare_interface(&mut env, base, base_body);

    let derived = interner.intersection2(
        object(&interner, &[("own", TypeId::NUMBER)]),
        interner.lazy(base),
    );

    let result = collect_properties(derived, &interner, &env);
    assert_eq!(
        collected_names(&result, &interner),
        vec!["own".to_string(), "self_member".to_string()],
        "cyclic lazy heritage must terminate without dropping reachable members",
    );
}

// ----------------------------------------------------------------------------
// single-property access
// ----------------------------------------------------------------------------

#[test]
fn property_access_finds_inherited_member_through_lazy_base() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let base = DefId(31_006);
    declare_interface(
        &mut env,
        base,
        object(&interner, &[("inherited", TypeId::STRING)]),
    );
    let derived = interner.intersection2(
        object(&interner, &[("own", TypeId::NUMBER)]),
        interner.lazy(base),
    );

    let evaluator = PropertyAccessEvaluator::with_resolver(&interner, &env);
    assert_eq!(
        evaluator
            .resolve_property_access_atom(derived, interner.intern_string("inherited"))
            .success_type(),
        Some(TypeId::STRING),
        "single-property access must descend into a lazy heritage base",
    );
    assert_eq!(
        evaluator
            .resolve_property_access_atom(derived, interner.intern_string("own"))
            .success_type(),
        Some(TypeId::NUMBER),
    );
}

#[test]
fn property_access_through_lazy_wrapped_interface() {
    // Production shape: the derived interface itself is `Lazy(def)` whose body is
    // the `own & Lazy(base)` intersection — access must unwrap then descend.
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let base = DefId(31_007);
    let derived_def = DefId(31_008);
    declare_interface(
        &mut env,
        base,
        object(&interner, &[("inherited", TypeId::STRING)]),
    );
    let derived_body = interner.intersection2(
        object(&interner, &[("own", TypeId::NUMBER)]),
        interner.lazy(base),
    );
    declare_interface(&mut env, derived_def, derived_body);

    let derived = interner.lazy(derived_def);
    let evaluator = PropertyAccessEvaluator::with_resolver(&interner, &env);
    assert_eq!(
        evaluator
            .resolve_property_access_atom(derived, interner.intern_string("inherited"))
            .success_type(),
        Some(TypeId::STRING),
    );
    assert_eq!(
        evaluator
            .resolve_property_access_atom(derived, interner.intern_string("own"))
            .success_type(),
        Some(TypeId::NUMBER),
    );
}

// ----------------------------------------------------------------------------
// structural relations
// ----------------------------------------------------------------------------

#[test]
fn subtype_lazy_heritage_source_matches_flattened_form() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let base = DefId(31_009);
    declare_interface(
        &mut env,
        base,
        object(&interner, &[("inherited", TypeId::STRING)]),
    );
    let lazy_form = interner.intersection2(
        object(&interner, &[("own", TypeId::NUMBER)]),
        interner.lazy(base),
    );
    let flattened_form = object(
        &interner,
        &[("own", TypeId::NUMBER), ("inherited", TypeId::STRING)],
    );

    let target = object(
        &interner,
        &[("own", TypeId::NUMBER), ("inherited", TypeId::STRING)],
    );
    let mismatch = object(
        &interner,
        &[("own", TypeId::NUMBER), ("inherited", TypeId::NUMBER)],
    );

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    assert!(
        checker.check_subtype(flattened_form, target).is_true(),
        "control: flattened form relates",
    );
    assert!(
        checker.check_subtype(lazy_form, target).is_true(),
        "lazy heritage source must relate identically to the flattened form (positive)",
    );
    assert!(
        !checker.check_subtype(lazy_form, mismatch).is_true(),
        "a real inherited-member type mismatch must still be rejected (negative)",
    );
}

#[test]
fn subtype_enforces_inherited_member_on_lazy_heritage_target() {
    // Lazy heritage type as the TARGET: a source missing an inherited member
    // must be rejected; one providing it must be accepted.
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let base = DefId(31_010);
    declare_interface(
        &mut env,
        base,
        object(&interner, &[("inherited", TypeId::STRING)]),
    );
    let target = interner.intersection2(
        object(&interner, &[("own", TypeId::NUMBER)]),
        interner.lazy(base),
    );

    let complete = object(
        &interner,
        &[("own", TypeId::NUMBER), ("inherited", TypeId::STRING)],
    );
    let missing_inherited = object(&interner, &[("own", TypeId::NUMBER)]);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    assert!(
        checker.check_subtype(complete, target).is_true(),
        "source satisfying the inherited requirement must be accepted",
    );
    assert!(
        !checker.check_subtype(missing_inherited, target).is_true(),
        "source missing the inherited member must be rejected",
    );
}

#[test]
fn subtype_substitutes_generic_base_through_chain() {
    // Generic type argument must flow through a generic base used as heritage:
    // Iter<T> = { cur: T } & Coll<T>;  Coll<U> = { items: U }.
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let (coll_param, coll_param_ty) = type_param(&interner, "U");
    let coll = DefId(31_011);
    declare_generic_interface(
        &mut env,
        coll,
        object(&interner, &[("items", coll_param_ty)]),
        vec![coll_param],
    );

    let (iter_param, iter_param_ty) = type_param(&interner, "T");
    let iter = DefId(31_012);
    let iter_body = interner.intersection2(
        object(&interner, &[("cur", iter_param_ty)]),
        interner.application(interner.lazy(coll), vec![iter_param_ty]),
    );
    declare_generic_interface(&mut env, iter, iter_body, vec![iter_param]);

    let derived = interner.application(interner.lazy(iter), vec![TypeId::NUMBER]);

    let result = collect_properties(derived, &interner, &env);
    assert_eq!(
        collected_type(&result, &interner, "items"),
        Some(TypeId::NUMBER),
        "the type argument must substitute through a generic heritage base",
    );
    assert_eq!(
        collected_type(&result, &interner, "cur"),
        Some(TypeId::NUMBER)
    );

    let target_ok = object(
        &interner,
        &[("cur", TypeId::NUMBER), ("items", TypeId::NUMBER)],
    );
    let target_bad = object(
        &interner,
        &[("cur", TypeId::NUMBER), ("items", TypeId::STRING)],
    );
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    assert!(
        checker.check_subtype(derived, target_ok).is_true(),
        "structural match through a generic heritage chain (positive)",
    );
    assert!(
        !checker.check_subtype(derived, target_bad).is_true(),
        "mismatch through a generic heritage chain (negative)",
    );
}
