//! Incremental service boundary. The service owns source revisions and asks the
//! compiler for semantic values; it does not own type algorithms.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::diagnostics::Diagnostic;
use crate::program::{
    CapabilityScope, CapabilityTarget, CompileOutput, Compiler, CompilerOptions, ProgramFile,
    SemanticCompletion, SourceInput,
};

mod navigation;

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
            let file = compiled_file(output, &normalized);
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
            let file = compiled_file(output, &normalized)?;
            if !output
                .capabilities
                .claim(
                    CapabilityTarget::QuickInfo,
                    file.capability_scope_at(offset)?,
                )
                .is_claimed()
            {
                return None;
            }
            navigation::NavigationIndex::build(output).quick_info(path, offset)
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
            let result = navigation::NavigationIndex::build(output).definition(path, offset)?;
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
            let result = navigation::NavigationIndex::build(output).references(path, offset);
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
            let result = navigation::NavigationIndex::build(output).document_highlights(
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
            let result = navigation::NavigationIndex::build(output).rename(path, offset);
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
        service_operation_claimed(
            output,
            &definition.file_name,
            definition.text_span.start,
            CapabilityTarget::Definition,
        )
    })
}

fn references_result_is_claimed(output: &CompileOutput, result: &[ReferencedSymbol]) -> bool {
    result.iter().all(|symbol| {
        service_operation_claimed(
            output,
            &symbol.definition.file_name,
            symbol.definition.text_span.start,
            CapabilityTarget::References,
        ) && symbol.references.iter().all(|reference| {
            service_operation_claimed(
                output,
                &reference.file_name,
                reference.text_span.start,
                CapabilityTarget::References,
            )
        })
    })
}

fn highlights_result_is_claimed(output: &CompileOutput, result: &[DocumentHighlights]) -> bool {
    result.iter().all(|document| {
        document.highlight_spans.iter().all(|highlight| {
            service_operation_claimed(
                output,
                &document.file_name,
                highlight.text_span.start,
                CapabilityTarget::Highlights,
            )
        })
    })
}

fn rename_result_is_claimed(output: &CompileOutput, result: &RenameResult) -> bool {
    result.locations.iter().all(|location| {
        service_operation_claimed(
            output,
            &location.file_name,
            location.text_span.start,
            CapabilityTarget::Rename,
        )
    })
}

fn service_operation_claimed(
    output: &CompileOutput,
    path: &str,
    offset: u32,
    target: CapabilityTarget,
) -> bool {
    let normalized = normalize_path(path);
    let Some(file) = compiled_file(output, &normalized) else {
        return false;
    };
    output
        .capabilities
        .claim(
            target,
            file.capability_scope_at(offset)
                .unwrap_or(CapabilityScope::File(file.source.id)),
        )
        .is_claimed()
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn compiled_file<'a>(output: &'a CompileOutput, normalized: &str) -> Option<&'a ProgramFile> {
    output.program.files.iter().find(|file| {
        normalize_path(&file.source.path.to_string_lossy()) == normalized
            || normalize_path(&file.source.host_path.to_string_lossy()) == normalized
    })
}

#[cfg(test)]
#[path = "../rewrite-tests/service_unit.rs"]
mod tests;
