//! Display-order helpers for mapped type evaluation.

use crate::evaluation::evaluate::TypeEvaluator;
use crate::relations::subtype::TypeResolver;
use crate::types::{PropertyInfo, TypeId};
use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    pub(super) fn sort_homomorphic_source_properties_for_display(
        &self,
        source: TypeId,
        resolved_source: TypeId,
        source_props: &mut [PropertyInfo],
    ) {
        crate::type_queries::sort_homomorphic_source_properties_for_display(
            self.interner(),
            source,
            resolved_source,
            source_props,
        );
    }

    pub(super) fn sort_mapped_properties_for_display(
        &self,
        source_object: Option<TypeId>,
        resolved_source_id: Option<TypeId>,
        source_decl_order: &[Atom],
        properties: &mut [PropertyInfo],
    ) {
        if let (Some(source), Some(resolved_source)) = (source_object, resolved_source_id)
            && crate::type_queries::sort_number_wrapper_properties_for_display(
                self.interner(),
                source,
                resolved_source,
                properties,
            )
        {
            return;
        }
        if source_decl_order.is_empty() {
            return;
        }
        let order_map: FxHashMap<Atom, usize> = source_decl_order
            .iter()
            .enumerate()
            .map(|(idx, name)| (*name, idx))
            .collect();
        properties.sort_by_key(|p| order_map.get(&p.name).copied().unwrap_or(usize::MAX));
    }
}
