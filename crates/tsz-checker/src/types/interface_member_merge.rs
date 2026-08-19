//! Structural merge of interface member (property) lists.
//!
//! Split from `interface_type.rs` to respect the 2000-line file cap; child
//! module of the same `CheckerState` impl surface. Owns the property-level
//! half of `merge_interface_types`: same-named member override/accumulation
//! per [`InterfaceMergeMode`] and tsc-parity member ordering.

use super::interface_type::InterfaceMergeMode;
use crate::state::CheckerState;

impl CheckerState<'_> {
    /// Merge a derived property that shares a name with a `base` property.
    ///
    /// In `Heritage` mode the derived member overrides the base member outright
    /// (no overload accumulation). Only `Declaration`/augmentation merges
    /// concatenate same-named callable signatures into a shared overload set.
    fn merge_overriding_property(
        &mut self,
        derived_prop: &tsz_solver::PropertyInfo,
        base_prop: &tsz_solver::PropertyInfo,
        mode: InterfaceMergeMode,
    ) -> tsz_solver::PropertyInfo {
        let merged_type = if matches!(
            mode,
            InterfaceMergeMode::Declaration | InterfaceMergeMode::CrossFileDeclaration
        ) && crate::query_boundaries::common::callable_shape_for_type(
            self.ctx.types,
            base_prop.type_id,
        )
        .is_some()
            && crate::query_boundaries::common::callable_shape_for_type(
                self.ctx.types,
                derived_prop.type_id,
            )
            .is_some()
        {
            // Propagate the mode so a cross-file method merge stamps the later
            // file's overload group above the earlier file's.
            self.merge_interface_types_with_mode(derived_prop.type_id, base_prop.type_id, mode)
        } else {
            derived_prop.type_id
        };

        let mut prop = derived_prop.clone();
        // When the merge produces a new callable type (from concatenating
        // derived + base call signatures), update BOTH type_id and write_type.
        // Leaving write_type pointing to the derived-only callable creates a
        // false "split accessor" (type_id != write_type) that triggers the
        // contravariant write-type check in check_property_compatibility,
        // causing false TS2322 errors for interface-extends assignments.
        if merged_type != derived_prop.type_id && prop.write_type == derived_prop.type_id {
            prop.write_type = merged_type;
        }
        prop.type_id = merged_type;
        prop
    }

    /// Merge derived and base interface properties.
    ///
    /// Derived properties override base properties when names match.
    /// Property order matches tsc: derived (own) members are listed first in
    /// declaration order, followed by base members not overridden by derived.
    /// `declaration_order` is offset for base-only members so a stable sort by
    /// `declaration_order` reproduces this own-first / base-last layout — for
    /// both diagnostic display and downstream `keyof T` iteration.
    ///
    /// # Arguments
    /// * `derived` - Properties from the derived interface
    /// * `base` - Properties from the base interface
    ///
    /// # Returns
    /// The merged properties vector
    pub(crate) fn merge_properties(
        &mut self,
        derived: &[tsz_solver::PropertyInfo],
        base: &[tsz_solver::PropertyInfo],
        mode: InterfaceMergeMode,
    ) -> Vec<tsz_solver::PropertyInfo> {
        use rustc_hash::FxHashMap;
        use tsz_common::interner::Atom;

        // Find the max declaration_order from derived so base-only properties
        // can be offset to come after all derived properties (tsc parity).
        let derived_max_order = derived
            .iter()
            .map(|p| p.declaration_order)
            .max()
            .unwrap_or(0);

        let total_len = derived.len() + base.len();
        if total_len <= 32 {
            let mut merged: Vec<tsz_solver::PropertyInfo> = Vec::with_capacity(total_len);
            // Walk derived first so own members keep their (low) declaration_order
            // and appear before inherited members in the final ordering.
            for prop in derived {
                let merged_prop = match base.iter().find(|p| p.name == prop.name) {
                    Some(base_prop) => self.merge_overriding_property(prop, base_prop, mode),
                    None => prop.clone(),
                };
                merged.push(merged_prop);
            }
            // Append base-only members with offset declaration_order so a sort by
            // declaration_order keeps them after all derived members.
            for base_prop in base {
                if !derived.iter().any(|p| p.name == base_prop.name) {
                    let mut new_prop = base_prop.clone();
                    new_prop.declaration_order = derived_max_order + base_prop.declaration_order;
                    merged.push(new_prop);
                }
            }
            return merged;
        }

        let mut derived_map: FxHashMap<Atom, &tsz_solver::PropertyInfo> =
            FxHashMap::with_capacity_and_hasher(derived.len(), Default::default());
        for prop in derived {
            derived_map.insert(prop.name, prop);
        }

        let mut merged = Vec::with_capacity(total_len);

        // Walk derived first so own members keep their (low) declaration_order.
        // For names that also appear in base, merge callable signatures.
        let mut base_by_name: FxHashMap<Atom, &tsz_solver::PropertyInfo> =
            FxHashMap::with_capacity_and_hasher(base.len(), Default::default());
        for base_prop in base {
            base_by_name.insert(base_prop.name, base_prop);
        }

        for derived_prop in derived {
            let merged_prop = match base_by_name.get(&derived_prop.name) {
                Some(base_prop) => self.merge_overriding_property(derived_prop, base_prop, mode),
                None => derived_prop.clone(),
            };
            merged.push(merged_prop);
        }

        // Append base-only members with offset declaration_order so they sort
        // after the derived members.
        for base_prop in base {
            if !derived_map.contains_key(&base_prop.name) {
                let mut new_prop = base_prop.clone();
                new_prop.declaration_order = derived_max_order + base_prop.declaration_order;
                merged.push(new_prop);
            }
        }

        merged
    }
}
