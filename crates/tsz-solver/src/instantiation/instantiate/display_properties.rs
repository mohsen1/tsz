use super::*;
use crate::types::{PropertyInfo, TypeId};
use rustc_hash::FxHashMap;

impl<'a> TypeInstantiator<'a> {
    /// Propagate display properties from intersection members to the result.
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
        let mut local_results = (properties.len() >= 8).then(FxHashMap::default);
        let mut instantiated: Option<Vec<PropertyInfo>> = None;
        for (index, property) in properties.iter().enumerate() {
            let type_id = self.instantiate_property_slot(property.type_id, &mut local_results);
            let write_type = if property.write_type == property.type_id {
                type_id
            } else {
                self.instantiate_property_slot(property.write_type, &mut local_results)
            };
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

    fn instantiate_property_slot(
        &mut self,
        type_id: TypeId,
        local_results: &mut Option<FxHashMap<TypeId, TypeId>>,
    ) -> TypeId {
        if let Some(local_results) = local_results {
            if let Some(cached) = local_results.get(&type_id) {
                return *cached;
            }
            let instantiated = self.instantiate(type_id);
            local_results.insert(type_id, instantiated);
            instantiated
        } else {
            self.instantiate(type_id)
        }
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

    /// Preserve the structural origin of an eagerly merged object intersection.
    ///
    /// Substituting a property creates a new object `TypeId`, so the origin map
    /// recorded when the generic intersection was merged cannot be reused by
    /// identity. Rebuild the raw origin members from the completed substitutions
    /// already memoized by the active walk, then preserve them as a raw
    /// intersection. A second semantic instantiation would merge the origin
    /// back into `result` and repaint shared application/display provenance,
    /// which can poison later conditional-alias inference.
    pub(super) fn propagate_instantiated_merged_intersection_origin(
        &mut self,
        source: TypeId,
        result: TypeId,
    ) {
        if source == result {
            return;
        }
        if self
            .interner
            .get_merged_intersection_origin(result)
            .is_some()
        {
            return;
        }
        let Some(origin) = self.interner.get_merged_intersection_origin(source) else {
            return;
        };
        if self.has_depth_exceeded() {
            return;
        }

        let Some(TypeData::Intersection(origin_list)) = self.interner.lookup(origin) else {
            return;
        };
        let origin_members = self.interner.type_list(origin_list);
        let mut rebuilt_members = FxHashMap::default();
        let instantiated_members = origin_members
            .iter()
            .map(|&member| self.rebuild_merged_origin_member(member, result, &mut rebuilt_members))
            .collect();
        let instantiated_origin = self.raw_intersection(instantiated_members);
        self.interner
            .store_merged_intersection_origin(result, instantiated_origin);
    }

    /// Rebuild one retained raw object member using only substitution results
    /// completed by the active merged-object walk. This is deliberately not a
    /// recursive instantiation entry point: all property/index slots in the
    /// merged source were already visited, and replaying them would repeat
    /// semantic reduction and provenance side effects.
    fn rebuild_merged_origin_member(
        &self,
        member: TypeId,
        protected_result: TypeId,
        rebuilt_members: &mut FxHashMap<TypeId, TypeId>,
    ) -> TypeId {
        if let Some(&rebuilt) = rebuilt_members.get(&member) {
            return rebuilt;
        }
        rebuilt_members.insert(member, member);
        let Some(kind) = self.interner.lookup(member) else {
            return member;
        };
        let shape_id = match kind {
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => shape_id,
            _ => return member,
        };
        let shape = self.interner.object_shape(shape_id);
        let properties = shape
            .properties
            .iter()
            .map(|property| {
                let mut property = property.clone();
                property.type_id = self.completed_instantiation(property.type_id);
                property.write_type = self.completed_instantiation(property.write_type);
                property
            })
            .collect();

        let rebuilt = if matches!(kind, TypeData::Object(_)) {
            self.interner
                .object_with_flags_and_symbol(properties, shape.flags, shape.symbol)
        } else {
            let rebuild_index = |index: Option<IndexSignature>| {
                index.map(|mut index| {
                    index.key_type = self.completed_instantiation(index.key_type);
                    index.value_type = self.completed_instantiation(index.value_type);
                    index
                })
            };
            self.interner.object_with_index(ObjectShape {
                flags: shape.flags,
                properties,
                string_index: rebuild_index(shape.string_index),
                number_index: rebuild_index(shape.number_index),
                symbol_index: rebuild_index(shape.symbol_index),
                symbol: shape.symbol,
            })
        };
        rebuilt_members.insert(member, rebuilt);

        if rebuilt != protected_result {
            if let Some(display_properties) = self.interner.get_display_properties(member) {
                let display_properties = display_properties
                    .iter()
                    .map(|property| {
                        let mut property = property.clone();
                        property.type_id = self.completed_instantiation(property.type_id);
                        property.write_type = self.completed_instantiation(property.write_type);
                        property
                    })
                    .collect();
                self.interner
                    .store_display_properties(rebuilt, display_properties);
            }
            if self.interner.get_application_eval_origin(rebuilt).is_none()
                && let Some(application) = self.interner.get_application_eval_origin(member)
                && let Some(TypeData::Application(application_id)) =
                    self.interner.lookup(application)
            {
                let application = self.interner.type_application(application_id);
                let base = self.completed_instantiation(application.base);
                let args = application
                    .args
                    .iter()
                    .map(|&argument| self.completed_instantiation(argument))
                    .collect();
                let application = self.interner.application(base, args);
                self.interner
                    .record_application_eval_origin(rebuilt, application);
            }
        }

        // A raw origin member can itself be the result of an earlier merge
        // (`(A & B) & C`). Replay that nested origin without re-entering
        // semantic instantiation. Never let a member that canonicalizes to the
        // outer result claim the outer result's first-write-wins provenance.
        if rebuilt != protected_result
            && self
                .interner
                .get_merged_intersection_origin(rebuilt)
                .is_none()
            && let Some(nested_origin) = self.interner.get_merged_intersection_origin(member)
            && let Some(TypeData::Intersection(nested_list)) = self.interner.lookup(nested_origin)
        {
            let nested_members = self.interner.type_list(nested_list);
            let instantiated_nested = nested_members
                .iter()
                .map(|&nested_member| {
                    self.rebuild_merged_origin_member(
                        nested_member,
                        protected_result,
                        rebuilt_members,
                    )
                })
                .collect();
            let instantiated_nested = self.raw_intersection(instantiated_nested);
            self.interner
                .store_display_alias(rebuilt, instantiated_nested);
            self.interner
                .store_merged_intersection_origin(rebuilt, instantiated_nested);
        }

        rebuilt
    }

    #[inline]
    fn completed_instantiation(&self, type_id: TypeId) -> TypeId {
        match self.visiting.get(&type_id).copied() {
            Some(InstantiationMemoEntry::Completed {
                result,
                environment_epoch,
            }) if environment_epoch == self.memo_environment_epoch => result,
            Some(InstantiationMemoEntry::Active | InstantiationMemoEntry::Completed { .. })
            | None => type_id,
        }
    }

    fn raw_intersection(&self, members: Vec<TypeId>) -> TypeId {
        let mut members = members.into_iter();
        let Some(first) = members.next() else {
            return TypeId::UNKNOWN;
        };
        members.fold(first, |left, right| {
            self.interner.intersect_types_raw2(left, right)
        })
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
