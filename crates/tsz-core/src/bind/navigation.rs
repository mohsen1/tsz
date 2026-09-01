//! Immutable binder facts consumed by declaration-identity services.

use crate::source::{DeclId, Span};

use super::{BoundFile, DeclarationKind, Meaning, ScopeId};

#[derive(Debug, Clone)]
pub(crate) struct BoundReferenceFact {
    pub(crate) span: Span,
    pub(crate) is_write_access: bool,
    target: BoundReferenceTarget,
}

#[derive(Debug, Clone)]
enum BoundReferenceTarget {
    Declaration(DeclId),
    ProgramGlobal { name: String, meaning: Meaning },
}

impl BoundReferenceFact {
    /// Resolve the only reference category that is unavailable during
    /// file-local binding. Lexical and type-parameter references already carry
    /// their declaration identity.
    pub(crate) fn declaration(
        &self,
        resolve_global: impl FnOnce(&str, Meaning) -> Option<DeclId>,
    ) -> Option<DeclId> {
        match &self.target {
            BoundReferenceTarget::Declaration(declaration) => Some(*declaration),
            BoundReferenceTarget::ProgramGlobal { name, meaning } => resolve_global(name, *meaning),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct PendingReferenceFact {
    span: Span,
    scope: ScopeId,
    name: String,
    meaning: Meaning,
    is_write_access: bool,
    type_query_root: bool,
}

impl PendingReferenceFact {
    pub(super) fn name(
        name: &str,
        span: Span,
        scope: ScopeId,
        meaning: Meaning,
        is_write_access: bool,
    ) -> Self {
        Self {
            span,
            scope,
            name: name.to_string(),
            meaning,
            is_write_access,
            type_query_root: false,
        }
    }

    pub(super) fn type_query_root(name: &str, span: Span, scope: ScopeId) -> Self {
        Self {
            span,
            scope,
            name: name.to_string(),
            meaning: Meaning::Value,
            is_write_access: false,
            type_query_root: true,
        }
    }
}

pub(super) fn finish(
    bound: &BoundFile,
    pending: Vec<PendingReferenceFact>,
) -> Vec<BoundReferenceFact> {
    pending
        .into_iter()
        .map(|reference| {
            let local = if reference.type_query_root {
                bound
                    .resolve(reference.scope, &reference.name, Meaning::Value)
                    .or_else(|| {
                        bound
                            .resolve(reference.scope, &reference.name, Meaning::Type)
                            .filter(|declaration| {
                                bound.declaration(*declaration).is_some_and(|declaration| {
                                    declaration.kind == DeclarationKind::Import
                                })
                            })
                    })
            } else {
                bound.resolve(reference.scope, &reference.name, reference.meaning)
            };
            BoundReferenceFact {
                span: reference.span,
                is_write_access: reference.is_write_access,
                target: local.map_or_else(
                    || BoundReferenceTarget::ProgramGlobal {
                        name: reference.name,
                        meaning: reference.meaning,
                    },
                    BoundReferenceTarget::Declaration,
                ),
            }
        })
        .collect()
}
