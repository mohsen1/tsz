use super::Checker;
use crate::source::FileId;
use crate::syntax::ContextualGrammarKind;

impl Checker<'_> {
    pub(super) fn check_contextual_grammar_facts(&mut self, file: FileId) {
        let facts = self.program.files[file.0 as usize]
            .syntax
            .contextual_grammar_facts()
            .to_vec();
        for fact in facts {
            let authored = self.program.files[file.0 as usize]
                .source
                .slice(fact.span)
                .to_string();
            let (code, message) = match fact.kind {
                ContextualGrammarKind::AccessorTypeParameters => {
                    (1094, "An accessor cannot have type parameters.".to_string())
                }
                ContextualGrammarKind::AccessorThisParameter => (
                    2784,
                    "'get' and 'set' accessors cannot declare 'this' parameters.".to_string(),
                ),
                ContextualGrammarKind::AwaitBinding => (
                    1359,
                    format!(
                        "Identifier expected. '{authored}' is a reserved word that cannot be used here."
                    ),
                ),
                ContextualGrammarKind::StrictYieldBinding => (
                    1212,
                    format!("Identifier expected. '{authored}' is a reserved word in strict mode."),
                ),
                ContextualGrammarKind::ClassStrictYieldBinding => (
                    1213,
                    format!(
                        "Identifier expected. '{authored}' is a reserved word in strict mode. Class definitions are automatically in strict mode."
                    ),
                ),
            };
            self.push_diagnostic(file, fact.span, message, code);
        }
    }
}
