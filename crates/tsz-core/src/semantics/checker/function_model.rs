use std::collections::{HashMap, HashSet};

use crate::bind::ScopeId;
use crate::source::{DeclId, FileId, NodeId};
use crate::syntax::{
    ArrowBody, Expression, ExpressionKind, FunctionDeclaration, Parameter, Statement,
    StatementKind, TypeNode,
};

use super::Checker;
use crate::semantics::types::{
    Completion, ParameterType, Signature, TypeId, TypeKind, UnionPolicy,
};

struct ReturnSite<'a> {
    statement: &'a Statement,
    expression: Option<&'a Expression>,
}

#[derive(Default)]
struct ReturnAnalysis<'a> {
    sites: Vec<ReturnSite<'a>>,
    supported: bool,
}

impl Checker<'_> {
    pub(super) fn infer_arrow_expression(
        &mut self,
        file: FileId,
        scope: ScopeId,
        owner: NodeId,
        parameters: &[Parameter],
        annotation: Option<&TypeNode>,
        body: &ArrowBody,
        expected: Option<TypeId>,
    ) -> TypeId {
        let arrow_scope = self.program.files[file.0 as usize]
            .bindings
            .scope_for_node
            .get(&owner)
            .copied()
            .unwrap_or(scope);
        let expected_signature = expected
            .and_then(|expected| self.complete_type(expected))
            .and_then(|expected| self.callable_signature(expected));
        let mut resolved = Vec::with_capacity(parameters.len());
        for (index, parameter) in parameters.iter().enumerate() {
            if parameter.initializer.is_some()
                && (parameter.annotation.is_some()
                    || parameter.initializer.as_ref().is_some_and(|initializer| {
                        !matches!(initializer.kind, ExpressionKind::Literal(_))
                    }))
            {
                let _ = self.require_completion(Completion::<()>::Deferred);
            }
            let ty = if let Some(annotation) = &parameter.annotation {
                if annotation.contains_type_query() {
                    let _ = self.require_completion(Completion::<()>::Deferred);
                }
                self.resolve_type_node(file, arrow_scope, annotation, &HashMap::new())
            } else if let Some(initializer) = &parameter.initializer {
                let completion = self.signature_initializer_type(file, arrow_scope, initializer);
                match self.require_completion(completion) {
                    Completion::Complete(ty) => ty,
                    Completion::Deferred | Completion::Cycle | Completion::Limit => {
                        self.store.builtins.any
                    }
                }
            } else {
                expected_signature
                    .as_ref()
                    .and_then(|signature| signature.parameters.get(index))
                    .map_or(self.store.builtins.any, |parameter| parameter.ty)
            };
            if parameter.annotation.is_none()
                && parameter.initializer.is_none()
                && expected_signature.is_none()
                && self.options.effective_no_implicit_any()
            {
                self.push_diagnostic(
                    file,
                    parameter.name_span,
                    format!(
                        "Parameter '{}' implicitly has an 'any' type.",
                        parameter.name
                    ),
                    7006,
                );
            }
            resolved.push(ParameterType {
                name: parameter.name.clone(),
                ty,
                optional: parameter.optional || parameter.initializer.is_some(),
                rest: parameter.rest,
            });
        }
        let expected_return = annotation
            .map(|annotation| {
                if annotation.contains_type_query() {
                    let _ = self.require_completion(Completion::<()>::Deferred);
                }
                self.resolve_type_node(file, arrow_scope, annotation, &HashMap::new())
            })
            .or_else(|| {
                expected_signature
                    .as_ref()
                    .map(|signature| signature.return_type)
            });
        let expected_return_order = annotation.and_then(|annotation| {
            self.property_order_for_type_node_root(file, arrow_scope, annotation)
        });
        let return_type = match body {
            ArrowBody::Expression(body) => {
                self.infer_expression(file, arrow_scope, body, expected_return)
            }
            ArrowBody::Block(statements) => {
                for statement in statements {
                    let statement_scope = self.program.files[file.0 as usize]
                        .bindings
                        .scope_for_node
                        .get(&statement.id)
                        .copied()
                        .unwrap_or(arrow_scope);
                    self.check_statement(
                        file,
                        statement_scope,
                        statement,
                        expected_return,
                        expected_return_order.as_ref(),
                    );
                }
                expected_return.unwrap_or(self.store.builtins.void)
            }
        };
        self.store.intern(TypeKind::Function(Signature {
            parameters: resolved,
            return_type,
        }))
    }

    pub(super) fn parameter_value_type(
        &mut self,
        file: FileId,
        scope: ScopeId,
        parameter: &Parameter,
    ) -> Completion<TypeId> {
        if parameter.annotation.is_some() && parameter.initializer.is_some() {
            return Completion::Deferred;
        }
        if let Some(annotation) = &parameter.annotation {
            return Completion::Complete(self.resolve_type_node(
                file,
                scope,
                annotation,
                &HashMap::new(),
            ));
        }
        parameter.initializer.as_ref().map_or(
            Completion::Complete(self.store.builtins.any),
            |initializer| self.signature_initializer_type(file, scope, initializer),
        )
    }

    pub(super) fn signature_initializer_type(
        &mut self,
        file: FileId,
        scope: ScopeId,
        initializer: &Expression,
    ) -> Completion<TypeId> {
        if matches!(
            initializer.kind,
            ExpressionKind::Literal(crate::syntax::Literal::BigInt(_))
        ) {
            return if matches!(
                self.options.target.as_str(),
                "es2020" | "es2021" | "es2022" | "es2023" | "es2024" | "es2025" | "esnext"
            ) {
                Completion::Complete(self.store.builtins.bigint)
            } else {
                Completion::Deferred
            };
        }
        if !matches!(initializer.kind, ExpressionKind::Literal(_)) {
            return Completion::Deferred;
        }
        let inferred = self.infer_expression(file, scope, initializer, None);
        Completion::Complete(self.widen(inferred))
    }

    pub(super) fn anonymous_signature_parameters(
        &mut self,
        file: FileId,
        scope: ScopeId,
        parameters: &[Parameter],
        type_parameters: &HashMap<String, TypeId>,
    ) -> Completion<Vec<ParameterType>> {
        let mut resolved = Vec::with_capacity(parameters.len());
        for parameter in parameters {
            let ty = if let Some(annotation) = &parameter.annotation {
                self.resolve_type_node(file, scope, annotation, type_parameters)
            } else if let Some(initializer) = &parameter.initializer {
                match self.signature_initializer_type(file, scope, initializer) {
                    Completion::Complete(ty) => ty,
                    Completion::Deferred => return Completion::Deferred,
                    Completion::Cycle => return Completion::Cycle,
                    Completion::Limit => return Completion::Limit,
                }
            } else {
                self.store.builtins.any
            };
            resolved.push(ParameterType {
                name: parameter.name.clone(),
                ty,
                optional: parameter.optional || parameter.initializer.is_some(),
                rest: parameter.rest,
            });
        }
        Completion::Complete(resolved)
    }

    pub(super) fn function_type(
        &mut self,
        id: DeclId,
        declaration: &FunctionDeclaration,
        scope: ScopeId,
    ) -> Completion<TypeId> {
        if !declaration.has_body && !declaration.declared {
            return Completion::Deferred;
        }
        let mut type_parameters = HashMap::new();
        let mut seen = HashSet::new();
        for (index, parameter) in declaration.type_parameters.iter().enumerate() {
            let ty = self.store.intern(TypeKind::TypeParameter {
                declaration: id,
                index: index as u32,
                name: parameter.name.clone(),
            });
            if seen.insert(parameter.name.as_str()) {
                type_parameters.insert(parameter.name.clone(), ty);
            }
        }
        let mut parameters = Vec::with_capacity(declaration.parameters.len());
        for parameter in &declaration.parameters {
            if parameter.annotation.is_some() && parameter.initializer.is_some() {
                return Completion::Deferred;
            }
            let ty = if let Some(annotation) = &parameter.annotation {
                if declaration.has_body && annotation.contains_type_query() {
                    return Completion::Deferred;
                }
                self.resolve_type_node(id.file, scope, annotation, &type_parameters)
            } else if let Some(initializer) = &parameter.initializer {
                match self.signature_initializer_type(id.file, scope, initializer) {
                    Completion::Complete(ty) => ty,
                    Completion::Deferred => return Completion::Deferred,
                    Completion::Cycle => return Completion::Cycle,
                    Completion::Limit => return Completion::Limit,
                }
            } else {
                self.store.builtins.any
            };
            parameters.push(ParameterType {
                name: parameter.name.clone(),
                ty,
                optional: parameter.optional || parameter.initializer.is_some(),
                rest: parameter.rest,
            });
        }
        let return_type = if let Some(return_type) = &declaration.return_type {
            if declaration.has_body && return_type.contains_type_query() {
                return Completion::Deferred;
            }
            self.resolve_type_node(id.file, scope, return_type, &type_parameters)
        } else {
            match self.infer_function_return(id, declaration, scope) {
                Completion::Complete(return_type) => return_type,
                Completion::Deferred => return Completion::Deferred,
                Completion::Cycle => return Completion::Cycle,
                Completion::Limit => return Completion::Limit,
            }
        };
        Completion::Complete(self.store.intern(TypeKind::Function(Signature {
            parameters,
            return_type,
        })))
    }

    fn infer_function_return(
        &mut self,
        id: DeclId,
        declaration: &FunctionDeclaration,
        scope: ScopeId,
    ) -> Completion<TypeId> {
        if declaration.declared || declaration.is_async {
            return Completion::Deferred;
        }
        let mut analysis = ReturnAnalysis {
            sites: Vec::new(),
            supported: true,
        };
        collect_return_sites(&declaration.body, &mut analysis);
        if !analysis.supported {
            return Completion::Deferred;
        }
        if analysis.sites.is_empty() || analysis.sites.iter().all(|site| site.expression.is_none())
        {
            return Completion::Complete(self.store.builtins.void);
        }
        if !block_definitely_returns(&declaration.body) {
            return Completion::Deferred;
        }

        let mut return_types = Vec::with_capacity(analysis.sites.len());
        for site in analysis.sites {
            let Some(expression) = site.expression else {
                return_types.push(self.store.builtins.undefined);
                continue;
            };
            let expression_scope = self.program.files[id.file.0 as usize]
                .bindings
                .scope_for_node
                .get(&site.statement.id)
                .copied()
                .unwrap_or(scope);
            let inferred = self.infer_expression(id.file, expression_scope, expression, None);
            let Some(inferred) = self.complete_type(inferred) else {
                return Completion::Deferred;
            };
            if !bounded_inferred_return(self.store.kind(inferred)) {
                return Completion::Deferred;
            }
            return_types.push(self.widen(inferred));
        }
        Completion::Complete(self.store.union(return_types, UnionPolicy::Canonical))
    }
}

const fn bounded_inferred_return(kind: &TypeKind) -> bool {
    matches!(
        kind,
        TypeKind::Any
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
            | TypeKind::Array(_)
            | TypeKind::Tuple(_)
            | TypeKind::ClassInstance { .. }
            | TypeKind::ClassConstructor { .. }
    )
}

fn collect_return_sites<'a>(statements: &'a [Statement], analysis: &mut ReturnAnalysis<'a>) {
    for statement in statements {
        match &statement.kind {
            StatementKind::Return(expression) => analysis.sites.push(ReturnSite {
                statement,
                expression: expression.as_ref(),
            }),
            StatementKind::Block(statements) => collect_return_sites(statements, analysis),
            StatementKind::If(control_flow) => {
                collect_return_sites(
                    std::slice::from_ref(control_flow.then_statement.as_ref()),
                    analysis,
                );
                if let Some(else_statement) = &control_flow.else_statement {
                    collect_return_sites(std::slice::from_ref(else_statement.as_ref()), analysis);
                }
            }
            StatementKind::Switch(_) | StatementKind::Unknown => analysis.supported = false,
            StatementKind::Import(_)
            | StatementKind::Export(_)
            | StatementKind::Variable(_)
            | StatementKind::Function(_)
            | StatementKind::Class(_)
            | StatementKind::TypeAlias(_)
            | StatementKind::Interface(_)
            | StatementKind::Break(_)
            | StatementKind::Continue(_)
            | StatementKind::Expression(_)
            | StatementKind::Empty => {}
        }
    }
}

fn block_definitely_returns(statements: &[Statement]) -> bool {
    statements.iter().any(statement_definitely_returns)
}

fn statement_definitely_returns(statement: &Statement) -> bool {
    match &statement.kind {
        StatementKind::Return(_) => true,
        StatementKind::Block(statements) => block_definitely_returns(statements),
        StatementKind::If(control_flow) => {
            let Some(else_statement) = &control_flow.else_statement else {
                return false;
            };
            statement_definitely_returns(&control_flow.then_statement)
                && statement_definitely_returns(else_statement)
        }
        StatementKind::Import(_)
        | StatementKind::Export(_)
        | StatementKind::Variable(_)
        | StatementKind::Function(_)
        | StatementKind::Class(_)
        | StatementKind::TypeAlias(_)
        | StatementKind::Interface(_)
        | StatementKind::Switch(_)
        | StatementKind::Break(_)
        | StatementKind::Continue(_)
        | StatementKind::Expression(_)
        | StatementKind::Empty
        | StatementKind::Unknown => false,
    }
}
