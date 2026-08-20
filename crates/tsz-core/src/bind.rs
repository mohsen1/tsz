//! Per-file binding. This phase owns declarations and lexical scopes, and
//! intentionally performs no type computation.

use std::collections::BTreeMap;

use rustc_hash::FxHashMap;

use crate::source::{DeclId, FileId, NodeId, Span};
use crate::syntax::{FunctionDeclaration, SourceUnit, Statement, StatementKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclarationKind {
    Variable,
    Parameter,
    Function,
    TypeAlias,
    Interface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Meaning {
    Value,
    Type,
}

#[derive(Debug, Clone)]
pub struct BoundDeclaration {
    pub id: DeclId,
    pub scope: ScopeId,
    pub owner: NodeId,
    pub name: String,
    pub name_span: Span,
    pub kind: DeclarationKind,
    pub meaning: Meaning,
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub id: ScopeId,
    pub parent: Option<ScopeId>,
    pub owner: Option<NodeId>,
    pub names: BTreeMap<String, Vec<DeclId>>,
}

#[derive(Debug, Clone)]
pub struct BoundFile {
    pub file: FileId,
    pub declarations: Vec<BoundDeclaration>,
    pub scopes: Vec<Scope>,
    pub scope_for_node: FxHashMap<NodeId, ScopeId>,
}

impl BoundFile {
    #[must_use]
    pub fn declaration(&self, id: DeclId) -> Option<&BoundDeclaration> {
        (id.file == self.file)
            .then(|| self.declarations.get(id.local as usize))
            .flatten()
    }

    #[must_use]
    pub fn resolve(&self, mut scope: ScopeId, name: &str, meaning: Meaning) -> Option<DeclId> {
        loop {
            let current = self.scopes.get(scope.0 as usize)?;
            if let Some(ids) = current.names.get(name)
                && let Some(id) = ids.iter().rev().find(|id| {
                    self.declaration(**id)
                        .is_some_and(|declaration| declaration.meaning == meaning)
                })
            {
                return Some(*id);
            }
            scope = current.parent?;
        }
    }
}

pub fn bind_source(file: FileId, unit: &SourceUnit) -> BoundFile {
    let mut binder = Binder {
        file,
        declarations: Vec::new(),
        scopes: vec![Scope {
            id: ScopeId(0),
            parent: None,
            owner: None,
            names: BTreeMap::new(),
        }],
        scope_for_node: FxHashMap::default(),
    };
    for statement in &unit.statements {
        binder.bind_statement(statement, ScopeId(0));
    }
    BoundFile {
        file,
        declarations: binder.declarations,
        scopes: binder.scopes,
        scope_for_node: binder.scope_for_node,
    }
}

struct Binder {
    file: FileId,
    declarations: Vec<BoundDeclaration>,
    scopes: Vec<Scope>,
    scope_for_node: FxHashMap<NodeId, ScopeId>,
}

impl Binder {
    fn bind_statement(&mut self, statement: &Statement, scope: ScopeId) {
        self.scope_for_node.insert(statement.id, scope);
        match &statement.kind {
            StatementKind::Variable(declaration) => {
                self.declare(
                    scope,
                    statement.id,
                    &declaration.name,
                    declaration.name_span,
                    DeclarationKind::Variable,
                    Meaning::Value,
                );
            }
            StatementKind::Function(declaration) => {
                self.declare(
                    scope,
                    statement.id,
                    &declaration.name,
                    declaration.name_span,
                    DeclarationKind::Function,
                    Meaning::Value,
                );
                self.bind_function(statement.id, declaration, scope);
            }
            StatementKind::TypeAlias(declaration) => {
                self.declare(
                    scope,
                    statement.id,
                    &declaration.name,
                    declaration.name_span,
                    DeclarationKind::TypeAlias,
                    Meaning::Type,
                );
            }
            StatementKind::Interface(declaration) => {
                self.declare(
                    scope,
                    statement.id,
                    &declaration.name,
                    declaration.name_span,
                    DeclarationKind::Interface,
                    Meaning::Type,
                );
            }
            StatementKind::Block(statements) => {
                let child = self.new_scope(scope, statement.id);
                for nested in statements {
                    self.bind_statement(nested, child);
                }
            }
            StatementKind::Return(_)
            | StatementKind::Expression(_)
            | StatementKind::Empty
            | StatementKind::Unknown => {}
        }
    }

    fn bind_function(&mut self, owner: NodeId, declaration: &FunctionDeclaration, parent: ScopeId) {
        let scope = self.new_scope(parent, owner);
        self.scope_for_node.insert(owner, scope);
        for parameter in &declaration.parameters {
            self.declare(
                scope,
                owner,
                &parameter.name,
                parameter.name_span,
                DeclarationKind::Parameter,
                Meaning::Value,
            );
        }
        for statement in &declaration.body {
            self.bind_statement(statement, scope);
        }
    }

    fn new_scope(&mut self, parent: ScopeId, owner: NodeId) -> ScopeId {
        let id = ScopeId(self.scopes.len() as u32);
        self.scopes.push(Scope {
            id,
            parent: Some(parent),
            owner: Some(owner),
            names: BTreeMap::new(),
        });
        id
    }

    fn declare(
        &mut self,
        scope: ScopeId,
        owner: NodeId,
        name: &str,
        name_span: Span,
        kind: DeclarationKind,
        meaning: Meaning,
    ) -> DeclId {
        let id = DeclId {
            file: self.file,
            local: self.declarations.len() as u32,
        };
        self.declarations.push(BoundDeclaration {
            id,
            scope,
            owner,
            name: name.to_string(),
            name_span,
            kind,
            meaning,
        });
        self.scopes[scope.0 as usize]
            .names
            .entry(name.to_string())
            .or_default()
            .push(id);
        id
    }
}
