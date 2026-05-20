//! Retained cache accounting for `CheckerContext`.

use rustc_hash::FxHashMap;
use std::mem;
use tsz_binder::SymbolId;
use tsz_solver::TypeId;
use tsz_solver::def::DefId;

use super::{CheckerContext, cross_file_type_params_cache_statistics};

const HASH_MAP_ENTRY_OVERHEAD_ESTIMATE: usize = 8;
const DASH_MAP_ENTRY_OVERHEAD_ESTIMATE: usize = 64;

/// Entry and size accounting for retained checker-context caches.
///
/// The counters are observational only. They make file-local and shared cache
/// residency visible to performance reports without changing lookup or
/// invalidation behavior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CheckerContextCacheStatistics {
    pub cross_file_type_params_cache_entries: usize,
    pub cross_file_type_params_cache_estimated_size_bytes: usize,
    pub lib_type_resolution_cache_entries: usize,
    pub lib_type_resolution_cache_estimated_size_bytes: usize,
    pub symbol_name_candidates_cache_entries: usize,
    pub symbol_name_candidates_cache_estimated_size_bytes: usize,
    pub lowering_entity_name_resolution_cache_entries: usize,
    pub lowering_entity_name_resolution_cache_estimated_size_bytes: usize,
    pub shared_lib_type_cache_entries: usize,
    pub shared_lib_type_cache_estimated_size_bytes: usize,
    pub flow_analysis_cache_entries: usize,
    pub flow_analysis_cache_estimated_size_bytes: usize,
    pub flow_switch_reference_cache_entries: usize,
    pub flow_switch_reference_cache_estimated_size_bytes: usize,
    pub flow_numeric_atom_cache_entries: usize,
    pub flow_numeric_atom_cache_estimated_size_bytes: usize,
    pub flow_reference_match_cache_entries: usize,
    pub flow_reference_match_cache_estimated_size_bytes: usize,
    pub js_export_surface_cache_entries: usize,
    pub js_export_surface_cache_estimated_size_bytes: usize,
    pub class_instance_type_cache_entries: usize,
    pub class_instance_type_cache_estimated_size_bytes: usize,
    pub class_constructor_type_cache_entries: usize,
    pub class_constructor_type_cache_estimated_size_bytes: usize,
    pub class_chain_summary_cache_entries: usize,
    pub class_chain_summary_cache_estimated_size_bytes: usize,
    pub env_eval_cache_entries: usize,
    pub env_eval_cache_estimated_size_bytes: usize,
    pub class_symbol_to_decl_cache_entries: usize,
    pub class_symbol_to_decl_cache_estimated_size_bytes: usize,
    pub heritage_symbol_cache_entries: usize,
    pub heritage_symbol_cache_estimated_size_bytes: usize,
    pub base_constructor_expr_cache_entries: usize,
    pub base_constructor_expr_cache_estimated_size_bytes: usize,
    pub base_instance_expr_cache_entries: usize,
    pub base_instance_expr_cache_estimated_size_bytes: usize,
    pub jsx_intrinsic_props_cache_entries: usize,
    pub jsx_intrinsic_props_cache_estimated_size_bytes: usize,
}

impl CheckerContextCacheStatistics {
    /// Estimated heap bytes retained by the accounted checker-context caches.
    #[must_use]
    pub const fn estimated_size_bytes(self) -> usize {
        self.cross_file_type_params_cache_estimated_size_bytes
            + self.lib_type_resolution_cache_estimated_size_bytes
            + self.symbol_name_candidates_cache_estimated_size_bytes
            + self.lowering_entity_name_resolution_cache_estimated_size_bytes
            + self.shared_lib_type_cache_estimated_size_bytes
            + self.flow_analysis_cache_estimated_size_bytes
            + self.flow_switch_reference_cache_estimated_size_bytes
            + self.flow_numeric_atom_cache_estimated_size_bytes
            + self.flow_reference_match_cache_estimated_size_bytes
            + self.js_export_surface_cache_estimated_size_bytes
            + self.class_instance_type_cache_estimated_size_bytes
            + self.class_constructor_type_cache_estimated_size_bytes
            + self.class_chain_summary_cache_estimated_size_bytes
            + self.env_eval_cache_estimated_size_bytes
            + self.class_symbol_to_decl_cache_estimated_size_bytes
            + self.heritage_symbol_cache_estimated_size_bytes
            + self.base_constructor_expr_cache_estimated_size_bytes
            + self.base_instance_expr_cache_estimated_size_bytes
            + self.jsx_intrinsic_props_cache_estimated_size_bytes
    }
}

impl<'a> CheckerContext<'a> {
    /// Return entry counts and estimated retained size for checker caches.
    #[must_use]
    pub fn cache_statistics(&self) -> CheckerContextCacheStatistics {
        let cross_file_type_params_cache_stats = self
            .cross_file_type_params_cache
            .as_ref()
            .map(cross_file_type_params_cache_statistics);
        let cross_file_type_params_cache_entries =
            cross_file_type_params_cache_stats.map_or(0, |stats| stats.entries);
        let cross_file_type_params_cache_estimated_size_bytes =
            cross_file_type_params_cache_stats.map_or(0, |stats| stats.estimated_size_bytes());

        let symbol_name_candidates_cache = self.symbol_name_candidates_cache.borrow();
        let lowering_entity_name_resolution_cache =
            self.lowering_entity_name_resolution_cache.borrow();
        let flow_analysis_cache = self.flow_analysis_cache.borrow();
        let flow_switch_reference_cache = self.flow_switch_reference_cache.borrow();
        let flow_numeric_atom_cache = self.flow_numeric_atom_cache.borrow();
        let flow_reference_match_cache = self.flow_reference_match_cache.borrow();
        let class_chain_summary_cache = self.class_chain_summary_cache.borrow();
        let env_eval_cache = self.env_eval_cache.borrow();
        let class_symbol_to_decl_cache = self.class_symbol_to_decl_cache.borrow();
        let heritage_symbol_cache = self.heritage_symbol_cache.borrow();
        let base_constructor_expr_cache = self.base_constructor_expr_cache.borrow();
        let base_instance_expr_cache = self.base_instance_expr_cache.borrow();

        let shared_lib_type_cache_entries = self
            .shared_lib_type_cache
            .as_ref()
            .map_or(0, |shared_lib_type_cache| shared_lib_type_cache.len());
        let shared_lib_type_cache_estimated_size_bytes = self
            .shared_lib_type_cache
            .as_ref()
            .map_or(0, |shared_lib_type_cache| {
                shared_lib_type_cache
                    .len()
                    .saturating_mul(
                        mem::size_of::<String>()
                            .saturating_add(mem::size_of::<Option<TypeId>>())
                            .saturating_add(DASH_MAP_ENTRY_OVERHEAD_ESTIMATE),
                    )
                    .saturating_add(
                        shared_lib_type_cache
                            .iter()
                            .map(|entry| entry.key().len())
                            .sum::<usize>(),
                    )
            });

        CheckerContextCacheStatistics {
            cross_file_type_params_cache_entries,
            cross_file_type_params_cache_estimated_size_bytes,
            lib_type_resolution_cache_entries: self.lib_type_resolution_cache.len(),
            lib_type_resolution_cache_estimated_size_bytes:
                string_option_type_cache_estimated_size_bytes(&self.lib_type_resolution_cache),
            symbol_name_candidates_cache_entries: symbol_name_candidates_cache.len(),
            symbol_name_candidates_cache_estimated_size_bytes:
                string_symbol_vec_cache_estimated_size_bytes(&symbol_name_candidates_cache),
            lowering_entity_name_resolution_cache_entries: lowering_entity_name_resolution_cache
                .len(),
            lowering_entity_name_resolution_cache_estimated_size_bytes:
                string_option_def_cache_estimated_size_bytes(&lowering_entity_name_resolution_cache),
            shared_lib_type_cache_entries,
            shared_lib_type_cache_estimated_size_bytes,
            flow_analysis_cache_entries: flow_analysis_cache.len(),
            flow_analysis_cache_estimated_size_bytes: fx_hash_map_estimated_size_bytes(
                &flow_analysis_cache,
            ),
            flow_switch_reference_cache_entries: flow_switch_reference_cache.len(),
            flow_switch_reference_cache_estimated_size_bytes: fx_hash_map_estimated_size_bytes(
                &flow_switch_reference_cache,
            ),
            flow_numeric_atom_cache_entries: flow_numeric_atom_cache.len(),
            flow_numeric_atom_cache_estimated_size_bytes: fx_hash_map_estimated_size_bytes(
                &flow_numeric_atom_cache,
            ),
            flow_reference_match_cache_entries: flow_reference_match_cache.len(),
            flow_reference_match_cache_estimated_size_bytes: fx_hash_map_estimated_size_bytes(
                &flow_reference_match_cache,
            ),
            js_export_surface_cache_entries: self.js_export_surface_cache.len(),
            js_export_surface_cache_estimated_size_bytes: fx_hash_map_estimated_size_bytes(
                &self.js_export_surface_cache,
            ),
            class_instance_type_cache_entries: self.class_instance_type_cache.len(),
            class_instance_type_cache_estimated_size_bytes: fx_hash_map_estimated_size_bytes(
                &self.class_instance_type_cache,
            ),
            class_constructor_type_cache_entries: self.class_constructor_type_cache.len(),
            class_constructor_type_cache_estimated_size_bytes: fx_hash_map_estimated_size_bytes(
                &self.class_constructor_type_cache,
            ),
            class_chain_summary_cache_entries: class_chain_summary_cache.len(),
            class_chain_summary_cache_estimated_size_bytes: fx_hash_map_estimated_size_bytes(
                &class_chain_summary_cache,
            ),
            env_eval_cache_entries: env_eval_cache.len(),
            env_eval_cache_estimated_size_bytes: fx_hash_map_estimated_size_bytes(&env_eval_cache),
            class_symbol_to_decl_cache_entries: class_symbol_to_decl_cache.len(),
            class_symbol_to_decl_cache_estimated_size_bytes: fx_hash_map_estimated_size_bytes(
                &class_symbol_to_decl_cache,
            ),
            heritage_symbol_cache_entries: heritage_symbol_cache.len(),
            heritage_symbol_cache_estimated_size_bytes: fx_hash_map_estimated_size_bytes(
                &heritage_symbol_cache,
            ),
            base_constructor_expr_cache_entries: base_constructor_expr_cache.len(),
            base_constructor_expr_cache_estimated_size_bytes: fx_hash_map_estimated_size_bytes(
                &base_constructor_expr_cache,
            ),
            base_instance_expr_cache_entries: base_instance_expr_cache.len(),
            base_instance_expr_cache_estimated_size_bytes: fx_hash_map_estimated_size_bytes(
                &base_instance_expr_cache,
            ),
            jsx_intrinsic_props_cache_entries: self.jsx_intrinsic_props_cache.len(),
            jsx_intrinsic_props_cache_estimated_size_bytes: fx_hash_map_estimated_size_bytes(
                &self.jsx_intrinsic_props_cache,
            ),
        }
    }
}

fn fx_hash_map_estimated_size_bytes<K, V>(cache: &FxHashMap<K, V>) -> usize {
    cache.capacity().saturating_mul(
        mem::size_of::<K>()
            .saturating_add(mem::size_of::<V>())
            .saturating_add(HASH_MAP_ENTRY_OVERHEAD_ESTIMATE),
    )
}

fn string_option_type_cache_estimated_size_bytes(
    cache: &FxHashMap<String, Option<TypeId>>,
) -> usize {
    fx_hash_map_estimated_size_bytes(cache)
        .saturating_add(cache.keys().map(String::len).sum::<usize>())
}

fn string_symbol_vec_cache_estimated_size_bytes(cache: &FxHashMap<String, Vec<SymbolId>>) -> usize {
    fx_hash_map_estimated_size_bytes(cache)
        .saturating_add(cache.keys().map(String::len).sum::<usize>())
        .saturating_add(
            cache
                .values()
                .map(|symbols| {
                    symbols
                        .capacity()
                        .saturating_mul(mem::size_of::<SymbolId>())
                })
                .sum::<usize>(),
        )
}

fn string_option_def_cache_estimated_size_bytes(cache: &FxHashMap<String, Option<DefId>>) -> usize {
    fx_hash_map_estimated_size_bytes(cache)
        .saturating_add(cache.keys().map(String::len).sum::<usize>())
}
