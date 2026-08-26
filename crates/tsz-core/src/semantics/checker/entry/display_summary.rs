//! Checker-owned, declaration-keyed display summaries for service consumers.

use std::collections::BTreeMap;

use crate::bind::{BoundDeclaration, DeclarationKind};
use crate::emit::display::{
    RenderedParameter, RenderedType, render_authored_parameter, render_authored_parameters,
    render_authored_type,
};
use crate::emit::{render_inferred_expression_type, variable_kind_text};
use crate::program::{ProgramFile, is_declaration_source};
use crate::source::{DeclId, NodeId, Span};
use crate::syntax::{Statement, StatementKind, VariableKind, for_each_statement_in};

use super::super::{Checker, DeclarationModel};

pub(crate) type DeclarationDisplaySummaries = BTreeMap<DeclId, DeclarationDisplaySummary>;

#[derive(Debug, Clone)]
pub(crate) enum DeclarationDisplayParts {
    Text,
    Variable(Option<RenderedType>),
    Function {
        parameters: Option<Vec<RenderedParameter>>,
        result: Option<RenderedType>,
    },
    Class,
    Parameter(Option<RenderedParameter>),
}

#[derive(Debug, Clone)]
pub(crate) struct DeclarationDisplaySummary {
    pub kind: &'static str,
    pub context_span: Option<Span>,
    pub exported: bool,
    pub ambient: bool,
    pub display: String,
    pub display_parts: DeclarationDisplayParts,
    pub quick_info_kind: Option<&'static str>,
}

impl Checker<'_> {
    pub(super) fn declaration_display_summaries(&self) -> DeclarationDisplaySummaries {
        let mut summaries = BTreeMap::new();
        for file in &self.program.files {
            for declaration in &file.bindings.declarations {
                if let Some(summary) = self.display_summary(file, declaration) {
                    summaries.insert(declaration.id, summary);
                }
            }
        }
        summaries
    }

    fn display_summary(
        &self,
        file: &ProgramFile,
        bound: &BoundDeclaration,
    ) -> Option<DeclarationDisplaySummary> {
        let context = owner_statement(file, bound.owner);
        let declaration_file = is_declaration_source(&file.source.path);
        let (kind, context_span, exported, ambient, display, display_parts, quick_info_kind) =
            match self.models.get(&bound.id).copied() {
                Some(DeclarationModel::Variable {
                    declaration,
                    declaration_kind,
                    ..
                }) => {
                    let kind = variable_kind_text(declaration_kind);
                    let ty = declaration
                        .annotation
                        .as_ref()
                        .and_then(|node| render_authored_type(file, self.options, node))
                        .or_else(|| {
                            declaration.initializer.as_ref().and_then(|expression| {
                                render_inferred_expression_type(
                                    expression,
                                    declaration_kind == VariableKind::Const,
                                )
                            })
                        });
                    let display = ty.as_ref().map_or_else(
                        || format!("{kind} {}", declaration.name),
                        |ty| format!("{kind} {}: {}", declaration.name, ty.text),
                    );
                    (
                        kind,
                        context.map(|statement| statement.span),
                        matches!(context.map(|statement| &statement.kind), Some(StatementKind::Variable(value)) if value.exported),
                        declaration_file,
                        display,
                        DeclarationDisplayParts::Variable(ty.clone()),
                        ty.is_some().then_some(kind),
                    )
                }
                Some(DeclarationModel::Function { declaration, .. }) => {
                    let parameters =
                        render_authored_parameters(file, self.options, &declaration.parameters);
                    let result = declaration
                        .return_type
                        .as_ref()
                        .and_then(|node| render_authored_type(file, self.options, node));
                    let display = match (&parameters, &result) {
                        (Some(parameters), Some(result)) => format!(
                            "function {}{}: {}",
                            declaration.name, parameters.text, result.text
                        ),
                        _ => format!("function {}", declaration.name),
                    };
                    let complete = declaration.type_parameters.is_empty()
                        && parameters.is_some()
                        && result.is_some();
                    (
                        "function",
                        context.map(|statement| statement.span),
                        declaration.exported,
                        declaration_file || declaration.declared,
                        display,
                        DeclarationDisplayParts::Function {
                            parameters: parameters.map(|value| value.parameters),
                            result,
                        },
                        complete.then_some("function"),
                    )
                }
                Some(DeclarationModel::TypeAlias { declaration, .. }) => {
                    let ty = render_authored_type(file, self.options, &declaration.ty);
                    let display = ty.as_ref().map_or_else(
                        || format!("type {}", declaration.name),
                        |ty| format!("type {} = {}", declaration.name, ty.text),
                    );
                    let complete = declaration.type_parameters.is_empty() && ty.is_some();
                    (
                        "type",
                        context.map(|statement| statement.span),
                        declaration.exported,
                        declaration_file,
                        display,
                        DeclarationDisplayParts::Text,
                        complete.then_some("type"),
                    )
                }
                Some(DeclarationModel::Interface { declaration, .. }) => (
                    "interface",
                    context.map(|statement| statement.span),
                    declaration.exported,
                    declaration_file,
                    format!("interface {}", declaration.name),
                    DeclarationDisplayParts::Text,
                    declaration
                        .type_parameters
                        .is_empty()
                        .then_some("interface"),
                ),
                Some(DeclarationModel::Class { declaration, .. }) => (
                    "class",
                    context.map(|statement| statement.span),
                    declaration.exported || declaration.default_export,
                    declaration_file || declaration.declared,
                    format!("class {}", declaration.name),
                    DeclarationDisplayParts::Class,
                    None,
                ),
                Some(DeclarationModel::Parameter { parameter, .. }) => {
                    let parts = render_authored_parameter(file, self.options, parameter);
                    let rendered = parts
                        .as_ref()
                        .filter(|_| {
                            parameter.initializer.is_none() && parameter.modifiers.is_empty()
                        })
                        .map(|parameter| parameter.text.clone());
                    let display = rendered.map_or_else(
                        || format!("(parameter) {}", parameter.name),
                        |parameter| format!("(parameter) {parameter}"),
                    );
                    (
                        "parameter",
                        Some(parameter.span),
                        false,
                        declaration_file
                            || matches!(context.map(|statement| &statement.kind), Some(StatementKind::Function(value)) if value.declared),
                        display,
                        DeclarationDisplayParts::Parameter(parts),
                        None,
                    )
                }
                None if bound.kind == DeclarationKind::Import => (
                    "alias",
                    context.map(|statement| statement.span),
                    false,
                    declaration_file,
                    format!("(alias) {}", bound.name),
                    DeclarationDisplayParts::Text,
                    None,
                ),
                Some(DeclarationModel::JavaScriptProperty(..)) | None => return None,
            };
        Some(DeclarationDisplaySummary {
            kind,
            context_span,
            exported,
            ambient,
            display,
            display_parts,
            quick_info_kind,
        })
    }
}

fn owner_statement(file: &ProgramFile, owner: NodeId) -> Option<&Statement> {
    let mut result = None;
    for_each_statement_in(&file.syntax.statements, &mut |statement| {
        if statement.id == owner {
            result = Some(statement);
        }
    });
    result
}
