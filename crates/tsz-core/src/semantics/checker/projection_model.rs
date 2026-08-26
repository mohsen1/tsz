use std::collections::HashSet;

use crate::bind::{Meaning, ScopeId};
use crate::source::{DeclId, FileId, Span};
use crate::syntax::{
    ClassMemberKind, Expression, ExpressionKind, Parameter, TypeMember, TypeMemberKind, TypeNode,
    TypeNodeKind,
};

use super::{
    Checker, DeclarationModel, IndexedAccessOrigin, PropertyQueryOrigin,
    recursion::{ReferenceDemand, ReferenceExpansionStack},
};
use crate::semantics::relation::RelationContext;
use crate::semantics::types::{
    Completion, DeferredType, IndexKeyKind, InvalidType, LiteralProvenance, ParameterType,
    Property, Signature, TypeId, TypeKind, TypeStore, UnionPolicy,
};
use crate::standard_library::{LibraryCallMember, LibraryMemberLookup, LibraryReceiver};

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

    fn substitute_authored_type_names(self, substitutions: &[(String, Self)]) -> Self {
        match self {
            Self::AuthoredTypeName(name) => substitutions
                .iter()
                .find(|(parameter, _)| parameter == &name)
                .map(|(_, replacement)| replacement.clone())
                .unwrap_or(Self::AuthoredTypeName(name)),
            Self::Alias {
                name,
                declaration,
                preserve_name,
                target,
            } => {
                if let Some((_, replacement)) = substitutions
                    .iter()
                    .find(|(parameter, _)| parameter == &name)
                {
                    replacement.clone()
                } else {
                    Self::Alias {
                        name,
                        declaration,
                        preserve_name,
                        target: Box::new(target.substitute_authored_type_names(substitutions)),
                    }
                }
            }
            Self::Object(properties) => Self::Object(
                properties
                    .into_iter()
                    .map(|(name, order)| {
                        (name, order.substitute_authored_type_names(substitutions))
                    })
                    .collect(),
            ),
            Self::Array(element) => Self::Array(Box::new(
                element.substitute_authored_type_names(substitutions),
            )),
            Self::Tuple(elements) => Self::Tuple(
                elements
                    .into_iter()
                    .map(|element| element.substitute_authored_type_names(substitutions))
                    .collect(),
            ),
            Self::Union(members) => Self::Union(
                members
                    .into_iter()
                    .map(|member| member.substitute_authored_type_names(substitutions))
                    .collect(),
            ),
            Self::Unknown => Self::Unknown,
        }
    }
}

impl Checker<'_> {
    /// Operation-local, name-bearing callback projection for an exact class method.
    pub(super) fn contextual_projection_type(
        &mut self,
        file: FileId,
        scope: ScopeId,
        node: &TypeNode,
    ) -> Option<TypeId> {
        match &node.kind {
            TypeNodeKind::Keyword(_)
            | TypeNodeKind::Literal(_)
            | TypeNodeKind::Reference { .. }
                if !matches!(&node.kind, TypeNodeKind::Reference { arguments, .. } if !arguments.is_empty()) =>
            {
                let ty =
                    self.resolve_type_node(file, scope, node, &std::collections::HashMap::new());
                (!matches!(self.store.kind(ty), TypeKind::Error | TypeKind::Invalid(_)))
                    .then_some(ty)
            }
            TypeNodeKind::Object(members) => {
                self.contextual_object_projection(file, scope, members)
            }
            _ => None,
        }
    }

    fn contextual_object_projection(
        &mut self,
        file: FileId,
        scope: ScopeId,
        members: &[TypeMember],
    ) -> Option<TypeId> {
        if members.is_empty() {
            return Some(self.store.object(Vec::new()));
        }
        let [member] = members else { return None };
        if member.recovered || !member.modifiers.nodes.is_empty() {
            return None;
        }
        let TypeMemberKind::Method {
            name,
            optional: false,
            type_parameters,
            parameters,
            return_type: Some(return_type),
        } = &member.kind
        else {
            return None;
        };
        if !type_parameters.is_empty() {
            return None;
        }
        let member_scope = self.node_scope(file, member.id, scope);
        let ty = self.contextual_function_projection(
            file,
            member_scope,
            parameters,
            Some(return_type),
            false,
        )?;
        Some(self.store.object(vec![Property {
            name: name.semantic_name()?.to_string(),
            ty,
            optional: false,
            readonly: false,
        }]))
    }

    pub(super) fn contextual_function_projection(
        &mut self,
        file: FileId,
        scope: ScopeId,
        parameters: &[Parameter],
        return_type: Option<&TypeNode>,
        empty_body: bool,
    ) -> Option<TypeId> {
        let mut resolved = Vec::with_capacity(parameters.len());
        for parameter in parameters {
            if parameter.initializer.is_some()
                || !parameter.modifiers.is_empty()
                || !parameter.overload_context_is_recovery_free()
            {
                return None;
            }
            resolved.push(ParameterType {
                name: Some(parameter.name.clone()),
                ty: self.contextual_projection_type(file, scope, parameter.annotation.as_ref()?)?,
                optional: parameter.optional,
                rest: parameter.rest,
            });
        }
        let return_type = match return_type {
            Some(return_type) => self.contextual_projection_type(file, scope, return_type)?,
            None if empty_body => self.store.builtins.void,
            None => return None,
        };
        Some(self.store.function(None, false, resolved, return_type))
    }

    /// Whether rendering this semantic type would require authored object-
    /// signature provenance that is intentionally absent from `TypeKind`.
    /// Incomplete wrappers stay typed nonclaims instead of being printed with
    /// synthetic parameter or index names.
    pub(super) fn requires_authored_shape_display(&mut self, ty: TypeId) -> Completion<bool> {
        let mut references = ReferenceExpansionStack::new(ReferenceDemand::AuthoredDisplay);
        self.requires_authored_shape_display_inner(ty, &mut HashSet::new(), &mut references)
    }

    pub(super) fn authored_shape_display_is_unavailable(&mut self, ty: TypeId) -> bool {
        match self.requires_authored_shape_display(ty) {
            Completion::Complete(false) => false,
            Completion::Complete(true) | Completion::Deferred => {
                let _ = self.require_completion(Completion::<()>::Deferred);
                true
            }
            Completion::Cycle => {
                let _ = self.require_completion(Completion::<()>::Cycle);
                true
            }
            Completion::Limit => {
                let _ = self.require_completion(Completion::<()>::Limit);
                true
            }
        }
    }

    fn requires_authored_shape_display_inner(
        &mut self,
        ty: TypeId,
        active: &mut HashSet<TypeId>,
        references: &mut ReferenceExpansionStack,
    ) -> Completion<bool> {
        if !active.insert(ty) {
            return Completion::Complete(false);
        }
        let kind = self.store.kind(ty).clone();
        let result = match kind {
            TypeKind::ShapeFunction(_) => Completion::Complete(true),
            TypeKind::Object(shape)
            | TypeKind::ClassInstance {
                properties: shape, ..
            } => {
                if !shape.call_signatures.is_empty()
                    || !shape.construct_signatures.is_empty()
                    || !shape.index_signatures.is_empty()
                    || shape.properties.iter().any(|property| {
                        matches!(self.store.kind(property.ty), TypeKind::ShapeFunction(_))
                    })
                {
                    Completion::Complete(true)
                } else {
                    self.children_require_authored(
                        shape.properties.into_iter().map(|property| property.ty),
                        active,
                        references,
                    )
                }
            }
            kind @ (TypeKind::Array(_)
            | TypeKind::Tuple(_)
            | TypeKind::Union(_)
            | TypeKind::Intersection(_)
            | TypeKind::Function(_)
            | TypeKind::LibraryReference { .. }) => {
                let mut children = Vec::new();
                TypeStore::push_type_children(&kind, &mut children);
                self.children_require_authored(children, active, references)
            }
            TypeKind::Deferred(deferred @ DeferredType::Reference { .. }) => {
                let DeferredType::Reference {
                    declaration,
                    arguments,
                } = &deferred
                else {
                    unreachable!()
                };
                if completed!(self.children_require_authored(
                    arguments.iter().copied(),
                    active,
                    references,
                )) {
                    Completion::Complete(true)
                } else if let Some(expansion) =
                    references.generative_expansion(ty, *declaration, arguments, &|ty| {
                        self.store.kind(ty).clone()
                    })
                {
                    // The growing edge needs no authored signature text. The
                    // enclosing display query still examines every sibling.
                    if self.generative_reference_supported(*declaration, arguments)
                        && references.expansion_segment_supports(
                            &expansion,
                            |frame_declaration, frame_arguments| {
                                self.reference_expansion_frame_supported(
                                    frame_declaration,
                                    frame_arguments,
                                )
                            },
                        )
                    {
                        Completion::Complete(false)
                    } else {
                        Completion::Deferred
                    }
                } else {
                    let checkpoint = references.checkpoint();
                    references.push(ty, *declaration, arguments);
                    let result = match self.force_type(ty, 0) {
                        Completion::Complete(resolved) if resolved != ty => {
                            self.requires_authored_shape_display_inner(resolved, active, references)
                        }
                        Completion::Complete(_) | Completion::Deferred => Completion::Deferred,
                        Completion::Cycle => Completion::Cycle,
                        Completion::Limit => Completion::Limit,
                    };
                    references.restore(checkpoint);
                    result
                }
            }
            TypeKind::Deferred(_) => match self.force_type(ty, 0) {
                Completion::Complete(resolved) if resolved != ty => {
                    self.requires_authored_shape_display_inner(resolved, active, references)
                }
                Completion::Complete(_) | Completion::Deferred => Completion::Deferred,
                Completion::Cycle => Completion::Cycle,
                Completion::Limit => Completion::Limit,
            },
            TypeKind::Invalid(InvalidType::MissingProperty { object, .. })
            | TypeKind::Invalid(InvalidType::MissingProperties { object, .. }) => {
                self.requires_authored_shape_display_inner(object, active, references)
            }
            TypeKind::Error | non_recursive_type_kind!() => Completion::Complete(false),
        };
        active.remove(&ty);
        result
    }

    fn children_require_authored(
        &mut self,
        children: impl IntoIterator<Item = TypeId>,
        active: &mut HashSet<TypeId>,
        references: &mut ReferenceExpansionStack,
    ) -> Completion<bool> {
        for child in children {
            if completed!(self.requires_authored_shape_display_inner(child, active, references)) {
                return Completion::Complete(true);
            }
        }
        Completion::Complete(false)
    }

    pub(super) fn infer_member_expression(
        &mut self,
        file: FileId,
        scope: ScopeId,
        object: &Expression,
        name: &str,
        name_span: Span,
        mut library_member: Option<&mut Completion<Option<LibraryCallMember>>>,
    ) -> TypeId {
        let authored_readonly = self.authored_readonly_array_receiver(file, scope, object);
        let object_type = self.infer_expression(file, scope, object, None);
        if let Some(library_member) = library_member.as_deref_mut() {
            match self.standard_library_call_projection(object_type, name, authored_readonly) {
                Completion::Complete(Some((ty, id))) => {
                    *library_member = Completion::Complete(Some(id));
                    return ty;
                }
                Completion::Complete(None) => {}
                Completion::Deferred => *library_member = Completion::Deferred,
                Completion::Cycle => *library_member = Completion::Cycle,
                Completion::Limit => *library_member = Completion::Limit,
            }
        }
        let property_order = self.property_order_for_expression(file, scope, object);
        let completion = self.property_type(object_type, name, library_member.is_some());
        match self.require_completion(completion) {
            Completion::Complete(Some(ty)) => ty,
            Completion::Complete(None) => {
                let complete_object = self.complete_type(object_type).unwrap_or(object_type);
                let object_name = self.display_type_with_property_order(
                    complete_object,
                    property_order.as_ref(),
                    0,
                );
                self.push_diagnostic(
                    file,
                    name_span,
                    format!("Property '{name}' does not exist on type '{object_name}'."),
                    2339,
                );
                self.store
                    .intern(TypeKind::Invalid(InvalidType::MissingProperty {
                        object: object_type,
                        name: name.to_string(),
                    }))
            }
            Completion::Deferred | Completion::Cycle | Completion::Limit => {
                self.deferred_property_type_with_order(object_type, name, name_span, property_order)
            }
        }
    }

    pub(super) fn standard_library_call_projection(
        &mut self,
        object: TypeId,
        name: &str,
        authored_readonly: bool,
    ) -> Completion<Option<(TypeId, LibraryCallMember)>> {
        let Some(object) = self.complete_type(object) else {
            return Completion::Deferred;
        };
        let (receiver, element, arguments) = match self.store.kind(object).clone() {
            TypeKind::Array(element) => {
                if element == self.store.builtins.never {
                    return Completion::Deferred;
                }
                (LibraryReceiver::Array, Some(element), None)
            }
            TypeKind::LibraryReference {
                declaration,
                arguments,
                ..
            } => (
                LibraryReceiver::Declaration(declaration),
                None,
                Some(arguments),
            ),
            _ => return Completion::Complete(None),
        };
        let LibraryMemberLookup::Found(member) = self.standard_library_call_member(receiver, name)
        else {
            return Completion::Deferred;
        };
        if authored_readonly
            && matches!(member, LibraryCallMember::Push | LibraryCallMember::Splice)
        {
            return Completion::Deferred;
        }
        let number = self.store.builtins.number;
        let signature = match member {
            LibraryCallMember::IndexOf | LibraryCallMember::LastIndexOf => shape_function(
                vec![
                    shape_param(element.expect("Array member element"), false, false),
                    shape_param(number, true, false),
                ],
                number,
            ),
            LibraryCallMember::Push => {
                shape_function(vec![shape_param(object, false, true)], number)
            }
            LibraryCallMember::Slice => shape_function(
                vec![
                    shape_param(number, true, false),
                    shape_param(number, true, false),
                ],
                object,
            ),
            LibraryCallMember::Splice => shape_function(
                vec![
                    shape_param(number, false, false),
                    shape_param(number, true, false),
                    shape_param(object, false, true),
                ],
                object,
            ),
            LibraryCallMember::Map => {
                let element = element.expect("Array member element");
                let callback = self.store.intern(shape_function(
                    vec![
                        shape_param(element, false, false),
                        shape_param(number, false, false),
                        shape_param(object, false, false),
                    ],
                    self.store.builtins.void,
                ));
                let result = self
                    .store
                    .intern(TypeKind::Array(self.store.builtins.unknown));
                shape_function(
                    vec![
                        shape_param(callback, false, false),
                        shape_param(self.store.builtins.any, true, false),
                    ],
                    result,
                )
            }
            LibraryCallMember::MapGet | LibraryCallMember::MapSet => {
                let Some([key, value]) = arguments.as_deref() else {
                    return Completion::Deferred;
                };
                let (parameters, return_type) = if member == LibraryCallMember::MapGet {
                    (
                        vec![shape_param(*key, false, false)],
                        self.store.union(
                            [*value, self.store.builtins.undefined],
                            UnionPolicy::Canonical,
                        ),
                    )
                } else {
                    (
                        vec![
                            shape_param(*key, false, false),
                            shape_param(*value, false, false),
                        ],
                        object,
                    )
                };
                shape_function(parameters, return_type)
            }
            LibraryCallMember::ToString => shape_function(Vec::new(), self.store.builtins.string),
        };
        let ty = self.store.intern(signature);
        Completion::Complete(Some((ty, member)))
    }

    pub(super) fn standard_library_call_member(
        &self,
        receiver: LibraryReceiver,
        name: &str,
    ) -> LibraryMemberLookup {
        self.program.standard_library.call_member(
            receiver,
            name,
            |owner| {
                self.program
                    .standard_library_type_has_authored_declarations(owner)
            },
            |owner, member| {
                self.program
                    .standard_library_type_has_authored_member(owner, member)
            },
        )
    }

    pub(super) fn authored_readonly_array_receiver(
        &self,
        file: FileId,
        scope: ScopeId,
        expression: &Expression,
    ) -> bool {
        let annotation = match &expression.peel_parentheses().kind {
            ExpressionKind::As { ty, .. } => Some(ty),
            ExpressionKind::Identifier { name, .. } => self
                .resolve_name(file, scope, name, Meaning::Value)
                .and_then(|declaration| match self.models.get(&declaration).copied() {
                    Some(DeclarationModel::Variable { declaration, .. }) => {
                        declaration.annotation.as_ref()
                    }
                    Some(DeclarationModel::Parameter { parameter, .. }) => {
                        parameter.annotation.as_ref()
                    }
                    _ => None,
                }),
            _ => None,
        };
        annotation.is_some_and(authored_readonly_array)
    }

    fn property_type(
        &mut self,
        object: TypeId,
        name: &str,
        allow_shape_callable: bool,
    ) -> Completion<Option<TypeId>> {
        let object = completed!(self.force_type(object, 0));
        match self.store.kind(object) {
            TypeKind::Object(shape)
            | TypeKind::ClassInstance {
                properties: shape, ..
            } => {
                let property = shape
                    .properties
                    .iter()
                    .find(|property| property.name == name);
                if !allow_shape_callable
                    && property.is_some_and(|property| {
                        matches!(self.store.kind(property.ty), TypeKind::ShapeFunction(_))
                    })
                {
                    return Completion::Deferred;
                }
                if let Some(property) = property {
                    return Completion::Complete(Some(property.ty));
                }
                if let Some(index) = shape.index(IndexKeyKind::String) {
                    if !allow_shape_callable
                        && matches!(self.store.kind(index.value), TypeKind::ShapeFunction(_))
                    {
                        return Completion::Deferred;
                    }
                    return Completion::Complete(Some(index.value));
                }
                if completed!(self.requires_authored_shape_display(object)) {
                    return Completion::Deferred;
                }
                Completion::Complete(None)
            }
            TypeKind::Any => Completion::Complete(Some(self.store.builtins.any)),
            TypeKind::Error | TypeKind::Invalid(_) => Completion::Complete(Some(object)),
            _ => Completion::Deferred,
        }
    }

    pub(super) fn relation_order_for_call_argument(
        &self,
        file: FileId,
        scope: ScopeId,
        callee: &Expression,
        index: usize,
        rest: bool,
    ) -> Option<PropertyOrderTree> {
        let callee = callee.peel_parentheses();
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
        // A generic declaration stays symbolic; a concrete instantiation records its own origin.
        if !matches!(self.store.kind(object), TypeKind::TypeParameter { .. })
            && !self.indexed_access_origins.contains(&origin)
        {
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
        let object = completed!(self.force_type(object, depth));
        match self.store.kind(object).clone() {
            TypeKind::Object(shape)
            | TypeKind::ClassInstance {
                properties: shape, ..
            } => {
                if let Some(property) = shape
                    .properties
                    .iter()
                    .find(|property| property.name == name)
                {
                    if matches!(self.store.kind(property.ty), TypeKind::ShapeFunction(_)) {
                        // Object-member signatures intentionally erase authored
                        // parameter names from semantic identity. Until the
                        // property-query origin carries signature display
                        // provenance, exposing this as a definitive callable
                        // would fabricate `arg0` in diagnostics/quickinfo.
                        return Completion::Deferred;
                    }
                    return Completion::Complete(property.ty);
                }
                if let Some(index) = shape.index(IndexKeyKind::String) {
                    if matches!(self.store.kind(index.value), TypeKind::ShapeFunction(_)) {
                        return Completion::Deferred;
                    }
                    return Completion::Complete(index.value);
                }
                if completed!(self.requires_authored_shape_display(object)) {
                    return Completion::Deferred;
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
            let completion = self.require_file_completion(origin.span.file, completion);
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
                let TypeKind::Object(shape) = self.store.kind(ty) else {
                    return self.store.display(ty);
                };
                if !shape.call_signatures.is_empty()
                    || !shape.construct_signatures.is_empty()
                    || !shape.index_signatures.is_empty()
                {
                    return self.store.display(ty);
                }
                let properties = &shape.properties;
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
        let Some(model) = self.models.get(&declaration).copied() else {
            active.remove(&declaration);
            return None;
        };
        let result = match model {
            DeclarationModel::Variable {
                declaration: variable,
                scope,
                ..
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
                scope,
            } => {
                let mut properties = self.interface_member_property_order(
                    declaration.file,
                    scope,
                    &interface.members,
                    active,
                );
                if interface.extends.is_empty() {
                    Some(PropertyOrderTree::Object(properties))
                } else {
                    match self.plain_property_interface_heritage_bases(declaration) {
                        Completion::Complete(mut bases) => {
                            // Pinned TS7 stable type ordering keeps the
                            // declaration order of bases, independently of the
                            // spelling order in the extends clause. Own members
                            // precede every inherited member.
                            bases.sort_unstable();
                            let mut seen = properties
                                .iter()
                                .map(|(name, _)| name.clone())
                                .collect::<HashSet<_>>();
                            let mut base_properties: Vec<Vec<(String, PropertyOrderTree)>> =
                                Vec::with_capacity(bases.len());
                            let mut bases_complete = true;
                            for base in bases {
                                let Some(DeclarationModel::Interface {
                                    declaration: base_interface,
                                    scope: base_scope,
                                }) = self.models.get(&base).copied()
                                else {
                                    bases_complete = false;
                                    break;
                                };
                                let renamed_parameters = base_interface
                                    .type_parameters
                                    .iter()
                                    .zip(&interface.type_parameters)
                                    .map(|(base, derived)| {
                                        (
                                            base.name.clone(),
                                            PropertyOrderTree::AuthoredTypeName(
                                                derived.name.clone(),
                                            ),
                                        )
                                    })
                                    .collect::<Vec<_>>();
                                base_properties.push(
                                    self.interface_member_property_order(
                                        base.file,
                                        base_scope,
                                        &base_interface.members,
                                        active,
                                    )
                                    .into_iter()
                                    .map(|(name, order)| {
                                        (
                                            name,
                                            order.substitute_authored_type_names(
                                                &renamed_parameters,
                                            ),
                                        )
                                    })
                                    .collect(),
                                );
                            }
                            if !bases_complete {
                                None
                            } else {
                                for (name, order) in base_properties.into_iter().flatten() {
                                    if seen.insert(name.clone()) {
                                        properties.push((name, order));
                                    }
                                }
                                Some(PropertyOrderTree::Object(properties))
                            }
                        }
                        Completion::Deferred | Completion::Cycle | Completion::Limit => None,
                    }
                }
            }
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
            DeclarationModel::Function { .. } | DeclarationModel::JavaScriptProperty(..) => None,
        };
        active.remove(&declaration);
        result
    }

    fn interface_member_property_order(
        &self,
        file: FileId,
        scope: ScopeId,
        members: &[crate::syntax::TypeMember],
        active: &mut HashSet<DeclId>,
    ) -> Vec<(String, PropertyOrderTree)> {
        members
            .iter()
            .filter_map(|member| {
                let TypeMemberKind::Property { name, ty, .. } = &member.kind else {
                    return None;
                };
                let name = name.semantic_name()?.to_string();
                let member_scope = self.node_scope(file, member.id, scope);
                let order = ty.as_ref().and_then(|ty| {
                    self.property_order_for_type_node(file, member_scope, ty, active)
                });
                Some((name, order.unwrap_or(PropertyOrderTree::Unknown)))
            })
            .collect()
    }

    fn property_order_for_type_node(
        &self,
        file: FileId,
        scope: ScopeId,
        node: &TypeNode,
        active: &mut HashSet<DeclId>,
    ) -> Option<PropertyOrderTree> {
        match &node.kind {
            TypeNodeKind::Object(members) => Some(PropertyOrderTree::Object(
                members
                    .iter()
                    .filter_map(|member| {
                        let TypeMemberKind::Property { name, ty, .. } = &member.kind else {
                            return None;
                        };
                        let name = name.semantic_name()?.to_string();
                        let order = ty.as_ref().and_then(|ty| {
                            self.property_order_for_type_node(file, scope, ty, active)
                        });
                        Some((name, order.unwrap_or(PropertyOrderTree::Unknown)))
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
            TypeNodeKind::Reference {
                name, arguments, ..
            } => {
                let Some(declaration) = self.resolve_name(file, scope, name, Meaning::Type) else {
                    return Some(PropertyOrderTree::AuthoredTypeName(name.clone()));
                };
                let mut target = self
                    .property_order_for_declaration_inner(declaration, active)
                    .unwrap_or(PropertyOrderTree::Unknown);
                if let Some(DeclarationModel::Interface {
                    declaration: interface,
                    ..
                }) = self.models.get(&declaration).copied()
                    && interface.type_parameters.len() == arguments.len()
                {
                    let substitutions = interface
                        .type_parameters
                        .iter()
                        .zip(arguments)
                        .map(|(parameter, argument)| {
                            (
                                parameter.name.clone(),
                                self.property_order_for_type_node(file, scope, argument, active)
                                    .unwrap_or(PropertyOrderTree::Unknown),
                            )
                        })
                        .collect::<Vec<_>>();
                    target = target.substitute_authored_type_names(&substitutions);
                }
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
                let resolved = self.program.resolve_type_query_root(file, scope, root)?;
                let declaration = resolved.semantic_declaration();
                let imported = resolved.navigation_declaration() != declaration;
                let mut shape = self.property_order_for_declaration_inner(declaration, active)?;
                for property in segments {
                    shape = shape.property(property)?.clone();
                }
                // A direct object with several absent properties needs TS2739,
                // whose diagnostic owner is not implemented yet. Do not let
                // resolving an import alias widen that case into a false
                // TS2322; nested arrays/tuples retain their authored order for
                // the already-owned TS2322 relation path.
                (!imported || !matches!(shape, PropertyOrderTree::Object(_))).then_some(shape)
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
                | DeclarationModel::Function { .. }
                | DeclarationModel::JavaScriptProperty(..),
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
            }
            ExpressionKind::Member { object, name, .. } => self
                .property_order_for_expression_inner(file, scope, object, active)?
                .property(name)
                .cloned(),
            ExpressionKind::ElementAccess { object, .. } => self
                .property_order_for_expression_inner(file, scope, object, active)?
                .element_owned(),
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
        let operand = completed!(self.force_type(operand, depth));
        let properties = match self.store.kind(operand).clone() {
            TypeKind::Object(shape)
            | TypeKind::ClassInstance {
                properties: shape, ..
            } => shape,
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
            | TypeKind::Union(_)
            | TypeKind::Intersection(_)
            | TypeKind::ClassConstructor { .. }
            | TypeKind::LibraryReference { .. }
            | TypeKind::Function(_)
            | TypeKind::ShapeFunction(_)
            | TypeKind::Deferred(_)
            | TypeKind::Void
            | TypeKind::Undefined
            | TypeKind::Null => return Completion::Deferred,
        };
        let mut keys = properties
            .properties
            .into_iter()
            .map(|property| {
                self.store.intern(TypeKind::LiteralString(
                    property.name,
                    LiteralProvenance::Regular,
                ))
            })
            .collect::<Vec<_>>();
        for index in properties.index_signatures {
            match index.key {
                IndexKeyKind::String => {
                    keys.push(self.store.builtins.string);
                    keys.push(self.store.builtins.number);
                }
                IndexKeyKind::Number => keys.push(self.store.builtins.number),
            }
        }
        Completion::Complete(self.store.union(keys, UnionPolicy::Canonical))
    }

    pub(super) fn property_key_type(&mut self) -> Completion<TypeId> {
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
        let object = completed!(self.force_type(object, depth));
        let index = completed!(self.force_type(index, depth));
        let properties = match self.store.kind(object).clone() {
            TypeKind::Object(shape)
            | TypeKind::ClassInstance {
                properties: shape, ..
            } => shape,
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
                .properties
                .iter()
                .find(|property| property.name == key)
                .map(|property| property.ty)
                .or_else(|| {
                    properties
                        .index(IndexKeyKind::String)
                        .map(|index| index.value)
                });
            if let Some(value) = value {
                if matches!(self.store.kind(value), TypeKind::ShapeFunction(_)) {
                    return Completion::Deferred;
                }
                values.push(value);
            } else {
                missing.push(key);
            }
        }
        if !missing.is_empty() {
            if completed!(self.requires_authored_shape_display(object)) {
                return Completion::Deferred;
            }
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
            let completion = self.force_type(origin.query, 0);
            let Completion::Complete(result) =
                self.require_file_completion(origin.span.file, completion)
            else {
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

const fn shape_param(ty: TypeId, optional: bool, rest: bool) -> ParameterType {
    ParameterType {
        name: None,
        ty,
        optional,
        rest,
    }
}

const fn shape_function(parameters: Vec<ParameterType>, return_type: TypeId) -> TypeKind {
    TypeKind::ShapeFunction(Signature {
        generic_declaration: None,
        untyped_javascript: false,
        parameters,
        return_type,
    })
}

fn authored_readonly_array(node: &TypeNode) -> bool {
    match &node.kind {
        TypeNodeKind::Readonly(inner) => matches!(inner.kind, TypeNodeKind::Array(_)),
        TypeNodeKind::Parenthesized(inner) => authored_readonly_array(inner),
        _ => false,
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
