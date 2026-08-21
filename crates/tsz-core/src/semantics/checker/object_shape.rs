use std::collections::{HashMap, HashSet};

use crate::bind::{ScopeId, TypeMemberSymbol};
use crate::source::FileId;
use crate::syntax::{
    KeywordType, Parameter, TypeMember, TypeMemberKind, TypeMemberModifiers, TypeNode, TypeNodeKind,
};

use super::{Checker, QueryState};
use crate::semantics::types::{
    Completion, DeferredType, IndexKeyKind, IndexSignature, ObjectShape, Property, ShapeParameter,
    ShapeSignature, TypeId, TypeKind,
};

impl Checker<'_> {
    pub(super) fn resolve_object_members(
        &mut self,
        file: FileId,
        scope: ScopeId,
        members: &[TypeMember],
        type_parameters: &HashMap<String, TypeId>,
    ) -> Completion<ObjectShape> {
        let bound = &self.program.files[file.0 as usize].bindings;
        if members
            .iter()
            .filter(|member| !member.recovered)
            .any(|member| {
                bound
                    .type_members
                    .get(&member.id)
                    .is_some_and(|bound_member| {
                        !matches!(bound_member.symbol, Some(TypeMemberSymbol::Index))
                            && bound
                                .type_member_group(member.id)
                                .is_some_and(|group| group.len() > 1)
                    })
            })
        {
            return Completion::Deferred;
        }

        let mut shape = ObjectShape::default();
        for member in members.iter().filter(|member| !member.recovered) {
            let member_scope = self.node_scope(file, member.id, scope);
            if unsupported_modifiers(&member.modifiers) {
                return Completion::Deferred;
            }
            match &member.kind {
                TypeMemberKind::Property {
                    name,
                    ty,
                    optional,
                    initializer,
                } => {
                    if *optional || initializer.is_some() {
                        return Completion::Deferred;
                    }
                    let Some(name) = name.semantic_name() else {
                        return Completion::Deferred;
                    };
                    let ty = if let Some(ty) = ty {
                        let ty = self.resolve_type_node(file, member_scope, ty, type_parameters);
                        match self.shape_child_type_supported(ty, &mut HashSet::new()) {
                            Completion::Complete(()) => ty,
                            Completion::Deferred => return Completion::Deferred,
                            Completion::Cycle => return Completion::Cycle,
                            Completion::Limit => return Completion::Limit,
                        }
                    } else if self.options.effective_no_implicit_any() {
                        return Completion::Deferred;
                    } else {
                        self.store.builtins.any
                    };
                    shape.properties.push(Property {
                        name: name.to_string(),
                        ty,
                        optional: *optional,
                        readonly: member.modifiers.readonly,
                    });
                }
                TypeMemberKind::Method {
                    name,
                    optional,
                    type_parameters: method_type_parameters,
                    parameters,
                    return_type,
                } => {
                    if *optional || member.modifiers.readonly || !method_type_parameters.is_empty()
                    {
                        return Completion::Deferred;
                    }
                    let Some(name) = name.semantic_name() else {
                        return Completion::Deferred;
                    };
                    let signature = match self.resolve_shape_signature(
                        file,
                        member_scope,
                        parameters,
                        return_type.as_ref(),
                        type_parameters,
                    ) {
                        Completion::Complete(signature) => signature,
                        Completion::Deferred => return Completion::Deferred,
                        Completion::Cycle => return Completion::Cycle,
                        Completion::Limit => return Completion::Limit,
                    };
                    let ty = self.store.intern(TypeKind::ShapeFunction(signature));
                    shape.properties.push(Property {
                        name: name.to_string(),
                        ty,
                        optional: *optional,
                        readonly: false,
                    });
                }
                TypeMemberKind::Call {
                    type_parameters: signature_type_parameters,
                    parameters,
                    return_type,
                } => {
                    if member.modifiers.readonly || !signature_type_parameters.is_empty() {
                        return Completion::Deferred;
                    }
                    let signature = match self.resolve_shape_signature(
                        file,
                        member_scope,
                        parameters,
                        return_type.as_ref(),
                        type_parameters,
                    ) {
                        Completion::Complete(signature) => signature,
                        Completion::Deferred => return Completion::Deferred,
                        Completion::Cycle => return Completion::Cycle,
                        Completion::Limit => return Completion::Limit,
                    };
                    shape.call_signatures.push(signature);
                }
                TypeMemberKind::Construct {
                    type_parameters: signature_type_parameters,
                    parameters,
                    return_type,
                } => {
                    if member.modifiers.readonly || !signature_type_parameters.is_empty() {
                        return Completion::Deferred;
                    }
                    let signature = match self.resolve_shape_signature(
                        file,
                        member_scope,
                        parameters,
                        return_type.as_ref(),
                        type_parameters,
                    ) {
                        Completion::Complete(signature) => signature,
                        Completion::Deferred => return Completion::Deferred,
                        Completion::Cycle => return Completion::Cycle,
                        Completion::Limit => return Completion::Limit,
                    };
                    shape.construct_signatures.push(signature);
                }
                TypeMemberKind::Index {
                    parameters,
                    value_type,
                } => {
                    if member.modifiers.readonly {
                        return Completion::Deferred;
                    }
                    let [parameter] = parameters.as_slice() else {
                        return Completion::Deferred;
                    };
                    if parameter.optional
                        || parameter.initializer.is_some()
                        || parameter.rest
                        || !parameter.modifiers.is_empty()
                    {
                        return Completion::Deferred;
                    }
                    let Some(annotation) = &parameter.annotation else {
                        return Completion::Deferred;
                    };
                    let key = match annotation.kind {
                        TypeNodeKind::Keyword(KeywordType::String) => IndexKeyKind::String,
                        TypeNodeKind::Keyword(KeywordType::Number) => IndexKeyKind::Number,
                        _ => return Completion::Deferred,
                    };
                    if key == IndexKeyKind::Number {
                        // Numeric property-key canonicalization is not yet a
                        // semantic query, so neither numeric literals nor
                        // canonical numeric strings can be claimed exactly.
                        return Completion::Deferred;
                    }
                    if shape.index(key).is_some() {
                        return Completion::Deferred;
                    }
                    let Some(value_type) = value_type else {
                        return Completion::Deferred;
                    };
                    let value =
                        self.resolve_type_node(file, member_scope, value_type, type_parameters);
                    match self.shape_child_type_supported(value, &mut HashSet::new()) {
                        Completion::Complete(()) => {}
                        Completion::Deferred => return Completion::Deferred,
                        Completion::Cycle => return Completion::Cycle,
                        Completion::Limit => return Completion::Limit,
                    }
                    shape.index_signatures.push(IndexSignature {
                        key,
                        value,
                        readonly: member.modifiers.readonly,
                    });
                }
                TypeMemberKind::Accessor { .. } => return Completion::Deferred,
            }
        }
        if shape.index_signatures.len() > 1
            || (!shape.index_signatures.is_empty() && !shape.properties.is_empty())
        {
            // TS2411/TS2413 require relation-backed diagnostics and authored
            // member provenance. Until that owner exists, mixed and dual
            // index shapes cannot enter definitive object caches.
            return Completion::Deferred;
        }
        Completion::Complete(shape)
    }

    fn resolve_shape_signature(
        &mut self,
        file: FileId,
        scope: ScopeId,
        parameters: &[Parameter],
        return_type: Option<&TypeNode>,
        type_parameters: &HashMap<String, TypeId>,
    ) -> Completion<ShapeSignature> {
        let mut semantic_parameters = Vec::with_capacity(parameters.len());
        for parameter in parameters {
            if parameter.rest || !parameter.modifiers.is_empty() {
                return Completion::Deferred;
            }
            let ty = if let Some(annotation) = &parameter.annotation {
                let ty = self.resolve_type_node(file, scope, annotation, type_parameters);
                match self.shape_child_type_supported(ty, &mut HashSet::new()) {
                    Completion::Complete(()) => ty,
                    Completion::Deferred => return Completion::Deferred,
                    Completion::Cycle => return Completion::Cycle,
                    Completion::Limit => return Completion::Limit,
                }
            } else if let Some(initializer) = &parameter.initializer {
                match self.signature_initializer_type(file, scope, initializer) {
                    Completion::Complete(ty) => ty,
                    Completion::Deferred => return Completion::Deferred,
                    Completion::Cycle => return Completion::Cycle,
                    Completion::Limit => return Completion::Limit,
                }
            } else if self.options.effective_no_implicit_any() {
                return Completion::Deferred;
            } else {
                self.store.builtins.any
            };
            semantic_parameters.push(ShapeParameter {
                ty,
                optional: parameter.optional || parameter.initializer.is_some(),
                rest: parameter.rest,
            });
        }
        let return_type = if let Some(return_type) = return_type {
            let ty = self.resolve_type_node(file, scope, return_type, type_parameters);
            match self.shape_child_type_supported(ty, &mut HashSet::new()) {
                Completion::Complete(()) => ty,
                Completion::Deferred => return Completion::Deferred,
                Completion::Cycle => return Completion::Cycle,
                Completion::Limit => return Completion::Limit,
            }
        } else if self.options.effective_no_implicit_any() {
            return Completion::Deferred;
        } else {
            self.store.builtins.any
        };
        Completion::Complete(ShapeSignature {
            parameters: semantic_parameters,
            return_type,
        })
    }

    /// Object-shape signatures use name-free callable identity. Existing
    /// function/constructor type nodes still use the authored-name-bearing
    /// `Signature`, so they cannot enter a definitive shape through aliases or
    /// wrappers until that semantic/display provenance split is complete.
    fn shape_child_type_supported(
        &mut self,
        ty: TypeId,
        active: &mut HashSet<TypeId>,
    ) -> Completion<()> {
        if !active.insert(ty) {
            return Completion::Complete(());
        }
        let kind = self.store.kind(ty).clone();
        let result = match kind {
            TypeKind::Array(child) => self.shape_child_type_supported(child, active),
            TypeKind::Tuple(children)
            | TypeKind::Union(children)
            | TypeKind::Intersection(children) => {
                for child in children {
                    match self.shape_child_type_supported(child, active) {
                        Completion::Complete(()) => {}
                        other => return other,
                    }
                }
                Completion::Complete(())
            }
            TypeKind::Object(shape)
            | TypeKind::ClassInstance {
                properties: shape, ..
            } => {
                for child in shape
                    .properties
                    .iter()
                    .map(|property| property.ty)
                    .chain(
                        shape
                            .call_signatures
                            .iter()
                            .chain(&shape.construct_signatures)
                            .flat_map(|signature| {
                                signature
                                    .parameters
                                    .iter()
                                    .map(|parameter| parameter.ty)
                                    .chain(std::iter::once(signature.return_type))
                            }),
                    )
                    .chain(shape.index_signatures.iter().map(|index| index.value))
                {
                    match self.shape_child_type_supported(child, active) {
                        Completion::Complete(()) => {}
                        other => return other,
                    }
                }
                Completion::Complete(())
            }
            TypeKind::ShapeFunction(signature) => {
                for child in signature
                    .parameters
                    .iter()
                    .map(|parameter| parameter.ty)
                    .chain(std::iter::once(signature.return_type))
                {
                    match self.shape_child_type_supported(child, active) {
                        Completion::Complete(()) => {}
                        other => return other,
                    }
                }
                Completion::Complete(())
            }
            TypeKind::Deferred(deferred @ DeferredType::Reference { .. }) => {
                if matches!(self.force_queries.get(&ty), Some(QueryState::Computing)) {
                    // Productive recursive object aliases revisit the
                    // declaration reference while its shape is assembled.
                    Completion::Complete(())
                } else {
                    match self.force_deferred(ty, deferred, 0) {
                        Completion::Complete(resolved) if resolved != ty => {
                            self.shape_child_type_supported(resolved, active)
                        }
                        Completion::Complete(_) | Completion::Deferred => Completion::Deferred,
                        Completion::Cycle => Completion::Cycle,
                        Completion::Limit => Completion::Limit,
                    }
                }
            }
            TypeKind::Deferred(deferred @ DeferredType::IndexedAccess { object, index })
                if !matches!(self.store.kind(object), TypeKind::Deferred(_))
                    && !matches!(self.store.kind(index), TypeKind::Deferred(_)) =>
            {
                match self.force_deferred(ty, deferred, 0) {
                    Completion::Complete(resolved) if resolved != ty => {
                        self.shape_child_type_supported(resolved, active)
                    }
                    Completion::Complete(_) | Completion::Deferred => Completion::Deferred,
                    Completion::Cycle => Completion::Cycle,
                    Completion::Limit => Completion::Limit,
                }
            }
            TypeKind::Function(_) | TypeKind::Deferred(_) => Completion::Deferred,
            TypeKind::Error
            | TypeKind::Invalid(_)
            | TypeKind::Any
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
            | TypeKind::ClassConstructor { .. } => Completion::Complete(()),
        };
        active.remove(&ty);
        result
    }
}

fn unsupported_modifiers(modifiers: &TypeMemberModifiers) -> bool {
    let repeated = modifiers.nodes.iter().enumerate().any(|(index, modifier)| {
        modifiers.nodes[..index]
            .iter()
            .any(|prior| prior.kind == modifier.kind)
    });
    repeated
        || modifiers.public
        || modifiers.protected
        || modifiers.private
        || modifiers.static_member
        || modifiers.abstract_member
        || modifiers.declared
        || modifiers.accessor
        || modifiers.async_member
        || modifiers.const_member
        || modifiers.default_member
        || modifiers.exported
        || modifiers.in_variance
        || modifiers.out_variance
        || modifiers.override_member
}
