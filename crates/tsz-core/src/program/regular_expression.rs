use std::collections::BTreeSet;

use crate::bind::{DeclarationKind, Meaning, ScopeId};
use crate::standard_library::StandardLibraryEnvironment;
use crate::syntax::{
    StatementKind, statements_form_regular_expression_expression_file,
    statements_form_regular_expression_variable_file,
};

use super::{CompilerOptions, ProgramFile};

/// Program-wide product gate for the first regular-expression vertical slice.
/// Every authored root must independently prove the same direct-host shape;
/// unrelated or mixed source files cannot borrow one safe file's capability.
pub(crate) fn has_unmodeled_regular_expression_program_products(
    files: &[ProgramFile],
    options: &CompilerOptions,
) -> bool {
    let has_authored = files
        .iter()
        .any(|file| file.syntax.has_authored_regular_expression());
    has_authored
        && (!regular_expression_program_options_supported(options)
            || options.declaration
            || options.source_map
            || options.inline_source_map
            || options.declaration_map
            || options.declaration_dir.is_some()
            || files.iter().any(|file| {
                !file.syntax.has_authored_regular_expression()
                    || file.syntax.has_unmodeled_regular_expression_products()
            })
            || !regular_expression_program_sources_supported(files, options))
}

fn regular_expression_program_sources_supported(
    files: &[ProgramFile],
    options: &CompilerOptions,
) -> bool {
    if !ambient_regular_expression_type_is_unambiguous(files, options) {
        return false;
    }
    if files
        .iter()
        .all(|file| statements_form_regular_expression_expression_file(&file.syntax.statements))
    {
        return true;
    }
    files.iter().all(|file| {
        statements_form_regular_expression_variable_file(&file.source, &file.syntax.statements)
    }) && regular_expression_variable_bindings_supported(files, options)
}

fn regular_expression_variable_bindings_supported(
    files: &[ProgramFile],
    options: &CompilerOptions,
) -> bool {
    let standard_library = StandardLibraryEnvironment::from_options(options);
    let mut names = BTreeSet::new();
    for file in files {
        let [statement] = file.syntax.statements.as_slice() else {
            return false;
        };
        let StatementKind::Variable(declaration) = &statement.kind else {
            return false;
        };
        let mut matches = file.bindings.declarations.iter().filter(|bound| {
            bound.owner == statement.id
                && bound.scope == ScopeId(0)
                && bound.kind == DeclarationKind::Variable
                && bound.meaning == Meaning::Value
        });
        let Some(bound) = matches.next() else {
            return false;
        };
        if matches.next().is_some()
            || bound.name != declaration.name
            || !names.insert(bound.name.clone())
            || standard_library
                .resolve(&bound.name, Meaning::Value)
                .is_some()
        {
            return false;
        }
    }
    true
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

fn exact_option_value(value: &str, supported: &[&str]) -> bool {
    value == value.trim()
        && supported
            .iter()
            .any(|candidate| value.eq_ignore_ascii_case(candidate))
}
