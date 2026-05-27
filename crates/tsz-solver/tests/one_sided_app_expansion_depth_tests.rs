//! Tests for the one-sided application-expansion recursion-identity bound
//! (`ONE_SIDED_APP_EXPANSION_MAX_DEPTH`), tsz's analogue of tsc's
//! `isDeeplyNestedType`. The bound keeps `App <: T` and `T <: App` relations
//! terminating cheaply when a generic's structural expansion keeps
//! re-introducing the same generic, while leaving shallow finite expansions
//! (and real mismatches) untouched.

use crate::construction::TypeInterner;
use crate::def::resolver::TypeEnvironment;
use crate::def::{DefId, DefKind};
use crate::relations::subtype::SubtypeChecker;
use crate::relations::subtype::rules::generics::ONE_SIDED_APP_EXPANSION_MAX_DEPTH;
use crate::types::{PropertyInfo, TypeData, TypeId, TypeParamInfo};

/// Build a recursive generic `R<T> = { next: R<R<T>> }` whose every expansion
/// re-introduces `R` with a structurally deeper argument. The iteration
/// variable name is a parameter so callers can prove the bound is keyed on the
/// generic's `DefId`, not the chosen type-parameter spelling.
fn insert_growing_recursive_generic(
    interner: &TypeInterner,
    env: &mut TypeEnvironment,
    def_id: DefId,
    param_name: &str,
) {
    let param = TypeParamInfo {
        name: interner.intern_string(param_name),
        constraint: None,
        default: None,
        is_const: false,
    };
    let param_ty = interner.intern(TypeData::TypeParameter(param));
    let r_lazy = interner.lazy(def_id);
    let r_of_t = interner.application(r_lazy, vec![param_ty]); // R<T>
    let r_of_r_of_t = interner.application(r_lazy, vec![r_of_t]); // R<R<T>>
    let body = interner.object(vec![PropertyInfo::new(
        interner.intern_string("next"),
        r_of_r_of_t,
    )]);
    env.insert_def_with_params(def_id, body, vec![param]);
    env.insert_def_kind(def_id, DefKind::TypeAlias);
}

#[test]
fn one_sided_app_expansion_depth_bounds_recursion_identity() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let def = DefId(4242);

    // The same generic may nest up to the limit on the one-sided expansion path.
    for _ in 0..ONE_SIDED_APP_EXPANSION_MAX_DEPTH {
        assert!(checker.enter_app_expansion_depth(def));
    }
    // One level past the limit is refused, signalling the caller to assume
    // related (`Ternary.Maybe`) instead of expanding further.
    assert!(!checker.enter_app_expansion_depth(def));

    // Leaving exactly frees one slot: it is a depth, not a permanent count.
    checker.leave_app_expansion_depth(def);
    assert!(checker.enter_app_expansion_depth(def));
    assert!(!checker.enter_app_expansion_depth(def));

    // The bound is keyed on the generic's DefId, so an unrelated generic is
    // tracked independently rather than sharing a single global counter.
    assert!(checker.enter_app_expansion_depth(DefId(9999)));
}

#[test]
fn deeply_nested_one_sided_application_terminates_and_is_related() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // Source: a generic whose expansion keeps re-introducing itself.
    let r = DefId(700);
    insert_growing_recursive_generic(&interner, &mut env, r, "T");

    // Target: a fixed recursive object `Q = { next: Q }`. The source's growing
    // application must be compared against it level after level, which without a
    // recursion-identity bound drives an expensive expansion.
    let q = DefId(701);
    let q_lazy = interner.lazy(q);
    let q_body = interner.object(vec![PropertyInfo::new(
        interner.intern_string("next"),
        q_lazy,
    )]);
    env.insert_def(q, q_body);
    env.insert_def_kind(q, DefKind::TypeAlias);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    let source = interner.application(interner.lazy(r), vec![TypeId::NUMBER]);

    // Both sides are infinite `{ next: { next: ... } }` shapes, so the relation
    // holds coinductively; the bound makes it terminate cheaply.
    assert!(checker.check_subtype(source, q_lazy).is_true());

    // Enter/leave stayed balanced: no expansion depth leaked past the relation.
    assert!(checker.app_expand_depth.values().all(|&d| d == 0));
}

#[test]
fn renamed_type_parameter_does_not_change_recursion_bound() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let q = DefId(801);
    let q_lazy = interner.lazy(q);
    let q_body = interner.object(vec![PropertyInfo::new(
        interner.intern_string("next"),
        q_lazy,
    )]);
    env.insert_def(q, q_body);
    env.insert_def_kind(q, DefKind::TypeAlias);

    // Two structurally identical recursive generics differing only in the
    // iteration-variable spelling must produce the same relation outcome.
    let r_t = DefId(802);
    insert_growing_recursive_generic(&interner, &mut env, r_t, "T");
    let r_k = DefId(803);
    insert_growing_recursive_generic(&interner, &mut env, r_k, "Key");

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    let source_t = interner.application(interner.lazy(r_t), vec![TypeId::NUMBER]);
    let source_k = interner.application(interner.lazy(r_k), vec![TypeId::NUMBER]);

    assert_eq!(
        checker.check_subtype(source_t, q_lazy).is_true(),
        checker.check_subtype(source_k, q_lazy).is_true(),
    );
}

#[test]
fn shallow_finite_application_still_resolves_exactly() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // Non-recursive `Box<T> = { value: T }` expands once and never approaches
    // the recursion bound, so the guard must not perturb ordinary results.
    let param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let param_ty = interner.intern(TypeData::TypeParameter(param));
    let box_def = DefId(900);
    let body = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        param_ty,
    )]);
    env.insert_def_with_params(box_def, body, vec![param]);
    env.insert_def_kind(box_def, DefKind::TypeAlias);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    let box_number = interner.application(interner.lazy(box_def), vec![TypeId::NUMBER]);

    let matching = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::NUMBER,
    )]);
    let mismatching = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::STRING,
    )]);

    // Positive: structural match is accepted.
    assert!(checker.check_subtype(box_number, matching).is_true());
    // Negative: a real shallow mismatch is still reported, not masked by the bound.
    assert!(!checker.check_subtype(box_number, mismatching).is_true());
}
