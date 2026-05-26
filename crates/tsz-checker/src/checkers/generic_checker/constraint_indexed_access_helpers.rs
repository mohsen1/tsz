//! Indexed-access helpers for `TS2344` constraint validation.
//!
//! Extracted from `constraint_validation.rs` to keep that file under the
//! checker per-file size guard. Behavior is unchanged except that the
//! key-space relation guard now goes through the shared diagnostic boundary.

use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn constraint_check_indexed_access_value_type(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
    ) -> Option<TypeId> {
        let object_type = self.evaluate_type_for_assignability(object_type);
        let mut object_type = self.resolve_lazy_type(object_type);
        if query::get_object_shape(self.ctx.types.as_type_database(), object_type).is_none()
            && let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(object_type)
        {
            object_type = self.type_reference_symbol_type(sym_id);
            object_type = self.evaluate_type_for_assignability(object_type);
            object_type = self.resolve_lazy_type(object_type);
        }
        let key_type = self.evaluate_type_for_assignability(index_type);
        let key_type = self.resolve_lazy_type(key_type);
        let db = self.ctx.types.as_type_database();
        let key_kind = query::classify_index_key(db, key_type);

        if let Some(shape) = query::get_object_shape(db, object_type) {
            if let Some(index) = &shape.string_index
                && query::key_matches_string_index(db, key_type, &key_kind)
            {
                return Some(index.value_type);
            }
            if let Some(index) = &shape.number_index
                && query::key_matches_number_index(db, key_type, &key_kind)
            {
                return Some(index.value_type);
            }
        }

        if let Some(shape) = query::callable_shape_for_type(db, object_type) {
            if let Some(index) = &shape.string_index
                && query::key_matches_string_index(db, key_type, &key_kind)
            {
                return Some(index.value_type);
            }
            if let Some(index) = &shape.number_index
                && query::key_matches_number_index(db, key_type, &key_kind)
            {
                return Some(index.value_type);
            }
        }

        // For mapped types `{ [K in C]: Template }`, the indexed access value
        // type is the template type. This handles cases like
        // `FunctionsObj<T>[keyof T]` where FunctionsObj is `{ [K in keyof T]: () => unknown }`.
        if let Some(template) = query::mapped_type_template(db, object_type) {
            return Some(template);
        }

        // For the built-in utility alias `Record<K, V>` and its equivalent
        // user-facing aliases, evaluate the alias body before falling back to
        // structural/object-shape checks. Without this, patterns like
        // `{ [K in keyof O]: Record<O[K], K> }[keyof O]` can still retain
        // an `Application` form and fail TS2344 checks even though
        // `Record`'s template is provably valid for the key space.
        if let Some(alias_object_type) =
            self.resolve_record_alias_type_for_indexed_access_value(object_type)
            && alias_object_type != object_type
            && let Some(value_type) =
                self.constraint_check_indexed_access_value_type(alias_object_type, index_type)
        {
            return Some(value_type);
        }

        // For concrete object maps like `HTMLElementTagNameMap`, an indexed access
        // `Map[K]` with `K extends keyof Map` has a base constraint equal to the
        // union of all mapped property value types. tsc eagerly uses that union for
        // TS2344 checks on `HTMLCollectionOf<HTMLElementTagNameMap[K]>` /
        // `NodeListOf<HTMLElementTagNameMap[K]>` instead of deferring the relation.
        let keyed_object_type = if query::is_bare_type_parameter(db, key_type) {
            let key_base = self.constraint_check_base_type(key_type);
            if key_base == TypeId::UNKNOWN {
                key_type
            } else {
                key_base
            }
        } else {
            key_type
        };

        if let Some(shape) = query::get_object_shape(db, object_type)
            && !shape.properties.is_empty()
            && let Some(object_keys) =
                crate::query_boundaries::common::keyof_object_properties(db, object_type)
        {
            let keyed_object_type =
                if let Some(keyed_operand) = query::keyof_operand(db, keyed_object_type) {
                    let keyed_operand = self.evaluate_type_for_assignability(keyed_operand);
                    let keyed_operand = self.resolve_lazy_type(keyed_operand);
                    if keyed_operand == object_type {
                        object_keys
                    } else {
                        keyed_object_type
                    }
                } else {
                    keyed_object_type
                };
            if !self.diagnostic_relation_boolean_guard(keyed_object_type, object_keys) {
                return None;
            }
            let mut property_types: Vec<TypeId> =
                shape.properties.iter().map(|prop| prop.type_id).collect();
            if let Some(index) = &shape.string_index {
                property_types.push(index.value_type);
            }
            if let Some(index) = &shape.number_index {
                property_types.push(index.value_type);
            }
            return match property_types.len() {
                0 => None,
                1 => property_types.first().copied(),
                _ => Some(self.ctx.types.union(property_types)),
            };
        }

        None
    }

    pub(super) fn concrete_indexed_access_property_union(
        &mut self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        let evaluated = self.evaluate_type_for_assignability(type_id);
        let evaluated = self.resolve_lazy_type(evaluated);
        let db = self.ctx.types.as_type_database();
        let (object_type, index_type) = query::index_access_components(db, evaluated)?;
        let value_type =
            self.constraint_check_indexed_access_value_type(object_type, index_type)?;
        let value_type = self.evaluate_type_for_assignability(value_type);
        let value_type = self.resolve_lazy_type(value_type);
        (!query::contains_free_type_parameters(self.ctx.types, value_type)).then_some(value_type)
    }
}
