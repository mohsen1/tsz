//! `TypeEnvironment` merge-back for throwaway delegate checkers.
//!
//! Split out of `resolver.rs` to keep it under the 2000-line arch ceiling;
//! see `TypeEnvironment::merge_from` for the invariant this protects.

use super::TypeEnvironment;

impl TypeEnvironment {
    /// Absorb `other`'s registrations into `self`.
    ///
    /// A delegated cross-arena `CheckerContext` (`with_parent_cache`'s
    /// callers — import-type member resolution, JSDoc typedef resolution,
    /// commonjs delegation) is constructed with a brand-new, empty
    /// `TypeEnvironment` rather than a fork of the parent's: `definition_store`
    /// is `Arc`-shared so `DefId`s stay globally unique, but `DefId -> TypeId`
    /// (and every other map here) is not. A delegate that resolves a class or
    /// interface registers that resolution only in its own throwaway
    /// environment; without this merge, the registration is discarded when the
    /// delegate goes out of scope. The parent's later attempt to resolve the
    /// same `DefId` (e.g. `Lazy(DefId)` inside a conditional-type relation
    /// check) then finds nothing, and treats the relation as `Undetermined`
    /// rather than `Holds`/`Fails` — the conditional stays deferred and its
    /// constraint (the union of both branches) leaks out where a single
    /// resolved branch belongs (see the `import("./module").Box` cross-file
    /// identity bug, #17158).
    ///
    /// `other`'s entries win on key collision: a delegate only writes keys it
    /// actually resolved, so a collision means `self`'s entry was stale or
    /// absent.
    pub fn merge_from(&mut self, other: &TypeEnvironment) {
        if other.types.is_empty()
            && other.type_params.is_empty()
            && other.boxed_types.is_empty()
            && other.array_base_type.is_none()
            && other.array_base_type_params.is_empty()
            && other.readonly_array_base_type.is_none()
            && other.def_types.is_empty()
            && other.def_type_params.is_empty()
            && other.declared_variances.is_empty()
            && other.def_to_symbol.is_empty()
            && other.symbol_to_def.is_empty()
            && other.numeric_enums.is_empty()
            && other.def_kinds.is_empty()
            && other.enum_namespace_types.is_empty()
            && other.enum_parents.is_empty()
            && other.enum_members.is_empty()
            && other.class_instance_types.is_empty()
            && other.boxed_def_ids.is_empty()
            && other.class_extends.is_empty()
            && other.verified_interface_extends.is_empty()
            && other.instance_type_to_class.is_empty()
            && other.definition_store.is_none()
            && other.unresolved_name_resolutions.is_empty()
            && other.well_known_symbol_name_to_ref.is_empty()
            && other.typeof_value_types.is_empty()
        {
            return;
        }

        self.types.extend(other.types.iter().map(|(&k, &v)| (k, v)));
        self.type_params
            .extend(other.type_params.iter().map(|(&k, v)| (k, v.clone())));
        self.boxed_types
            .extend(other.boxed_types.iter().map(|(&k, &v)| (k, v)));
        if self.array_base_type.is_none() {
            self.array_base_type = other.array_base_type;
        }
        if self.array_base_type_params.is_empty() {
            self.array_base_type_params = other.array_base_type_params.clone();
        }
        if self.readonly_array_base_type.is_none() {
            self.readonly_array_base_type = other.readonly_array_base_type;
        }
        self.def_types
            .extend(other.def_types.iter().map(|(&k, &v)| (k, v)));
        self.def_type_params
            .extend(other.def_type_params.iter().map(|(&k, v)| (k, v.clone())));
        self.declared_variances.extend(
            other
                .declared_variances
                .iter()
                .map(|(&k, v)| (k, v.clone())),
        );
        self.def_to_symbol
            .extend(other.def_to_symbol.iter().map(|(&k, &v)| (k, v)));
        self.symbol_to_def
            .extend(other.symbol_to_def.iter().map(|(&k, &v)| (k, v)));
        self.numeric_enums
            .extend(other.numeric_enums.iter().copied());
        self.def_kinds
            .extend(other.def_kinds.iter().map(|(&k, &v)| (k, v)));
        self.enum_namespace_types
            .extend(other.enum_namespace_types.iter().map(|(&k, &v)| (k, v)));
        self.enum_parents
            .extend(other.enum_parents.iter().map(|(&k, &v)| (k, v)));
        self.enum_members
            .extend(other.enum_members.iter().map(|(&k, v)| (k, v.clone())));
        self.class_instance_types
            .extend(other.class_instance_types.iter().map(|(&k, &v)| (k, v)));
        for (&kind, def_ids) in &other.boxed_def_ids {
            let entry = self.boxed_def_ids.entry(kind).or_default();
            for &def_id in def_ids {
                if !entry.contains(&def_id) {
                    entry.push(def_id);
                }
            }
        }
        self.class_extends
            .extend(other.class_extends.iter().map(|(&k, &v)| (k, v)));
        self.verified_interface_extends.extend(
            other
                .verified_interface_extends
                .iter()
                .map(|(&k, &v)| (k, v)),
        );
        self.instance_type_to_class
            .extend(other.instance_type_to_class.iter().map(|(&k, &v)| (k, v)));
        if self.definition_store.is_none() {
            self.definition_store = other.definition_store.clone();
        }
        self.unresolved_name_resolutions.extend(
            other
                .unresolved_name_resolutions
                .iter()
                .map(|(k, &v)| (k.clone(), v)),
        );
        self.well_known_symbol_name_to_ref.extend(
            other
                .well_known_symbol_name_to_ref
                .iter()
                .map(|(k, &v)| (k.clone(), v)),
        );
        self.typeof_value_types
            .extend(other.typeof_value_types.iter().map(|(&k, &v)| (k, v)));

        self.bump_generation();
    }
}
