//! Solver-owned type construction facade.
//!
//! Keeps checker code on a narrow constructor surface so it cannot
//! interact with raw type internals.

use crate::db::TypeDatabase;
use crate::types::{
    CallableShape, ConditionalType, FunctionShape, MappedType, ObjectFlags, ObjectShape, PropertyInfo,
    TemplateSpan, TupleElement, TypeId, TypeParamInfo,
};
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;

#[derive(Clone, Copy)]
pub struct TypeFactory<'db> {
    db: &'db dyn TypeDatabase,
}

impl<'db> TypeFactory<'db> {
    pub(crate) fn new(db: &'db dyn TypeDatabase) -> Self {
        Self { db }
    }

    #[inline]
    pub fn literal_string(&self, value: &str) -> TypeId {
        self.db.literal_string(value)
    }

    #[inline]
    pub fn literal_number(&self, value: f64) -> TypeId {
        self.db.literal_number(value)
    }

    #[inline]
    pub fn literal_boolean(&self, value: bool) -> TypeId {
        self.db.literal_boolean(value)
    }

    #[inline]
    pub fn literal_bigint(&self, value: &str) -> TypeId {
        self.db.literal_bigint(value)
    }

    #[inline]
    pub fn literal_bigint_with_sign(&self, negative: bool, digits: &str) -> TypeId {
        self.db.literal_bigint_with_sign(negative, digits)
    }

    #[inline]
    pub fn literal_string_atom(&self, atom: Atom) -> TypeId {
        self.db.literal_string_atom(atom)
    }

    #[inline]
    pub fn union(&self, members: Vec<TypeId>) -> TypeId {
        self.db.union(members)
    }

    #[inline]
    pub fn intersection(&self, members: Vec<TypeId>) -> TypeId {
        self.db.intersection(members)
    }

    #[inline]
    pub fn array(&self, element: TypeId) -> TypeId {
        self.db.array(element)
    }

    #[inline]
    pub fn tuple(&self, elements: Vec<TupleElement>) -> TypeId {
        self.db.tuple(elements)
    }

    #[inline]
    pub fn object(&self, properties: Vec<PropertyInfo>) -> TypeId {
        self.db.object(properties)
    }

    #[inline]
    pub fn object_with_flags(&self, properties: Vec<PropertyInfo>, flags: ObjectFlags) -> TypeId {
        self.db.object_with_flags(properties, flags)
    }

    #[inline]
    pub fn object_fresh(&self, properties: Vec<PropertyInfo>) -> TypeId {
        self.db.object_fresh(properties)
    }

    #[inline]
    pub fn object_with_index(&self, shape: ObjectShape) -> TypeId {
        self.db.object_with_index(shape)
    }

    #[inline]
    pub fn object_with_flags_and_symbol(
        &self,
        properties: Vec<PropertyInfo>,
        flags: ObjectFlags,
        symbol: Option<SymbolId>,
    ) -> TypeId {
        self.db
            .object_with_flags_and_symbol(properties, flags, symbol)
    }

    #[inline]
    pub fn function(&self, shape: FunctionShape) -> TypeId {
        self.db.function(shape)
    }

    #[inline]
    pub fn callable(&self, shape: CallableShape) -> TypeId {
        self.db.callable(shape)
    }

    #[inline]
    pub fn template_literal(&self, spans: Vec<TemplateSpan>) -> TypeId {
        self.db.template_literal(spans)
    }

    #[inline]
    pub fn conditional(&self, conditional: ConditionalType) -> TypeId {
        self.db.conditional(conditional)
    }

    #[inline]
    pub fn mapped(&self, mapped: MappedType) -> TypeId {
        self.db.mapped(mapped)
    }

    #[inline]
    pub fn reference(&self, symbol: crate::types::SymbolRef) -> TypeId {
        self.db.reference(symbol)
    }

    #[inline]
    pub fn lazy(&self, def_id: crate::def::DefId) -> TypeId {
        self.db.lazy(def_id)
    }

    #[inline]
    pub fn bound_parameter(&self, index: u32) -> TypeId {
        self.db.bound_parameter(index)
    }

    #[inline]
    pub fn recursive(&self, depth: u32) -> TypeId {
        self.db.recursive(depth)
    }

    #[inline]
    pub fn type_param(&self, info: TypeParamInfo) -> TypeId {
        self.db.type_param(info)
    }

    #[inline]
    pub fn type_query(&self, symbol: crate::types::SymbolRef) -> TypeId {
        self.db.type_query(symbol)
    }

    #[inline]
    pub fn enum_type(&self, def_id: crate::def::DefId, structural_type: TypeId) -> TypeId {
        self.db.enum_type(def_id, structural_type)
    }

    #[inline]
    pub fn application(&self, base: TypeId, args: Vec<TypeId>) -> TypeId {
        self.db.application(base, args)
    }

    #[inline]
    pub fn union_preserve_members(&self, members: Vec<TypeId>) -> TypeId {
        self.db.union_preserve_members(members)
    }

    #[inline]
    pub fn readonly_type(&self, inner: TypeId) -> TypeId {
        self.db.readonly_type(inner)
    }

    #[inline]
    pub fn keyof(&self, inner: TypeId) -> TypeId {
        self.db.keyof(inner)
    }

    #[inline]
    pub fn index_access(&self, object_type: TypeId, index_type: TypeId) -> TypeId {
        self.db.index_access(object_type, index_type)
    }
}
