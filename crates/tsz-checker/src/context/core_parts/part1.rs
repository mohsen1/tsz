use rustc_hash::{FxHashMap, FxHashSet};

#[cfg(test)]
use std::cell::RefCell;

use std::collections::VecDeque;

use std::sync::Arc;

use crate::control_flow::FlowGraph;

use crate::module_resolution::build_file_name_index;

use tsz_binder::symbols::StableLocation;

use tsz_binder::{BinderState, SymbolId};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeArena;

use tsz_solver::TypeId;

use super::{CheckerContext, LibContext, ResolutionError, TypeCache};

/// Kill-switch for order-independent cross-file alias/`export =` resolution.
///
/// When enabled (default), overlay writes that record a symbol's owning file
/// prefer the stable, immutable `global_symbol_file_index` (declaring file)
/// before consulting the monotonically-growing dynamic overlay, so the same
/// `(file, symbol)` resolves to the same endpoint regardless of processing
/// order. Set `TSZ_DISABLE_ORDER_INDEP_RESOLUTION=1` to restore the legacy
/// dynamic-first behaviour for a clean A/B comparison (refs #7574, #12148).
///
/// Cached in a `OnceLock` so the environment is read at most once per process.
pub(crate) fn order_independent_resolution_disabled() -> bool {
    use std::sync::OnceLock;
    static DISABLED: OnceLock<bool> = OnceLock::new();
    *DISABLED.get_or_init(|| {
        std::env::var("TSZ_DISABLE_ORDER_INDEP_RESOLUTION")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
    })
}

#[cfg(test)]
thread_local! {
    static TYPE_NODE_RESOLUTION_COUNTS: RefCell<FxHashMap<NodeIndex, u32>> =
        RefCell::new(FxHashMap::default());
}

impl TypeCache {
    /// Invalidate cached symbol types that depend on the provided roots.
    /// Returns the number of affected symbols.
    pub fn invalidate_symbols(&mut self, roots: &[SymbolId]) -> usize {
        if roots.is_empty() {
            return 0;
        }

        let mut reverse: FxHashMap<SymbolId, Vec<SymbolId>> = FxHashMap::default();
        for (symbol, deps) in &self.symbol_dependencies {
            for dep in deps {
                reverse.entry(*dep).or_default().push(*symbol);
            }
        }

        let mut affected: FxHashSet<SymbolId> = FxHashSet::default();
        let mut pending = VecDeque::new();
        for &root in roots {
            if affected.insert(root) {
                pending.push_back(root);
            }
        }

        while let Some(sym_id) = pending.pop_front() {
            if let Some(dependents) = reverse.get(&sym_id) {
                for &dependent in dependents {
                    if affected.insert(dependent) {
                        pending.push_back(dependent);
                    }
                }
            }
        }

        for sym_id in &affected {
            self.symbol_types.remove(sym_id);
            self.symbol_instance_types.remove(sym_id);
            self.symbol_dependencies.remove(sym_id);
        }
        self.node_types.clear();
        self.class_instance_type_cache.clear();
        self.class_constructor_type_cache.clear();
        self.class_instance_type_to_decl.clear();
        affected.len()
    }

    /// Merge another `TypeCache` into this one.
    /// Used to accumulate type information from multiple file checks for declaration emit.
    pub fn merge(&mut self, other: Self) {
        self.symbol_types.extend(other.symbol_types);
        self.symbol_instance_types
            .extend(other.symbol_instance_types);
        self.node_types.extend(other.node_types.iter());
        self.class_instance_type_to_decl
            .extend(other.class_instance_type_to_decl);
        self.class_instance_type_cache
            .extend(other.class_instance_type_cache);
        self.class_constructor_type_cache
            .extend(other.class_constructor_type_cache);
        self.type_only_nodes.extend(other.type_only_nodes);
        self.namespace_module_names
            .extend(other.namespace_module_names);

        // Merge symbol dependencies sets
        for (sym, deps) in other.symbol_dependencies {
            self.symbol_dependencies
                .entry(sym)
                .or_default()
                .extend(deps);
        }

        // Merge def_to_symbol and def_to_name mappings
        self.def_to_symbol.extend(other.def_to_symbol);
        self.def_to_name.extend(other.def_to_name);
        self.def_types.extend(other.def_types);
        self.def_type_params.extend(other.def_type_params);
        self.well_known_symbol_names
            .extend(other.well_known_symbol_names);
        self.boxed_types.extend(other.boxed_types);
        for (kind, def_ids) in other.boxed_def_ids {
            self.boxed_def_ids.entry(kind).or_default().extend(def_ids);
        }
    }
}
