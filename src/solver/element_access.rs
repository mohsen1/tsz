use crate::solver::evaluate::{evaluate_index_access_with_options, evaluate_type};
use crate::solver::subtype::is_subtype_of;
use crate::solver::{LiteralValue, TypeDatabase, TypeId, TypeKey};

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

    pub fn set_no_unchecked_indexed_access(&mut self, enabled: bool) {
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
        if let Some(TypeKey::Tuple(elements)) = self.interner.lookup(evaluated_object) {
            if let Some(index) = literal_index {
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
        }

        // 3. Check for index signature (if not a specific property access)
        if result_type == TypeId::UNDEFINED {
            if self.should_report_no_index_signature(evaluated_object, index_type) {
                return ElementAccessResult::NoIndexSignature {
                    type_id: evaluated_object,
                };
            }
        }

        ElementAccessResult::Success(result_type)
    }

    fn is_indexable(&self, type_id: TypeId) -> bool {
        match self.interner.lookup(type_id) {
            Some(TypeKey::Array(_))
            | Some(TypeKey::Tuple(_))
            | Some(TypeKey::Object(_))
            | Some(TypeKey::ObjectWithIndex(_))
            | Some(TypeKey::StringIntrinsic { .. })
            | Some(TypeKey::Literal(LiteralValue::String(_)))
            | Some(TypeKey::Intersection(_)) => true,
            Some(TypeKey::Union(members)) => {
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
        // Simplified check: checking if object has index signature compatible with index_type
        match self.interner.lookup(object_type) {
            Some(TypeKey::Object(_)) => {
                // Object without explicit index signature
                // If index_type is string/number (not literal), then it's an error
                // unless it's ANY
                if is_subtype_of(self.interner, index_type, TypeId::STRING)
                    || is_subtype_of(self.interner, index_type, TypeId::NUMBER)
                {
                    return true;
                }
                false
            }
            Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                if is_subtype_of(self.interner, index_type, TypeId::STRING)
                    && shape.string_index.is_none()
                {
                    return true;
                }
                // For number index, we can fallback to string index if present
                if is_subtype_of(self.interner, index_type, TypeId::NUMBER) {
                    if shape.number_index.is_none() && shape.string_index.is_none() {
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }
}
