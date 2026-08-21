//! Incremental service boundary. The service owns source revisions and asks the
//! compiler for semantic values; it does not own type algorithms.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::diagnostics::Diagnostic;
use crate::program::{CompileOutput, Compiler, CompilerOptions, SemanticCompletion, SourceInput};
use crate::source::Span;
use crate::syntax::{ClassMemberKind, Statement, StatementKind, VariableKind, parse_source};

mod display;
mod navigation;

use display::{
    display_parameter, display_parameter_type, display_type_node, display_variable_type,
};

pub use navigation::{
    DefinitionAndBoundSpan, DefinitionInfo, DocumentHighlights, HighlightSpan, ReferenceEntry,
    ReferencedSymbol, ReferencedSymbolDefinition, RenameInfo, RenameLocation, RenameResult,
    SymbolDisplayPart,
};

#[derive(Debug, Clone)]
struct OpenFile {
    text: Arc<str>,
    version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickInfo {
    pub kind: String,
    pub text_span: TextSpan,
    pub display: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextSpan {
    pub start: u32,
    pub length: u32,
}

/// Semantic diagnostics together with the checked program's completion truth.
///
/// An empty diagnostic list is a definitive answer only when
/// `semantic_completion` is `Complete`.
#[derive(Debug, Clone)]
#[must_use = "semantic diagnostics are definitive only with their completion verdict"]
pub struct SemanticDiagnosticResult {
    pub diagnostics: Vec<Diagnostic>,
    pub semantic_completion: SemanticCompletion,
}

#[derive(Debug, Default)]
pub struct LanguageService {
    compiler: Compiler,
    options: CompilerOptions,
    files: BTreeMap<String, OpenFile>,
}

impl LanguageService {
    #[must_use]
    pub const fn new(options: CompilerOptions) -> Self {
        Self {
            compiler: Compiler::new(),
            options,
            files: BTreeMap::new(),
        }
    }

    pub fn configure(&mut self, options: CompilerOptions) {
        self.options = options;
    }

    pub fn open(&mut self, path: impl Into<String>, text: impl Into<Arc<str>>) {
        self.files.insert(
            normalize_path(&path.into()),
            OpenFile {
                text: text.into(),
                version: 1,
            },
        );
    }

    pub fn change(&mut self, path: &str, text: impl Into<Arc<str>>) -> bool {
        let Some(file) = self.files.get_mut(&normalize_path(path)) else {
            return false;
        };
        file.text = text.into();
        file.version += 1;
        true
    }

    pub fn close(&mut self, path: &str) -> bool {
        self.files.remove(&normalize_path(path)).is_some()
    }

    pub fn reset(&mut self) {
        self.files.clear();
    }

    #[must_use]
    pub fn text(&self, path: &str) -> Option<Arc<str>> {
        self.files
            .get(&normalize_path(path))
            .map(|file| Arc::clone(&file.text))
    }

    #[must_use]
    pub fn version(&self, path: &str) -> Option<u64> {
        self.files
            .get(&normalize_path(path))
            .map(|file| file.version)
    }

    pub fn compile(&self) -> CompileOutput {
        let inputs = self
            .files
            .iter()
            .map(|(path, file)| SourceInput::new(path, Arc::clone(&file.text)))
            .collect();
        self.compiler.compile(inputs, &self.options)
    }

    pub fn syntactic_diagnostics(&self, path: &str) -> Vec<Diagnostic> {
        self.diagnostics(path, |code| code < 2000)
    }

    pub fn semantic_diagnostics(&self, path: &str) -> SemanticDiagnosticResult {
        let normalized = normalize_path(path);
        let output = self.compile();
        SemanticDiagnosticResult {
            diagnostics: output
                .diagnostics
                .into_iter()
                .filter(|diagnostic| normalize_path(&diagnostic.file) == normalized)
                .filter(|diagnostic| diagnostic.code >= 2000)
                .collect(),
            semantic_completion: output.semantic_completion,
        }
    }

    fn diagnostics(&self, path: &str, include: impl Fn(u32) -> bool) -> Vec<Diagnostic> {
        let normalized = normalize_path(path);
        self.compile()
            .diagnostics
            .into_iter()
            .filter(|diagnostic| normalize_path(&diagnostic.file) == normalized)
            .filter(|diagnostic| include(diagnostic.code))
            .collect()
    }

    /// A small R0 quick-info surface for declarations represented by the new
    /// syntax tree. Unsupported syntax returns `None` instead of a fabricated
    /// semantic result.
    pub fn quick_info(&self, path: &str, offset: u32) -> Option<QuickInfo> {
        let normalized = normalize_path(path);
        let file = self.files.get(&normalized)?;
        let source = crate::source::SourceText::new(
            crate::source::FileId(0),
            normalized.into(),
            Arc::clone(&file.text),
        );
        let parsed = parse_source(&source);
        quick_info_in_statements(&parsed.unit.statements, offset)
    }

    /// Resolve the declaration at `offset` together with the token span that
    /// was bound. Unsupported syntax returns `None`; it never fabricates a
    /// same-spelling result.
    pub fn definition_and_bound_span(
        &self,
        path: &str,
        offset: u32,
    ) -> Option<DefinitionAndBoundSpan> {
        navigation::NavigationIndex::build(self.compile().program).definition(path, offset)
    }

    /// Find references through the same declaration identity used by
    /// definition lookup.
    pub fn references(&self, path: &str, offset: u32) -> Vec<ReferencedSymbol> {
        navigation::NavigationIndex::build(self.compile().program).references(path, offset)
    }

    /// Return identity-based highlights, restricted to the requested files.
    pub fn document_highlights(
        &self,
        path: &str,
        offset: u32,
        files_to_search: &[String],
    ) -> Vec<DocumentHighlights> {
        navigation::NavigationIndex::build(self.compile().program).document_highlights(
            path,
            offset,
            files_to_search,
        )
    }

    /// Return the rename trigger and all locations for the resolved symbol.
    pub fn rename(&self, path: &str, offset: u32) -> RenameResult {
        navigation::NavigationIndex::build(self.compile().program).rename(path, offset)
    }
}

fn quick_info_in_statements(statements: &[Statement], offset: u32) -> Option<QuickInfo> {
    for statement in statements {
        match &statement.kind {
            StatementKind::Variable(declaration) if contains(declaration.name_span, offset) => {
                let annotation = display_variable_type(declaration)?;
                let declaration_kind = match declaration.declaration_kind {
                    VariableKind::Const => "const",
                    VariableKind::Let => "let",
                    VariableKind::Var => "var",
                };
                return Some(QuickInfo {
                    kind: declaration_kind.to_string(),
                    text_span: text_span(declaration.name_span),
                    display: format!("{declaration_kind} {}: {annotation}", declaration.name),
                });
            }
            StatementKind::Function(declaration) if contains(declaration.name_span, offset) => {
                if !declaration.type_parameters.is_empty() {
                    return None;
                }
                let parameters = declaration
                    .parameters
                    .iter()
                    .map(display_parameter)
                    .collect::<Option<Vec<_>>>()?
                    .join(", ");
                let result = display_type_node(declaration.return_type.as_ref()?)?;
                return Some(QuickInfo {
                    kind: "function".to_string(),
                    text_span: text_span(declaration.name_span),
                    display: format!("function {}({parameters}): {result}", declaration.name),
                });
            }
            StatementKind::TypeAlias(declaration) if contains(declaration.name_span, offset) => {
                if !declaration.type_parameters.is_empty() {
                    return None;
                }
                return Some(QuickInfo {
                    kind: "type".to_string(),
                    text_span: text_span(declaration.name_span),
                    display: format!(
                        "type {} = {}",
                        declaration.name,
                        display_type_node(&declaration.ty)?
                    ),
                });
            }
            StatementKind::Interface(declaration) if contains(declaration.name_span, offset) => {
                if !declaration.type_parameters.is_empty() {
                    return None;
                }
                return Some(QuickInfo {
                    kind: "interface".to_string(),
                    text_span: text_span(declaration.name_span),
                    display: format!("interface {}", declaration.name),
                });
            }
            StatementKind::Function(declaration) => {
                if let Some(info) = quick_info_in_statements(&declaration.body, offset) {
                    return Some(info);
                }
            }
            StatementKind::Class(declaration) => {
                for member in &declaration.members {
                    match &member.kind {
                        ClassMemberKind::Constructor { body, .. }
                        | ClassMemberKind::Method { body, .. } => {
                            if let Some(info) = quick_info_in_statements(body, offset) {
                                return Some(info);
                            }
                        }
                        ClassMemberKind::Property { .. } => {}
                    }
                }
            }
            StatementKind::Block(statements) => {
                if let Some(info) = quick_info_in_statements(statements, offset) {
                    return Some(info);
                }
            }
            StatementKind::If(control_flow) => {
                if let Some(info) = quick_info_in_statements(
                    std::slice::from_ref(control_flow.then_statement.as_ref()),
                    offset,
                ) {
                    return Some(info);
                }
                if let Some(else_statement) = &control_flow.else_statement
                    && let Some(info) = quick_info_in_statements(
                        std::slice::from_ref(else_statement.as_ref()),
                        offset,
                    )
                {
                    return Some(info);
                }
            }
            StatementKind::Switch(control_flow) => {
                for clause in &control_flow.clauses {
                    if let Some(info) = quick_info_in_statements(&clause.statements, offset) {
                        return Some(info);
                    }
                }
            }
            StatementKind::Import(_)
            | StatementKind::Export(_)
            | StatementKind::Break(_)
            | StatementKind::Continue(_)
            | StatementKind::Return(_)
            | StatementKind::Expression(_)
            | StatementKind::Empty
            | StatementKind::Unknown
            | StatementKind::Variable(_)
            | StatementKind::TypeAlias(_)
            | StatementKind::Interface(_) => {}
        }
    }
    None
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

const fn contains(span: Span, offset: u32) -> bool {
    span.start <= offset && offset <= span.end
}

const fn text_span(span: Span) -> TextSpan {
    TextSpan {
        start: span.start,
        length: span.len(),
    }
}
