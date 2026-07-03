//! Base-class instance surface merging for inherited class members.

use crate::query_boundaries::state::type_resolution as query;
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::interner::Atom;
use tsz_solver::{IndexSignature, PropertyInfo, TypeId};

impl<'a> CheckerState<'a> {
    pub(crate) fn merge_base_instance_properties(
        &mut self,
        base_instance_type: TypeId,
        properties: &mut FxHashMap<Atom, PropertyInfo>,
        string_index: &mut Option<IndexSignature>,
        number_index: &mut Option<IndexSignature>,
        symbol_index: &mut Option<IndexSignature>,
    ) {
        let mut visited = FxHashSet::default();
        self.merge_base_instance_properties_inner(
            base_instance_type,
            properties,
            string_index,
            number_index,
            symbol_index,
            &mut visited,
        );
    }

    pub(crate) fn merge_base_instance_properties_inner(
        &mut self,
        base_instance_type: TypeId,
        properties: &mut FxHashMap<Atom, PropertyInfo>,
        string_index: &mut Option<IndexSignature>,
        number_index: &mut Option<IndexSignature>,
        symbol_index: &mut Option<IndexSignature>,
        visited: &mut FxHashSet<TypeId>,
    ) {
        let base_instance_type = self.normalize_base_instance_type_for_merge(base_instance_type);
        if !visited.insert(base_instance_type) {
            return;
        }

        match query::classify_for_base_instance_merge(self.ctx.types, base_instance_type) {
            query::BaseInstanceMergeKind::Object(base_shape_id) => {
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                for base_prop in &base_shape.properties {
                    properties
                        .entry(base_prop.name)
                        .or_insert_with(|| base_prop.clone());
                }
                if let Some(idx) = base_shape.string_index_signature().copied() {
                    Self::merge_index_signature(string_index, idx);
                }
                if let Some(idx) = base_shape.number_index {
                    Self::merge_index_signature(number_index, idx);
                }
                if let Some(idx) = base_shape.symbol_index_signature().copied() {
                    Self::merge_index_signature(symbol_index, idx);
                }
            }
            query::BaseInstanceMergeKind::Intersection(members) => {
                for member in members {
                    self.merge_base_instance_properties_inner(
                        member,
                        properties,
                        string_index,
                        number_index,
                        symbol_index,
                        visited,
                    );
                }
            }
            query::BaseInstanceMergeKind::Union(members) => {
                let mut common_props: Option<FxHashMap<Atom, PropertyInfo>> = None;
                let mut common_string_index: Option<IndexSignature> = None;
                let mut common_number_index: Option<IndexSignature> = None;
                let mut common_symbol_index: Option<IndexSignature> = None;

                for member in members {
                    let mut member_props = FxHashMap::default();
                    let mut member_string_index = None;
                    let mut member_number_index = None;
                    let mut member_symbol_index = None;
                    let mut member_visited = FxHashSet::default();
                    member_visited.insert(base_instance_type);

                    self.merge_base_instance_properties_inner(
                        member,
                        &mut member_props,
                        &mut member_string_index,
                        &mut member_number_index,
                        &mut member_symbol_index,
                        &mut member_visited,
                    );

                    if common_props.is_none() {
                        common_props = Some(member_props);
                        common_string_index = member_string_index;
                        common_number_index = member_number_index;
                        common_symbol_index = member_symbol_index;
                        continue;
                    }

                    let Some(mut props) = common_props.take() else {
                        common_props = Some(member_props);
                        common_string_index = member_string_index;
                        common_number_index = member_number_index;
                        common_symbol_index = member_symbol_index;
                        continue;
                    };
                    props.retain(|name, prop| {
                        let Some(member_prop) = member_props.get(name) else {
                            return false;
                        };
                        prop.type_id = union_if_distinct(self, prop.type_id, member_prop.type_id);
                        prop.write_type =
                            union_if_distinct(self, prop.write_type, member_prop.write_type);
                        prop.optional |= member_prop.optional;
                        prop.readonly &= member_prop.readonly;
                        prop.is_method &= member_prop.is_method;
                        true
                    });
                    common_props = Some(props);

                    common_string_index =
                        self.common_base_index(common_string_index.take(), member_string_index);
                    common_number_index =
                        self.common_base_index(common_number_index.take(), member_number_index);
                    common_symbol_index =
                        self.common_base_index(common_symbol_index.take(), member_symbol_index);

                    if common_props.as_ref().is_none_or(FxHashMap::is_empty)
                        && common_string_index.is_none()
                        && common_number_index.is_none()
                        && common_symbol_index.is_none()
                    {
                        break;
                    }
                }

                if let Some(props) = common_props {
                    for prop in props.into_values() {
                        properties.entry(prop.name).or_insert(prop);
                    }
                }
                if let Some(idx) = common_string_index {
                    Self::merge_index_signature(string_index, idx);
                }
                if let Some(idx) = common_number_index {
                    Self::merge_index_signature(number_index, idx);
                }
                if let Some(idx) = common_symbol_index {
                    Self::merge_index_signature(symbol_index, idx);
                }
            }
            query::BaseInstanceMergeKind::Other => {}
        }
    }

    fn common_base_index(
        &mut self,
        left: Option<IndexSignature>,
        right: Option<IndexSignature>,
    ) -> Option<IndexSignature> {
        let (Some(mut left), Some(right)) = (left, right) else {
            return None;
        };
        left.value_type = union_if_distinct(self, left.value_type, right.value_type);
        left.readonly &= right.readonly;
        Some(left)
    }
}

fn union_if_distinct(state: &mut CheckerState<'_>, left: TypeId, right: TypeId) -> TypeId {
    if left == right {
        left
    } else {
        state.ctx.types.union2(left, right)
    }
}
