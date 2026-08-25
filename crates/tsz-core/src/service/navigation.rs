//! Declaration-identity navigation shared by the service operations.
//!
//! When a source location binds to a declaration, TypeScript 7 uses that same
//! identity for definition, references, highlights, and rename. TSZ does the
//! same here through a single immutable index over the program's bound files.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::bind::{DeclarationKind, Meaning, ScopeId};
use crate::program::{Program, ProgramFile};
use crate::source::{DeclId, FileId, NodeId, Span};
use crate::syntax::{
    ClassMemberKind, DescendantAdapter, DescendantContainer, Expression, ExpressionKind,
    FunctionLikeBody, FunctionLikeExpression, NestedStatement, Parameter, Statement, StatementKind,
    SwitchClauseKind, TypeMember, TypeMemberKind, TypeMemberNameKind, TypeNode, TypeNodeKind,
    TypeParameterDeclaration, VariableKind, walk_function_like_descendants,
    walk_statement_descendants,
};

use super::{
    TextSpan, display_parameter, display_parameter_type, display_type_node, display_variable_type,
    normalize_path,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinitionInfo {
    pub file_name: String,
    pub text_span: TextSpan,
    pub kind: String,
    pub name: String,
    pub container_name: String,
    pub is_local: bool,
    pub is_ambient: bool,
    pub unverified: bool,
    pub failed_alias_resolution: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_span: Option<TextSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinitionAndBoundSpan {
    pub definitions: Vec<DefinitionInfo>,
    pub text_span: TextSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolDisplayPart {
    pub text: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferencedSymbolDefinition {
    pub container_kind: String,
    pub container_name: String,
    pub file_name: String,
    pub kind: String,
    pub name: String,
    pub text_span: TextSpan,
    pub display_parts: Vec<SymbolDisplayPart>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_span: Option<TextSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceEntry {
    pub text_span: TextSpan,
    pub file_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_span: Option<TextSpan>,
    pub is_write_access: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_definition: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferencedSymbol {
    pub definition: ReferencedSymbolDefinition,
    pub references: Vec<ReferenceEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightSpan {
    pub text_span: TextSpan,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_span: Option<TextSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentHighlights {
    pub file_name: String,
    pub highlight_spans: Vec<HighlightSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameInfo {
    pub can_rename: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind_modifiers: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_span: Option<TextSpan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub localized_error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameLocation {
    pub file_name: String,
    pub text_span: TextSpan,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_span: Option<TextSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameResult {
    pub info: RenameInfo,
    pub locations: Vec<RenameLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum SymbolKey {
    Bound(DeclId),
    GlobalValue(String),
    GlobalType(String),
    GlobalValueAndType(String),
    Synthetic {
        file: FileId,
        start: u32,
        meaning: NavigationMeaning,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum NavigationMeaning {
    Value,
    Type,
}

impl From<Meaning> for NavigationMeaning {
    fn from(value: Meaning) -> Self {
        match value {
            Meaning::Value => Self::Value,
            Meaning::Type => Self::Type,
        }
    }
}

#[derive(Debug, Clone)]
struct DeclarationMetadata {
    file_name: String,
    name: String,
    kind: String,
    span: TextSpan,
    context_span: Option<TextSpan>,
    is_local: bool,
    is_ambient: bool,
    display: String,
    display_parts: Vec<SymbolDisplayPart>,
}

#[derive(Debug, Clone)]
struct Occurrence {
    key: SymbolKey,
    file_name: String,
    span: TextSpan,
    context_span: Option<TextSpan>,
    is_write_access: bool,
    is_declaration: bool,
}

#[derive(Debug, Default)]
pub(super) struct NavigationIndex {
    declarations: BTreeMap<SymbolKey, Vec<DeclarationMetadata>>,
    occurrences: Vec<Occurrence>,
    declaration_keys: BTreeMap<DeclId, SymbolKey>,
}

impl NavigationIndex {
    pub(super) fn build(program: &Program) -> Self {
        let mut index = Self::default();
        let dual_globals = dual_global_names(program);

        for file in &program.files {
            index.collect_bound_declarations(file, &dual_globals);
        }
        for file in &program.files {
            index.collect_references(program, file);
        }
        index.occurrences.sort_by(|left, right| {
            (&left.file_name, left.span.start, left.span.length).cmp(&(
                &right.file_name,
                right.span.start,
                right.span.length,
            ))
        });
        index
    }

    pub(super) fn definition(&self, path: &str, offset: u32) -> Option<DefinitionAndBoundSpan> {
        let occurrence = self.occurrence_at(path, offset)?;
        let declarations = self.declarations.get(&occurrence.key)?;
        Some(DefinitionAndBoundSpan {
            definitions: declarations
                .iter()
                .map(|declaration| DefinitionInfo {
                    file_name: declaration.file_name.clone(),
                    text_span: declaration.span,
                    kind: declaration.kind.clone(),
                    name: declaration.name.clone(),
                    container_name: String::new(),
                    is_local: declaration.is_local,
                    is_ambient: declaration.is_ambient,
                    unverified: false,
                    failed_alias_resolution: false,
                    context_span: declaration.context_span,
                })
                .collect(),
            text_span: occurrence.span,
        })
    }

    pub(super) fn references(&self, path: &str, offset: u32) -> Vec<ReferencedSymbol> {
        let Some(origin) = self.occurrence_at(path, offset) else {
            return Vec::new();
        };
        let Some(declaration) = self
            .declarations
            .get(&origin.key)
            .and_then(|declarations| declarations.first())
        else {
            return Vec::new();
        };
        let mark_definitions = origin.is_declaration;
        let references = self
            .occurrences
            .iter()
            .filter(|occurrence| occurrence.key == origin.key)
            .map(|occurrence| ReferenceEntry {
                text_span: occurrence.span,
                file_name: occurrence.file_name.clone(),
                context_span: occurrence.context_span,
                is_write_access: occurrence.is_write_access,
                is_definition: mark_definitions.then_some(occurrence.is_declaration),
            })
            .collect();
        vec![ReferencedSymbol {
            definition: ReferencedSymbolDefinition {
                container_kind: String::new(),
                container_name: String::new(),
                file_name: declaration.file_name.clone(),
                kind: declaration.kind.clone(),
                name: declaration.display.clone(),
                text_span: declaration.span,
                display_parts: declaration.display_parts.clone(),
                context_span: declaration.context_span,
            },
            references,
        }]
    }

    pub(super) fn document_highlights(
        &self,
        path: &str,
        offset: u32,
        files_to_search: &[String],
    ) -> Vec<DocumentHighlights> {
        let Some(origin) = self.occurrence_at(path, offset) else {
            return Vec::new();
        };
        let requested = files_to_search
            .iter()
            .map(|path| normalize_path(path))
            .collect::<BTreeSet<_>>();
        let mut by_file: BTreeMap<String, Vec<HighlightSpan>> = BTreeMap::new();
        for occurrence in self
            .occurrences
            .iter()
            .filter(|occurrence| occurrence.key == origin.key)
        {
            if !requested.is_empty() && !requested.contains(&occurrence.file_name) {
                continue;
            }
            by_file
                .entry(occurrence.file_name.clone())
                .or_default()
                .push(HighlightSpan {
                    text_span: occurrence.span,
                    kind: if occurrence.is_write_access {
                        "writtenReference".to_string()
                    } else {
                        "reference".to_string()
                    },
                    context_span: occurrence.context_span,
                });
        }
        by_file
            .into_iter()
            .map(|(file_name, highlight_spans)| DocumentHighlights {
                file_name,
                highlight_spans,
            })
            .collect()
    }

    pub(super) fn rename(&self, path: &str, offset: u32) -> RenameResult {
        let Some(origin) = self.occurrence_at(path, offset) else {
            return RenameResult::failure();
        };
        let Some(declaration) = self
            .declarations
            .get(&origin.key)
            .and_then(|declarations| declarations.first())
        else {
            return RenameResult::failure();
        };
        RenameResult {
            info: RenameInfo {
                can_rename: true,
                display_name: Some(declaration.name.clone()),
                full_display_name: Some(declaration.name.clone()),
                kind: Some(declaration.kind.clone()),
                kind_modifiers: Some(String::new()),
                trigger_span: Some(origin.span),
                localized_error_message: None,
            },
            locations: self
                .occurrences
                .iter()
                .filter(|occurrence| occurrence.key == origin.key)
                .map(|occurrence| RenameLocation {
                    file_name: occurrence.file_name.clone(),
                    text_span: occurrence.span,
                    context_span: occurrence.context_span,
                })
                .collect(),
        }
    }

    fn occurrence_at(&self, path: &str, offset: u32) -> Option<&Occurrence> {
        let normalized = normalize_path(path);
        self.occurrences
            .iter()
            .find(|occurrence| {
                occurrence.file_name == normalized
                    && occurrence.span.start <= offset
                    && offset < occurrence.span.start + occurrence.span.length
            })
            .or_else(|| {
                self.occurrences.iter().find(|occurrence| {
                    occurrence.file_name == normalized
                        && occurrence.span.length > 0
                        && occurrence.span.start + occurrence.span.length == offset
                })
            })
    }

    fn collect_bound_declarations(&mut self, file: &ProgramFile, dual_globals: &BTreeSet<String>) {
        let file_name = normalize_path(&file.source.path.to_string_lossy());
        let module_file = file.is_external_module();
        let syntax_metadata = syntax_declaration_metadata(file);
        let mut same_span: BTreeMap<(u32, u32), SymbolKey> = BTreeMap::new();

        for declaration in &file.bindings.declarations {
            // Type-member groups are binder facts for checking. Until the
            // service owns merged overload/call/index display provenance,
            // exposing each declaration as an independent property would be
            // a false quickinfo/rename success.
            if matches!(
                declaration.kind,
                DeclarationKind::TypeMember | DeclarationKind::AnonymousSignature
            ) || declaration.kind == DeclarationKind::FunctionExpression
                && declaration.name.is_empty()
            {
                continue;
            }
            let span_key = (declaration.name_span.start, declaration.name_span.end);
            let key = if let Some(key) = same_span.get(&span_key) {
                key.clone()
            } else if declaration.scope == ScopeId(0)
                && declaration.kind != DeclarationKind::Import
                && !module_file
            {
                if dual_globals.contains(&declaration.name) {
                    SymbolKey::GlobalValueAndType(declaration.name.clone())
                } else {
                    match declaration.meaning {
                        Meaning::Value => SymbolKey::GlobalValue(declaration.name.clone()),
                        Meaning::Type => SymbolKey::GlobalType(declaration.name.clone()),
                    }
                }
            } else {
                SymbolKey::Bound(declaration.id)
            };
            same_span.entry(span_key).or_insert_with(|| key.clone());
            self.declaration_keys.insert(declaration.id, key.clone());

            if self.declarations.get(&key).is_some_and(|items| {
                items
                    .iter()
                    .any(|item| item.span == text_span(declaration.name_span))
            }) {
                continue;
            }
            let fallback = fallback_metadata(declaration.kind, &declaration.name);
            let syntax = syntax_metadata.get(&span_key);
            let is_local = declaration.scope != ScopeId(0)
                || declaration.kind == DeclarationKind::Import
                || (module_file && !syntax.is_some_and(|metadata| metadata.exported));
            let kind =
                syntax.map_or_else(|| fallback.kind.clone(), |metadata| metadata.kind.clone());
            let metadata = DeclarationMetadata {
                file_name: file_name.clone(),
                name: declaration.name.clone(),
                kind: if declaration.kind == DeclarationKind::Variable && is_local && kind == "var"
                {
                    "local var".to_string()
                } else {
                    kind
                },
                span: text_span(declaration.name_span),
                context_span: syntax.and_then(|metadata| metadata.context_span),
                is_local,
                is_ambient: syntax.is_some_and(|metadata| metadata.ambient),
                display: syntax.map_or_else(
                    || fallback.display.clone(),
                    |metadata| metadata.display.clone(),
                ),
                display_parts: syntax.map_or_else(
                    || fallback.display_parts.clone(),
                    |metadata| metadata.display_parts.clone(),
                ),
            };
            self.declarations
                .entry(key.clone())
                .or_default()
                .push(metadata);
            self.occurrences.push(Occurrence {
                key,
                file_name: file_name.clone(),
                span: text_span(declaration.name_span),
                context_span: syntax.and_then(|metadata| metadata.context_span),
                is_write_access: true,
                is_declaration: true,
            });
        }
    }

    fn collect_references(&mut self, program: &Program, file: &ProgramFile) {
        let file_name = normalize_path(&file.source.path.to_string_lossy());
        let mut visitor = ReferenceVisitor {
            program,
            file,
            file_name,
            index: self,
            value_locals: Vec::new(),
            type_locals: Vec::new(),
        };
        visitor.visit_statements(&file.syntax.statements, ScopeId(0));
    }
}

impl RenameResult {
    pub(super) fn failure() -> Self {
        Self {
            info: RenameInfo {
                can_rename: false,
                display_name: None,
                full_display_name: None,
                kind: None,
                kind_modifiers: None,
                trigger_span: None,
                localized_error_message: Some("You cannot rename this element.".to_string()),
            },
            locations: Vec::new(),
        }
    }
}

struct ReferenceVisitor<'a> {
    program: &'a Program,
    file: &'a ProgramFile,
    file_name: String,
    index: &'a mut NavigationIndex,
    value_locals: Vec<BTreeMap<String, SymbolKey>>,
    type_locals: Vec<BTreeMap<String, SymbolKey>>,
}

impl ReferenceVisitor<'_> {
    fn scope_for_node(&self, node: NodeId, fallback: ScopeId) -> ScopeId {
        self.file
            .bindings
            .scope_for_node
            .get(&node)
            .copied()
            .unwrap_or(fallback)
    }

    fn visit_statements(&mut self, statements: &[Statement], scope: ScopeId) {
        for statement in statements {
            self.visit_statement(statement, scope);
        }
    }

    fn visit_bound_statements(&mut self, statements: &[Statement], scope: ScopeId) {
        for statement in statements {
            let statement_scope = self.scope_for_node(statement.id, scope);
            self.visit_statement(statement, statement_scope);
        }
    }

    fn visit_statement(&mut self, statement: &Statement, scope: ScopeId) {
        match &statement.kind {
            StatementKind::Export(declaration) => {
                for specifier in &declaration.specifiers {
                    let meaning = if declaration.type_only || specifier.type_only {
                        Meaning::Type
                    } else {
                        Meaning::Value
                    };
                    self.record_name(
                        &specifier.local,
                        specifier.local_span,
                        scope,
                        meaning,
                        false,
                    );
                }
                if let Some(assignment) = &declaration.assignment {
                    self.visit_expression(assignment, scope, false);
                }
            }
            StatementKind::Variable(declaration) => {
                if let Some(annotation) = &declaration.annotation {
                    self.visit_type(annotation, scope);
                }
                if let Some(initializer) = &declaration.initializer {
                    self.visit_expression(initializer, scope, false);
                }
            }
            StatementKind::Function(declaration) => {
                let retained_type_locals = self.visit_signature_types_with_host(
                    statement.id,
                    scope,
                    &declaration.type_parameters,
                    &declaration.parameters,
                    declaration.return_type.as_ref(),
                    declaration.has_body,
                );
                let function_scope = self.scope_for_node(statement.id, scope);
                if declaration.has_body {
                    for parameter in &declaration.parameters {
                        if let Some(initializer) = &parameter.initializer {
                            self.visit_expression(initializer, function_scope, false);
                        }
                    }
                }
                self.visit_bound_statements(&declaration.body, function_scope);
                if retained_type_locals {
                    self.type_locals.pop();
                }
            }
            StatementKind::Class(declaration) => {
                self.push_type_parameter_locals(&declaration.type_parameters);
                self.visit_type_parameter_bounds(&declaration.type_parameters, scope);
                let class_scope = self.scope_for_node(statement.id, scope);
                if let Some(extends) = &declaration.extends {
                    self.visit_type(extends, scope);
                }
                for implemented in &declaration.implements {
                    self.visit_type(implemented, scope);
                }
                for member in &declaration.members {
                    let (type_parameters, parameters, return_type, body, has_body) =
                        match &member.kind {
                            ClassMemberKind::Property {
                                annotation,
                                initializer,
                                ..
                            } => {
                                if let Some(annotation) = annotation {
                                    self.visit_type(annotation, class_scope);
                                }
                                if let Some(initializer) = initializer {
                                    self.visit_expression(initializer, class_scope, false);
                                }
                                continue;
                            }
                            ClassMemberKind::Constructor {
                                parameters,
                                body,
                                has_body,
                                ..
                            } => (
                                &[][..],
                                parameters.as_slice(),
                                None,
                                body.as_slice(),
                                *has_body,
                            ),
                            ClassMemberKind::Method {
                                type_parameters,
                                parameters,
                                return_type,
                                body,
                                has_body,
                                ..
                            } => (
                                type_parameters.as_slice(),
                                parameters.as_slice(),
                                return_type.as_ref(),
                                body.as_slice(),
                                *has_body,
                            ),
                        };
                    let retained_type_locals = self.visit_signature_types_with_host(
                        member.id,
                        class_scope,
                        type_parameters,
                        parameters,
                        return_type,
                        has_body,
                    );
                    let member_scope = self.scope_for_node(member.id, class_scope);
                    if has_body {
                        for parameter in parameters {
                            if let Some(initializer) = &parameter.initializer {
                                self.visit_expression(initializer, member_scope, false);
                            }
                        }
                    }
                    self.visit_bound_statements(body, member_scope);
                    if retained_type_locals {
                        self.type_locals.pop();
                    }
                }
                self.type_locals.pop();
            }
            StatementKind::TypeAlias(declaration) => {
                self.push_type_parameter_locals(&declaration.type_parameters);
                self.visit_type_parameter_bounds(&declaration.type_parameters, scope);
                self.visit_type(&declaration.ty, scope);
                self.type_locals.pop();
            }
            StatementKind::Interface(declaration) => {
                self.push_type_parameter_locals(&declaration.type_parameters);
                self.visit_type_parameter_bounds(&declaration.type_parameters, scope);
                for extended in &declaration.extends {
                    self.visit_type(extended, scope);
                }
                for member in &declaration.members {
                    self.visit_type_member(member, scope);
                }
                self.type_locals.pop();
            }
            StatementKind::Return(expression) => {
                if let Some(expression) = expression {
                    self.visit_expression(expression, scope, false);
                }
            }
            StatementKind::Block(statements) => {
                let block_scope = self.scope_for_node(statement.id, scope);
                self.visit_statements(statements, block_scope);
            }
            StatementKind::If(control_flow) => {
                self.visit_expression(&control_flow.condition, scope, false);
                let then_scope = self.scope_for_node(control_flow.then_statement.id, scope);
                self.visit_statement(&control_flow.then_statement, then_scope);
                if let Some(else_statement) = &control_flow.else_statement {
                    let else_scope = self.scope_for_node(else_statement.id, scope);
                    self.visit_statement(else_statement, else_scope);
                }
            }
            StatementKind::Switch(control_flow) => {
                let switch_scope = self.scope_for_node(statement.id, scope);
                self.visit_expression(&control_flow.expression, switch_scope, false);
                for clause in &control_flow.clauses {
                    if let SwitchClauseKind::Case(expression) = &clause.kind {
                        self.visit_expression(expression, switch_scope, false);
                    }
                    self.visit_statements(&clause.statements, switch_scope);
                }
            }
            StatementKind::Expression(expression) => {
                self.visit_expression(expression, scope, false);
            }
            StatementKind::Import(_)
            | StatementKind::Break(_)
            | StatementKind::Continue(_)
            | StatementKind::Empty
            | StatementKind::Unknown => {}
        }
    }

    fn visit_expression(&mut self, expression: &Expression, scope: ScopeId, write: bool) {
        match &expression.kind {
            ExpressionKind::Identifier {
                name,
                name_span,
                entity_name,
            } => {
                if !entity_name {
                    return;
                }
                self.record_name(name, *name_span, scope, Meaning::Value, write);
            }
            ExpressionKind::This
            | ExpressionKind::Literal(_)
            | ExpressionKind::RegularExpression(_)
            | ExpressionKind::Missing => {}
            ExpressionKind::Object(properties) => {
                for property in properties {
                    self.visit_expression(&property.value, scope, write);
                }
            }
            ExpressionKind::Array(elements) => {
                for element in elements {
                    self.visit_expression(element, scope, false);
                }
            }
            ExpressionKind::Call {
                callee,
                type_arguments,
                arguments,
            } => {
                self.visit_expression(callee, scope, false);
                for type_argument in type_arguments.iter().flatten() {
                    self.visit_type(type_argument, scope);
                }
                for argument in arguments {
                    self.visit_expression(argument, scope, false);
                }
            }
            ExpressionKind::New {
                callee,
                type_arguments,
                arguments,
            } => {
                self.visit_expression(callee, scope, false);
                for type_argument in type_arguments {
                    self.visit_type(type_argument, scope);
                }
                for argument in arguments {
                    self.visit_expression(argument, scope, false);
                }
            }
            ExpressionKind::Member { object, .. } => {
                self.visit_expression(object, scope, false);
            }
            ExpressionKind::ElementAccess { object, index } => {
                self.visit_expression(object, scope, false);
                self.visit_expression(index, scope, false);
            }
            ExpressionKind::FunctionLike(function) => {
                let function_scope = self.scope_for_node(expression.id, scope);
                let retained_type_locals = self.visit_signature_types_with_host(
                    expression.id,
                    function_scope,
                    &function.type_parameters,
                    &function.parameters,
                    function.return_type.as_ref(),
                    true,
                );
                for parameter in &function.parameters {
                    if let Some(initializer) = &parameter.initializer {
                        self.visit_expression(initializer, function_scope, false);
                    }
                }
                match function.syntax.body() {
                    FunctionLikeBody::Expression(body) => {
                        self.visit_expression(body, function_scope, false)
                    }
                    FunctionLikeBody::Statements(body) => {
                        self.visit_bound_statements(body, function_scope)
                    }
                }
                if retained_type_locals {
                    self.type_locals.pop();
                }
            }
            ExpressionKind::Binary { left, right, .. } => {
                self.visit_expression(left, scope, false);
                self.visit_expression(right, scope, false);
            }
            ExpressionKind::Unary { operand, .. } => {
                self.visit_expression(operand, scope, false);
            }
            ExpressionKind::Assignment { left, right, .. } => {
                self.visit_expression(left, scope, true);
                self.visit_expression(right, scope, false);
            }
            ExpressionKind::As { expression, ty } => {
                self.visit_expression(expression, scope, false);
                self.visit_type(ty, scope);
            }
            ExpressionKind::Parenthesized(expression) => {
                self.visit_expression(expression, scope, write);
            }
        }
    }

    fn visit_type(&mut self, node: &TypeNode, scope: ScopeId) {
        match &node.kind {
            TypeNodeKind::Keyword(_) | TypeNodeKind::Literal(_) | TypeNodeKind::Missing => {}
            TypeNodeKind::Array(element)
            | TypeNodeKind::KeyOf(element)
            | TypeNodeKind::Readonly(element)
            | TypeNodeKind::Parenthesized(element) => self.visit_type(element, scope),
            TypeNodeKind::Tuple(elements)
            | TypeNodeKind::Union(elements)
            | TypeNodeKind::Intersection(elements) => {
                for element in elements {
                    self.visit_type(element, scope);
                }
            }
            TypeNodeKind::Object(members) => {
                for member in members {
                    self.visit_type_member(member, scope);
                }
            }
            TypeNodeKind::Function {
                id,
                type_parameters,
                parameters,
                return_type,
                ..
            }
            | TypeNodeKind::Constructor {
                id,
                type_parameters,
                parameters,
                return_type,
                ..
            } => {
                self.visit_signature_types(
                    *id,
                    scope,
                    type_parameters,
                    parameters,
                    Some(return_type),
                );
            }
            TypeNodeKind::Reference {
                name,
                name_span,
                arguments,
            } => {
                self.record_name(name, *name_span, scope, Meaning::Type, false);
                for argument in arguments {
                    self.visit_type(argument, scope);
                }
            }
            TypeNodeKind::TypeQuery {
                name,
                name_span,
                segment_spans,
            } => {
                let root = name.split('.').next().unwrap_or(name);
                let root_span = segment_spans.first().copied().unwrap_or(*name_span);
                self.record_type_query_root(root, root_span, scope);
            }
            TypeNodeKind::Infer { constraint, .. } => {
                if let Some(constraint) = constraint {
                    self.visit_type(constraint, scope);
                }
            }
            TypeNodeKind::Predicate { ty, .. } => {
                if let Some(ty) = ty {
                    self.visit_type(ty, scope);
                }
            }
            TypeNodeKind::Conditional {
                check_type,
                extends_type,
                true_type,
                false_type,
            } => {
                self.visit_type(check_type, scope);
                self.visit_type(extends_type, scope);
                self.visit_type(true_type, scope);
                self.visit_type(false_type, scope);
            }
            TypeNodeKind::Mapped {
                parameter,
                parameter_span,
                constraint,
                name_type,
                value_type,
                members,
                ..
            } => {
                self.visit_type(constraint, scope);
                let key = self.synthetic_type_parameter(parameter, *parameter_span);
                self.type_locals
                    .push(BTreeMap::from([(parameter.clone(), key)]));
                if let Some(name_type) = name_type {
                    self.visit_type(name_type, scope);
                }
                self.visit_type(value_type, scope);
                for member in members {
                    self.visit_type_member(member, scope);
                }
                self.type_locals.pop();
            }
            TypeNodeKind::IndexedAccess { object, index } => {
                self.visit_type(object, scope);
                self.visit_type(index, scope);
            }
        }
    }

    fn visit_type_member(&mut self, member: &TypeMember, scope: ScopeId) {
        if member.recovered {
            return;
        }
        let member_scope = self.file.bindings.scope_for_node.get(&member.id).copied();
        if let TypeMemberKind::Property {
            name,
            ty,
            initializer,
            ..
        } = &member.kind
        {
            if let TypeMemberNameKind::Computed(expression) = &name.kind {
                self.visit_expression(expression, scope, false);
            }
            if let Some(member_scope) = member_scope {
                if let Some(ty) = ty {
                    self.visit_type(ty, member_scope);
                }
                if let Some(initializer) = initializer {
                    self.visit_expression(initializer, member_scope, false);
                }
            }
            return;
        }
        let Some((name, type_parameters, parameters, return_type)) = member.kind.signature() else {
            return;
        };
        if let Some(name) = name
            && let TypeMemberNameKind::Computed(expression) = &name.kind
        {
            self.visit_expression(expression, scope, false);
        }
        self.visit_signature_types(member.id, scope, type_parameters, parameters, return_type);
    }

    fn visit_signature_types(
        &mut self,
        owner: NodeId,
        enclosing_scope: ScopeId,
        type_parameters: &[TypeParameterDeclaration],
        parameters: &[Parameter],
        return_type: Option<&TypeNode>,
    ) {
        self.visit_signature_types_with_host(
            owner,
            enclosing_scope,
            type_parameters,
            parameters,
            return_type,
            false,
        );
    }

    fn visit_signature_types_with_host(
        &mut self,
        owner: NodeId,
        enclosing_scope: ScopeId,
        type_parameters: &[TypeParameterDeclaration],
        parameters: &[Parameter],
        return_type: Option<&TypeNode>,
        implementation: bool,
    ) -> bool {
        let Some(signature_scope) = self.file.bindings.scope_for_node.get(&owner).copied() else {
            return false;
        };
        self.push_type_parameter_locals(type_parameters);
        self.visit_type_parameter_bounds(type_parameters, enclosing_scope);
        for parameter in parameters {
            if let Some(annotation) = &parameter.annotation {
                self.visit_type(annotation, signature_scope);
            }
            if !implementation && let Some(initializer) = &parameter.initializer {
                self.visit_expression(initializer, signature_scope, false);
            }
        }
        if let Some(return_type) = return_type {
            self.visit_type(return_type, signature_scope);
        }
        if !implementation {
            self.type_locals.pop();
        }
        implementation
    }

    fn push_type_parameter_locals(&mut self, parameters: &[TypeParameterDeclaration]) {
        let mut locals = BTreeMap::new();
        for parameter in parameters {
            let key = self.synthetic_type_parameter(&parameter.name, parameter.name_span);
            locals.entry(parameter.name.clone()).or_insert(key);
        }
        self.type_locals.push(locals);
    }

    fn visit_type_parameter_bounds(
        &mut self,
        parameters: &[TypeParameterDeclaration],
        scope: ScopeId,
    ) {
        for parameter in parameters {
            if let Some(constraint) = &parameter.constraint {
                self.visit_type(constraint, scope);
            }
            if let Some(default) = &parameter.default {
                self.visit_type(default, scope);
            }
        }
    }

    fn record_name(
        &mut self,
        name: &str,
        span: Span,
        scope: ScopeId,
        meaning: Meaning,
        write: bool,
    ) {
        let local = match meaning {
            Meaning::Value => self
                .value_locals
                .iter()
                .rev()
                .find_map(|locals| locals.get(name)),
            Meaning::Type => self
                .type_locals
                .iter()
                .rev()
                .find_map(|locals| locals.get(name)),
        };
        let key = local.cloned().or_else(|| {
            self.file
                .bindings
                .resolve(scope, name, meaning)
                .or_else(|| self.program.resolve_global(name, meaning))
                .and_then(|id| self.index.declaration_keys.get(&id).cloned())
        });
        let Some(key) = key else {
            return;
        };
        self.index.occurrences.push(Occurrence {
            key,
            file_name: self.file_name.clone(),
            span: text_span(span),
            context_span: None,
            is_write_access: write,
            is_declaration: false,
        });
    }

    fn record_type_query_root(&mut self, name: &str, span: Span, scope: ScopeId) {
        let local = self
            .value_locals
            .iter()
            .rev()
            .find_map(|locals| locals.get(name))
            .cloned();
        let key = local.or_else(|| {
            self.program
                .resolve_type_query_root(self.file.source.id, scope, name)
                .map(|root| root.navigation_declaration())
                .and_then(|declaration| self.index.declaration_keys.get(&declaration).cloned())
        });
        let Some(key) = key else {
            return;
        };
        self.index.occurrences.push(Occurrence {
            key,
            file_name: self.file_name.clone(),
            span: text_span(span),
            context_span: None,
            is_write_access: false,
            is_declaration: false,
        });
    }

    fn synthetic_type_parameter(&mut self, name: &str, span: Span) -> SymbolKey {
        let key = SymbolKey::Synthetic {
            file: self.file.source.id,
            start: span.start,
            meaning: NavigationMeaning::Type,
        };
        let display = format!("(type parameter) {name} in type");
        self.index
            .declarations
            .entry(key.clone())
            .or_default()
            .push(DeclarationMetadata {
                file_name: self.file_name.clone(),
                name: name.to_string(),
                kind: "type parameter".to_string(),
                span: text_span(span),
                context_span: None,
                is_local: true,
                is_ambient: false,
                display: display.clone(),
                display_parts: vec![display_part(&display, "text")],
            });
        self.index.occurrences.push(Occurrence {
            key: key.clone(),
            file_name: self.file_name.clone(),
            span: text_span(span),
            context_span: None,
            is_write_access: true,
            is_declaration: true,
        });
        key
    }
}

fn dual_global_names(program: &Program) -> BTreeSet<String> {
    program
        .files
        .iter()
        .filter(|file| !file.is_external_module())
        .flat_map(|file| &file.bindings.declarations)
        .filter(|declaration| {
            declaration.scope == ScopeId(0) && matches!(declaration.kind, DeclarationKind::Class)
        })
        .map(|declaration| declaration.name.clone())
        .collect()
}

#[derive(Debug, Clone)]
struct SyntaxMetadata {
    kind: String,
    context_span: Option<TextSpan>,
    exported: bool,
    ambient: bool,
    display: String,
    display_parts: Vec<SymbolDisplayPart>,
}

fn syntax_declaration_metadata(file: &ProgramFile) -> BTreeMap<(u32, u32), SyntaxMetadata> {
    let mut metadata = BTreeMap::new();
    let ambient_context = is_declaration_file(file);
    {
        let mut collector = SyntaxMetadataCollector {
            metadata: &mut metadata,
        };
        for statement in &file.syntax.statements {
            collector.statement(ambient_context, statement);
        }
    }
    metadata
}

struct SyntaxMetadataCollector<'metadata> {
    metadata: &'metadata mut BTreeMap<(u32, u32), SyntaxMetadata>,
}

impl SyntaxMetadataCollector<'_> {
    fn statement(&mut self, ambient_context: bool, statement: &Statement) {
        match &statement.kind {
            StatementKind::Import(declaration) => {
                for binding in &declaration.bindings {
                    let display = format!("(alias) {}", binding.local);
                    self.insert(
                        (binding.local_span, statement.span),
                        "alias",
                        (false, ambient_context),
                        (display.clone(), vec![display_part(&display, "text")]),
                    );
                }
            }
            StatementKind::Variable(declaration) => {
                let kind = match declaration.declaration_kind {
                    VariableKind::Const => "const",
                    VariableKind::Let => "let",
                    VariableKind::Var => "var",
                };
                let ty = display_variable_type(declaration);
                let display = ty.as_ref().map_or_else(
                    || format!("{kind} {}", declaration.name),
                    |ty| format!("{kind} {}: {ty}", declaration.name),
                );
                self.insert(
                    (declaration.name_span, statement.span),
                    kind,
                    (declaration.exported, ambient_context),
                    (
                        display,
                        variable_display_parts(kind, &declaration.name, ty.as_deref()),
                    ),
                );
            }
            StatementKind::Function(declaration) => {
                let parameters = declaration
                    .parameters
                    .iter()
                    .map(display_parameter)
                    .collect::<Option<Vec<_>>>()
                    .map(|parameters| parameters.join(", "));
                let result = declaration.return_type.as_ref().and_then(display_type_node);
                let display = match (&parameters, &result) {
                    (Some(parameters), Some(result)) => {
                        format!("function {}({parameters}): {result}", declaration.name)
                    }
                    _ => format!("function {}", declaration.name),
                };
                self.insert(
                    (declaration.name_span, statement.span),
                    "function",
                    (
                        declaration.exported,
                        ambient_context || declaration.declared,
                    ),
                    (display, function_display_parts(declaration)),
                );
            }
            StatementKind::Class(declaration) => {
                let display = format!("class {}", declaration.name);
                self.insert(
                    (declaration.name_span, statement.span),
                    "class",
                    (
                        declaration.exported || declaration.default_export,
                        ambient_context || declaration.declared,
                    ),
                    (
                        display,
                        vec![
                            display_part("class", "keyword"),
                            display_part(" ", "space"),
                            display_part(&declaration.name, "className"),
                        ],
                    ),
                );
            }
            StatementKind::TypeAlias(declaration) => {
                let display = display_type_node(&declaration.ty).map_or_else(
                    || format!("type {}", declaration.name),
                    |ty| format!("type {} = {ty}", declaration.name),
                );
                self.insert(
                    (declaration.name_span, statement.span),
                    "type",
                    (declaration.exported, ambient_context),
                    (display.clone(), vec![display_part(&display, "text")]),
                );
            }
            StatementKind::Interface(declaration) => {
                let display = format!("interface {}", declaration.name);
                self.insert(
                    (declaration.name_span, statement.span),
                    "interface",
                    (declaration.exported, ambient_context),
                    (display.clone(), vec![display_part(&display, "text")]),
                );
            }
            StatementKind::Export(_)
            | StatementKind::Break(_)
            | StatementKind::Continue(_)
            | StatementKind::Return(_)
            | StatementKind::Block(_)
            | StatementKind::If(_)
            | StatementKind::Switch(_)
            | StatementKind::Expression(_)
            | StatementKind::Empty
            | StatementKind::Unknown => {}
        }
        walk_statement_descendants(self, &ambient_context, statement);
    }

    fn insert(
        &mut self,
        (name_span, context_span): (Span, Span),
        kind: &str,
        (exported, ambient): (bool, bool),
        (display, display_parts): (String, Vec<SymbolDisplayPart>),
    ) {
        self.metadata.insert(
            (name_span.start, name_span.end),
            SyntaxMetadata {
                kind: kind.to_string(),
                context_span: Some(text_span(context_span)),
                exported,
                ambient,
                display,
                display_parts,
            },
        );
    }

    fn parameters(&mut self, parameters: &[Parameter], ambient_context: bool) -> bool {
        for parameter in parameters {
            insert_parameter_metadata(parameter, ambient_context, self.metadata);
        }
        ambient_context
    }
}

impl<'ast> DescendantAdapter<'ast> for SyntaxMetadataCollector<'_> {
    type Context = bool;

    fn context(
        &mut self,
        ambient_context: &Self::Context,
        container: DescendantContainer<'ast>,
    ) -> Self::Context {
        match container {
            DescendantContainer::Function(_, declaration) => self.parameters(
                &declaration.parameters,
                *ambient_context || declaration.declared,
            ),
            DescendantContainer::Class(_, declaration) => *ambient_context || declaration.declared,
            DescendantContainer::ClassMember(member) => match &member.kind {
                ClassMemberKind::Constructor { parameters, .. }
                | ClassMemberKind::Method { parameters, .. } => {
                    self.parameters(parameters, *ambient_context)
                }
                ClassMemberKind::Property { .. } => *ambient_context,
            },
            DescendantContainer::Statement(_) | DescendantContainer::FunctionLike(_, _) => {
                *ambient_context
            }
        }
    }

    fn nested_statement(
        &mut self,
        ambient_context: &Self::Context,
        statement: &'ast Statement,
        _next_statement: Option<&'ast Statement>,
    ) -> NestedStatement {
        self.statement(*ambient_context, statement);
        NestedStatement::Handled
    }

    fn function_like(
        &mut self,
        ambient_context: &Self::Context,
        expression: &'ast Expression,
        function: &'ast FunctionLikeExpression,
    ) {
        self.parameters(&function.parameters, *ambient_context);
        walk_function_like_descendants(self, ambient_context, expression, function);
    }
}

fn insert_parameter_metadata(
    parameter: &Parameter,
    ambient: bool,
    metadata: &mut BTreeMap<(u32, u32), SyntaxMetadata>,
) {
    let display = display_parameter(parameter).map_or_else(
        || format!("(parameter) {}", parameter.name),
        |parameter| format!("(parameter) {parameter}"),
    );
    metadata.insert(
        (parameter.name_span.start, parameter.name_span.end),
        SyntaxMetadata {
            kind: "parameter".to_string(),
            context_span: Some(text_span(parameter.span)),
            exported: false,
            ambient,
            display,
            display_parts: parameter_display_parts(parameter),
        },
    );
}

fn fallback_metadata(kind: DeclarationKind, name: &str) -> SyntaxMetadata {
    let (kind, display) = match kind {
        DeclarationKind::Variable => ("var", format!("var {name}")),
        DeclarationKind::Parameter => ("parameter", format!("(parameter) {name}")),
        DeclarationKind::Import => ("alias", format!("(alias) {name}")),
        DeclarationKind::Function | DeclarationKind::FunctionExpression => {
            ("function", format!("function {name}"))
        }
        DeclarationKind::Class => ("class", format!("class {name}")),
        DeclarationKind::TypeAlias => ("type", format!("type {name}")),
        DeclarationKind::Interface => ("interface", format!("interface {name}")),
        DeclarationKind::TypeMember | DeclarationKind::JavaScriptPropertyAssignment => {
            ("property", format!("(property) {name}"))
        }
        DeclarationKind::AnonymousSignature => ("type", "(anonymous signature)".to_string()),
        DeclarationKind::UnmodeledHost => ("module", format!("module {name}")),
    };
    SyntaxMetadata {
        kind: kind.to_string(),
        context_span: None,
        exported: false,
        ambient: false,
        display: display.clone(),
        display_parts: vec![display_part(&display, "text")],
    }
}

fn is_declaration_file(file: &ProgramFile) -> bool {
    let path = file.source.path.to_string_lossy();
    path.ends_with(".d.ts") || path.ends_with(".d.mts") || path.ends_with(".d.cts")
}

fn variable_display_parts(kind: &str, name: &str, ty: Option<&str>) -> Vec<SymbolDisplayPart> {
    let mut parts = vec![
        display_part(kind, "keyword"),
        display_part(" ", "space"),
        display_part(name, "localName"),
    ];
    if let Some(ty) = ty {
        parts.push(display_part(":", "punctuation"));
        parts.push(display_part(" ", "space"));
        parts.push(display_part(ty, display_type_part_kind(ty)));
    }
    parts
}

fn function_display_parts(
    declaration: &crate::syntax::FunctionDeclaration,
) -> Vec<SymbolDisplayPart> {
    let mut parts = vec![
        display_part("function", "keyword"),
        display_part(" ", "space"),
        display_part(&declaration.name, "functionName"),
        display_part("(", "punctuation"),
    ];
    for (index, parameter) in declaration.parameters.iter().enumerate() {
        if index > 0 {
            parts.push(display_part(",", "punctuation"));
            parts.push(display_part(" ", "space"));
        }
        let Some(ty) = display_parameter_type(parameter) else {
            return vec![display_part(
                &format!("function {}", declaration.name),
                "text",
            )];
        };
        if parameter.rest {
            parts.push(display_part("...", "punctuation"));
        }
        parts.push(display_part(&parameter.name, "parameterName"));
        if parameter.optional || parameter.initializer.is_some() {
            parts.push(display_part("?", "punctuation"));
        }
        parts.push(display_part(":", "punctuation"));
        parts.push(display_part(" ", "space"));
        parts.push(display_part(&ty, display_type_part_kind(&ty)));
    }
    let Some(result) = declaration.return_type.as_ref().and_then(display_type_node) else {
        return vec![display_part(
            &format!("function {}", declaration.name),
            "text",
        )];
    };
    parts.push(display_part(")", "punctuation"));
    parts.push(display_part(":", "punctuation"));
    parts.push(display_part(" ", "space"));
    parts.push(display_part(&result, display_type_part_kind(&result)));
    parts
}

fn parameter_display_parts(parameter: &Parameter) -> Vec<SymbolDisplayPart> {
    let Some(ty) = display_parameter_type(parameter) else {
        return vec![
            display_part("(", "punctuation"),
            display_part("parameter", "text"),
            display_part(")", "punctuation"),
            display_part(" ", "space"),
            display_part(&parameter.name, "parameterName"),
        ];
    };
    let mut parts = vec![
        display_part("(", "punctuation"),
        display_part("parameter", "text"),
        display_part(")", "punctuation"),
        display_part(" ", "space"),
    ];
    if parameter.rest {
        parts.push(display_part("...", "punctuation"));
    }
    parts.push(display_part(&parameter.name, "parameterName"));
    if parameter.optional || parameter.initializer.is_some() {
        parts.push(display_part("?", "punctuation"));
    }
    parts.push(display_part(":", "punctuation"));
    parts.push(display_part(" ", "space"));
    parts.push(display_part(&ty, display_type_part_kind(&ty)));
    parts
}

fn display_type_part_kind(ty: &str) -> &'static str {
    match ty {
        "any" | "bigint" | "boolean" | "never" | "null" | "number" | "string" | "undefined"
        | "unknown" | "void" => "keyword",
        value if value.starts_with('"') || value.bytes().all(|byte| byte.is_ascii_digit()) => {
            "stringLiteral"
        }
        _ => "text",
    }
}

fn display_part(text: &str, kind: &str) -> SymbolDisplayPart {
    SymbolDisplayPart {
        text: text.to_string(),
        kind: kind.to_string(),
    }
}

const fn text_span(span: Span) -> TextSpan {
    TextSpan {
        start: span.start,
        length: span.len(),
    }
}
