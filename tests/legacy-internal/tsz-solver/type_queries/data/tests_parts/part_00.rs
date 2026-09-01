/// The project-cached content walker (`contains_*_db`) and the generic
/// uncached `contains_type_matching` walk must give identical answers: both
/// are drivers over the same canonical `CONTENT_PREDICATE` child enumeration.
/// This replaces the old "must mirror `check_key` exactly" comment contract
/// with an executable check over a generated shape corpus, driven by the REAL
/// `ContentPredicate` impls so predicate edits cannot desynchronize the pin.
#[test]
fn cached_content_walker_agrees_with_generic_walker_on_corpus() {
    use super::content_predicates::{
        ConditionalPredicate, ContentPredicate, InferPredicate, LazyOrRecursivePredicate,
        SubstitutionDependentPredicate, ThisTypePredicate, TypeQueryPredicate,
    };
    use crate::visitors::visitor_predicates::contains_type_matching;

    let interner = TypeInterner::new();
    let corpus = content_walk_agreement_corpus(&interner);
    assert!(corpus.len() > 100);

    fn assert_agreement<P: ContentPredicate>(
        interner: &TypeInterner,
        corpus: &[TypeId],
        predicate: &P,
        cached_query: impl Fn(&TypeInterner, TypeId) -> bool,
        label: &str,
    ) {
        for &root in corpus {
            let cached = cached_query(interner, root);
            let generic =
                contains_type_matching(interner, root, |key| predicate.matches_node(interner, key));
            assert_eq!(cached, generic, "{label} mismatch on {root:?}");
        }
    }

    assert_agreement(
        &interner,
        &corpus,
        &InferPredicate,
        |i, t| contains_infer_types_db(i, t),
        "contains_infer",
    );
    assert_agreement(
        &interner,
        &corpus,
        &TypeQueryPredicate,
        |i, t| contains_type_query_db(i, t),
        "contains_type_query",
    );
    assert_agreement(
        &interner,
        &corpus,
        &LazyOrRecursivePredicate,
        |i, t| contains_lazy_or_recursive_db(i, t),
        "contains_lazy_or_recursive",
    );
    assert_agreement(
        &interner,
        &corpus,
        &ThisTypePredicate,
        |i, t| contains_this_type_db(i, t),
        "contains_this",
    );
    assert_agreement(
        &interner,
        &corpus,
        &ConditionalPredicate,
        |i, t| contains_conditional_type(i, t),
        "contains_conditional",
    );
    assert_agreement(
        &interner,
        &corpus,
        &SubstitutionDependentPredicate,
        |i, t| is_substitution_dependent_type(i, t),
        "substitution-dependent",
    );
}

/// `has_policy_children` must stay in lockstep with the canonical enumerator:
/// whenever it reports a node as terminal under a policy, the enumerator must
/// yield zero children for that node under the same policy. A `false` from
/// `has_policy_children` while children exist would make walkers silently
/// skip subtrees behind their memo/terminal fast paths.
#[test]
fn has_policy_children_matches_enumerator_on_corpus() {
    use crate::visitors::child_policy::{
        ChildPolicy, for_each_child_with_policy, has_policy_children,
    };

    let interner = TypeInterner::new();
    let corpus = content_walk_agreement_corpus(&interner);
    let policies = [
        ("FULL", ChildPolicy::FULL),
        ("EVERYTHING", ChildPolicy::EVERYTHING),
        ("CONTENT_PREDICATE", ChildPolicy::CONTENT_PREDICATE),
        ("FREE_TYPE_PARAMS", ChildPolicy::FREE_TYPE_PARAMS),
        ("FREE_INFER", ChildPolicy::FREE_INFER),
        ("FREE_PARAM_COLLECT", ChildPolicy::FREE_PARAM_COLLECT),
        ("STRUCTURAL_USES", ChildPolicy::STRUCTURAL_USES),
        ("ERROR_CONTAINMENT", ChildPolicy::ERROR_CONTAINMENT),
        ("SHALLOW", ChildPolicy::SHALLOW),
        (
            "STRUCTURAL_USES_SHALLOW",
            ChildPolicy::STRUCTURAL_USES_SHALLOW,
        ),
    ];
    for &root in &corpus {
        let Some(key) = interner.lookup(root) else {
            continue;
        };
        for (name, policy) in &policies {
            if has_policy_children(&key, policy) {
                continue;
            }
            let mut children = 0usize;
            for_each_child_with_policy(&interner, &key, policy, |_| children += 1);
            assert_eq!(
                children, 0,
                "has_policy_children claims terminal under {name} but enumerator \
                 yields {children} children for {root:?}"
            );
        }
    }
}

/// `contains_error_type_db` and the visitor-side `contains_error_type` are one
/// canonical walk: every nested error position — application args, application
/// bases, the raw `TypeId::ERROR` sentinel, and wrapper kinds — must be
/// detected identically through both entry points.
#[test]
fn error_containment_is_unified_across_query_paths() {
    let interner = TypeInterner::new();

    let cases = [
        (TypeId::ERROR, true),
        (
            interner.application(interner.lazy(crate::def::DefId(7)), vec![TypeId::ERROR]),
            true,
        ),
        (
            interner.application(TypeId::ERROR, vec![TypeId::STRING]),
            true,
        ),
        // Deferred operations are opaque: an error inside a keyof/conditional
        // operand is only real once evaluation selects it.
        (interner.keyof(TypeId::ERROR), false),
        (interner.array(TypeId::ERROR), true),
        (interner.union(vec![TypeId::STRING, TypeId::NUMBER]), false),
        (interner.array(TypeId::STRING), false),
    ];
    for (root, expected) in cases {
        assert_eq!(
            contains_error_type_db(&interner, root),
            expected,
            "contains_error_type_db on {root:?}"
        );
        assert_eq!(
            crate::visitors::visitor_predicates::contains_error_type(&interner, root),
            expected,
            "visitor contains_error_type on {root:?}"
        );
    }
}

// =============================================================================
// contains_file_relative_content_db
// =============================================================================

/// Direct file-relative roots: every variant whose meaning depends on the
/// producing file or lexical scope must be flagged.
#[test]
fn file_relative_content_flags_direct_roots() {
    use crate::types::SymbolRef;
    let interner = TypeInterner::new();

    let unresolved = interner.unresolved_type_name(interner.intern_string("LocalName"));
    let type_query = interner.type_query(SymbolRef(7));
    let unique_symbol = interner.unique_symbol(SymbolRef(7));
    let module_ns = interner.module_namespace(SymbolRef(7));
    let this_type = interner.this_type();

    for ty in [unresolved, type_query, unique_symbol, module_ns, this_type] {
        assert!(
            contains_file_relative_content_db(&interner, ty),
            "expected file-relative root to be flagged"
        );
    }
}

/// File-relative content nested inside structural types is found by the deep
/// walk (union member, array element, tuple element).
#[test]
fn file_relative_content_flags_nested_content() {
    use crate::types::{SymbolRef, TupleElement};
    let interner = TypeInterner::new();

    let type_query = interner.type_query(SymbolRef(3));
    let in_union = interner.union(vec![TypeId::STRING, type_query]);
    assert!(contains_file_relative_content_db(&interner, in_union));

    let unresolved = interner.unresolved_type_name(interner.intern_string("Gaps"));
    let in_array = interner.array(unresolved);
    assert!(contains_file_relative_content_db(&interner, in_array));

    let in_tuple = interner.tuple(vec![TupleElement {
        type_id: interner.this_type(),
        optional: false,
        rest: false,
        name: None,
    }]);
    assert!(contains_file_relative_content_db(&interner, in_tuple));
}

/// Program-global content is NOT file-relative: intrinsics, literals,
/// `Lazy(DefId)` references, and applications of lazy bases over concrete
/// args all have one program-wide meaning through the shared store.
#[test]
fn file_relative_content_accepts_program_global_types() {
    use crate::def::DefId;
    let interner = TypeInterner::new();

    let literal = interner.literal_string("transformation");
    let lazy = interner.lazy(DefId(42));
    let app = interner.application(lazy, vec![TypeId::STRING, literal]);
    let union = interner.union(vec![TypeId::NUMBER, app]);
    let arr = interner.array(union);

    for ty in [TypeId::STRING, literal, lazy, app, union, arr] {
        assert!(
            !contains_file_relative_content_db(&interner, ty),
            "expected program-global type to be shareable"
        );
    }
}

/// The memoized walk returns consistent answers on repeat queries (the
/// per-node results live in the shared interner cache).
#[test]
fn file_relative_content_is_stable_across_repeat_queries() {
    use crate::types::SymbolRef;
    let interner = TypeInterner::new();

    let tainted = interner.union(vec![TypeId::STRING, interner.type_query(SymbolRef(9))]);
    let clean = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    for _ in 0..3 {
        assert!(contains_file_relative_content_db(&interner, tainted));
        assert!(!contains_file_relative_content_db(&interner, clean));
    }
}
