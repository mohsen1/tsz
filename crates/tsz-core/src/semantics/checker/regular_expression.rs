use crate::bind::Meaning;
use crate::program::SemanticCompletion;
use crate::semantics::types::TypeId;
use crate::source::FileId;
use crate::syntax::{RegularExpressionIssue, RegularExpressionLiteral};

use super::Checker;

impl Checker<'_> {
    /// Publish checked-path regex facts and return the ambient `RegExp` identity.
    pub(super) fn infer_regular_expression(
        &mut self,
        file: FileId,
        literal: &RegularExpressionLiteral,
    ) -> TypeId {
        match literal.validation_issues() {
            Some(issues) if literal.terminated => {
                for issue in issues {
                    let (span, message, code) = regular_expression_diagnostic(issue);
                    self.push_diagnostic(file, span, message.into(), code);
                }
            }
            Some(_) => {}
            None => self.observe_file_completion(file, SemanticCompletion::Deferred),
        }

        let Some(declaration) = self
            .program
            .standard_library
            .resolve("RegExp", Meaning::Type)
        else {
            self.observe_file_completion(file, SemanticCompletion::Deferred);
            return self.store.builtins.error;
        };

        // Authored declarations make the ambient merge incomplete.
        if self
            .program
            .standard_library_type_has_authored_declarations(declaration)
        {
            self.observe_file_completion(file, SemanticCompletion::Deferred);
        }
        self.store.symbolic_reference(declaration, Vec::new())
    }

    /// Recognize the nonliteral symbolic ambient reference during widening.
    pub(super) fn is_symbolic_regular_expression_type(&self, ty: TypeId) -> bool {
        self.program
            .standard_library
            .resolve("RegExp", Meaning::Type)
            .is_some_and(|declaration| self.store.is_unapplied_symbolic_reference(ty, declaration))
    }
}

const fn regular_expression_diagnostic(
    issue: RegularExpressionIssue,
) -> (crate::source::Span, &'static str, u32) {
    match issue {
        RegularExpressionIssue::HexDigit(span) => (span, "Hexadecimal digit expected.", 1125),
        RegularExpressionIssue::UnicodeRange(span) => (
            span,
            "An extended Unicode escape value must be between 0x0 and 0x10FFFF inclusive.",
            1198,
        ),
        RegularExpressionIssue::UnknownFlag(span) => {
            (span, "Unknown regular expression flag.", 1499)
        }
        RegularExpressionIssue::DuplicateFlag(span) => {
            (span, "Duplicate regular expression flag.", 1500)
        }
        RegularExpressionIssue::CloseBrace(span) => (
            span,
            "Unexpected '}'. Did you mean to escape it with backslash?",
            1508,
        ),
    }
}
