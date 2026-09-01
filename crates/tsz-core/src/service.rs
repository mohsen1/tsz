//! Incremental service boundary. The service owns source revisions and asks the
//! compiler for semantic values; it does not own type algorithms.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::diagnostics::Diagnostic;
use crate::program::{
    CapabilityTarget as Target, CompileOutput, Compiler, CompilerOptions, ProgramFile,
    SemanticCompletion, SourceInput,
};
use crate::source::{FileId, SourceText};

mod navigation;

#[cfg(test)]
use navigation::remove_source_extension;

pub use navigation::{
    DefinitionAndBoundSpan, DefinitionInfo, DocumentHighlights, HighlightSpan, ReferenceEntry,
    ReferencedSymbol, ReferencedSymbolDefinition, RenameInfo, RenameLocation, RenameResult,
    SymbolDisplayPart,
};

#[derive(Debug, Clone)]
struct OpenFile {
    source: SourceText,
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
/// Semantic diagnostics and the checked program's completion truth.
#[derive(Debug, Clone)]
#[must_use = "semantic diagnostics are definitive only with their completion verdict"]
pub struct SemanticDiagnosticResult {
    pub diagnostics: Vec<Diagnostic>,
    pub semantic_completion: SemanticCompletion,
}
/// Syntactic diagnostics and the parser product's file-local completion truth.
#[derive(Debug, Clone)]
#[must_use = "syntactic diagnostics are definitive only with their completion verdict"]
pub struct SyntacticDiagnosticResult {
    pub diagnostics: Vec<Diagnostic>,
    pub syntactic_completion: SemanticCompletion,
}
/// Why a navigation query cannot publish a definitive answer yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationNonclaim {
    Deferred,
}
impl NavigationNonclaim {
    #[must_use]
    pub const fn completion(self) -> SemanticCompletion {
        SemanticCompletion::Deferred
    }
}
/// A value with capability truth; `Nonclaimed` is not an empty/negative answer.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "navigation values are definitive only when their query is claimed"]
pub enum ServiceQuery<T> {
    Claimed(T),
    Nonclaimed(NavigationNonclaim),
}
macro_rules! direct_navigation_queries {
    ($( $name:ident => $target:ident, $result:ty, $query:ident; )+) => {
        $(
            pub fn $name(&self, path: &str, offset: u32) -> ServiceQuery<$result> {
                self.navigation_query(Target::$target, path, offset, &[], |index| {
                    index.$query(path, offset)
                })
            }
        )+
    };
}
#[derive(Debug, Default)]
pub struct LanguageService {
    options: CompilerOptions,
    files: BTreeMap<String, OpenFile>,
    /// Reused only for the exact options/open-file revisions.
    compiled_snapshot: RefCell<Option<CompileOutput>>,
}

impl LanguageService {
    #[must_use]
    pub const fn new(options: CompilerOptions) -> Self {
        Self {
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
        let path = normalize_path(&path.into());
        self.files.insert(
            path.clone(),
            OpenFile {
                source: SourceText::new(FileId(0), path.into(), text.into()),
                version: 1,
            },
        );
        self.compiled_snapshot.get_mut().take();
    }
    pub fn change(&mut self, path: &str, text: impl Into<Arc<str>>) -> bool {
        let Some(file) = self.files.get_mut(&normalize_path(path)) else {
            return false;
        };
        file.source = SourceText::new(FileId(0), file.source.path.clone(), text.into());
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
            .map(|file| Arc::clone(&file.source.text))
    }
    #[must_use]
    pub fn source_coordinates(&self, path: &str) -> Option<&SourceText> {
        self.files
            .get(&normalize_path(path))
            .map(|file| &file.source)
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
            .map(|(path, file)| SourceInput::new(path, Arc::clone(&file.source.text)))
            .collect();
        Compiler::new().compile(inputs, &self.options)
    }
    fn with_compiled_snapshot<R>(&self, query: impl FnOnce(&CompileOutput) -> R) -> R {
        let mut snapshot = self.compiled_snapshot.borrow_mut();
        query(snapshot.get_or_insert_with(|| self.compile()))
    }
    pub fn syntactic_diagnostics(&self, path: &str) -> SyntacticDiagnosticResult {
        let normalized = normalize_path(path);
        self.with_compiled_snapshot(|output| {
            let syntactic_completion = compiled_file(output, &normalized)
                .filter(|file| {
                    output
                        .capabilities
                        .syntactic_diagnostics_file_is_claimed(file.source.id)
                })
                .map_or(SemanticCompletion::Deferred, |_| {
                    SemanticCompletion::Complete
                });
            SyntacticDiagnosticResult {
                diagnostics: file_diagnostics(&output.syntactic_diagnostics, &normalized),
                syntactic_completion,
            }
        })
    }
    pub fn semantic_diagnostics(&self, path: &str) -> SemanticDiagnosticResult {
        let normalized = normalize_path(path);
        self.with_compiled_snapshot(|output| {
            let semantic_completion = compiled_file(output, &normalized)
                .filter(|file| {
                    output
                        .capabilities
                        .semantic_diagnostics_file_is_claimed(file.source.id)
                })
                .and_then(|file| output.check_file_completions.get(file.source.id.0 as usize))
                .copied()
                .unwrap_or(SemanticCompletion::Deferred);
            SemanticDiagnosticResult {
                diagnostics: file_diagnostics(&output.semantic_diagnostics, &normalized),
                semantic_completion,
            }
        })
    }

    direct_navigation_queries! {
        quick_info => QuickInfo, Option<QuickInfo>, quick_info;
        definition_and_bound_span => Definition, Option<DefinitionAndBoundSpan>, definition;
        type_definition => TypeDefinition, Vec<DefinitionInfo>, type_definition;
        references => References, Vec<ReferencedSymbol>, references;
    }
    pub fn document_highlights(
        &self,
        path: &str,
        offset: u32,
        files_to_search: &[String],
    ) -> ServiceQuery<Vec<DocumentHighlights>> {
        self.navigation_query(Target::Highlights, path, offset, files_to_search, |index| {
            index.document_highlights(path, offset, files_to_search)
        })
    }
    pub fn rename(&self, path: &str, offset: u32) -> ServiceQuery<RenameResult> {
        self.navigation_query(Target::Rename, path, offset, &[], |index| {
            index.rename(path, offset)
        })
    }
    fn navigation_query<T>(
        &self,
        target: Target,
        path: &str,
        offset: u32,
        files_to_search: &[String],
        query: impl FnOnce(&navigation::NavigationIndex<'_>) -> T,
    ) -> ServiceQuery<T> {
        self.with_compiled_snapshot(|output| {
            let index = navigation::NavigationIndex::build(output);
            if !index.query_is_claimed(target, path, offset, files_to_search) {
                return ServiceQuery::Nonclaimed(NavigationNonclaim::Deferred);
            }
            ServiceQuery::Claimed(query(&index))
        })
    }
}
fn file_diagnostics(diagnostics: &[Diagnostic], normalized: &str) -> Vec<Diagnostic> {
    diagnostics
        .iter()
        .filter(|diagnostic| normalize_path(&diagnostic.file) == normalized)
        .cloned()
        .collect()
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
#[cfg(test)]
#[path = "../rewrite-tests/service_type_definition_unit.rs"]
mod type_definition_tests;
