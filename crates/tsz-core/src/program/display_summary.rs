//! Checker-produced value artifacts shared by program consumers.
use super::SemanticCompletion;
use crate::source::{DeclId, FileId, NodeId, Span};
use std::collections::BTreeMap;
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
#[derive(Debug, Clone, Default)]
pub(crate) struct DeclarationDisplaySummaries {
    declarations: BTreeMap<DeclId, DeclarationDisplaySummary>,
    default_exports: BTreeMap<(FileId, NodeId), DefaultExportDeclaration>,
}
#[derive(Debug, Clone)]
pub(crate) enum DefaultExportDeclaration {
    Literal,
    Typed {
        ty: RenderedType,
        preferred_name: Option<String>,
        dependencies: Vec<DeclId>,
    },
}
impl DeclarationDisplaySummaries {
    pub(crate) const fn new() -> Self {
        Self {
            declarations: BTreeMap::new(),
            default_exports: BTreeMap::new(),
        }
    }
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.declarations.is_empty() && self.default_exports.is_empty()
    }
    pub(crate) fn insert(&mut self, id: DeclId, summary: DeclarationDisplaySummary) {
        self.declarations.insert(id, summary);
    }
    pub(crate) fn get(&self, id: &DeclId) -> Option<&DeclarationDisplaySummary> {
        self.declarations.get(id)
    }
    pub(crate) fn insert_default_export(
        &mut self,
        file: FileId,
        statement: NodeId,
        declaration: DefaultExportDeclaration,
    ) {
        self.default_exports.insert((file, statement), declaration);
    }
    pub(crate) fn default_export(
        &self,
        file: FileId,
        statement: NodeId,
    ) -> Option<&DefaultExportDeclaration> {
        self.default_exports.get(&(file, statement))
    }
}
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
    pub quick_info_completion: SemanticCompletion,
    /// Checker-owned stable type declarations published by `TypeDefinition`.
    pub type_definition_targets: Vec<DeclId>,
    pub type_definition_completion: SemanticCompletion,
    /// Completion of the checker-derived definition display published by References.
    /// Identity-only navigation products do not consume this dependency.
    pub references_completion: SemanticCompletion,
    /// Checker-owned complete type text consumed by declaration emit.
    pub declaration_type: Option<RenderedType>,
}
