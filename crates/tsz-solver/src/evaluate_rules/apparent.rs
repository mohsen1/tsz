//! Apparent type utilities.
//!
//! This module provides utilities for working with apparent types of primitives.
//! Apparent types define the shape of primitive values (e.g., string has .length, .charAt(), etc.)

use crate::apparent::apparent_primitive_members;
use crate::subtype::TypeResolver;
use crate::types::Visibility;
use crate::types::*;
use crate::visitor::{intrinsic_kind, literal_value, template_literal_id};
use crate::{ApparentMemberKind, TypeDatabase};

use super::super::evaluate::TypeEvaluator;

/// Standalone helper to create an apparent method type.
/// Used by both TypeEvaluator and visitors.
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
    pub(crate) fn apparent_literal_kind(&self, literal: &LiteralValue) -> Option<IntrinsicKind> {
        match literal {
            LiteralValue::String(_) => Some(IntrinsicKind::String),
            LiteralValue::Number(_) => Some(IntrinsicKind::Number),
            LiteralValue::BigInt(_) => Some(IntrinsicKind::Bigint),
            LiteralValue::Boolean(_) => Some(IntrinsicKind::Boolean),
        }
    }

    /// Get the apparent object shape for a type if it's a primitive.
    #[allow(dead_code)]
    pub(crate) fn apparent_primitive_shape_for_type(&self, type_id: TypeId) -> Option<ObjectShape> {
        let kind = self.apparent_primitive_kind(type_id)?;
        Some(self.apparent_primitive_shape(kind))
    }

    /// Get the intrinsic kind for a type if it represents a primitive.
    #[allow(dead_code)]
    pub(crate) fn apparent_primitive_kind(&self, type_id: TypeId) -> Option<IntrinsicKind> {
        if let Some(kind) = intrinsic_kind(self.interner(), type_id) {
            return match kind {
                IntrinsicKind::String
                | IntrinsicKind::Number
                | IntrinsicKind::Boolean
                | IntrinsicKind::Bigint
                | IntrinsicKind::Symbol => Some(kind),
                _ => None,
            };
        }

        if let Some(literal) = literal_value(self.interner(), type_id) {
            return match literal {
                LiteralValue::String(_) => Some(IntrinsicKind::String),
                LiteralValue::Number(_) => Some(IntrinsicKind::Number),
                LiteralValue::BigInt(_) => Some(IntrinsicKind::Bigint),
                LiteralValue::Boolean(_) => Some(IntrinsicKind::Boolean),
            };
        }

        if template_literal_id(self.interner(), type_id).is_some() {
            return Some(IntrinsicKind::String);
        }

        None
    }

    /// Build an object shape for a primitive type's apparent members.
    pub(crate) fn apparent_primitive_shape(&self, kind: IntrinsicKind) -> ObjectShape {
        let members = apparent_primitive_members(self.interner(), kind);
        let mut properties = Vec::with_capacity(members.len());

        for member in members {
            let name = self.interner().intern_string(member.name);
            match member.kind {
                ApparentMemberKind::Value(type_id) => properties.push(PropertyInfo {
                    name,
                    type_id,
                    write_type: type_id,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                }),
                ApparentMemberKind::Method(return_type) => properties.push(PropertyInfo {
                    name,
                    type_id: self.apparent_method_type(return_type),
                    write_type: self.apparent_method_type(return_type),
                    optional: false,
                    readonly: false,
                    is_method: true,
                    visibility: Visibility::Public,
                    parent_id: None,
                }),
            }
        }
        properties.sort_by(|a, b| a.name.cmp(&b.name));

        let number_index = if kind == IntrinsicKind::String {
            Some(IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::STRING,
                readonly: false,
            })
        } else {
            None
        };

        ObjectShape {
            flags: ObjectFlags::empty(),
            properties,
            string_index: None,
            number_index,
            symbol: None,
        }
    }

    /// Create a function type representing a method.
    pub(crate) fn apparent_method_type(&self, return_type: TypeId) -> TypeId {
        make_apparent_method_type(self.interner(), return_type)
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
