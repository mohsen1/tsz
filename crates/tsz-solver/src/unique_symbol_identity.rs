//! Canonical identity for `unique symbol` types.
//!
//! A `unique symbol`'s `SymbolRef` must be stable across every re-derivation of
//! the same declaration and distinct from every other declaration — including
//! declarations in *other* files. This module owns the single scheme both the
//! checker's `unique symbol` type-operator construction and the lowering pass
//! use to mint that ref, so the same declaration always interns to the same
//! `UniqueSymbol` type id.

use crate::types::SymbolRef;

/// A globally-unique, source-position-stable `SymbolRef` for a `unique symbol`
/// type minted from a `(file_name, pos, end)` triple.
///
/// Keying a `unique symbol`'s identity off an arena-local node index is not
/// enough: node indices repeat across files, so a `unique symbol` declared at
/// the same local index in two different lib files (`Symbol.iterator` in
/// `lib.es2015.iterable` vs `Symbol.asyncIterator` in `lib.es2018.asynciterable`)
/// would collide into one identity. Folding the file name in restores global
/// uniqueness; folding the source span in keeps distinct declarations in one
/// file distinct. The high bit is set so these synthesized refs never collide
/// with a binder-minted `SymbolId`.
#[must_use]
pub fn unique_symbol_ref_from_source_span(file_name: &str, pos: u32, end: u32) -> SymbolRef {
    let mut hash = 0x811c_9dc5u32;
    for byte in file_name.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    for value in [pos, end] {
        hash ^= value;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    SymbolRef(hash | 0x8000_0000)
}
