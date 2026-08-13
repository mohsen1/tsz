//! Unit tests for union/intersection normalization and disjointness helpers.
//!
//! Split out of `normalize.rs` to keep that module under the 2000-line
//! source ceiling. Included as a child module via `#[path]`, so `use super::*`
//! still resolves `normalize.rs`'s module-private items.

use super::*;
use crate::types::{TemplateSpan, TupleElement};

#[test]
fn shallow_subtype_skips_literal_to_template_literal_matching() {
    let interner = TypeInterner::new();
    let literal = interner.literal_string("foo-x");
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo-")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    assert!(
        !interner.is_subtype_shallow(literal, template),
        "union normalization should not invoke full template-literal subtype matching"
    );
}

#[test]
fn union_template_absorbs_matching_string_literal() {
    let interner = TypeInterner::new();
    let literal = interner.literal_string("foo-1");
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo-")),
        TemplateSpan::Type(TypeId::NUMBER),
    ]);

    // `"foo-1" | `foo-${number}`` reduces to the template member.
    assert_eq!(interner.union(vec![literal, template]), template);
}

#[test]
fn union_template_keeps_non_matching_string_literal() {
    let interner = TypeInterner::new();
    let literal = interner.literal_string("bar-1");
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo-")),
        TemplateSpan::Type(TypeId::NUMBER),
    ]);

    let union = interner.union(vec![literal, template]);
    let Some(TypeData::Union(list_id)) = interner.lookup(union) else {
        panic!("expected a two-member union to survive normalization");
    };
    assert_eq!(interner.type_list(list_id).len(), 2);
}

#[test]
fn union_template_without_leading_text_absorbs_literal() {
    let interner = TypeInterner::new();
    let literal = interner.literal_string("12px");
    let template = interner.template_literal(vec![
        TemplateSpan::Type(TypeId::NUMBER),
        TemplateSpan::Text(interner.intern_string("px")),
    ]);

    // No leading-text prefilter applies; the full shallow match still runs.
    assert_eq!(interner.union(vec![literal, template]), template);
}

#[test]
fn union_string_placeholder_template_keeps_literals_shallow() {
    let interner = TypeInterner::new();
    // Mirrors the lib.dom `AutoFill` family shape: many plain literals next
    // to `${string}`-placeholder templates. Shallow matching does not match
    // `${string}` placeholders, so every member must survive.
    let members = vec![
        interner.literal_string("name"),
        interner.literal_string("billing name"),
        interner.template_literal(vec![
            TemplateSpan::Text(interner.intern_string("section-")),
            TemplateSpan::Type(TypeId::STRING),
            TemplateSpan::Text(interner.intern_string(" name")),
        ]),
    ];
    let union = interner.union(members);
    let Some(TypeData::Union(list_id)) = interner.lookup(union) else {
        panic!("expected union to survive normalization");
    };
    assert_eq!(interner.type_list(list_id).len(), 3);
}

fn distinct_conditional(interner: &TypeInterner, n: u32) -> TypeId {
    // A deferred, distributive conditional whose check type is unique per `n`
    // so each interns to a separate `TypeData::Conditional`. This is the shape
    // produced by distributing `Exclude`/`Extract` over a wide union before
    // the conditionals resolve.
    interner.conditional(crate::types::ConditionalType {
        check_type: interner.literal_number(f64::from(n)),
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: true,
    })
}

#[test]
fn union_of_deferred_conditionals_all_survive_reduction() {
    let interner = TypeInterner::new();
    // The shallow subtype engine cannot relate two deferred conditionals, so a
    // union of distinct unevaluated conditionals is irreducible: every member
    // must survive. (The pairwise sweep over them is also skipped, but the
    // perf counter is only wired under `TSZ_PERF_COUNTERS`, so the observable
    // unit-test invariant is the surviving member set.)
    const N: u32 = 64;
    let members: Vec<TypeId> = (0..N).map(|i| distinct_conditional(&interner, i)).collect();
    let union = interner.union(members);
    let Some(TypeData::Union(list_id)) = interner.lookup(union) else {
        panic!("expected a wide deferred-conditional union to survive normalization");
    };
    assert_eq!(interner.type_list(list_id).len(), N as usize);
}

#[test]
fn deferred_conditional_does_not_block_concrete_subtype_reduction() {
    let interner = TypeInterner::new();
    // A union mixing an inert deferred conditional with concrete members where
    // one is a subtype of another (`"a"` <: `string`). The deferred member is
    // inert and must be preserved, but it must NOT suppress the concrete
    // reduction: `"a"` is absorbed into `string`, leaving `string` + the
    // conditional. This is the JSX-props regression in miniature — folding the
    // deferred member into a whole-union skip wrongly kept `"a"`.
    let cond = distinct_conditional(&interner, 0);
    let members = vec![interner.literal_string("a"), TypeId::STRING, cond];
    let union = interner.union(members);
    let Some(TypeData::Union(list_id)) = interner.lookup(union) else {
        panic!("expected a mixed deferred/concrete union to survive normalization");
    };
    let list = interner.type_list(list_id);
    assert_eq!(
        list.len(),
        2,
        "concrete `\"a\" | string` must reduce to `string` beside the inert conditional"
    );
    assert!(
        list.contains(&TypeId::STRING),
        "widened `string` must survive"
    );
    assert!(
        list.contains(&cond),
        "inert deferred conditional must survive"
    );
    assert!(
        !list.contains(&interner.literal_string("a")),
        "literal `\"a\"` must be absorbed by `string`"
    );
}

#[test]
fn cross_domain_primitive_and_literals_all_survive_reduction() {
    let interner = TypeInterner::new();
    // `number | "m0" | "m1" | ... | "mN-1"`: the primitive (`number`) does
    // not absorb the string literals (different domain), and distinct string
    // literals are mutually non-subtypes. The literal-vs-literal pairs are
    // skipped by the structural-bucket gate (`may_relate(Literal, Literal)`
    // is `false`), but the surviving member set must be identical to a full
    // pairwise sweep: every member survives.
    const N: usize = 50;
    let mut members = vec![TypeId::NUMBER];
    for i in 0..N {
        members.push(interner.literal_string(&format!("m{i}")));
    }
    let union = interner.union(members);
    let Some(TypeData::Union(list_id)) = interner.lookup(union) else {
        panic!("expected a `number | <string literals>` union to survive");
    };
    let list = interner.type_list(list_id);
    assert_eq!(
        list.len(),
        N + 1,
        "no member of a cross-domain primitive + distinct-literals union reduces"
    );
    assert!(list.contains(&TypeId::NUMBER), "`number` must survive");
}

#[test]
fn same_domain_primitive_absorbs_all_literals_with_mask() {
    let interner = TypeInterner::new();
    // `string | "s0" | ... | "sN-1"` must still collapse to just `string`.
    // Absorption runs before the pairwise sweep, so the structural-bucket
    // gate never changes this outcome — guards against the skip leaking into
    // the absorb path.
    const N: usize = 50;
    let mut members = vec![TypeId::STRING];
    for i in 0..N {
        members.push(interner.literal_string(&format!("s{i}")));
    }
    assert_eq!(
        interner.union(members),
        TypeId::STRING,
        "widened `string` absorbs every string literal regardless of the literal mask"
    );
}

#[test]
fn literal_mask_preserves_object_vs_literal_reduction() {
    let interner = TypeInterner::new();
    // A union mixing distinct string literals with two structurally-related
    // objects where one reduces into the other, plus a widened `number`
    // primitive. A union of *only* literals and objects hits the literal-gate
    // early return (`all_non_reducible && !has_primitive`) and never runs the
    // pairwise sweep, so the object pair would not reduce. Adding the widened
    // `number` primitive sets `has_primitive = true`, bypasses the gate, and
    // exercises the object-vs-object reduction. Width subtyping makes the
    // narrower-keyed object the supertype, so `{ a: 1; b: 2 } <: { a: 1 }` and
    // the wider object is absorbed; the literals and the `number` primitive are
    // irreducible and all survive.
    let obj_narrow = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        interner.literal_number(1.0),
    )]);
    let obj_wide = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), interner.literal_number(1.0)),
        PropertyInfo::new(interner.intern_string("b"), interner.literal_number(2.0)),
    ]);
    let members = vec![
        interner.literal_string("x"),
        interner.literal_string("y"),
        obj_narrow,
        obj_wide,
        TypeId::NUMBER,
    ];
    let union = interner.union(members);
    let Some(TypeData::Union(list_id)) = interner.lookup(union) else {
        panic!("expected the mixed literal/object/primitive union to survive as a union");
    };
    let list = interner.type_list(list_id);
    // Two string literals, the `number` primitive, and exactly one object of
    // the reducing pair survive: the widened primitive bypasses the literal
    // gate so `may_relate(Object, Object)` still runs and absorbs the wider
    // object into the narrower-keyed supertype.
    assert_eq!(
        list.len(),
        4,
        "object-vs-object reduction must still fire once a widened primitive bypasses the literal gate"
    );
    assert!(list.contains(&interner.literal_string("x")));
    assert!(list.contains(&interner.literal_string("y")));
    assert!(list.contains(&TypeId::NUMBER));
    assert!(
        list.contains(&obj_narrow),
        "the narrower-keyed object is the width-subtyping supertype and must survive"
    );
    assert!(
        !list.contains(&obj_wide),
        "the wider object is a shallow subtype of the narrower one and must be absorbed"
    );
}

#[test]
fn enum_absorbed_into_base_primitive_in_union_reduction() {
    use crate::def::DefId;
    let interner = TypeInterner::new();
    // A nominal enum wrapping a union of `members` under `def`.
    let mk_enum =
        |def: u32, members: Vec<TypeId>| interner.enum_type(DefId(def), interner.union(members));
    let num_members = || vec![interner.literal_number(0.0), interner.literal_number(1.0)];

    // A numeric enum's structural member union (`0 | 1`) is a subtype of
    // `number`, so `number | E` reduces to `number`; a string enum's
    // (`"p" | "q"`) is a subtype of `string`, so `string | S` reduces to
    // `string`. This mirrors tsc's `getUnionType` reduction after
    // `getBaseTypeOfLiteralType` brands enum members as their base primitive.
    let num_enum = mk_enum(7001, num_members());
    assert_eq!(
        interner.union(vec![TypeId::NUMBER, num_enum]),
        TypeId::NUMBER,
        "number | (numeric enum) must reduce to number"
    );
    // Intersection mirrors this: `E & number` keeps the nominal enum.
    assert_eq!(
        interner.intersection(vec![TypeId::NUMBER, num_enum]),
        num_enum,
        "number & (numeric enum) must reduce to the enum"
    );

    let str_enum = mk_enum(
        7002,
        vec![interner.literal_string("p"), interner.literal_string("q")],
    );
    assert_eq!(
        interner.union(vec![TypeId::STRING, str_enum]),
        TypeId::STRING,
        "string | (string enum) must reduce to string"
    );

    // A string enum is disjoint from `number`, so `number | S` is preserved.
    let mixed = interner.union(vec![TypeId::NUMBER, str_enum]);
    let Some(TypeData::Union(list_id)) = interner.lookup(mixed) else {
        panic!("number | (string enum) must stay a union");
    };
    assert_eq!(interner.type_list(list_id).len(), 2);

    // Distinct enums stay nominal: neither absorbs the other even with
    // identical structural members.
    let other_num_enum = mk_enum(7003, num_members());
    let two = interner.union(vec![num_enum, other_num_enum]);
    let Some(TypeData::Union(list_id)) = interner.lookup(two) else {
        panic!("two distinct enums must stay a union");
    };
    assert_eq!(
        interner.type_list(list_id).len(),
        2,
        "distinct enums are nominal and irreducible"
    );
}

#[test]
fn enum_absorbed_into_base_primitive_via_literal_only_union() {
    // Regression for #16866's TS2411 false positive on
    // `unionSubtypeIfEveryConstituentTypeIsSubtype.ts`: an interface property
    // annotation like `foo2: e | number` is constructed through
    // `normalize_union_literal_only` (the type-annotation path,
    // `get_type_from_union_type` -> `union_or_single_literal_reduce`), which
    // — unlike the general `union()` path already covered by
    // `enum_absorbed_into_base_primitive_in_union_reduction` above — skips
    // subtype-based reduction entirely and relies solely on the literal
    // ladder's `absorb_literals_into_primitives` step. tsc's own
    // `removeRedundantLiteralTypes` sweeps enum member literals the same way
    // it sweeps plain literals, so this path must absorb too.
    use crate::def::DefId;
    let interner = TypeInterner::new();
    let mk_enum =
        |def: u32, members: Vec<TypeId>| interner.enum_type(DefId(def), interner.union(members));

    // Renamed binder (different DefId/name than the union() test above) to
    // prove the rule is structural, not tied to a specific def or literal set.
    let num_enum = mk_enum(
        8101,
        vec![interner.literal_number(0.0), interner.literal_number(1.0)],
    );
    let str_enum = mk_enum(
        8102,
        vec![interner.literal_string("p"), interner.literal_string("q")],
    );
    // A specific enum member (not the whole-enum union) — the checker
    // represents `E.a` as `Enum(member_def_id, <that member's own literal>)`.
    let num_member = interner.enum_type(DefId(8103), interner.literal_number(5.0));

    for enriched in [false, true] {
        assert_eq!(
            interner.union_literal_ladder_for_test(vec![TypeId::NUMBER, num_enum], enriched),
            TypeId::NUMBER,
            "number | (numeric enum) must reduce to number on the literal-only path (enriched={enriched})"
        );
        assert_eq!(
            interner.union_literal_ladder_for_test(vec![TypeId::STRING, str_enum], enriched),
            TypeId::STRING,
            "string | (string enum) must reduce to string on the literal-only path (enriched={enriched})"
        );
        assert_eq!(
            interner.union_literal_ladder_for_test(vec![TypeId::NUMBER, num_member], enriched),
            TypeId::NUMBER,
            "number | (specific numeric enum member) must reduce to number (enriched={enriched})"
        );

        // Negative control: a string enum is disjoint from `number` and must
        // NOT be absorbed.
        let mixed =
            interner.union_literal_ladder_for_test(vec![TypeId::NUMBER, str_enum], enriched);
        let Some(TypeData::Union(list_id)) = interner.lookup(mixed) else {
            panic!(
                "number | (string enum) must stay a union on the literal-only path (enriched={enriched})"
            );
        };
        assert_eq!(interner.type_list(list_id).len(), 2);

        // Negative control: without the co-present primitive, two distinct
        // numeric enums stay nominal (mirrors the real I14 case: `e | E2`
        // itself, absent any bare `number`, is not touched by this rule).
        let other_num_enum = mk_enum(
            8104,
            vec![interner.literal_number(0.0), interner.literal_number(1.0)],
        );
        let two = interner.union_literal_ladder_for_test(vec![num_enum, other_num_enum], enriched);
        let Some(TypeData::Union(list_id)) = interner.lookup(two) else {
            panic!(
                "two distinct enums without a bare primitive must stay a union (enriched={enriched})"
            );
        };
        assert_eq!(interner.type_list(list_id).len(), 2);
    }
}

fn distinct_keyof(interner: &TypeInterner, n: u32) -> TypeId {
    // `keyof <unique literal>` — a distinct, unevaluated `TypeData::KeyOf`
    // per `n`. This is the shape produced by distributing `keyof` over a
    // wide union before the operands resolve.
    interner.keyof(interner.literal_number(f64::from(n)))
}

#[test]
fn union_of_deferred_keyofs_all_survive_reduction() {
    let interner = TypeInterner::new();
    // The shallow subtype engine cannot relate two deferred `keyof`
    // operations, so a union of distinct ones is irreducible: every member
    // survives. Before widening the inert-deferred lift past
    // `Conditional`/`IndexAccess`, this width drove the full N·(N−1)
    // pairwise sweep (all `false`).
    const N: u32 = 64;
    let members: Vec<TypeId> = (0..N).map(|i| distinct_keyof(&interner, i)).collect();
    let union = interner.union(members);
    let Some(TypeData::Union(list_id)) = interner.lookup(union) else {
        panic!("expected a wide deferred-keyof union to survive normalization");
    };
    assert_eq!(interner.type_list(list_id).len(), N as usize);
}

#[test]
fn deferred_keyof_does_not_block_concrete_subtype_reduction() {
    let interner = TypeInterner::new();
    // A union mixing an inert deferred `keyof` with concrete members where
    // one is a subtype of another (`"a"` <: `string`). The deferred member
    // is inert and must be preserved, but it must NOT suppress the concrete
    // reduction: `"a"` is absorbed into `string`. This proves the widened
    // lift reduces only the reducible remainder, exactly like the
    // `Conditional` case.
    let kof = distinct_keyof(&interner, 0);
    let members = vec![interner.literal_string("a"), TypeId::STRING, kof];
    let union = interner.union(members);
    let Some(TypeData::Union(list_id)) = interner.lookup(union) else {
        panic!("expected a mixed deferred/concrete union to survive normalization");
    };
    let list = interner.type_list(list_id);
    assert_eq!(
        list.len(),
        2,
        "concrete `\"a\" | string` must reduce to `string` beside the inert keyof"
    );
    assert!(
        list.contains(&TypeId::STRING),
        "widened `string` must survive"
    );
    assert!(list.contains(&kof), "inert deferred keyof must survive");
    assert!(
        !list.contains(&interner.literal_string("a")),
        "literal `\"a\"` must be absorbed by `string`"
    );
}

#[test]
fn widened_lift_preserves_concrete_result_beside_many_inert_members() {
    // The lift partitions inert members aside, reduces only the remainder,
    // then splices them back. The reduced *concrete* result must be exactly
    // what the same concrete members produce on their own — independent of
    // how many inert members are mixed in. Use a literal→primitive
    // absorption (`"a" | "b" | string` → `string`), which is a genuine,
    // deterministic reduction.
    let interner = TypeInterner::new();
    let a = interner.literal_string("a");
    let b = interner.literal_string("b");

    // Concrete-only baseline: collapses to `string`.
    let baseline = interner.union(vec![a, b, TypeId::STRING]);
    assert_eq!(
        baseline,
        TypeId::STRING,
        "`\"a\" | \"b\" | string` is `string`"
    );

    // Same concrete members beside a wide band of inert deferred members of
    // several families (keyof, conditional). The concrete part must still
    // collapse to exactly `string`; every inert member must survive.
    let mut inert: Vec<TypeId> = (0..40).map(|i| distinct_keyof(&interner, i)).collect();
    inert.extend((0..40).map(|i| distinct_conditional(&interner, i)));
    let mut members = vec![a, b, TypeId::STRING];
    members.extend(inert.iter().copied());
    let mixed = interner.union(members);
    let Some(TypeData::Union(list_id)) = interner.lookup(mixed) else {
        panic!("expected `string | <inert band>` to survive as a union");
    };
    let list = interner.type_list(list_id);

    assert_eq!(
        list.len(),
        inert.len() + 1,
        "exactly the collapsed `string` plus every inert member remain"
    );
    assert!(
        list.contains(&TypeId::STRING),
        "collapsed `string` must survive"
    );
    for &m in &inert {
        assert!(
            list.contains(&m),
            "every inert member must survive the lift"
        );
    }
    assert!(!list.contains(&a), "`\"a\"` must be absorbed by `string`");
    assert!(!list.contains(&b), "`\"b\"` must be absorbed by `string`");
}

fn obj_with_unique_prop(interner: &TypeInterner, i: usize) -> TypeId {
    let name = interner.intern_string(&format!("p{i}"));
    let val = interner.literal_number((1000 + i) as f64);
    interner.object(vec![PropertyInfo::new(name, val)])
}

#[test]
fn structural_bucket_preserves_mixed_object_primitive_literal_union() {
    // The large-row shape that reaches the O(N^2) pairwise sweep: a widened
    // primitive (so the `all_non_reducible && !has_primitive` early return
    // does not fire) beside distinct unique-prop objects and cross-domain
    // literals. No member is a subtype of any other, so every member must
    // survive — the structural-bucket skip must not drop any of them.
    let interner = TypeInterner::new();
    let mut members = vec![TypeId::BOOLEAN];
    for i in 0..100 {
        match i % 3 {
            0 => members.push(obj_with_unique_prop(&interner, i)),
            1 => members.push(interner.literal_number((7_000_000 + i) as f64)),
            _ => members.push(interner.literal_string(&format!("s{i}"))),
        }
    }
    let expected = members.len();
    let union = interner.union(members);
    let Some(TypeData::Union(list_id)) = interner.lookup(union) else {
        panic!("expected a wide mixed-kind union to survive normalization");
    };
    assert_eq!(
        interner.type_list(list_id).len(),
        expected,
        "every disjoint mixed-kind member must survive the bucketed sweep"
    );
}

#[test]
fn structural_bucket_still_reduces_object_subtype_in_wide_union() {
    // A wide union (so it takes the >64 bucketed path) carrying a genuine
    // object-vs-object reduction. The shallow object engine compares
    // overlapping property types at depth 0, where the depth-limited
    // recursion returns `false` for any *distinct* property type pair — so a
    // real shallow reduction needs identical-typed overlapping properties.
    // `{ a: 1, b: 2 }` <: `{ a: 1 }` by width subtyping (the wider-keyed
    // source satisfies every property of the narrower target with an
    // identical `a: 1`), so the *wider* object is the subtype and must be
    // reduced away even surrounded by disjoint padding members; the narrower
    // `{ a: 1 }` survives.
    let interner = TypeInterner::new();
    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let one = interner.literal_number(1.0);
    let narrow = interner.object(vec![PropertyInfo::new(a, one)]);
    let wide = interner.object(vec![
        PropertyInfo::new(a, one),
        PropertyInfo::new(b, interner.literal_number(2.0)),
    ]);

    let mut members = vec![TypeId::BOOLEAN, narrow, wide];
    // Disjoint padding to push the union onto the >64 bucketed path.
    for i in 0..80 {
        members.push(interner.literal_string(&format!("pad{i}")));
    }
    let union = interner.union(members);
    let Some(TypeData::Union(list_id)) = interner.lookup(union) else {
        panic!("expected the padded union to survive normalization");
    };
    let list = interner.type_list(list_id);
    assert!(
        list.contains(&narrow),
        "the narrower object `{{ a: 1 }}` must survive as the supertype"
    );
    assert!(
        !list.contains(&wide),
        "the wider object `{{ a: 1, b: 2 }}` is a width-subtype of `{{ a: 1 }}` \
         and must be reduced away even on the bucketed >64 path"
    );
}

#[test]
fn structural_bucket_skip_matches_unbucketed_reduction_small_partition() {
    // Drive the <=64 quadratic partition path (via the `boolean` primitive
    // keeping the early-return from firing) and assert the surviving set is
    // exactly the disjoint members: the stack-allocated bucket precompute
    // must not change reduction on the small path either.
    let interner = TypeInterner::new();
    let mut members = vec![TypeId::BOOLEAN];
    for i in 0..40 {
        if i % 2 == 0 {
            members.push(obj_with_unique_prop(&interner, i));
        } else {
            members.push(interner.literal_number((9_000_000 + i) as f64));
        }
    }
    let expected = members.len();
    let union = interner.union(members);
    let Some(TypeData::Union(list_id)) = interner.lookup(union) else {
        panic!("expected the small mixed union to survive normalization");
    };
    assert_eq!(
        interner.type_list(list_id).len(),
        expected,
        "disjoint members must all survive the small bucketed partition path"
    );
}

// -----------------------------------------------------------------------------
// Union construction-mode campaign (#15809): shared literal ladder +
// `subtype_reduced` derived query (Stage 1). See `intern/union_mode.rs`.
// -----------------------------------------------------------------------------

#[test]
fn union_literal_default_flag_defaults_off() {
    // The campaign is default-OFF; flag-off must be byte-identical by
    // construction, so the default read must be false in an unconfigured run.
    assert!(
        !crate::intern::union_literal_default_enabled(),
        "TSZ_UNION_LITERAL_DEFAULT must default OFF"
    );
}

#[test]
fn union_literal_ladder_enriched_merges_split_enums() {
    use crate::def::DefId;
    let interner = TypeInterner::new();
    // Two members of the same enum def, split apart. tsc's Literal ladder
    // rejoins `E.a | E.b` into `E`; the legacy literal-only path did not.
    let e_a = interner.enum_type(DefId(9101), interner.literal_string("a"));
    let e_b = interner.enum_type(DefId(9101), interner.literal_string("b"));

    let enriched = interner.union_literal_ladder_for_test(vec![e_a, e_b], true);
    assert!(
        matches!(
            interner.lookup(enriched),
            Some(TypeData::Enum(DefId(9101), _))
        ),
        "enriched literal ladder must merge same-def enum parts into one Enum"
    );

    let legacy = interner.union_literal_ladder_for_test(vec![e_a, e_b], false);
    let Some(TypeData::Union(list_id)) = interner.lookup(legacy) else {
        panic!("legacy literal ladder must keep the two split enum members as a union");
    };
    assert_eq!(
        interner.type_list(list_id).len(),
        2,
        "legacy literal ladder omits same-def enum merging"
    );
}

#[test]
fn union_literal_ladder_enriched_absorbs_intersection_with_present_constituent() {
    let interner = TypeInterner::new();
    let a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::STRING,
    )]);
    let b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("y"),
        TypeId::NUMBER,
    )]);
    // A raw intersection so the union carries a `TypeData::Intersection` member
    // whose part `a` is also a bare union constituent.
    let inter = interner.intersect_types_raw(vec![a, b]);
    assert!(
        matches!(interner.lookup(inter), Some(TypeData::Intersection(_))),
        "test setup: intersect_types_raw must keep an Intersection node"
    );

    let enriched = interner.union_literal_ladder_for_test(vec![a, inter], true);
    assert_eq!(
        enriched, a,
        "enriched literal ladder drops an intersection whose part is present (A | (A & B) => A)"
    );

    let legacy = interner.union_literal_ladder_for_test(vec![a, inter], false);
    let Some(TypeData::Union(list_id)) = interner.lookup(legacy) else {
        panic!("legacy literal ladder must keep both members");
    };
    assert_eq!(
        interner.type_list(list_id).len(),
        2,
        "legacy literal ladder omits intersection-with-constituent absorption"
    );
}

/// Build a narrow object `{ k1: 1 }` and a wider structural subtype
/// `{ k1: 1, k2: 2 }` (`wide <: narrow`) — the fixture the #15809 reduction tests
/// share to distinguish literal-mode (keeps both) from subtype-mode (drops `wide`).
fn narrow_wide_key_objects(interner: &TypeInterner, k1: &str, k2: &str) -> (TypeId, TypeId) {
    let narrow = interner.object(vec![PropertyInfo::new(
        interner.intern_string(k1),
        interner.literal_number(1.0),
    )]);
    let wide = interner.object(vec![
        PropertyInfo::new(interner.intern_string(k1), interner.literal_number(1.0)),
        PropertyInfo::new(interner.intern_string(k2), interner.literal_number(2.0)),
    ]);
    (narrow, wide)
}

/// Build a `{ narrow-key } | { wide-key } | number` union in literal mode (no
/// subtype reduction), so the wider object survives for `subtype_reduced` to
/// remove. The widened `number` primitive bypasses the pairwise reducer's
/// all-object literal gate (see `object_vs_object_reduces_only_with_primitive`).
fn literal_mode_object_union(interner: &TypeInterner, k1: &str, k2: &str) -> (TypeId, TypeId) {
    let (narrow, wide) = narrow_wide_key_objects(interner, k1, k2);
    let lit_union = interner.union_literal_reduce(vec![narrow, wide, TypeId::NUMBER]);
    (lit_union, narrow)
}

#[test]
fn subtype_reduced_recovers_reduction_dropped_by_literal_default() {
    let interner = TypeInterner::new();
    let (lit_union, narrow) = literal_mode_object_union(&interner, "a", "b");

    // Literal mode keeps the wider object (the subsumed member) — tsc's
    // UnionReduction.Literal behavior.
    let Some(TypeData::Union(lit_list)) = interner.lookup(lit_union) else {
        panic!("literal-mode union must retain all three members");
    };
    assert_eq!(
        interner.type_list(lit_list).len(),
        3,
        "literal default must keep the subsumed wide object"
    );

    // The `.Subtype` counterpart recovers the reduction: the wide object is a
    // shallow subtype of the narrow-keyed supertype and is dropped.
    let reduced = interner.subtype_reduced(lit_union, 0);
    let Some(TypeData::Union(reduced_list)) = interner.lookup(reduced) else {
        panic!("reduced union should still be a union (narrow | number)");
    };
    let list = interner.type_list(reduced_list);
    assert_eq!(
        list.len(),
        2,
        "subtype_reduced must drop the subsumed wide object"
    );
    assert!(list.contains(&narrow));
    assert!(list.contains(&TypeId::NUMBER));
    assert_ne!(
        reduced, lit_union,
        "reduction forks a distinct union TypeId"
    );
}

/// #15809 constructor gate: the default `normalize_union` construction path
/// honors the literal/subtype reduction mode. With `literal_only = false`
/// (historical `main`, flag OFF) a subsumed structural member is removed at
/// construction; with `literal_only = true` (flag ON, tsc
/// `UnionReduction.Literal`) it survives, so the constructor is genuinely
/// literal-mode — the discipline the evaluate-layer Stage 2 gate assumes when it
/// re-interns evaluated unions without its own blanket reduce.
#[test]
fn normalize_union_construction_gate_skips_subtype_reduction_in_literal_mode() {
    let interner = TypeInterner::new();
    let (narrow, wide) = narrow_wide_key_objects(&interner, "a", "b");
    // `number` keeps the union off the pairwise reducer's all-object-literal gate
    // so the subtype sweep actually runs in the historical mode.
    let members = vec![narrow, wide, TypeId::NUMBER];

    // Historical / flag-OFF: `wide` is a shallow subtype of the narrow-keyed
    // object and is removed at construction, leaving `narrow | number`.
    let reduced = interner.normalize_union_for_test(members.clone(), false);
    let Some(TypeData::Union(reduced_list)) = interner.lookup(reduced) else {
        panic!("subtype-mode construction should leave narrow | number");
    };
    let list = interner.type_list(reduced_list);
    assert_eq!(
        list.len(),
        2,
        "subtype-mode construction drops the subsumed wide object"
    );
    assert!(list.contains(&narrow));
    assert!(list.contains(&TypeId::NUMBER));

    // Flag-ON / literal mode: the subsumed `wide` object survives — tsc's
    // UnionReduction.Literal, no construction-time pairwise removal.
    let literal = interner.normalize_union_for_test(members, true);
    let Some(TypeData::Union(literal_list)) = interner.lookup(literal) else {
        panic!("literal-mode construction should keep all three members");
    };
    assert_eq!(
        interner.type_list(literal_list).len(),
        3,
        "literal-mode construction keeps the subsumed wide object"
    );
    assert_ne!(
        reduced, literal,
        "the two reduction modes fork distinct union identities"
    );
}

/// The gate only removes the pairwise sweep — it never adds, drops, or reorders
/// members on a union with nothing to subtype-reduce, so a disjoint member set is
/// byte-identical across both modes.
#[test]
fn normalize_union_construction_gate_is_identity_on_irreducible_unions() {
    let interner = TypeInterner::new();
    let members = vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN];
    assert_eq!(
        interner.normalize_union_for_test(members.clone(), false),
        interner.normalize_union_for_test(members, true),
        "modes agree when there is no subtype pair to remove"
    );
}

#[test]
fn subtype_reduced_is_idempotent() {
    let interner = TypeInterner::new();
    let (lit_union, _) = literal_mode_object_union(&interner, "a", "b");
    let reduced = interner.subtype_reduced(lit_union, 0);
    assert_eq!(
        interner.subtype_reduced(reduced, 0),
        reduced,
        "subtype_reduced(subtype_reduced(u)) == subtype_reduced(u)"
    );
}

#[test]
fn subtype_reduced_returns_non_union_and_complex_unions_unchanged() {
    let interner = TypeInterner::new();
    // Non-union input is returned unchanged.
    assert_eq!(interner.subtype_reduced(TypeId::NUMBER, 0), TypeId::NUMBER);

    // A union carrying a `TypeParameter` member hits the `has_complex` guard and
    // is returned unchanged (reducing against an unresolved parameter is unsound).
    use crate::TypeParamInfo;
    let tp = interner.type_param(TypeParamInfo::simple(interner.intern_string("T")));
    let (narrow, wide) = narrow_wide_key_objects(&interner, "a", "b");
    let complex_union = interner.union_literal_reduce(vec![tp, narrow, wide, TypeId::NUMBER]);
    assert_eq!(
        interner.subtype_reduced(complex_union, 0),
        complex_union,
        "unions with a TypeParameter/Lazy member must not be subtype-reduced"
    );
}

#[test]
fn subtype_reduced_result_is_stable_across_resolver_generations() {
    let interner = TypeInterner::new();
    let (lit_union, _) = literal_mode_object_union(&interner, "a", "b");
    // The shallow reduction is resolver-independent; the generation key only
    // scopes the memo (intersection_merge_cache precedent). Different generations
    // must yield the same reduced TypeId.
    let g0 = interner.subtype_reduced(lit_union, 0);
    let g7 = interner.subtype_reduced(lit_union, 7);
    assert_eq!(
        g0, g7,
        "reduced form must be identical across resolver generations"
    );
    // And a re-query at a bumped generation recomputes to the same result.
    assert_eq!(interner.subtype_reduced(lit_union, 7), g7);
}

#[test]
fn subtype_reduced_is_binder_name_invariant() {
    // Same structural union, different property names: reduction must drop the
    // wide object in both, proving it is structural (not name-driven).
    let interner = TypeInterner::new();
    let (union_ab, narrow_ab) = literal_mode_object_union(&interner, "a", "b");
    let (union_pq, narrow_pq) = literal_mode_object_union(&interner, "p", "q");

    let reduced_ab = interner.subtype_reduced(union_ab, 0);
    let reduced_pq = interner.subtype_reduced(union_pq, 0);
    for (reduced, narrow) in [(reduced_ab, narrow_ab), (reduced_pq, narrow_pq)] {
        let Some(TypeData::Union(list_id)) = interner.lookup(reduced) else {
            panic!("both renamed unions must reduce to a two-member union");
        };
        let list = interner.type_list(list_id);
        assert_eq!(list.len(), 2);
        assert!(list.contains(&narrow));
        assert!(list.contains(&TypeId::NUMBER));
    }
}

/// The object members of an intersection are merged into one synthesized
/// object, so a redundant object intersection has no `TypeData::Intersection`
/// left for the relation layer to recognise. The merge origin is what makes
/// the pre-merge members recoverable; conditional extends-clause identity
/// (`intersection_member_set`) depends on it being recorded here.
#[test]
fn redundant_object_intersection_merges_and_records_its_origin() {
    let interner = TypeInterner::new();
    let a = interner.intern_string("a");
    let one = interner.literal_number(1.0);
    let one_or_number = interner.union(vec![one, TypeId::NUMBER]);
    let narrow = interner.object(vec![crate::types::PropertyInfo::new(a, one)]);
    let wide = interner.object(vec![crate::types::PropertyInfo::new(a, one_or_number)]);

    let intersection = interner.intersection(vec![narrow, wide]);

    assert!(
        matches!(interner.lookup(intersection), Some(TypeData::Object(_))),
        "object members of an intersection are merged into a single object shape"
    );
    assert_ne!(
        intersection, narrow,
        "the merged object is a distinct type, not the subsumed member itself"
    );

    let origin = interner
        .get_merged_intersection_origin(intersection)
        .expect("the merged object must record the intersection it came from");
    let Some(TypeData::Intersection(list_id)) = interner.lookup(origin) else {
        panic!("the recorded origin must still be an Intersection");
    };
    let members: Vec<TypeId> = interner.type_list(list_id).to_vec();
    assert_eq!(members.len(), 2);
    assert!(members.contains(&narrow) && members.contains(&wide));
}

/// The array sibling of the shape above takes a different path: array members
/// are not merged, so the intersection node survives interning and the plain
/// `TypeData::Intersection` arm of `intersection_member_set` already sees it.
#[test]
fn redundant_array_intersection_survives_interning() {
    let interner = TypeInterner::new();
    let narrow = interner.array(TypeId::STRING);
    let wide = interner.array(interner.union(vec![TypeId::STRING, TypeId::NUMBER]));

    let intersection = interner.intersection(vec![narrow, wide]);

    assert!(
        matches!(
            interner.lookup(intersection),
            Some(TypeData::Intersection(_))
        ),
        "array members are never merged, so the intersection node is preserved"
    );
    assert_ne!(intersection, narrow);
}

// `union2` is a fast path for `union`; the two constructors must mint the
// identical interned `TypeId` for the same member set. Regression tests for the
// content-canonical-identity bug (#14344) where `union2`'s hand-rolled insertion
// ordered by allocation order and skipped reduction, diverging from `union`.

#[test]
fn union2_matches_union_for_string_literal_insertion() {
    let interner = TypeInterner::new();
    // Intern "b" and "c" first, then "a", so "a" has the highest allocation
    // order. Content-canonical ordering is by string value ("a" < "b" < "c"),
    // which is independent of that allocation order.
    let b = interner.literal_string("b");
    let c = interner.literal_string("c");
    let a = interner.literal_string("a");

    let existing = interner.union(vec![b, c]);
    assert_eq!(
        interner.union2(a, existing),
        interner.union(vec![a, b, c]),
        "union2 must place the new literal at its content-canonical position, not by alloc order"
    );
}

#[test]
fn union2_absorbs_literals_into_primitive_like_union() {
    let interner = TypeInterner::new();
    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let lits = interner.union(vec![a, b]);
    // `string | "a" | "b"` collapses to `string` in both constructors.
    assert_eq!(interner.union2(TypeId::STRING, lits), TypeId::STRING);
    assert_eq!(
        interner.union2(TypeId::STRING, lits),
        interner.union(vec![TypeId::STRING, a, b])
    );
}

#[test]
fn union2_reduces_object_subtypes_like_union() {
    let interner = TypeInterner::new();
    let obj_narrow = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        interner.literal_number(1.0),
    )]);
    let obj_wide = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), interner.literal_number(1.0)),
        PropertyInfo::new(interner.intern_string("b"), interner.literal_number(2.0)),
    ]);
    // `{ a: 1; b: 2 } <: { a: 1 }` (width subtyping): the wide object is absorbed.
    // The `number` member keeps this off the all-literals-and-objects early-return
    // gate so the pairwise object reduction actually runs.
    let existing = interner.union(vec![obj_narrow, TypeId::NUMBER]);
    assert_eq!(
        interner.union2(obj_wide, existing),
        interner.union(vec![obj_narrow, obj_wide, TypeId::NUMBER]),
        "union2 must apply the same structural subtype reduction as union"
    );
}

#[test]
fn union2_result_is_invariant_under_interning_order() {
    // The interned union `TypeId` must be a pure function of the member set, not
    // of the order the members were interned in (allocation order). Build the
    // same `"x" | "y" | "z"` union in two interners with opposite interning
    // order, via both constructors, and assert one identity and one rendered
    // order across the board.
    let render = |i: &TypeInterner, u: TypeId| -> Vec<String> {
        match i.lookup(u) {
            Some(TypeData::Union(l)) => i
                .type_list(l)
                .iter()
                .map(|&m| match i.lookup(m) {
                    Some(TypeData::Literal(LiteralValue::String(atom))) => {
                        i.string_interner.resolve(atom).to_string()
                    }
                    other => format!("{other:?}"),
                })
                .collect(),
            _ => vec![],
        }
    };

    // Forward interning order: x, y, z.
    let fi = TypeInterner::new();
    let fx = fi.literal_string("x");
    let fy = fi.literal_string("y");
    let fz = fi.literal_string("z");
    let f_union = fi.union2(fz, fi.union(vec![fx, fy]));
    assert_eq!(
        f_union,
        fi.union(vec![fx, fy, fz]),
        "union2 == union (forward)"
    );

    // Reverse interning order: z, y, x.
    let ri = TypeInterner::new();
    let rz = ri.literal_string("z");
    let ry = ri.literal_string("y");
    let rx = ri.literal_string("x");
    let r_union = ri.union2(rx, ri.union(vec![ry, rz]));
    assert_eq!(
        r_union,
        ri.union(vec![rx, ry, rz]),
        "union2 == union (reverse)"
    );

    // Rendered member order is content-canonical regardless of interning order.
    assert_eq!(
        render(&fi, f_union),
        render(&ri, r_union),
        "union member order must be content-canonical, independent of interning order"
    );
    assert_eq!(render(&fi, f_union), vec!["x", "y", "z"]);
}

#[test]
fn tuple_union_members_order_by_widened_element_not_alloc_order() {
    // A union of tuple members must sort by their widened element types (tsc's
    // `stableTypeOrdering`), not by the order the tuples were interned. Intern
    // the boolean-tuple first so allocation order alone would keep it first;
    // the canonical order must instead place the number-tuple first because
    // `number` precedes `boolean`. This is the ordering that drives covariant
    // inference for `new Map([["", true], ["", 0]])` (issue #17364): the first
    // element-union candidate wins, so `V` must resolve to `number`.
    let elem_index = |i: &TypeInterner, u: TypeId| -> Vec<u8> {
        // Map each tuple member to a small tag for its second element's base:
        // 0 = number-ish, 1 = boolean-ish, 2 = other.
        match i.lookup(u) {
            Some(TypeData::Union(l)) => i
                .type_list(l)
                .iter()
                .map(|&m| match i.lookup(m) {
                    Some(TypeData::Tuple(list)) => {
                        let elems = i.tuple_list(list);
                        match elems.last().map(|e| e.type_id) {
                            Some(TypeId::NUMBER) => 0,
                            Some(id)
                                if matches!(
                                    i.lookup(id),
                                    Some(TypeData::Literal(LiteralValue::Number(_)))
                                ) =>
                            {
                                0
                            }
                            Some(
                                TypeId::BOOLEAN | TypeId::BOOLEAN_TRUE | TypeId::BOOLEAN_FALSE,
                            ) => 1,
                            _ => 2,
                        }
                    }
                    _ => 2,
                })
                .collect(),
            _ => vec![],
        }
    };

    let build = |i: &TypeInterner, bool_first: bool| -> TypeId {
        let s = TypeId::STRING;
        let make = |elem: TypeId| i.tuple(vec![TupleElement::fixed(s), TupleElement::fixed(elem)]);
        if bool_first {
            let t_bool = make(TypeId::BOOLEAN);
            let t_num = make(TypeId::NUMBER);
            i.union(vec![t_bool, t_num])
        } else {
            let t_num = make(TypeId::NUMBER);
            let t_bool = make(TypeId::BOOLEAN);
            i.union(vec![t_num, t_bool])
        }
    };

    // Boolean-tuple interned first (allocation order = boolean, number).
    let a = TypeInterner::new();
    let ua = build(&a, true);
    assert_eq!(
        elem_index(&a, ua),
        vec![0, 1],
        "number-element tuple must sort before boolean-element tuple regardless of alloc order"
    );

    // Number-tuple interned first: same canonical order.
    let b = TypeInterner::new();
    let ub = build(&b, false);
    assert_eq!(
        elem_index(&b, ub),
        vec![0, 1],
        "tuple union order must be content-canonical, independent of interning order"
    );

    // Fresh literal tuples (`["", true]` / `["", 0]`) order the same way once
    // their scalar elements are widened for the ordering key.
    let c = TypeInterner::new();
    let s = c.literal_string("");
    let t_true = c.tuple(vec![
        TupleElement::fixed(s),
        TupleElement::fixed(TypeId::BOOLEAN_TRUE),
    ]);
    let zero = c.literal_number(0.0);
    let t_zero = c.tuple(vec![TupleElement::fixed(s), TupleElement::fixed(zero)]);
    let u_lit = c.union(vec![t_true, t_zero]);
    assert_eq!(
        elem_index(&c, u_lit),
        vec![0, 1],
        "`[\"\", 0]` must sort before `[\"\", true]` via widened element ordering"
    );
}

#[test]
fn union2_dedup_and_literal_absorption_fast_paths_stay_canonical() {
    let interner = TypeInterner::new();
    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let u = interner.union(vec![a, b]);
    // Re-adding an existing member returns the same union identity.
    assert_eq!(interner.union2(a, u), u);
    // Number-literal insertion is ordered by numeric value, not alloc order.
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);
    let one = interner.literal_number(1.0);
    let nums = interner.union(vec![two, three]);
    assert_eq!(
        interner.union2(one, nums),
        interner.union(vec![one, two, three])
    );
}
