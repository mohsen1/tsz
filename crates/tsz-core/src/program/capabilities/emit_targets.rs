use crate::bind::ClassMemberSymbol;
use crate::syntax::{
    AccessorKind, ClassDeclaration, ClassMember, ClassMemberKind, DescendantContainer, Expression,
    ExpressionKind, ExpressionRoot, ExpressionTraversal, FunctionLikeBody, Parameter,
    ParameterModifier, ParameterNameKind, PropertyNameKind, Statement, StatementKind, TypeMember,
    TypeMemberKind, TypeNode, TypeParameterDeclaration as TypeParam, contains_matching_expression,
    for_each_statement_in,
};

use super::{
    CapabilityNonclaim, CapabilityScope, CapabilityTarget, CompilerOptions, ProgramFile,
    SemanticGap, SyntaxGap, add_javascript, add_semantic,
};

pub(super) fn add_nonclaims(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    file: &ProgramFile,
    options: &CompilerOptions,
) {
    let id = file.source.id;
    let async_transform = target_requires_async_transform(&options.target);
    let class_field_transform = target_requires_class_property_transform(&options.target);
    let target_preserves_class_fields = !class_field_transform;
    let class_field_semantics_mismatch = options
        .use_define_for_class_fields
        .is_some_and(|value| value != target_preserves_class_fields);
    if async_transform || class_field_transform || class_field_semantics_mismatch {
        for_each_statement_in(
            &file.syntax.statements,
            &mut |statement| match &statement.kind {
                StatementKind::Function(function)
                    if async_transform && function.is_async && function.has_body =>
                {
                    add_javascript(
                        nonclaims,
                        CapabilityScope::node(id, statement.id),
                        SyntaxGap::AsyncFunctionTransform,
                    );
                }
                StatementKind::Class(class) if !class.declared => {
                    for member in &class.members {
                        let runtime_member = class_member_is_emitted(member);
                        if async_transform
                            && member.modifiers.async_member
                            && matches!(member.kind, ClassMemberKind::Method { has_body: true, .. })
                        {
                            add_javascript(
                                nonclaims,
                                CapabilityScope::node(id, member.id),
                                SyntaxGap::AsyncFunctionTransform,
                            );
                        }
                        if class_field_transform
                            && runtime_member
                            && matches!(member.name_kind, PropertyNameKind::PrivateIdentifier)
                        {
                            add_javascript(
                                nonclaims,
                                CapabilityScope::node(id, member.id),
                                SyntaxGap::PrivateIdentifierTransform,
                            );
                        }
                        let mismatched_field = matches!(
                            &member.kind,
                            ClassMemberKind::Property { initializer, .. }
                                if class_field_semantics_mismatch
                                    && (member.name_kind != PropertyNameKind::PrivateIdentifier
                                        || !member.modifiers.static_member
                                            && initializer.is_some())
                        );
                        if runtime_member
                            && (class_field_transform
                                && matches!(member.kind, ClassMemberKind::Property { .. })
                                || mismatched_field)
                        {
                            add_javascript(
                                nonclaims,
                                CapabilityScope::node(id, member.id),
                                SyntaxGap::ClassFieldTransform,
                            );
                        }
                        if class_field_semantics_mismatch
                            && runtime_member
                            && matches!(
                                &member.kind,
                                ClassMemberKind::Constructor { parameters, .. }
                                    if parameters.iter().any(Parameter::is_property)
                            )
                        {
                            add_javascript(
                                nonclaims,
                                CapabilityScope::node(id, member.id),
                                SyntaxGap::ClassFieldTransform,
                            );
                        }
                    }
                }
                _ => {}
            },
        );
    }

    add_accessor_summary_nonclaims(nonclaims, file, options);
}

fn add_accessor_summary_nonclaims(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    file: &ProgramFile,
    options: &CompilerOptions,
) {
    let id = file.source.id;
    let accessor_gap = SemanticGap::DeclarationAccessorSummary;
    for_each_statement_in(&file.syntax.statements, &mut |statement| {
        if let StatementKind::Class(class) = &statement.kind {
            if AccessorRequirement::All.in_class_header(class) {
                add_node_gap(nonclaims, id, statement.id, accessor_gap);
            }
            for member in &class.members {
                if class_member_needs_semantic_summary(file, member) {
                    add_node_gap(nonclaims, id, member.id, SemanticGap::ClassMemberSemantics);
                }
                if member_needs_diagnostic_summary(class, member, options)
                    || class_accessor_pair_needs_semantic_summary(file, class, member)
                {
                    add_node_gap(nonclaims, id, member.id, accessor_gap);
                }
            }
        } else if statement_needs_diagnostic_summary(statement) {
            add_node_gap(nonclaims, id, statement.id, accessor_gap);
        }
    });

    for statement in &file.syntax.statements {
        match &statement.kind {
            StatementKind::Class(class) => {
                if AccessorRequirement::Inferred.in_class_header(class) {
                    add_declaration_accessor_summary(nonclaims, id, statement.id);
                }
                for member in &class.members {
                    if member_needs_declaration_summary(member) {
                        add_declaration_accessor_summary(nonclaims, id, member.id);
                    }
                }
            }
            _ if AccessorRequirement::Inferred.in_statement(statement)
                || statement_result_needs_declaration_summary(statement) =>
            {
                add_declaration_accessor_summary(nonclaims, id, statement.id);
            }
            _ => {}
        }
    }
}

fn add_node_gap(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    file: crate::source::FileId,
    owner: crate::source::NodeId,
    gap: SemanticGap,
) {
    add_semantic(
        nonclaims,
        &[
            CapabilityTarget::SemanticCheck,
            CapabilityTarget::RequiredType,
            CapabilityTarget::SemanticDiagnostics,
        ],
        CapabilityScope::node(file, owner),
        gap,
    );
}

fn class_member_needs_semantic_summary(file: &ProgramFile, member: &ClassMember) -> bool {
    matches!(
        file.bindings.class_member_symbol(member.id),
        Some(
            ClassMemberSymbol::ReservedPrivateConstructor
                | ClassMemberSymbol::ReservedStaticPrototype
        )
    ) || matches!(&member.kind,
        ClassMemberKind::Constructor { type_parameters, return_type, .. }
            if !type_parameters.is_empty()
                || return_type.is_some()
                || member.modifiers.static_member
                || member.modifiers.readonly
                || member.modifiers.abstract_member
                || member.modifiers.declared
                || member.modifiers.async_member
                || member.modifiers.unsupported_for_overload_completion)
}

fn class_accessor_pair_needs_semantic_summary(
    file: &ProgramFile,
    class: &ClassDeclaration,
    member: &ClassMember,
) -> bool {
    let Some(group) = file.bindings.class_member_group(member.id) else {
        return false;
    };
    let mut pair = [None, None];
    for candidate in &class.members {
        if file.bindings.class_member_group(candidate.id) != Some(group) {
            continue;
        }
        let index = match &candidate.kind {
            ClassMemberKind::Method {
                accessor: Some(AccessorKind::Get),
                ..
            } => 0,
            ClassMemberKind::Method {
                accessor: Some(AccessorKind::Set),
                ..
            } => 1,
            _ => continue,
        };
        pair[index] = Some(candidate);
    }
    let [Some(getter), Some(setter)] = pair else {
        return false;
    };
    getter.modifiers.abstract_member != setter.modifiers.abstract_member
        || class_member_accessibility(getter) < class_member_accessibility(setter)
}

fn class_member_accessibility(member: &ClassMember) -> u8 {
    if member.modifiers.private || member.name_kind == PropertyNameKind::PrivateIdentifier {
        0
    } else if member.modifiers.protected {
        1
    } else {
        2
    }
}

fn add_declaration_accessor_summary(
    nonclaims: &mut Vec<CapabilityNonclaim>,
    file: crate::source::FileId,
    owner: crate::source::NodeId,
) {
    add_semantic(
        nonclaims,
        &[CapabilityTarget::Declaration],
        CapabilityScope::node(file, owner),
        SemanticGap::DeclarationAccessorSummary,
    );
}

pub(super) fn target_requires_class_property_transform(target: &str) -> bool {
    [
        "es3", "es5", "es6", "es2015", "es2016", "es2017", "es2018", "es2019", "es2020", "es2021",
    ]
    .iter()
    .any(|candidate| target.trim().eq_ignore_ascii_case(candidate))
}

fn target_requires_async_transform(target: &str) -> bool {
    ["es3", "es5", "es6", "es2015", "es2016"]
        .iter()
        .any(|candidate| target.trim().eq_ignore_ascii_case(candidate))
}

const fn class_member_is_emitted(member: &ClassMember) -> bool {
    !member.modifiers.declared
        && !member.modifiers.abstract_member
        && !matches!(
            member.kind,
            ClassMemberKind::Constructor {
                has_body: false,
                ..
            } | ClassMemberKind::Method {
                has_body: false,
                ..
            }
        )
}

pub(super) const fn class_member_declaration_type_is_erased(member: &ClassMember) -> bool {
    member.modifiers.private || matches!(member.name_kind, PropertyNameKind::PrivateIdentifier)
}

pub(super) fn class_parameter_property_type_is_published(parameter: &Parameter) -> bool {
    parameter.is_property()
        && !parameter
            .modifiers
            .iter()
            .any(|modifier| modifier.kind == ParameterModifier::Private)
}

fn member_needs_diagnostic_summary(
    class: &ClassDeclaration,
    member: &ClassMember,
    options: &CompilerOptions,
) -> bool {
    if accessor_body_semantics_are_modeled(class, member, options) {
        return false;
    }
    AccessorRequirement::All.in_class_member(member)
        || accessor_body_needs_semantic_summary(member)
        || match &member.kind {
            ClassMemberKind::Constructor { parameters, .. }
            | ClassMemberKind::Method { parameters, .. } => parameters.iter().any(|parameter| {
                expression_needs_diagnostic_summary(parameter.initializer.as_ref())
            }),
            ClassMemberKind::Property { initializer, .. } => {
                expression_needs_diagnostic_summary(initializer.as_ref())
            }
        }
}

fn statement_needs_diagnostic_summary(statement: &Statement) -> bool {
    AccessorRequirement::All.in_statement(statement)
        || contains_matching_expression(
            ExpressionRoot::Statement(statement),
            ExpressionTraversal::All,
            |expression| AccessorRequirement::All.owned_by_expression(expression),
        )
}

fn member_needs_declaration_summary(member: &ClassMember) -> bool {
    let parameter_contains = |parameter: &Parameter| match &parameter.annotation {
        Some(annotation) => AccessorRequirement::Inferred.in_type(annotation),
        None => parameter
            .initializer
            .as_ref()
            .is_some_and(expression_result_needs_summary),
    };
    if class_member_declaration_type_is_erased(member) {
        return match &member.kind {
            ClassMemberKind::Constructor { parameters, .. } => parameters
                .iter()
                .filter(|parameter| class_parameter_property_type_is_published(parameter))
                .any(parameter_contains),
            ClassMemberKind::Property { .. } | ClassMemberKind::Method { .. } => false,
        };
    }
    AccessorRequirement::Inferred.in_class_member(member)
        || match &member.kind {
            ClassMemberKind::Property {
                annotation,
                initializer,
                ..
            } => {
                annotation.is_none()
                    && initializer
                        .as_ref()
                        .is_some_and(expression_result_needs_summary)
            }
            ClassMemberKind::Constructor { parameters, .. }
            | ClassMemberKind::Method { parameters, .. } => {
                parameters.iter().any(parameter_contains)
            }
        }
}

fn statement_result_needs_declaration_summary(statement: &Statement) -> bool {
    let expression_contains = published_expression_needs_summary;
    let parameter_contains = |parameter: &Parameter| {
        parameter.annotation.is_none() && expression_contains(parameter.initializer.as_ref())
    };
    match &statement.kind {
        StatementKind::Variable(statement) => statement.declarators.iter().any(|declarator| {
            declarator.annotation.is_none() && expression_contains(declarator.initializer.as_ref())
        }),
        StatementKind::Function(declaration) => {
            declaration.parameters.iter().any(parameter_contains)
        }
        StatementKind::Export(declaration) => expression_contains(declaration.assignment.as_ref()),
        _ => false,
    }
}

fn expression_needs_diagnostic_summary(expression: Option<&Expression>) -> bool {
    expression.is_some_and(|expression| {
        contains_matching_expression(
            ExpressionRoot::Expression(expression),
            ExpressionTraversal::All,
            |candidate| AccessorRequirement::All.owned_by_expression(candidate),
        )
    })
}

fn published_expression_needs_summary(expression: Option<&Expression>) -> bool {
    expression.is_some_and(expression_result_needs_summary)
}

fn expression_result_needs_summary(expression: &Expression) -> bool {
    contains_matching_expression(
        ExpressionRoot::Expression(expression),
        ExpressionTraversal::Executed,
        |candidate| {
            AccessorRequirement::All.owned_by_expression(candidate)
                || matches!(&candidate.kind, ExpressionKind::FunctionLike(function)
                    if function_result_needs_summary(function))
        },
    )
}

fn function_result_needs_summary(function: &crate::syntax::FunctionLikeExpression) -> bool {
    AccessorRequirement::Inferred.in_signature(
        &function.type_parameters,
        &function.parameters,
        function.return_type.as_ref(),
    ) || function.parameters.iter().any(|parameter| {
        parameter.annotation.is_none()
            && published_expression_needs_summary(parameter.initializer.as_ref())
    }) || function.return_type.is_none()
        && match function.syntax.body() {
            FunctionLikeBody::Expression(body) => expression_result_needs_summary(body),
            FunctionLikeBody::Statements(body) => {
                body.iter().any(statement_return_contains_summary)
            }
        }
}

fn statement_return_contains_summary(statement: &Statement) -> bool {
    let mut found = false;
    statement.for_each_statement_where(
        &mut |container| {
            !matches!(
                container,
                DescendantContainer::Function(_, _)
                    | DescendantContainer::Class(_, _)
                    | DescendantContainer::FunctionLike(_, _)
            )
        },
        &mut |statement| {
            if let StatementKind::Return(Some(expression)) = &statement.kind {
                found |= expression_result_needs_summary(expression);
            }
        },
    );
    found
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AccessorRequirement {
    Inferred,
    All,
}

fn type_parameter_types(parameters: &[TypeParam]) -> impl Iterator<Item = &TypeNode> {
    parameters
        .iter()
        .flat_map(|parameter| parameter.constraint.iter().chain(parameter.default.iter()))
}

impl AccessorRequirement {
    fn in_type(self, node: &TypeNode) -> bool {
        node.contains_matching_type_member(&mut |member| self.direct_type_member(member))
    }

    fn in_signature(
        self,
        type_parameters: &[TypeParam],
        parameters: &[Parameter],
        return_type: Option<&TypeNode>,
    ) -> bool {
        type_parameter_types(type_parameters)
            .chain(
                parameters
                    .iter()
                    .filter_map(|parameter| parameter.annotation.as_ref()),
            )
            .chain(return_type)
            .any(|node| self.in_type(node))
    }

    fn in_class_member(self, member: &ClassMember) -> bool {
        if matches!(&member.kind,
            ClassMemberKind::Method {
                accessor: Some(accessor),
                parameters,
                return_type,
                ..
            } if self.matches(*accessor, parameters, return_type.as_ref()))
        {
            return true;
        }
        match &member.kind {
            ClassMemberKind::Constructor { parameters, .. } => parameters
                .iter()
                .filter_map(|parameter| parameter.annotation.as_ref())
                .any(|node| self.in_type(node)),
            ClassMemberKind::Property { annotation, .. } => {
                annotation.as_ref().is_some_and(|node| self.in_type(node))
            }
            ClassMemberKind::Method {
                type_parameters,
                parameters,
                return_type,
                ..
            } => self.in_signature(type_parameters, parameters, return_type.as_ref()),
        }
    }

    fn in_statement(self, statement: &Statement) -> bool {
        match &statement.kind {
            StatementKind::Variable(statement) => statement
                .declarators
                .iter()
                .filter_map(|declarator| declarator.annotation.as_ref())
                .any(|node| self.in_type(node)),
            StatementKind::Function(declaration) => self.in_signature(
                &declaration.type_parameters,
                &declaration.parameters,
                declaration.return_type.as_ref(),
            ),
            StatementKind::Class(class) => {
                self.in_class_header(class)
                    || class
                        .members
                        .iter()
                        .any(|member| self.in_class_member(member))
            }
            StatementKind::TypeAlias(declaration) => {
                type_parameter_types(&declaration.type_parameters).any(|node| self.in_type(node))
                    || self.in_type(&declaration.ty)
            }
            StatementKind::Interface(declaration) => {
                type_parameter_types(&declaration.type_parameters).any(|node| self.in_type(node))
                    || declaration.extends.iter().any(|node| self.in_type(node))
                    || declaration
                        .members
                        .iter()
                        .any(|member| self.in_type_member(member))
            }
            _ => false,
        }
    }

    fn in_type_member(self, member: &TypeMember) -> bool {
        if self.direct_type_member(member) {
            return true;
        }
        match &member.kind {
            TypeMemberKind::Property { ty, .. } => {
                ty.as_ref().is_some_and(|node| self.in_type(node))
            }
            _ => member.kind.signature().is_some_and(
                |(_, type_parameters, parameters, return_type)| {
                    self.in_signature(type_parameters, parameters, return_type)
                },
            ),
        }
    }

    fn in_class_header(self, class: &ClassDeclaration) -> bool {
        type_parameter_types(&class.type_parameters).any(|node| self.in_type(node))
            || class
                .extends
                .as_ref()
                .is_some_and(|node| self.in_type(node))
            || class.implements.iter().any(|node| self.in_type(node))
    }

    fn owned_by_expression(self, expression: &Expression) -> bool {
        match &expression.kind {
            ExpressionKind::Call { type_arguments, .. } => type_arguments
                .iter()
                .flatten()
                .any(|node| self.in_type(node)),
            ExpressionKind::New { type_arguments, .. } => {
                type_arguments.iter().any(|node| self.in_type(node))
            }
            ExpressionKind::FunctionLike(function) => self.in_signature(
                &function.type_parameters,
                &function.parameters,
                function.return_type.as_ref(),
            ),
            ExpressionKind::As { ty, .. } => self.in_type(ty),
            _ => false,
        }
    }

    fn direct_type_member(self, member: &TypeMember) -> bool {
        matches!(&member.kind,
            TypeMemberKind::Accessor { accessor, parameters, return_type, .. }
                if self.matches(*accessor, parameters, return_type.as_ref()))
    }

    fn matches(
        self,
        accessor: AccessorKind,
        parameters: &[Parameter],
        return_type: Option<&TypeNode>,
    ) -> bool {
        let mut values = parameters
            .iter()
            .filter(|parameter| parameter.name_kind != ParameterNameKind::This);
        let value = values.next();
        let inferred = match accessor {
            AccessorKind::Get => return_type.is_none(),
            AccessorKind::Set => value.is_none_or(|parameter| parameter.annotation.is_none()),
        };
        let grammar = match accessor {
            AccessorKind::Get => value.is_some(),
            AccessorKind::Set => {
                return_type.is_some()
                    || value.is_none()
                    || values.next().is_some()
                    || value.is_some_and(|parameter| {
                        parameter.optional
                            || parameter.rest
                            || parameter.initializer.is_some()
                            || !parameter.modifiers.is_empty()
                    })
            }
        };
        inferred || self == Self::All && grammar
    }
}

const fn accessor_body_needs_semantic_summary(member: &ClassMember) -> bool {
    matches!(
        member.kind,
        ClassMemberKind::Method {
            accessor: Some(_),
            has_body: true,
            ..
        }
    )
}

/// The checker owns this deliberately small straight-line accessor body
/// family. More complex control flow, inferred strict parameters, and any
/// signature containing an accessor type remain behind the typed summary
/// nonclaim until their semantic owners graduate.
fn accessor_body_semantics_are_modeled(
    class: &ClassDeclaration,
    member: &ClassMember,
    options: &CompilerOptions,
) -> bool {
    if class.declared
        || class.abstract_class
        || !class.type_parameters.is_empty()
        || class.extends.is_some()
        || !class.implements.is_empty()
        || !class.member_syntax_recovery_free
        || !(member.overload_context_is_recovery_free()
            || matches!(
                member.name_kind,
                PropertyNameKind::StringLiteral | PropertyNameKind::NumericLiteral
            ) && member.emit_products_supported)
        || !member.modifiers.method_modifiers_are_modeled()
        || member.modifiers.async_member
        || matches!(
            member.name_kind,
            PropertyNameKind::Computed
                | PropertyNameKind::PrivateIdentifier
                | PropertyNameKind::Unsupported
        )
    {
        return false;
    }
    let ClassMemberKind::Method {
        type_parameters,
        parameters,
        return_type,
        body,
        has_body: true,
        accessor: Some(accessor),
        ..
    } = &member.kind
    else {
        return false;
    };
    if !type_parameters.is_empty()
        || return_type.as_ref().is_some_and(|node| {
            node.contains_type_query() || AccessorRequirement::All.in_type(node)
        })
        || parameters.iter().any(|parameter| {
            parameter.annotation.as_ref().is_some_and(|node| {
                node.contains_type_query() || AccessorRequirement::All.in_type(node)
            })
        })
    {
        return false;
    }
    match accessor {
        AccessorKind::Get => parameters.is_empty() && bounded_getter_body(body),
        AccessorKind::Set => {
            return_type.is_none()
                && body.is_empty()
                && matches!(parameters.as_slice(), [parameter]
                    if parameter.name_kind == ParameterNameKind::Binding
                        && !parameter.optional
                        && !parameter.rest
                        && parameter.initializer.is_none()
                        && parameter.modifiers.is_empty()
                        && parameter.overload_context_is_recovery_free()
                        && (parameter.annotation.is_some()
                            || !options.effective_no_implicit_any()))
        }
    }
}

fn bounded_getter_body(body: &[Statement]) -> bool {
    let Some((last, prefix)) = body.split_last() else {
        return false;
    };
    matches!(&last.kind, StatementKind::Return(Some(expression)) if matches!(expression.kind, ExpressionKind::Literal(_)))
        && prefix.iter().all(|statement| {
            matches!(&statement.kind,
                StatementKind::Variable(statement)
                    if statement.declarators.iter().all(|declarator| {
                        declarator.annotation.is_none()
                            && declarator.initializer.as_ref().is_some_and(|initializer| {
                                matches!(initializer.kind, ExpressionKind::Literal(_) | ExpressionKind::This)
                            })
                    }))
        })
}
