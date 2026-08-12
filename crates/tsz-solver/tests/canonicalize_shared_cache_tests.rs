//! Cross-instance interior-sharing behavior of [`Canonicalizer`] (#13508).
//!
//! The shared memo must be a pure accelerator: with or without it, every
//! probe returns exactly what a fresh empty-stack canonicalizer computes.
//! Entries are persisted only for clean, empty-stack subtrees, so a
//! registration-window `Lazy` or a guard bail can never leak a stale or
//! truncated identity into another probe.

use super::*;
use crate::intern::TypeInterner;
use crate::relations::subtype::TypeEnvironment;
use crate::types::{PropertyInfo, TypeParamInfo};
use rustc_hash::FxHashMap;
use std::cell::RefCell;

fn obj(interner: &TypeInterner, name: &str, ty: TypeId) -> TypeId {
    interner.object(vec![PropertyInfo::new(interner.intern_string(name), ty)])
}

/// Two roots sharing a large interior: results with a shared memo must be
/// byte-identical to fresh, memo-less walks, and the memo must hold interior
/// entries after the first probe.
#[test]
fn shared_cache_matches_fresh_walks_and_fills_interior() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    // interior: { p: (string | { q: number[] })[] }
    let inner = obj(&interner, "q", interner.array(TypeId::NUMBER));
    let member = interner.union2(TypeId::STRING, inner);
    let interior = obj(&interner, "p", interner.array(member));
    let root_a = interner.union2(TypeId::BOOLEAN, interior);
    let root_b = obj(&interner, "wrap", interior);

    let fresh_a = Canonicalizer::new(&interner, &env).canonicalize(root_a);
    let fresh_b = Canonicalizer::new(&interner, &env).canonicalize(root_b);

    let shared = RefCell::new(FxHashMap::default());
    let shared_a = Canonicalizer::new(&interner, &env)
        .with_shared_cache(&shared)
        .canonicalize(root_a);
    // Second probe reuses the interior computed by the first.
    let shared_b = Canonicalizer::new(&interner, &env)
        .with_shared_cache(&shared)
        .canonicalize(root_b);

    assert_eq!(shared_a, fresh_a, "sharing must not change probe results");
    assert_eq!(shared_b, fresh_b, "sharing must not change probe results");
    assert!(
        shared.borrow().contains_key(&interior),
        "clean interior nodes are persisted for later probes"
    );
    assert!(
        shared.borrow().contains_key(&member),
        "sharing reaches nested members"
    );
}

/// A subtree containing a `Lazy` whose def kind is not yet known (a
/// registration-window artifact) must not be persisted; a concrete sibling
/// subtree still is.
#[test]
fn shared_cache_skips_unresolved_lazy_subtrees() {
    let interner = TypeInterner::new();
    // Empty environment: DefId(7) is unregistered, so `get_def_kind` is None.
    let env = TypeEnvironment::new();

    let unresolved = interner.lazy(crate::def::DefId(7));
    let dirty_subtree = obj(&interner, "lazy", unresolved);
    let clean_subtree = obj(&interner, "done", TypeId::STRING);
    let root = interner.union2(dirty_subtree, clean_subtree);

    let shared = RefCell::new(FxHashMap::default());
    let result = Canonicalizer::new(&interner, &env)
        .with_shared_cache(&shared)
        .canonicalize(root);
    assert_eq!(
        result,
        Canonicalizer::new(&interner, &env).canonicalize(root),
        "sharing must not change the computed form"
    );

    assert!(
        !shared.borrow().contains_key(&root),
        "a root over an unresolved def is a registration-window artifact"
    );
    assert!(
        !shared.borrow().contains_key(&dirty_subtree),
        "the unresolved subtree is never persisted"
    );
    assert!(
        shared.borrow().contains_key(&clean_subtree),
        "clean sibling subtrees are still persisted"
    );
}

/// Alpha-equivalent generic functions must keep one canonical identity when
/// probes share a memo: binder-scope interiors are computed per instance (the
/// scope stacks are non-empty there), so the erased-name form stays stable.
#[test]
fn shared_cache_preserves_alpha_equivalence_across_probes() {
    use crate::types::{FunctionShape, ParamInfo};
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let make = |name: &str| {
        let info = TypeParamInfo {
            name: interner.intern_string(name),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        };
        let pref = interner.type_param(info);
        interner.function(FunctionShape {
            type_params: vec![info],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: pref,
                optional: false,
                rest: false,
                arity_only_optional: false,
            }],
            this_type: None,
            return_type: pref,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let shared = RefCell::new(FxHashMap::default());
    let r1 = Canonicalizer::new(&interner, &env)
        .with_shared_cache(&shared)
        .canonicalize(make("T"));
    let r2 = Canonicalizer::new(&interner, &env)
        .with_shared_cache(&shared)
        .canonicalize(make("U"));
    assert_eq!(
        r1, r2,
        "alpha-equivalent generics share one canonical form under a shared memo"
    );
}
