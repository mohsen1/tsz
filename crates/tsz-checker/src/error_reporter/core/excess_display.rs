//! Excess property diagnostic display helpers.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter::core) fn normalize_excess_display_object_type(
        &self,
        ty: TypeId,
    ) -> Option<TypeId> {
        let shape = crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty)?;
        let mut normalized = shape.as_ref().clone();
        let mut changed = false;

        for prop in &mut normalized.properties {
            let read = self.normalize_excess_display_type(prop.type_id);
            let write = self.normalize_excess_display_type(prop.write_type);
            let read = if prop.readonly {
                read
            } else {
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, read)
            };
            let write = if prop.readonly {
                write
            } else {
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, write)
            };
            changed |= read != prop.type_id || write != prop.write_type;
            prop.type_id = read;
            prop.write_type = write;
        }

        if let Some(index) = normalized.string_index.as_mut() {
            let value = self.normalize_excess_display_type(index.value_type);
            let value = if index.readonly {
                value
            } else {
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, value)
            };
            changed |= value != index.value_type;
            index.value_type = value;
        }

        if let Some(index) = normalized.number_index.as_mut() {
            let value = self.normalize_excess_display_type(index.value_type);
            let value = if index.readonly {
                value
            } else {
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, value)
            };
            changed |= value != index.value_type;
            index.value_type = value;
        }

        Some(if changed {
            self.ctx.types.factory().object_with_index(normalized)
        } else {
            ty
        })
    }

    pub(in crate::error_reporter::core) fn strip_top_level_readonly_for_excess_display(
        &self,
        ty: TypeId,
    ) -> TypeId {
        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty)
        else {
            return ty;
        };

        let has_readonly_property = shape.properties.iter().any(|prop| prop.readonly);
        let has_readonly_index = shape
            .string_index
            .as_ref()
            .is_some_and(|index| index.readonly)
            || shape
                .number_index
                .as_ref()
                .is_some_and(|index| index.readonly);
        let display_props = self.ctx.types.get_display_properties(ty);
        let has_readonly_display_property = display_props
            .as_ref()
            .is_some_and(|props| props.iter().any(|prop| prop.readonly));

        if !has_readonly_property && !has_readonly_index && !has_readonly_display_property {
            return ty;
        }

        let mut normalized = shape.as_ref().clone();
        for prop in &mut normalized.properties {
            prop.readonly = false;
        }
        if let Some(index) = normalized.string_index.as_mut() {
            index.readonly = false;
        }
        if let Some(index) = normalized.number_index.as_mut() {
            index.readonly = false;
        }

        let normalized_ty = self.ctx.types.factory().object_with_index(normalized);
        if let Some(props) = display_props {
            let mut props = props.as_ref().clone();
            for prop in &mut props {
                prop.readonly = false;
            }
            self.ctx
                .types
                .store_display_properties(normalized_ty, props);
        }
        if let Some(alias_origin) = self.ctx.types.get_display_alias(ty) {
            self.ctx
                .types
                .store_display_alias(normalized_ty, alias_origin);
        }
        if let Some(def_id) = self.ctx.definition_store.find_def_for_type(ty) {
            self.ctx
                .definition_store
                .register_type_to_def(normalized_ty, def_id);
        }
        normalized_ty
    }
}
