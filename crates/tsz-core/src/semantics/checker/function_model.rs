use std::collections::HashMap;

use crate::bind::ScopeId;
use crate::source::DeclId;
use crate::syntax::{Expression, FunctionDeclaration, Statement, StatementKind};

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
    pub(super) fn function_type(
        &mut self,
        id: DeclId,
        declaration: &FunctionDeclaration,
        scope: ScopeId,
    ) -> Completion<TypeId> {
        let mut type_parameters = HashMap::new();
        for (index, parameter) in declaration.type_parameters.iter().enumerate() {
            let ty = self.store.intern(TypeKind::TypeParameter {
                declaration: id,
                index: index as u32,
                name: parameter.name.clone(),
            });
            type_parameters.insert(parameter.name.clone(), ty);
        }
        let parameters = declaration
            .parameters
            .iter()
            .map(|parameter| ParameterType {
                name: parameter.name.clone(),
                ty: parameter
                    .annotation
                    .as_ref()
                    .map_or(self.store.builtins.any, |annotation| {
                        self.resolve_type_node(id.file, scope, annotation, &type_parameters)
                    }),
                optional: parameter.optional,
                rest: parameter.rest,
            })
            .collect();
        let return_type = if let Some(return_type) = &declaration.return_type {
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
