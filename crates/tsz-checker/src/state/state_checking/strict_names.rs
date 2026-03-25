//! Strict-mode reserved name and eval/arguments checking.
//!
//! Covers TS1100, TS1212, TS1213, TS1214 diagnostics for identifiers
//! that are restricted in strict mode contexts.

use crate::state::CheckerState;

/// Check if a name is a strict mode reserved word (ES5 §7.6.1.2).
/// These identifiers cannot be used as variable/function/class names in strict mode.
pub(crate) fn is_strict_mode_reserved_name(name: &str) -> bool {
    matches!(
        name,
        "implements"
            | "interface"
            | "let"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "static"
            | "yield"
    )
}

/// Check if a name is `eval` or `arguments`.
/// These identifiers have restricted use in strict mode (TS1100).
pub(crate) fn is_eval_or_arguments(name: &str) -> bool {
    name == "eval" || name == "arguments"
}

impl<'a> CheckerState<'a> {
    /// Check a declaration name node for strict mode reserved words.
    /// Emits TS1212 (general strict mode), TS1213 (class context), or TS1214 (module context).
    pub(crate) fn check_strict_mode_reserved_name_at(
        &mut self,
        name_idx: tsz_parser::parser::NodeIndex,
        context_node: tsz_parser::parser::NodeIndex,
    ) {
        if name_idx.is_none() || !self.is_strict_mode_for_node(context_node) {
            return;
        }
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return;
        };
        if !is_strict_mode_reserved_name(&ident.escaped_text) {
            return;
        }
        self.emit_strict_mode_reserved_word_error(name_idx, &ident.escaped_text, true);
    }

    /// Check a class declaration name for strict-mode reserved words.
    /// Class definitions are automatically in strict mode, so the class name
    /// always gets TS1213 (class context), regardless of whether there's an
    /// enclosing class. This matches tsc behavior where `class implements {}`
    /// in a namespace emits TS1213, not TS1212.
    pub(crate) fn check_class_name_strict_mode_reserved(
        &mut self,
        name_idx: tsz_parser::parser::NodeIndex,
    ) {
        // Suppress when file has parse errors (tsc's grammarErrorOnNode pattern).
        if name_idx.is_none() || self.has_syntax_parse_errors() {
            return;
        }
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return;
        };
        if !is_strict_mode_reserved_name(&ident.escaped_text) {
            return;
        }
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        let message = format_message(
            diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
            &[&ident.escaped_text],
        );
        self.error_at_node(
            name_idx,
            &message,
            diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
        );
    }

    /// Emit the appropriate TS1212/TS1213/TS1214 diagnostic for a strict-mode reserved word.
    ///
    /// When `use_class_message` is true and we're inside a class, emits TS1213 (class context).
    /// When inside a module, emits TS1214 (module context).
    /// Otherwise emits TS1212 (general strict mode).
    pub(crate) fn emit_strict_mode_reserved_word_error(
        &mut self,
        name_idx: tsz_parser::parser::NodeIndex,
        escaped_text: &str,
        use_class_message: bool,
    ) {
        // Suppress when file has real parser errors (tsc's grammarErrorOnNode pattern).
        // Use has_parse_errors() (set by the CLI driver from actual parser diagnostics)
        // rather than has_syntax_parse_errors() which is temporarily set for grammar
        // errors like TS1108 (return outside function). TS1212 should still fire
        // alongside grammar errors — tsc's parser emits TS1212 independently.
        if self.has_parse_errors() {
            return;
        }
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        // Prevent duplicate TS1212/TS1213/TS1214 at the same position.
        // Multiple paths (type resolution, identifier resolution, parameter checking)
        // can trigger this for the same identifier; tsc only emits one.
        if let Some(node) = self.ctx.arena.get(name_idx) {
            let pos = node.pos;
            let already_emitted = self.ctx.diagnostics.iter().any(|d| {
                d.start == pos
                    && (d.code == diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE
                        || d.code == diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO
                        || d.code == diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY)
            });
            if already_emitted {
                return;
            }
        }
        if use_class_message && self.ctx.enclosing_class.is_some() {
            let message = format_message(
                diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
                &[escaped_text],
            );
            self.error_at_node(
                name_idx,
                &message,
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
            );
        } else if self.ctx.is_external_module_file() {
            let message = format_message(
                diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY,
                &[escaped_text],
            );
            self.error_at_node(
                name_idx,
                &message,
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY,
            );
        } else {
            let message = format_message(
                diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE,
                &[escaped_text],
            );
            self.error_at_node(
                name_idx,
                &message,
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE,
            );
        }
    }

    /// Like `emit_strict_mode_reserved_word_error`, but detects class context via AST walk
    /// instead of `enclosing_class`. This is needed during type resolution, where the
    /// checker's `enclosing_class` field may not reflect the actual AST context.
    pub(crate) fn emit_strict_mode_reserved_word_error_with_ast_walk(
        &mut self,
        name_idx: tsz_parser::parser::NodeIndex,
        escaped_text: &str,
    ) {
        // Suppress when file has real parser errors (see emit_strict_mode_reserved_word_error).
        if self.has_parse_errors() {
            return;
        }
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_parser::parser::syntax_kind_ext;

        // Detect class context by walking up the AST
        let in_class = {
            let mut cur = name_idx;
            let mut found = false;
            for _ in 0..256 {
                if !cur.is_some() {
                    break;
                }
                if let Some(n) = self.ctx.arena.get(cur) {
                    if n.kind == syntax_kind_ext::CLASS_DECLARATION
                        || n.kind == syntax_kind_ext::CLASS_EXPRESSION
                    {
                        found = true;
                        break;
                    }
                    if n.kind == syntax_kind_ext::SOURCE_FILE {
                        break;
                    }
                }
                let Some(ext) = self.ctx.arena.get_extended(cur) else {
                    break;
                };
                if ext.parent.is_none() {
                    break;
                }
                cur = ext.parent;
            }
            found
        };

        if in_class {
            let message = format_message(
                diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
                &[escaped_text],
            );
            self.error_at_node(
                name_idx,
                &message,
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
            );
        } else if self.ctx.is_external_module_file() {
            let message = format_message(
                diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY,
                &[escaped_text],
            );
            self.error_at_node(
                name_idx,
                &message,
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY,
            );
        } else {
            let message = format_message(
                diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE,
                &[escaped_text],
            );
            self.error_at_node(
                name_idx,
                &message,
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE,
            );
        }
    }

    /// Emit TS1214 (module context) for a strict-mode reserved word.
    /// Used by import declarations where the file is always a module.
    pub(crate) fn emit_module_strict_mode_reserved_word_error(
        &mut self,
        name_idx: tsz_parser::parser::NodeIndex,
        escaped_text: &str,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        let message = format_message(
            diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY,
            &[escaped_text],
        );
        self.error_at_node(
            name_idx,
            &message,
            diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY,
        );
    }

    /// Emit TS1100 for `eval` or `arguments` used as a binding name in strict mode.
    ///
    /// In strict mode, using `eval` or `arguments` as a variable name, parameter name,
    /// function name, or any other binding is a syntax error (TS1100).
    pub(crate) fn emit_eval_or_arguments_strict_mode_error(
        &mut self,
        name_idx: tsz_parser::parser::NodeIndex,
        name: &str,
    ) {
        // Suppress when file has parse errors (tsc's grammarErrorOnNode pattern).
        if self.has_syntax_parse_errors() {
            return;
        }
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        let in_class = self
            .ctx
            .enclosing_class
            .as_ref()
            .is_some_and(|class| !class.is_declared);
        let (message, code) = if in_class {
            (
                format_message(
                    diagnostic_messages::CODE_CONTAINED_IN_A_CLASS_IS_EVALUATED_IN_JAVASCRIPTS_STRICT_MODE_WHICH_DOES_NOT,
                    &[name],
                ),
                diagnostic_codes::CODE_CONTAINED_IN_A_CLASS_IS_EVALUATED_IN_JAVASCRIPTS_STRICT_MODE_WHICH_DOES_NOT,
            )
        } else if self.ctx.is_external_module_file() {
            (
                format_message(
                    diagnostic_messages::INVALID_USE_OF_MODULES_ARE_AUTOMATICALLY_IN_STRICT_MODE,
                    &[name],
                ),
                diagnostic_codes::INVALID_USE_OF_MODULES_ARE_AUTOMATICALLY_IN_STRICT_MODE,
            )
        } else {
            (
                format_message(diagnostic_messages::INVALID_USE_OF_IN_STRICT_MODE, &[name]),
                diagnostic_codes::INVALID_USE_OF_IN_STRICT_MODE,
            )
        };
        self.error_at_node(name_idx, &message, code);
    }
}
