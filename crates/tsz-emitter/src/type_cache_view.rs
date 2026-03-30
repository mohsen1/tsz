use rustc_hash::FxHashMap;
use tsz_binder::SymbolId;
use tsz_solver::types::TypeParamInfo;
use tsz_solver::{DefId, TypeId};

/// Emitter-local view of checker-produced type caches.
///
/// This keeps tsz-emitter decoupled from tsz-checker internals while still
/// accepting the minimal cache data needed for declaration usage analysis and
/// lazy type-name printing.
#[derive(Debug, Clone, Default)]
pub struct TypeCacheView {
    pub node_types: FxHashMap<u32, TypeId>,
    pub symbol_types: FxHashMap<SymbolId, TypeId>,
    pub def_to_symbol: FxHashMap<DefId, SymbolId>,
    /// Maps DefId.0 -> resolved body TypeId (from TypeEnvironment).
    /// Used by the declaration emitter to evaluate mapped types and type alias
    /// applications that reference cross-file type aliases via Lazy(DefId).
    pub def_types: FxHashMap<u32, TypeId>,
    /// Maps DefId.0 -> type parameters (from TypeEnvironment).
    /// Paired with `def_types` for type alias application evaluation.
    pub def_type_params: FxHashMap<u32, Vec<TypeParamInfo>>,
}
