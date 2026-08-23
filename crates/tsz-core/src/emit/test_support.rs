use crate::config::ProjectProvenance;
use crate::emit_paths::EmitPlan;
use crate::program::{
    CapabilityAnalysis, CapabilityContext, CompilerOptions, EmittedFile, ProgramFile,
};

use super::emit_file_with_plan;

pub(super) fn emit_file(file: &ProgramFile, options: &CompilerOptions) -> Vec<EmittedFile> {
    let capabilities = CapabilityAnalysis::derive(
        std::slice::from_ref(file),
        options,
        CapabilityContext::default(),
    );
    let plan = EmitPlan::for_program(
        std::slice::from_ref(file),
        options,
        &ProjectProvenance::default(),
        &capabilities,
    );
    emit_file_with_plan(file, options, plan.for_file(file.source.id))
}
