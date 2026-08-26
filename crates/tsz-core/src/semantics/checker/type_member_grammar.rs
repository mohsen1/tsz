use std::collections::{HashMap, HashSet};

use crate::bind::{Meaning, ScopeId};
use crate::semantics::relation::RelationContext;
use crate::semantics::types::{Completion, DeferredType, TypeId, TypeKind};
use crate::source::{DeclId, FileId, NodeId};
use crate::syntax::{
    ExpressionKind, KeywordType, Parameter, TypeMember, TypeMemberKind, TypeNode, TypeNodeKind,
};

use super::{Checker, DeclarationModel};

macro_rules! d {
    ($checker:expr, $file:expr, $span:expr, $code:expr) => {
        $checker.push_diagnostic($file, $span, grammar_message($code).into(), $code)
    };
    ($checker:expr, $file:expr, $span:expr, $message:expr, $code:expr) => {
        $checker.push_diagnostic($file, $span, ($message).into(), $code)
    };
}

/// Ordered by the diagnostic precedence used for authored unions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum IndexKeySyntax {
    Valid,
    Unknown,
    Invalid,
    LiteralOrGeneric,
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
        d!(self, file, span, 7061);
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
                    let message = format!("Member '{name_text}' implicitly has an 'any' type.");
                    d!(self, file, name.span, message, 7008);
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
                    let message = format!(
                        "'{name_text}', which lacks return-type annotation, implicitly has an 'any' return type."
                    );
                    d!(self, file, member.span, message, 7010);
                } else {
                    let _ = self.require_completion(Completion::<()>::Deferred);
                }
            }
            TypeMemberKind::Call {
                return_type: None, ..
            } => d!(self, file, member.span, 7020),
            TypeMemberKind::Construct {
                return_type: None, ..
            } => d!(self, file, member.span, 7013),
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
            let (code, kind, ty) = if parameter.rest {
                (7019, "Rest parameter", "any[]")
            } else {
                (7006, "Parameter", "any")
            };
            let message = format!("{kind} '{}' implicitly has an '{ty}' type.", parameter.name);
            let span = if parameter.rest {
                parameter
                    .rest_span
                    .map_or(parameter.name_span, |rest| rest.merge(parameter.name_span))
            } else {
                parameter.name_span
            };
            d!(self, file, span, message, code);
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
        for parameter in parameters
            .iter()
            .filter(|parameter| parameter.implementation_name_is_recovery_free())
        {
            *occurrences.entry(parameter.name.as_str()).or_default() += 1;
        }
        for parameter in parameters.iter().filter(|parameter| {
            parameter.implementation_name_is_recovery_free()
                && occurrences[parameter.name.as_str()] > 1
        }) {
            let message = format!("Duplicate identifier '{}'.", parameter.name);
            d!(self, file, parameter.name_span, message, 2300);
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
                d!(self, file, parameter.name_span, 1015);
            }
            if parameter.rest && parameter.initializer.is_some() {
                d!(self, file, parameter.name_span, 1048);
            }
            let has_property_modifier = parameter.is_property();
            if has_property_modifier
                && !matches!(
                    host,
                    ParameterGrammarHost::Implementation { constructor: true }
                )
            {
                d!(self, file, parameter.span, 2369);
            } else if has_property_modifier {
                // Parameter properties also synthesize instance members and
                // runtime assignments. Until the class-shape owner consumes
                // them, the checker must not cache a property-free instance.
                let _ = self.require_completion(Completion::<()>::Deferred);
            }
            if parameter
                .modifiers
                .iter()
                .any(|modifier| !modifier.kind.is_property())
            {
                let _ = self.require_completion(Completion::<()>::Deferred);
            }
            if parameter.initializer.is_some() && matches!(host, ParameterGrammarHost::Signature) {
                d!(self, file, parameter.span, 2371);
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
            let ty = self.resolve_type_node(file, scope, annotation, type_parameters);
            self.rest_type_id(ty, &mut HashSet::new())
        });
        let optional_breaks_array = parameter.optional
            && self.options.effective_strict_null_checks()
            && !matches!(
                declared_array_like,
                Some(RestTypeSyntax::Any | RestTypeSyntax::ErrorCascade)
            );
        if matches!(declared_array_like, Some(RestTypeSyntax::NonArray)) || optional_breaks_array {
            d!(self, file, parameter.span, 2370);
        } else if matches!(declared_array_like, Some(RestTypeSyntax::Unknown)) {
            let _ = self.require_completion(Completion::<()>::Deferred);
        }
        if report_optional && let Some(optional_span) = parameter.optional_span {
            d!(self, file, optional_span, 1047);
        }
    }

    fn begin_alias_walk(
        &mut self,
        declaration: DeclId,
        active_aliases: &mut HashSet<DeclId>,
    ) -> Option<bool> {
        if !self.semantic_declaration_is_claimed(declaration) {
            let _ = self.require_completion(Completion::<()>::Deferred);
            return None;
        }
        let is_alias = matches!(
            self.models.get(&declaration),
            Some(DeclarationModel::TypeAlias { .. })
        );
        if is_alias && !active_aliases.insert(declaration) {
            return None;
        }
        Some(is_alias)
    }

    fn rest_type_id(&mut self, ty: TypeId, active_aliases: &mut HashSet<DeclId>) -> RestTypeSyntax {
        match self.store.kind(ty).clone() {
            TypeKind::Any => RestTypeSyntax::Any,
            TypeKind::Never => RestTypeSyntax::Bottom,
            TypeKind::Array(_) | TypeKind::Tuple(_) => RestTypeSyntax::ArrayLike,
            TypeKind::Union(members) => combine_rest_types(
                members,
                RestTypeSyntax::NonArray,
                RestTypeSyntax::ArrayLike,
                |member| self.rest_type_id(member, active_aliases),
            ),
            TypeKind::Intersection(members) => combine_rest_types(
                members,
                RestTypeSyntax::ArrayLike,
                RestTypeSyntax::NonArray,
                |member| self.rest_type_id(member, active_aliases),
            ),
            TypeKind::Deferred(DeferredType::Reference {
                declaration,
                arguments,
            }) => {
                let Some(remove_alias) = self.begin_alias_walk(declaration, active_aliases) else {
                    return RestTypeSyntax::Unknown;
                };
                let result = if self
                    .program
                    .standard_library
                    .is_rest_array_type(declaration)
                    && arguments.len() == 1
                {
                    RestTypeSyntax::ArrayLike
                } else {
                    match self.models.get(&declaration).copied() {
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
                        _ => RestTypeSyntax::Unknown,
                    }
                };
                if remove_alias {
                    active_aliases.remove(&declaration);
                }
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
            | TypeKind::LibraryReference { .. }
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
        for member in members.iter().filter(|member| !member.recovered) {
            let TypeMemberKind::Method { .. } = member.kind else {
                continue;
            };
            let Some(group) = bound.type_member_group(member.id) else {
                continue;
            };
            let methods = group
                .iter()
                .filter_map(|declaration| bound.declaration(*declaration))
                .filter_map(|declaration| by_node.get(&declaration.owner).copied())
                .filter_map(|candidate| match &candidate.kind {
                    TypeMemberKind::Method { name, optional, .. } => {
                        Some((candidate.id, name.span, *optional))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let Some((first, _, canonical_optional)) = methods.first().copied() else {
                continue;
            };
            if first != member.id {
                continue;
            }
            for (_, span, optional) in methods {
                if optional != canonical_optional {
                    d!(self, file, span, 2386);
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
            d!(self, file, span, 1096);
            return;
        };
        if let Some(span) = parameter.rest_span {
            d!(self, file, span, 1017);
            self.validate_rest_parameter_type(file, scope, parameter, type_parameters, false);
            return;
        }
        if !parameter.modifiers.is_empty() {
            if parameter.is_property() {
                d!(self, file, parameter.span, 2369);
            }
            d!(self, file, parameter.name_span, 1018);
            return;
        }
        if let Some(span) = parameter.optional_span {
            d!(self, file, span, 1019);
            return;
        }
        if parameter.initializer.is_some() {
            d!(self, file, parameter.name_span, 1020);
            d!(self, file, parameter.span, 2371);
            return;
        }
        let Some(annotation) = &parameter.annotation else {
            d!(self, file, parameter.name_span, 1022);
            return;
        };
        if matches!(
            annotation.kind,
            TypeNodeKind::Keyword(KeywordType::UniqueSymbol)
        ) {
            d!(self, file, annotation.span, 1335);
            return;
        }
        let invalid = match self.index_key_syntax(
            file,
            scope,
            annotation,
            type_parameters,
            &mut HashSet::new(),
        ) {
            IndexKeySyntax::LiteralOrGeneric => Some(1337),
            IndexKeySyntax::Invalid => Some(1268),
            IndexKeySyntax::Valid | IndexKeySyntax::Unknown => None,
        };
        if let Some(code) = invalid {
            d!(self, file, parameter.name_span, code);
            return;
        }
        if value_type.is_none() {
            d!(self, file, member.span, 1021);
        }
    }

    fn index_key_syntax(
        &mut self,
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
            | TypeNodeKind::Array(_)
            | TypeNodeKind::This => IndexKeySyntax::Invalid,
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
                let Some(remove_alias) = self.begin_alias_walk(declaration_id, active_aliases)
                else {
                    return IndexKeySyntax::Unknown;
                };
                let result = match self.models.get(&declaration_id).copied() {
                    Some(DeclarationModel::TypeAlias {
                        declaration: alias,
                        scope: alias_scope,
                    }) if alias.type_parameters.is_empty() && arguments.is_empty() => self
                        .index_key_syntax(
                            file,
                            alias_scope,
                            &alias.ty,
                            type_parameters,
                            active_aliases,
                        ),
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
                    _ => IndexKeySyntax::Unknown,
                };
                if remove_alias {
                    active_aliases.remove(&declaration_id);
                }
                result
            }
            TypeNodeKind::Union(members) => {
                members.iter().fold(IndexKeySyntax::Valid, |state, member| {
                    state.max(self.index_key_syntax(
                        file,
                        scope,
                        member,
                        type_parameters,
                        active_aliases,
                    ))
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

fn combine_rest_types(
    members: Vec<TypeId>,
    decisive: RestTypeSyntax,
    mut result: RestTypeSyntax,
    mut classify: impl FnMut(TypeId) -> RestTypeSyntax,
) -> RestTypeSyntax {
    for member in members {
        match classify(member) {
            value if value == decisive => return decisive,
            RestTypeSyntax::Any | RestTypeSyntax::Bottom | RestTypeSyntax::ErrorCascade
                if decisive == RestTypeSyntax::ArrayLike =>
            {
                return decisive;
            }
            RestTypeSyntax::Unknown => result = RestTypeSyntax::Unknown,
            _ => {}
        }
    }
    result
}

const fn grammar_message(code: u32) -> &'static str {
    match code {
        1015 => "Parameter cannot have question mark and initializer.",
        1017 => "An index signature cannot have a rest parameter.",
        1018 => "An index signature parameter cannot have an accessibility modifier.",
        1019 => "An index signature parameter cannot have a question mark.",
        1020 => "An index signature parameter cannot have an initializer.",
        1021 => "An index signature must have a type annotation.",
        1022 => "An index signature parameter must have a type annotation.",
        1047 => "A rest parameter cannot be optional.",
        1048 => "A rest parameter cannot have an initializer.",
        1096 => "An index signature must have exactly one parameter.",
        1268 => {
            "An index signature parameter type must be 'string', 'number', 'symbol', or a template literal type."
        }
        1335 => "'unique symbol' types are not allowed here.",
        1337 => {
            "An index signature parameter type cannot be a literal type or generic type. Consider using a mapped object type instead."
        }
        2369 => "A parameter property is only allowed in a constructor implementation.",
        2370 => "A rest parameter must be of an array type.",
        2371 => {
            "A parameter initializer is only allowed in a function or constructor implementation."
        }
        2386 => "Overload signatures must all be optional or required.",
        7013 => {
            "Construct signature, which lacks return-type annotation, implicitly has an 'any' return type."
        }
        7020 => {
            "Call signature, which lacks return-type annotation, implicitly has an 'any' return type."
        }
        7061 => "A mapped type may not declare properties or methods.",
        _ => unreachable!(),
    }
}
