//! Literal Type Utilities Module
//!
//! Thin wrappers for literal type queries, delegating to solver.

use crate::state::CheckerState;
use tsz_solver::TypeId;
use tsz_solver::type_queries_extended::{
    get_number_literal_value as solver_get_number_literal_value, get_string_literal_atom,
    is_number_literal, is_string_literal,
};

impl<'a> CheckerState<'a> {
    /// Check if a type is a string literal type.
    ///
    /// Returns true for types like `"hello"`, `"world"`, etc.
    pub fn is_string_literal_type(&self, type_id: TypeId) -> bool {
        is_string_literal(self.ctx.types, type_id)
    }

    /// Check if a type is a number literal type.
    ///
    /// Returns true for types like `0`, `1`, `42`, `3.14`, etc.
    pub fn is_number_literal_type(&self, type_id: TypeId) -> bool {
        is_number_literal(self.ctx.types, type_id)
    }

    /// Get the string value from a string literal type.
    ///
    /// Returns the string value if the type is a string literal,
    /// or None otherwise.
    pub fn get_string_literal_value(&self, type_id: TypeId) -> Option<String> {
        get_string_literal_atom(self.ctx.types, type_id)
            .map(|atom| self.ctx.types.resolve_atom_ref(atom).to_string())
    }

    /// Get the numeric value from a number literal type.
    ///
    /// Returns the number value if the type is a number literal,
    /// or None otherwise.
    pub fn get_number_literal_value(&self, type_id: TypeId) -> Option<f64> {
        solver_get_number_literal_value(self.ctx.types, type_id)
    }

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
