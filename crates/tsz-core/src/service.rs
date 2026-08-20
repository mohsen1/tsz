//! Incremental service boundary. The service owns source revisions and asks the
//! compiler for semantic values; it does not own type algorithms.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::diagnostics::Diagnostic;
use crate::program::{CompileOutput, Compiler, CompilerOptions, SourceInput};
use crate::source::Span;
use crate::syntax::{
    ExpressionKind, Literal, StatementKind, TypeNode, TypeNodeKind, VariableDeclaration,
    VariableKind, parse_source,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TextSpan {
    pub start: u32,
    pub length: u32,
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

    pub fn semantic_diagnostics(&self, path: &str) -> Vec<Diagnostic> {
        self.diagnostics(path, |code| code >= 2000)
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
        for statement in &parsed.unit.statements {
            match &statement.kind {
                StatementKind::Variable(declaration) if contains(declaration.name_span, offset) => {
                    let annotation = display_variable_type(declaration);
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
                    let parameters = declaration
                        .parameters
                        .iter()
                        .map(|parameter| {
                            let ty = parameter
                                .annotation
                                .as_ref()
                                .map_or("any".to_string(), display_type_node);
                            format!("{}: {ty}", parameter.name)
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    let result = declaration
                        .return_type
                        .as_ref()
                        .map_or("any".to_string(), display_type_node);
                    return Some(QuickInfo {
                        kind: "function".to_string(),
                        text_span: text_span(declaration.name_span),
                        display: format!("function {}({parameters}): {result}", declaration.name),
                    });
                }
                StatementKind::TypeAlias(declaration)
                    if contains(declaration.name_span, offset) =>
                {
                    return Some(QuickInfo {
                        kind: "type".to_string(),
                        text_span: text_span(declaration.name_span),
                        display: format!(
                            "type {} = {}",
                            declaration.name,
                            display_type_node(&declaration.ty)
                        ),
                    });
                }
                StatementKind::Interface(declaration)
                    if contains(declaration.name_span, offset) =>
                {
                    return Some(QuickInfo {
                        kind: "interface".to_string(),
                        text_span: text_span(declaration.name_span),
                        display: format!("interface {}", declaration.name),
                    });
                }
                StatementKind::Return(_)
                | StatementKind::Block(_)
                | StatementKind::Expression(_)
                | StatementKind::Empty
                | StatementKind::Unknown
                | StatementKind::Variable(_)
                | StatementKind::Function(_)
                | StatementKind::TypeAlias(_)
                | StatementKind::Interface(_) => {}
            }
        }
        None
    }
}

fn display_variable_type(declaration: &VariableDeclaration) -> String {
    if let Some(annotation) = &declaration.annotation {
        return display_type_node(annotation);
    }
    let Some(initializer) = &declaration.initializer else {
        return "any".to_string();
    };
    let ExpressionKind::Literal(literal) = &initializer.kind else {
        return "unknown".to_string();
    };
    match (declaration.declaration_kind, literal) {
        (VariableKind::Const, Literal::String(value)) => format!("\"{value}\""),
        (VariableKind::Const, Literal::Number(value)) => value.clone(),
        (VariableKind::Const, Literal::Boolean(value)) => value.to_string(),
        (_, Literal::String(_)) => "string".to_string(),
        (_, Literal::Number(_)) => "number".to_string(),
        (_, Literal::Boolean(_)) => "boolean".to_string(),
        (_, Literal::Null) => "null".to_string(),
    }
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

fn display_type_node(node: &TypeNode) -> String {
    match &node.kind {
        TypeNodeKind::Keyword(keyword) => format!("{keyword:?}").to_ascii_lowercase(),
        TypeNodeKind::Literal(crate::syntax::Literal::String(value)) => format!("\"{value}\""),
        TypeNodeKind::Literal(crate::syntax::Literal::Number(value)) => value.clone(),
        TypeNodeKind::Literal(crate::syntax::Literal::Boolean(value)) => value.to_string(),
        TypeNodeKind::Literal(crate::syntax::Literal::Null) => "null".to_string(),
        TypeNodeKind::Array(element) => format!("{}[]", display_type_node(element)),
        TypeNodeKind::Tuple(elements) => format!(
            "[{}]",
            elements
                .iter()
                .map(display_type_node)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        TypeNodeKind::Union(members) => members
            .iter()
            .map(display_type_node)
            .collect::<Vec<_>>()
            .join(" | "),
        TypeNodeKind::Intersection(members) => members
            .iter()
            .map(display_type_node)
            .collect::<Vec<_>>()
            .join(" & "),
        TypeNodeKind::Reference {
            name, arguments, ..
        } => {
            if arguments.is_empty() {
                name.clone()
            } else {
                format!(
                    "{}<{}>",
                    name,
                    arguments
                        .iter()
                        .map(display_type_node)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
        TypeNodeKind::Object(_)
        | TypeNodeKind::Function { .. }
        | TypeNodeKind::KeyOf(_)
        | TypeNodeKind::IndexedAccess { .. }
        | TypeNodeKind::Parenthesized(_)
        | TypeNodeKind::Missing => "unknown".to_string(),
    }
}
