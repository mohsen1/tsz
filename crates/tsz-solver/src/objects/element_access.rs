use crate::construction::TypeDatabase;
use crate::evaluation::evaluate::{evaluate_index_access_with_options, evaluate_type};
use crate::{LiteralValue, TypeData, TypeId};

#[derive(Debug, Clone)]
pub enum ElementAccessResult {
    Success(TypeId),
    NotIndexable {
        type_id: TypeId,
    },
    IndexOutOfBounds {
        type_id: TypeId,
        index: usize,
        length: usize,
    },
    NoIndexSignature {
        type_id: TypeId,
    },
    /// Numeric property does not exist on any member of a union type.
    /// Used when all tuple members of a union are out of bounds for a literal index.
    PropertyNotFound {
        /// The original union type (for diagnostic message)
        type_id: TypeId,
        /// The literal index that was accessed
        index: usize,
    },
}

pub struct ElementAccessEvaluator<'a> {
    interner: &'a dyn TypeDatabase,
    no_unchecked_indexed_access: bool,
}

impl<'a> ElementAccessEvaluator<'a> {
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        Self {
            interner,
            no_unchecked_indexed_access: false,
        }
    }

    pub const fn set_no_unchecked_indexed_access(&mut self, enabled: bool) {
        self.no_unchecked_indexed_access = enabled;
    }

    pub fn resolve_element_access(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> ElementAccessResult {
        // Evaluate object type first
        let evaluated_object = evaluate_type(self.interner, object_type);

        // Handle error/any
        if evaluated_object == TypeId::ERROR || index_type == TypeId::ERROR {
            return ElementAccessResult::Success(TypeId::ERROR);
        }
        if evaluated_object == TypeId::ANY {
            return ElementAccessResult::Success(TypeId::ANY);
        }

        // Structural index-access checks (indexability, tuple bounds, missing
        // index signature) operate on the apparent type: a `TypeParameter` /
        // `Infer` is indexed via the apparent type of its base constraint,
        // matching tsc's `getApparentType`. The result-type computation already
        // routes through `evaluate_index_access`, which substitutes the
        // constraint when concrete; without this apparent-type walk, the
        // structural gate below would reject every generic-parameter receiver
        // as `NotIndexable` and collapse `t[K]` to `TypeId::ERROR`.
        let apparent_object =
            crate::type_queries::get_base_constraint_or_type(self.interner, evaluated_object);

        // Use the existing index access evaluator to get the type
        let result_type = evaluate_index_access_with_options(
            self.interner,
            object_type,
            index_type,
            self.no_unchecked_indexed_access,
        );

        // 1. Check if object is indexable
        if !self.is_indexable(apparent_object) {
            return ElementAccessResult::NotIndexable {
                type_id: evaluated_object,
            };
        }

        // 2. Check for Tuple out of bounds.
        // Also handle ReadonlyType(Tuple) — readonly tuples have the same positional
        // bounds as their inner tuple; the wrapper only restricts writes.
        let tuple_inner = self.unwrap_readonly_tuple(apparent_object);
        if let Some(TypeData::Tuple(elements)) = self.interner.lookup(tuple_inner)
            && let Some(index) = literal_index
        {
            let tuple_elements = self.interner.tuple_list(elements);

            // Check bounds if no rest element
            let has_rest = tuple_elements.iter().any(|e| e.rest);
            if !has_rest && index >= tuple_elements.len() {
                return ElementAccessResult::IndexOutOfBounds {
                    type_id: evaluated_object,
                    index,
                    length: tuple_elements.len(),
                };
            }
        }

        // 2b. Check for union-of-tuples out of bounds.
        // When all tuple members of a union are out of bounds for a literal index,
        // tsc emits TS2339 "Property 'N' does not exist on type 'X'".
        // Union members may be ReadonlyType(Tuple) — unwrap those too.
        if let Some(TypeData::Union(members_id)) = self.interner.lookup(apparent_object)
            && let Some(index) = literal_index
        {
            let members = self.interner.type_list(members_id);
            let mut all_out_of_bounds = true;
            let mut has_any_tuple = false;
            for &member in members.iter() {
                let member_inner = self.unwrap_readonly_tuple(member);
                if let Some(TypeData::Tuple(elems)) = self.interner.lookup(member_inner) {
                    has_any_tuple = true;
                    let tuple_elements = self.interner.tuple_list(elems);
                    let has_rest = tuple_elements.iter().any(|e| e.rest);
                    if has_rest || index < tuple_elements.len() {
                        all_out_of_bounds = false;
                        break;
                    }
                } else {
                    all_out_of_bounds = false;
                    break;
                }
            }
            if has_any_tuple && all_out_of_bounds {
                return ElementAccessResult::PropertyNotFound {
                    type_id: evaluated_object,
                    index,
                };
            }
        }

        // 3. Check for index signature (if not a specific property access)
        if result_type == TypeId::UNDEFINED
            && self.should_report_no_index_signature(apparent_object, index_type)
        {
            return ElementAccessResult::NoIndexSignature {
                type_id: evaluated_object,
            };
        }

        ElementAccessResult::Success(result_type)
    }

    /// Strip a `ReadonlyType` wrapper when the inner type is a `Tuple`,
    /// returning the original `type_id` unchanged for all other shapes.
    fn unwrap_readonly_tuple(&self, type_id: TypeId) -> TypeId {
        let inner = crate::type_queries::unwrap_readonly(self.interner, type_id);
        if matches!(self.interner.lookup(inner), Some(TypeData::Tuple(_))) {
            inner
        } else {
            type_id
        }
    }

    fn is_indexable(&self, type_id: TypeId) -> bool {
        // STRING and ANY are explicitly indexable; other intrinsics aren't.
        // BOOLEAN_TRUE/FALSE intrinsics resolve to Literal(Boolean), which the
        // match below doesn't catch (only Literal(String) is indexable), so
        // they correctly fall to false.
        if type_id == TypeId::STRING || type_id == TypeId::ANY {
            return true;
        }
        if type_id.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(type_id) {
            Some(
                TypeData::Array(_)
                | TypeData::Tuple(_)
                | TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Callable(_)
                | TypeData::StringIntrinsic { .. }
                | TypeData::Literal(LiteralValue::String(_))
                | TypeData::Intersection(_)
                | TypeData::Mapped(_),
            ) => true,
            Some(TypeData::ReadonlyType(inner)) => self.is_indexable(inner),
            Some(TypeData::Union(members)) => {
                let members = self.interner.type_list(members);
                members.iter().all(|&m| self.is_indexable(m))
            }
            _ => false,
        }
    }

    fn should_report_no_index_signature(&self, object_type: TypeId, index_type: TypeId) -> bool {
        if object_type.is_intrinsic() {
            return false;
        }
        let index_type = evaluate_type(self.interner, index_type);
        // PERF: Reuse a single SubtypeChecker across all checks
        let mut checker = crate::relations::subtype::SubtypeChecker::new(self.interner);
        // Simplified check: checking if object has index signature compatible with index_type
        match self.interner.lookup(object_type) {
            Some(TypeData::Object(_)) => {
                // Object without explicit index signature
                // If index_type is string/number (not literal), then it's an error
                // unless it's ANY
                checker.reset();
                if checker.is_subtype_of(index_type, TypeId::STRING) {
                    return true;
                }
                checker.reset();
                if checker.is_subtype_of(index_type, TypeId::NUMBER) {
                    return true;
                }
                false
            }
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                // When a string index exists with a restricted key type (e.g. a template
                // literal), check whether index_type actually satisfies that key type.
                // For example, `{ [key: \`on${string}\`]: V }["someKey"]` should report
                // NoIndexSignature because "someKey" is not assignable to `on${string}`.
                if let Some(sig) = shape.string_index.as_ref()
                    && sig.key_type != TypeId::STRING
                    && sig.key_type != TypeId::SYMBOL
                {
                    checker.reset();
                    if checker.is_subtype_of(index_type, TypeId::STRING) {
                        checker.reset();
                        return !checker.is_subtype_of(index_type, sig.key_type);
                    }
                    return false;
                }
                checker.reset();
                if checker.is_subtype_of(index_type, TypeId::STRING) && shape.string_index.is_none()
                {
                    return true;
                }
                // For number index, we can fallback to string index if present
                checker.reset();
                if checker.is_subtype_of(index_type, TypeId::NUMBER)
                    && shape.number_index.is_none()
                    && shape.string_index.is_none()
                {
                    return true;
                }
                false
            }
            Some(TypeData::Callable(shape_id)) => {
                let shape = self.interner.callable_shape(shape_id);
                checker.reset();
                if checker.is_subtype_of(index_type, TypeId::STRING) && shape.string_index.is_none()
                {
                    return true;
                }
                checker.reset();
                if checker.is_subtype_of(index_type, TypeId::NUMBER)
                    && shape.number_index.is_none()
                    && shape.string_index.is_none()
                {
                    return true;
                }
                false
            }
            Some(TypeData::ReadonlyType(inner)) => {
                // Readonly-wrapping does not add or remove an index signature.
                self.should_report_no_index_signature(inner, index_type)
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intern::TypeInterner;
    use crate::types::{MappedType, TypeParamInfo};
    use tsz_common::interner::Atom;

    #[test]
    fn mapped_type_is_indexable() {
        let interner = TypeInterner::new();

        // Create a mapped type: { [P in K as `get${P}`]: { a: P } }
        let type_param = interner.type_param(TypeParamInfo {
            name: Atom::NONE,
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
        });
        let mapped_with_as = interner.mapped(MappedType {
            type_param: TypeParamInfo {
                name: Atom::NONE,
                constraint: Some(TypeId::STRING),
                default: None,
                is_const: false,
            },
            constraint: type_param,
            name_type: Some(TypeId::STRING), // has as-clause
            template: TypeId::STRING,
            readonly_modifier: None,
            optional_modifier: None,
        });

        let evaluator = ElementAccessEvaluator::new(&interner);
        assert!(
            evaluator.is_indexable(mapped_with_as),
            "Mapped type with as-clause should be indexable"
        );
    }

    #[test]
    fn mapped_type_without_as_clause_is_indexable() {
        let interner = TypeInterner::new();

        let type_param = interner.type_param(TypeParamInfo {
            name: Atom::NONE,
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
        });
        let mapped_no_as = interner.mapped(MappedType {
            type_param: TypeParamInfo {
                name: Atom::NONE,
                constraint: Some(TypeId::STRING),
                default: None,
                is_const: false,
            },
            constraint: type_param,
            name_type: None, // no as-clause
            template: TypeId::STRING,
            readonly_modifier: None,
            optional_modifier: None,
        });

        let evaluator = ElementAccessEvaluator::new(&interner);
        assert!(
            evaluator.is_indexable(mapped_no_as),
            "Mapped type without as-clause should be indexable"
        );
    }

    #[test]
    fn union_of_mapped_types_is_indexable() {
        let interner = TypeInterner::new();

        let type_param = interner.type_param(TypeParamInfo {
            name: Atom::NONE,
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
        });
        let mapped = interner.mapped(MappedType {
            type_param: TypeParamInfo {
                name: Atom::NONE,
                constraint: Some(TypeId::STRING),
                default: None,
                is_const: false,
            },
            constraint: type_param,
            name_type: Some(TypeId::STRING),
            template: TypeId::NUMBER,
            readonly_modifier: None,
            optional_modifier: None,
        });

        let obj = interner.object(vec![]);
        let union = interner.union2(mapped, obj);

        let evaluator = ElementAccessEvaluator::new(&interner);
        assert!(
            evaluator.is_indexable(union),
            "Union of mapped type and object should be indexable"
        );
    }

    #[test]
    fn readonly_array_is_indexable() {
        let interner = TypeInterner::new();
        let arr = interner.array(TypeId::NUMBER);
        let readonly_arr = interner.readonly_type(arr);

        let evaluator = ElementAccessEvaluator::new(&interner);
        assert!(
            evaluator.is_indexable(readonly_arr),
            "ReadonlyType(Array(number)) should be indexable"
        );
    }

    fn make_readonly_num_str_tuple(interner: &TypeInterner) -> TypeId {
        use crate::types::TupleElement;
        let tuple = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        interner.readonly_type(tuple)
    }

    #[test]
    fn readonly_tuple_is_indexable() {
        let interner = TypeInterner::new();
        let readonly_tuple = make_readonly_num_str_tuple(&interner);
        let evaluator = ElementAccessEvaluator::new(&interner);
        assert!(
            evaluator.is_indexable(readonly_tuple),
            "ReadonlyType(Tuple) should be indexable"
        );
    }

    #[test]
    fn union_of_readonly_arrays_is_indexable() {
        let interner = TypeInterner::new();
        let arr_num = interner.array(TypeId::NUMBER);
        let readonly_arr_num = interner.readonly_type(arr_num);
        let arr_str = interner.array(TypeId::STRING);
        let readonly_arr_str = interner.readonly_type(arr_str);
        let union = interner.union2(readonly_arr_num, readonly_arr_str);

        let evaluator = ElementAccessEvaluator::new(&interner);
        assert!(
            evaluator.is_indexable(union),
            "Union of ReadonlyType(Array) members should be indexable"
        );
    }

    #[test]
    fn readonly_array_element_access_returns_element_type() {
        let interner = TypeInterner::new();
        let arr = interner.array(TypeId::NUMBER);
        let readonly_arr = interner.readonly_type(arr);

        let evaluator = ElementAccessEvaluator::new(&interner);
        let result = evaluator.resolve_element_access(readonly_arr, TypeId::NUMBER, None);
        assert!(
            matches!(result, ElementAccessResult::Success(t) if t == TypeId::NUMBER),
            "Element access on ReadonlyType(Array(number)) with number index should succeed with number type"
        );
    }

    #[test]
    fn readonly_tuple_element_access_in_bounds_succeeds() {
        let interner = TypeInterner::new();
        let readonly_tuple = make_readonly_num_str_tuple(&interner);
        let evaluator = ElementAccessEvaluator::new(&interner);
        let literal_0 = interner.literal_number(0.0);
        let result = evaluator.resolve_element_access(readonly_tuple, literal_0, Some(0));
        assert!(
            matches!(result, ElementAccessResult::Success(_)),
            "In-bounds access on ReadonlyType(Tuple) should succeed"
        );
    }

    #[test]
    fn readonly_tuple_element_access_out_of_bounds() {
        let interner = TypeInterner::new();
        let readonly_tuple = make_readonly_num_str_tuple(&interner);
        let evaluator = ElementAccessEvaluator::new(&interner);
        let literal_2 = interner.literal_number(2.0);
        let result = evaluator.resolve_element_access(readonly_tuple, literal_2, Some(2));
        assert!(
            matches!(
                result,
                ElementAccessResult::IndexOutOfBounds {
                    index: 2,
                    length: 2,
                    ..
                }
            ),
            "Out-of-bounds access on ReadonlyType(Tuple) should return IndexOutOfBounds"
        );
    }

    fn type_param_with_constraint(interner: &TypeInterner, constraint: TypeId) -> TypeId {
        interner.type_param(TypeParamInfo {
            name: Atom::NONE,
            constraint: Some(constraint),
            default: None,
            is_const: false,
        })
    }

    /// `T extends number[]` ⇒ `T[0]` must resolve to `number`, not collapse
    /// to `ERROR` via the indexability gate. This is the structural rule
    /// behind issue #9716.
    #[test]
    fn type_param_constrained_to_array_resolves_element_type() {
        let interner = TypeInterner::new();
        let arr = interner.array(TypeId::NUMBER);
        let t = type_param_with_constraint(&interner, arr);
        let evaluator = ElementAccessEvaluator::new(&interner);
        let literal_0 = interner.literal_number(0.0);
        let result = evaluator.resolve_element_access(t, literal_0, Some(0));
        assert!(
            matches!(result, ElementAccessResult::Success(t) if t == TypeId::NUMBER),
            "T[0] for T extends number[] should evaluate to number, got {result:?}",
        );
    }

    /// Renamed type parameter (`P` instead of `T`) must behave identically:
    /// the rule is structural, not name-based.
    #[test]
    fn renamed_type_param_constrained_to_array_resolves_element_type() {
        let interner = TypeInterner::new();
        let arr = interner.array(TypeId::STRING);
        let p = interner.type_param(TypeParamInfo {
            name: Atom::NONE, // identifier name is irrelevant
            constraint: Some(arr),
            default: None,
            is_const: false,
        });
        let evaluator = ElementAccessEvaluator::new(&interner);
        let literal_0 = interner.literal_number(0.0);
        let result = evaluator.resolve_element_access(p, literal_0, Some(0));
        assert!(
            matches!(result, ElementAccessResult::Success(t) if t == TypeId::STRING),
            "P[0] for P extends string[] should evaluate to string, got {result:?}",
        );
    }

    /// Unconstrained `T` (or `T extends unknown[]`) keeps the element type
    /// of the constraint, which is `unknown` for the bottom case.
    #[test]
    fn type_param_constrained_to_unknown_array_resolves_unknown() {
        let interner = TypeInterner::new();
        let arr = interner.array(TypeId::UNKNOWN);
        let t = type_param_with_constraint(&interner, arr);
        let evaluator = ElementAccessEvaluator::new(&interner);
        let literal_0 = interner.literal_number(0.0);
        let result = evaluator.resolve_element_access(t, literal_0, Some(0));
        assert!(
            matches!(result, ElementAccessResult::Success(t) if t == TypeId::UNKNOWN),
            "T[0] for T extends unknown[] should evaluate to unknown, got {result:?}",
        );
    }

    /// Tuple constraint: `T extends [string, number]` ⇒ `T[0]` resolves to
    /// the constrained tuple's element type.
    #[test]
    fn type_param_constrained_to_tuple_resolves_positional_element() {
        let interner = TypeInterner::new();
        let tuple = interner.tuple(vec![
            crate::types::TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
            crate::types::TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        let t = type_param_with_constraint(&interner, tuple);
        let evaluator = ElementAccessEvaluator::new(&interner);
        let literal_0 = interner.literal_number(0.0);
        let r0 = evaluator.resolve_element_access(t, literal_0, Some(0));
        assert!(
            matches!(r0, ElementAccessResult::Success(t) if t == TypeId::STRING),
            "T[0] for T extends [string, number] should be string, got {r0:?}",
        );
        let literal_1 = interner.literal_number(1.0);
        let r1 = evaluator.resolve_element_access(t, literal_1, Some(1));
        assert!(
            matches!(r1, ElementAccessResult::Success(t) if t == TypeId::NUMBER),
            "T[1] for T extends [string, number] should be number, got {r1:?}",
        );
    }

    /// Tuple constraint with an out-of-bounds literal index must surface
    /// `IndexOutOfBounds` (TS2493), not collapse to `NotIndexable` because
    /// the receiver is a type parameter.
    #[test]
    fn type_param_constrained_to_tuple_reports_out_of_bounds() {
        let interner = TypeInterner::new();
        let tuple = interner.tuple(vec![crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        }]);
        let t = type_param_with_constraint(&interner, tuple);
        let evaluator = ElementAccessEvaluator::new(&interner);
        let literal_5 = interner.literal_number(5.0);
        let result = evaluator.resolve_element_access(t, literal_5, Some(5));
        assert!(
            matches!(
                result,
                ElementAccessResult::IndexOutOfBounds {
                    index: 5,
                    length: 1,
                    ..
                }
            ),
            "T[5] for T extends [string] should be IndexOutOfBounds, got {result:?}",
        );
    }

    /// Indirect type-parameter chain: `T extends number[]`, `U extends T` ⇒
    /// `U[0]` must still see the apparent element type through the chain.
    #[test]
    fn nested_type_param_chain_resolves_through_apparent_type() {
        let interner = TypeInterner::new();
        let arr = interner.array(TypeId::NUMBER);
        let t = type_param_with_constraint(&interner, arr);
        let u = type_param_with_constraint(&interner, t);
        let evaluator = ElementAccessEvaluator::new(&interner);
        let literal_0 = interner.literal_number(0.0);
        let result = evaluator.resolve_element_access(u, literal_0, Some(0));
        assert!(
            matches!(result, ElementAccessResult::Success(t) if t == TypeId::NUMBER),
            "U[0] for U extends T extends number[] should be number, got {result:?}",
        );
    }

    /// `T` with a non-indexable constraint (`number`) still reports
    /// `NotIndexable`. The apparent-type walk must preserve the original
    /// negative gate for non-indexable apparent shapes.
    #[test]
    fn type_param_constrained_to_non_indexable_reports_not_indexable() {
        let interner = TypeInterner::new();
        let t = type_param_with_constraint(&interner, TypeId::NUMBER);
        let evaluator = ElementAccessEvaluator::new(&interner);
        let literal_0 = interner.literal_number(0.0);
        let result = evaluator.resolve_element_access(t, literal_0, Some(0));
        assert!(
            matches!(result, ElementAccessResult::NotIndexable { .. }),
            "T[0] for T extends number should be NotIndexable, got {result:?}",
        );
    }

    /// Unconstrained `T` is the implicit-`unknown` case: still not
    /// indexable, no regression.
    #[test]
    fn unconstrained_type_param_reports_not_indexable() {
        let interner = TypeInterner::new();
        let t = interner.type_param(TypeParamInfo {
            name: Atom::NONE,
            constraint: None,
            default: None,
            is_const: false,
        });
        let evaluator = ElementAccessEvaluator::new(&interner);
        let literal_0 = interner.literal_number(0.0);
        let result = evaluator.resolve_element_access(t, literal_0, Some(0));
        assert!(
            matches!(result, ElementAccessResult::NotIndexable { .. }),
            "T[0] for unconstrained T should be NotIndexable, got {result:?}",
        );
    }

    /// An explicit `extends any` constraint must be normalized to `unknown`
    /// for the apparent-type walk, matching `getConstraintFromTypeParameter`.
    /// The receiver is not indexable in this case.
    #[test]
    fn type_param_extends_any_normalizes_to_unknown_and_not_indexable() {
        let interner = TypeInterner::new();
        let t = type_param_with_constraint(&interner, TypeId::ANY);
        let evaluator = ElementAccessEvaluator::new(&interner);
        let literal_0 = interner.literal_number(0.0);
        let result = evaluator.resolve_element_access(t, literal_0, Some(0));
        // The fast `evaluated_object == ANY` short-circuit at the top of
        // resolve_element_access does not fire here because the receiver is
        // a `TypeParameter` whose constraint is `any`. Apparent-type walking
        // normalizes `any` to `unknown`, which is correctly not indexable.
        assert!(
            matches!(result, ElementAccessResult::NotIndexable { .. }),
            "T[0] for T extends any should be NotIndexable after apparent-type \
             normalization, got {result:?}",
        );
    }
}
