//! Does a CommonJS named export assign a class?
//!
//! Split out of `js_exports.rs` to keep that file under the 2000-line cap. The
//! question is syntactic on purpose: a direct `exports.K = class {}` and an
//! indirect `exports.K = NS.K` resolve to the same type, yet only the direct
//! form gives the export a type meaning in `tsc`.

use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    /// Whether the target module assigns a **class** directly to
    /// `exports.<export_name>` (or `module.exports.<export_name>`).
    ///
    /// Only that gives the export a type meaning for a bare JSDoc reference:
    /// `exports.K = class {}` and `class K {} … exports.K = K` do, while
    /// `exports.v = 1`, `exports.f = function () {}` and an indirect
    /// `exports.K = NS.K` do not — `tsc` reports TS2749 for those. The check is
    /// syntactic because the direct and indirect class exports resolve to the
    /// same type.
    ///
    /// Returns `true` when no such assignment is found, leaving other resolution
    /// paths (`module.exports = …`, re-exports, `.d.ts` targets) untouched.
    pub(crate) fn commonjs_named_export_assigns_a_class(
        &self,
        module_name: &str,
        export_name: &str,
        source_file_idx: Option<usize>,
    ) -> bool {
        let Some(target_file_idx) = source_file_idx
            .and_then(|file_idx| {
                self.ctx
                    .resolve_import_target_from_file(file_idx, module_name)
            })
            .or_else(|| self.ctx.resolve_import_target(module_name))
        else {
            return true;
        };
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let Some(source_file) = target_arena.source_files.first() else {
            return true;
        };

        let mut found_assignment = false;
        let mut assigns_class = false;
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = target_arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(stmt) = target_arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let Some(expr_node) = target_arena.get(stmt.expression) else {
                continue;
            };
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(binary) = target_arena.get_binary_expr(expr_node) else {
                continue;
            };
            if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                continue;
            }
            // `exports.<name>` / `module.exports.<name>`
            let Some(lhs_node) = target_arena.get(binary.left) else {
                continue;
            };
            if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                continue;
            }
            let Some(lhs) = target_arena.get_access_expr(lhs_node) else {
                continue;
            };
            if target_arena
                .get_identifier_at(lhs.name_or_argument)
                .is_none_or(|ident| ident.escaped_text != export_name)
            {
                continue;
            }
            // Either `exports.X = …` (bare identifier) or `module.exports.X = …`.
            let receiver_is_exports = target_arena
                .get_identifier_at(lhs.expression)
                .is_some_and(|ident| ident.escaped_text == "exports")
                || Self::is_module_exports_target_in_arena(target_arena, lhs.expression);
            if !receiver_is_exports {
                continue;
            }

            found_assignment = true;
            assigns_class = Self::commonjs_export_rhs_is_class_in_arena(target_arena, binary.right);
        }

        if found_assignment {
            assigns_class
        } else {
            true
        }
    }

    /// Whether the target module assigns a **class** to `module.exports`
    /// (or bare `exports`) as a whole-module export.
    ///
    /// A JS module has no TypeScript `export =`, so this is how
    /// `module.exports = class {}` / `class C {} module.exports = C` gives the
    /// module a type meaning for a bare `import('./m')`. A function export does
    /// not: TS7 dropped constructor-function inference, and `tsc` reports
    /// TS1340 for `@typedef {import('./m')}` when the module exports a plain
    /// function.
    pub(crate) fn commonjs_whole_module_export_assigns_a_class(
        &self,
        module_name: &str,
        source_file_idx: Option<usize>,
    ) -> bool {
        let Some(target_file_idx) = source_file_idx
            .and_then(|file_idx| {
                self.ctx
                    .resolve_import_target_from_file(file_idx, module_name)
            })
            .or_else(|| self.ctx.resolve_import_target(module_name))
        else {
            return false;
        };
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let Some(source_file) = target_arena.source_files.first() else {
            return false;
        };

        let mut assigns_class = false;
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = target_arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(stmt) = target_arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let Some(expr_node) = target_arena.get(stmt.expression) else {
                continue;
            };
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(binary) = target_arena.get_binary_expr(expr_node) else {
                continue;
            };
            if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                continue;
            }
            let targets_whole_module =
                Self::is_module_exports_target_in_arena(target_arena, binary.left)
                    || target_arena
                        .get_identifier_at(binary.left)
                        .is_some_and(|ident| ident.escaped_text == "exports");
            if !targets_whole_module {
                continue;
            }
            assigns_class = Self::commonjs_export_rhs_is_class_in_arena(target_arena, binary.right);
        }
        assigns_class
    }

    /// Whether an `exports.X = <rhs>` right-hand side is a class: a class
    /// expression, or a bare identifier naming a class declared in the same
    /// file. A property access (`NS.K`) is deliberately excluded.
    fn commonjs_export_rhs_is_class_in_arena(
        arena: &tsz_parser::parser::node::NodeArena,
        rhs_idx: NodeIndex,
    ) -> bool {
        let Some(rhs) = arena.get(rhs_idx) else {
            return false;
        };
        if rhs.kind == syntax_kind_ext::CLASS_EXPRESSION {
            return true;
        }
        if rhs.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(name) = arena.get_identifier(rhs).map(|i| i.escaped_text.clone()) else {
            return false;
        };
        let Some(source_file) = arena.source_files.first() else {
            return false;
        };
        source_file.statements.nodes.iter().any(|&stmt_idx| {
            arena.get(stmt_idx).is_some_and(|stmt| {
                stmt.kind == syntax_kind_ext::CLASS_DECLARATION
                    && arena
                        .get_class(stmt)
                        .and_then(|class| arena.get_identifier_at(class.name))
                        .is_some_and(|ident| ident.escaped_text == name)
            })
        })
    }
}
