//! Apparent type utilities.
//!
//! This module provides utilities for working with apparent types of primitives.
//! Apparent types define the shape of primitive values (e.g., string has .length, .`charAt()`, etc.)

use crate::TypeDatabase;
use crate::objects::apparent::{apparent_primitive_members, apparent_primitive_shape};
use crate::relations::subtype::TypeResolver;
use crate::types::{FunctionShape, IntrinsicKind, LiteralValue, ObjectShape, ParamInfo, TypeId};

use super::super::evaluate::TypeEvaluator;

/// Standalone helper to create an apparent method type.
/// Used by both `TypeEvaluator` and visitors.
pub(crate) fn make_apparent_method_type(db: &dyn TypeDatabase, return_type: TypeId) -> TypeId {
    let rest_array = db.array(TypeId::ANY);
    let rest_param = ParamInfo {
        name: None,
        type_id: rest_array,
        optional: false,
        rest: true,
    };
    db.function(FunctionShape {
        params: vec![rest_param],
        this_type: None,
        return_type,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    })
}

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Get the apparent type kind for a literal value.
    pub(crate) const fn apparent_literal_kind(
        &self,
        literal: &LiteralValue,
    ) -> Option<IntrinsicKind> {
        match literal {
            LiteralValue::String(_) => Some(IntrinsicKind::String),
            LiteralValue::Number(_) => Some(IntrinsicKind::Number),
            LiteralValue::BigInt(_) => Some(IntrinsicKind::Bigint),
            LiteralValue::Boolean(_) => Some(IntrinsicKind::Boolean),
        }
    }

    /// Build an object shape for a primitive type's apparent members.
    pub(crate) fn apparent_primitive_shape(&self, kind: IntrinsicKind) -> ObjectShape {
        apparent_primitive_shape(self.interner(), kind, make_apparent_method_type)
    }

    /// Get keyof for a primitive type.
    pub(crate) fn apparent_primitive_keyof(&self, kind: IntrinsicKind) -> TypeId {
        let members = apparent_primitive_members(self.interner(), kind);
        let mut key_types = Vec::with_capacity(members.len());
        for member in members {
            key_types.push(self.interner().literal_string(member.name));
        }
        if kind == IntrinsicKind::String {
            key_types.push(TypeId::NUMBER);
        }
        if key_types.is_empty() {
            TypeId::NEVER
        } else {
            self.interner().union(key_types)
        }
    }
}
