use super::*;
use crate::def::DefId;

#[test]
fn application_union_member_order_is_permutation_independent() {
    // Build a set of distinct `Application` union members exercising every
    // axis the comparator orders on: builtin vs deferred (`Lazy`) base,
    // different `DefId`s, differing argument lists, and differing arity.
    // Mixing in a non-application deferred member and a builtin also drives
    // the `Application`-vs-other ranking. Canonical union construction must
    // produce the SAME interned type regardless of input order; the comparator
    // that orders these members must remain a consistent strict total order.
    let interner = TypeInterner::new();
    let base_a = interner.lazy(DefId(101));
    let base_b = interner.lazy(DefId(202));

    let members = vec![
        interner.application(base_a, vec![TypeId::NUMBER]),
        interner.application(base_b, vec![TypeId::STRING]),
        interner.application(base_a, vec![TypeId::STRING]),
        interner.application(base_a, vec![TypeId::NUMBER, TypeId::STRING]),
        interner.application(base_b, vec![]),
        interner.application(TypeId::OBJECT, vec![TypeId::BOOLEAN]),
        interner.lazy(DefId(303)),
        TypeId::NUMBER,
    ];

    let canonical = interner.union(members.clone());
    let Some(TypeData::Union(canonical_list)) = interner.lookup(canonical) else {
        panic!("expected the application-bearing union to survive normalization");
    };
    let canonical_members: Vec<TypeId> = interner.type_list(canonical_list).to_vec();
    assert!(
        canonical_members.len() >= 2,
        "the disjoint application members must survive reduction"
    );

    // Reversed, rotated, and swapped permutations of the SAME member set
    // must all canonicalize to the identical interned union type id.
    let mut reversed = members.clone();
    reversed.reverse();

    let mut rotated = members.clone();
    rotated.rotate_left(3);

    let mut swapped = members.clone();
    swapped.swap(0, members.len() - 1);
    swapped.swap(1, 4);

    for perm in [reversed, rotated, swapped] {
        assert_eq!(
            interner.union(perm),
            canonical,
            "application union member ordering must be independent of input order"
        );
    }
}
