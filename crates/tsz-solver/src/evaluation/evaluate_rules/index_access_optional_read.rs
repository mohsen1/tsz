//! Optional-property and constraint-resolved index reads.
//!
//! Helpers that decide whether an index read can land on an optional property
//! (and so must include `undefined`), apply mapped-type optional-read
//! semantics, and resolve an index type through its constraint before looking
//! up object members. Extracted from `index_access.rs` to keep that file under
//! the architecture size ratchet; behavior is unchanged.

use crate::construction::TypeDatabase;
use crate::relations::subtype::TypeResolver;
use crate::types::{MappedModifier, MappedType, ObjectShape, PropertyInfo, TypeData, TypeId};
use crate::visitor::{keyof_inner_type, literal_number, union_list_id};

use super::super::evaluate::TypeEvaluator;

impl<R: TypeResolver> TypeEvaluator<'_, R> {
    /// Returns `true` when both `left` and `right` are `KeyOf(X)` with the same inner `X`.
    /// Purely structural — no evaluation — so safe for recursive/conditional inner types.
    ///
    /// Type-parameter inners are compared by identity (`name` `Atom`), not by raw
    /// `TypeId`. Nested generic instantiation can produce two distinct interned
    /// `TypeParameter` `TypeId`s for the *same* logical parameter (e.g. when
    /// `Record<keyof T, V>` is expanded as the argument of an outer homomorphic
    /// mapped type like `Partial<…>`). Both `keyof T` occurrences denote the same
    /// key space, so a raw-`TypeId` comparison would spuriously reject
    /// `{ [P in keyof T]?: V }[K]` for `K extends keyof T`.
    pub(super) fn keyof_same_inner(db: &dyn TypeDatabase, left: TypeId, right: TypeId) -> bool {
        if left == right {
            return true;
        }
        let Some(TypeData::KeyOf(l_inner)) = db.lookup(left) else {
            return false;
        };
        let Some(TypeData::KeyOf(r_inner)) = db.lookup(right) else {
            return false;
        };
        if l_inner == r_inner {
            return true;
        }
        match (db.lookup(l_inner), db.lookup(r_inner)) {
            (Some(TypeData::TypeParameter(l_tp)), Some(TypeData::TypeParameter(r_tp))) => {
                l_tp.name == r_tp.name
            }
            _ => false,
        }
    }

    pub(super) fn constraints_semantically_match(&mut self, left: TypeId, right: TypeId) -> bool {
        if left == right {
            return true;
        }

        // `keyof T` denotes the same key space regardless of which interned
        // `TypeParameter` `TypeId` represents `T`. Nested generic instantiation can
        // alias the same logical `T` to two distinct `TypeId`s, so compare the
        // `KeyOf` inners by type-parameter identity before falling back to
        // evaluation. This is the homomorphic-mapped read counterpart to the
        // same-name handling already used for the mapped iteration variable.
        if Self::keyof_same_inner(self.interner(), left, right) {
            return true;
        }

        let evaluated_left = self.evaluate(left);
        let evaluated_right = self.evaluate(right);
        if evaluated_left == evaluated_right || left == evaluated_right || evaluated_left == right {
            return true;
        }
        Self::keyof_same_inner(self.interner(), evaluated_left, evaluated_right)
    }

    fn index_type_overlaps_optional_props(
        &mut self,
        index_type: TypeId,
        optional_props: &[tsz_common::Atom],
    ) -> bool {
        if let Some(name) = self.literal_property_lookup_atom(index_type) {
            return optional_props.contains(&name);
        }

        if let Some(members) = union_list_id(self.interner(), index_type) {
            return self
                .interner()
                .type_list(members)
                .iter()
                .any(|&member| self.index_type_overlaps_optional_props(member, optional_props));
        }

        // Intrinsics never match TypeParameter/KeyOf/Intersection — skip lookup.
        if index_type.is_intrinsic() {
            return false;
        }
        match self.interner().lookup(index_type) {
            Some(TypeData::TypeParameter(tp)) => tp.constraint.is_some_and(|constraint| {
                self.index_type_overlaps_optional_props(constraint, optional_props)
            }),
            Some(TypeData::KeyOf(inner)) => {
                let evaluated = self.evaluate(self.interner().keyof(inner));
                evaluated != index_type
                    && self.index_type_overlaps_optional_props(evaluated, optional_props)
            }
            Some(TypeData::Intersection(list_id)) => self
                .interner()
                .type_list(list_id)
                .iter()
                .any(|&member| self.index_type_overlaps_optional_props(member, optional_props)),
            _ => false,
        }
    }

    pub(super) fn index_type_can_hit_optional_property(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
    ) -> bool {
        let evaluated_object = self.evaluate(object_type);
        let optional_props: Vec<_> = match self.interner().lookup(evaluated_object) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => self
                .interner()
                .object_shape(shape_id)
                .properties
                .iter()
                .filter(|prop| prop.optional)
                .map(|prop| prop.name)
                .collect(),
            Some(TypeData::Callable(shape_id)) => self
                .interner()
                .callable_shape(shape_id)
                .properties
                .iter()
                .filter(|prop| prop.optional)
                .map(|prop| prop.name)
                .collect(),
            _ => return false,
        };

        !optional_props.is_empty()
            && self.index_type_overlaps_optional_props(index_type, &optional_props)
    }

    pub(super) fn apply_mapped_optional_read_semantics(
        &mut self,
        object_type: TypeId,
        mapped: &MappedType,
        index_type: TypeId,
        value_type: TypeId,
    ) -> TypeId {
        if matches!(mapped.optional_modifier, Some(MappedModifier::Add))
            || (mapped.optional_modifier.is_none()
                && self.index_type_can_hit_optional_property(object_type, index_type))
        {
            return self.interner().union2(value_type, TypeId::UNDEFINED);
        }

        value_type
    }

    pub(super) fn homomorphic_mapped_source_for_index_read(
        &mut self,
        mapped: &MappedType,
    ) -> Option<TypeId> {
        let Some(TypeData::IndexAccess(source, idx)) = self.interner().lookup(mapped.template)
        else {
            return None;
        };
        let Some(TypeData::TypeParameter(param)) = self.interner().lookup(idx) else {
            return None;
        };
        if param.name != mapped.type_param.name {
            return None;
        }

        if let Some(keyof_source) = keyof_inner_type(self.interner(), mapped.constraint) {
            return (source == keyof_source).then_some(source);
        }

        if matches!(
            self.interner().lookup(source),
            Some(TypeData::TypeParameter(_))
        ) {
            return None;
        }
        (self.evaluate(self.interner().keyof(source)) == mapped.constraint).then_some(source)
    }

    pub(super) fn mapped_tuple_literal_index_should_materialize(
        &mut self,
        mapped: &MappedType,
        index_type: TypeId,
    ) -> bool {
        if mapped.name_type.is_some() || literal_number(self.interner(), index_type).is_none() {
            return false;
        }

        let Some(source) = keyof_inner_type(self.interner(), mapped.constraint) else {
            return false;
        };
        let source = self.evaluate(source);
        let source = crate::type_queries::data::unwrap_readonly(self.interner(), source);
        matches!(self.interner().lookup(source), Some(TypeData::Tuple(_)))
    }

    pub(super) fn constrained_index_type(&mut self, index_type: TypeId) -> Option<TypeId> {
        if index_type.is_intrinsic() {
            return None;
        }
        match self.interner().lookup(index_type) {
            Some(TypeData::TypeParameter(tp)) => tp.constraint.and_then(|constraint| {
                let evaluated = self.evaluate(constraint);
                (evaluated != index_type).then_some(evaluated)
            }),
            Some(TypeData::KeyOf(inner)) => {
                let evaluated = self.evaluate(self.interner().keyof(inner));
                (evaluated != index_type).then_some(evaluated)
            }
            Some(TypeData::Intersection(list_id)) => {
                let members: Vec<_> = self.interner().type_list(list_id).iter().copied().collect();
                let resolved: Vec<_> = members
                    .into_iter()
                    .filter_map(|member| {
                        self.constrained_index_type(member)
                            .filter(|resolved| *resolved != member)
                    })
                    .collect();
                match resolved.as_slice() {
                    [] => None,
                    [only] => Some(*only),
                    _ => Some(self.interner().intersection(resolved)),
                }
            }
            _ => None,
        }
    }

    pub(super) fn evaluate_object_index_from_constraint(
        &mut self,
        props: &[PropertyInfo],
        index_type: TypeId,
    ) -> Option<TypeId> {
        let constrained = self.constrained_index_type(index_type)?;
        let result = self.evaluate_object_index(props, constrained);
        (result != TypeId::UNDEFINED
            || !crate::type_queries::is_generic_type(self.interner(), constrained))
        .then_some(result)
    }

    pub(super) fn evaluate_object_with_index_from_constraint(
        &mut self,
        shape: &ObjectShape,
        index_type: TypeId,
    ) -> Option<TypeId> {
        let constrained = self.constrained_index_type(index_type)?;
        let result = self.evaluate_object_with_index(shape, constrained);
        (result != TypeId::UNDEFINED
            || !crate::type_queries::is_generic_type(self.interner(), constrained))
        .then_some(result)
    }
}
