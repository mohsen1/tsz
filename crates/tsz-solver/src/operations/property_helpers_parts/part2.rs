impl<'a> PropertyAccessEvaluator<'a> {
    /// Recursively simplifies Array<T> Application types to T[] array types.
    fn simplify_array_application(&self, type_id: TypeId, array_base: TypeId) -> TypeId {
        // Intrinsics are never Application/Callable/Array/etc — pass through.
        if type_id.is_intrinsic() {
            return type_id;
        }
        match self.interner().lookup(type_id) {
            Some(TypeData::Application(app_id)) => {
                let app = self.interner().type_application(app_id);
                // Check if this is Array<T>
                if app.base == array_base && app.args.len() == 1 {
                    // Simplify Array<T> to T[]
                    return self.interner().array(app.args[0]);
                }
                // Not an array application, return as-is
                type_id
            }
            Some(TypeData::Callable(callable_id)) => {
                // Simplify function return types
                let shape = self.interner().callable_shape(callable_id);
                let mut simplified_call_sigs = Vec::new();
                let mut simplified_construct_sigs = Vec::new();
                let mut changed = false;

                // Simplify call signatures
                for sig in &shape.call_signatures {
                    let simplified_return =
                        self.simplify_array_application(sig.return_type, array_base);
                    if simplified_return != sig.return_type {
                        changed = true;
                        let mut new_sig = sig.clone();
                        new_sig.return_type = simplified_return;
                        simplified_call_sigs.push(new_sig);
                    } else {
                        simplified_call_sigs.push(sig.clone());
                    }
                }

                // Simplify construct signatures
                for sig in &shape.construct_signatures {
                    let simplified_return =
                        self.simplify_array_application(sig.return_type, array_base);
                    if simplified_return != sig.return_type {
                        changed = true;
                        let mut new_sig = sig.clone();
                        new_sig.return_type = simplified_return;
                        simplified_construct_sigs.push(new_sig);
                    } else {
                        simplified_construct_sigs.push(sig.clone());
                    }
                }

                if changed {
                    let mut new_shape = (*shape).clone();
                    new_shape.call_signatures = simplified_call_sigs;
                    new_shape.construct_signatures = simplified_construct_sigs;
                    self.interner().callable(new_shape)
                } else {
                    type_id
                }
            }
            Some(TypeData::Union(list_id)) => {
                // Simplify union members
                let members = self.interner().type_list(list_id);
                let simplified_members: Vec<TypeId> = members
                    .iter()
                    .map(|&m| self.simplify_array_application(m, array_base))
                    .collect();

                // Check if any member changed
                if simplified_members
                    .iter()
                    .zip(members.iter())
                    .any(|(s, o)| s != o)
                {
                    self.interner().union(simplified_members)
                } else {
                    type_id
                }
            }
            Some(TypeData::Intersection(list_id)) => {
                // Simplify intersection members
                let members = self.interner().type_list(list_id);
                let simplified_members: Vec<TypeId> = members
                    .iter()
                    .map(|&m| self.simplify_array_application(m, array_base))
                    .collect();

                // Check if any member changed
                if simplified_members
                    .iter()
                    .zip(members.iter())
                    .any(|(s, o)| s != o)
                {
                    self.interner().intersection(simplified_members)
                } else {
                    type_id
                }
            }
            _ => type_id,
        }
    }

    pub(crate) fn array_element_type(&self, array_type: TypeId) -> TypeId {
        if array_type.is_intrinsic() {
            return TypeId::ERROR;
        }
        match self.interner().lookup(array_type) {
            Some(TypeData::Array(elem)) => elem,
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner().tuple_list(elements);
                self.tuple_element_union(&elements)
            }
            _ => TypeId::ERROR, // Return ERROR instead of ANY for non-array/tuple types
        }
    }

    fn tuple_element_union(&self, elements: &[TupleElement]) -> TypeId {
        let mut members = Vec::new();
        for elem in elements {
            let mut ty = if elem.rest {
                self.array_element_type(elem.type_id)
            } else {
                elem.type_id
            };
            if elem.optional {
                ty = self.element_type_with_undefined(ty);
            }
            members.push(ty);
        }
        self.interner().union(members)
    }

    fn tuple_fixed_element_type(&self, elements: &[TupleElement], index: usize) -> Option<TypeId> {
        crate::operations::sequence_property::tuple_fixed_element_type(
            self.interner(),
            elements,
            index,
        )
    }

    fn element_type_with_undefined(&self, element_type: TypeId) -> TypeId {
        crate::operations::sequence_property::element_type_with_undefined(
            self.interner(),
            element_type,
        )
    }

    fn compute_tuple_length_type(&self, type_id: TypeId) -> Option<TypeId> {
        let (min, max) = self.compute_tuple_length_bounds(type_id)?;
        if min == max {
            return Some(self.interner().literal_number(max as f64));
        }

        let members = (min..=max)
            .map(|len| self.interner().literal_number(len as f64))
            .collect();
        Some(self.interner().union(members))
    }

    fn compute_tuple_length_bounds(&self, type_id: TypeId) -> Option<(usize, usize)> {
        const MAX_FIXED_LENGTH: usize = 1000;

        if type_id.is_intrinsic() {
            return None;
        }
        let list_id = match self.interner().lookup(type_id) {
            Some(TypeData::Tuple(id)) => id,
            _ => return None,
        };

        let elements = self.interner().tuple_list(list_id);
        let mut min = 0usize;
        let mut max = 0usize;
        let mut rest_type: Option<TypeId> = None;
        let mut rest_count = 0;

        for elem in elements.iter() {
            if elem.rest {
                rest_count += 1;
                if rest_count > 1 {
                    return None;
                }
                rest_type = Some(elem.type_id);
            } else {
                if !elem.optional {
                    min += 1;
                }
                max += 1;
                if max > MAX_FIXED_LENGTH {
                    return None;
                }
            }
        }

        while let Some(rest_id) = rest_type.take() {
            if rest_id.is_intrinsic() {
                return None;
            }
            let inner_list_id = match self.interner().lookup(rest_id) {
                Some(TypeData::Tuple(id)) => id,
                _ => return None,
            };
            let inner_elements = self.interner().tuple_list(inner_list_id);
            let mut inner_rest_count = 0;
            for elem in inner_elements.iter() {
                if elem.rest {
                    inner_rest_count += 1;
                    if inner_rest_count > 1 {
                        return None;
                    }
                    rest_type = Some(elem.type_id);
                } else {
                    if !elem.optional {
                        min += 1;
                    }
                    max += 1;
                    if max > MAX_FIXED_LENGTH {
                        return None;
                    }
                }
            }
        }

        Some((min, max))
    }

    pub(super) fn resolve_function_property(
        &self,
        func_type: TypeId,
        prop_name: &str,
        prop_atom: Atom,
    ) -> PropertyAccessResult {
        // STEP 1: Consult the boxed `Function` interface from lib.d.ts FIRST so
        // user augmentations (e.g., `interface Function { now(): string; }`)
        // and target-specific lib differences win over the hardcoded list
        // below. Mirrors the primitive resolver at `resolve_intrinsic_property`
        // — boxed first, hardcoded only as a no-lib bootstrap.
        //
        // Robustness audit (PR #O, item 15 in
        // `docs/architecture/ROBUSTNESS_AUDIT_2026-04-26.md`).
        let boxed_function_loaded = if let Some(boxed_type) =
            crate::def::resolver::TypeResolver::get_boxed_type(self.db, IntrinsicKind::Function)
        {
            let result = self.resolve_property_access_inner(boxed_type, prop_name, Some(prop_atom));
            if !result.is_not_found() {
                return result;
            }
            true
        } else {
            false
        };

        // STEP 2: Hardcoded well-known Function members (no-lib / bootstrap path).
        // Reached when the boxed `Function` interface is unavailable (no lib loaded)
        // or didn't resolve the property. We emit a structured trace event so
        // drift (e.g. tests inadvertently bootstrapping with no-lib semantics)
        // is visible at runtime — see robustness audit item 15 / PR #O.
        //
        // `name` is gated behind `!boxed_function_loaded`: it was added to the
        // `Function` interface in lib.es2015.core.d.ts. When a lib older than
        // es2015 is loaded explicitly, the boxed lookup correctly reports the
        // property as absent, and the bootstrap fallback must not paper over
        // that not-found result with the hardcoded `name => string` entry.
        // Other entries here are core es5 members that legitimately fall
        // back even when the boxed interface lookup misses (e.g. lookup on a
        // synthesized constructor-signature intersection that does not
        // navigate to the boxed `Function` interface).
        let hardcoded_match = match prop_name {
            "apply" | "call" | "bind" => Some(self.method_result(TypeId::ANY)),
            "toString" => Some(self.method_result(TypeId::STRING)),
            "name" if !boxed_function_loaded => Some(PropertyAccessResult::simple(TypeId::STRING)),
            "length" => Some(PropertyAccessResult::simple(TypeId::NUMBER)),
            "prototype" | "arguments" => Some(PropertyAccessResult::simple(TypeId::ANY)),
            "caller" => Some(PropertyAccessResult::simple(
                self.any_args_function(TypeId::ANY),
            )),
            _ => None,
        };
        if let Some(result) = hardcoded_match {
            tracing::trace!(
                target: "tsz_solver::function_hardcoded_fallback",
                prop_name = prop_name,
                "Function property resolved via hardcoded no-lib fallback"
            );
            return result;
        }

        if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
            return result;
        }

        PropertyAccessResult::PropertyNotFound {
            type_id: func_type,
            property_name: prop_atom,
        }
    }
}
