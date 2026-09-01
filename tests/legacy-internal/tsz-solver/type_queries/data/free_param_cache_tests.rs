//! Parity tests for the memoized free-type-parameter containment query.
//!
//! `contains_free_type_parameters_db` was changed (#13250) to memoize its deep
//! FREE-policy walk per node in the project-wide predicate cache. These tests
//! pin that the cached answer is byte-identical to a fresh uncached
//! [`crate::visitors::visitor_predicates`]-style walk, that re-querying the same
//! id is stable (cache hits return the same value), and that the FREE policy
//! semantics — generic signature bodies bind their own parameters and so do not
//! count as free — survive the cache.

use crate::def::DefId;
use crate::intern::TypeInterner;
use crate::type_queries::contains_free_type_parameters_db;
use crate::types::{
    ConditionalType, FunctionShape, MappedType, PropertyInfo, TupleElement, TypeData, TypeId,
    TypeParamInfo,
};
use crate::visitors::child_policy::ChildPolicy;
use crate::visitors::visitor_predicates::contains_type_matching;

/// Reference oracle: a fresh, fully-uncached FREE-policy containment walk. This
/// mirrors the pre-memoization `DeepContainsChecker` behaviour exactly, so any
/// divergence from `contains_free_type_parameters_db` is a cache bug.
fn reference_contains_free(interner: &TypeInterner, root: TypeId) -> bool {
    use crate::visitors::child_policy::{for_each_child_with_policy, has_policy_children};
    use rustc_hash::FxHashSet;

    fn matches(key: &TypeData) -> bool {
        matches!(
            key,
            TypeData::TypeParameter(_)
                | TypeData::Infer(_)
                | TypeData::ThisType
                | TypeData::BoundParameter(_)
        )
    }

    let mut stack = vec![root];
    let mut visited = FxHashSet::default();
    while let Some(current) = stack.pop() {
        if current.is_intrinsic() || !visited.insert(current) {
            continue;
        }
        let Some(key) = interner.lookup(current) else {
            continue;
        };
        if matches(&key) {
            return true;
        }
        if !has_policy_children(&key, &ChildPolicy::FREE_TYPE_PARAMS) {
            continue;
        }
        for_each_child_with_policy(interner, &key, &ChildPolicy::FREE_TYPE_PARAMS, |child| {
            stack.push(child);
        });
    }
    false
}

fn build_corpus(interner: &TypeInterner) -> Vec<TypeId> {
    let t = interner.type_param(TypeParamInfo::simple(interner.intern_string("T")));
    let p = interner.intern_string("p");

    let leaves = [
        TypeId::STRING,
        TypeId::NUMBER,
        t,
        interner.infer(TypeParamInfo::simple(interner.intern_string("V"))),
        interner.this_type(),
        interner.conditional(ConditionalType {
            check_type: t,
            extends_type: TypeId::STRING,
            true_type: TypeId::NUMBER,
            false_type: t,
            is_distributive: false,
        }),
        interner.keyof(t),
        interner.index_access(t, TypeId::STRING),
        interner.mapped(MappedType {
            type_param: TypeParamInfo::simple(interner.intern_string("K")),
            constraint: interner.keyof(t),
            name_type: None,
            template: t,
            readonly_modifier: None,
            optional_modifier: None,
        }),
    ];

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

    // Node kinds that `contains_free_type_parameters_db` short-circuits to
    // `false` at the *root* before the deep FREE-policy walk (`Enum`,
    // `Recursive`, `ModuleNamespace`, `UniqueSymbol`). For these the reference
    // oracle below takes a structurally different path: `has_policy_children`
    // reports no FREE children for the latter three (so the walk also yields
    // `false`), while `Enum` *does* expose its closed structural member type as
    // a child, so the oracle walks it. Real enums are nominal — their structural
    // member type is a closed union of literal members and can never embed a
    // free type parameter — so both paths must agree on `false`. Pinning these
    // here proves the root short-circuit is byte-identical to the deep walk for
    // exactly the shapes the structural corpus above omits (#13250).
    let enum_members = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    corpus.push(interner.enum_type(DefId(4242), enum_members));
    // Enum nested inside other shapes still resolves through the (non-root)
    // policy walk; assert the answer matches the oracle there too.
    let enum_ty = interner.enum_type(DefId(4243), enum_members);
    corpus.push(interner.array(enum_ty));
    corpus.push(interner.union(vec![enum_ty, t]));
    corpus.push(interner.recursive(0));
    corpus.push(interner.array(interner.recursive(1)));
    corpus
}

#[test]
fn cached_free_param_query_matches_uncached_reference() {
    let interner = TypeInterner::new();
    for root in build_corpus(&interner) {
        let expected = reference_contains_free(&interner, root);
        // First query populates the cache; second must hit it with the same answer.
        let first = contains_free_type_parameters_db(&interner, root);
        let second = contains_free_type_parameters_db(&interner, root);
        assert_eq!(
            first,
            expected,
            "cached free-param query disagreed with reference for {:?}",
            interner.lookup(root)
        );
        assert_eq!(
            second,
            first,
            "cached free-param query not stable on re-query for {:?}",
            interner.lookup(root)
        );
    }
}

#[test]
fn generic_signature_body_params_are_not_free() {
    let interner = TypeInterner::new();
    // `<W>() => W` — the only parameter use is bound by the signature's own
    // generic declaration, so the type contains NO free parameter.
    let w = TypeParamInfo::simple(interner.intern_string("W"));
    let w_type = interner.type_param(w);
    let generic_fn = interner.function(FunctionShape {
        type_params: vec![w],
        params: vec![],
        this_type: None,
        return_type: w_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(
        !contains_free_type_parameters_db(&interner, generic_fn),
        "generic signature body param must not count as free"
    );

    // Wrapping the generic fn in an object keeps it non-free.
    let wrapper = interner.object(vec![PropertyInfo::new(
        interner.intern_string("m"),
        generic_fn,
    )]);
    assert!(
        !contains_free_type_parameters_db(&interner, wrapper),
        "object wrapping a generic signature must not count as free"
    );

    // But a *free* outer `T` reachable outside any signature must be detected,
    // even when an inner generic signature is also present (cache must not let
    // the body-skip mask the free outer use).
    let t = TypeParamInfo::simple(interner.intern_string("T"));
    let t_type = interner.type_param(t);
    let mixed = interner.union(vec![t_type, generic_fn]);
    assert!(
        contains_free_type_parameters_db(&interner, mixed),
        "free outer T alongside a generic signature must be detected"
    );
    // Sanity: the leaf `T` is still a free param under the content-predicate
    // contract used by other walkers.
    assert!(contains_type_matching(&interner, t_type, |k| matches!(
        k,
        TypeData::TypeParameter(_)
    )));
}
