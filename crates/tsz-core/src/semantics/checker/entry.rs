use crate::bind::ScopeId;
use crate::diagnostics::Diagnostic;
use crate::program::{CapabilityAnalysis, CompilerOptions, Program, SemanticCompletion};

use super::{Checker, relation_diagnostic::ContextualType};

#[derive(Debug)]
pub struct CheckResult {
    pub diagnostics: Vec<Diagnostic>,
    pub type_count: usize,
    pub semantic_completion: SemanticCompletion,
    pub file_semantic_completions: Vec<SemanticCompletion>,
}

pub fn check_program(
    program: &Program,
    options: &CompilerOptions,
    capabilities: &CapabilityAnalysis,
) -> CheckResult {
    Checker::new(program, options, capabilities).check()
}

impl Checker<'_> {
    fn check(mut self) -> CheckResult {
        self.require_explicit_type_positions();
        for file_id in &self.program.source_order {
            let file = &self.program.files[file_id.0 as usize];
            if !self
                .capabilities
                .semantic_check_file_is_enabled(file.source.id)
            {
                continue;
            }
            self.completion.set_current(Some(file.source.id));
            self.check_statement_list(
                file.source.id,
                ScopeId(0),
                &file.syntax.statements,
                ContextualType::Absent,
                None,
            );
        }
        self.completion.set_current(None);
        self.flush_property_diagnostics();
        self.flush_indexed_access_diagnostics();
        self.flush_construct_diagnostics();
        let semantic_completion = self.completion.program();
        let file_semantic_completions = self.completion.into_file_verdicts();
        CheckResult {
            diagnostics: self.diagnostics,
            type_count: self.store.len(),
            semantic_completion,
            file_semantic_completions,
        }
    }
}
