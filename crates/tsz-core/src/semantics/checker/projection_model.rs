use std::collections::HashSet;

use crate::bind::{Meaning, ScopeId};
use crate::source::{DeclId, FileId, Span};
use crate::syntax::{ClassMemberKind, Expression, ExpressionKind, TypeNode, TypeNodeKind};

use super::{Checker, DeclarationModel, IndexedAccessOrigin, PropertyQueryOrigin};
use crate::semantics::relation::RelationContext;
use crate::semantics::types::{
    Completion, DeferredType, InvalidType, LiteralProvenance, TypeId, TypeKind, UnionPolicy,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PropertyOrderTree {
    Object(Vec<(String, Self)>),
    Array(Box<Self>),
    Tuple(Vec<Self>),
    Union(Vec<Self>),
    Alias {
        name: String,
        declaration: DeclId,
        preserve_name: bool,
        target: Box<Self>,
    },
    /// Per-use syntax provenance for a reference that already produced its
    /// owning TS2304. The semantic type remains `Error`; diagnostics may still
    /// spell the authored name inside a surrounding object shape.
    AuthoredTypeName(String),
    Unknown,
}

impl PropertyOrderTree {
    pub(super) fn property(&self, name: &str) -> Option<&Self> {
        match self {
            Self::Object(properties) => properties
                .iter()
                .find(|(property, _)| property == name)
                .map(|(_, shape)| shape),
            Self::Alias { target, .. } => target.property(name),
            Self::Array(_)
            | Self::Tuple(_)
            | Self::Union(_)
            | Self::AuthoredTypeName(_)
            | Self::Unknown => None,
        }
    }

    pub(super) fn element_owned(&self) -> Option<Self> {
        match self {
            Self::Array(element) => Some((**element).clone()),
            Self::Alias { target, .. } => target.element_owned(),
            Self::Union(members) => Some(Self::Union(
                members
                    .iter()
                    .map(Self::element_owned)
                    .collect::<Option<Vec<_>>>()?,
            )),
            Self::Object(_) | Self::Tuple(_) | Self::AuthoredTypeName(_) | Self::Unknown => None,
        }
    }

    pub(super) fn property_owned(&self, name: &str) -> Option<Self> {
        match self {
            Self::Object(_) | Self::Alias { .. } => self.property(name).cloned(),
            Self::Union(members) => Some(Self::Union(
                members
                    .iter()
                    .map(|member| member.property_owned(name))
                    .collect::<Option<Vec<_>>>()?,
            )),
            Self::Array(_) | Self::Tuple(_) | Self::AuthoredTypeName(_) | Self::Unknown => None,
        }
    }
}

impl Checker<'_> {
    pub(super) fn relation_order_for_call_argument(
        &self,
        file: FileId,
        scope: ScopeId,
        callee: &Expression,
        index: usize,
        rest: bool,
    ) -> Option<PropertyOrderTree> {
        let callee = peel_expression_parentheses(callee);
        let ExpressionKind::Identifier { name, .. } = &callee.kind else {
            return None;
        };
        let declaration = self.resolve_name(file, scope, name, Meaning::Value)?;
        let DeclarationModel::Function {
            declaration: function,
            scope: function_scope,
        } = self.models.get(&declaration).copied()?
        else {
            return None;
        };
        let parameter = function
            .parameters
            .get(index)
            .or_else(|| function.parameters.iter().find(|parameter| parameter.rest))?;
        let annotation = parameter.annotation.as_ref()?;
        let order =
            self.property_order_for_type_node_root(declaration.file, function_scope, annotation)?;
        if rest {
            order.element_owned().or(Some(order))
        } else {
            Some(order)
        }
    }

    pub(super) fn deferred_indexed_access_type(
        &mut self,
        object: TypeId,
        index: TypeId,
        index_span: Span,
        receiver_order: Option<PropertyOrderTree>,
        receiver_alias: Option<String>,
    ) -> TypeId {
        let query = self
            .store
            .intern(TypeKind::Deferred(DeferredType::IndexedAccess {
                object,
                index,
            }));
        let origin = IndexedAccessOrigin {
            query,
            span: index_span,
            receiver_order,
            receiver_alias,
        };
        if !self.indexed_access_origins.contains(&origin) {
            self.indexed_access_origins.push(origin);
        }
        query
    }

    pub(super) fn deferred_property_type_with_order(
        &mut self,
        object: TypeId,
        name: &str,
        name_span: Span,
        property_order: Option<PropertyOrderTree>,
    ) -> TypeId {
        let ty = self
            .store
            .intern(TypeKind::Deferred(DeferredType::Property {
                object,
                name: name.to_string(),
            }));
        let origin = PropertyQueryOrigin {
            query: ty,
            name: name.to_string(),
            span: name_span,
            property_order,
        };
        if !self.property_query_origins.contains(&origin) {
            self.property_query_origins.push(origin);
        }
        ty
    }

    pub(super) fn evaluate_property(
        &mut self,
        _query: TypeId,
        object: TypeId,
        name: &str,
        depth: usize,
    ) -> Completion<TypeId> {
        let object = match self.force_type(object, depth) {
            Completion::Complete(object) => object,
            Completion::Deferred => return Completion::Deferred,
            Completion::Cycle => return Completion::Cycle,
            Completion::Limit => return Completion::Limit,
        };
        match self.store.kind(object).clone() {
            TypeKind::Object(properties) | TypeKind::ClassInstance { properties, .. } => {
                if let Some(property) = properties.iter().find(|property| property.name == name) {
                    return Completion::Complete(property.ty);
                }
                Completion::Complete(self.store.intern(TypeKind::Invalid(
                    InvalidType::MissingProperty {
                        object,
                        name: name.to_string(),
                    },
                )))
            }
            TypeKind::Any => Completion::Complete(self.store.builtins.any),
            TypeKind::Error | TypeKind::Invalid(_) => Completion::Complete(object),
            _ => Completion::Deferred,
        }
    }

    /// Report each failed property use at its own syntax origin after every
    /// root has been registered. Interned property queries remain span-free,
    /// so cold/warm caches and reversed root order cannot elect a diagnostic
    /// location.
    pub(super) fn flush_property_diagnostics(&mut self) {
        for origin in self.property_query_origins.clone() {
            let completion = self.force_type(origin.query, 0);
            let Completion::Complete(result) = completion else {
                continue;
            };
            let TypeKind::Invalid(InvalidType::MissingProperty { object, name }) =
                self.store.kind(result).clone()
            else {
                continue;
            };
            if name != origin.name {
                continue;
            }
            let object_name =
                self.display_type_with_property_order(object, origin.property_order.as_ref(), 0);
            self.push_diagnostic(
                origin.span.file,
                origin.span,
                format!("Property '{name}' does not exist on type '{object_name}'."),
                2339,
            );
        }
    }

    pub(super) fn property_order_for_declaration(
        &self,
        declaration: DeclId,
    ) -> Option<PropertyOrderTree> {
        self.property_order_for_declaration_inner(declaration, &mut HashSet::new())
    }

    pub(super) fn property_order_for_type_node_root(
        &self,
        file: FileId,
        scope: ScopeId,
        node: &TypeNode,
    ) -> Option<PropertyOrderTree> {
        self.property_order_for_type_node(file, scope, node, &mut HashSet::new())
    }

    pub(super) fn property_order_for_expression(
        &self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
    ) -> Option<PropertyOrderTree> {
        self.property_order_for_expression_inner(file, scope, expression, &mut HashSet::new())
    }

    pub(super) fn display_type_with_property_order(
        &self,
        ty: TypeId,
        property_order: Option<&PropertyOrderTree>,
        depth: usize,
    ) -> String {
        if depth > 24 {
            return "...".to_string();
        }
        match property_order {
            Some(
                PropertyOrderTree::Alias {
                    name,
                    preserve_name: true,
                    ..
                }
                | PropertyOrderTree::AuthoredTypeName(name),
            ) => name.clone(),
            Some(PropertyOrderTree::Alias { target, .. }) => {
                self.display_type_with_property_order(ty, Some(target), depth + 1)
            }
            Some(PropertyOrderTree::Array(element_order)) => {
                let TypeKind::Array(element) = self.store.kind(ty) else {
                    return self.store.display(ty);
                };
                let element_name =
                    self.display_type_with_property_order(*element, Some(element_order), depth + 1);
                if matches!(
                    self.store.kind(*element),
                    TypeKind::Union(_) | TypeKind::Function(_)
                ) {
                    format!("({element_name})[]")
                } else {
                    format!("{element_name}[]")
                }
            }
            Some(PropertyOrderTree::Tuple(element_orders)) => {
                let TypeKind::Tuple(elements) = self.store.kind(ty) else {
                    return self.store.display(ty);
                };
                let values = elements
                    .iter()
                    .enumerate()
                    .map(|(index, element)| {
                        self.display_type_with_property_order(
                            *element,
                            element_orders.get(index),
                            depth + 1,
                        )
                    })
                    .collect::<Vec<_>>();
                format!("[{}]", values.join(", "))
            }
            Some(PropertyOrderTree::Union(members))
                if members.iter().all(|member| {
                    matches!(
                        member,
                        PropertyOrderTree::Alias {
                            preserve_name: true,
                            ..
                        }
                    )
                }) =>
            {
                let mut names = members
                    .iter()
                    .filter_map(|member| match member {
                        PropertyOrderTree::Alias { name, .. } => Some(name.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                names.sort();
                names.dedup();
                names.join(" | ")
            }
            Some(PropertyOrderTree::Object(order)) => {
                let TypeKind::Object(properties) = self.store.kind(ty) else {
                    return self.store.display(ty);
                };
                if properties.is_empty() {
                    return "{}".to_string();
                }
                let mut rendered = Vec::with_capacity(properties.len());
                for (name, child_order) in order {
                    let Some(property) = properties.iter().find(|property| &property.name == name)
                    else {
                        continue;
                    };
                    rendered.push(format!(
                        "{}{}: {}",
                        property.name,
                        if property.optional { "?" } else { "" },
                        self.display_type_with_property_order(
                            property.ty,
                            Some(child_order),
                            depth + 1,
                        )
                    ));
                }
                for property in properties {
                    if order.iter().any(|(name, _)| name == &property.name) {
                        continue;
                    }
                    rendered.push(format!(
                        "{}{}: {}",
                        property.name,
                        if property.optional { "?" } else { "" },
                        self.store.display(property.ty)
                    ));
                }
                format!("{{ {}; }}", rendered.join("; "))
            }
            Some(PropertyOrderTree::Union(_)) | Some(PropertyOrderTree::Unknown) | None => {
                self.store.display(ty)
            }
        }
    }

    fn property_order_for_declaration_inner(
        &self,
        declaration: DeclId,
        active: &mut HashSet<DeclId>,
    ) -> Option<PropertyOrderTree> {
        if !active.insert(declaration) {
            return None;
        }
        let result = match self.models.get(&declaration).copied()? {
            DeclarationModel::Variable {
                declaration: variable,
                scope,
            } => {
                if let Some(annotation) = &variable.annotation {
                    self.property_order_for_type_node(declaration.file, scope, annotation, active)
                } else if let Some(initializer) = &variable.initializer {
                    self.property_order_for_expression_inner(
                        declaration.file,
                        scope,
                        initializer,
                        active,
                    )
                } else {
                    None
                }
            }
            DeclarationModel::Parameter { parameter, scope } => {
                parameter.annotation.as_ref().and_then(|annotation| {
                    self.property_order_for_type_node(declaration.file, scope, annotation, active)
                })
            }
            DeclarationModel::TypeAlias {
                declaration: alias,
                scope,
            } => self.property_order_for_type_node(declaration.file, scope, &alias.ty, active),
            DeclarationModel::Interface {
                declaration: interface,
                ..
            } => Some(PropertyOrderTree::Object(
                interface
                    .properties
                    .iter()
                    .map(|property| {
                        (
                            property.name.clone(),
                            self.property_order_for_type_node(
                                declaration.file,
                                ScopeId(0),
                                &property.ty,
                                active,
                            )
                            .unwrap_or(PropertyOrderTree::Unknown),
                        )
                    })
                    .collect(),
            )),
            DeclarationModel::Class {
                declaration: class,
                scope,
                ..
            } => Some(PropertyOrderTree::Object(
                class
                    .members
                    .iter()
                    .filter_map(|member| {
                        let ClassMemberKind::Property {
                            annotation,
                            initializer,
                            ..
                        } = &member.kind
                        else {
                            return None;
                        };
                        let shape = if let Some(annotation) = annotation {
                            self.property_order_for_type_node(
                                declaration.file,
                                scope,
                                annotation,
                                active,
                            )
                        } else if let Some(initializer) = initializer {
                            self.property_order_for_expression_inner(
                                declaration.file,
                                scope,
                                initializer,
                                active,
                            )
                        } else {
                            None
                        };
                        Some((
                            member.name.clone(),
                            shape.unwrap_or(PropertyOrderTree::Unknown),
                        ))
                    })
                    .collect(),
            )),
            DeclarationModel::Function { .. } => None,
        };
        active.remove(&declaration);
        result
    }

    fn property_order_for_type_node(
        &self,
        file: FileId,
        scope: ScopeId,
        node: &TypeNode,
        active: &mut HashSet<DeclId>,
    ) -> Option<PropertyOrderTree> {
        match &node.kind {
            TypeNodeKind::Object(properties) => Some(PropertyOrderTree::Object(
                properties
                    .iter()
                    .map(|property| {
                        (
                            property.name.clone(),
                            self.property_order_for_type_node(file, scope, &property.ty, active)
                                .unwrap_or(PropertyOrderTree::Unknown),
                        )
                    })
                    .collect(),
            )),
            TypeNodeKind::Array(element) => Some(PropertyOrderTree::Array(Box::new(
                self.property_order_for_type_node(file, scope, element, active)
                    .unwrap_or(PropertyOrderTree::Unknown),
            ))),
            TypeNodeKind::Tuple(elements) => Some(PropertyOrderTree::Tuple(
                elements
                    .iter()
                    .map(|element| {
                        self.property_order_for_type_node(file, scope, element, active)
                            .unwrap_or(PropertyOrderTree::Unknown)
                    })
                    .collect(),
            )),
            TypeNodeKind::Union(members) => Some(PropertyOrderTree::Union(
                members
                    .iter()
                    .map(|member| {
                        self.property_order_for_type_node(file, scope, member, active)
                            .unwrap_or(PropertyOrderTree::Unknown)
                    })
                    .collect(),
            )),
            TypeNodeKind::Parenthesized(inner) | TypeNodeKind::Readonly(inner) => {
                self.property_order_for_type_node(file, scope, inner, active)
            }
            TypeNodeKind::Reference { name, .. } => {
                let Some(declaration) = self.resolve_name(file, scope, name, Meaning::Type) else {
                    return Some(PropertyOrderTree::AuthoredTypeName(name.clone()));
                };
                let target = self
                    .property_order_for_declaration_inner(declaration, active)
                    .unwrap_or(PropertyOrderTree::Unknown);
                Some(PropertyOrderTree::Alias {
                    name: name.clone(),
                    declaration,
                    preserve_name: self.declaration_preserves_alias_name(declaration),
                    target: Box::new(target),
                })
            }
            TypeNodeKind::TypeQuery { name, .. } => {
                let mut segments = name.split('.');
                let root = segments.next()?;
                let declaration = self.resolve_name(file, scope, root, Meaning::Value)?;
                let mut shape = self.property_order_for_declaration_inner(declaration, active)?;
                for property in segments {
                    shape = shape.property(property)?.clone();
                }
                Some(shape)
            }
            _ => None,
        }
    }

    fn declaration_preserves_alias_name(&self, declaration: DeclId) -> bool {
        match self.models.get(&declaration) {
            Some(DeclarationModel::Interface { .. } | DeclarationModel::Class { .. }) => true,
            Some(DeclarationModel::TypeAlias {
                declaration: alias, ..
            }) => diagnostic_alias_shape(&alias.ty),
            Some(
                DeclarationModel::Variable { .. }
                | DeclarationModel::Parameter { .. }
                | DeclarationModel::Function { .. },
            )
            | None => false,
        }
    }

    fn property_order_for_expression_inner(
        &self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
        active: &mut HashSet<DeclId>,
    ) -> Option<PropertyOrderTree> {
        match &expression.kind {
            ExpressionKind::Object(properties) => Some(PropertyOrderTree::Object(
                properties
                    .iter()
                    .map(|property| {
                        (
                            property.name.clone(),
                            self.property_order_for_expression_inner(
                                file,
                                scope,
                                &property.value,
                                active,
                            )
                            .unwrap_or(PropertyOrderTree::Unknown),
                        )
                    })
                    .collect(),
            )),
            ExpressionKind::Array(elements) => {
                let element = elements.first().and_then(|element| {
                    self.property_order_for_expression_inner(file, scope, element, active)
                });
                Some(PropertyOrderTree::Array(Box::new(
                    element.unwrap_or(PropertyOrderTree::Unknown),
                )))
            }
            ExpressionKind::Identifier { name, .. } => {
                let declaration = self.resolve_name(file, scope, name, Meaning::Value)?;
                self.property_order_for_declaration_inner(declaration, active)
                    .map(PropertyOrderTree::without_root_alias)
            }
            ExpressionKind::Member { object, name, .. } => self
                .property_order_for_expression_inner(file, scope, object, active)?
                .property(name)
                .cloned(),
            ExpressionKind::As { ty, .. } => {
                self.property_order_for_type_node(file, scope, ty, active)
            }
            ExpressionKind::Parenthesized(inner) => {
                self.property_order_for_expression_inner(file, scope, inner, active)
            }
            _ => None,
        }
    }

    pub(super) fn evaluate_keyof(&mut self, operand: TypeId, depth: usize) -> Completion<TypeId> {
        let operand = match self.force_type(operand, depth) {
            Completion::Complete(operand) => operand,
            Completion::Deferred => return Completion::Deferred,
            Completion::Cycle => return Completion::Cycle,
            Completion::Limit => return Completion::Limit,
        };
        let properties = match self.store.kind(operand).clone() {
            TypeKind::Object(properties) | TypeKind::ClassInstance { properties, .. } => properties,
            TypeKind::Any | TypeKind::Never | TypeKind::Error => {
                return self.property_key_type();
            }
            TypeKind::Unknown | TypeKind::ObjectKeyword => {
                return Completion::Complete(self.store.builtins.never);
            }
            TypeKind::Invalid(_) => return Completion::Complete(operand),
            TypeKind::Boolean
            | TypeKind::Number
            | TypeKind::String
            | TypeKind::BigInt
            | TypeKind::Symbol
            | TypeKind::LiteralBoolean(_, _)
            | TypeKind::LiteralNumber(_, _)
            | TypeKind::LiteralString(_, _)
            | TypeKind::TypeParameter { .. }
            | TypeKind::Array(_)
            | TypeKind::Tuple(_)
            | TypeKind::StringIndex(_)
            | TypeKind::Union(_)
            | TypeKind::Intersection(_)
            | TypeKind::ClassConstructor { .. }
            | TypeKind::Function(_)
            | TypeKind::Deferred(_)
            | TypeKind::Void
            | TypeKind::Undefined
            | TypeKind::Null => return Completion::Deferred,
        };
        let keys = properties
            .into_iter()
            .map(|property| {
                self.store.intern(TypeKind::LiteralString(
                    property.name,
                    LiteralProvenance::Regular,
                ))
            })
            .collect::<Vec<_>>();
        Completion::Complete(self.store.union(keys, UnionPolicy::Canonical))
    }

    fn property_key_type(&mut self) -> Completion<TypeId> {
        Completion::Complete(self.store.union(
            [
                self.store.builtins.string,
                self.store.builtins.number,
                self.store.builtins.symbol,
            ],
            UnionPolicy::Canonical,
        ))
    }

    pub(super) fn evaluate_indexed_access(
        &mut self,
        object: TypeId,
        index: TypeId,
        depth: usize,
    ) -> Completion<TypeId> {
        let object = match self.force_type(object, depth) {
            Completion::Complete(object) => object,
            Completion::Deferred => return Completion::Deferred,
            Completion::Cycle => return Completion::Cycle,
            Completion::Limit => return Completion::Limit,
        };
        let index = match self.force_type(index, depth) {
            Completion::Complete(index) => index,
            Completion::Deferred => return Completion::Deferred,
            Completion::Cycle => return Completion::Cycle,
            Completion::Limit => return Completion::Limit,
        };
        let properties = match self.store.kind(object).clone() {
            TypeKind::Object(properties) | TypeKind::ClassInstance { properties, .. } => properties,
            TypeKind::Any => return Completion::Complete(self.store.builtins.any),
            TypeKind::Error => return Completion::Complete(self.store.builtins.error),
            TypeKind::Invalid(_) => return Completion::Complete(object),
            _ => return Completion::Deferred,
        };
        let keys = match self.store.kind(index).clone() {
            TypeKind::LiteralString(key, _) => vec![key],
            TypeKind::Union(members) => {
                let mut keys = Vec::with_capacity(members.len());
                for member in members {
                    let TypeKind::LiteralString(key, _) = self.store.kind(member) else {
                        return Completion::Deferred;
                    };
                    keys.push(key.clone());
                }
                keys
            }
            _ => return Completion::Deferred,
        };
        let mut values = Vec::with_capacity(keys.len());
        let mut missing = Vec::new();
        for key in keys {
            let value = properties
                .iter()
                .find(|property| property.name == key)
                .map(|property| property.ty);
            if let Some(value) = value {
                values.push(value);
            } else {
                missing.push(key);
            }
        }
        if !missing.is_empty() {
            return Completion::Complete(self.store.intern(TypeKind::Invalid(
                InvalidType::MissingProperties {
                    object,
                    names: missing,
                },
            )));
        }
        Completion::Complete(self.store.union(values, UnionPolicy::Canonical))
    }

    pub(super) fn flush_indexed_access_diagnostics(&mut self) {
        for origin in self.indexed_access_origins.clone() {
            let Completion::Complete(result) = self.force_type(origin.query, 0) else {
                continue;
            };
            let (object, names) = match self.store.kind(result).clone() {
                TypeKind::Invalid(InvalidType::MissingProperty { object, name }) => {
                    (object, vec![name])
                }
                TypeKind::Invalid(InvalidType::MissingProperties { object, names }) => {
                    (object, names)
                }
                _ => continue,
            };
            let object_name = self.indexed_access_receiver_name(
                object,
                origin.receiver_order.as_ref(),
                origin.receiver_alias.as_deref(),
            );
            for name in names {
                self.push_diagnostic_with_identity(
                    origin.span.file,
                    origin.span,
                    format!("Property '{name}' does not exist on type '{object_name}'."),
                    2339,
                    super::DiagnosticIdentity::MissingProperty(name),
                );
            }
        }
    }

    fn indexed_access_receiver_name(
        &self,
        object: TypeId,
        receiver_order: Option<&PropertyOrderTree>,
        receiver_alias: Option<&str>,
    ) -> String {
        if let Some(alias) = receiver_alias {
            return alias.to_string();
        }
        match self.store.kind(object) {
            TypeKind::Boolean | TypeKind::LiteralBoolean(_, _) => "Boolean".to_string(),
            TypeKind::Number | TypeKind::LiteralNumber(_, _) => "Number".to_string(),
            TypeKind::String | TypeKind::LiteralString(_, _) => "String".to_string(),
            TypeKind::BigInt => "BigInt".to_string(),
            TypeKind::Symbol => "Symbol".to_string(),
            _ => self.display_type_with_property_order(object, receiver_order, 0),
        }
    }
}

impl PropertyOrderTree {
    fn without_root_alias(self) -> Self {
        match self {
            Self::Alias { target, .. } => *target,
            other => other,
        }
    }
}

pub(super) fn authored_type_reference_name(node: &TypeNode) -> Option<String> {
    match &node.kind {
        TypeNodeKind::Reference { name, .. } => Some(name.clone()),
        TypeNodeKind::Parenthesized(inner) | TypeNodeKind::Readonly(inner) => {
            authored_type_reference_name(inner)
        }
        _ => None,
    }
}

fn diagnostic_alias_shape(node: &TypeNode) -> bool {
    match &node.kind {
        TypeNodeKind::Array(_) | TypeNodeKind::Object(_) | TypeNodeKind::Function { .. } => true,
        TypeNodeKind::Tuple(elements) => !elements.is_empty(),
        TypeNodeKind::Union(members) | TypeNodeKind::Intersection(members) => {
            !members.is_empty() && members.iter().all(diagnostic_alias_shape)
        }
        TypeNodeKind::Parenthesized(inner) | TypeNodeKind::Readonly(inner) => {
            diagnostic_alias_shape(inner)
        }
        _ => false,
    }
}

fn peel_expression_parentheses(mut expression: &Expression) -> &Expression {
    while let ExpressionKind::Parenthesized(inner) = &expression.kind {
        expression = inner;
    }
    expression
}
