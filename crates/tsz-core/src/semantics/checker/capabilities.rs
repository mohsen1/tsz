use crate::bind::{Meaning, ScopeId};
use crate::program::SemanticCompletion;
use crate::semantics::types::Completion;
use crate::source::{DeclId, FileId};

use super::{Checker, DiagnosticIdentity};

pub(super) struct CompletionTracker {
    program: SemanticCompletion,
    current_demand: Option<FileId>,
    files: Vec<SemanticCompletion>,
    captures: Vec<SemanticCompletion>,
}

impl CompletionTracker {
    pub(super) fn new(file_count: usize) -> Self {
        Self {
            program: SemanticCompletion::Complete,
            current_demand: None,
            files: vec![SemanticCompletion::Complete; file_count],
            captures: Vec::new(),
        }
    }

    pub(super) fn begin_capture(&mut self) {
        self.captures.push(SemanticCompletion::Complete);
    }

    pub(super) fn finish_capture(&mut self) -> SemanticCompletion {
        let captured = self
            .captures
            .pop()
            .expect("completion capture must be balanced");
        if let Some(parent) = self.captures.last_mut() {
            *parent = parent.combine(captured);
        }
        captured
    }

    fn capture(&mut self, observed: SemanticCompletion) {
        if let Some(capture) = self.captures.last_mut() {
            *capture = capture.combine(observed);
        }
    }

    pub(super) const fn set_current(&mut self, current: Option<FileId>) {
        self.current_demand = current;
    }

    fn observe_current(&mut self, observed: SemanticCompletion) {
        self.capture(observed);
        self.program = self.program.combine(observed);
        if let Some(file) = self.current_demand
            && let Some(completion) = self.files.get_mut(file.0 as usize)
        {
            *completion = completion.combine(observed);
        }
    }

    fn observe_file(&mut self, file: FileId, observed: SemanticCompletion) {
        self.capture(observed);
        self.program = self.program.combine(observed);
        if let Some(completion) = self.files.get_mut(file.0 as usize) {
            *completion = completion.combine(observed);
        }
        if self.current_demand != Some(file)
            && let Some(current) = self.current_demand
            && let Some(completion) = self.files.get_mut(current.0 as usize)
        {
            *completion = completion.combine(observed);
        }
    }

    pub(super) const fn program(&self) -> SemanticCompletion {
        self.program
    }

    pub(super) fn into_file_verdicts(self) -> Vec<SemanticCompletion> {
        self.files
    }
}

impl Checker<'_> {
    pub(super) fn record_semantic_diagnostic(
        &mut self,
        file: FileId,
        start: u32,
        code: u32,
        identity: DiagnosticIdentity,
    ) -> bool {
        self.capabilities.semantic_check_file_is_enabled(file)
            && self.reported.insert((file, start, code, identity))
    }

    pub(super) fn resolve_name(
        &self,
        file: FileId,
        scope: ScopeId,
        name: &str,
        meaning: Meaning,
    ) -> Option<DeclId> {
        self.program.files[file.0 as usize]
            .bindings
            .resolve(scope, name, meaning)
            .or_else(|| self.program.resolve_global(name, meaning))
    }

    pub(super) fn resolve_semantic_name(
        &mut self,
        file: FileId,
        scope: ScopeId,
        name: &str,
        meaning: Meaning,
    ) -> Option<DeclId> {
        let declaration = self.resolve_name(file, scope, name, meaning)?;
        self.observe_semantic_declaration(file, declaration);
        Some(declaration)
    }

    pub(super) fn observe_semantic_declaration(&mut self, file: FileId, declaration: DeclId) {
        if !self.semantic_declaration_is_claimed(declaration) {
            self.observe_file_completion(file, SemanticCompletion::Deferred);
        }
    }

    /// Aggregate only results that escape at a required checking boundary.
    /// The active root file receives the same verdict so service consumers do
    /// not republish an unrelated root's incompleteness.
    pub(super) fn require_completion<T>(&mut self, completion: Completion<T>) -> Completion<T> {
        let observed = match &completion {
            Completion::Complete(_) => SemanticCompletion::Complete,
            Completion::Deferred => SemanticCompletion::Deferred,
            Completion::Cycle => SemanticCompletion::Cycle,
            Completion::Limit => SemanticCompletion::Limit,
        };
        self.observe_completion(observed);
        completion
    }

    pub(super) fn require_file_completion<T>(
        &mut self,
        file: FileId,
        completion: Completion<T>,
    ) -> Completion<T> {
        let observed = match &completion {
            Completion::Complete(_) => SemanticCompletion::Complete,
            Completion::Deferred => SemanticCompletion::Deferred,
            Completion::Cycle => SemanticCompletion::Cycle,
            Completion::Limit => SemanticCompletion::Limit,
        };
        self.observe_file_completion(file, observed);
        completion
    }

    pub(super) fn observe_completion(&mut self, observed: SemanticCompletion) {
        self.completion.observe_current(observed);
    }

    pub(super) fn observe_file_completion(&mut self, file: FileId, observed: SemanticCompletion) {
        self.completion.observe_file(file, observed);
    }

    pub(super) fn collect_models(&mut self) {
        for file_id in &self.program.source_order {
            let file = &self.program.files[file_id.0 as usize];
            for statement in &file.syntax.statements {
                self.collect_statement_model(file.source.id, statement);
            }
        }
    }

    /// A semantic demand is claimable only when its declaration and every
    /// binder-owned peer in the same meaning group have modeled owners.
    pub(super) fn semantic_declaration_is_claimed(&self, id: DeclId) -> bool {
        if self.program.standard_library_declaration(id).is_some() {
            return true;
        }
        self.capabilities
            .semantic_declaration_is_claimed(&self.program.files, id)
    }
}
