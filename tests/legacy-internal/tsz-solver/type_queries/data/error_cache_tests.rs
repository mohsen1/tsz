//! Parity tests for the memoized error-containment query.
//!
//! `contains_error_type` / `contains_error_type_db` was changed (#15729) to
//! memoize its deep `ERROR_CONTAINMENT`-policy walk per node in the
//! project-wide `ContainsError` predicate-cache slot, mirroring the sibling
//! `Contains*` predicates, instead of running an ephemeral `DeepContainsChecker`
//! whose memo was discarded every call. These tests pin that the cached answer
//! is byte-identical to a fresh uncached walk over the same policy (including
//! the intrinsic-range `TypeId::ERROR` sentinel), that re-querying the same id
//! is stable (cache hits return the same value), and that the
//! `ERROR_CONTAINMENT` semantics — `Application` bases count, but the operands
//! of deferred conditional/mapped/indexed-access/`keyof` operations and
//! type-parameter declaration metadata do not — survive the cache.

use crate::intern::TypeInterner;
use crate::types::{
    ConditionalType, MappedType, ParamInfo, PropertyInfo, TupleElement, TypeData, TypeId,
    TypeParamInfo, Visibility,
};
use crate::visitors::child_policy::ChildPolicy;
use crate::visitors::visitor_predicates::contains_error_type;

use super::error_predicate::contains_error_type_db;

/// Reference oracle: a fresh, fully-uncached `ERROR_CONTAINMENT`-policy
/// containment walk with the `TypeId::ERROR` sentinel matched before the
/// intrinsic fast path. This re-derives the semantics independently of the
/// cached path, so any divergence from `contains_error_type` is a cache bug.
fn reference_contains_error(interner: &TypeInterner, root: TypeId) -> bool {
    use crate::visitors::child_policy::{for_each_child_with_policy, has_policy_children};
    use rustc_hash::FxHashSet;

    let mut stack = vec![root];
    let mut visited = FxHashSet::default();
    while let Some(current) = stack.pop() {
        // The sentinel sits in the intrinsic id range, so it is matched before
        // the intrinsic skip — an id-level match is the only way to see it.
        if current == TypeId::ERROR {
            return true;
        }
        if current.is_intrinsic() || !visited.insert(current) {
            continue;
        }
        let Some(key) = interner.lookup(current) else {
            continue;
        };
        if matches!(key, TypeData::Error | TypeData::UnresolvedTypeName(_)) {
            return true;
        }
        if !has_policy_children(&key, &ChildPolicy::ERROR_CONTAINMENT) {
            continue;
        }
        for_each_child_with_policy(interner, &key, &ChildPolicy::ERROR_CONTAINMENT, |child| {
            stack.push(child);
        });
    }
    false
}

fn property(name: tsz_common::Atom, read: TypeId, write: TypeId) -> PropertyInfo {
    PropertyInfo {
        name,
        type_id: read,
        write_type: write,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }
}

fn build_corpus(interner: &TypeInterner) -> Vec<TypeId> {
    let t = interner.type_param(TypeParamInfo::simple(interner.intern_string("T")));
    let unresolved = interner.unresolved_type_name(interner.intern_string("Missing"));
    let p = interner.intern_string("p");

    // Both a nested sentinel and a nested `UnresolvedTypeName` are matches; a
    // plain scalar is not. Each is exercised in every committed-structure
    // position below.
    let error_leaves = [TypeId::ERROR, unresolved];
    let clean_leaves = [TypeId::STRING, t];

    let mut corpus = Vec::new();
    for leaf in error_leaves.into_iter().chain(clean_leaves) {
        corpus.push(leaf);
        corpus.push(interner.array(leaf));
        corpus.push(interner.union(vec![TypeId::NUMBER, leaf]));
        corpus.push(interner.tuple(vec![TupleElement::fixed(leaf)]));
        corpus.push(interner.object(vec![PropertyInfo::new(p, leaf)]));
        // A leaf only in a property *write* type: `ERROR_CONTAINMENT` visits
        // write types (unlike `CONTENT_PREDICATE`), so this must still be found.
        corpus.push(interner.object(vec![property(p, TypeId::STRING, leaf)]));
        // Application args are always committed structure.
        corpus.push(interner.application(interner.lazy(crate::def::DefId(7)), vec![leaf]));
        // Application *base* is committed under `ERROR_CONTAINMENT`.
        corpus.push(interner.application(leaf, vec![TypeId::STRING]));
        // A function parameter and return position.
        corpus.push(interner.function(crate::types::FunctionShape::new(
            vec![ParamInfo::unnamed(leaf)],
            TypeId::VOID,
        )));
        // Two levels deep, with a shared subtree so the per-node memo is
        // exercised across nesting.
        let nested = interner.array(interner.union(vec![leaf, interner.array(leaf)]));
        corpus.push(nested);

        // Deferred operands are opaque: a leaf reachable only through a
        // conditional/mapped/indexed-access/`keyof` operand must NOT be found,
        // even when it would be found in committed structure.
        corpus.push(interner.keyof(leaf));
        corpus.push(interner.index_access(leaf, TypeId::STRING));
        corpus.push(interner.conditional(ConditionalType {
            check_type: leaf,
            extends_type: leaf,
            true_type: leaf,
            false_type: leaf,
            is_distributive: false,
        }));
        corpus.push(interner.mapped(MappedType {
            type_param: TypeParamInfo::simple(interner.intern_string("K")),
            constraint: interner.keyof(leaf),
            name_type: None,
            template: leaf,
            readonly_modifier: None,
            optional_modifier: None,
        }));
        // A bare type parameter's constraint is declaration metadata, not a
        // committed use — an error there must not poison the parameter.
        corpus.push(interner.type_param(TypeParamInfo {
            constraint: Some(leaf),
            ..TypeParamInfo::simple(interner.intern_string("U"))
        }));
    }

    corpus.push(interner.recursive(0));
    corpus.push(interner.array(interner.recursive(1)));
    corpus
}

#[test]
fn cached_error_query_matches_uncached_reference() {
    let interner = TypeInterner::new();
    for root in build_corpus(&interner) {
        let expected = reference_contains_error(&interner, root);
        // First query populates the cache; second must hit it with the same
        // answer. Both public entry points must agree with the oracle.
        let first = contains_error_type_db(&interner, root);
        let second = contains_error_type_db(&interner, root);
        let via_visitor = contains_error_type(&interner, root);
        assert_eq!(
            first,
            expected,
            "cached contains_error_type_db disagreed with reference for {:?}",
            interner.lookup(root)
        );
        assert_eq!(
            second,
            first,
            "cached contains_error_type_db not stable on re-query for {:?}",
            interner.lookup(root)
        );
        assert_eq!(
            via_visitor,
            expected,
            "visitor contains_error_type disagreed with reference for {:?}",
            interner.lookup(root)
        );
    }
}

#[test]
fn nested_error_behind_a_shared_subtree_is_cached_consistently() {
    let interner = TypeInterner::new();
    let inner = interner.array(TypeId::ERROR);
    // The same interned `inner` appears twice; the per-node cache populated by
    // the first branch must give the identical answer for the second.
    let outer = interner.union(vec![
        inner,
        interner.tuple(vec![TupleElement::fixed(inner)]),
    ]);
    assert!(contains_error_type_db(&interner, outer));
    assert!(contains_error_type_db(&interner, inner));
    // Re-query after the cache is warm.
    assert!(contains_error_type_db(&interner, outer));
}
