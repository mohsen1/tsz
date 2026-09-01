//! Checker-owned, declaration-keyed display summaries for service consumers.
use std::collections::{BTreeSet, HashSet};

use super::super::{Checker, DeclarationModel, declaration_value::ValueQueryState};
use crate::bind::{BoundDeclaration, DeclarationKind, Meaning, ScopeId};
use crate::emit::display::{
    render_authored_parameter, render_authored_parameters, render_authored_type,
};
use crate::emit::variable_kind_text;
use crate::program::{
    CapabilityScope, CapabilityTarget, DeclarationDisplayParts, DeclarationDisplaySummaries,
    DeclarationDisplaySummary, DefaultExportDeclaration, ProgramFile, RenderedType,
    SemanticCompletion, is_declaration_source,
};
use crate::semantics::types::{Completion, TypeId, TypeKind, TypeStore};
use crate::source::{DeclId, FileId, NodeId};
use crate::syntax::{
    Expression, ExpressionKind, Literal, ObjectProperty, PropertyNameKind, Statement,
    StatementKind, TypeNode, TypeNodeKind, UnaryOperator, VariableDeclarator,
    for_each_statement_in,
};
use crate::text::quote_string;
type DefTargets = Result<Vec<DeclId>, SemanticCompletion>;
const DEFERRED: SemanticCompletion = SemanticCompletion::Deferred;

fn default_export_is_literal(expression: &Expression) -> bool {
    match &expression.peel_parentheses().kind {
        ExpressionKind::Literal(
            Literal::String(_)
            | Literal::NoSubstitutionTemplate(_)
            | Literal::Number(_)
            | Literal::BigInt(_)
            | Literal::Boolean(_),
        ) => true,
        ExpressionKind::Unary { operator, operand } => match operator {
            UnaryOperator::Plus => matches!(
                operand.peel_parentheses().kind,
                ExpressionKind::Literal(Literal::Number(_))
            ),
            UnaryOperator::Minus => matches!(
                operand.peel_parentheses().kind,
                ExpressionKind::Literal(Literal::Number(_) | Literal::BigInt(_))
            ),
            _ => false,
        },
        _ => false,
    }
}

impl Checker<'_> {
    pub(super) fn declaration_display_summaries(&mut self) -> DeclarationDisplaySummaries {
        if self.options.declaration {
            let values = self
                .models
                .iter()
                .filter_map(|(id, model)| {
                    matches!(
                        model,
                        DeclarationModel::Variable { .. } | DeclarationModel::Function { .. }
                    )
                    .then_some(*id)
                })
                .collect::<BTreeSet<_>>();
            for value in values {
                let _ = self.declaration_value_type(value);
            }
        }
        let mut summaries = DeclarationDisplaySummaries::new();
        for file in &self.program.files {
            for declaration in &file.bindings.declarations {
                summaries.insert(declaration.id, self.display_summary(file, declaration));
            }
        }
        let exports = self
            .program
            .files
            .iter()
            .flat_map(|file| {
                file.syntax.statements.iter().filter_map(|statement| {
                    let StatementKind::Export(declaration) = &statement.kind else {
                        return None;
                    };
                    let expression = declaration
                        .default_export
                        .then_some(declaration.assignment.as_ref())
                        .flatten()?;
                    (!matches!(expression.kind, ExpressionKind::Identifier { .. })).then(|| {
                        (
                            file.source.id,
                            statement.id,
                            file.bindings
                                .scope_for_node
                                .get(&expression.id)
                                .copied()
                                .unwrap_or(ScopeId(0)),
                            expression.clone(),
                        )
                    })
                })
            })
            .collect::<Vec<_>>();
        for (file, statement, scope, expression) in exports {
            if let Some(declaration) =
                self.default_export_declaration(file, statement, scope, &expression)
            {
                summaries.insert_default_export(file, statement, declaration);
            }
        }
        summaries
    }
    fn default_export_declaration(
        &mut self,
        file: FileId,
        statement: NodeId,
        scope: ScopeId,
        expression: &Expression,
    ) -> Option<DefaultExportDeclaration> {
        if !self
            .capabilities
            .semantic_check_node_is_claimed(file, statement)
            || !self
                .capabilities
                .claim(
                    CapabilityTarget::Declaration,
                    CapabilityScope::node(file, statement),
                )
                .is_claimed()
        {
            return None;
        }
        if default_export_is_literal(expression) {
            return Some(DefaultExportDeclaration::Literal);
        }
        let inferred = match self
            .expression_type_origins
            .get(&(file, expression.id))
            .copied()
        {
            Some(inferred) => inferred,
            None => self.infer_expression(file, scope, expression, None),
        };
        if matches!(
            self.store.kind(inferred),
            TypeKind::Error | TypeKind::Invalid(_)
        ) {
            return None;
        }
        let ready = self.complete_type(inferred)?;
        let ready = self.store.widened_literal_type(ready);
        let Completion::Complete(text) = self.display_type_for_diagnostic(ready) else {
            return None;
        };
        let dependencies = self.default_export_dependencies(ready)?;
        Some(DefaultExportDeclaration::Typed {
            ty: RenderedType {
                text,
                part_kind: "text",
            },
            preferred_name: match &expression.peel_parentheses().kind {
                ExpressionKind::Identifier { name, .. } => Some(name.clone()),
                _ => None,
            },
            dependencies,
        })
    }

    fn default_export_dependencies(&self, root: TypeId) -> Option<Vec<DeclId>> {
        let mut pending = vec![root];
        let mut seen = HashSet::new();
        let mut dependencies = BTreeSet::new();
        while let Some(ty) = pending.pop() {
            let Completion::Complete(ty) = self.ready_type_for_display(ty) else {
                return None;
            };
            if !seen.insert(ty) {
                continue;
            }
            match self.store.kind(ty) {
                TypeKind::Error | TypeKind::Invalid(_) | TypeKind::TypeParameter { .. } => {
                    return None;
                }
                TypeKind::ClassInstance { declaration, .. }
                | TypeKind::ClassConstructor { declaration, .. } => {
                    dependencies.insert(*declaration);
                }
                _ => {}
            }
            TypeStore::push_type_children(self.store.kind(ty), &mut pending);
        }
        dependencies
            .iter()
            .all(|declaration| {
                self.capabilities.declaration_is_claimed(
                    &self.program.files,
                    CapabilityTarget::Declaration,
                    *declaration,
                )
            })
            .then(|| dependencies.into_iter().collect())
    }
    fn display_summary(
        &self,
        file: &ProgramFile,
        bound: &BoundDeclaration,
    ) -> DeclarationDisplaySummary {
        let context = owner_statement(file, bound.owner);
        let declaration_file = is_declaration_source(&file.source.path);
        let (kind, context_span, exported, ambient, display, display_parts, quick_info_complete) =
            match self.models.get(&bound.id).copied() {
                Some(DeclarationModel::Variable {
                    declaration,
                    declaration_kind,
                    ..
                }) => {
                    let kind = variable_kind_text(declaration_kind);
                    let ty = match declaration.annotation.as_ref() {
                        Some(node) => render_authored_type(file, self.options, node),
                        None => self.inferred_display_type(bound.id),
                    };
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
                        ty.is_some(),
                    )
                }
                Some(DeclarationModel::Function { declaration, .. }) => {
                    let infer_empty = declaration.has_body && !declaration.is_async;
                    let parameters =
                        render_authored_parameters(file, self.options, &declaration.parameters);
                    let result = declaration
                        .return_type
                        .as_ref()
                        .and_then(|node| render_authored_type(file, self.options, node))
                        .or_else(|| {
                            (infer_empty && declaration.body.is_empty()).then(|| RenderedType {
                                text: "void".to_string(),
                                part_kind: "keyword",
                            })
                        })
                        .or_else(|| self.inferred_function_result(bound.id));
                    let display = match (&parameters, &result) {
                        (Some(parameters), Some(result)) => format!(
                            "function {}{}: {}",
                            declaration.name, parameters.text, result.text
                        ),
                        _ => format!("function {}", declaration.name),
                    };
                    let complete = declaration.type_parameters.is_empty()
                        && parameters.is_some()
                        && result.is_some()
                        && !self.function_value_requires_overload_resolution(bound.id);
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
                        complete,
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
                        complete,
                    )
                }
                Some(DeclarationModel::Interface { declaration, .. }) => (
                    "interface",
                    context.map(|statement| statement.span),
                    declaration.exported,
                    declaration_file,
                    format!("interface {}", declaration.name),
                    DeclarationDisplayParts::Text,
                    declaration.type_parameters.is_empty(),
                ),
                Some(DeclarationModel::Class { declaration, .. }) => (
                    "class",
                    context.map(|statement| statement.span),
                    declaration.exported || declaration.default_export,
                    declaration_file || declaration.declared,
                    format!("class {}", declaration.name),
                    DeclarationDisplayParts::Class,
                    false,
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
                        false,
                    )
                }
                None if bound.kind == DeclarationKind::Import => (
                    "alias",
                    context.map(|statement| statement.span),
                    false,
                    declaration_file,
                    format!("(alias) {}", bound.name),
                    DeclarationDisplayParts::Text,
                    false,
                ),
                None if bound.kind == DeclarationKind::UnmodeledHost => (
                    "module",
                    context.map(|statement| statement.span),
                    false,
                    false,
                    format!("module {}", bound.name),
                    DeclarationDisplayParts::Text,
                    false,
                ),
                Some(DeclarationModel::JavaScriptProperty(..)) | None => {
                    let kind = match bound.kind {
                        DeclarationKind::Variable => "var",
                        DeclarationKind::Parameter => "parameter",
                        DeclarationKind::Import => "alias",
                        DeclarationKind::Function
                        | DeclarationKind::FunctionExpression
                        | DeclarationKind::AnonymousSignature => "function",
                        DeclarationKind::JavaScriptPropertyAssignment
                        | DeclarationKind::TypeMember => "property",
                        DeclarationKind::Class => "class",
                        DeclarationKind::TypeAlias => "type",
                        DeclarationKind::Interface => "interface",
                        DeclarationKind::TypeParameter => "type parameter",
                        DeclarationKind::AnonymousType => "",
                        DeclarationKind::UnmodeledHost => "module",
                    };
                    (
                        kind,
                        context.map(|statement| statement.span),
                        false,
                        false,
                        format!("({kind}) {}", bound.name),
                        DeclarationDisplayParts::Text,
                        false,
                    )
                }
            };
        let declaration_type = match self.models.get(&bound.id).copied() {
            Some(DeclarationModel::Variable { declaration, .. }) => {
                self.variable_declaration_type_summary(file, bound, declaration)
            }
            _ => None,
        };
        let references_complete = match self.models.get(&bound.id).copied() {
            Some(DeclarationModel::Variable { .. } | DeclarationModel::Function { .. }) => {
                quick_info_complete
            }
            Some(DeclarationModel::Class { declaration, .. }) => {
                declaration.type_parameters.is_empty()
            }
            Some(DeclarationModel::Parameter { parameter, .. }) => {
                parameter.annotation.is_some()
                    && matches!(&display_parts, DeclarationDisplayParts::Parameter(Some(_)))
            }
            None if matches!(
                bound.kind,
                DeclarationKind::Parameter | DeclarationKind::TypeParameter
            ) =>
            {
                file.bindings
                    .declarations
                    .iter()
                    .filter(|declaration| {
                        declaration.scope == bound.scope
                            && declaration.name == bound.name
                            && declaration.meaning == bound.meaning
                    })
                    .count()
                    == 1
            }
            Some(DeclarationModel::TypeAlias { .. } | DeclarationModel::Interface { .. }) => {
                quick_info_complete
                    && file.bindings.declarations.iter().all(|declaration| {
                        declaration.scope != bound.scope
                            || declaration.name != bound.name
                            || declaration.meaning == bound.meaning
                    })
            }
            Some(DeclarationModel::JavaScriptProperty(..)) | None => false,
        };
        let (type_definition_targets, type_definition_completion) = self.type_def_summary(bound);
        DeclarationDisplaySummary {
            kind,
            context_span,
            exported,
            ambient,
            display,
            display_parts,
            quick_info_completion: if quick_info_complete {
                SemanticCompletion::Complete
            } else {
                SemanticCompletion::Deferred
            },
            type_definition_targets,
            type_definition_completion,
            references_completion: if references_complete {
                SemanticCompletion::Complete
            } else {
                SemanticCompletion::Deferred
            },
            declaration_type,
        }
    }
    fn type_def_summary(&self, bound: &BoundDeclaration) -> (Vec<DeclId>, SemanticCompletion) {
        match self.type_definition_targets(bound, &[]) {
            Ok(targets)
                if targets.iter().all(|target| {
                    self.capabilities.declaration_is_claimed(
                        &self.program.files,
                        CapabilityTarget::TypeDefinition,
                        *target,
                    )
                }) =>
            {
                (targets, SemanticCompletion::Complete)
            }
            Ok(_) => (Vec::new(), DEFERRED),
            Err(completion) => (Vec::new(), completion.combine(DEFERRED)),
        }
    }
    fn type_definition_targets(
        &self,
        bound: &BoundDeclaration,
        active_aliases: &[DeclId],
    ) -> DefTargets {
        match self.models.get(&bound.id).copied() {
            Some(DeclarationModel::Variable {
                declaration, scope, ..
            }) => match &declaration.annotation {
                Some(node) => self.type_defs(bound.id.file, scope, node, active_aliases),
                None => self.inferred_type_defs(bound, declaration).ok_or(DEFERRED),
            },
            Some(DeclarationModel::Function { declaration, scope })
                if !self.function_value_requires_overload_resolution(bound.id) =>
            {
                let node = declaration.return_type.as_ref().ok_or(DEFERRED)?;
                self.type_defs(bound.id.file, scope, node, active_aliases)
            }
            Some(DeclarationModel::TypeAlias { .. } | DeclarationModel::Interface { .. }) => {
                Ok(vec![bound.id])
            }
            Some(DeclarationModel::Class { identity, .. }) => Ok(vec![identity]),
            None if bound.kind == DeclarationKind::AnonymousType => Ok(vec![bound.id]),
            None if bound.kind == DeclarationKind::Import => {
                let (value, r#type) = self
                    .program
                    .import_alias_targets(bound.id)
                    .ok_or(DEFERRED)?;
                match bound.meaning {
                    Meaning::Type => r#type.map(|target| vec![target]).ok_or(DEFERRED),
                    Meaning::Value => {
                        let target = value
                            .and_then(|value| {
                                self.program
                                    .file(value.file)
                                    .and_then(|file| file.bindings.declaration(value))
                            })
                            .ok_or(DEFERRED)?;
                        self.type_definition_targets(target, active_aliases)
                    }
                }
            }
            _ => Err(DEFERRED),
        }
    }
    fn inferred_type_defs(
        &self,
        bound: &BoundDeclaration,
        declaration: &VariableDeclarator,
    ) -> Option<Vec<DeclId>> {
        let initializer = declaration
            .initializer
            .as_ref()
            .map(Expression::peel_parentheses_and_assertions)?;
        match &initializer.kind {
            ExpressionKind::Literal(_) => Some(Vec::new()),
            ExpressionKind::New { callee, .. }
                if matches!(
                    self.value_queries.get(&bound.id),
                    Some(ValueQueryState::Ready(_))
                ) =>
            {
                let ExpressionKind::Identifier { name, .. } = &callee.peel_parentheses().kind
                else {
                    return None;
                };
                let target = self.resolve_name(bound.id.file, bound.scope, name, Meaning::Value)?;
                let Some(DeclarationModel::Class { identity, .. }) = self.models.get(&target)
                else {
                    return None;
                };
                Some(vec![*identity])
            }
            _ => None,
        }
    }
    fn type_defs(
        &self,
        file: FileId,
        scope: ScopeId,
        node: &TypeNode,
        active_aliases: &[DeclId],
    ) -> DefTargets {
        match &node.kind {
            TypeNodeKind::Reference { name, .. } => {
                let target = self
                    .resolve_name(file, scope, name, Meaning::Type)
                    .ok_or(DEFERRED)?;
                if self.program.standard_library_declaration(target).is_some() {
                    return Err(DEFERRED);
                }
                if let Some(DeclarationModel::TypeAlias {
                    declaration,
                    scope: alias_scope,
                }) = self.models.get(&target).copied()
                {
                    if active_aliases.contains(&target) {
                        return Err(SemanticCompletion::Cycle);
                    }
                    let mut nested = active_aliases.to_vec();
                    nested.push(target);
                    let targets =
                        self.type_defs(target.file, alias_scope, &declaration.ty, &nested)?;
                    if !targets.is_empty() {
                        return Ok(targets);
                    }
                }
                Ok(vec![target])
            }
            TypeNodeKind::Union(members) | TypeNodeKind::Intersection(members) => {
                let mut targets = Vec::new();
                for member in members {
                    for target in self.type_defs(file, scope, member, active_aliases)? {
                        if !targets.contains(&target) {
                            targets.push(target);
                        }
                    }
                }
                Ok(targets)
            }
            TypeNodeKind::Parenthesized(return_type) => {
                self.type_defs(file, scope, return_type, active_aliases)
            }
            TypeNodeKind::Object(_) => self
                .anonymous_type_def(file, node)
                .map(|target| vec![target])
                .ok_or(DEFERRED),
            TypeNodeKind::Keyword(_) | TypeNodeKind::Literal(_) => Ok(Vec::new()),
            _ => Err(DEFERRED),
        }
    }
    fn anonymous_type_def(&self, file: FileId, node: &TypeNode) -> Option<DeclId> {
        self.program
            .file(file)?
            .bindings
            .anonymous_type_literal(node.span)
    }
    fn variable_declaration_type_summary(
        &self,
        file: &ProgramFile,
        bound: &BoundDeclaration,
        declaration: &VariableDeclarator,
    ) -> Option<RenderedType> {
        if declaration.annotation.is_some() || !self.semantic_declaration_is_claimed(bound.id) {
            return None;
        }
        let initializer = declaration.initializer.as_ref()?.peel_parentheses();
        if let ExpressionKind::Identifier {
            name,
            name_span: reference_span,
            ..
        } = &initializer.kind
        {
            let target = self.resolve_name(file.source.id, bound.scope, name, Meaning::Value)?;
            return (target != bound.id
                && matches!(
                self.models.get(&target),
                Some(DeclarationModel::Function { declaration, .. })
                    if declaration.has_body
                        && !declaration.declared
                        && !declaration.default_export
                        && !declaration.abstract_declaration
                        && declaration.overload_context_is_recovery_free()
                )
                && self.semantic_declaration_is_claimed(target)
                && self.value_group_ids(target).as_slice() == [target])
            .then(|| RenderedType {
                text: format!("typeof {}", file.source.slice(*reference_span).trim()),
                part_kind: "text",
            });
        }
        self.array_object_union_declaration_type(bound.id, initializer)
            .or_else(|| self.inferred_display_type(bound.id))
    }
    fn inferred_display_type(&self, declaration: DeclId) -> Option<RenderedType> {
        let ValueQueryState::Ready(value) = self.value_queries.get(&declaration)? else {
            return None;
        };
        let Completion::Complete(text) = self.display_type_for_diagnostic(*value) else {
            return None;
        };
        Some(RenderedType {
            text,
            part_kind: "text",
        })
    }

    fn inferred_function_result(&self, declaration: DeclId) -> Option<RenderedType> {
        let ValueQueryState::Ready(value) = self.value_queries.get(&declaration)? else {
            return None;
        };
        let (TypeKind::Function(signature) | TypeKind::ShapeFunction(signature)) =
            self.store.kind(*value)
        else {
            return None;
        };
        let Completion::Complete(result) = self.ready_type_for_display(signature.return_type)
        else {
            return None;
        };
        let Completion::Complete(text) = self.display_type_for_diagnostic(result) else {
            return None;
        };
        Some(RenderedType {
            text,
            part_kind: "text",
        })
    }

    fn array_object_union_declaration_type(
        &self,
        declaration: DeclId,
        initializer: &Expression,
    ) -> Option<RenderedType> {
        let ExpressionKind::Array(elements) = &initializer.kind else {
            return None;
        };
        let value = match self.value_queries.get(&declaration)? {
            ValueQueryState::Ready(value) => *value,
            ValueQueryState::Provisional | ValueQueryState::Computing => return None,
        };
        let TypeKind::Array(element) = self.store.kind(value) else {
            return None;
        };
        let TypeKind::Union(members) = self.store.kind(*element) else {
            return None;
        };
        if elements.len() != members.len() {
            return None;
        }
        let objects = elements
            .iter()
            .map(|element| match &element.peel_parentheses().kind {
                ExpressionKind::Object(properties) => Some(properties.as_slice()),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()?;
        let mut display_properties = Vec::<(String, String)>::new();
        for properties in &objects {
            for property in *properties {
                let display = declaration_property_name(property)?;
                if !display_properties
                    .iter()
                    .any(|(name, _)| name == &property.name)
                {
                    display_properties.push((property.name.clone(), display));
                }
            }
        }
        let mut text = "(".to_string();
        for (index, (member, authored)) in members.iter().zip(objects).enumerate() {
            let TypeKind::Object(shape) = self.store.kind(*member) else {
                return None;
            };
            if shape.properties.len() != display_properties.len()
                || !shape.call_signatures.is_empty()
                || !shape.construct_signatures.is_empty()
                || !shape.index_signatures.is_empty()
            {
                return None;
            }
            if index != 0 {
                text.push_str(" | ");
            }
            text.push_str("{\n");
            for (name, display_name) in &display_properties {
                let property = shape
                    .properties
                    .iter()
                    .find(|property| &property.name == name)?;
                let authored_property = authored.iter().find(|property| &property.name == name);
                text.push_str("    ");
                if let Some(method) = authored_property
                    .filter(|property| object_property_is_method(property))
                    .and_then(|_| self.render_object_method(display_name, property.ty))
                {
                    text.push_str(&method);
                } else {
                    if authored_property.is_none()
                        && (!property.optional || property.ty != self.store.builtins.undefined)
                    {
                        return None;
                    }
                    text.push_str(display_name);
                    if property.optional {
                        text.push('?');
                    }
                    text.push_str(": ");
                    let Completion::Complete(property_type) = self.store.display(property.ty)
                    else {
                        return None;
                    };
                    text.push_str(&property_type);
                }
                text.push_str(";\n");
            }
            text.push('}');
        }
        text.push_str(")[]");
        Some(RenderedType {
            text,
            part_kind: "text",
        })
    }
    fn render_object_method(&self, name: &str, ty: TypeId) -> Option<String> {
        let TypeKind::Function(signature) = self.store.kind(ty) else {
            return None;
        };
        if signature.generic_declaration.is_some() || !signature.parameters.is_empty() {
            return None;
        }
        let Completion::Complete(return_type) = self.store.display(signature.return_type) else {
            return None;
        };
        Some(format!("{name}(): {return_type}"))
    }
}
fn object_property_is_method(property: &ObjectProperty) -> bool {
    matches!(
        &property.value.peel_parentheses().kind,
        ExpressionKind::FunctionLike(function) if function.syntax.is_object_method()
    )
}
fn declaration_property_name(property: &ObjectProperty) -> Option<String> {
    match property.name_kind {
        PropertyNameKind::Identifier | PropertyNameKind::NumericLiteral => {
            Some(property.name.clone())
        }
        PropertyNameKind::StringLiteral | PropertyNameKind::Computed => {
            Some(quote_string(&property.name))
        }
        PropertyNameKind::PrivateIdentifier | PropertyNameKind::Unsupported => None,
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
