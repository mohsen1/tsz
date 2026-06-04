impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Visit a tuple type: [A, B, ...C]
    ///
    /// Evaluates each element's type if it is a meta-type that can simplify
    /// (`IndexAccess`, Mapped, Conditional, etc.). For rest/spread elements
    /// whose evaluated type is itself a tuple, flattens them inline.
    /// For example: `[string, ...([number, boolean])]` → `[string, number, boolean]`
    ///
    /// Conservative: only evaluates element types that are known meta-types
    /// to avoid exponential blowup with recursive conditional types that
    /// produce tuples.
    fn visit_tuple(&mut self, tuple_list_id: TupleListId, original_type_id: TypeId) -> TypeId {
        use crate::intern::TEMPLATE_LITERAL_EXPANSION_LIMIT;
        use tsz_common::limits::MAX_REPRESENTABLE_TUPLE_LENGTH;

        let elements = self.interner.tuple_list(tuple_list_id);

        // Quick check: does any element need evaluation or structural normalization?
        // Also triggers when a rest element holds a concrete Tuple that must be
        // flattened — e.g. `[L, ...R]` after infer-binding R to `[1, 2]` — or a
        // union of concrete tuples that must be distributed — e.g.
        // `[0, ...([2] | [3, 4]), 1]` fans out into `[0, 2, 1] | [0, 3, 4, 1]`.
        // See `union_is_fully_spreadable` for which unions qualify (tuple
        // members only; array-unions and generic spreads are left alone to
        // match tsc).
        // ReadonlyType(Tuple) rest elements are already caught by is_evaluable_meta_type.
        let needs_eval = elements.iter().any(|elem| {
            Self::is_evaluable_meta_type(self.interner, elem.type_id)
                || (elem.rest
                    && (matches!(self.interner.lookup(elem.type_id), Some(TypeData::Tuple(_)))
                        || Self::union_is_fully_spreadable(self.interner, elem.type_id)))
        });
        if !needs_eval {
            return original_type_id;
        }

        let mut alternatives: Vec<Vec<TupleElement>> = vec![Vec::with_capacity(elements.len())];
        let mut changed = false;
        let mut spread_product = 1usize;

        for elem in elements.iter() {
            // Only evaluate element types that are meta-types (IndexAccess,
            // Mapped, Lazy, Application, etc.) — skip type parameters,
            // primitives, and already-concrete types to avoid blowup.
            let evaluated = if Self::is_evaluable_meta_type(self.interner, elem.type_id) {
                self.evaluate(elem.type_id)
            } else {
                elem.type_id
            };
            if evaluated != elem.type_id {
                changed = true;
            }

            // For rest/spread elements, if the evaluated type is a tuple,
            // flatten its elements inline (spreading the inner tuple).
            if elem.rest {
                if let Some(count) = self.tuple_spread_alternative_count(evaluated) {
                    spread_product = spread_product.saturating_mul(count);
                    if spread_product >= TEMPLATE_LITERAL_EXPANSION_LIMIT {
                        self.interner.mark_union_too_complex();
                        return TypeId::ERROR;
                    }
                }

                let evaluated_inner =
                    crate::type_queries::data::unwrap_readonly(self.interner, evaluated);
                if let Some(TypeData::Tuple(inner_list_id)) = self.interner.lookup(evaluated_inner)
                {
                    let inner_elements = self.interner.tuple_list(inner_list_id);
                    let current_len = alternatives.iter().map(|a| a.len()).max().unwrap_or(0);
                    if current_len.saturating_add(inner_elements.len())
                        > MAX_REPRESENTABLE_TUPLE_LENGTH
                    {
                        self.interner.mark_tuple_too_large();
                        return TypeId::ERROR;
                    }
                    for alternative in &mut alternatives {
                        alternative.extend(inner_elements.iter().copied());
                    }
                    changed = true;
                    continue;
                } else if let Some(TypeData::Array(element_type)) = self.interner.lookup(evaluated)
                {
                    // Rest element evaluating to an array stays as rest
                    let rest_element = TupleElement {
                        type_id: element_type,
                        name: elem.name,
                        optional: elem.optional,
                        rest: true,
                    };
                    for alternative in &mut alternatives {
                        alternative.push(rest_element);
                    }
                    if element_type != elem.type_id {
                        changed = true;
                    }
                    continue;
                } else if let Some(TypeData::Union(list_id)) = self.interner.lookup(evaluated) {
                    let members = self.interner.type_list(list_id);
                    let mut spread_alternatives: Vec<Vec<TupleElement>> =
                        Vec::with_capacity(members.len());
                    for &member in members.iter() {
                        let member_inner =
                            crate::type_queries::data::unwrap_readonly(self.interner, member);
                        match self.interner.lookup(member_inner) {
                            Some(TypeData::Tuple(inner_list_id)) => {
                                spread_alternatives
                                    .push(self.interner.tuple_list(inner_list_id).to_vec());
                            }
                            Some(TypeData::Array(element_type)) => {
                                spread_alternatives.push(vec![TupleElement {
                                    type_id: element_type,
                                    name: elem.name,
                                    optional: elem.optional,
                                    rest: true,
                                }]);
                            }
                            _ => {
                                spread_alternatives.push(vec![TupleElement {
                                    type_id: member,
                                    name: elem.name,
                                    optional: elem.optional,
                                    rest: true,
                                }]);
                            }
                        }
                    }

                    let alternative_count =
                        alternatives.len().saturating_mul(spread_alternatives.len());
                    if alternative_count >= TEMPLATE_LITERAL_EXPANSION_LIMIT {
                        self.interner.mark_union_too_complex();
                        return TypeId::ERROR;
                    }

                    let max_prefix = alternatives.iter().map(|p| p.len()).max().unwrap_or(0);
                    let max_spread = spread_alternatives
                        .iter()
                        .map(|s| s.len())
                        .max()
                        .unwrap_or(0);
                    if max_prefix.saturating_add(max_spread) > MAX_REPRESENTABLE_TUPLE_LENGTH {
                        self.interner.mark_tuple_too_large();
                        return TypeId::ERROR;
                    }

                    let mut distributed = Vec::with_capacity(alternative_count);
                    for prefix in alternatives {
                        for spread in &spread_alternatives {
                            let mut next = Vec::with_capacity(prefix.len() + spread.len());
                            next.extend_from_slice(&prefix);
                            next.extend_from_slice(spread);
                            distributed.push(next);
                        }
                    }
                    alternatives = distributed;
                    changed = true;
                    continue;
                }
            }

            let next_element = TupleElement {
                type_id: evaluated,
                name: elem.name,
                optional: elem.optional,
                rest: elem.rest,
            };
            for alternative in &mut alternatives {
                alternative.push(next_element);
            }
        }

        if !changed {
            return original_type_id;
        }

        if alternatives.len() == 1 {
            self.interner.tuple(alternatives.pop().unwrap_or_default())
        } else {
            self.interner.union(
                alternatives
                    .into_iter()
                    .map(|elems| self.interner.tuple(elems))
                    .collect(),
            )
        }
    }

    /// A union is "fully spreadable" when it is non-empty and every member is a
    /// concrete tuple type (possibly `readonly`-wrapped). Such a union in spread
    /// position distributes into one tuple per member — `[a, ...(X | Y), b]`
    /// becomes `[a, ...X, b] | [a, ...Y, b]` — because the members have
    /// differing fixed shapes that a single tuple cannot encode.
    ///
    /// Members that are bare arrays are intentionally excluded: tsc keeps a
    /// union-of-arrays rest as a single rest element (e.g.
    /// `[a, b, ...(X[] | Y[])]` stays put rather than fanning out), since an
    /// unbounded rest already encodes the union without distribution.
    /// Unions containing a generic type parameter or any other non-tuple member
    /// are likewise left undistributed, matching tsc's lazy handling of generic
    /// spreads.
    fn union_is_fully_spreadable(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
        let Some(TypeData::Union(list_id)) = db.lookup(type_id) else {
            return false;
        };
        let members = db.type_list(list_id);
        !members.is_empty()
            && members.iter().all(|&member| {
                let inner = crate::type_queries::data::unwrap_readonly(db, member);
                matches!(db.lookup(inner), Some(TypeData::Tuple(_)))
            })
    }

    fn tuple_spread_alternative_count(&self, type_id: TypeId) -> Option<usize> {
        match self.interner.lookup(type_id) {
            Some(TypeData::Tuple(_)) => Some(1),
            Some(TypeData::Union(list_id)) => Some(self.interner.type_list(list_id).len()),
            _ => None,
        }
    }

    /// Check if a type is a meta-type that would benefit from evaluation
    /// inside a tuple element. Excludes type parameters and concrete types
    /// to avoid recursive blowup.
    fn is_evaluable_meta_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        let Some(key) = db.lookup(type_id) else {
            return false;
        };
        matches!(
            key,
            TypeData::IndexAccess(_, _)
                | TypeData::Mapped(_)
                | TypeData::Lazy(_)
                | TypeData::Application(_)
                | TypeData::KeyOf(_)
                | TypeData::TemplateLiteral(_)
                | TypeData::StringIntrinsic { .. }
                | TypeData::ReadonlyType(_)
                | TypeData::TypeQuery(_)
        )
    }

    /// Visit a union type: A | B | C
    fn visit_union(&mut self, type_id: TypeId, list_id: TypeListId) -> TypeId {
        self.evaluate_union(type_id, list_id)
    }

    /// Visit an array type: T[].
    ///
    /// Keep the same conservative policy as tuple element evaluation: only
    /// evaluate element types that are solver meta-types. This lets aliases in
    /// array element position simplify before printing without
    /// recursively expanding already-concrete element types.
    fn visit_array(&mut self, elem: TypeId, original_type_id: TypeId) -> TypeId {
        if !Self::is_evaluable_meta_type(self.interner, elem) {
            return original_type_id;
        }

        let evaluated = self.evaluate(elem);
        if evaluated == elem {
            original_type_id
        } else {
            self.interner.array(evaluated)
        }
    }
}
