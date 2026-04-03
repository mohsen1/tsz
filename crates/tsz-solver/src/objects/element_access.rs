use crate::evaluation::evaluate::{evaluate_index_access_with_options, evaluate_type};
use crate::{LiteralValue, TypeData, TypeDatabase, TypeId};

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

        // Use the existing index access evaluator to get the type
        let result_type = evaluate_index_access_with_options(
            self.interner,
            object_type,
            index_type,
            self.no_unchecked_indexed_access,
        );

        // 1. Check if object is indexable
        if !self.is_indexable(evaluated_object) {
            return ElementAccessResult::NotIndexable {
                type_id: evaluated_object,
            };
        }

        // 2. Check for Tuple out of bounds
        if let Some(TypeData::Tuple(elements)) = self.interner.lookup(evaluated_object)
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
        if let Some(TypeData::Union(members_id)) = self.interner.lookup(evaluated_object)
            && let Some(index) = literal_index
        {
            let members = self.interner.type_list(members_id);
            let mut all_out_of_bounds = true;
            let mut has_any_tuple = false;
            for &member in members.iter() {
                if let Some(TypeData::Tuple(elems)) = self.interner.lookup(member) {
                    has_any_tuple = true;
                    let tuple_elements = self.interner.tuple_list(elems);
                    let has_rest = tuple_elements.iter().any(|e| e.rest);
                    if has_rest || index < tuple_elements.len() {
                        all_out_of_bounds = false;
                        break;
                    }
                } else {
                    // Non-tuple member — can't determine bounds
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
            && self.should_report_no_index_signature(evaluated_object, index_type)
        {
            return ElementAccessResult::NoIndexSignature {
                type_id: evaluated_object,
            };
        }

        ElementAccessResult::Success(result_type)
    }

    fn is_indexable(&self, type_id: TypeId) -> bool {
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
            Some(TypeData::Union(members)) => {
                let members = self.interner.type_list(members);
                members.iter().all(|&m| self.is_indexable(m))
            }
            _ => {
                if type_id == TypeId::STRING || type_id == TypeId::ANY {
                    return true;
                }
                false
            }
        }
    }

    fn should_report_no_index_signature(&self, object_type: TypeId, index_type: TypeId) -> bool {
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
}
