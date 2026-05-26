use crate::evaluation::evaluate::TypeEvaluator;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::relations::subtype::TypeResolver;
use crate::types::{MappedModifier, MappedType, TupleListId, TypeData, TypeId};

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Evaluate a homomorphic mapped type over an Array type.
    ///
    /// For example: `type Partial<T> = { [P in keyof T]?: T[P] }`
    ///   `Partial<number[]>` should produce `(number | undefined)[]`
    ///
    /// We instantiate the template with `K = number` to get the mapped element type.
    pub(super) fn evaluate_mapped_array(
        &mut self,
        mapped: &MappedType,
        _element_type: TypeId,
    ) -> TypeId {
        let subst = TypeSubstitution::single(mapped.type_param.name, TypeId::NUMBER);

        // Substitute into the template to get the mapped element type
        let mut mapped_element =
            self.evaluate(instantiate_type(self.interner(), mapped.template, &subst));

        // TypeScript adds undefined to the element type when ? modifier is present.
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
    /// Used for `ReadonlyArray<T>` to preserve readonly semantics.
    pub(super) fn evaluate_mapped_array_with_readonly(
        &mut self,
        mapped: &MappedType,
        _element_type: TypeId,
        is_readonly: bool,
    ) -> TypeId {
        let subst = TypeSubstitution::single(mapped.type_param.name, TypeId::NUMBER);

        // Substitute into the template to get the mapped element type
        let mut mapped_element =
            self.evaluate(instantiate_type(self.interner(), mapped.template, &subst));

        // Handle optional modifier (`Partial<T[]>` case).
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
    /// A tuple's readonly-ness is a property of the whole tuple (via the
    /// `ReadonlyType` wrapper), not of individual elements, so the modifier is
    /// resolved here with the standard homomorphic rule:
    /// `+readonly` => readonly, `-readonly` => mutable, none => preserve the
    /// source's readonly-ness (`source_readonly`). This mirrors
    /// [`Self::evaluate_mapped_array_with_readonly`].
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
    /// to the tuple position `index`, then evaluate it. For a homomorphic
    /// mapping this makes `X[I]` resolve to that position's element type.
    fn map_template_at_index(&mut self, mapped: &MappedType, index: usize) -> TypeId {
        let index_type = self.interner().literal_number(index as f64);
        let subst = TypeSubstitution::single(mapped.type_param.name, index_type);
        self.evaluate(instantiate_type(self.interner(), mapped.template, &subst))
    }

    /// Evaluate a homomorphic mapped type over a Tuple type.
    ///
    /// For example: `type Partial<T> = { [P in keyof T]?: T[P] }`
    ///   `Partial<[number, string]>` should produce `[number?, string?]`
    ///
    /// We instantiate the template with `K = 0, 1, 2...` for each tuple element.
    /// This preserves tuple structure including optional and rest elements.
    /// The result is always a mutable tuple; the `readonly` modifier is applied
    /// by [`Self::evaluate_mapped_tuple_with_readonly`] at the tuple level.
    fn evaluate_mapped_tuple(&mut self, mapped: &MappedType, tuple_id: TupleListId) -> TypeId {
        use crate::types::TupleElement;

        let tuple_elements = self.interner().tuple_list(tuple_id);
        let mut mapped_elements = Vec::new();
        let mut seen_rest = false;

        for (i, elem) in tuple_elements.iter().enumerate() {
            if elem.rest {
                let is_first_rest = !seen_rest;
                seen_rest = true;
                let rest_type = elem.type_id;
                let mapped_rest_type = match self.interner().lookup(rest_type) {
                    Some(TypeData::Array(_)) if is_first_rest => {
                        // `...E[]` as the first rest: every position before it is
                        // fixed, so `X[i]` (this position's index) unambiguously
                        // resolves to the rest element's own type. Binding `I` to
                        // `X[number]` instead, as array mapping does, would wrongly
                        // yield the union of every tuple element. Re-wrap as an array
                        // so the slot stays a rest element.
                        let mapped_element = self.map_template_at_index(mapped, i);
                        self.interner().array(mapped_element)
                    }
                    Some(TypeData::Array(inner_elem)) => {
                        self.evaluate_mapped_array(mapped, inner_elem)
                    }
                    Some(TypeData::Tuple(inner_tuple_id)) => {
                        // Nested tuple spread (`...[A, B]`) - recurse.
                        self.evaluate_mapped_tuple(mapped, inner_tuple_id)
                    }
                    _ => {
                        // Generic/opaque rest (e.g. `...U`): index substitution.
                        self.map_template_at_index(mapped, i)
                    }
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

            // Non-rest elements: bind the iteration variable to this position's
            // index so the template's `X[I]` resolves to this element's type.
            let raw_mapped_type = self.map_template_at_index(mapped, i);

            // Per-element readonly is not representable on a `TupleElement`; the
            // mapped type's readonly modifier is applied at the tuple level by
            // `evaluate_mapped_tuple_with_readonly`.
            let optional = match mapped.optional_modifier {
                Some(MappedModifier::Add) => true,
                Some(MappedModifier::Remove) => false,
                None => elem.optional,
            };

            // When `-?` removes optionality from a previously optional element,
            // tsc strips the implicit `| undefined` that indexed access on an
            // optional tuple element introduced.
            let mapped_type = if elem.optional && !optional {
                crate::narrowing::utils::remove_undefined(self.interner(), raw_mapped_type)
            } else {
                raw_mapped_type
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
