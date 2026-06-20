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

use crate::classes::inheritance::InheritanceGraph;
use crate::construction::TypeInterner;
use crate::def::resolver::TypeEnvironment;
use crate::def::{DefId, DefKind};
use crate::objects::collect::{PropertyCollectionResult, collect_properties};
use crate::operations::property::PropertyAccessEvaluator;
use crate::relations::subtype::SubtypeChecker;
use crate::types::{PropertyInfo, SymbolRef, TypeData, TypeId, TypeParamInfo};
use tsz_binder::SymbolId;

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

// ----------------------------------------------------------------------------
// Nominal heritage subtype short-circuit
// ----------------------------------------------------------------------------
//
// When the heritage representation keeps the bases as bare `Lazy(DefId)`
// references registered in the `InheritanceGraph`, a `Lazy(Derived) <:
// Lazy(Base)` relation is settled by an O(1) nominal reachability bit *before*
// evaluation materializes the base's members. The shortcut is authoritative
// only for an edge whose verdict cannot depend on type arguments threading into
// the target's members: both ends are classes, or the target is non-generic.
// A generic target falls through to the structural check, which honors variance.
// These cases vary the synthetic `DefId`/`SymbolId` numbers and member names so
// no result depends on a particular interned name.

/// Register a `def -> sym` bridge so a `Lazy(def)` maps onto a graph node.
fn bridge(env: &mut TypeEnvironment, def: DefId, sym: SymbolId) {
    env.register_def_symbol_mapping(def, sym);
}

/// A non-generic derived interface resolves `<:` its base purely through the
/// nominal heritage edge — even when the base body is *unresolvable* (never
/// materialized). Without the graph the same relation falls back to a structural
/// check that cannot resolve the base and therefore fails, proving the verdict
/// came from on-demand nominal descent rather than from materialized members.
#[test]
fn subtype_nominal_heritage_resolves_without_materializing_base() {
    let interner = TypeInterner::new();
    let base_def = DefId(33_001);
    let derived_def = DefId(33_002);
    let base_sym = SymbolId(34_001);
    let derived_sym = SymbolId(34_002);

    let mut env = TypeEnvironment::new();
    // Derived has a body; the base deliberately has none (unresolvable), so a
    // structural attempt to descend into the base must fail.
    declare_interface(
        &mut env,
        derived_def,
        object(&interner, &[("p", TypeId::NUMBER)]),
    );
    bridge(&mut env, base_def, base_sym);
    bridge(&mut env, derived_def, derived_sym);

    let graph = InheritanceGraph::new();
    graph.add_inheritance(derived_sym, &[base_sym]);

    let source = interner.lazy(derived_def);
    let target = interner.lazy(base_def);

    let mut with_graph =
        SubtypeChecker::with_resolver(&interner, &env).with_inheritance_graph(&graph);
    assert!(
        with_graph.check_subtype(source, target).is_true(),
        "a non-generic derived interface must be a subtype of its base via the \
         nominal heritage edge, without materializing the (unresolvable) base",
    );

    let mut without_graph = SubtypeChecker::with_resolver(&interner, &env);
    assert!(
        !without_graph.check_subtype(source, target).is_true(),
        "without the inheritance graph there is no on-demand heritage descent, \
         and the unresolvable base makes the structural check fail — confirming \
         the positive verdict above came from the nominal short-circuit",
    );
}

/// The nominal heritage relation is directional: a derived interface is a
/// subtype of its base, but the base is not a subtype of the derived (it lacks
/// the derived's extra members).
#[test]
fn subtype_nominal_heritage_is_directional() {
    let interner = TypeInterner::new();
    let base_def = DefId(33_011);
    let derived_def = DefId(33_012);
    let base_sym = SymbolId(34_011);
    let derived_sym = SymbolId(34_012);

    let mut env = TypeEnvironment::new();
    declare_interface(
        &mut env,
        base_def,
        object(&interner, &[("a", TypeId::NUMBER)]),
    );
    declare_interface(
        &mut env,
        derived_def,
        object(&interner, &[("a", TypeId::NUMBER), ("b", TypeId::STRING)]),
    );
    bridge(&mut env, base_def, base_sym);
    bridge(&mut env, derived_def, derived_sym);

    let graph = InheritanceGraph::new();
    graph.add_inheritance(derived_sym, &[base_sym]);

    let derived = interner.lazy(derived_def);
    let base = interner.lazy(base_def);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env).with_inheritance_graph(&graph);
    assert!(
        checker.check_subtype(derived, base).is_true(),
        "derived <: base holds (nominal edge)",
    );

    let mut reverse = SubtypeChecker::with_resolver(&interner, &env).with_inheritance_graph(&graph);
    assert!(
        !reverse.check_subtype(base, derived).is_true(),
        "base is not a subtype of derived: no heritage edge that direction and \
         the base is missing the derived's extra member",
    );
}

/// For a **non-generic** target the nominal edge is authoritative: a registered
/// heritage edge yields `<: true` without consulting members. This mirrors the
/// pre-existing class-nominal behavior (a configuration where the derived drops
/// a base member is impossible in practice — TS2430 — so trusting the edge is
/// sound). Pairs with `subtype_generic_target_does_not_take_nominal_shortcut`.
#[test]
fn subtype_nominal_shortcut_trusts_non_generic_heritage_edge() {
    let interner = TypeInterner::new();
    let base_def = DefId(33_021);
    let derived_def = DefId(33_022);
    let base_sym = SymbolId(34_021);
    let derived_sym = SymbolId(34_022);

    let mut env = TypeEnvironment::new();
    declare_interface(
        &mut env,
        base_def,
        object(
            &interner,
            &[("a", TypeId::NUMBER), ("extra", TypeId::STRING)],
        ),
    );
    declare_interface(
        &mut env,
        derived_def,
        object(&interner, &[("a", TypeId::NUMBER)]),
    );
    bridge(&mut env, base_def, base_sym);
    bridge(&mut env, derived_def, derived_sym);

    let graph = InheritanceGraph::new();
    graph.add_inheritance(derived_sym, &[base_sym]);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env).with_inheritance_graph(&graph);
    assert!(
        checker
            .check_subtype(interner.lazy(derived_def), interner.lazy(base_def))
            .is_true(),
        "a non-generic heritage edge is authoritative — the nominal short-circuit \
         answers true without materializing/comparing the base's members",
    );
}

/// For a **generic** target the nominal edge is *not* authoritative — the
/// verdict could depend on type arguments threading into the base's members — so
/// the relation must fall through to the structural check, which honors the
/// member shapes. Identical heritage to the previous test except the base is
/// generic; the structurally-incompatible members now yield `false`.
#[test]
fn subtype_generic_target_does_not_take_nominal_shortcut() {
    let interner = TypeInterner::new();
    let base_def = DefId(33_031);
    let derived_def = DefId(33_032);
    let base_sym = SymbolId(34_031);
    let derived_sym = SymbolId(34_032);

    let (t_param, _t_ty) = type_param(&interner, "T");

    let mut env = TypeEnvironment::new();
    // Same shapes as the non-generic case, but the base now carries a type
    // parameter, so the nominal short-circuit must be disabled.
    declare_generic_interface(
        &mut env,
        base_def,
        object(
            &interner,
            &[("a", TypeId::NUMBER), ("extra", TypeId::STRING)],
        ),
        vec![t_param],
    );
    declare_interface(
        &mut env,
        derived_def,
        object(&interner, &[("a", TypeId::NUMBER)]),
    );
    bridge(&mut env, base_def, base_sym);
    bridge(&mut env, derived_def, derived_sym);

    let graph = InheritanceGraph::new();
    graph.add_inheritance(derived_sym, &[base_sym]);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env).with_inheritance_graph(&graph);
    assert!(
        !checker
            .check_subtype(interner.lazy(derived_def), interner.lazy(base_def))
            .is_true(),
        "a generic target must fall through to the structural check; the derived \
         body is missing the base's `extra` member, so the relation is false",
    );
}

/// The pre-existing class-nominal path is preserved: with a class-check closure
/// in scope, a subclass resolves `<:` its superclass via the graph even when the
/// base body is unresolvable.
#[test]
fn subtype_nominal_class_heritage_still_resolves() {
    let interner = TypeInterner::new();
    let base_def = DefId(33_041);
    let derived_def = DefId(33_042);
    let base_sym = SymbolId(34_041);
    let derived_sym = SymbolId(34_042);

    let mut env = TypeEnvironment::new();
    env.insert_def(derived_def, object(&interner, &[("p", TypeId::NUMBER)]));
    bridge(&mut env, base_def, base_sym);
    bridge(&mut env, derived_def, derived_sym);

    let graph = InheritanceGraph::new();
    graph.add_inheritance(derived_sym, &[base_sym]);

    let is_class = |sym: SymbolRef| sym == SymbolRef(base_sym.0) || sym == SymbolRef(derived_sym.0);
    let mut checker = SubtypeChecker::with_resolver(&interner, &env)
        .with_inheritance_graph(&graph)
        .with_class_check(&is_class);
    assert!(
        checker
            .check_subtype(interner.lazy(derived_def), interner.lazy(base_def))
            .is_true(),
        "class nominal subtyping via the inheritance graph is unchanged",
    );
}

/// On-demand heritage descent follows the full transitive closure: an interface
/// that derives from a base through an intermediate interface resolves `<:` the
/// root base nominally, with neither the intermediate nor the root materialized.
#[test]
fn subtype_transitive_nominal_heritage_resolves() {
    let interner = TypeInterner::new();
    let root_def = DefId(33_051);
    let mid_def = DefId(33_052);
    let leaf_def = DefId(33_053);
    let root_sym = SymbolId(34_051);
    let mid_sym = SymbolId(34_052);
    let leaf_sym = SymbolId(34_053);

    let mut env = TypeEnvironment::new();
    // Only the leaf has a body; mid and root are unresolvable.
    declare_interface(
        &mut env,
        leaf_def,
        object(&interner, &[("p", TypeId::NUMBER)]),
    );
    bridge(&mut env, root_def, root_sym);
    bridge(&mut env, mid_def, mid_sym);
    bridge(&mut env, leaf_def, leaf_sym);

    let graph = InheritanceGraph::new();
    graph.add_inheritance(leaf_sym, &[mid_sym]);
    graph.add_inheritance(mid_sym, &[root_sym]);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env).with_inheritance_graph(&graph);
    assert!(
        checker
            .check_subtype(interner.lazy(leaf_def), interner.lazy(root_def))
            .is_true(),
        "leaf <: root via the transitive heritage closure, without materializing \
         the intermediate or root interfaces",
    );
}

/// Without a heritage edge the relation still uses the ordinary structural
/// member check — the nominal broadening must not swallow structurally unrelated
/// references. Two unrelated non-generic interfaces compare by their members.
#[test]
fn subtype_unrelated_interfaces_fall_through_to_structural() {
    let interner = TypeInterner::new();
    let a_def = DefId(33_061);
    let b_def = DefId(33_062);
    let a_sym = SymbolId(34_061);
    let b_sym = SymbolId(34_062);

    let mut env = TypeEnvironment::new();
    // `b` is a structural subtype of `a` (b has all of a's members), but they
    // share no heritage edge.
    declare_interface(&mut env, a_def, object(&interner, &[("a", TypeId::NUMBER)]));
    declare_interface(
        &mut env,
        b_def,
        object(&interner, &[("a", TypeId::NUMBER), ("b", TypeId::STRING)]),
    );
    bridge(&mut env, a_def, a_sym);
    bridge(&mut env, b_def, b_sym);

    // Empty graph: no registered edges.
    let graph = InheritanceGraph::new();

    let mut to_super =
        SubtypeChecker::with_resolver(&interner, &env).with_inheritance_graph(&graph);
    assert!(
        to_super
            .check_subtype(interner.lazy(b_def), interner.lazy(a_def))
            .is_true(),
        "structural fall-through: b has all of a's members, so b <: a",
    );

    let mut to_sub = SubtypeChecker::with_resolver(&interner, &env).with_inheritance_graph(&graph);
    assert!(
        !to_sub
            .check_subtype(interner.lazy(a_def), interner.lazy(b_def))
            .is_true(),
        "structural fall-through: a is missing b's `b` member, so a is not <: b",
    );
}
