//! Boundary aliases for stable solver definition identity.

use tsz_binder::SymbolId;
use tsz_solver::SymbolRef;
use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;

pub(crate) use super::common::lazy_def_id;
pub(crate) type DefId = tsz_solver::def::DefId;

/// Convert a solver `SymbolRef` that explicitly represents a binder symbol
/// reference, such as `typeof X`/`TypeQuery`, through the checker-owned identity
/// boundary.
///
/// Keep raw ordinal reinterpretation centralized here while the checker still
/// carries legacy `SymbolRef` surfaces. New code should prefer `DefId`-backed
/// identity where available, then use this bridge only for true symbol-ref
/// query surfaces.
pub(crate) const fn symbol_ref_to_symbol_id(symbol: SymbolRef) -> SymbolId {
    SymbolId(symbol.0)
}

pub(crate) fn is_lazy_def_identity(db: &dyn TypeDatabase, type_id: TypeId, def_id: DefId) -> bool {
    tsz_solver::type_queries::get_lazy_def_id(db, type_id) == Some(def_id)
}

pub(crate) fn type_has_well_known_typed_array_name(
    db: &dyn TypeDatabase,
    def_store: &tsz_solver::DefinitionStore,
    type_id: TypeId,
) -> bool {
    let Some(name) = tsz_solver::type_queries::get_lazy_def_id(db, type_id)
        .or_else(|| def_store.find_def_for_type(type_id))
        .and_then(|def_id| def_store.get_name(def_id))
    else {
        return false;
    };
    matches!(
        db.resolve_atom_ref(name).as_ref(),
        "Int8Array"
            | "Uint8Array"
            | "Uint8ClampedArray"
            | "Int16Array"
            | "Uint16Array"
            | "Int32Array"
            | "Uint32Array"
            | "Float32Array"
            | "Float64Array"
            | "BigInt64Array"
            | "BigUint64Array"
    )
}
