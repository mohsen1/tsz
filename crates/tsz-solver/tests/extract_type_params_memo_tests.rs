//! Memoization and pruning guardrails for `extract_type_params_from_type`.
//!
//! Split from `evaluation/evaluate/support.rs` to keep that shard under the
//! repository 2000-line file ceiling.

use super::TypeEvaluator;
use crate::intern::TypeInterner;
use crate::types::{TypeId, TypeParamInfo, TypeParamOrigin};

#[test]
fn memoizes_extract_type_params_per_type_id() {
    let interner = TypeInterner::new();
    let t_atom = interner.intern_string("T");
    let t_param = interner.type_param(TypeParamInfo {
        name: t_atom,
        constraint: None,
        default: None,
        is_const: false,
        origin: TypeParamOrigin::User,
    });
    // `T[]` forces the walk to descend one structural level before
    // collecting the parameter, so the memo covers a non-leaf type.
    let arr = interner.array(t_param);

    // Nothing cached before the first extraction.
    assert!(interner.extract_type_params_memo(arr).is_none());

    let ev = TypeEvaluator::new(&interner);
    let first = ev.extract_type_params_from_type(arr);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].name, t_atom);

    // The first extraction populated the shared interner memo.
    let cached = interner
        .extract_type_params_memo(arr)
        .expect("memo populated after first extraction");
    assert_eq!(cached.len(), 1);
    assert_eq!(cached[0].name, t_atom);

    // A second extraction (served from the memo) is identical.
    let second = ev.extract_type_params_from_type(arr);
    assert_eq!(second.len(), first.len());
    assert_eq!(second[0].name, first[0].name);
}

#[test]
fn intrinsic_types_bypass_the_memo() {
    let interner = TypeInterner::new();
    let ev = TypeEvaluator::new(&interner);
    assert!(ev.extract_type_params_from_type(TypeId::NUMBER).is_empty());
    // Intrinsics short-circuit before touching the cache.
    assert!(interner.extract_type_params_memo(TypeId::NUMBER).is_none());
}

fn param(interner: &TypeInterner, name: &str) -> (tsz_common::interner::Atom, TypeId) {
    let atom = interner.intern_string(name);
    let id = interner.type_param(TypeParamInfo {
        name: atom,
        constraint: None,
        default: None,
        is_const: false,
        origin: TypeParamOrigin::User,
    });
    (atom, id)
}

/// The reachability gate must not change what the collector finds: a
/// parameter buried under concrete wrappers is still collected, for any
/// binder name.
#[test]
fn pruning_gate_preserves_deep_collection_across_binder_names() {
    let interner = TypeInterner::new();
    let ev = TypeEvaluator::new(&interner);
    for name in ["T", "Elem", "Zeta"] {
        let (atom, tp) = param(&interner, name);
        // union(string, { p: name[] })
        let arr = interner.array(tp);
        let obj = interner.object(vec![crate::types::PropertyInfo::new(
            interner.intern_string("p"),
            arr,
        )]);
        let root = interner.union2(TypeId::STRING, obj);
        let found = ev.extract_type_params_from_type(root);
        assert_eq!(found.len(), 1, "binder {name} must be found");
        assert_eq!(found[0].name, atom);
    }
}

/// Fully concrete subtrees are pruned to an empty result (the gate answers
/// `false`), matching the collector's historical answer.
#[test]
fn pruning_gate_concrete_subtree_collects_nothing() {
    let interner = TypeInterner::new();
    let ev = TypeEvaluator::new(&interner);
    let obj = interner.object(vec![crate::types::PropertyInfo::new(
        interner.intern_string("p"),
        interner.array(TypeId::STRING),
    )]);
    let root = interner.union2(TypeId::NUMBER, obj);
    assert!(ev.extract_type_params_from_type(root).is_empty());
    assert!(!crate::type_queries::contains_extractable_type_params_db(
        &interner, root
    ));
}

/// A `Callable` that *declares* signature type parameters without
/// referencing them in any child position is still collected — the gate's
/// node-level `Callable` match must cover the declaration-only case the
/// collector reads.
#[test]
fn pruning_gate_keeps_declaration_only_callable_type_params() {
    use crate::types::{CallSignature, CallableShape};
    let interner = TypeInterner::new();
    let ev = TypeEvaluator::new(&interner);
    for name in ["T", "Widget"] {
        let atom = interner.intern_string(name);
        let info = TypeParamInfo {
            name: atom,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::User,
        };
        // `<name>() => void` — the parameter is declared but unused, so no
        // `TypeParameter` node is reachable anywhere in the shape.
        let callable = interner.callable(CallableShape {
            call_signatures: vec![CallSignature {
                type_params: vec![info],
                params: vec![],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
                declaration_group: 0,
            }],
            construct_signatures: vec![],
            properties: vec![],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });
        let root = interner.union2(TypeId::STRING, callable);
        let found = ev.extract_type_params_from_type(root);
        assert_eq!(found.len(), 1, "declared-only binder {name} kept");
        assert_eq!(found[0].name, atom);
    }
}

/// The collector descends `Application` bases, so the gate must too: a
/// parameter reachable only through a structural application base is
/// still found.
#[test]
fn pruning_gate_covers_application_base() {
    let interner = TypeInterner::new();
    let ev = TypeEvaluator::new(&interner);
    let (atom, tp) = param(&interner, "Q");
    let base = interner.object(vec![crate::types::PropertyInfo::new(
        interner.intern_string("b"),
        tp,
    )]);
    let app = interner.application(base, vec![TypeId::STRING]);
    let found = ev.extract_type_params_from_type(app);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, atom);
}
