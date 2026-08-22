use std::collections::BTreeSet;

use crate::bind::{DeclarationKind, Meaning, ScopeId};
use crate::standard_library::StandardLibraryEnvironment;
use crate::syntax::{StatementKind, statements_form_extended_unicode_string_variable_file};

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
        && (!extended_unicode_string_program_options_supported(options)
            || files.iter().any(|file| {
                !file.syntax.has_authored_extended_unicode_string()
                    || file.syntax.has_unmodeled_extended_unicode_string_products()
            })
            || !files.iter().all(|file| {
                statements_form_extended_unicode_string_variable_file(
                    &file.source,
                    &file.syntax.statements,
                )
            })
            || !extended_unicode_string_variable_bindings_supported(files, options))
}

fn extended_unicode_string_variable_bindings_supported(
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

fn extended_unicode_string_program_options_supported(options: &CompilerOptions) -> bool {
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

fn exact_option_value(value: &str, supported: &[&str]) -> bool {
    value == value.trim()
        && supported
            .iter()
            .any(|candidate| value.eq_ignore_ascii_case(candidate))
}
