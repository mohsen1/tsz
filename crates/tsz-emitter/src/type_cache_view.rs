use rustc_hash::FxHashMap;
use tsz_binder::SymbolId;
use tsz_solver::{DefId, TypeId};

/// Emitter-local view of checker-produced type caches.
///
/// This keeps tsz-emitter decoupled from tsz-checker internals while still
/// accepting the minimal cache data needed for declaration usage analysis and
/// lazy type-name printing.
#[derive(Debug, Clone, Default)]
pub struct TypeCacheView {
    pub node_types: FxHashMap<u32, TypeId>,
    pub def_to_symbol: FxHashMap<DefId, SymbolId>,
}
