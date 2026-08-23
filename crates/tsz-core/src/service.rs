//! Incremental service boundary. The service owns source revisions and asks the
//! compiler for semantic values; it does not own type algorithms.

use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::diagnostics::Diagnostic;
use crate::program::{
    CapabilityScope, CapabilityTarget, CompileOutput, Compiler, CompilerOptions, ProgramFile,
    SemanticCompletion, SourceInput,
};
use crate::source::Span;
use crate::syntax::{ClassMemberKind, Statement, StatementKind, VariableKind};

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
    /// One compiled snapshot for the current options plus exact open-file
    /// revisions. Every mutation owner invalidates it; incomplete snapshots
    /// remain reusable because capability verdicts are part of the value.
    compiled_snapshot: RefCell<Option<CompileOutput>>,
}

impl LanguageService {
    #[must_use]
    pub const fn new(options: CompilerOptions) -> Self {
        Self {
            compiler: Compiler::new(),
            options,
            files: BTreeMap::new(),
            compiled_snapshot: RefCell::new(None),
        }
    }

    pub fn configure(&mut self, options: CompilerOptions) {
        self.options = options;
        self.compiled_snapshot.get_mut().take();
    }

    pub fn open(&mut self, path: impl Into<String>, text: impl Into<Arc<str>>) {
        self.files.insert(
            normalize_path(&path.into()),
            OpenFile {
                text: text.into(),
                version: 1,
            },
        );
        self.compiled_snapshot.get_mut().take();
    }

    pub fn change(&mut self, path: &str, text: impl Into<Arc<str>>) -> bool {
        let Some(file) = self.files.get_mut(&normalize_path(path)) else {
            return false;
        };
        file.text = text.into();
        file.version += 1;
        self.compiled_snapshot.get_mut().take();
        true
    }

    pub fn close(&mut self, path: &str) -> bool {
        let removed = self.files.remove(&normalize_path(path)).is_some();
        if removed {
            self.compiled_snapshot.get_mut().take();
        }
        removed
    }

    pub fn reset(&mut self) {
        self.files.clear();
        self.compiled_snapshot.get_mut().take();
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
        self.compile_uncached()
    }

    fn compile_uncached(&self) -> CompileOutput {
        let inputs = self
            .files
            .iter()
            .map(|(path, file)| SourceInput::new(path, Arc::clone(&file.text)))
            .collect();
        self.compiler.compile(inputs, &self.options)
    }

    fn with_compiled_snapshot<R>(&self, query: impl FnOnce(&CompileOutput) -> R) -> R {
        if self.compiled_snapshot.borrow().is_none() {
            let output = self.compile_uncached();
            self.compiled_snapshot.borrow_mut().replace(output);
        }
        let snapshot = self.compiled_snapshot.borrow();
        query(snapshot.as_ref().expect("compiled snapshot"))
    }

    pub fn syntactic_diagnostics(&self, path: &str) -> Vec<Diagnostic> {
        self.diagnostics(path, |code| code < 2000)
    }

    pub fn semantic_diagnostics(&self, path: &str) -> SemanticDiagnosticResult {
        let normalized = normalize_path(path);
        self.with_compiled_snapshot(|output| {
            let file = output.program.files.iter().find(|file| {
                normalize_path(&file.source.path.to_string_lossy()) == normalized
                    || normalize_path(&file.source.host_path.to_string_lossy()) == normalized
            });
            let semantic_completion = file.map_or(SemanticCompletion::Deferred, |file| {
                if output
                    .capabilities
                    .semantic_diagnostics_file_is_claimed(file.source.id)
                {
                    output
                        .check_file_completions
                        .get(file.source.id.0 as usize)
                        .copied()
                        .unwrap_or(SemanticCompletion::Deferred)
                } else {
                    SemanticCompletion::Deferred
                }
            });
            SemanticDiagnosticResult {
                diagnostics: output
                    .diagnostics
                    .iter()
                    .filter(|diagnostic| normalize_path(&diagnostic.file) == normalized)
                    .filter(|diagnostic| diagnostic.code >= 2000)
                    .cloned()
                    .collect(),
                semantic_completion,
            }
        })
    }

    fn diagnostics(&self, path: &str, include: impl Fn(u32) -> bool) -> Vec<Diagnostic> {
        let normalized = normalize_path(path);
        self.with_compiled_snapshot(|output| {
            output
                .diagnostics
                .iter()
                .filter(|diagnostic| normalize_path(&diagnostic.file) == normalized)
                .filter(|diagnostic| include(diagnostic.code))
                .cloned()
                .collect()
        })
    }

    /// A small R0 quick-info surface for declarations represented by the new
    /// syntax tree. Unsupported syntax returns `None` instead of a fabricated
    /// semantic result.
    pub fn quick_info(&self, path: &str, offset: u32) -> Option<QuickInfo> {
        let normalized = normalize_path(path);
        self.files.get(&normalized)?;
        self.with_compiled_snapshot(|output| {
            let file = output.program.files.iter().find(|file| {
                normalize_path(&file.source.path.to_string_lossy()) == normalized
                    || normalize_path(&file.source.host_path.to_string_lossy()) == normalized
            })?;
            if !output
                .capabilities
                .claim(
                    CapabilityTarget::QuickInfo,
                    capability_scope_at(file, offset)?,
                )
                .is_claimed()
            {
                return None;
            }
            quick_info_in_statements(&file.syntax.statements, offset)
        })
    }

    /// Resolve the declaration at `offset` together with the token span that
    /// was bound. Unsupported syntax returns `None`; it never fabricates a
    /// same-spelling result.
    pub fn definition_and_bound_span(
        &self,
        path: &str,
        offset: u32,
    ) -> Option<DefinitionAndBoundSpan> {
        self.with_compiled_snapshot(|output| {
            if !service_operation_claimed(output, path, offset, CapabilityTarget::Definition) {
                return None;
            }
            let result =
                navigation::NavigationIndex::build(&output.program).definition(path, offset)?;
            definition_result_is_claimed(output, &result).then_some(result)
        })
    }

    /// Find references through the same declaration identity used by
    /// definition lookup.
    pub fn references(&self, path: &str, offset: u32) -> Vec<ReferencedSymbol> {
        self.with_compiled_snapshot(|output| {
            if !service_operation_claimed(output, path, offset, CapabilityTarget::References) {
                return Vec::new();
            }
            let result =
                navigation::NavigationIndex::build(&output.program).references(path, offset);
            if references_result_is_claimed(output, &result) {
                result
            } else {
                Vec::new()
            }
        })
    }

    /// Return identity-based highlights, restricted to the requested files.
    pub fn document_highlights(
        &self,
        path: &str,
        offset: u32,
        files_to_search: &[String],
    ) -> Vec<DocumentHighlights> {
        self.with_compiled_snapshot(|output| {
            if !service_operation_claimed(output, path, offset, CapabilityTarget::Highlights) {
                return Vec::new();
            }
            let result = navigation::NavigationIndex::build(&output.program).document_highlights(
                path,
                offset,
                files_to_search,
            );
            if highlights_result_is_claimed(output, &result) {
                result
            } else {
                Vec::new()
            }
        })
    }

    /// Return the rename trigger and all locations for the resolved symbol.
    pub fn rename(&self, path: &str, offset: u32) -> RenameResult {
        self.with_compiled_snapshot(|output| {
            if !service_operation_claimed(output, path, offset, CapabilityTarget::Rename) {
                return RenameResult::failure();
            }
            let result = navigation::NavigationIndex::build(&output.program).rename(path, offset);
            if rename_result_is_claimed(output, &result) {
                result
            } else {
                RenameResult::failure()
            }
        })
    }
}

fn definition_result_is_claimed(output: &CompileOutput, result: &DefinitionAndBoundSpan) -> bool {
    result.definitions.iter().all(|definition| {
        service_location_claimed(
            output,
            &definition.file_name,
            definition.text_span,
            CapabilityTarget::Definition,
        )
    })
}

fn references_result_is_claimed(output: &CompileOutput, result: &[ReferencedSymbol]) -> bool {
    result.iter().all(|symbol| {
        service_location_claimed(
            output,
            &symbol.definition.file_name,
            symbol.definition.text_span,
            CapabilityTarget::References,
        ) && symbol.references.iter().all(|reference| {
            service_location_claimed(
                output,
                &reference.file_name,
                reference.text_span,
                CapabilityTarget::References,
            )
        })
    })
}

fn highlights_result_is_claimed(output: &CompileOutput, result: &[DocumentHighlights]) -> bool {
    result.iter().all(|document| {
        document.highlight_spans.iter().all(|highlight| {
            service_location_claimed(
                output,
                &document.file_name,
                highlight.text_span,
                CapabilityTarget::Highlights,
            )
        })
    })
}

fn rename_result_is_claimed(output: &CompileOutput, result: &RenameResult) -> bool {
    result.locations.iter().all(|location| {
        service_location_claimed(
            output,
            &location.file_name,
            location.text_span,
            CapabilityTarget::Rename,
        )
    })
}

fn service_location_claimed(
    output: &CompileOutput,
    path: &str,
    span: TextSpan,
    target: CapabilityTarget,
) -> bool {
    let normalized = normalize_path(path);
    let Some(file) = output.program.files.iter().find(|file| {
        normalize_path(&file.source.path.to_string_lossy()) == normalized
            || normalize_path(&file.source.host_path.to_string_lossy()) == normalized
    }) else {
        return false;
    };
    output
        .capabilities
        .claim(
            target,
            capability_scope_at(file, span.start).unwrap_or(CapabilityScope::File(file.source.id)),
        )
        .is_claimed()
}

fn service_operation_claimed(
    output: &CompileOutput,
    path: &str,
    offset: u32,
    target: CapabilityTarget,
) -> bool {
    let normalized = normalize_path(path);
    let Some(file) = output.program.files.iter().find(|file| {
        normalize_path(&file.source.path.to_string_lossy()) == normalized
            || normalize_path(&file.source.host_path.to_string_lossy()) == normalized
    }) else {
        return false;
    };
    output
        .capabilities
        .claim(
            target,
            capability_scope_at(file, offset).unwrap_or(CapabilityScope::File(file.source.id)),
        )
        .is_claimed()
}

fn capability_scope_at(file: &ProgramFile, offset: u32) -> Option<CapabilityScope> {
    let mut owner = None;
    let mut best = None;
    for statement in &file.syntax.statements {
        statement.for_each_statement(&mut |statement| {
            if contains(statement.span, offset) {
                let candidate = (
                    statement.span.start != offset,
                    statement.span.len(),
                    Reverse(statement.span.start),
                );
                if best.is_none_or(|current| candidate < current) {
                    owner = Some(statement.id);
                    best = Some(candidate);
                }
            }
        });
    }
    owner.map(|owner| CapabilityScope::node(file.source.id, owner))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiled_snapshot_is_reused_and_invalidated_by_every_revision_owner() {
        let mut service = LanguageService::new(CompilerOptions::default());
        service.open(
            "case.ts",
            Arc::<str>::from("const gap = `plain`; const value: string = missing;"),
        );
        assert!(service.compiled_snapshot.get_mut().is_none());

        let first = service.semantic_diagnostics("case.ts");
        assert_eq!(first.diagnostics.len(), 1);
        assert_eq!(first.semantic_completion, SemanticCompletion::Deferred);
        assert!(service.compiled_snapshot.get_mut().is_some());
        let uncached = service.compile();
        let cached = service.compiled_snapshot.get_mut().as_ref().unwrap();
        assert_eq!(cached.semantic_completion, uncached.semantic_completion);
        assert_eq!(cached.diagnostics, uncached.diagnostics);

        service.configure(CompilerOptions {
            no_check: true,
            ..CompilerOptions::default()
        });
        assert!(service.compiled_snapshot.get_mut().is_none());
        let _ = service.semantic_diagnostics("case.ts");
        assert!(service.compiled_snapshot.get_mut().is_some());

        service.open("other.ts", Arc::<str>::from("const other = 1;"));
        assert!(service.compiled_snapshot.get_mut().is_none());
        service.quick_info("other.ts", 7);
        assert!(service.compiled_snapshot.get_mut().is_some());

        assert!(service.change("other.ts", Arc::<str>::from("const renamed = 1;")));
        assert!(service.compiled_snapshot.get_mut().is_none());
        service.quick_info("other.ts", 7);
        assert!(service.compiled_snapshot.get_mut().is_some());

        assert!(service.close("other.ts"));
        assert!(service.compiled_snapshot.get_mut().is_none());
        let _ = service.semantic_diagnostics("case.ts");
        assert!(service.compiled_snapshot.get_mut().is_some());

        service.reset();
        assert!(service.compiled_snapshot.get_mut().is_none());
    }

    #[test]
    fn capability_scope_prefers_adjacent_starts_and_nested_right_edges() {
        let adjacent = "const g = `plain`;veryLongSiblingName;const veryLongSiblingName = 1;";
        let adjacent_reference = adjacent.find("veryLongSiblingName").unwrap() as u32;
        let mut service = LanguageService::new(CompilerOptions::default());
        service.open("adjacent.ts", Arc::<str>::from(adjacent));

        let definition = service
            .definition_and_bound_span("adjacent.ts", adjacent_reference)
            .expect("the adjacent statement start must not inherit the prior nonclaim");
        assert_eq!(definition.definitions.len(), 1);
        assert_eq!(definition.definitions[0].name, "veryLongSiblingName");

        let nested = "function shell(bad: ){const sibling:string='x';sibling}";
        let nested_reference = nested.rfind("sibling").unwrap() as u32;
        let mut service = LanguageService::new(CompilerOptions::default());
        service.open("nested.ts", Arc::<str>::from(nested));
        for offset in [nested_reference, nested_reference + "sibling".len() as u32] {
            let definition = service
                .definition_and_bound_span("nested.ts", offset)
                .expect("a nested statement owns both its token and right-edge query");
            assert_eq!(definition.definitions.len(), 1);
            assert_eq!(definition.definitions[0].name, "sibling");
        }
    }
}
