use std::collections::HashSet;

use super::Checker;
use crate::semantics::types::{ObjectShape, TypeId, TypeKind};

macro_rules! cacheable_signature {
    ($checker:expr, $signature:expr, $active:expr) => {
        $signature
            .parameters
            .iter()
            .all(|parameter| $checker.is_cacheable_type_inner(parameter.ty, $active))
            && $checker.is_cacheable_type_inner($signature.return_type, $active)
    };
}

impl Checker<'_> {
    /// Definitive caches may only retain graphs whose complete structure is
    /// itself definitive. A clean outer array/object/union must not conceal
    /// an error, invalid projection, or deferred query in a nested position.
    pub(super) fn is_cacheable_type(&self, ty: TypeId) -> bool {
        self.is_cacheable_type_inner(ty, &mut HashSet::new())
    }

    fn is_cacheable_type_inner(&self, ty: TypeId, active: &mut HashSet<TypeId>) -> bool {
        if !active.insert(ty) {
            return true;
        }
        let cacheable = match self.store.kind(ty) {
            TypeKind::Error | TypeKind::Invalid(_) | TypeKind::Deferred(_) => false,
            TypeKind::Array(element) => self.is_cacheable_type_inner(*element, active),
            TypeKind::Tuple(elements)
            | TypeKind::Union(elements)
            | TypeKind::Intersection(elements) => elements
                .iter()
                .all(|element| self.is_cacheable_type_inner(*element, active)),
            TypeKind::Object(shape) => self.is_cacheable_object_shape(shape, active),
            TypeKind::ClassInstance {
                arguments,
                properties,
                ..
            } => {
                arguments
                    .iter()
                    .all(|argument| self.is_cacheable_type_inner(*argument, active))
                    && self.is_cacheable_object_shape(properties, active)
            }
            TypeKind::Function(signature) => cacheable_signature!(self, signature, active),
            TypeKind::ShapeFunction(signature) => cacheable_signature!(self, signature, active),
            TypeKind::Any
            | TypeKind::Unknown
            | TypeKind::Never
            | TypeKind::Void
            | TypeKind::Undefined
            | TypeKind::Null
            | TypeKind::Boolean
            | TypeKind::Number
            | TypeKind::String
            | TypeKind::BigInt
            | TypeKind::ObjectKeyword
            | TypeKind::Symbol
            | TypeKind::LiteralBoolean(_, _)
            | TypeKind::LiteralNumber(_, _)
            | TypeKind::LiteralString(_, _)
            | TypeKind::TypeParameter { .. }
            | TypeKind::ClassConstructor { .. } => true,
        };
        active.remove(&ty);
        cacheable
    }

    fn is_cacheable_object_shape(&self, shape: &ObjectShape, active: &mut HashSet<TypeId>) -> bool {
        shape
            .properties
            .iter()
            .all(|property| self.is_cacheable_type_inner(property.ty, active))
            && shape
                .call_signatures
                .iter()
                .chain(&shape.construct_signatures)
                .all(|signature| cacheable_signature!(self, signature, active))
            && shape
                .index_signatures
                .iter()
                .all(|index| self.is_cacheable_type_inner(index.value, active))
    }
}
