use std::collections::BTreeSet;

use crate::bind::{DeclarationKind, Meaning, ScopeId};
use crate::standard_library::StandardLibraryEnvironment;
use crate::syntax::{AuthoredLiteralKind, StatementKind};

use super::{CompilerOptions, ProgramFile};

#[derive(Debug, Clone, Copy)]
pub(super) enum LiteralProductFamily {
    NoSubstitutionTemplate,
    ExtendedUnicodeString,
    RegularExpression,
    NumericRecovery,
}

pub(super) fn roots_are_homogeneous_literal_products(
    files: &[ProgramFile],
    family: LiteralProductFamily,
) -> bool {
    files.iter().all(|file| match family {
        LiteralProductFamily::NoSubstitutionTemplate => {
            file.syntax
                .has_authored_literal(AuthoredLiteralKind::Template)
                && !file.syntax.has_unmodeled_template_products()
        }
        LiteralProductFamily::ExtendedUnicodeString => {
            file.syntax.has_authored_extended_unicode_string()
                && !file.syntax.has_unmodeled_extended_unicode_string_products()
        }
        LiteralProductFamily::RegularExpression => {
            file.syntax.has_authored_regular_expression()
                && !file.syntax.has_unmodeled_regular_expression_products()
        }
        LiteralProductFamily::NumericRecovery => {
            file.syntax
                .has_authored_literal(AuthoredLiteralKind::NumericRecovery)
                && !file.syntax.has_unmodeled_numeric_recovery_products()
        }
    })
}

pub(super) fn unique_top_level_value_bindings_supported(
    files: &[ProgramFile],
    options: &CompilerOptions,
) -> bool {
    let standard_library = StandardLibraryEnvironment::from_options(options);
    let mut names = BTreeSet::new();
    for file in files {
        for statement in &file.syntax.statements {
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
    }
    true
}

pub(super) fn exact_option_value(value: &str, supported: &[&str]) -> bool {
    value == value.trim()
        && supported
            .iter()
            .any(|candidate| value.eq_ignore_ascii_case(candidate))
}
