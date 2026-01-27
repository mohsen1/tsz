//! Array literal type construction.

use crate::solver::TypeDatabase;
use crate::solver::types::*;

/// Builder for constructing array and tuple types from literals.
pub struct ArrayLiteralBuilder<'a> {
    interner: &'a dyn TypeDatabase,
}

impl<'a> ArrayLiteralBuilder<'a> {
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        Self { interner }
    }

    pub fn build_array_type(
        &self,
        element_types: Vec<TypeId>,
        contextual: Option<TypeId>,
    ) -> TypeId {
        if element_types.is_empty() {
            if let Some(ctx) = contextual {
                return ctx;
            }
            return self.interner.array(TypeId::NEVER);
        }

        if let Some(ctx_type) = contextual {
            if let Some(TypeKey::Array(context_elem)) = self.interner.lookup(ctx_type) {
                let all_match = element_types
                    .iter()
                    .all(|&elem| self.is_subtype(elem, context_elem));
                if all_match {
                    return self.interner.array(context_elem);
                }
            }
        }

        let element_type = self.best_common_type(&element_types);
        self.interner.array(element_type)
    }

    pub fn build_tuple_type(&self, elements: Vec<TupleElement>) -> TypeId {
        self.interner.tuple(elements)
    }

    pub fn expand_spread(&self, spread_type: TypeId) -> Vec<TupleElement> {
        if spread_type == TypeId::ANY
            || spread_type == TypeId::UNKNOWN
            || spread_type == TypeId::ERROR
        {
            return vec![TupleElement {
                type_id: spread_type,
                name: None,
                optional: false,
                rest: true,
            }];
        }

        let mut ty = spread_type;
        for _ in 0..100 {
            if let Some(TypeKey::ReadonlyType(inner)) = self.interner.lookup(ty) {
                ty = inner;
            } else {
                break;
            }
        }

        match self.interner.lookup(ty) {
            Some(TypeKey::Tuple(tuple_id)) => {
                let elems = self.interner.tuple_list(tuple_id);
                elems
                    .iter()
                    .map(|e| TupleElement {
                        type_id: e.type_id,
                        name: None,
                        optional: false,
                        rest: false,
                    })
                    .collect()
            }
            Some(TypeKey::Array(elem)) => {
                vec![TupleElement {
                    type_id: elem,
                    name: None,
                    optional: false,
                    rest: true,
                }]
            }
            Some(TypeKey::Union(members_id)) => {
                let members = self.interner.type_list(members_id);
                let mut result = Vec::new();
                for &member in members.iter() {
                    result.extend(self.expand_spread(member));
                }
                result
            }
            _ => vec![TupleElement {
                type_id: spread_type,
                name: None,
                optional: false,
                rest: true,
            }],
        }
    }

    pub fn extract_iterable_element_type(&self, iterable_type: TypeId) -> TypeId {
        if iterable_type == TypeId::ANY
            || iterable_type == TypeId::UNKNOWN
            || iterable_type == TypeId::ERROR
        {
            return iterable_type;
        }

        let mut ty = iterable_type;
        for _ in 0..100 {
            if let Some(TypeKey::ReadonlyType(inner)) = self.interner.lookup(ty) {
                ty = inner;
            } else {
                break;
            }
        }

        match self.interner.lookup(ty) {
            Some(TypeKey::Array(elem)) => elem,
            Some(TypeKey::Tuple(tuple_id)) => {
                let elems = self.interner.tuple_list(tuple_id);
                let types: Vec<TypeId> = elems.iter().map(|e| e.type_id).collect();
                if types.is_empty() {
                    TypeId::NEVER
                } else if types.len() == 1 {
                    types[0]
                } else {
                    self.interner.union(types)
                }
            }
            Some(TypeKey::Union(members_id)) => {
                let members = self.interner.type_list(members_id);
                let types: Vec<TypeId> = members
                    .iter()
                    .map(|&m| self.extract_iterable_element_type(m))
                    .collect();
                self.interner.union(types)
            }
            _ => TypeId::ANY,
        }
    }

    pub fn best_common_type(&self, types: &[TypeId]) -> TypeId {
        if types.is_empty() {
            TypeId::NEVER
        } else if types.len() == 1 {
            types[0]
        } else {
            self.interner.union(types.to_vec())
        }
    }

    fn is_subtype(&self, source: TypeId, target: TypeId) -> bool {
        crate::solver::subtype::is_subtype_of(self.interner, source, target)
    }
}
