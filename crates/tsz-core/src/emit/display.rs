use crate::program::{CompilerOptions, ProgramFile};
use crate::syntax::{
    AuthoredTypeEdge, AuthoredTypeItem, KeywordType, Literal, NumberLiteral, Parameter,
    StringLiteral, TypeMemberKind, TypeMemberModifier, TypeMemberNameKind, TypeNode, TypeNodeKind,
};

use super::{Printer, TYPE_PREC_LOWEST};

#[derive(Debug, Clone)]
pub(crate) struct RenderedType {
    pub text: String,
    pub part_kind: &'static str,
}

#[derive(Debug, Clone)]
pub(crate) struct RenderedParameter {
    pub text: String,
    pub name: String,
    pub rest: bool,
    pub optional: bool,
    pub ty: RenderedType,
}

#[derive(Debug, Clone)]
pub(crate) struct RenderedParameters {
    pub text: String,
    pub parameters: Vec<RenderedParameter>,
}

pub(crate) fn render_authored_type(
    file: &ProgramFile,
    options: &CompilerOptions,
    ty: &TypeNode,
) -> Option<RenderedType> {
    if !authored_type_display_is_supported(ty) {
        return None;
    }
    let mut printer = Printer::new(&file.source, &file.bindings, options);
    printer.emitting_declaration = true;
    printer.compact_type = true;
    printer.write_type(ty, TYPE_PREC_LOWEST);
    printer.declaration_supported.then_some(RenderedType {
        text: printer.output,
        part_kind: authored_type_part_kind(ty),
    })
}

fn authored_type_display_is_supported(root: &TypeNode) -> bool {
    let mut pending = vec![AuthoredTypeItem::Type(root, AuthoredTypeEdge::Nested)];
    while let Some(item) = pending.pop() {
        match item {
            AuthoredTypeItem::Type(node, _) => {
                match &node.kind {
                    TypeNodeKind::Missing
                    | TypeNodeKind::Predicate {
                        asserts: false,
                        ty: None,
                        ..
                    }
                    | TypeNodeKind::Literal(
                        Literal::String(StringLiteral::Extended(_))
                        | Literal::NoSubstitutionTemplate(_)
                        | Literal::Number(NumberLiteral::Separated(_) | NumberLiteral::Recovery(_)),
                    ) => return false,
                    TypeNodeKind::Object(members) => {
                        let mut names = Vec::with_capacity(members.len());
                        for member in members {
                            let name = match &member.kind {
                                TypeMemberKind::Property { name, .. }
                                | TypeMemberKind::Method { name, .. } => name,
                                TypeMemberKind::Accessor { .. }
                                | TypeMemberKind::Call { .. }
                                | TypeMemberKind::Construct { .. }
                                | TypeMemberKind::Index { .. } => return false,
                            };
                            let TypeMemberNameKind::Identifier(name) = &name.kind else {
                                return false;
                            };
                            if names.contains(name) {
                                return false;
                            }
                            names.push(name.clone());
                        }
                    }
                    TypeNodeKind::Function {
                        type_parameters,
                        parameters,
                        ..
                    }
                    | TypeNodeKind::Constructor {
                        type_parameters,
                        parameters,
                        ..
                    } if !plain_signature(type_parameters, parameters) => return false,
                    TypeNodeKind::Mapped { members, .. } if !members.is_empty() => return false,
                    _ => {}
                }
                node.push_authored_children(&mut pending);
            }
            AuthoredTypeItem::Member(member) => {
                let modifiers_displayable = match &member.kind {
                    TypeMemberKind::Property { .. } => member
                        .modifiers
                        .nodes
                        .iter()
                        .all(|modifier| modifier.kind == TypeMemberModifier::Readonly),
                    _ => member.modifiers.nodes.is_empty(),
                };
                if member.recovered || !modifiers_displayable {
                    return false;
                }
                if let Some((_, type_parameters, parameters, _)) = member.kind.signature()
                    && !plain_signature(type_parameters, parameters)
                {
                    return false;
                }
                member.push_authored_children(&mut pending);
            }
        }
    }
    true
}

fn plain_signature(
    type_parameters: &[crate::syntax::TypeParameterDeclaration],
    parameters: &[Parameter],
) -> bool {
    type_parameters.iter().all(|parameter| {
        !parameter.const_parameter && !parameter.in_variance && !parameter.out_variance
    }) && parameters
        .iter()
        .all(|parameter| parameter.initializer.is_none() && parameter.modifiers.is_empty())
}

pub(crate) fn render_authored_parameter(
    file: &ProgramFile,
    options: &CompilerOptions,
    parameter: &Parameter,
) -> Option<RenderedParameter> {
    if !parameter.modifiers.is_empty()
        || parameter.initializer.is_some() && parameter.annotation.is_none()
    {
        return None;
    }
    let ty = parameter.annotation.as_ref().map_or_else(
        || {
            Some(RenderedType {
                text: "any".to_string(),
                part_kind: "keyword",
            })
        },
        |ty| render_authored_type(file, options, ty),
    )?;
    let optional = parameter.optional || parameter.initializer.is_some();
    Some(RenderedParameter {
        text: format!(
            "{}{}{}: {}",
            if parameter.rest { "..." } else { "" },
            parameter.name,
            if optional { "?" } else { "" },
            ty.text
        ),
        name: parameter.name.clone(),
        rest: parameter.rest,
        optional,
        ty,
    })
}

pub(crate) fn render_authored_parameters(
    file: &ProgramFile,
    options: &CompilerOptions,
    parameters: &[Parameter],
) -> Option<RenderedParameters> {
    if parameters
        .iter()
        .any(|parameter| parameter.initializer.is_some() || !parameter.modifiers.is_empty())
    {
        return None;
    }
    let parameters = parameters
        .iter()
        .map(|parameter| render_authored_parameter(file, options, parameter))
        .collect::<Option<Vec<_>>>()?;
    let text = format!(
        "({})",
        parameters
            .iter()
            .map(|parameter| parameter.text.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    Some(RenderedParameters { text, parameters })
}

const fn authored_type_part_kind(ty: &TypeNode) -> &'static str {
    match &ty.kind {
        TypeNodeKind::Keyword(
            KeywordType::Any
            | KeywordType::BigInt
            | KeywordType::Boolean
            | KeywordType::Never
            | KeywordType::Null
            | KeywordType::Number
            | KeywordType::String
            | KeywordType::Undefined
            | KeywordType::Unknown
            | KeywordType::Void,
        ) => "keyword",
        TypeNodeKind::Literal(
            crate::syntax::Literal::String(crate::syntax::StringLiteral::Plain(_))
            | crate::syntax::Literal::Number(crate::syntax::NumberLiteral::Plain(_)),
        ) => "stringLiteral",
        _ => "text",
    }
}
