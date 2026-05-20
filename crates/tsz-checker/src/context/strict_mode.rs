//! Strict mode detection for AST nodes.
//!
//! Determines whether a given node is in strict mode by walking up the AST
//! looking for "use strict" directives, class bodies, or external modules.

use super::CheckerContext;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl CheckerContext<'_> {
    /// Check whether the given AST node is in strict mode context.
    ///
    /// Strict mode is active when any of these conditions are true:
    /// - `compilerOptions.alwaysStrict` is set
    /// - The file is an external module (ESM)
    /// - The node is inside a class body
    /// - An ancestor function/source-file has a `"use strict"` directive prologue
    pub fn is_strict_mode_for_node(&self, idx: NodeIndex) -> bool {
        if !self.js_strict_mode_diagnostics_enabled() {
            return false;
        }

        // `noImplicitUseStrict` cancels `alwaysStrict` for checking purposes.
        // When both are set, tsc does not enforce strict-mode rules (e.g. TS1100).
        if self.compiler_options.always_strict && !self.compiler_options.no_implicit_use_strict {
            return true;
        }

        if self.is_external_module_file() {
            return true;
        }

        let mut current = idx;
        loop {
            let Some(node) = self.arena.get(current) else {
                return false;
            };

            if node.kind == syntax_kind_ext::CLASS_DECLARATION
                || node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                return true;
            }

            let Some(ext) = self.arena.get_extended(current) else {
                return false;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.arena.get(parent) else {
                return false;
            };

            match parent_node.kind {
                k if k == syntax_kind_ext::SOURCE_FILE => {
                    if let Some(sf) = self.arena.get_source_file(parent_node)
                        && sf
                            .statements
                            .nodes
                            .iter()
                            .any(|&stmt_idx| statement_is_use_strict(self, stmt_idx))
                    {
                        return true;
                    }
                    return false;
                }
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION =>
                {
                    if let Some(func) = self.arena.get_function(parent_node)
                        && func.body.is_some()
                        && block_has_use_strict(self, func.body)
                    {
                        return true;
                    }
                }
                _ => {}
            }

            current = parent;
        }
    }

    /// Check whether this file is an external module (ESM).
    ///
    /// Uses the per-file cache from the CLI driver if available,
    /// falling back to the binder's detection for single-file mode.
    pub(crate) fn is_external_module_file(&self) -> bool {
        if let Some(ref map) = self.is_external_module_by_file
            && let Some(&is_ext) = map.get(&self.file_name)
        {
            return is_ext;
        }
        if self.binder.is_external_module() {
            return true;
        }

        self.arena
            .source_files
            .first()
            .map(|sf| &sf.statements)
            .is_some_and(|statements| {
                statements.nodes.iter().any(|&stmt_idx| {
                    self.arena.get(stmt_idx).is_some_and(|stmt| {
                        stmt.kind == syntax_kind_ext::IMPORT_DECLARATION
                            || stmt.kind == syntax_kind_ext::EXPORT_DECLARATION
                            || stmt.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                            || stmt.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                    })
                })
            })
    }
}

/// Check if a statement node is a `"use strict"` expression statement.
fn statement_is_use_strict(ctx: &CheckerContext<'_>, stmt_idx: NodeIndex) -> bool {
    ctx.arena
        .get(stmt_idx)
        .filter(|stmt| stmt.kind == syntax_kind_ext::EXPRESSION_STATEMENT)
        .and_then(|stmt| ctx.arena.get_expression_statement(stmt))
        .and_then(|expr_stmt| ctx.arena.get(expr_stmt.expression))
        .filter(|expr_node| expr_node.kind == SyntaxKind::StringLiteral as u16)
        .and_then(|expr_node| ctx.arena.get_literal(expr_node))
        .is_some_and(|lit| lit.text == "use strict")
}

/// Check if a block contains a `"use strict"` directive in its prologue.
fn block_has_use_strict(ctx: &CheckerContext<'_>, block_idx: NodeIndex) -> bool {
    let Some(block_node) = ctx.arena.get(block_idx) else {
        return false;
    };
    let Some(block) = ctx.arena.get_block(block_node) else {
        return false;
    };
    for &stmt_idx in &block.statements.nodes {
        if statement_is_use_strict(ctx, stmt_idx) {
            return true;
        }
        let Some(stmt_node) = ctx.arena.get(stmt_idx) else {
            return false;
        };
        if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            break;
        }
    }
    false
}
