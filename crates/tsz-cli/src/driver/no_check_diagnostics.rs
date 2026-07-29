//! No-check diagnostic collection for CLI check scheduling.

use super::check_file::collect_no_check_file_diagnostics;
use super::*;

pub(super) struct NoCheckDiagnosticsInput<'a> {
    pub(super) files: &'a [tsz::parallel::BoundFile],
    pub(super) file_indices: &'a [usize],
    pub(super) options: &'a ResolvedCompilerOptions,
    pub(super) program_has_real_syntax_errors: bool,
    pub(super) include_isolated_declaration_diagnostics: bool,
}

pub(super) struct NoCheckFileDiagnostics {
    pub(super) file_idx: usize,
    pub(super) diagnostics: Vec<Diagnostic>,
}

pub(super) fn collect_no_check_diagnostics_for_files(
    input: NoCheckDiagnosticsInput<'_>,
) -> Vec<NoCheckFileDiagnostics> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use rayon::prelude::*;
        tsz::parallel::ensure_rayon_global_pool();
        input
            .file_indices
            .par_iter()
            .map(|&file_idx| collect_no_check_diagnostics_for_file(&input, file_idx))
            .collect()
    }

    #[cfg(target_arch = "wasm32")]
    {
        input
            .file_indices
            .iter()
            .map(|&file_idx| collect_no_check_diagnostics_for_file(&input, file_idx))
            .collect()
    }
}

fn collect_no_check_diagnostics_for_file(
    input: &NoCheckDiagnosticsInput<'_>,
    file_idx: usize,
) -> NoCheckFileDiagnostics {
    let file = &input.files[file_idx];
    let mut diagnostics = collect_no_check_file_diagnostics(
        file,
        input.options,
        input.program_has_real_syntax_errors,
    );

    // TSC still reports the `--isolatedDeclarations` grammar diagnostics
    // (TS9007/TS9011/TS9012/etc.) under `--noCheck` because they gate
    // declaration emission, not type checking (#3709). Run only the
    // isolated-declaration grammar pass.
    if input.include_isolated_declaration_diagnostics && input.options.checker.isolated_declarations
    {
        let mut binder = tsz_binder::state::BinderState::new();
        binder.bind_source_file(&file.arena, file.source_file);
        diagnostics.extend(tsz::checker::run_isolated_declarations_pass(
            &file.arena,
            &binder,
            file.source_file,
            file.file_name.clone(),
            input.options.checker.clone(),
        ));
    }

    NoCheckFileDiagnostics {
        file_idx,
        diagnostics,
    }
}
