//! Literal Type Utilities Module

use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    /// Validate regex literal flags against the compilation target.
    ///
    /// Emits TS1501 if a regex flag is used that requires a newer target than specified.
    pub(crate) fn validate_regex_literal_flags(&mut self, idx: tsz_parser::parser::NodeIndex) {
        use tsz_common::diagnostics::diagnostic_codes;
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };

        if node.kind != SyntaxKind::RegularExpressionLiteral as u16 {
            return;
        }

        let Some(literal) = self.ctx.arena.get_literal(node) else {
            return;
        };

        // Parse flags from regex literal text (format: /pattern/flags)
        // Find the last '/' and extract flags after it
        let text = &literal.text;
        if let Some(last_slash_pos) = text.rfind('/') {
            let flags = &text[last_slash_pos + 1..];
            let target = self.ctx.compiler_options.target;

            for (i, ch) in flags.char_indices() {
                let (_required_target, target_name) = match ch {
                    'u' | 'y' => (tsz_common::ScriptTarget::ES2015, "ES2015"),
                    's' => (tsz_common::ScriptTarget::ES2018, "ES2018"),
                    'd' => (tsz_common::ScriptTarget::ES2022, "ES2022"),
                    'v' => (tsz_common::ScriptTarget::ESNext, "ESNext"),
                    _ => continue, // g, i, m are ES3+, other flags already handled by parser
                };

                // Check if target supports this flag
                let target_supports_flag = match ch {
                    'u' | 'y' => target.supports_es2015(),
                    's' => target.supports_es2018(),
                    'd' => target.supports_es2022(),
                    'v' => target == tsz_common::ScriptTarget::ESNext,
                    _ => true,
                };

                if !target_supports_flag {
                    // Calculate position of the flag character relative to node start
                    let flag_pos = ((node.pos as usize) + last_slash_pos + 1 + i) as u32;
                    self.error(
                        flag_pos,
                        1,
                        format!(
                            "This regular expression flag is only available when targeting '{target_name}' or later."
                        ),
                        diagnostic_codes::THIS_REGULAR_EXPRESSION_FLAG_IS_ONLY_AVAILABLE_WHEN_TARGETING_OR_LATER,
                    );
                }
            }
        }
    }
}
