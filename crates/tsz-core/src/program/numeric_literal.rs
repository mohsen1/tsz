use crate::syntax::numeric_recovery_family;

use super::literal_products::{
    LiteralProductFamily, exact_option_value, roots_are_homogeneous_literal_products,
};
use super::{CompilerOptions, ProgramFile};

/// Program-wide gate for scanner-recovered numeric products. Diagnostics are
/// lexical and target-independent; checked summaries and emit graduate only
/// when every root proves the same direct recovery family.
pub(crate) fn has_unmodeled_numeric_recovery_program_products(
    files: &[ProgramFile],
    options: &CompilerOptions,
) -> bool {
    let has_authored = files
        .iter()
        .any(|file| file.syntax.has_authored_numeric_recovery());
    has_authored
        && (!numeric_recovery_program_options_supported(options)
            || !roots_are_homogeneous_literal_products(
                files,
                LiteralProductFamily::NumericRecovery,
            )
            || !numeric_recovery_families_are_homogeneous(files))
}

fn numeric_recovery_families_are_homogeneous(files: &[ProgramFile]) -> bool {
    let Some(first) = files
        .first()
        .and_then(|file| numeric_recovery_family(&file.syntax.statements))
    else {
        return false;
    };
    files.iter().all(|file| {
        numeric_recovery_family(&file.syntax.statements).is_some_and(|family| family == first)
    })
}

fn numeric_recovery_program_options_supported(options: &CompilerOptions) -> bool {
    !options.no_lib
        && options.lib.is_none()
        && !options.no_emit_on_error
        && !options.declaration
        && !options.declaration_map
        && !options.source_map
        && !options.inline_source_map
        && !options.remove_comments
        && options.root_dir.is_none()
        && options.out_dir.is_none()
        && options.declaration_dir.is_none()
        && exact_option_value(&options.target, &["es6", "es2015"])
        && exact_option_value(&options.module, &["commonjs", "esnext", "preserve"])
}
