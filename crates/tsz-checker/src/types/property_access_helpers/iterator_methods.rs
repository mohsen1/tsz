//! Synthesized iterator method types for array and tuple property access.

use crate::query_boundaries::common::{
    TypeSubstitution, array_element_type, get_tuple_element_type_union, instantiate_type,
    object_shape_for_type,
};
use crate::state::CheckerState;
use tsz_solver::{FunctionShape, ObjectShape, TupleElement, TypeId};

impl<'a> CheckerState<'a> {
    pub(in crate::types_domain) fn synthesized_array_iterator_method_type(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        if !matches!(property_name, "values" | "keys" | "entries") {
            return None;
        }
        let element_type = array_element_type(self.ctx.types, object_type)
            .or_else(|| get_tuple_element_type_union(self.ctx.types, object_type))?;

        let return_arg = match property_name {
            "values" => element_type,
            "keys" => TypeId::NUMBER,
            "entries" => self.ctx.types.tuple(vec![
                TupleElement {
                    type_id: TypeId::NUMBER,
                    name: None,
                    optional: false,
                    rest: false,
                },
                TupleElement {
                    type_id: element_type,
                    name: None,
                    optional: false,
                    rest: false,
                },
            ]),
            _ => return None,
        };

        let return_type = self.synthesized_array_iterator_return_type(return_arg)?;

        Some(self.ctx.types.function(FunctionShape {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: None,
            return_type,
            type_predicate: None,
            is_constructor: false,
            is_method: true,
        }))
    }

    fn synthesized_array_iterator_return_type(&mut self, return_arg: TypeId) -> Option<TypeId> {
        if let Some(iterator_base) = self
            .resolve_entity_name_text_to_def_id_for_lowering("ArrayIterator")
            .map(|def_id| self.ctx.types.lazy(def_id))
        {
            let array_iterator = self.ctx.types.application(iterator_base, vec![return_arg]);

            // The canonical ArrayIterator lazy body can be populated from the
            // es2015-only declaration before the es2025 iterator-helper augmentation
            // is resolved. IteratorObject carries the helper members, while the
            // ArrayIterator application preserves the yielded type argument for
            // assignability checks.
            if let Some(iterator_type) = self.resolve_lib_type_by_name("IteratorObject") {
                let iterator_object = self.instantiate_synthesized_iterator_type(
                    "IteratorObject",
                    iterator_type,
                    return_arg,
                );
                if let Some(array_iterator_sym) = self.ctx.binder.file_locals.get("ArrayIterator")
                    && let Some(shape) = object_shape_for_type(self.ctx.types, iterator_object)
                {
                    let stamped_object = self.ctx.types.factory().object_with_index(ObjectShape {
                        flags: shape.flags,
                        properties: shape.properties.clone(),
                        string_index: shape.string_index,
                        number_index: shape.number_index,
                        symbol: Some(array_iterator_sym),
                    });
                    return Some(
                        self.ctx
                            .types
                            .intersection(vec![array_iterator, stamped_object]),
                    );
                }
            }

            Some(array_iterator)
        } else if let Some(iterator_type) = self.resolve_lib_type_by_name("IterableIterator") {
            Some(self.instantiate_synthesized_iterator_type(
                "IterableIterator",
                iterator_type,
                return_arg,
            ))
        } else {
            let iterator_base = self
                .resolve_entity_name_text_to_def_id_for_lowering("IterableIterator")
                .map(|def_id| self.ctx.types.lazy(def_id));
            iterator_base.map(|base| self.ctx.types.application(base, vec![return_arg]))
        }
    }

    fn instantiate_synthesized_iterator_type(
        &mut self,
        iterator_name: &str,
        iterator_type: TypeId,
        return_arg: TypeId,
    ) -> TypeId {
        let mut type_args = if iterator_name == "IteratorObject" {
            vec![
                return_arg,
                self.builtin_iterator_return_intrinsic_type(),
                TypeId::UNKNOWN,
            ]
        } else {
            vec![return_arg]
        };
        let type_params = self
            .ctx
            .binder
            .file_locals
            .get(iterator_name)
            .map(|sym_id| self.get_type_params_for_symbol(sym_id))
            .unwrap_or_default();
        for param in type_params.iter().skip(type_args.len()) {
            type_args.push(
                param
                    .default
                    .or(param.constraint)
                    .unwrap_or(TypeId::UNKNOWN),
            );
        }
        type_args.truncate(type_params.len());

        let substitution = TypeSubstitution::from_args(self.ctx.types, &type_params, &type_args);
        instantiate_type(self.ctx.types, iterator_type, &substitution)
    }
}
