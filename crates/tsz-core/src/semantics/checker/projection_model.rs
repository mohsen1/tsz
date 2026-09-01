use super::{Checker, DeclarationModel, IndexedAccessOrigin, PropertyQueryOrigin};
use crate::bind::{Meaning, ScopeId};
use crate::semantics::relation::RelationContext;
use crate::semantics::types::{
    Completion, DeferredType, IndexKeyKind, InvalidType, LiteralProvenance, ParameterType,
    Property, Signature, TypeId, TypeKind, UnionPolicy,
};
use crate::source::{FileId, Span};
use crate::standard_library::{LibraryCallMember, LibraryMemberLookup, LibraryReceiver};
use crate::syntax::{
    Expression, ExpressionKind, Parameter, TypeMember, TypeMemberKind, TypeNode, TypeNodeKind,
};
use std::collections::HashSet;
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ObjectDisplayOrigin {
    members: Vec<(String, Option<String>)>,
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
        let completion = self.property_type(object_type, name, library_member.is_some());
        match self.require_completion(completion) {
            Completion::Complete(Some(ty)) => ty,
            Completion::Complete(None) => {
                let Some(complete_object) = self.complete_type(object_type) else {
                    return self.deferred_property_type(object_type, name, name_span);
                };
                let display = self.display_type_for_diagnostic(complete_object);
                let Completion::Complete(object_name) = self.require_file_completion(file, display)
                else {
                    return self.deferred_property_type(object_type, name, name_span);
                };
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
                self.deferred_property_type(object_type, name, name_span)
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
                Completion::Complete(None)
            }
            TypeKind::Any => Completion::Complete(Some(self.store.builtins.any)),
            TypeKind::Error | TypeKind::Invalid(_) => Completion::Complete(Some(object)),
            _ => Completion::Deferred,
        }
    }
    pub(super) fn deferred_indexed_access_type(
        &mut self,
        object: TypeId,
        index: TypeId,
        index_span: Span,
        receiver_display: Option<ObjectDisplayOrigin>,
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
            receiver_display,
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
    pub(super) fn deferred_property_type(
        &mut self,
        object: TypeId,
        name: &str,
        name_span: Span,
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
            let display = self.display_type_for_diagnostic(object);
            let Completion::Complete(object_name) =
                self.require_file_completion(origin.span.file, display)
            else {
                continue;
            };
            self.push_diagnostic(
                origin.span.file,
                origin.span,
                format!("Property '{name}' does not exist on type '{object_name}'."),
                2339,
            );
        }
    }
    /// Render diagnostic text from the semantic type graph without forcing a
    /// query or consulting a parallel syntax-order mirror. `ObjectShape` owns
    /// stable authored member order; only declaration-owned alias spelling is
    /// recovered here for a symbolic reference at the active demand.
    pub(super) fn display_type_for_diagnostic(&self, ty: TypeId) -> Completion<String> {
        self.display_type_for_diagnostic_inner(ty, &mut HashSet::new(), 0)
    }
    fn display_type_for_diagnostic_inner(
        &self,
        ty: TypeId,
        active: &mut HashSet<TypeId>,
        depth: usize,
    ) -> Completion<String> {
        if depth > 24 {
            return Completion::Complete("...".to_owned());
        }
        if !active.insert(ty) {
            return Completion::Complete("...".to_owned());
        }
        let result = match self.store.kind(ty) {
            TypeKind::Deferred(DeferredType::Reference { declaration, .. })
                if self.declaration_preserves_alias_name(*declaration) =>
            {
                self.declaration_name(*declaration)
                    .map(str::to_owned)
                    .map_or(Completion::Deferred, Completion::Complete)
            }
            TypeKind::Deferred(_) => match self.ready_type_for_display(ty) {
                Completion::Complete(ready) if ready != ty => {
                    self.display_type_for_diagnostic_inner(ready, active, depth + 1)
                }
                Completion::Complete(_) | Completion::Deferred => Completion::Deferred,
                Completion::Cycle => Completion::Cycle,
                Completion::Limit => Completion::Limit,
            },
            TypeKind::Array(element) => {
                let element_name =
                    completed!(
                        self.display_type_for_diagnostic_inner(*element, active, depth + 1,)
                    );
                let parentheses = matches!(
                    self.store.kind(*element),
                    TypeKind::Union(_) | TypeKind::Intersection(_) | TypeKind::Function(_)
                );
                Completion::Complete(if parentheses {
                    format!("({element_name})[]")
                } else {
                    format!("{element_name}[]")
                })
            }
            TypeKind::Tuple(elements) => {
                let mut rendered = Vec::with_capacity(elements.len());
                for element in elements {
                    rendered.push(completed!(self.display_type_for_diagnostic_inner(
                        *element,
                        active,
                        depth + 1,
                    )));
                }
                Completion::Complete(format!("[{}]", rendered.join(", ")))
            }
            TypeKind::Union(members) | TypeKind::Intersection(members) => {
                let is_union = matches!(self.store.kind(ty), TypeKind::Union(_));
                let separator = if is_union { " | " } else { " & " };
                let mut rendered = Vec::with_capacity(members.len());
                for member in members {
                    rendered.push(completed!(self.display_type_for_diagnostic_inner(
                        *member,
                        active,
                        depth + 1,
                    )));
                }
                if is_union
                    && members.iter().all(|member| {
                        matches!(
                            self.store.kind(*member),
                            TypeKind::Deferred(DeferredType::Reference { declaration, .. })
                                if self.declaration_preserves_alias_name(*declaration)
                        )
                    })
                {
                    rendered.sort();
                    rendered.dedup();
                }
                Completion::Complete(rendered.join(separator))
            }
            TypeKind::Object(shape) => {
                if !shape.call_signatures.is_empty()
                    || !shape.construct_signatures.is_empty()
                    || !shape.index_signatures.is_empty()
                {
                    Completion::Deferred
                } else if shape.properties.is_empty() {
                    Completion::Complete("{}".to_owned())
                } else {
                    let mut rendered = Vec::with_capacity(shape.properties.len());
                    for property in &shape.properties {
                        rendered.push(format!(
                            "{}{}: {}",
                            property.name,
                            if property.optional { "?" } else { "" },
                            completed!(self.display_type_for_diagnostic_inner(
                                property.ty,
                                active,
                                depth + 1,
                            )),
                        ));
                    }
                    Completion::Complete(format!("{{ {}; }}", rendered.join("; ")))
                }
            }
            _ => self.store.display(ty),
        };
        active.remove(&ty);
        result
    }
    pub(super) fn declaration_name(&self, declaration: crate::source::DeclId) -> Option<&str> {
        self.program
            .file(declaration.file)?
            .bindings
            .declaration(declaration)
            .map(|declaration| declaration.name.as_str())
    }
    pub(super) fn declaration_preserves_alias_name(
        &self,
        declaration: crate::source::DeclId,
    ) -> bool {
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
            let display = self.indexed_access_receiver_name(
                object,
                origin.receiver_display.as_ref(),
                origin.receiver_alias.as_deref(),
            );
            let Completion::Complete(object_name) =
                self.require_file_completion(origin.span.file, display)
            else {
                continue;
            };
            for name in names {
                self.push_diagnostic(
                    origin.span.file,
                    origin.span,
                    format!("Property '{name}' does not exist on type '{object_name}'."),
                    2339,
                );
            }
        }
    }
    fn indexed_access_receiver_name(
        &self,
        object: TypeId,
        receiver_display: Option<&ObjectDisplayOrigin>,
        receiver_alias: Option<&str>,
    ) -> Completion<String> {
        if let Some(alias) = receiver_alias {
            return Completion::Complete(alias.to_string());
        }
        let _ = completed!(self.store.display(object));
        Completion::Complete(match self.store.kind(object) {
            TypeKind::Boolean | TypeKind::LiteralBoolean(_, _) => "Boolean".to_string(),
            TypeKind::Number | TypeKind::LiteralNumber(_, _) => "Number".to_string(),
            TypeKind::String | TypeKind::LiteralString(_, _) => "String".to_string(),
            TypeKind::BigInt => "BigInt".to_string(),
            TypeKind::Symbol => "Symbol".to_string(),
            TypeKind::Object(shape) if receiver_display.is_some() => {
                let origin = receiver_display.expect("checked above");
                let mut members = Vec::with_capacity(shape.properties.len());
                for (name, authored_type) in &origin.members {
                    let Some(property) = shape
                        .properties
                        .iter()
                        .find(|property| &property.name == name)
                    else {
                        continue;
                    };
                    let ty = authored_type
                        .clone()
                        .unwrap_or(completed!(self.display_type_for_diagnostic(property.ty)));
                    members.push(format!(
                        "{}{}: {ty}",
                        property.name,
                        if property.optional { "?" } else { "" },
                    ));
                }
                format!("{{ {}; }}", members.join("; "))
            }
            _ => completed!(self.display_type_for_diagnostic(object)),
        })
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
pub(super) fn direct_object_display_origin(node: &TypeNode) -> Option<ObjectDisplayOrigin> {
    if let TypeNodeKind::Parenthesized(inner) | TypeNodeKind::Readonly(inner) = &node.kind {
        return direct_object_display_origin(inner);
    }
    let TypeNodeKind::Object(members) = &node.kind else {
        return None;
    };
    let members = members
        .iter()
        .filter_map(|member| {
            let TypeMemberKind::Property { name, ty, .. } = &member.kind else {
                return None;
            };
            Some((
                name.semantic_name()?.to_string(),
                ty.as_ref().and_then(authored_type_reference_name),
            ))
        })
        .collect();
    Some(ObjectDisplayOrigin { members })
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
