//! Cross-binder raw-`SymbolId` ambiguity.
//!
//! `SymbolId` is a per-arena handle. Whether it is globally unique depends on
//! how the program was assembled: the production driver hands the checker
//! globally-remapped ids, but any assembly that binds files independently
//! leaves each binder numbering its own symbols from `0`. In that second case
//! the *same* raw id names a different declaration in every binder, so a
//! `SymbolId -> file_idx` answer is meaningless — there is no single owner to
//! name.
//!
//! A contested id must therefore resolve to "unknown" and let callers take
//! their local/name-based fallback, never to an arbitrary last-writer file.
//! Resolving one anyway is the `#15983` false-`TS2538` family: an import
//! alias whose own raw id happens to collide with an unrelated symbol in
//! another binder materializes as *that* symbol's type.

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

use tsz_binder::{BinderState, SymbolId};

/// Raw `SymbolId`s declared by two or more of `binders`.
///
/// An empty set means every id names exactly one declaration program-wide,
/// which is the normal state after the driver's global remap — so gating on
/// this set costs nothing where ids are already unique and only bites where
/// they genuinely collide.
#[must_use]
pub fn build_contested_symbol_ids(binders: &[Arc<BinderState>]) -> FxHashSet<SymbolId> {
    let mut owner: FxHashMap<SymbolId, usize> = FxHashMap::default();
    let mut contested = FxHashSet::default();
    for (file_idx, binder) in binders.iter().enumerate() {
        for symbol in binder.symbols.iter() {
            match owner.entry(symbol.id) {
                std::collections::hash_map::Entry::Vacant(slot) => {
                    slot.insert(file_idx);
                }
                std::collections::hash_map::Entry::Occupied(slot) => {
                    if *slot.get() != file_idx {
                        contested.insert(symbol.id);
                    }
                }
            }
        }
    }
    contested
}
