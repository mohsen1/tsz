use crate::syntax::{AuthoredLiteralKind, numeric_recovery_family};

use super::literal_products::{self, LiteralProductFamily, roots_are_homogeneous_literal_products};
use super::{CompilerOptions, ProgramFile};

/// Program-wide gate for scanner-recovered numeric products. Diagnostics are
/// lexical and target-independent; checked summaries and emit graduate only
/// when every root proves the same direct recovery family.
pub(crate) fn has_unmodeled_numeric_recovery_program_products(
    files: &[ProgramFile],
    options: &CompilerOptions,
) -> bool {
    let has_authored = files.iter().any(|file| {
        file.syntax
            .has_authored_literal(AuthoredLiteralKind::NumericRecovery)
    });
    has_authored
        && (!literal_products::direct_literal_program_options_supported(options)
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
