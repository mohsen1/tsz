//! Module-none (TS1148) checks.
//!
//! Emits TS1148 when `module=none` and the file contains imports, exports,
//! or module augmentations.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    /// Emit TS1148 if module=none and the file contains imports, exports, or module augmentations.
    pub(crate) fn check_module_none_statements(&mut self, stmts: &[NodeIndex]) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_parser::parser::syntax_kind_ext;

        for &stmt_idx in stmts {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            let mut is_error = false;

            match node.kind {
                syntax_kind_ext::IMPORT_DECLARATION
                | syntax_kind_ext::EXPORT_DECLARATION
                | syntax_kind_ext::EXPORT_ASSIGNMENT => {
                    is_error = true;
                }
                syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                    // `import Alias = Ns.Member` is an internal namespace alias and
                    // should not trigger TS1148. Only `import x = require("...")`
                    // should be treated as module syntax for module=none checks.
                    if let Some(import_decl) = self.ctx.arena.get_import_decl(node)
                        && let Some(module_ref_node) =
                            self.ctx.arena.get(import_decl.module_specifier)
                        && module_ref_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
                    {
                        is_error = true;
                    }
                }
                syntax_kind_ext::MODULE_DECLARATION => {
                    if self.is_declaration_exported(self.ctx.arena, stmt_idx) {
                        is_error = true;
                    } else if let Some(module) = self.ctx.arena.get_module(node)
                        && let Some(name_node) = self.ctx.arena.get(module.name)
                        && name_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
                    {
                        is_error = true;
                    }
                }
                // Declarations that can have an `export` modifier
                k if k == syntax_kind_ext::VARIABLE_STATEMENT
                    || k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::INTERFACE_DECLARATION
                    || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || k == syntax_kind_ext::ENUM_DECLARATION =>
                {
                    if self.is_declaration_exported(self.ctx.arena, stmt_idx) {
                        is_error = true;
                    }
                }
                _ => {}
            }

            if is_error {
                self.error_at_node(
                    stmt_idx,
                    diagnostic_messages::CANNOT_USE_IMPORTS_EXPORTS_OR_MODULE_AUGMENTATIONS_WHEN_MODULE_IS_NONE,
                    diagnostic_codes::CANNOT_USE_IMPORTS_EXPORTS_OR_MODULE_AUGMENTATIONS_WHEN_MODULE_IS_NONE,
                );
            }
        }
    }
}
