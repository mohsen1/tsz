use crate::syntax::statements_form_extended_unicode_string_variable_file;

use super::literal_products::{
    self, LiteralProductFamily, roots_are_homogeneous_literal_products,
    unique_top_level_value_bindings_supported,
};
use super::{CompilerOptions, ProgramFile};

/// Program-wide capability for the first ordinary-string extended Unicode
/// escape slice. Every root must prove the same direct mutable `var` host;
/// unrelated or mixed roots cannot borrow a safe source's capability.
pub(crate) fn has_unmodeled_extended_unicode_string_program_products(
    files: &[ProgramFile],
    options: &CompilerOptions,
) -> bool {
    let has_authored = files
        .iter()
        .any(|file| file.syntax.has_authored_extended_unicode_string());
    has_authored
        && (!literal_products::direct_literal_program_options_supported(options)
            || !roots_are_homogeneous_literal_products(
                files,
                LiteralProductFamily::ExtendedUnicodeString,
            )
            || !files.iter().all(|file| {
                statements_form_extended_unicode_string_variable_file(
                    &file.source,
                    &file.syntax.statements,
                )
            })
            || !unique_top_level_value_bindings_supported(files, options))
}
