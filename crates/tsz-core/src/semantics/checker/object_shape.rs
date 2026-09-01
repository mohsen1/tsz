use super::{
    Checker, DeclarationModel,
    recursion::{AliasRecursionProductivity, ReferenceRecursion},
    relation_diagnostic::ContextualType,
};
use crate::bind::{DeclarationKind, Meaning, ScopeId, TypeMemberSymbol};
use crate::semantics::types::{
    Completion, DeferredType, IndexKeyKind, IndexSignature, ObjectShape, ParameterType, Property,
    Signature, TypeId, TypeKind, TypeStore, UnionPolicy,
};
use crate::source::{DeclId, FileId};
use crate::syntax::{
    Expression, ExpressionKind, InterfaceDeclaration, KeywordType, Parameter, ParameterNameKind,
    TypeMember, TypeMemberKind, TypeMemberModifiers, TypeNode, TypeNodeKind,
    TypeParameterDeclaration,
};
use std::collections::{HashMap, HashSet};
impl Checker<'_> {
    pub(super) fn infer_array_expression(
        &mut self,
        file: FileId,
        scope: ScopeId,
        elements: &[Expression],
        expected: ContextualType,
    ) -> TypeId {
        let mut inferred = elements
            .iter()
            .map(|element| {
                let ty = self.infer_expression_contextual(file, scope, element, expected);
                if expected.is_known() {
                    ty
                } else {
                    self.widen(ty)
                }
            })
            .collect::<Vec<_>>();
        let object_union = self.options.effective_strict_null_checks()
            && inferred.len() > 1
            && elements.iter().all(|element| {
                matches!(element.peel_parentheses().kind, ExpressionKind::Object(_))
            });
        if object_union {
            inferred = self.normalize_object_literal_union(inferred);
        }
        let element = self.store.union(
            inferred,
            if object_union {
                UnionPolicy::PreserveAuthoredStructuralOrder
            } else {
                UnionPolicy::Canonical
            },
        );
        self.store.intern(TypeKind::Array(element))
    }
    fn normalize_object_literal_union(&mut self, members: Vec<TypeId>) -> Vec<TypeId> {
        let mut names = Vec::new();
        for member in &members {
            let TypeKind::Object(shape) = self.store.kind(*member) else {
                return members;
            };
            if !shape.call_signatures.is_empty()
                || !shape.construct_signatures.is_empty()
                || !shape.index_signatures.is_empty()
            {
                return members;
            }
            for property in &shape.properties {
                if !names.contains(&property.name) {
                    names.push(property.name.clone());
                }
            }
        }
        members
            .into_iter()
            .map(|member| {
                let TypeKind::Object(mut shape) = self.store.kind(member).clone() else {
                    unreachable!("object-literal union members were validated above")
                };
                for name in &names {
                    if !shape
                        .properties
                        .iter()
                        .any(|property| &property.name == name)
                    {
                        shape.properties.push(Property {
                            name: name.clone(),
                            ty: self.store.builtins.undefined,
                            optional: true,
                            readonly: false,
                        });
                    }
                }
                self.store.object_shape(shape)
            })
            .collect()
    }
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
        // Bounded property-only heritage: sole one-hop declarations, required
        // unmodified members, and exact positional generic pass-through.
        let base_declarations =
            completed!(self.plain_property_interface_heritage_bases(declaration));
        debug_assert_eq!(interface.extends.len(), base_declarations.len());
        let mut shape = ObjectShape::default();
        let mut property_indices = HashMap::new();
        let own_shape = completed!(self.resolve_object_members(
            declaration.file,
            scope,
            &interface.members,
            type_parameters,
        ));
        if !merge_plain_property_shape(&mut shape, &mut property_indices, own_shape) {
            return Completion::Deferred;
        }
        let mut bases = interface
            .extends
            .iter()
            .zip(base_declarations.iter().copied())
            .collect::<Vec<_>>();
        // Shape order is own members, then bases by declaration identity.
        bases.sort_by_key(|(_, declaration)| *declaration);
        for (heritage, base_declaration) in bases {
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
                completed!(self.shape_child_type_supported(*argument, &mut HashSet::new()));
            }
            // Resolve proven sole bases directly so productive mutual shape
            // edges close without re-entering the active reference query.
            let base_parameters =
                self.substitution(base_declaration, &base.type_parameters, &arguments);
            let base_shape = completed!(self.resolve_object_members(
                base_declaration.file,
                base_scope,
                &base.members,
                &base_parameters,
            ));
            if !merge_plain_property_shape(&mut shape, &mut property_indices, base_shape) {
                return Completion::Deferred;
            }
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
        let bound = match (bound.meaning, bound.kind) {
            (Meaning::Type, _) => bound,
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
                counterpart
            }
            (Meaning::Value, _) => return false,
        };
        let is_global = bound.scope == ScopeId(0) && !file.is_external_module();
        if !self.is_sole_symbol_declaration(file, bound, is_global) {
            return false;
        }
        if bound.kind != DeclarationKind::Class {
            return true;
        }
        let mut counterparts = file.bindings.declarations.iter().filter(|candidate| {
            candidate.owner == bound.owner
                && candidate.kind == DeclarationKind::Class
                && candidate.meaning == Meaning::Value
        });
        let Some(counterpart) = counterparts.next() else {
            return false;
        };
        counterparts.next().is_none()
            && self.is_sole_symbol_declaration(file, counterpart, is_global)
    }
    fn is_sole_symbol_declaration(
        &self,
        file: &crate::program::ProgramFile,
        declaration: &crate::bind::BoundDeclaration,
        is_global: bool,
    ) -> bool {
        if is_global {
            let declarations = match declaration.meaning {
                Meaning::Value => &self.program.global_values,
                Meaning::Type => &self.program.global_types,
            };
            return self
                .program
                .standard_library
                .resolve(&declaration.name, declaration.meaning)
                .is_none()
                && declarations
                    .get(&declaration.name)
                    .is_some_and(|declarations| declarations.as_slice() == [declaration.id]);
        }
        file.bindings
            .scopes
            .get(declaration.scope.0 as usize)
            .and_then(|scope| scope.names.get(&declaration.name))
            .is_some_and(|declarations| {
                declarations
                    .iter()
                    .filter_map(|candidate| file.bindings.declaration(*candidate))
                    .filter(|candidate| candidate.meaning == declaration.meaning)
                    .map(|candidate| candidate.id)
                    .eq([declaration.id])
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
    /// Prove a nonforcing one-hop interface base list with exact substitution.
    /// Conflicts remain owned by property assembly after substitution.
    pub(super) fn plain_property_interface_heritage_bases(
        &self,
        declaration: DeclId,
    ) -> Completion<Vec<DeclId>> {
        // Direct/mutual illegal heritage stays typed; broader graphs defer.
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
    /// Validate a bounded declaration-keyed heritage path before cycle
    /// classification; only one empty-derived/plain-base edge is acyclic.
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
        // Inspect one extra edge for TS2310-family cycles; transitive graphs defer.
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
                        completed!(self.shape_child_type_supported(ty, &mut HashSet::new()));
                        ty
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
                    let signature = completed!(self.resolve_shape_signature(
                        file,
                        member_scope,
                        parameters,
                        return_type.as_ref(),
                        type_parameters,
                    ));
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
                    let signature = completed!(self.resolve_shape_signature(
                        file,
                        member_scope,
                        parameters,
                        return_type.as_ref(),
                        type_parameters,
                    ));
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
                    let signature = completed!(self.resolve_shape_signature(
                        file,
                        member_scope,
                        parameters,
                        return_type.as_ref(),
                        type_parameters,
                    ));
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
                        // Numeric key canonicalization is not yet owned.
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
                    completed!(self.shape_child_type_supported(value, &mut HashSet::new()));
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
            // Mixed/dual indexes defer until TS2411/TS2413 provenance is owned.
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
    ) -> Completion<Signature> {
        if parameters
            .iter()
            .any(|parameter| parameter.name_kind == ParameterNameKind::This)
        {
            return Completion::Deferred;
        }
        let mut semantic_parameters = Vec::with_capacity(parameters.len());
        for parameter in parameters {
            if parameter.rest || !parameter.modifiers.is_empty() {
                return Completion::Deferred;
            }
            let ty = if let Some(annotation) = &parameter.annotation {
                let ty = self.resolve_type_node(file, scope, annotation, type_parameters);
                completed!(self.shape_child_type_supported(ty, &mut HashSet::new()));
                ty
            } else if let Some(initializer) = &parameter.initializer {
                completed!(self.signature_initializer_type(file, scope, initializer))
            } else if self.options.effective_no_implicit_any() {
                return Completion::Deferred;
            } else {
                self.store.builtins.any
            };
            semantic_parameters.push(ParameterType {
                name: None,
                ty,
                optional: parameter.optional || parameter.initializer.is_some(),
                rest: parameter.rest,
            });
        }
        let return_type = if let Some(return_type) = return_type {
            let ty = self.resolve_type_node(file, scope, return_type, type_parameters);
            completed!(self.shape_child_type_supported(ty, &mut HashSet::new()));
            ty
        } else if self.options.effective_no_implicit_any() {
            return Completion::Deferred;
        } else {
            self.store.builtins.any
        };
        Completion::Complete(Signature {
            generic_declaration: None,
            untyped_javascript: false,
            parameters: semantic_parameters,
            return_type,
        })
    }
    /// Shape signatures are name-free; authored callables remain deferred
    /// until semantic and display provenance are separate.
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
            TypeKind::Deferred(deferred @ DeferredType::Reference { .. }) => {
                let DeferredType::Reference {
                    declaration,
                    arguments,
                } = &deferred
                else {
                    unreachable!()
                };
                // Validate arguments before admitting a provisional edge.
                for argument in arguments {
                    completed!(self.shape_child_type_supported(*argument, active));
                }
                match self.shape_reference_recursion(ty, *declaration, arguments) {
                    ReferenceRecursion::Exact => {
                        // Productive object aliases may revisit the exact reference.
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
                        // TS7 keeps a growing generic edge provisional and
                        // noncacheable while every sibling still completes.
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
            TypeKind::Deferred(deferred)
                if matches!(
                    &deferred,
                    DeferredType::IndexedAccess { object, index }
                        if !matches!(self.store.kind(*object), TypeKind::Deferred(_))
                            && !matches!(self.store.kind(*index), TypeKind::Deferred(_))
                ) || matches!(
                    &deferred,
                    DeferredType::KeyOf(operand)
                        if !matches!(self.store.kind(*operand), TypeKind::Deferred(_))
                ) =>
            {
                if let DeferredType::KeyOf(operand) = &deferred
                    && matches!(self.store.kind(*operand), TypeKind::TypeParameter { .. })
                {
                    self.symbolic_keyof_operand_supported(*operand)
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
            // Authored callables and unresolved children both lack a definitive shape.
            TypeKind::Function(_) | TypeKind::Deferred(_) => Completion::Deferred,
            TypeKind::Invalid(_) => Completion::Complete(()),
            kind => {
                let mut children = Vec::new();
                TypeStore::push_type_children(&kind, &mut children);
                for child in children {
                    completed!(self.shape_child_type_supported(child, active));
                }
                Completion::Complete(())
            }
        };
        active.remove(&ty);
        result
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
pub(super) fn authored_structural_union_member(node: &TypeNode) -> bool {
    match &node.kind {
        TypeNodeKind::Object(_) => true,
        TypeNodeKind::Array(element)
        | TypeNodeKind::Readonly(element)
        | TypeNodeKind::Parenthesized(element) => authored_structural_union_member(element),
        TypeNodeKind::Tuple(elements) => {
            !elements.is_empty() && elements.iter().all(authored_structural_union_member)
        }
        _ => false,
    }
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
