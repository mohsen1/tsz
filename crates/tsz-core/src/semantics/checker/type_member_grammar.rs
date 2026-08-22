use std::collections::{HashMap, HashSet};

use crate::bind::{Meaning, ScopeId};
use crate::semantics::relation::RelationContext;
use crate::semantics::types::{Completion, DeferredType, TypeId, TypeKind};
use crate::source::{DeclId, FileId, NodeId};
use crate::syntax::{
    ExpressionKind, KeywordType, Parameter, ParameterModifier, TypeMember, TypeMemberKind,
    TypeNode, TypeNodeKind,
};

use super::{Checker, DeclarationModel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IndexKeySyntax {
    Valid,
    LiteralOrGeneric,
    Invalid,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RestTypeSyntax {
    Any,
    Bottom,
    ErrorCascade,
    ArrayLike,
    NonArray,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ParameterGrammarHost {
    Signature,
    Implementation { constructor: bool },
}

impl Checker<'_> {
    /// Validate one authored `TypeElement` list. The required-type traversal is
    /// the sole recursive host; these rules never rediscover syntax children.
    pub(super) fn validate_type_member_list(
        &mut self,
        file: FileId,
        scope: ScopeId,
        members: &[TypeMember],
        type_parameters: &HashMap<String, TypeId>,
    ) {
        if members
            .iter()
            .any(|member| member.recovered && member.recovery_incomplete)
        {
            let _ = self.require_completion(Completion::<()>::Deferred);
        }
        self.validate_method_optional_overloads(file, members);
        for member in members.iter().filter(|member| !member.recovered) {
            self.validate_type_member_implicit_any(file, member);
            if let TypeMemberKind::Index {
                parameters,
                value_type,
            } = &member.kind
            {
                self.validate_index_signature(
                    file,
                    scope,
                    member,
                    parameters,
                    value_type.as_ref(),
                    type_parameters,
                );
            }
        }
    }

    pub(super) fn validate_mapped_type_members(&mut self, file: FileId, members: &[TypeMember]) {
        let Some(first) = members.iter().find(|member| !member.recovered) else {
            return;
        };
        let span = match &first.kind {
            TypeMemberKind::Property { name, .. } => name.span,
            TypeMemberKind::Method { .. }
            | TypeMemberKind::Accessor { .. }
            | TypeMemberKind::Call { .. }
            | TypeMemberKind::Construct { .. }
            | TypeMemberKind::Index { .. } => first.span,
        };
        self.push_diagnostic(
            file,
            span,
            "A mapped type may not declare properties or methods.".to_string(),
            7061,
        );
    }

    /// TypeScript reports the declaration-level implicit-`any` facts for
    /// authored type members independently from semantic shape completion.
    /// The latter remains deferred under `noImplicitAny`, so an error recovery
    /// type can never enter a definitive object-shape cache.
    fn validate_type_member_implicit_any(&mut self, file: FileId, member: &TypeMember) {
        if !self.options.effective_no_implicit_any() {
            return;
        }
        match &member.kind {
            TypeMemberKind::Property {
                name,
                ty: None,
                initializer: None,
                ..
            } => {
                if let Some(name_text) = name.semantic_name() {
                    self.push_diagnostic(
                        file,
                        name.span,
                        format!("Member '{name_text}' implicitly has an 'any' type."),
                        7008,
                    );
                } else {
                    let _ = self.require_completion(Completion::<()>::Deferred);
                }
            }
            TypeMemberKind::Method {
                name,
                return_type: None,
                ..
            } => {
                if let Some(name_text) = name.semantic_name() {
                    self.push_diagnostic(
                        file,
                        member.span,
                        format!(
                            "'{name_text}', which lacks return-type annotation, implicitly has an 'any' return type."
                        ),
                        7010,
                    );
                } else {
                    let _ = self.require_completion(Completion::<()>::Deferred);
                }
            }
            TypeMemberKind::Call {
                return_type: None, ..
            } => self.push_diagnostic(
                file,
                member.span,
                "Call signature, which lacks return-type annotation, implicitly has an 'any' return type."
                    .to_string(),
                7020,
            ),
            TypeMemberKind::Construct {
                return_type: None, ..
            } => self.push_diagnostic(
                file,
                member.span,
                "Construct signature, which lacks return-type annotation, implicitly has an 'any' return type."
                    .to_string(),
                7013,
            ),
            TypeMemberKind::Property { .. }
            | TypeMemberKind::Method { .. }
            | TypeMemberKind::Call { .. }
            | TypeMemberKind::Construct { .. }
            | TypeMemberKind::Accessor { .. }
            | TypeMemberKind::Index { .. } => {}
        }
    }

    pub(super) fn validate_implicit_any_parameters(
        &mut self,
        file: FileId,
        parameters: &[Parameter],
    ) {
        if !self.options.effective_no_implicit_any() {
            return;
        }
        for parameter in parameters
            .iter()
            .filter(|parameter| parameter.annotation.is_none() && parameter.initializer.is_none())
        {
            let (code, message) = if parameter.rest {
                (
                    7019,
                    format!(
                        "Rest parameter '{}' implicitly has an 'any[]' type.",
                        parameter.name
                    ),
                )
            } else {
                (
                    7006,
                    format!(
                        "Parameter '{}' implicitly has an 'any' type.",
                        parameter.name
                    ),
                )
            };
            let span = if parameter.rest {
                parameter
                    .rest_span
                    .map_or(parameter.name_span, |rest| rest.merge(parameter.name_span))
            } else {
                parameter.name_span
            };
            self.push_diagnostic(file, span, message, code);
        }
    }

    pub(super) fn validate_authored_parameters(
        &mut self,
        file: FileId,
        scope: ScopeId,
        parameters: &[Parameter],
        type_parameters: &HashMap<String, TypeId>,
    ) {
        let mut occurrences = HashMap::<&str, usize>::new();
        for parameter in parameters {
            *occurrences.entry(parameter.name.as_str()).or_default() += 1;
        }
        for parameter in parameters
            .iter()
            .filter(|parameter| occurrences[parameter.name.as_str()] > 1)
        {
            self.push_diagnostic(
                file,
                parameter.name_span,
                format!("Duplicate identifier '{}'.", parameter.name),
                2300,
            );
        }

        for parameter in parameters.iter().filter(|parameter| parameter.rest) {
            self.validate_rest_parameter_type(file, scope, parameter, type_parameters, true);
        }
    }

    pub(super) fn validate_parameter_host_grammar(
        &mut self,
        file: FileId,
        parameters: &[Parameter],
        host: ParameterGrammarHost,
    ) {
        for parameter in parameters {
            if parameter.optional && parameter.initializer.is_some() {
                self.push_diagnostic(
                    file,
                    parameter.name_span,
                    "Parameter cannot have question mark and initializer.".to_string(),
                    1015,
                );
            }
            if parameter.rest && parameter.initializer.is_some() {
                self.push_diagnostic(
                    file,
                    parameter.name_span,
                    "A rest parameter cannot have an initializer.".to_string(),
                    1048,
                );
            }
            let parameter_property_modifiers = parameter.modifiers.iter().any(|modifier| {
                matches!(
                    modifier.kind,
                    ParameterModifier::Public
                        | ParameterModifier::Protected
                        | ParameterModifier::Private
                        | ParameterModifier::Readonly
                        | ParameterModifier::Override
                )
            });
            if parameter_property_modifiers
                && !matches!(
                    host,
                    ParameterGrammarHost::Implementation { constructor: true }
                )
            {
                self.push_diagnostic(
                    file,
                    parameter.span,
                    "A parameter property is only allowed in a constructor implementation."
                        .to_string(),
                    2369,
                );
            } else if parameter_property_modifiers {
                // Parameter properties also synthesize instance members and
                // runtime assignments. Until the class-shape owner consumes
                // them, the checker must not cache a property-free instance.
                let _ = self.require_completion(Completion::<()>::Deferred);
            }
            if parameter.modifiers.iter().any(|modifier| {
                !matches!(
                    modifier.kind,
                    ParameterModifier::Public
                        | ParameterModifier::Protected
                        | ParameterModifier::Private
                        | ParameterModifier::Readonly
                        | ParameterModifier::Override
                )
            }) {
                let _ = self.require_completion(Completion::<()>::Deferred);
            }
            if parameter.initializer.is_some() && matches!(host, ParameterGrammarHost::Signature) {
                self.push_diagnostic(
                    file,
                    parameter.span,
                    "A parameter initializer is only allowed in a function or constructor implementation."
                        .to_string(),
                    2371,
                );
            }
            if parameter.annotation.is_none()
                && parameter.initializer.as_ref().is_some_and(|initializer| {
                    !matches!(initializer.kind, ExpressionKind::Literal(_))
                })
            {
                let _ = self.require_completion(Completion::<()>::Deferred);
            }
            if parameter.annotation.is_some() && parameter.initializer.is_some() {
                // The initializer-to-annotation relation and body narrowing
                // need a checked parameter summary. Do not let emit or body
                // inference claim success before that owner exists.
                let _ = self.require_completion(Completion::<()>::Deferred);
            }
        }
    }

    fn validate_rest_parameter_type(
        &mut self,
        file: FileId,
        scope: ScopeId,
        parameter: &Parameter,
        type_parameters: &HashMap<String, TypeId>,
        report_optional: bool,
    ) {
        let declared_array_like = parameter.annotation.as_ref().map(|annotation| {
            self.rest_type_semantics(
                file,
                scope,
                annotation,
                type_parameters,
                &mut HashSet::new(),
            )
        });
        let optional_breaks_array = parameter.optional
            && self.options.effective_strict_null_checks()
            && !matches!(
                declared_array_like,
                Some(RestTypeSyntax::Any | RestTypeSyntax::ErrorCascade)
            );
        if matches!(declared_array_like, Some(RestTypeSyntax::NonArray)) || optional_breaks_array {
            self.push_diagnostic(
                file,
                parameter.span,
                "A rest parameter must be of an array type.".to_string(),
                2370,
            );
        } else if matches!(declared_array_like, Some(RestTypeSyntax::Unknown)) {
            let _ = self.require_completion(Completion::<()>::Deferred);
        }
        if report_optional && let Some(optional_span) = parameter.optional_span {
            self.push_diagnostic(
                file,
                optional_span,
                "A rest parameter cannot be optional.".to_string(),
                1047,
            );
        }
    }

    fn rest_type_semantics(
        &mut self,
        file: FileId,
        scope: ScopeId,
        node: &TypeNode,
        type_parameters: &HashMap<String, TypeId>,
        active_aliases: &mut HashSet<DeclId>,
    ) -> RestTypeSyntax {
        let ty = self.resolve_type_node(file, scope, node, type_parameters);
        self.rest_type_id(ty, active_aliases)
    }

    fn rest_type_id(&mut self, ty: TypeId, active_aliases: &mut HashSet<DeclId>) -> RestTypeSyntax {
        match self.store.kind(ty).clone() {
            TypeKind::Any => RestTypeSyntax::Any,
            TypeKind::Never => RestTypeSyntax::Bottom,
            TypeKind::Array(_) | TypeKind::Tuple(_) => RestTypeSyntax::ArrayLike,
            TypeKind::Union(members) => {
                let mut result = RestTypeSyntax::ArrayLike;
                for member in members {
                    match self.rest_type_id(member, active_aliases) {
                        RestTypeSyntax::NonArray => return RestTypeSyntax::NonArray,
                        RestTypeSyntax::Unknown => result = RestTypeSyntax::Unknown,
                        RestTypeSyntax::Any
                        | RestTypeSyntax::Bottom
                        | RestTypeSyntax::ErrorCascade
                        | RestTypeSyntax::ArrayLike => {}
                    }
                }
                result
            }
            TypeKind::Intersection(members) => {
                let mut saw_unknown = false;
                for member in members {
                    match self.rest_type_id(member, active_aliases) {
                        RestTypeSyntax::Any
                        | RestTypeSyntax::Bottom
                        | RestTypeSyntax::ErrorCascade
                        | RestTypeSyntax::ArrayLike => return RestTypeSyntax::ArrayLike,
                        RestTypeSyntax::Unknown => saw_unknown = true,
                        RestTypeSyntax::NonArray => {}
                    }
                }
                if saw_unknown {
                    RestTypeSyntax::Unknown
                } else {
                    RestTypeSyntax::NonArray
                }
            }
            TypeKind::Deferred(DeferredType::Reference {
                declaration,
                arguments,
            }) => {
                if self
                    .program
                    .standard_library
                    .is_rest_array_type(declaration)
                    && arguments.len() == 1
                {
                    return RestTypeSyntax::ArrayLike;
                }
                if !active_aliases.insert(declaration) {
                    return RestTypeSyntax::Unknown;
                }
                let result = match self.models.get(&declaration).copied() {
                    Some(DeclarationModel::TypeAlias {
                        declaration: alias,
                        scope,
                    }) => {
                        let substitutions =
                            self.substitution(declaration, &alias.type_parameters, &arguments);
                        let alias_ty = self.resolve_type_node(
                            declaration.file,
                            scope,
                            &alias.ty,
                            &substitutions,
                        );
                        self.rest_type_id(alias_ty, active_aliases)
                    }
                    Some(DeclarationModel::Interface { .. } | DeclarationModel::Class { .. }) => {
                        RestTypeSyntax::Unknown
                    }
                    Some(
                        DeclarationModel::Variable { .. }
                        | DeclarationModel::Parameter { .. }
                        | DeclarationModel::Function { .. },
                    )
                    | None => RestTypeSyntax::Unknown,
                };
                active_aliases.remove(&declaration);
                result
            }
            TypeKind::Deferred(DeferredType::IndexedAccess { .. }) => {
                match self.force_type(ty, 0) {
                    Completion::Complete(resolved) if resolved != ty => {
                        self.rest_type_id(resolved, active_aliases)
                    }
                    Completion::Complete(_)
                    | Completion::Deferred
                    | Completion::Cycle
                    | Completion::Limit => RestTypeSyntax::Unknown,
                }
            }
            TypeKind::TypeParameter { .. } | TypeKind::Deferred(_) => RestTypeSyntax::Unknown,
            TypeKind::Error | TypeKind::Invalid(_) => RestTypeSyntax::ErrorCascade,
            TypeKind::Unknown
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
            | TypeKind::Object(_)
            | TypeKind::ClassInstance { .. }
            | TypeKind::ClassConstructor { .. }
            | TypeKind::Function(_)
            | TypeKind::ShapeFunction(_) => RestTypeSyntax::NonArray,
        }
    }

    fn validate_method_optional_overloads(&mut self, file: FileId, members: &[TypeMember]) {
        let bound = &self.program.files[file.0 as usize].bindings;
        let by_node = members
            .iter()
            .filter(|member| !member.recovered)
            .map(|member| (member.id, member))
            .collect::<HashMap<NodeId, &TypeMember>>();
        let mut checked = HashSet::<DeclId>::new();
        for member in members.iter().filter(|member| !member.recovered) {
            let TypeMemberKind::Method { .. } = member.kind else {
                continue;
            };
            let Some(canonical) = bound.canonical_type_member_declaration(member.id) else {
                continue;
            };
            if !checked.insert(canonical) {
                continue;
            }
            let Some(group) = bound.type_member_group(member.id) else {
                continue;
            };
            let methods = group
                .iter()
                .filter_map(|declaration| bound.declaration(*declaration))
                .filter_map(|declaration| by_node.get(&declaration.owner).copied())
                .filter_map(|candidate| match &candidate.kind {
                    TypeMemberKind::Method { name, optional, .. } => Some((name.span, *optional)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let Some((_, canonical_optional)) = methods.first().copied() else {
                continue;
            };
            if methods.len() > 1
                && methods
                    .iter()
                    .any(|(_, optional)| *optional != canonical_optional)
            {
                for (span, optional) in methods {
                    if optional != canonical_optional {
                        self.push_diagnostic(
                            file,
                            span,
                            "Overload signatures must all be optional or required.".to_string(),
                            2386,
                        );
                    }
                }
            }
        }
    }

    fn validate_index_signature(
        &mut self,
        file: FileId,
        scope: ScopeId,
        member: &TypeMember,
        parameters: &[Parameter],
        value_type: Option<&TypeNode>,
        type_parameters: &HashMap<String, TypeId>,
    ) {
        let [parameter] = parameters else {
            let span = parameters
                .first()
                .map_or(member.span, |parameter| parameter.name_span);
            self.push_diagnostic(
                file,
                span,
                "An index signature must have exactly one parameter.".to_string(),
                1096,
            );
            return;
        };
        if let Some(span) = parameter.rest_span {
            self.push_diagnostic(
                file,
                span,
                "An index signature cannot have a rest parameter.".to_string(),
                1017,
            );
            self.validate_rest_parameter_type(file, scope, parameter, type_parameters, false);
            return;
        }
        if !parameter.modifiers.is_empty() {
            if parameter.modifiers.iter().any(|modifier| {
                matches!(
                    modifier.kind,
                    ParameterModifier::Public
                        | ParameterModifier::Protected
                        | ParameterModifier::Private
                        | ParameterModifier::Readonly
                        | ParameterModifier::Override
                )
            }) {
                self.push_diagnostic(
                    file,
                    parameter.span,
                    "A parameter property is only allowed in a constructor implementation."
                        .to_string(),
                    2369,
                );
            }
            self.push_diagnostic(
                file,
                parameter.name_span,
                "An index signature parameter cannot have an accessibility modifier.".to_string(),
                1018,
            );
            return;
        }
        if let Some(span) = parameter.optional_span {
            self.push_diagnostic(
                file,
                span,
                "An index signature parameter cannot have a question mark.".to_string(),
                1019,
            );
            return;
        }
        if parameter.initializer.is_some() {
            self.push_diagnostic(
                file,
                parameter.name_span,
                "An index signature parameter cannot have an initializer.".to_string(),
                1020,
            );
            self.push_diagnostic(
                file,
                parameter.span,
                "A parameter initializer is only allowed in a function or constructor implementation."
                    .to_string(),
                2371,
            );
            return;
        }
        let Some(annotation) = &parameter.annotation else {
            self.push_diagnostic(
                file,
                parameter.name_span,
                "An index signature parameter must have a type annotation.".to_string(),
                1022,
            );
            return;
        };
        if matches!(
            annotation.kind,
            TypeNodeKind::Keyword(KeywordType::UniqueSymbol)
        ) {
            self.push_diagnostic(
                file,
                annotation.span,
                "'unique symbol' types are not allowed here.".to_string(),
                1335,
            );
            return;
        }
        let invalid = match self.index_key_syntax(
            file,
            scope,
            annotation,
            type_parameters,
            &mut HashSet::new(),
        ) {
            IndexKeySyntax::LiteralOrGeneric => Some((
                1337,
                "An index signature parameter type cannot be a literal type or generic type. Consider using a mapped object type instead.",
            )),
            IndexKeySyntax::Invalid => Some((
                1268,
                "An index signature parameter type must be 'string', 'number', 'symbol', or a template literal type.",
            )),
            IndexKeySyntax::Valid | IndexKeySyntax::Unknown => None,
        };
        if let Some((code, message)) = invalid {
            self.push_diagnostic(file, parameter.name_span, message.to_string(), code);
            return;
        }
        if value_type.is_none() {
            self.push_diagnostic(
                file,
                member.span,
                "An index signature must have a type annotation.".to_string(),
                1021,
            );
        }
    }

    fn index_key_syntax(
        &self,
        file: FileId,
        scope: ScopeId,
        node: &TypeNode,
        type_parameters: &HashMap<String, TypeId>,
        active_aliases: &mut HashSet<DeclId>,
    ) -> IndexKeySyntax {
        match &node.kind {
            TypeNodeKind::Keyword(
                KeywordType::String | KeywordType::Number | KeywordType::Symbol,
            ) => IndexKeySyntax::Valid,
            TypeNodeKind::Keyword(_)
            | TypeNodeKind::Function { .. }
            | TypeNodeKind::Constructor { .. }
            | TypeNodeKind::Object(_)
            | TypeNodeKind::Tuple(_)
            | TypeNodeKind::Array(_) => IndexKeySyntax::Invalid,
            TypeNodeKind::Literal(_) | TypeNodeKind::Infer { .. } => {
                IndexKeySyntax::LiteralOrGeneric
            }
            TypeNodeKind::Reference {
                name, arguments, ..
            } => {
                if type_parameters.contains_key(name) {
                    return IndexKeySyntax::LiteralOrGeneric;
                }
                let Some(declaration_id) = self.resolve_name(file, scope, name, Meaning::Type)
                else {
                    return IndexKeySyntax::Unknown;
                };
                match self.models.get(&declaration_id) {
                    Some(DeclarationModel::TypeAlias {
                        declaration: alias,
                        scope: alias_scope,
                    }) if alias.type_parameters.is_empty() && arguments.is_empty() => {
                        if !active_aliases.insert(declaration_id) {
                            return IndexKeySyntax::Unknown;
                        }
                        let result = self.index_key_syntax(
                            file,
                            *alias_scope,
                            &alias.ty,
                            type_parameters,
                            active_aliases,
                        );
                        active_aliases.remove(&declaration_id);
                        result
                    }
                    Some(DeclarationModel::Interface { .. } | DeclarationModel::Class { .. }) => {
                        IndexKeySyntax::Invalid
                    }
                    None if self
                        .program
                        .standard_library_declaration(declaration_id)
                        .is_some() =>
                    {
                        IndexKeySyntax::Invalid
                    }
                    Some(DeclarationModel::TypeAlias { .. })
                    | Some(DeclarationModel::Variable { .. })
                    | Some(DeclarationModel::Parameter { .. })
                    | Some(DeclarationModel::Function { .. })
                    | None => IndexKeySyntax::Unknown,
                }
            }
            TypeNodeKind::Union(members) => {
                members.iter().fold(IndexKeySyntax::Valid, |state, member| {
                    combine_index_key_syntax(
                        state,
                        self.index_key_syntax(file, scope, member, type_parameters, active_aliases),
                    )
                })
            }
            TypeNodeKind::Parenthesized(inner) => {
                self.index_key_syntax(file, scope, inner, type_parameters, active_aliases)
            }
            TypeNodeKind::Intersection(_)
            | TypeNodeKind::TypeQuery { .. }
            | TypeNodeKind::Predicate { .. }
            | TypeNodeKind::KeyOf(_)
            | TypeNodeKind::Readonly(_)
            | TypeNodeKind::Conditional { .. }
            | TypeNodeKind::Mapped { .. }
            | TypeNodeKind::IndexedAccess { .. }
            | TypeNodeKind::Missing => IndexKeySyntax::Unknown,
        }
    }
}

const fn combine_index_key_syntax(left: IndexKeySyntax, right: IndexKeySyntax) -> IndexKeySyntax {
    match (left, right) {
        (IndexKeySyntax::LiteralOrGeneric, _) | (_, IndexKeySyntax::LiteralOrGeneric) => {
            IndexKeySyntax::LiteralOrGeneric
        }
        (IndexKeySyntax::Invalid, _) | (_, IndexKeySyntax::Invalid) => IndexKeySyntax::Invalid,
        (IndexKeySyntax::Unknown, _) | (_, IndexKeySyntax::Unknown) => IndexKeySyntax::Unknown,
        (IndexKeySyntax::Valid, IndexKeySyntax::Valid) => IndexKeySyntax::Valid,
    }
}
