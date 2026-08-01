//! Parity tests for the memoized free-`infer` containment query.
//!
//! `contains_free_infer_types` was changed (#15729) to memoize its deep
//! `FREE_INFER`-policy walk per node in the project-wide predicate cache,
//! mirroring `contains_free_type_parameters_db` (#13250). These tests pin
//! that the cached answer is byte-identical to a fresh uncached walk over the
//! same policy, that re-querying the same id is stable (cache hits return the
//! same value), and that the `FREE_INFER` semantics — generic signature
//! bodies and the operands of deferred conditional/mapped/indexed-access/
//! `keyof` operations do not count as free `infer` uses (#14784) — survive
//! the cache.

use crate::intern::TypeInterner;
use crate::types::{
    ConditionalType, FunctionShape, MappedType, PropertyInfo, TupleElement, TypeData, TypeId,
    TypeParamInfo,
};
use crate::visitors::child_policy::ChildPolicy;
use crate::visitors::visitor_predicates::contains_free_infer_types;

/// Reference oracle: a fresh, fully-uncached `FREE_INFER`-policy containment
/// walk. This mirrors the pre-memoization `DeepContainsChecker` behaviour
/// exactly, so any divergence from `contains_free_infer_types` is a cache bug.
fn reference_contains_free_infer(interner: &TypeInterner, root: TypeId) -> bool {
    use crate::visitors::child_policy::{for_each_child_with_policy, has_policy_children};
    use rustc_hash::FxHashSet;

    let mut stack = vec![root];
    let mut visited = FxHashSet::default();
    while let Some(current) = stack.pop() {
        if current.is_intrinsic() || !visited.insert(current) {
            continue;
        }
        let Some(key) = interner.lookup(current) else {
            continue;
        };
        if matches!(key, TypeData::Infer(_)) {
            return true;
        }
        if !has_policy_children(&key, &ChildPolicy::FREE_INFER) {
            continue;
        }
        for_each_child_with_policy(interner, &key, &ChildPolicy::FREE_INFER, |child| {
            stack.push(child);
        });
    }
    false
}

fn build_corpus(interner: &TypeInterner) -> Vec<TypeId> {
    let t = interner.type_param(TypeParamInfo::simple(interner.intern_string("T")));
    let infer_v = interner.infer(TypeParamInfo::simple(interner.intern_string("V")));
    let p = interner.intern_string("p");

    let leaves = [TypeId::STRING, TypeId::NUMBER, t, infer_v];

    let mut corpus = Vec::new();
    for leaf in leaves {
        corpus.push(leaf);
        corpus.push(interner.array(leaf));
        corpus.push(interner.union(vec![TypeId::NUMBER, leaf]));
        corpus.push(interner.tuple(vec![TupleElement::fixed(leaf)]));
        corpus.push(interner.object(vec![PropertyInfo::new(p, leaf)]));
        // Nest two levels deep so the per-node memo has shared subtrees to reuse.
        let nested = interner.array(interner.union(vec![leaf, interner.array(leaf)]));
        corpus.push(nested);
    }

    // A conditional whose `extends` clause structurally contains `infer V` is
    // the canonical *declaration-site* shape (`X extends Foo<infer V> ? V :
    // never`): the deferred operand is not descended, so the conditional as a
    // whole must NOT be reported as containing a free `infer`, even though
    // `infer_v` itself is reachable as a direct child of `extends_type`.
    let declared_cond = interner.conditional(ConditionalType {
        check_type: t,
        extends_type: infer_v,
        true_type: TypeId::NUMBER,
        false_type: TypeId::STRING,
        is_distributive: false,
    });
    corpus.push(declared_cond);
    corpus.push(interner.array(declared_cond));

    // A conditional whose `true_type` branch leaks a bare `infer` that is NOT
    // itself declared in this conditional's `extends` clause is a separate
    // question from the declaration-site case above; the branch bodies are
    // ordinary structural children (not deferred operands) and must still be
    // walked, so a free `infer` embedded there is still detected.
    let leaking_cond = interner.conditional(ConditionalType {
        check_type: t,
        extends_type: TypeId::STRING,
        true_type: infer_v,
        false_type: TypeId::NUMBER,
        is_distributive: false,
    });
    corpus.push(leaking_cond);

    // `keyof`/indexed-access operands are deferred the same way.
    corpus.push(interner.keyof(infer_v));
    corpus.push(interner.index_access(infer_v, TypeId::STRING));

    // A mapped type's `constraint` (a `keyof` operand) hides its `infer` the
    // same way a conditional's `extends` clause does.
    let mapped_hidden = interner.mapped(MappedType {
        type_param: TypeParamInfo::simple(interner.intern_string("K")),
        constraint: interner.keyof(infer_v),
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    });
    corpus.push(mapped_hidden);

    // A generic function whose only `infer`-shaped use is inside its own
    // signature body must not count as a free `infer` (skipped wholesale).
    let generic_fn = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo::simple(interner.intern_string("W"))],
        params: vec![],
        this_type: None,
        return_type: infer_v,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    corpus.push(generic_fn);
    corpus.push(interner.union(vec![TypeId::NUMBER, generic_fn]));

    // But a free outer `infer` alongside a signature body must still be found.
    let mixed = interner.union(vec![infer_v, generic_fn]);
    corpus.push(mixed);

    corpus.push(interner.recursive(0));
    corpus.push(interner.array(interner.recursive(1)));
    corpus
}

#[test]
fn cached_free_infer_query_matches_uncached_reference() {
    let interner = TypeInterner::new();
    for root in build_corpus(&interner) {
        let expected = reference_contains_free_infer(&interner, root);
        // First query populates the cache; second must hit it with the same answer.
        let first = contains_free_infer_types(&interner, root);
        let second = contains_free_infer_types(&interner, root);
        assert_eq!(
            first,
            expected,
            "cached free-infer query disagreed with reference for {:?}",
            interner.lookup(root)
        );
        assert_eq!(
            second,
            first,
            "cached free-infer query not stable on re-query for {:?}",
            interner.lookup(root)
        );
    }
}

#[test]
fn declaration_site_infer_is_not_free() {
    let interner = TypeInterner::new();
    let t = interner.type_param(TypeParamInfo::simple(interner.intern_string("T")));
    let infer_v = interner.infer(TypeParamInfo::simple(interner.intern_string("V")));

    // `T extends Foo<infer V> ? V : never`-shaped: `infer V` is declared in
    // the (deferred, unwalked) `extends` clause, so the conditional as a
    // whole contains no free `infer`.
    let cond = interner.conditional(ConditionalType {
        check_type: t,
        extends_type: infer_v,
        true_type: TypeId::NUMBER,
        false_type: TypeId::STRING,
        is_distributive: false,
    });
    assert!(
        !contains_free_infer_types(&interner, cond),
        "infer declared only in a conditional's extends clause must not count as free"
    );
}

#[test]
fn generic_signature_body_infer_is_not_free() {
    let interner = TypeInterner::new();
    let infer_v = interner.infer(TypeParamInfo::simple(interner.intern_string("V")));
    let generic_fn = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo::simple(interner.intern_string("W"))],
        params: vec![],
        this_type: None,
        return_type: infer_v,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(
        !contains_free_infer_types(&interner, generic_fn),
        "infer used only inside a signature body must not count as free"
    );

    // A free outer `infer` reachable outside any signature must still be
    // detected, even when an inner signature is also present (the cache must
    // not let the body-skip mask the free outer use).
    let mixed = interner.union(vec![infer_v, generic_fn]);
    assert!(
        contains_free_infer_types(&interner, mixed),
        "free outer infer alongside a generic signature must be detected"
    );
}
