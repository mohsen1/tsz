//! Regression: a library interface's heritage (`extends`) members must
//! materialize even when the shared checker recursion budget is already
//! saturated by *unrelated* deep recursion.
//!
//! Structural rule: `SetIterator<T>` / `MapIterator<T>` / `ArrayIterator<T>` /
//! `IterableIterator<T>` inherit `next` (and `return`/`throw`) from `Iterator`
//! through a short, name-cycle-guarded `extends` chain
//! (`SetIterator -> IteratorObject -> Iterator`). `tsc` always exposes those
//! inherited members. tsz used to merge that heritage under the *shared*
//! `CheckerRecursion` depth counter (limit 50), which unrelated deep recursion
//! (e.g. immer's mutually-recursive `Drafted`/`SetState` graph) can exhaust;
//! when it did, the merge bailed to an own-members-only ("heritage-thin") body
//! that dropped the inherited `next`, producing schedule/context-dependent
//! `TS2741`/`TS2345` false positives (issue #13942 FP#3/#4/#7, also seen on the
//! `superstruct` and `zustand` canaries).
//!
//! The heritage graph is shallow and bounded by the name-cycle guard
//! (`lib_heritage_in_progress`), so it must be tracked by a dedicated, local
//! depth budget rather than the global counter polluted by unrelated work.

use crate::context::{CheckerOptions, LibContext};
use crate::query_boundaries::common::TypeInterner;
use crate::query_boundaries::property_access::type_has_property;
use crate::state::CheckerState;
use crate::test_utils::load_default_lib_files;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;

/// Resolve `lib_name` and report whether the materialized body exposes
/// `member`. When `saturate_recursion` is set, the shared `CheckerRecursion`
/// budget is driven to its limit first — mimicking a resolution that happens
/// deep inside unrelated recursion.
fn lib_member_visible(lib_name: &str, member: &str, saturate_recursion: bool) -> Option<bool> {
    let lib_files = load_default_lib_files();
    if lib_files.is_empty() {
        // Stripped lib assets unavailable in this environment; skip.
        return None;
    }

    let mut parser = ParserState::new("test.ts".to_string(), "export {};".to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());
    checker.check_source_file(root);

    if saturate_recursion {
        // Saturate the shared CheckerRecursion budget exactly the way unrelated
        // deep recursion (e.g. immer's recursive Drafted/SetState graph) does,
        // leaving no headroom for the heritage merge. `base_depth` is set to the
        // saturated value so the counter's drop-time leak assertion is satisfied.
        let max = checker.ctx.recursion_depth.borrow().max_depth();
        *checker.ctx.recursion_depth.borrow_mut() =
            tsz_solver::recursion::DepthCounter::with_initial_depth(max, max);
    }

    let ty = checker.resolve_lib_type_by_name(lib_name)?;
    let atom = types.intern_string(member);
    Some(type_has_property(&types, ty, atom))
}

#[test]
fn set_iterator_inherits_next_at_shallow_depth() {
    // Control: at the top level the inherited member is always present.
    let Some(visible) = lib_member_visible("SetIterator", "next", false) else {
        return;
    };
    assert!(
        visible,
        "SetIterator must expose the inherited `next` member at shallow depth"
    );
}

/// The bug: with the shared recursion budget saturated by unrelated recursion,
/// the heritage merge dropped `next`. The dedicated lib-heritage budget keeps
/// the inherited member visible for every derived iterator interface.
fn assert_inherits_next_under_recursion_pressure(lib_name: &str) {
    let Some(visible) = lib_member_visible(lib_name, "next", true) else {
        return;
    };
    assert!(
        visible,
        "{lib_name} must expose the inherited `next` member even when the shared \
         checker recursion budget is saturated by unrelated recursion"
    );
}

#[test]
fn set_iterator_inherits_next_under_recursion_pressure() {
    assert_inherits_next_under_recursion_pressure("SetIterator");
}

#[test]
fn map_iterator_inherits_next_under_recursion_pressure() {
    assert_inherits_next_under_recursion_pressure("MapIterator");
}

#[test]
fn array_iterator_inherits_next_under_recursion_pressure() {
    assert_inherits_next_under_recursion_pressure("ArrayIterator");
}

#[test]
fn iterable_iterator_inherits_next_under_recursion_pressure() {
    assert_inherits_next_under_recursion_pressure("IterableIterator");
}
