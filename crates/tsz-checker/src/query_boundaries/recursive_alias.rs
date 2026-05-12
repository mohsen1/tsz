//! Recursive type-alias detection for diagnostic display policies.
//!
//! Used by TS2322-family diagnostic formatters to decide when expanding a
//! generic alias body would produce an unbounded `[..., ...]` cascade rather
//! than a useful structural form. tsc keeps the alias annotation in those
//! cases; this helper exposes the structural rule so the checker side can
//! match that policy without pattern-matching solver internals directly.

use rustc_hash::FxHashSet;
use tsz_common::interner::Atom;
use tsz_solver::TypeDatabase;
use tsz_solver::TypeId;
use tsz_solver::def::{DefId, DefKind, DefinitionStore};

/// Returns the non-generic recursive type alias whose registered body is
/// `type_id`, if any.
///
/// Handles cases like `type Box2 = Box<Box2 | number>` where the alias annotation
/// `const x: Box2` resolves to the Application body type, NOT `Lazy(Box2_def)`.
/// tsc preserves "Box2" in TS2322 messages; this helper exposes the structural
/// rule so the checker can match that policy without hardcoding alias names.
pub(crate) fn recursive_non_generic_type_alias_body_def(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
) -> Option<DefId> {
    // Find the alias whose body is registered as this exact type.
    let def_id = def_store.find_type_alias_by_body(type_id)?;
    let def = def_store.get(def_id)?;
    // Only applies to non-generic type aliases; generic aliases (with type parameters)
    // are handled by is_recursive_type_alias_application which preserves "Alias<Args>".
    if def.kind != DefKind::TypeAlias || !def.type_params.is_empty() {
        return None;
    }
    let body = def.body?;
    // Verify the body recursively references the alias itself.
    let mut visited: FxHashSet<TypeId> = FxHashSet::default();
    type_reaches_alias_def(db, body, def_id, &mut visited).then_some(def_id)
}

pub(crate) fn recursive_non_generic_type_alias_body_name(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
) -> Option<Atom> {
    let def_id = recursive_non_generic_type_alias_body_def(db, def_store, type_id)?;
    def_store.get(def_id).map(|def| def.name)
}

/// True when `def_id` refers to a non-generic `TypeAlias` whose body recursively
/// references the same alias.
///
/// Used to decide whether to preserve a `Lazy(def_id)` alias name in TS2322
/// messages rather than following the evaluated `display_alias` to an expanded
/// Application form. tsc shows "Box2" not "Box<number | Box2>" for
/// `type Box2 = Box<Box2 | number>` in error messages.
pub(crate) fn is_def_non_generic_recursive_alias(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    def_id: DefId,
) -> bool {
    let Some(def) = def_store.get(def_id) else {
        return false;
    };
    if def.kind != DefKind::TypeAlias || !def.type_params.is_empty() {
        return false;
    }
    let Some(body) = def.body else {
        return false;
    };
    let mut visited: FxHashSet<TypeId> = FxHashSet::default();
    type_reaches_alias_def(db, body, def_id, &mut visited)
}

/// True when `type_id` is `Application(Lazy(D), args)` and the alias body of
/// `D` reaches another reference to `D` (directly via `Lazy(D)` or via
/// `Application(Lazy(D), _)`, possibly through nested types).
///
/// The diagnostic printer uses this to detect recursive type aliases whose
/// expansion produces an unbounded `[..., ...]` cascade when alias names are
/// skipped — the structural rule is "tsc keeps the alias annotation in the
/// TS2322 message for `Application(Lazy(D), args)` whenever D is recursive".
pub(crate) fn is_recursive_type_alias_application(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
) -> bool {
    let Some(def_id) = tsz_solver::type_queries::get_application_lazy_def_id(db, type_id) else {
        return false;
    };
    let Some(def) = def_store.get(def_id) else {
        return false;
    };
    if def.kind != DefKind::TypeAlias {
        return false;
    }
    let Some(body) = def.body else {
        return false;
    };
    let mut visited: FxHashSet<TypeId> = FxHashSet::default();
    type_reaches_alias_def(db, body, def_id, &mut visited)
}

fn type_reaches_alias_def(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    target_def_id: DefId,
    visited: &mut FxHashSet<TypeId>,
) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    if !visited.insert(type_id) {
        return false;
    }
    if tsz_solver::type_queries::get_lazy_def_id(db, type_id) == Some(target_def_id) {
        return true;
    }
    if tsz_solver::type_queries::get_application_lazy_def_id(db, type_id) == Some(target_def_id) {
        return true;
    }
    let mut found = false;
    tsz_solver::visitor::for_each_child_by_id(db, type_id, |child| {
        if !found {
            found = type_reaches_alias_def(db, child, target_def_id, visited);
        }
    });
    found
}
