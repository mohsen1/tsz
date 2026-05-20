//! Unit tests for `tsz_solver::deep_reduce_for_display`
//! (`crates/tsz-solver/src/diagnostics/reduce.rs`).
//!
//! These tests are mounted via the `#[cfg(test)] #[path = ...]` pattern
//! from `diagnostics/reduce.rs`, so `super::*` is the solver crate root and
//! we can reach internal-but-`pub(crate)` paths through `crate::...` while
//! also using the public solver surface.
//!
//! Coverage strategy:
//!
//! - Lock the documented invariants: intrinsics return identity, the cycle
//!   guard via `visited` is honored, structural composites recurse and only
//!   re-intern when a child changes, leaves that don't reduce stay verbatim.
//! - Use `NoopResolver` so `Application` / `Conditional` cannot be replaced
//!   via lazy-DefId resolution. Conditionals over concrete intrinsics still
//!   reduce because `evaluate_conditional` only needs subtype checks.
//! - Build composite types and assert `deep_reduce` returns the IDENTICAL
//!   `TypeId` when no leaf reduces (the optimization arm that skips
//!   re-interning when nothing changed).

use super::*;
use crate::TypeInterner;
use crate::deep_reduce_for_display;
use crate::def::resolver::NoopResolver;
use crate::types::{ConditionalType, TupleElement, TypeData};

// =============================================================================
// Intrinsic identity
// =============================================================================

#[test]
fn deep_reduce_returns_intrinsics_unchanged() {
    let interner = TypeInterner::new();
    let resolver = NoopResolver;

    // Each intrinsic must short-circuit at the `is_intrinsic()` check.
    for &id in &[
        TypeId::ANY,
        TypeId::UNKNOWN,
        TypeId::NEVER,
        TypeId::VOID,
        TypeId::NULL,
        TypeId::UNDEFINED,
        TypeId::BOOLEAN,
        TypeId::NUMBER,
        TypeId::STRING,
        TypeId::BIGINT,
        TypeId::SYMBOL,
    ] {
        let out = deep_reduce_for_display(&interner, &resolver, id);
        assert_eq!(out, id, "intrinsic {id:?} must round-trip");
    }
}

#[test]
fn deep_reduce_returns_error_intrinsic_unchanged() {
    let interner = TypeInterner::new();
    let resolver = NoopResolver;

    // ERROR is also an intrinsic — verify the fast path explicitly.
    let out = deep_reduce_for_display(&interner, &resolver, TypeId::ERROR);
    assert_eq!(out, TypeId::ERROR);
}

#[test]
fn deep_reduce_returns_literal_unchanged() {
    let interner = TypeInterner::new();
    let resolver = NoopResolver;

    // Literals are not intrinsic, but they fall through the `_ => type_id`
    // arm of `reduce_inner` because they aren't Application / Conditional /
    // Intersection / Union / Object / ObjectWithIndex.
    let lit_str = interner.literal_string("hello");
    let lit_num = interner.literal_number(1.0);
    let lit_bool = interner.literal_boolean(true);

    assert_eq!(
        deep_reduce_for_display(&interner, &resolver, lit_str),
        lit_str
    );
    assert_eq!(
        deep_reduce_for_display(&interner, &resolver, lit_num),
        lit_num
    );
    assert_eq!(
        deep_reduce_for_display(&interner, &resolver, lit_bool),
        lit_bool
    );
}

// =============================================================================
// Conditional types
// =============================================================================

#[test]
fn deep_reduce_replaces_concrete_conditional_true_branch() {
    let interner = TypeInterner::new();
    let resolver = NoopResolver;

    // string extends string ? number : boolean → reduces to number.
    let cond = interner.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    });

    let reduced = deep_reduce_for_display(&interner, &resolver, cond);
    assert_eq!(reduced, TypeId::NUMBER);
}

#[test]
fn deep_reduce_replaces_concrete_conditional_false_branch() {
    let interner = TypeInterner::new();
    let resolver = NoopResolver;

    // number extends string ? boolean : bigint → reduces to bigint.
    let cond = interner.conditional(ConditionalType {
        check_type: TypeId::NUMBER,
        extends_type: TypeId::STRING,
        true_type: TypeId::BOOLEAN,
        false_type: TypeId::BIGINT,
        is_distributive: false,
    });

    let reduced = deep_reduce_for_display(&interner, &resolver, cond);
    assert_eq!(reduced, TypeId::BIGINT);
}

// =============================================================================
// Application leaves stay verbatim under NoopResolver
// =============================================================================

#[test]
fn deep_reduce_preserves_application_leaf_when_evaluator_cannot_resolve() {
    let interner = TypeInterner::new();
    let resolver = NoopResolver;

    // `evaluate_application` only succeeds when the base resolves to a DefId.
    // For a non-DefId base (here NUMBER), evaluate returns the same TypeId,
    // and `reduce_inner` should keep the leaf verbatim (the `reduced == type_id`
    // branch of the Application/Conditional arm).
    let app = interner.application(TypeId::NUMBER, vec![TypeId::STRING]);

    let reduced = deep_reduce_for_display(&interner, &resolver, app);
    assert_eq!(
        reduced, app,
        "Application that does not reduce must stay verbatim"
    );
}

// =============================================================================
// Union recursion
// =============================================================================

#[test]
fn deep_reduce_returns_primitive_union_unchanged() {
    let interner = TypeInterner::new();
    let resolver = NoopResolver;

    // `string | number` — every member is intrinsic, so the union arm walks
    // each child, observes no change, and must return `type_id` (not a
    // re-interned union with the same members). Identity matters because
    // re-interning would defeat the `if changed` optimization.
    let u = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let reduced = deep_reduce_for_display(&interner, &resolver, u);
    assert_eq!(reduced, u);
}

#[test]
fn deep_reduce_replaces_union_with_reducing_conditional_member() {
    let interner = TypeInterner::new();
    let resolver = NoopResolver;

    // (string extends string ? number : boolean) | bigint
    //   = number | bigint
    let cond = interner.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    });
    let u = interner.union(vec![cond, TypeId::BIGINT]);

    let reduced = deep_reduce_for_display(&interner, &resolver, u);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::BIGINT]);
    assert_eq!(reduced, expected);
}

// =============================================================================
// Intersection recursion
// =============================================================================

#[test]
fn deep_reduce_returns_intersection_unchanged_when_no_member_reduces() {
    let interner = TypeInterner::new();
    let resolver = NoopResolver;

    // Build an object intersection with only intrinsic property types — no
    // member reduces, so the intersection arm hits its identity-return path.
    let prop_a = PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER);
    let prop_b = PropertyInfo::new(interner.intern_string("b"), TypeId::STRING);
    let obj_a = interner.object(vec![prop_a]);
    let obj_b = interner.object(vec![prop_b]);
    let inter = interner.intersection(vec![obj_a, obj_b]);

    let reduced = deep_reduce_for_display(&interner, &resolver, inter);
    assert_eq!(reduced, inter);
}

// =============================================================================
// Object recursion
// =============================================================================

#[test]
fn deep_reduce_returns_object_unchanged_when_no_property_reduces() {
    let interner = TypeInterner::new();
    let resolver = NoopResolver;

    // { foo: number; bar: string } — no property has a reducible leaf, so
    // the Object arm walks every property, observes no change, and returns
    // the same TypeId rather than constructing a new object.
    let foo = PropertyInfo::new(interner.intern_string("foo"), TypeId::NUMBER);
    let bar = PropertyInfo::new(interner.intern_string("bar"), TypeId::STRING);
    let obj = interner.object(vec![foo, bar]);

    let reduced = deep_reduce_for_display(&interner, &resolver, obj);
    assert_eq!(reduced, obj);
}

#[test]
fn deep_reduce_replaces_object_property_with_reducing_conditional() {
    let interner = TypeInterner::new();
    let resolver = NoopResolver;

    // { x: (string extends string ? number : boolean) } → { x: number }
    let cond = interner.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    });
    let prop = PropertyInfo::new(interner.intern_string("x"), cond);
    let obj = interner.object(vec![prop]);

    let reduced = deep_reduce_for_display(&interner, &resolver, obj);

    // The result is a fresh object whose only property `x` is NUMBER.
    let Some(TypeData::Object(shape_id)) = interner.lookup(reduced) else {
        panic!("expected Object, got {:?}", interner.lookup(reduced));
    };
    let shape = interner.object_shape(shape_id);
    assert_eq!(shape.properties.len(), 1);
    assert_eq!(shape.properties[0].type_id, TypeId::NUMBER);
    assert_eq!(shape.properties[0].write_type, TypeId::NUMBER);
}

#[test]
fn deep_reduce_object_preserves_write_type_when_distinct_from_read_type() {
    let interner = TypeInterner::new();
    let resolver = NoopResolver;

    // Property whose write_type differs from type_id and contains a
    // reducible Conditional. Both branches must be reduced independently.
    let read_cond = interner.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    });
    let write_cond = interner.conditional(ConditionalType {
        check_type: TypeId::NUMBER,
        extends_type: TypeId::STRING,
        true_type: TypeId::BOOLEAN,
        false_type: TypeId::BIGINT,
        is_distributive: false,
    });
    let mut prop = PropertyInfo::new(interner.intern_string("y"), read_cond);
    prop.write_type = write_cond;
    let obj = interner.object(vec![prop]);

    let reduced = deep_reduce_for_display(&interner, &resolver, obj);
    let Some(TypeData::Object(shape_id)) = interner.lookup(reduced) else {
        panic!("expected Object, got {:?}", interner.lookup(reduced));
    };
    let shape = interner.object_shape(shape_id);
    assert_eq!(shape.properties[0].type_id, TypeId::NUMBER);
    assert_eq!(shape.properties[0].write_type, TypeId::BIGINT);
}

// =============================================================================
// Non-recursing kinds preserved as identity
// =============================================================================

#[test]
fn deep_reduce_returns_array_unchanged() {
    let interner = TypeInterner::new();
    let resolver = NoopResolver;

    // Array<T> is `TypeData::Application(Array, [T])` *only* when the
    // checker has registered the Array base. Without that registration,
    // `interner.array(T)` lowers to a structural object/tuple-like form,
    // but in either case it lands in the `_ => type_id` arm because
    // `reduce_inner` only special-cases Application/Conditional and a
    // small set of structural composites. Lock the identity round-trip.
    let arr = interner.array(TypeId::NUMBER);
    let reduced = deep_reduce_for_display(&interner, &resolver, arr);
    assert_eq!(reduced, arr);
}

#[test]
fn deep_reduce_returns_tuple_unchanged() {
    let interner = TypeInterner::new();
    let resolver = NoopResolver;

    // Tuples are NOT recursed by `reduce_inner` (only Object/ObjectWithIndex
    // are). Lock that identity round-trip explicitly so a future change that
    // adds Tuple recursion notices the contract shift.
    let tup = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let reduced = deep_reduce_for_display(&interner, &resolver, tup);
    assert_eq!(reduced, tup);
}

#[test]
fn deep_reduce_returns_keyof_unchanged() {
    let interner = TypeInterner::new();
    let resolver = NoopResolver;

    // `keyof T` lands in the catch-all arm (`_ => type_id`).
    let prop = PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER);
    let obj = interner.object(vec![prop]);
    let k = interner.keyof(obj);
    let reduced = deep_reduce_for_display(&interner, &resolver, k);
    assert_eq!(reduced, k);
}

// =============================================================================
// Recursive composites
// =============================================================================

#[test]
fn deep_reduce_recurses_through_nested_intersections() {
    let interner = TypeInterner::new();
    let resolver = NoopResolver;

    // { p: (string extends string ? number : boolean) } & { q: number }
    //
    // The intersection arm of `reduce_inner` recurses into each member;
    // the first member's `p` reduces from Conditional to NUMBER, so the
    // arm rebuilds the intersection. We do NOT lock the post-merge shape
    // (the `interner.intersection` constructor may simplify the result —
    // e.g. merge two single-property objects into one). What we lock is:
    //
    //   1. The TypeId changes (i.e. the reducer noticed the inner change).
    //   2. The reduced result has the post-reduction property type NUMBER
    //      reachable somewhere — either as a member of an Intersection or
    //      as a property of a merged Object.
    let cond = interner.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    });
    let prop_p = PropertyInfo::new(interner.intern_string("p"), cond);
    let obj_p = interner.object(vec![prop_p]);
    let prop_q = PropertyInfo::new(interner.intern_string("q"), TypeId::NUMBER);
    let obj_q = interner.object(vec![prop_q]);
    let inter = interner.intersection(vec![obj_p, obj_q]);

    let reduced = deep_reduce_for_display(&interner, &resolver, inter);
    assert_ne!(
        reduced, inter,
        "intersection containing reducible leaf must change identity"
    );

    // Walk the post-reduction shape to confirm `p` is now NUMBER.
    let mut saw_reduced_p = false;
    let mut visit_shape = |shape_id| {
        let shape = interner.object_shape(shape_id);
        for p in &shape.properties {
            let name = interner.resolve_atom_ref(p.name);
            if name.as_ref() == "p" && p.type_id == TypeId::NUMBER {
                saw_reduced_p = true;
            }
        }
    };
    match interner.lookup(reduced) {
        Some(TypeData::Intersection(list_id)) => {
            let members = interner.type_list(list_id);
            for &m in members.iter() {
                if let Some(TypeData::Object(s)) | Some(TypeData::ObjectWithIndex(s)) =
                    interner.lookup(m)
                {
                    visit_shape(s);
                }
            }
        }
        Some(TypeData::Object(s)) | Some(TypeData::ObjectWithIndex(s)) => {
            visit_shape(s);
        }
        other => panic!("expected Intersection or merged Object after reduction, got {other:?}"),
    }
    assert!(
        saw_reduced_p,
        "expected reduced property `p: number` somewhere in the result"
    );
}

// =============================================================================
// Cycle guard
// =============================================================================

#[test]
fn deep_reduce_revisits_same_id_safely() {
    let interner = TypeInterner::new();
    let resolver = NoopResolver;

    // Two adjacent unions with identical members reuse the same interned
    // `TypeId`; calling deep_reduce twice must be idempotent and return the
    // identical handle each time.
    let u = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let r1 = deep_reduce_for_display(&interner, &resolver, u);
    let r2 = deep_reduce_for_display(&interner, &resolver, u);
    assert_eq!(r1, u);
    assert_eq!(r2, u);
    assert_eq!(r1, r2);
}

#[test]
fn deep_reduce_handles_self_referential_union_via_visited_guard() {
    let interner = TypeInterner::new();
    let resolver = NoopResolver;

    // We cannot construct a literally-self-referential `TypeId` without a
    // resolver, but we can stress the visited guard by deeply nesting unions
    // and intersections that share members. The reducer must terminate and
    // return the same TypeId because no leaf reduces.
    let inner_u = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let outer_u = interner.union(vec![inner_u, TypeId::BOOLEAN]);
    let outer_i = interner.intersection(vec![outer_u, inner_u]);

    let reduced = deep_reduce_for_display(&interner, &resolver, outer_i);
    assert_eq!(reduced, outer_i);
}

// =============================================================================
// Application with reducible inner — current contract: Application leaves
// are NOT recursed into, only `evaluate(...)` is asked. Lock that.
// =============================================================================

#[test]
fn deep_reduce_does_not_descend_into_application_args() {
    let interner = TypeInterner::new();
    let resolver = NoopResolver;

    // App(NUMBER, [Conditional(reduces to STRING)])
    // Without a resolver, evaluate_application returns the original TypeId,
    // so reduce_inner takes the `reduced == type_id` arm and returns the
    // application verbatim — even though the inner argument *would* reduce
    // if it were the top-level input.
    let cond = interner.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    });
    let app = interner.application(TypeId::NUMBER, vec![cond]);

    let reduced = deep_reduce_for_display(&interner, &resolver, app);
    assert_eq!(
        reduced, app,
        "Application with non-DefId base stays verbatim"
    );

    // Verify the inner argument is preserved as the original conditional ID
    // by inspecting the application back.
    let Some(TypeData::Application(app_id)) = interner.lookup(reduced) else {
        panic!("expected Application after reduction");
    };
    let app_data = interner.type_application(app_id);
    assert_eq!(app_data.args, vec![cond]);
    assert_eq!(app_data.base, TypeId::NUMBER);
}
