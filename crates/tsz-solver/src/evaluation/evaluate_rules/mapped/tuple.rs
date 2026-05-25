use super::*;

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

        // CRITICAL: Handle optional modifier (Partial<T[]> case)
        // TypeScript adds undefined to the element type when ? modifier is present
        if matches!(mapped.optional_modifier, Some(MappedModifier::Add)) {
            mapped_element = self.interner().union2(mapped_element, TypeId::UNDEFINED);
        }

        // Check if readonly modifier should be applied
        let is_readonly = matches!(mapped.readonly_modifier, Some(MappedModifier::Add));

        // Create the new array type
        if is_readonly {
            // Wrap the array type in ReadonlyType to get readonly semantics
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

        // Substitute into the template to get the mapped element type
        let mut mapped_element =
            self.evaluate(instantiate_type(self.interner(), mapped.template, &subst));

        // CRITICAL: Handle optional modifier (Partial<T[]> case)
        if matches!(mapped.optional_modifier, Some(MappedModifier::Add)) {
            mapped_element = self.interner().union2(mapped_element, TypeId::UNDEFINED);
        }

        // Apply readonly modifier if present
        let final_readonly = match mapped.readonly_modifier {
            Some(MappedModifier::Add) => true,
            Some(MappedModifier::Remove) => false,
            None => is_readonly, // Preserve original readonly status
        };

        if final_readonly {
            // Wrap the array type in ReadonlyType to get readonly semantics
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
        source: TypeId,
        source_readonly: bool,
    ) -> TypeId {
        let mapped_tuple = self.evaluate_mapped_tuple(mapped, tuple_id, source);
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

    /// Evaluate a homomorphic mapped type over a Tuple type.
    ///
    /// For example: `type Partial<T> = { [P in keyof T]: T[P] }`
    ///   `Partial<[number, string]>` should produce `[number?, string?]`
    ///
    /// Mirrors tsc's `instantiateMappedTupleType`. For every tuple element we
    /// rebind the mapped's outer source `T` to a per-element "singleton" that
    /// captures the element's kind (Required/Optional/Rest/Variadic) and then
    /// substitute the iteration variable `K`.
    ///
    /// This preserves tuple structure - including rest, variadic, and labeled
    /// elements - even when the source tuple contains a rest element whose
    /// `T[number]` would otherwise widen to the union of all element types.
    ///
    /// `source` is the outer `T` as it appears in `mapped.template` after the
    /// mapped type was instantiated with the tuple. We replace occurrences of
    /// `source` with the per-element singleton via `substitute_exact_type` so
    /// `T[K]` evaluates per element.
    fn evaluate_mapped_tuple(
        &mut self,
        mapped: &MappedType,
        tuple_id: TupleListId,
        source: TypeId,
    ) -> TypeId {
        let tuple_elements = self.interner().tuple_list(tuple_id);
        let mut mapped_elements = Vec::with_capacity(tuple_elements.len());

        for elem in tuple_elements.iter().copied() {
            mapped_elements.push(self.evaluate_mapped_tuple_element(mapped, source, elem));
        }

        self.interner().tuple(mapped_elements)
    }

    /// Map a single tuple element by rebinding the mapped's outer source to a
    /// per-element singleton, then substituting the iteration variable.
    ///
    /// Mirrors the per-element switch in tsc's `instantiateMappedTupleType`:
    /// - Required/Optional fixed element `T_i`: rebind T -> `[T_i]`, K -> 0.
    /// - Rest of `Array<E>`: rebind T -> `Array<E>`, K -> number; wrap the
    ///   result in `Array<>` to keep the rest's "array of element type" shape.
    /// - Variadic spread of a tuple: rebind T -> the inner tuple and recurse
    ///   into the inner tuple's elements, returning a tuple in the rest's
    ///   `type_id` for downstream `expand_tuple_rest` to flatten.
    /// - Other rest types (lazy refs, type parameters): rebind T -> the rest
    ///   type as-is, K -> number; treat as an opaque variadic.
    fn evaluate_mapped_tuple_element(
        &mut self,
        mapped: &MappedType,
        source: TypeId,
        elem: TupleElement,
    ) -> TupleElement {
        let rest_inner_kind = elem.rest.then(|| self.interner().lookup(elem.type_id));

        // Variadic spread of a tuple: rebind T -> the inner tuple across
        // template/constraint/name_type and recurse so the inner tuple's
        // elements are mapped position-by-position. The result is a tuple
        // in the rest's `type_id`; `expand_tuple_rest` flattens it
        // downstream.
        if let Some(Some(TypeData::Tuple(inner_tuple_id))) = rest_inner_kind {
            let inner_mapped = self.rebind_mapped_source(mapped, source, elem.type_id);
            let inner_result =
                self.evaluate_mapped_tuple(&inner_mapped, inner_tuple_id, elem.type_id);
            return TupleElement {
                type_id: inner_result,
                name: elem.name,
                optional: elem.optional,
                rest: true,
            };
        }

        // Per-element source rebinding:
        // - Rest: T -> the rest's type as-is (e.g. `Array<E>`, type parameter,
        //   lazy ref); K -> number. For an `Array<E>` rest this makes `T[K]`
        //   evaluate to E rather than the union of all tuple element types -
        //   the bug we are fixing.
        // - Fixed element `T_i`: T -> the singleton tuple `[T_i]`; K -> 0.
        let (new_source, key) = if elem.rest {
            (elem.type_id, TypeId::NUMBER)
        } else {
            let singleton = self.interner().tuple(vec![TupleElement {
                type_id: elem.type_id,
                name: None,
                optional: false,
                rest: false,
            }]);
            (singleton, self.interner().literal_number(0.0))
        };
        let mut inner = self.evaluate_mapped_template_with_source_rebind(
            mapped.template,
            source,
            new_source,
            mapped.type_param.name,
            key,
        );

        // Optional modifier: rest elements absorb `Add` as `inner | undefined`
        // (a rest cannot syntactically combine with `?`), while fixed
        // elements toggle the per-element `optional` flag.
        let optional = if elem.rest {
            if matches!(mapped.optional_modifier, Some(MappedModifier::Add)) {
                inner = self.interner().union2(inner, TypeId::UNDEFINED);
            }
            elem.optional
        } else {
            match mapped.optional_modifier {
                Some(MappedModifier::Add) => true,
                Some(MappedModifier::Remove) => false,
                None => elem.optional,
            }
        };

        // Rewrap the rest in `Array<>` when the input rest was array-shaped;
        // opaque rests (type parameter, lazy ref) keep their evaluated form
        // so deferred indexed-access types survive.
        let type_id = if matches!(rest_inner_kind, Some(Some(TypeData::Array(_)))) {
            self.interner().array(inner)
        } else {
            inner
        };

        TupleElement {
            type_id,
            name: elem.name,
            optional,
            rest: elem.rest,
        }
    }

    /// Rewrite `template` so every occurrence of `old_source` becomes
    /// `new_source`, then substitute the iteration variable `iter_var` with
    /// `key` and evaluate.
    fn evaluate_mapped_template_with_source_rebind(
        &mut self,
        template: TypeId,
        old_source: TypeId,
        new_source: TypeId,
        iter_var: Atom,
        key: TypeId,
    ) -> TypeId {
        let rewritten = if new_source == old_source {
            template
        } else {
            let mut memo: FxHashMap<TypeId, TypeId> = FxHashMap::default();
            self.substitute_exact_type(template, old_source, new_source, &mut memo)
        };
        let subst = TypeSubstitution::single(iter_var, key);
        let instantiated = instantiate_type(self.interner(), rewritten, &subst);
        self.evaluate(instantiated)
    }

    /// Build a new `MappedType` with `old_source` replaced by `new_source`
    /// across `template`, `constraint`, and `name_type`. Used for the variadic
    /// (tuple-rest) path so that the recursive `evaluate_mapped_tuple` call
    /// iterates with the inner tuple bound as T.
    fn rebind_mapped_source(
        &mut self,
        mapped: &MappedType,
        old_source: TypeId,
        new_source: TypeId,
    ) -> MappedType {
        if new_source == old_source {
            return *mapped;
        }
        let rewrite = |this: &mut Self, ty: TypeId| -> TypeId {
            let mut memo: FxHashMap<TypeId, TypeId> = FxHashMap::default();
            this.substitute_exact_type(ty, old_source, new_source, &mut memo)
        };
        let template = rewrite(self, mapped.template);
        let constraint = rewrite(self, mapped.constraint);
        let name_type = mapped.name_type.map(|nt| rewrite(self, nt));
        MappedType {
            type_param: mapped.type_param,
            constraint,
            name_type,
            template,
            readonly_modifier: mapped.readonly_modifier,
            optional_modifier: mapped.optional_modifier,
        }
    }
}
