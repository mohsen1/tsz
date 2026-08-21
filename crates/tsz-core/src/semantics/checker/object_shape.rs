use std::collections::{HashMap, HashSet};

use crate::bind::{DeclarationKind, Meaning, ScopeId, TypeMemberSymbol};
use crate::source::{DeclId, FileId};
use crate::syntax::{
    InterfaceDeclaration, KeywordType, Parameter, TypeMember, TypeMemberKind, TypeMemberModifiers,
    TypeNode, TypeNodeKind, TypeParameterDeclaration,
};

use super::{
    Checker, DeclarationModel,
    recursion::{AliasRecursionProductivity, ReferenceRecursion},
};
use crate::semantics::types::{
    Completion, DeferredType, IndexKeyKind, IndexSignature, ObjectShape, Property, ShapeParameter,
    ShapeSignature, TypeId, TypeKind,
};

impl Checker<'_> {
    pub(super) fn resolve_interface_shape(
        &mut self,
        declaration: DeclId,
        interface: &InterfaceDeclaration,
        scope: ScopeId,
        type_parameters: &HashMap<String, TypeId>,
    ) -> Completion<ObjectShape> {
        if interface.extends.is_empty() {
            return self.resolve_object_members(
                declaration.file,
                scope,
                &interface.members,
                type_parameters,
            );
        }

        // This is the bounded property-only heritage boundary. TSZ does not
        // yet own general base-type diagnostics, so every declaration in the
        // one-hop graph must be sole and every member must be a required,
        // unmodified property. Exact positional generic pass-through keeps
        // substitution declaration-owned; transitive and transformed bases
        // remain symbolic.
        let base_declarations = match self.plain_property_interface_heritage_bases(declaration) {
            Completion::Complete(bases) => bases,
            Completion::Deferred => return Completion::Deferred,
            Completion::Cycle => return Completion::Cycle,
            Completion::Limit => return Completion::Limit,
        };
        debug_assert_eq!(interface.extends.len(), base_declarations.len());

        let mut shape = ObjectShape::default();
        let mut property_indices = HashMap::new();
        for (heritage, base_declaration) in interface
            .extends
            .iter()
            .zip(base_declarations.iter().copied())
        {
            let Some(DeclarationModel::Interface {
                declaration: base,
                scope: base_scope,
            }) = self.models.get(&base_declaration).copied()
            else {
                return Completion::Deferred;
            };
            let base_reference =
                self.resolve_type_node(declaration.file, scope, heritage, type_parameters);
            let TypeKind::Deferred(deferred) = self.store.kind(base_reference).clone() else {
                return Completion::Deferred;
            };
            let DeferredType::Reference {
                declaration: resolved_base,
                arguments,
            } = deferred
            else {
                return Completion::Deferred;
            };
            if resolved_base != base_declaration || arguments.len() != base.type_parameters.len() {
                return Completion::Deferred;
            }
            for argument in &arguments {
                match self.shape_child_type_supported(*argument, &mut HashSet::new()) {
                    Completion::Complete(()) => {}
                    Completion::Deferred => return Completion::Deferred,
                    Completion::Cycle => return Completion::Cycle,
                    Completion::Limit => return Completion::Limit,
                }
            }

            // Bases have already been proven sole and heritage-free. Resolve
            // their authored members directly with the base declaration's
            // parameter names instead of re-entering a base reference query.
            // That permits a productive `Base<T> -> Derived<T> -> Base<T>`
            // shape edge to close provisionally without converting the
            // active base query into an illegal-heritage Cycle.
            let base_parameters =
                self.substitution(base_declaration, &base.type_parameters, &arguments);
            let base_shape = match self.resolve_object_members(
                base_declaration.file,
                base_scope,
                &base.members,
                &base_parameters,
            ) {
                Completion::Complete(shape) => shape,
                Completion::Deferred => return Completion::Deferred,
                Completion::Cycle => return Completion::Cycle,
                Completion::Limit => return Completion::Limit,
            };
            if !merge_plain_property_shape(&mut shape, &mut property_indices, base_shape) {
                return Completion::Deferred;
            }
        }

        let own_shape = match self.resolve_object_members(
            declaration.file,
            scope,
            &interface.members,
            type_parameters,
        ) {
            Completion::Complete(shape) => shape,
            Completion::Deferred => return Completion::Deferred,
            Completion::Cycle => return Completion::Cycle,
            Completion::Limit => return Completion::Limit,
        };
        if !merge_plain_property_shape(&mut shape, &mut property_indices, own_shape) {
            return Completion::Deferred;
        }
        Completion::Complete(shape)
    }

    pub(super) fn is_single_interface_declaration(&self, declaration: DeclId) -> bool {
        let Some(file) = self.program.file(declaration.file) else {
            return false;
        };
        let Some(bound) = file.bindings.declaration(declaration) else {
            return false;
        };
        bound.kind == DeclarationKind::Interface
            && self.is_single_type_symbol_declaration(declaration)
            && matches!(
                self.models.get(&declaration),
                Some(DeclarationModel::Interface { .. })
            )
    }

    pub(super) fn is_single_type_symbol_declaration(&self, declaration: DeclId) -> bool {
        let Some(file) = self.program.file(declaration.file) else {
            return false;
        };
        let Some(bound) = file.bindings.declaration(declaration) else {
            return false;
        };
        let declaration = match (bound.meaning, bound.kind) {
            (Meaning::Type, _) => declaration,
            (Meaning::Value, DeclarationKind::Class) => {
                let mut counterparts = file.bindings.declarations.iter().filter(|candidate| {
                    candidate.owner == bound.owner
                        && candidate.kind == DeclarationKind::Class
                        && candidate.meaning == Meaning::Type
                });
                let Some(counterpart) = counterparts.next() else {
                    return false;
                };
                if counterparts.next().is_some() {
                    return false;
                }
                counterpart.id
            }
            (Meaning::Value, _) => return false,
        };
        let Some(bound) = file.bindings.declaration(declaration) else {
            return false;
        };
        let is_global = bound.scope == ScopeId(0) && !file.is_external_module();
        if is_global
            && self
                .program
                .standard_library
                .resolve(&bound.name, Meaning::Type)
                .is_some()
        {
            return false;
        }
        if bound.kind == DeclarationKind::Class
            && !self.is_single_class_value_declaration(file, bound, is_global)
        {
            return false;
        }
        let declarations = if is_global {
            self.program
                .global_types
                .get(&bound.name)
                .map(Vec::as_slice)
        } else {
            file.bindings
                .scopes
                .get(bound.scope.0 as usize)
                .and_then(|scope| scope.names.get(&bound.name))
                .map(Vec::as_slice)
        };
        declarations.is_some_and(|declarations| {
            let mut type_declarations = declarations.iter().copied().filter(|candidate| {
                self.program
                    .file(candidate.file)
                    .and_then(|candidate_file| candidate_file.bindings.declaration(*candidate))
                    .is_some_and(|candidate| candidate.meaning == Meaning::Type)
            });
            type_declarations.next() == Some(declaration) && type_declarations.next().is_none()
        })
    }

    fn is_single_class_value_declaration(
        &self,
        file: &crate::program::ProgramFile,
        class_type: &crate::bind::BoundDeclaration,
        is_global: bool,
    ) -> bool {
        let mut counterparts = file.bindings.declarations.iter().filter(|candidate| {
            candidate.owner == class_type.owner
                && candidate.kind == DeclarationKind::Class
                && candidate.meaning == Meaning::Value
        });
        let Some(counterpart) = counterparts.next() else {
            return false;
        };
        if counterparts.next().is_some()
            || is_global
                && self
                    .program
                    .standard_library
                    .resolve(&class_type.name, Meaning::Value)
                    .is_some()
        {
            return false;
        }
        let declarations = if is_global {
            self.program
                .global_values
                .get(&class_type.name)
                .map(Vec::as_slice)
        } else {
            file.bindings
                .scopes
                .get(class_type.scope.0 as usize)
                .and_then(|scope| scope.names.get(&class_type.name))
                .map(Vec::as_slice)
        };
        declarations.is_some_and(|declarations| {
            let mut value_declarations = declarations.iter().copied().filter(|candidate| {
                self.program
                    .file(candidate.file)
                    .and_then(|candidate_file| candidate_file.bindings.declaration(*candidate))
                    .is_some_and(|candidate| candidate.meaning == Meaning::Value)
            });
            value_declarations.next() == Some(counterpart.id) && value_declarations.next().is_none()
        })
    }

    pub(super) fn plain_property_interface_heritage_reference_supported(
        &self,
        declaration: DeclId,
        arguments: &[TypeId],
    ) -> bool {
        let Completion::Complete(_) = self.plain_property_interface_heritage_bases(declaration)
        else {
            return false;
        };
        let Some(DeclarationModel::Interface {
            declaration: derived,
            ..
        }) = self.models.get(&declaration).copied()
        else {
            return false;
        };
        arguments.len() == derived.type_parameters.len()
    }

    /// Select a bounded one-hop interface base list for property assembly.
    ///
    /// This query does not force types. It proves that authored order and
    /// substitution are sufficient to assemble the eventual object shape:
    /// every declaration is sole, all parameters are plain, every heritage
    /// argument is the corresponding derived parameter, and every member is
    /// a required property. Conflicts are decided only after substitution.
    pub(super) fn plain_property_interface_heritage_bases(
        &self,
        declaration: DeclId,
    ) -> Completion<Vec<DeclId>> {
        // Retain the existing typed result for the narrow direct and mutual
        // illegal-heritage cycles. Broader/transitive graphs remain Deferred.
        if matches!(
            self.narrow_interface_heritage_base(declaration),
            Completion::Cycle
        ) {
            return Completion::Cycle;
        }

        let Some((interface, scope)) = self.narrow_heritage_interface(declaration) else {
            return Completion::Deferred;
        };
        if interface.extends.is_empty() || !interface.members.iter().all(plain_required_property) {
            return Completion::Deferred;
        }

        let mut bases = Vec::with_capacity(interface.extends.len());
        for heritage in &interface.extends {
            let Some(base) =
                self.interface_base_from_heritage(declaration.file, interface, scope, heritage)
            else {
                return Completion::Deferred;
            };
            if base == declaration {
                return Completion::Deferred;
            }
            let Some(DeclarationModel::Interface {
                declaration: base_interface,
                ..
            }) = self.models.get(&base).copied()
            else {
                return Completion::Deferred;
            };
            if !base_interface.extends.is_empty()
                || !plain_type_parameters(&base_interface.type_parameters)
                || base_interface.type_parameters.len() != interface.type_parameters.len()
                || !base_interface.members.iter().all(plain_required_property)
            {
                return Completion::Deferred;
            }
            bases.push(base);
        }
        Completion::Complete(bases)
    }

    /// Validate the complete narrow heritage path before classifying a cycle.
    ///
    /// This is bounded and declaration-keyed: merged symbols cannot be
    /// selected by root order, and a long or diamond-shaped base graph cannot
    /// recurse or be rescanned quadratically. Only one direct
    /// empty-derived/plain-base edge is a supported acyclic result. Direct and
    /// mutual validated revisits retain the separate illegal-heritage Cycle.
    fn narrow_interface_heritage_base(&self, declaration: DeclId) -> Completion<DeclId> {
        let Some((interface, scope)) = self.narrow_heritage_interface(declaration) else {
            return Completion::Deferred;
        };
        if !interface.members.is_empty() {
            return Completion::Deferred;
        }
        let Some(base) = self.direct_interface_base(declaration, interface, scope) else {
            return Completion::Deferred;
        };
        if base == declaration {
            return Completion::Cycle;
        }
        let Some((base_interface, base_scope)) = self.narrow_heritage_interface(base) else {
            return Completion::Deferred;
        };
        if base_interface.type_parameters.len() != interface.type_parameters.len() {
            return Completion::Deferred;
        }
        if base_interface.extends.is_empty() {
            return Completion::Complete(base);
        }
        if !base_interface.members.is_empty() {
            return Completion::Deferred;
        }

        // Transitive inheritance is outside the narrow merge. Inspect only
        // one more validated edge so direct/mutual TS2310-family cycles do not
        // become successful shapes, then fail closed without walking a chain.
        let Some(next) = self.direct_interface_base(base, base_interface, base_scope) else {
            return Completion::Deferred;
        };
        if next == declaration || next == base {
            Completion::Cycle
        } else {
            Completion::Deferred
        }
    }

    fn narrow_heritage_interface(
        &self,
        declaration: DeclId,
    ) -> Option<(&InterfaceDeclaration, ScopeId)> {
        if !self.is_single_interface_declaration(declaration) {
            return None;
        }
        let DeclarationModel::Interface {
            declaration: interface,
            scope,
        } = self.models.get(&declaration).copied()?
        else {
            return None;
        };
        plain_type_parameters(&interface.type_parameters).then_some((interface, scope))
    }

    fn direct_interface_base(
        &self,
        declaration: DeclId,
        interface: &InterfaceDeclaration,
        scope: ScopeId,
    ) -> Option<DeclId> {
        let [heritage] = interface.extends.as_slice() else {
            return None;
        };
        self.interface_base_from_heritage(declaration.file, interface, scope, heritage)
    }

    fn interface_base_from_heritage(
        &self,
        file: FileId,
        interface: &InterfaceDeclaration,
        scope: ScopeId,
        heritage: &TypeNode,
    ) -> Option<DeclId> {
        let TypeNodeKind::Reference {
            name, arguments, ..
        } = &heritage.kind
        else {
            return None;
        };
        if !positional_type_parameter_pass_through(&interface.type_parameters, arguments) {
            return None;
        }
        let base = self.sole_interface_reference(file, scope, name)?;
        let DeclarationModel::Interface {
            declaration: base_interface,
            ..
        } = self.models.get(&base).copied()?
        else {
            return None;
        };
        (base_interface.type_parameters.len() == interface.type_parameters.len()).then_some(base)
    }

    fn sole_interface_reference(
        &self,
        file: FileId,
        mut scope: ScopeId,
        name: &str,
    ) -> Option<DeclId> {
        let bound = &self.program.files[file.0 as usize].bindings;
        loop {
            let current = bound.scopes.get(scope.0 as usize)?;
            if let Some(declarations) = current.names.get(name) {
                let declarations = declarations
                    .iter()
                    .copied()
                    .filter(|candidate| {
                        bound
                            .declaration(*candidate)
                            .is_some_and(|declaration| declaration.meaning == Meaning::Type)
                    })
                    .collect::<Vec<_>>();
                if !declarations.is_empty() {
                    return match declarations.as_slice() {
                        [only] if self.is_single_interface_declaration(*only) => Some(*only),
                        _ => None,
                    };
                }
            }
            let Some(parent) = current.parent else {
                break;
            };
            scope = parent;
        }

        match self.program.global_types.get(name).map(Vec::as_slice) {
            Some([only]) if self.is_single_interface_declaration(*only) => Some(*only),
            _ => None,
        }
    }

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
            TypeKind::Object(shape) => self.shape_children_supported(&shape, active),
            TypeKind::ClassInstance {
                arguments,
                properties,
                ..
            } => {
                for argument in arguments {
                    match self.shape_child_type_supported(argument, active) {
                        Completion::Complete(()) => {}
                        other => return other,
                    }
                }
                self.shape_children_supported(&properties, active)
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
                let DeferredType::Reference {
                    declaration,
                    arguments,
                } = &deferred
                else {
                    unreachable!()
                };
                // A provisional recursive edge cannot hide an unsupported
                // callable or another incomplete form inside its arguments.
                for argument in arguments {
                    match self.shape_child_type_supported(*argument, active) {
                        Completion::Complete(()) => {}
                        other => return other,
                    }
                }
                match self.shape_reference_recursion(ty, *declaration, arguments) {
                    ReferenceRecursion::Exact => {
                        // Productive recursive object aliases revisit the
                        // exact reference while its shape is assembled.
                        match self.models.get(declaration) {
                            Some(DeclarationModel::TypeAlias { .. }) => {
                                match self.alias_recursion_productivity(*declaration) {
                                    AliasRecursionProductivity::Productive => {
                                        Completion::Complete(())
                                    }
                                    AliasRecursionProductivity::Unproductive => Completion::Cycle,
                                    AliasRecursionProductivity::Acyclic
                                    | AliasRecursionProductivity::Unsupported => {
                                        Completion::Deferred
                                    }
                                }
                            }
                            Some(DeclarationModel::Interface { .. })
                                if self.reference_expansion_frame_supported(
                                    *declaration,
                                    arguments,
                                ) =>
                            {
                                Completion::Complete(())
                            }
                            _ => Completion::Deferred,
                        }
                    }
                    ReferenceRecursion::Generative => {
                        // Pinned TS7 treats a repeatedly instantiated generic
                        // origin as provisional recursion. This edge remains
                        // symbolic and non-cacheable; enclosing siblings still
                        // have to complete before the shape succeeds.
                        if self.generative_reference_supported(*declaration, arguments) {
                            Completion::Complete(())
                        } else {
                            Completion::Deferred
                        }
                    }
                    ReferenceRecursion::UnsupportedGenerative => Completion::Deferred,
                    ReferenceRecursion::Distinct => {
                        self.force_reference_shape(ty, deferred, active)
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

    fn shape_children_supported(
        &mut self,
        shape: &ObjectShape,
        active: &mut HashSet<TypeId>,
    ) -> Completion<()> {
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

    fn force_reference_shape(
        &mut self,
        ty: TypeId,
        deferred: DeferredType,
        active: &mut HashSet<TypeId>,
    ) -> Completion<()> {
        let DeferredType::Reference {
            declaration,
            arguments,
        } = &deferred
        else {
            unreachable!()
        };
        let declaration = *declaration;
        let arguments = arguments.clone();
        let checkpoint = self.force_reference_stack.checkpoint();
        self.force_reference_stack.push(ty, declaration, &arguments);
        let completion = match self.force_deferred(ty, deferred, 0) {
            Completion::Complete(resolved) if resolved != ty => {
                self.shape_child_type_supported(resolved, active)
            }
            Completion::Complete(_) | Completion::Deferred => Completion::Deferred,
            Completion::Cycle => Completion::Cycle,
            Completion::Limit => Completion::Limit,
        };
        self.force_reference_stack.restore(checkpoint);
        completion
    }
}

fn merge_plain_property_shape(
    target: &mut ObjectShape,
    property_indices: &mut HashMap<String, usize>,
    source: ObjectShape,
) -> bool {
    if !source.call_signatures.is_empty()
        || !source.construct_signatures.is_empty()
        || !source.index_signatures.is_empty()
    {
        return false;
    }

    for property in source.properties {
        if property.optional || property.readonly {
            return false;
        }
        if let Some(index) = property_indices.get(&property.name).copied() {
            if target.properties[index] != property {
                return false;
            }
        } else {
            property_indices.insert(property.name.clone(), target.properties.len());
            target.properties.push(property);
        }
    }
    true
}

pub(super) fn plain_type_parameters(parameters: &[TypeParameterDeclaration]) -> bool {
    let mut names = HashSet::new();
    parameters.iter().all(|parameter| {
        names.insert(parameter.name.as_str())
            && parameter.constraint.is_none()
            && parameter.default.is_none()
            && !parameter.const_parameter
            && !parameter.in_variance
            && !parameter.out_variance
    })
}

fn positional_type_parameter_pass_through(
    parameters: &[TypeParameterDeclaration],
    arguments: &[TypeNode],
) -> bool {
    parameters.len() == arguments.len()
        && parameters
            .iter()
            .zip(arguments)
            .all(|(parameter, argument)| {
                matches!(
                    &argument.kind,
                    TypeNodeKind::Reference { name, arguments, .. }
                        if name == &parameter.name && arguments.is_empty()
                )
            })
}

pub(super) fn plain_required_property(member: &TypeMember) -> bool {
    !member.recovered
        && member.modifiers.nodes.is_empty()
        && matches!(
            &member.kind,
            TypeMemberKind::Property {
                name,
                ty: Some(_),
                optional: false,
                initializer: None,
            } if name.semantic_name().is_some()
        )
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
