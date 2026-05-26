use crate::evaluation::evaluate::TypeEvaluator;
use crate::instantiation::instantiate::{
    TypeSubstitution, instantiate_type, instantiate_type_preserving_with_declared,
};
use crate::relations::subtype::TypeResolver;
use crate::types::{MappedModifier, MappedType, TupleElement, TupleListId, TypeData, TypeId};

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Evaluate a homomorphic mapped type over an Array type.
    ///
    /// For example: `type Partial<T> = { [P in keyof T]?: T[P] }`
    /// `Partial<number[]>` should produce `(number | undefined)[]`.
    ///
    /// We instantiate the template with `K = number` to get the mapped element type.
    pub(super) fn evaluate_mapped_array(
        &mut self,
        mapped: &MappedType,
        _element_type: TypeId,
    ) -> TypeId {
        let subst = TypeSubstitution::single(mapped.type_param.name, TypeId::NUMBER);

        let mut mapped_element =
            self.evaluate(instantiate_type(self.interner(), mapped.template, &subst));

        if matches!(mapped.optional_modifier, Some(MappedModifier::Add)) {
            mapped_element = self.interner().union2(mapped_element, TypeId::UNDEFINED);
        }

        let is_readonly = matches!(mapped.readonly_modifier, Some(MappedModifier::Add));

        if is_readonly {
            let array_type = self.interner().array(mapped_element);
            self.interner().readonly_type(array_type)
        } else {
            self.interner().array(mapped_element)
        }
    }

    /// Evaluate a homomorphic mapped type over an Array type with explicit readonly flag.
    ///
    /// Used for `ReadonlyArray`<T> to preserve readonly semantics.
    pub(super) fn evaluate_mapped_array_with_readonly(
        &mut self,
        mapped: &MappedType,
        _element_type: TypeId,
        is_readonly: bool,
    ) -> TypeId {
        let subst = TypeSubstitution::single(mapped.type_param.name, TypeId::NUMBER);

        let mut mapped_element =
            self.evaluate(instantiate_type(self.interner(), mapped.template, &subst));

        if matches!(mapped.optional_modifier, Some(MappedModifier::Add)) {
            mapped_element = self.interner().union2(mapped_element, TypeId::UNDEFINED);
        }

        let final_readonly = match mapped.readonly_modifier {
            Some(MappedModifier::Add) => true,
            Some(MappedModifier::Remove) => false,
            None => is_readonly,
        };

        if final_readonly {
            let array_type = self.interner().array(mapped_element);
            self.interner().readonly_type(array_type)
        } else {
            self.interner().array(mapped_element)
        }
    }

    /// Evaluate a homomorphic mapped type over a Tuple type, applying the
    /// mapped type's `readonly` modifier at the tuple level.
    ///
    /// A tuple's readonly-ness is a property of the whole tuple through the
    /// `ReadonlyType` wrapper, not of individual elements.
    pub(super) fn evaluate_mapped_tuple_with_readonly(
        &mut self,
        mapped: &MappedType,
        tuple_id: TupleListId,
        source_readonly: bool,
    ) -> TypeId {
        let mapped_tuple = self.evaluate_mapped_tuple(mapped, tuple_id);
        let final_readonly = match mapped.readonly_modifier {
            Some(MappedModifier::Add) => true,
            Some(MappedModifier::Remove) => false,
            None => source_readonly,
        };
        if final_readonly {
            self.interner().readonly_type(mapped_tuple)
        } else {
            mapped_tuple
        }
    }

    /// Instantiate a mapped type's template with the iteration variable bound
    /// to the tuple position `index` as a string-literal key, then evaluate it.
    ///
    /// This matches `keyof tuple` semantics: tuple indices are `"0"`, `"1"`,
    /// etc. Homomorphic `T[K]` templates still resolve to the element type
    /// because tuple indexed access accepts numeric string-literal keys.
    fn map_template_at_index(&mut self, mapped: &MappedType, index: usize) -> TypeId {
        let index_type = self.interner().literal_string(&index.to_string());
        let subst = TypeSubstitution::single(mapped.type_param.name, index_type);
        self.evaluate(instantiate_type(self.interner(), mapped.template, &subst))
    }

    /// Instantiate a mapped type template for an array rest segment inside a
    /// tuple. Fixed tuple positions bind `K` to string-literal keys (`"0"`),
    /// but a rest tail is keyed by `number`. When the template reads `T[K]`,
    /// preserve the rest element type so `[H, ...R[]]` maps its tail as `R[]`
    /// instead of `T[number][]` (`H | R`)[].
    fn map_template_at_array_rest(
        &mut self,
        mapped: &MappedType,
        rest_element_type: TypeId,
    ) -> TypeId {
        let subst = TypeSubstitution::single(mapped.type_param.name, TypeId::NUMBER);
        let instantiated = if let Some(source) =
            self.extract_template_index_source(mapped.template, mapped.type_param.name)
        {
            instantiate_type_preserving_with_declared(
                self.interner(),
                mapped.template,
                &subst,
                source,
                mapped.type_param.name,
                rest_element_type,
            )
        } else {
            instantiate_type(self.interner(), mapped.template, &subst)
        };
        self.evaluate(instantiated)
    }

    /// Evaluate a homomorphic mapped type over a Tuple type.
    ///
    /// For example: `type Partial<T> = { [P in keyof T]?: T[P] }`
    /// `Partial<[number, string]>` should produce `[number?, string?]`.
    ///
    /// We instantiate the template with `K = "0", "1", "2"...` for each
    /// tuple element, matching `keyof tuple` semantics.
    fn evaluate_mapped_tuple(&mut self, mapped: &MappedType, tuple_id: TupleListId) -> TypeId {
        let tuple_elements = self.interner().tuple_list(tuple_id);
        let mut mapped_elements = Vec::new();
        let mut seen_rest = false;

        for (i, elem) in tuple_elements.iter().enumerate() {
            if elem.rest {
                let is_first_rest = !seen_rest;
                seen_rest = true;
                let rest_type = elem.type_id;
                let mapped_rest_type = match self.interner().lookup(rest_type) {
                    Some(TypeData::Array(inner_elem)) if is_first_rest => {
                        // `...E[]` as the first rest: every position before it is
                        // fixed, so a homomorphic `T[K]` can resolve to the rest
                        // element's own type. The rest tail's key is still
                        // `number`, not a fixed string index. Re-wrap as an array
                        // so the slot stays a rest element.
                        //
                        // A later rest cannot use this: `tuple_index_literal`
                        // short-circuits on the first rest it meets, so positions
                        // after an earlier rest/variadic spread are unreliable.
                        let mapped_element = self.map_template_at_array_rest(mapped, inner_elem);
                        self.interner().array(mapped_element)
                    }
                    Some(TypeData::Array(inner_elem)) => {
                        self.evaluate_mapped_array(mapped, inner_elem)
                    }
                    Some(TypeData::Tuple(inner_tuple_id)) => {
                        self.evaluate_mapped_tuple(mapped, inner_tuple_id)
                    }
                    _ => self.map_template_at_index(mapped, i),
                };

                let final_rest_type =
                    if matches!(mapped.optional_modifier, Some(MappedModifier::Add)) {
                        self.interner().union2(mapped_rest_type, TypeId::UNDEFINED)
                    } else {
                        mapped_rest_type
                    };

                mapped_elements.push(TupleElement {
                    type_id: final_rest_type,
                    name: elem.name,
                    optional: elem.optional,
                    rest: true,
                });
                continue;
            }

            let mapped_type = self.map_template_at_index(mapped, i);

            // Per-element readonly is not representable on a `TupleElement`; the
            // mapped type's readonly modifier is applied at the tuple level by
            // `evaluate_mapped_tuple_with_readonly`.
            let optional = match mapped.optional_modifier {
                Some(MappedModifier::Add) => true,
                Some(MappedModifier::Remove) => false,
                None => elem.optional,
            };

            mapped_elements.push(TupleElement {
                type_id: mapped_type,
                name: elem.name,
                optional,
                rest: elem.rest,
            });
        }

        self.interner().tuple(mapped_elements)
    }
}
