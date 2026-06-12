use super::*;
use crate::types::{PropertyInfo, TypeId};

impl<'a> TypeInstantiator<'a> {
    /// Propagate display properties from intersection members to the result.
    #[allow(dead_code)]
    pub(super) fn propagate_display_properties_for_intersection(
        &self,
        original_members: &[TypeId],
        result: TypeId,
    ) {
        let display_vec = crate::types::merge_display_properties_for_intersection(
            self.interner,
            original_members,
        );
        if !display_vec.is_empty() {
            self.interner.store_display_properties(result, display_vec);
        }
    }

    /// Instantiate a slice of properties by substituting type IDs.
    pub(super) fn instantiate_properties_if_changed(
        &mut self,
        properties: &[PropertyInfo],
    ) -> Option<Vec<PropertyInfo>> {
        let mut instantiated: Option<Vec<PropertyInfo>> = None;
        for (index, property) in properties.iter().enumerate() {
            let type_id = self.instantiate(property.type_id);
            let write_type = self.instantiate(property.write_type);
            if let Some(instantiated) = &mut instantiated {
                let mut property = property.clone();
                property.type_id = type_id;
                property.write_type = write_type;
                instantiated.push(property);
            } else if type_id != property.type_id || write_type != property.write_type {
                let mut changed = Vec::with_capacity(properties.len());
                changed.extend_from_slice(&properties[..index]);
                let mut property = property.clone();
                property.type_id = type_id;
                property.write_type = write_type;
                changed.push(property);
                instantiated = Some(changed);
            }
        }
        tsz_common::perf_counters::record_property_instantiation_walk(
            properties.len() as u64,
            instantiated.is_some(),
        );
        instantiated
    }

    pub(super) fn propagate_instantiated_display_properties(
        &mut self,
        source: TypeId,
        result: TypeId,
    ) {
        let Some(display_props) = self.interner.get_display_properties(source) else {
            return;
        };
        let props = self
            .instantiate_properties_if_changed(display_props.as_ref())
            .unwrap_or_else(|| display_props.as_ref().clone());
        self.interner.store_display_properties(result, props);
    }

    /// Propagate semantic application provenance through instantiation.
    ///
    /// A nominal class/interface instantiation that evaluation lowered to a
    /// structural shape keeps an `application_eval_origin` link. When the
    /// shape is then *instantiated* (e.g. a generic interface body holding an
    /// already-evaluated `ExpressionWrapper<O[K]>` member gets its own type
    /// arguments substituted), the new shape is produced by substitution, not
    /// by application evaluation, so it would otherwise lose the link. Rebuild
    /// the origin by substituting the same arguments into the original
    /// application's type arguments.
    pub(super) fn propagate_instantiated_application_origin(
        &mut self,
        source: TypeId,
        result: TypeId,
    ) {
        if source == result {
            return;
        }
        let Some(origin) = self.interner.get_application_eval_origin(source) else {
            return;
        };
        let Some(crate::types::TypeData::Application(app_id)) = self.interner.lookup(origin) else {
            return;
        };
        let app = self.interner.type_application(app_id);
        let base = app.base;
        let args: Vec<TypeId> = app.args.clone();
        let new_args: Vec<TypeId> = args.iter().map(|&arg| self.instantiate(arg)).collect();
        let new_origin = self.interner.application(base, new_args);
        self.interner
            .record_application_eval_origin(result, new_origin);
    }
}
