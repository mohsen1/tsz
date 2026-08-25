use crate::bind::{Meaning, ScopeId};
use crate::standard_library::StandardLibraryEnvironment;
use crate::syntax::{
    statements_form_regular_expression_expression_file,
    statements_form_regular_expression_variable_file,
};

use super::literal_products::{
    LiteralProductFamily, exact_option_value, roots_are_homogeneous_literal_products,
    unique_top_level_value_bindings_supported,
};
use super::{CompilerOptions, ProgramFile};

/// Program-wide product gate for the first regular-expression vertical slice.
/// Every authored root must independently prove the same direct-host shape;
/// unrelated or mixed source files cannot borrow one safe file's capability.
pub(crate) fn has_unmodeled_regular_expression_program_products(
    files: &[ProgramFile],
    options: &CompilerOptions,
) -> bool {
    files
        .iter()
        .any(|file| file.syntax.has_authored_regular_expression())
        && (!regular_expression_program_options_supported(options)
            || options.declaration
            || options.source_map
            || options.inline_source_map
            || options.declaration_map
            || options.declaration_dir.is_some()
            || !roots_are_homogeneous_literal_products(
                files,
                LiteralProductFamily::RegularExpression,
            )
            || !regular_expression_program_sources_supported(files, options))
}

fn regular_expression_program_sources_supported(
    files: &[ProgramFile],
    options: &CompilerOptions,
) -> bool {
    ambient_regular_expression_type_is_unambiguous(files, options)
        && (files.iter().all(|file| {
            statements_form_regular_expression_expression_file(&file.syntax.statements)
        }) || files.iter().all(|file| {
            statements_form_regular_expression_variable_file(&file.source, &file.syntax.statements)
        }) && unique_top_level_value_bindings_supported(files, options))
}

fn ambient_regular_expression_type_is_unambiguous(
    files: &[ProgramFile],
    options: &CompilerOptions,
) -> bool {
    let standard_library = StandardLibraryEnvironment::from_options(options);
    standard_library.resolve("RegExp", Meaning::Type).is_some()
        && files.iter().all(|file| {
            !file.bindings.declarations.iter().any(|declaration| {
                declaration.scope == ScopeId(0)
                    && declaration.meaning == Meaning::Type
                    && declaration.name == "RegExp"
            })
        })
}

fn regular_expression_program_options_supported(options: &CompilerOptions) -> bool {
    !options.no_lib
        && options.lib.is_none()
        && exact_option_value(
            &options.target,
            &[
                "es6", "es2015", "es2016", "es2017", "es2018", "es2019", "es2020", "es2021",
                "es2022", "es2023", "es2024", "es2025", "esnext",
            ],
        )
        && exact_option_value(&options.module, &["commonjs", "esnext", "preserve"])
}
