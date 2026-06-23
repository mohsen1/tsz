//! Homomorphic mapped-type evaluation over array and tuple sources, split out
//! of `mapped.rs` to keep each source shard under the file-size limit. These
//! are additional inherent methods on [`TypeEvaluator`]; behavior is unchanged.

use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type_cached};
use crate::relations::subtype::TypeResolver;
use crate::types::{MappedModifier, MappedType, TupleElement, TupleListId, TypeData, TypeId};
use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;

use super::super::evaluate::TypeEvaluator;

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Evaluate a homomorphic mapped type over an Array type.
    ///
    /// For example: `type Partial<T> = { [P in keyof T]?: T[P] }`
    ///   `Partial<number[]>` should produce `(number | undefined)[]`
    ///
    /// We instantiate the template with `K = number` to get the mapped element type.
    pub(crate) fn evaluate_mapped_array(
        &mut self,
        mapped: &MappedType,
        _element_type: TypeId,
    ) -> TypeId {
        let subst = TypeSubstitution::single(mapped.type_param.name, TypeId::NUMBER);

        // Substitute into the template to get the mapped element type
        let mut mapped_element = self.evaluate(instantiate_type_cached(
            self.interner(),
            self.query_db(),
            mapped.template,
            &subst,
        ));

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
    /// Used for `ReadonlyArray`<T> to preserve readonly semantics. The mapped
    /// type's `readonly` modifier is resolved homomorphically against the
    /// source's readonly-ness (`+readonly` => readonly, `-readonly` => mutable,
    /// none => copy `source_readonly`).
    pub(crate) fn evaluate_mapped_array_with_readonly(
        &mut self,
        mapped: &MappedType,
        element_type: TypeId,
        source_readonly: bool,
    ) -> TypeId {
        let final_readonly = mapped.resolve_readonly(source_readonly);
        self.evaluate_mapped_array_with_explicit_readonly(mapped, element_type, final_readonly)
    }

    /// Evaluate a homomorphic mapped type over an Array type, wrapping the result
    /// as readonly exactly when `final_readonly` is set.
    ///
    /// Unlike [`Self::evaluate_mapped_array_with_readonly`], the caller decides
    /// the final readonly-ness rather than having the mapped modifier resolved
    /// here. This is required for `as`-clause maps: tsc only applies the
    /// `+readonly`/`-readonly` modifier to an array's surface for a no-`as`
    /// homomorphic map (`instantiateMappedArrayType` under
    /// `if (!type.declaration.nameType)`); with an `as` clause the array's
    /// readonly-ness mirrors the source instead, so a readonly array stays
    /// readonly even under `-readonly`.
    pub(crate) fn evaluate_mapped_array_with_explicit_readonly(
        &mut self,
        mapped: &MappedType,
        _element_type: TypeId,
        final_readonly: bool,
    ) -> TypeId {
        let subst = TypeSubstitution::single(mapped.type_param.name, TypeId::NUMBER);

        // Substitute into the template to get the mapped element type
        let mut mapped_element = self.evaluate(instantiate_type_cached(
            self.interner(),
            self.query_db(),
            mapped.template,
            &subst,
        ));

        // CRITICAL: Handle optional modifier (Partial<T[]> case)
        if matches!(mapped.optional_modifier, Some(MappedModifier::Add)) {
            mapped_element = self.interner().union2(mapped_element, TypeId::UNDEFINED);
        }

        if final_readonly {
            // Wrap the array type in ReadonlyType to get readonly semantics
            let array_type = self.interner().array(mapped_element);
            self.interner().readonly_type(array_type)
        } else {
            self.interner().array(mapped_element)
        }
    }

    /// Map a homomorphic identity mapped type over a syntax-level `readonly`
    /// source — `readonly [..]` (a `ReadonlyType(Tuple)`) or `readonly T[]`
    /// (a `ReadonlyType(Array(..))`, which `ReadonlyArray<T>` also interns to).
    /// The lib-interface `ReadonlyArray<T>` form that resolves to an
    /// `ObjectWithIndex` is detected separately by the caller's match block,
    /// which needs structural array-marker checks this helper does not perform.
    ///
    /// Returns `None` (so the caller falls through to the generic object path)
    /// when `inner` is neither tuple- nor array-shaped, and for the one array
    /// shape we cannot model: `-readonly` under an `as` clause.
    ///
    /// Without the array arm here a homomorphic mapped type — `{ [K in keyof T]:
    /// T[K] }`, an identity `as K`, or a key-filtering `as` clause — fell through
    /// to the generic property-building path, reshaping the readonly array into a
    /// plain object and dropping its `readonly` modifier.
    ///
    /// The mapped `readonly` modifier only rewrites the array surface for a
    /// no-`as` homomorphic map (tsc's `instantiateMappedArrayType`, gated by
    /// `if (!type.declaration.nameType)`). With an `as` clause tsc routes through
    /// the object path, where the modifier never adds or removes a readonly
    /// array's methods (`push`/`pop`/...): the surface mirrors the source. We
    /// reproduce that by keeping the array readonly when an `as` clause is present
    /// and only resolving `+readonly`/`-readonly` for the no-`as` case.
    ///
    /// `-readonly` under an `as` clause is the shape we cannot represent as an
    /// array: tsc yields a hybrid object with a *writable* index but still no
    /// mutable-array methods (a readonly array minus its readonly index). A
    /// mutable array would invent `push`; a readonly array would reject valid
    /// element writes. That case returns `None` and keeps its pre-existing
    /// object-path approximation rather than emitting a false positive.
    pub(crate) fn evaluate_mapped_over_readonly_source(
        &mut self,
        mapped: &MappedType,
        source: TypeId,
        inner: TypeId,
    ) -> Option<TypeId> {
        match self.interner().lookup(inner) {
            Some(TypeData::Tuple(tuple_id)) => {
                let resolved_source = self.interner().readonly_type(inner);
                Some(self.evaluate_mapped_tuple_with_readonly_source(
                    mapped,
                    tuple_id,
                    source,
                    resolved_source,
                    true,
                ))
            }
            Some(TypeData::Array(element_type)) => {
                if mapped.name_type.is_some()
                    && matches!(mapped.readonly_modifier, Some(MappedModifier::Remove))
                {
                    return None;
                }
                let final_readonly = if mapped.name_type.is_none() {
                    mapped.resolve_readonly(true)
                } else {
                    true
                };
                Some(self.evaluate_mapped_array_with_explicit_readonly(
                    mapped,
                    element_type,
                    final_readonly,
                ))
            }
            _ => None,
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
    pub(crate) fn evaluate_mapped_tuple_with_readonly(
        &mut self,
        mapped: &MappedType,
        tuple_id: TupleListId,
        source: TypeId,
        source_readonly: bool,
    ) -> TypeId {
        self.evaluate_mapped_tuple_with_readonly_source(
            mapped,
            tuple_id,
            source,
            source,
            source_readonly,
        )
    }

    pub(crate) fn evaluate_mapped_tuple_with_readonly_source(
        &mut self,
        mapped: &MappedType,
        tuple_id: TupleListId,
        original_source: TypeId,
        mapped_source: TypeId,
        source_readonly: bool,
    ) -> TypeId {
        let rebound_mapped;
        let mapped = if original_source == mapped_source {
            mapped
        } else {
            rebound_mapped = self.rebind_mapped_source(mapped, original_source, mapped_source);
            &rebound_mapped
        };
        let mapped_tuple = self.evaluate_mapped_tuple(mapped, tuple_id, mapped_source);
        if mapped.resolve_readonly(source_readonly) {
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
        use tsz_common::limits::MAX_REPRESENTABLE_TUPLE_LENGTH;

        let tuple_elements = self.interner().tuple_list(tuple_id);
        let mut mapped_elements = Vec::with_capacity(tuple_elements.len());
        let mut seen_rest = false;

        for (index, elem) in tuple_elements.iter().copied().enumerate() {
            // Fixed elements that follow any rest element have ambiguous numeric
            // indices on the full source tuple (T[i] can land in the rest range
            // or a suffix slot depending on the actual length). The per-element
            // helper receives this flag so it can use a proxy source instead.
            let is_suffix = seen_rest && !elem.rest;
            if elem.rest {
                seen_rest = true;
            }
            let mapped_element =
                self.evaluate_mapped_tuple_element(mapped, source, index, elem, is_suffix);
            if mapped_element.rest {
                let mapped_rest = crate::type_queries::data::unwrap_readonly(
                    self.interner(),
                    mapped_element.type_id,
                );
                if let Some(TypeData::Tuple(inner_tuple_id)) = self.interner().lookup(mapped_rest) {
                    let inner_elements = self.interner().tuple_list(inner_tuple_id);
                    if mapped_elements.len().saturating_add(inner_elements.len())
                        > MAX_REPRESENTABLE_TUPLE_LENGTH
                    {
                        self.interner().mark_tuple_too_large();
                        return TypeId::ERROR;
                    }
                    mapped_elements.extend(inner_elements.iter().copied());
                    continue;
                }
            }
            mapped_elements.push(mapped_element);
        }

        self.interner().tuple(mapped_elements)
    }

    /// Map a single tuple element by rebinding the mapped's outer source to a
    /// per-element singleton, then substituting the iteration variable.
    ///
    /// Mirrors the per-element switch in tsc's `instantiateMappedTupleType`:
    /// - Required/Optional fixed element `T_i`: keep T, K -> `"0"`.
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
        index: usize,
        elem: TupleElement,
        is_suffix: bool,
    ) -> TupleElement {
        let evaluated_rest_type =
            if elem.rest && self.mapped_tuple_rest_needs_evaluation(elem.type_id) {
                self.evaluate(elem.type_id)
            } else {
                elem.type_id
            };
        let rest_inner =
            crate::type_queries::data::unwrap_readonly(self.interner(), evaluated_rest_type);
        let rest_inner_kind = elem.rest.then(|| self.interner().lookup(rest_inner));
        // Variadic spread of a tuple: rebind T -> the inner tuple across
        // template/constraint/name_type and recurse so the inner tuple's
        // elements are mapped position-by-position. The result is a tuple
        // in the rest's `type_id`; `expand_tuple_rest` flattens it
        // downstream.
        if let Some(Some(TypeData::Tuple(inner_tuple_id))) = rest_inner_kind {
            let inner_mapped = self.rebind_mapped_source(mapped, source, evaluated_rest_type);
            let inner_result =
                self.evaluate_mapped_tuple(&inner_mapped, inner_tuple_id, evaluated_rest_type);
            return TupleElement {
                type_id: inner_result,
                name: elem.name,
                optional: elem.optional,
                rest: true,
            };
        }

        // Deferred resolvable rest (`...Util<R>`): the spread operand is an
        // alias/conditional/indexed-access that *resolves to* a tuple or array
        // but could not be evaluated to a concrete shape in this frame — e.g. a
        // recursive utility reached at a deep `def_depth`, or a cross-file alias
        // mid-registration. Collapsing it through the opaque `F<source[index]>`
        // path below loses per-element identity (every spread index folds onto
        // the same `source[index]`). Instead keep the rest as a homomorphic map
        // over the *rest itself* (`...{ [K in keyof R]: F<R[K]> }`): a later
        // evaluation pass — most importantly the top-level element access, where
        // the `def_depth` budget is fresh and the resolver is fully populated —
        // resolves `R` to its concrete tuple and the deferred map flattens to the
        // correct per-element values. Genuine opaque rests (a bare `...T` type
        // parameter, which is not a "needs-evaluation" deferral) fall through to
        // the indexed-access path below, preserving the reverse-inference link.
        if elem.rest
            && !matches!(rest_inner_kind, Some(Some(TypeData::Array(_))))
            && self.mapped_tuple_rest_needs_evaluation(rest_inner)
        {
            let mut inner_mapped = self.rebind_mapped_source(mapped, source, evaluated_rest_type);
            // The mapped constraint may have been eagerly evaluated to a literal
            // key union of the *outer* tuple; re-anchor it on `keyof R` so the
            // deferred map is recognized as homomorphic over the rest when it is
            // finally evaluated.
            inner_mapped.constraint = self.interner().keyof(evaluated_rest_type);
            let deferred = self.interner().mapped(inner_mapped);
            return TupleElement {
                type_id: deferred,
                name: elem.name,
                optional: elem.optional,
                rest: true,
            };
        }

        // Opaque variadic rests (`...T`) must keep the source tuple in the
        // indexed access. Rewriting to `T[number]` loses the relationship that
        // reverse inference uses to infer `T` from mapped tuple rest elements.
        if elem.rest && !matches!(rest_inner_kind, Some(Some(TypeData::Array(_)))) {
            let key = self.interner().literal_number(index as f64);
            let mut inner = self.evaluate_mapped_template_with_source_rebind(
                mapped.template,
                source,
                source,
                mapped.type_param.name,
                key,
            );
            if matches!(mapped.optional_modifier, Some(MappedModifier::Add)) {
                inner = self.interner().union2(inner, TypeId::UNDEFINED);
            }
            return TupleElement {
                type_id: inner,
                name: elem.name,
                optional: elem.optional,
                rest: true,
            };
        }

        // Per-element source rebinding so that `T[K]` evaluates to the element's
        // own type rather than the union of every tuple element type:
        //
        // - Array rest `...E[]`: rebind T -> `E[]`, K -> number so `(E[])[number]` = E.
        // - Fixed suffix element (after any rest): T[i] is ambiguous because i can
        //   land in either the rest range or the suffix. Rebind T -> `[elem_type]` and
        //   K -> "0" so `([elem_type])["0"]` = elem_type unambiguously.
        // - Fixed prefix element: every preceding position is fixed, so the existing
        //   string-literal index is unambiguous; no rebinding needed.
        let (new_source, key) = if elem.rest {
            (elem.type_id, TypeId::NUMBER)
        } else if is_suffix {
            let proxy = self
                .interner()
                .tuple(vec![TupleElement::fixed(elem.type_id)]);
            (proxy, self.interner().literal_string("0"))
        } else {
            (source, self.interner().literal_string(&index.to_string()))
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

        if !elem.rest {
            inner = self.strip_removed_optional_undefined(inner, elem.optional && !optional);
        }

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
        let instantiated =
            instantiate_type_cached(self.interner(), self.query_db(), rewritten, &subst);
        self.evaluate(instantiated)
    }

    fn mapped_tuple_rest_needs_evaluation(&self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        matches!(
            self.interner().lookup(type_id),
            Some(
                TypeData::Application(_)
                    | TypeData::Conditional(_)
                    | TypeData::IndexAccess(_, _)
                    | TypeData::KeyOf(_)
                    | TypeData::Lazy(_)
                    | TypeData::Mapped(_)
                    | TypeData::ReadonlyType(_)
                    | TypeData::StringIntrinsic { .. }
                    | TypeData::TemplateLiteral(_)
                    | TypeData::TypeQuery(_)
            )
        )
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
