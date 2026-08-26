use std::sync::Arc;

use crate::bind::ScopeId;
use crate::diagnostics::Diagnostic;
use crate::program::{CapabilityAnalysis, CompilerOptions, Program, SemanticCompletion};

use super::{Checker, relation_diagnostic::ContextualType};

mod display_summary;

pub(crate) use display_summary::{
    DeclarationDisplayParts, DeclarationDisplaySummaries, DeclarationDisplaySummary,
};

#[derive(Debug)]
pub struct CheckResult {
    pub diagnostics: Vec<Diagnostic>,
    pub type_count: usize,
    pub semantic_completion: SemanticCompletion,
    pub file_semantic_completions: Vec<SemanticCompletion>,
    pub declaration_display_summaries: DeclarationDisplaySummaries,
}

pub fn check_program(
    program: &Program,
    options: &CompilerOptions,
    capabilities: &CapabilityAnalysis,
) -> CheckResult {
    Checker::new(program, options, capabilities).check()
}

pub fn summarize_program(
    program: &Program,
    options: &CompilerOptions,
    capabilities: &CapabilityAnalysis,
) -> DeclarationDisplaySummaries {
    Checker::new(program, options, capabilities).declaration_display_summaries()
}

impl Checker<'_> {
    fn check(mut self) -> CheckResult {
        for &(authored_id, library_id) in &self.program.standard_library_type_alias_collisions {
            let Some(origin) = self
                .program
                .standard_library
                .homogeneous_record_origin(library_id)
            else {
                continue;
            };
            let file = &self.program.files[authored_id.file.0 as usize];
            let Some(authored) = file.bindings.declaration(authored_id) else {
                continue;
            };
            let message = format!("Duplicate identifier '{}'.", authored.name);
            if self
                .capabilities
                .semantic_check_file_is_enabled(authored_id.file)
            {
                self.diagnostics.push(Diagnostic::at(
                    &file.source,
                    authored.name_span,
                    message.clone(),
                    2300,
                ));
            }
            let mut library = Diagnostic::error_at_text(
                origin.path.to_string(),
                origin.name_start,
                origin.name.len() as u32,
                Arc::from(origin.source),
                message,
                2300,
            );
            library.file_id = Some(library_id.file);
            self.diagnostics.push(library);
        }
        self.require_explicit_type_positions();
        for file_id in &self.program.source_order {
            let file = &self.program.files[file_id.0 as usize];
            self.completion.set_current(Some(file.source.id));
            self.check_contextual_grammar_facts(file.source.id);
            if !self
                .capabilities
                .semantic_check_file_is_enabled(file.source.id)
            {
                continue;
            }
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
        let declaration_display_summaries = self.declaration_display_summaries();
        let semantic_completion = self.completion.program();
        let file_semantic_completions = self.completion.into_file_verdicts();
        CheckResult {
            diagnostics: self.diagnostics,
            type_count: self.store.len(),
            semantic_completion,
            file_semantic_completions,
            declaration_display_summaries,
        }
    }
}
