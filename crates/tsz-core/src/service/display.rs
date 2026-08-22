//! Bounded syntax display for the language-service surface.
//!
//! This is deliberately not a type printer. It only renders shapes whose
//! meaning is fully represented by the fresh syntax tree. When syntax is
//! missing or initializer inference would require the checker, callers get
//! `None` instead of a plausible-looking semantic answer.

use crate::syntax::{
    AccessorKind, Expression, ExpressionKind, KeywordType, Literal, Parameter, TypeMember,
    TypeMemberKind, TypeMemberName, TypeMemberNameKind, TypeNode, TypeNodeKind,
    TypeParameterDeclaration, VariableDeclaration, VariableKind,
};

const MAX_DISPLAY_DEPTH: usize = 64;

pub(super) fn display_variable_type(declaration: &VariableDeclaration) -> Option<String> {
    if let Some(annotation) = &declaration.annotation {
        return display_type_node(annotation);
    }
    let Some(initializer) = &declaration.initializer else {
        return Some("any".to_string());
    };
    if let ExpressionKind::Literal(Literal::String(crate::syntax::StringLiteral::Extended(
        literal,
    ))) = &initializer.kind
    {
        return (declaration.declaration_kind == VariableKind::Var
            && !declaration.exported
            && literal.validation_supported())
        .then(|| "string".to_string());
    }
    display_expression_type(
        initializer,
        declaration.declaration_kind == VariableKind::Const,
        0,
    )
}

pub(super) fn display_type_node(node: &TypeNode) -> Option<String> {
    display_type_node_at_depth(node, 0)
}

pub(super) fn display_parameter(parameter: &Parameter) -> Option<String> {
    display_parameter_at_depth(parameter, 0)
}

pub(super) fn display_parameter_type(parameter: &Parameter) -> Option<String> {
    display_parameter_type_at_depth(parameter, 0)
}

fn display_expression_type(
    expression: &Expression,
    preserve_literal: bool,
    depth: usize,
) -> Option<String> {
    let depth = descend(depth)?;
    match &expression.kind {
        ExpressionKind::Literal(literal) => display_inferred_literal(literal, preserve_literal),
        ExpressionKind::Object(properties) => {
            if properties.is_empty() {
                return Some("{}".to_string());
            }
            let mut rendered = Vec::with_capacity(properties.len());
            for property in properties {
                // Object properties are mutable even when the binding is
                // `const`, so TypeScript widens their primitive literals.
                let ty = display_expression_type(&property.value, false, depth)?;
                rendered.push(format!("{}: {ty};", display_property_name(&property.name)));
            }
            Some(format!("{{ {} }}", rendered.join(" ")))
        }
        ExpressionKind::Parenthesized(inner) => {
            display_expression_type(inner, preserve_literal, depth)
        }
        ExpressionKind::Identifier { .. }
        | ExpressionKind::RegularExpression(_)
        | ExpressionKind::Array(_)
        | ExpressionKind::Call { .. }
        | ExpressionKind::New { .. }
        | ExpressionKind::Member { .. }
        | ExpressionKind::Arrow { .. }
        | ExpressionKind::Binary { .. }
        | ExpressionKind::Unary { .. }
        | ExpressionKind::Assignment { .. }
        | ExpressionKind::As { .. }
        | ExpressionKind::Missing => None,
    }
}

fn display_inferred_literal(literal: &Literal, preserve_literal: bool) -> Option<String> {
    match (preserve_literal, literal) {
        (true, Literal::String(crate::syntax::StringLiteral::Plain(value))) => {
            Some(display_string_literal(value))
        }
        (_, Literal::String(crate::syntax::StringLiteral::Extended(_))) => None,
        (true, Literal::NoSubstitutionTemplate(literal)) => {
            Some(display_string_literal(&literal.cooked))
        }
        (true, Literal::Number(value) | Literal::BigInt(value)) => Some(value.clone()),
        (true, Literal::Boolean(value)) => Some(value.to_string()),
        (_, Literal::String(_) | Literal::NoSubstitutionTemplate(_)) => Some("string".to_string()),
        (_, Literal::Number(_)) => Some("number".to_string()),
        (_, Literal::BigInt(_)) => Some("bigint".to_string()),
        (_, Literal::Boolean(_)) => Some("boolean".to_string()),
        (_, Literal::Null) => Some("null".to_string()),
    }
}

fn display_type_node_at_depth(node: &TypeNode, depth: usize) -> Option<String> {
    let depth = descend(depth)?;
    match &node.kind {
        TypeNodeKind::Keyword(keyword) => Some(display_keyword(*keyword).to_string()),
        TypeNodeKind::Literal(Literal::String(crate::syntax::StringLiteral::Plain(value))) => {
            Some(display_string_literal(value))
        }
        TypeNodeKind::Literal(Literal::String(crate::syntax::StringLiteral::Extended(literal))) => {
            let _ = literal;
            None
        }
        TypeNodeKind::Literal(Literal::Number(value) | Literal::BigInt(value)) => {
            Some(value.clone())
        }
        TypeNodeKind::Literal(Literal::Boolean(value)) => Some(value.to_string()),
        TypeNodeKind::Literal(Literal::Null) => Some("null".to_string()),
        TypeNodeKind::Array(element) => {
            Some(format!("{}[]", display_type_node_at_depth(element, depth)?))
        }
        TypeNodeKind::Tuple(elements) => {
            Some(format!("[{}]", display_type_nodes(elements, ", ", depth)?))
        }
        TypeNodeKind::Union(members) => display_type_nodes(members, " | ", depth),
        TypeNodeKind::Intersection(members) => display_type_nodes(members, " & ", depth),
        TypeNodeKind::Object(members) => {
            if members.is_empty() {
                return Some("{}".to_string());
            }
            if !type_member_list_is_displayable(members) {
                return None;
            }
            let rendered = members
                .iter()
                .map(|member| display_type_member(member, depth))
                .collect::<Option<Vec<_>>>()?;
            Some(format!("{{ {} }}", rendered.join(" ")))
        }
        TypeNodeKind::Function {
            type_parameters,
            parameters,
            return_type,
            ..
        } => Some(format!(
            "{}({}) => {}",
            display_type_parameters(type_parameters, depth)?,
            display_parameters(parameters, depth)?,
            display_type_node_at_depth(return_type, depth)?
        )),
        TypeNodeKind::Constructor {
            type_parameters,
            parameters,
            return_type,
            abstract_constructor,
            ..
        } => {
            let prefix = if *abstract_constructor {
                "abstract new"
            } else {
                "new"
            };
            Some(format!(
                "{prefix} {}({}) => {}",
                display_type_parameters(type_parameters, depth)?,
                display_parameters(parameters, depth)?,
                display_type_node_at_depth(return_type, depth)?
            ))
        }
        TypeNodeKind::Reference {
            name, arguments, ..
        } => {
            if arguments.is_empty() {
                Some(name.clone())
            } else {
                Some(format!(
                    "{name}<{}>",
                    display_type_nodes(arguments, ", ", depth)?
                ))
            }
        }
        TypeNodeKind::TypeQuery { name, .. } => Some(format!("typeof {name}")),
        TypeNodeKind::Infer {
            name, constraint, ..
        } => match constraint {
            Some(constraint) => Some(format!(
                "infer {name} extends {}",
                display_type_node_at_depth(constraint, depth)?
            )),
            None => Some(format!("infer {name}")),
        },
        TypeNodeKind::Predicate {
            parameter,
            asserts,
            ty,
            ..
        } => {
            let prefix = if *asserts { "asserts " } else { "" };
            match ty {
                Some(ty) => Some(format!(
                    "{prefix}{parameter} is {}",
                    display_type_node_at_depth(ty, depth)?
                )),
                None if *asserts => Some(format!("asserts {parameter}")),
                None => None,
            }
        }
        TypeNodeKind::KeyOf(operand) => Some(format!(
            "keyof {}",
            display_type_node_at_depth(operand, depth)?
        )),
        TypeNodeKind::Readonly(operand) => Some(format!(
            "readonly {}",
            display_type_node_at_depth(operand, depth)?
        )),
        TypeNodeKind::Conditional {
            check_type,
            extends_type,
            true_type,
            false_type,
        } => Some(format!(
            "{} extends {} ? {} : {}",
            display_type_node_at_depth(check_type, depth)?,
            display_type_node_at_depth(extends_type, depth)?,
            display_type_node_at_depth(true_type, depth)?,
            display_type_node_at_depth(false_type, depth)?
        )),
        TypeNodeKind::Mapped {
            parameter,
            constraint,
            name_type,
            value_type,
            readonly,
            optional,
            members,
            ..
        } => {
            if !members.is_empty() {
                return None;
            }
            let readonly = match readonly {
                Some(true) => "readonly ",
                Some(false) => "-readonly ",
                None => "",
            };
            let name_type = name_type.as_ref().map_or_else(
                || Some(String::new()),
                |name_type| {
                    Some(format!(
                        " as {}",
                        display_type_node_at_depth(name_type, depth)?
                    ))
                },
            )?;
            let optional = match optional {
                Some(true) => "?",
                Some(false) => "-?",
                None => "",
            };
            Some(format!(
                "{{ {readonly}[{parameter} in {}{name_type}]{optional}: {}; }}",
                display_type_node_at_depth(constraint, depth)?,
                display_type_node_at_depth(value_type, depth)?
            ))
        }
        TypeNodeKind::IndexedAccess { object, index } => Some(format!(
            "{}[{}]",
            display_type_node_at_depth(object, depth)?,
            display_type_node_at_depth(index, depth)?
        )),
        TypeNodeKind::Parenthesized(inner) => {
            Some(format!("({})", display_type_node_at_depth(inner, depth)?))
        }
        TypeNodeKind::Literal(Literal::NoSubstitutionTemplate(_)) | TypeNodeKind::Missing => None,
    }
}

fn type_member_list_is_displayable(members: &[TypeMember]) -> bool {
    let mut names = Vec::<&str>::new();
    for member in members {
        if member.recovered {
            return false;
        }
        if (!matches!(member.kind, TypeMemberKind::Property { .. })
            && !member.modifiers.nodes.is_empty())
            || member
                .modifiers
                .nodes
                .iter()
                .enumerate()
                .any(|(index, modifier)| {
                    !matches!(modifier.kind, crate::syntax::TypeMemberModifier::Readonly)
                        || member.modifiers.nodes[..index]
                            .iter()
                            .any(|prior| prior.kind == modifier.kind)
                })
        {
            return false;
        }
        let name = match &member.kind {
            TypeMemberKind::Property { name, .. } | TypeMemberKind::Method { name, .. } => name,
            TypeMemberKind::Accessor { .. }
            | TypeMemberKind::Call { .. }
            | TypeMemberKind::Construct { .. }
            | TypeMemberKind::Index { .. } => return false,
        };
        let TypeMemberNameKind::Identifier(name) = &name.kind else {
            return false;
        };
        if names.contains(&name.as_str()) {
            return false;
        }
        names.push(name);
    }
    true
}

fn display_type_member(member: &TypeMember, depth: usize) -> Option<String> {
    let readonly = if member.modifiers.readonly {
        "readonly "
    } else {
        ""
    };
    Some(match &member.kind {
        TypeMemberKind::Property {
            name,
            ty,
            optional,
            initializer,
        } => {
            if initializer.is_some() {
                return None;
            }
            format!(
                "{readonly}{}{}: {};",
                display_type_member_name(name)?,
                if *optional { "?" } else { "" },
                ty.as_ref().map_or_else(
                    || Some("any".to_string()),
                    |ty| display_type_node_at_depth(ty, depth)
                )?
            )
        }
        TypeMemberKind::Method {
            name,
            optional,
            type_parameters,
            parameters,
            return_type,
        } => format!(
            "{}{}{}({}): {};",
            display_type_member_name(name)?,
            if *optional { "?" } else { "" },
            display_type_parameters(type_parameters, depth)?,
            display_parameters(parameters, depth)?,
            return_type.as_ref().map_or_else(
                || Some("any".to_string()),
                |ty| display_type_node_at_depth(ty, depth)
            )?
        ),
        TypeMemberKind::Call {
            type_parameters,
            parameters,
            return_type,
        } => format!(
            "{}({}): {};",
            display_type_parameters(type_parameters, depth)?,
            display_parameters(parameters, depth)?,
            return_type.as_ref().map_or_else(
                || Some("any".to_string()),
                |ty| display_type_node_at_depth(ty, depth)
            )?
        ),
        TypeMemberKind::Construct {
            type_parameters,
            parameters,
            return_type,
        } => format!(
            "new {}({}): {};",
            display_type_parameters(type_parameters, depth)?,
            display_parameters(parameters, depth)?,
            return_type.as_ref().map_or_else(
                || Some("any".to_string()),
                |ty| display_type_node_at_depth(ty, depth)
            )?
        ),
        TypeMemberKind::Index {
            parameters,
            value_type,
        } => format!(
            "{readonly}[{}]: {};",
            display_parameters(parameters, depth)?,
            display_type_node_at_depth(value_type.as_ref()?, depth)?
        ),
        TypeMemberKind::Accessor {
            name,
            accessor,
            parameters,
            return_type,
        } => format!(
            "{} {}({}){};",
            match accessor {
                AccessorKind::Get => "get",
                AccessorKind::Set => "set",
            },
            display_type_member_name(name)?,
            display_parameters(parameters, depth)?,
            return_type.as_ref().map_or_else(
                || Some(String::new()),
                |ty| Some(format!(": {}", display_type_node_at_depth(ty, depth)?))
            )?
        ),
    })
}

fn display_type_member_name(name: &TypeMemberName) -> Option<String> {
    match &name.kind {
        TypeMemberNameKind::Identifier(name)
        | TypeMemberNameKind::NumericLiteral(name)
        | TypeMemberNameKind::BigIntLiteral(name) => Some(name.clone()),
        TypeMemberNameKind::StringLiteral(name) => Some(display_string_literal(name)),
        TypeMemberNameKind::Computed(_) => None,
    }
}

fn display_type_parameters(
    parameters: &[TypeParameterDeclaration],
    depth: usize,
) -> Option<String> {
    if parameters.is_empty() {
        return Some(String::new());
    }
    parameters
        .iter()
        .map(|parameter| {
            if parameter.const_parameter || parameter.in_variance || parameter.out_variance {
                return None;
            }
            let constraint = parameter.constraint.as_ref().map_or_else(
                || Some(String::new()),
                |constraint| {
                    Some(format!(
                        " extends {}",
                        display_type_node_at_depth(constraint, depth)?
                    ))
                },
            )?;
            let default = parameter.default.as_ref().map_or_else(
                || Some(String::new()),
                |default| {
                    Some(format!(
                        " = {}",
                        display_type_node_at_depth(default, depth)?
                    ))
                },
            )?;
            Some(format!("{}{constraint}{default}", parameter.name))
        })
        .collect::<Option<Vec<_>>>()
        .map(|parameters| format!("<{}>", parameters.join(", ")))
}

fn display_type_nodes(nodes: &[TypeNode], separator: &str, depth: usize) -> Option<String> {
    nodes
        .iter()
        .map(|node| display_type_node_at_depth(node, depth))
        .collect::<Option<Vec<_>>>()
        .map(|parts| parts.join(separator))
}

fn display_parameters(parameters: &[Parameter], depth: usize) -> Option<String> {
    parameters
        .iter()
        .map(|parameter| display_parameter_at_depth(parameter, depth))
        .collect::<Option<Vec<_>>>()
        .map(|parts| parts.join(", "))
}

fn display_parameter_at_depth(parameter: &Parameter, depth: usize) -> Option<String> {
    if parameter.initializer.is_some() || !parameter.modifiers.is_empty() {
        return None;
    }
    let annotation = display_parameter_type_at_depth(parameter, depth)?;
    let rest = if parameter.rest { "..." } else { "" };
    let optional = if parameter.optional { "?" } else { "" };
    Some(format!("{rest}{}{optional}: {annotation}", parameter.name))
}

fn display_parameter_type_at_depth(parameter: &Parameter, depth: usize) -> Option<String> {
    if !parameter.modifiers.is_empty()
        || (parameter.initializer.is_some() && parameter.annotation.is_none())
    {
        return None;
    }
    parameter.annotation.as_ref().map_or_else(
        || Some("any".to_string()),
        |annotation| display_type_node_at_depth(annotation, depth),
    )
}

const fn display_keyword(keyword: KeywordType) -> &'static str {
    match keyword {
        KeywordType::Any => "any",
        KeywordType::Unknown => "unknown",
        KeywordType::Never => "never",
        KeywordType::Void => "void",
        KeywordType::Undefined => "undefined",
        KeywordType::Null => "null",
        KeywordType::Boolean => "boolean",
        KeywordType::Number => "number",
        KeywordType::String => "string",
        KeywordType::BigInt => "bigint",
        KeywordType::Object => "object",
        KeywordType::Symbol => "symbol",
        KeywordType::UniqueSymbol => "unique symbol",
    }
}

fn display_property_name(name: &str) -> String {
    let mut characters = name.chars();
    let identifier = characters.next().is_some_and(is_identifier_start)
        && characters.all(is_identifier_continue);
    let numeric = !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_digit());
    if identifier || numeric {
        name.to_string()
    } else {
        display_string_literal(name)
    }
}

fn display_string_literal(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn is_identifier_start(character: char) -> bool {
    character == '_' || character == '$' || character.is_alphabetic()
}

fn is_identifier_continue(character: char) -> bool {
    is_identifier_start(character) || character.is_alphanumeric()
}

const fn descend(depth: usize) -> Option<usize> {
    if depth >= MAX_DISPLAY_DEPTH {
        None
    } else {
        Some(depth + 1)
    }
}
