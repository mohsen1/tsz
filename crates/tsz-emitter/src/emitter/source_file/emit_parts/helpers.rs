use super::super::super::Printer;
use super::super::super::core::JsxEmit;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

pub(in crate::emitter) struct SourceFileCommentSetup {
    pub inside_module_wrapper: bool,
    pub first_erased_stmt_pos: Option<u32>,
    pub first_erased_is_import_export: bool,
}

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn prepare_source_file_emit_state(
        &mut self,
        file_name: &str,
        statements: &NodeList,
    ) {
        // Track whether this is a JavaScript source file. JS files do not
        // undergo import elision since the checker treats all imports as values.
        {
            let file_name = file_name.to_ascii_lowercase();
            self.source_is_js_file = file_name.ends_with(".js")
                || file_name.ends_with(".jsx")
                || file_name.ends_with(".cjs")
                || file_name.ends_with(".mjs");
        }

        // Detect export assignment (export =) to suppress other exports
        if self.has_export_assignment(statements) {
            self.ctx.module_state.has_export_assignment = true;
        }

        // Store file name for jsx=react-jsxdev source location emission
        if matches!(self.ctx.options.jsx, JsxEmit::ReactJsxDev) {
            self.jsx_dev_file_name = Some(jsx_dev_file_name(file_name));
        } else {
            self.jsx_dev_file_name = None;
        }
        self.jsx_legacy_cjs_runtime_var = None;

        // Collect all identifiers in the file for temp name collision detection.
        // This mirrors TypeScript's `sourceFile.identifiers` used by `makeUniqueName`.
        self.file_identifiers.clear();
        for ident in &self.arena.identifiers {
            self.file_identifiers.insert(ident.escaped_text.clone());
        }
        if !self.ctx.is_inside_module_wrapper_body() {
            self.commonjs_named_import_substitutions.clear();
        }
        if !matches!(self.ctx.original_module_kind, Some(ModuleKind::AMD)) {
            self.wrapped_export_module_substitutions.clear();
        }
        self.generated_temp_names.clear();
        self.reserved_nested_temp_names.clear();
        self.preplanned_legacy_decorated_class_aliases.clear();
        self.async_generator_inner_name_counts.clear();
        self.reserved_disposable_env_names.clear();
        self.reserved_top_level_using_class_result_temps.clear();
        self.hoisted_deferred_static_class_result_temps.clear();
        self.node_esm_create_require_names = None;
        // The wrapper sets the correct binding (e.g. "tslib_2" for a second outFile
        // module) before calling the body; resetting here would clobber that value.
        if !self.ctx.is_inside_module_wrapper_body() {
            self.commonjs_tslib_import_binding = "tslib_1".to_string();
        }
        self.ctx.arguments_capture_counter = 0;
        self.ctx.loop_this_capture_counter = 0;
        self.ctx.loop_this_capture_name = None;
        self.next_dynamic_import_promise_id = 1;
        self.first_for_of_emitted = false;
        self.namespace_all_exported_names.clear();
        self.collect_all_namespace_exports(statements);
        self.prepare_file_level_class_temp_reservations(statements);
    }

    pub(in crate::emitter) fn prepare_source_file_comments(
        &mut self,
        statements: &NodeList,
    ) -> SourceFileCommentSetup {
        // Extract comments. Triple-slash reference directives (/// <reference ...>)
        // are preserved as regular comments in CJS/ESM JS output (tsc behavior).
        // In AMD/UMD/System modes, reference directives are stripped from the
        // wrapper body since they don't belong inside `define()` / `System.register()`.
        // `/// <amd-dependency .../>` directives are emitted before define() via
        // extract_amd_dependencies() and must not appear in all_comments to avoid
        // duplication. However, `/// <amd-module name="..."/>` directives MUST
        // be kept so they appear inside the AMD wrapper body (matching tsc behavior).
        // Store on self so nested blocks can also distribute comments.
        let inside_module_wrapper = self.ctx.original_module_kind.is_some();
        self.all_comments = if !self.ctx.options.remove_comments {
            if let Some(text) = self.source_text {
                self.source_comment_ranges
                    .iter()
                    .filter(|c| {
                        let content = c.get_text(text);
                        // When inside a module wrapper (AMD/UMD/System):
                        // - Suppress amd-dependency directives (already emitted before
                        //   define()). Keep amd-module so it appears inside the wrapper
                        //   body matching tsc behavior.
                        // - Suppress reference directives — they will be extracted and
                        //   emitted BEFORE the wrapper call (tsc puts them outside the
                        //   define() body, not inside it).
                        // In CJS/ESM mode, reference directives pass through as regular
                        // comments (tsc preserves them in CJS/ESM JS output).
                        if inside_module_wrapper {
                            if content.contains("<amd-dependency") {
                                return false;
                            }
                            let trimmed = content.trim_start_matches('/');
                            let trimmed = trimmed.trim_start();
                            if trimmed.starts_with("<reference") {
                                return false;
                            }
                        }
                        true
                    })
                    .cloned()
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // Filter out comments associated with erased declarations
        // (interfaces, type aliases). TSC strips both the declaration body
        // and its leading trivia (comments directly before it). However,
        // file-level comments before any declarations are preserved.
        // We use prev_end to track the previous statement's end position;
        // for the first statement, we use node.pos to preserve file-level comments.
        // Track position of first erased statement for header comment filtering.
        // Only set when the erased statement is at the START of the file (no
        // non-erased statements before it). This prevents suppressing header
        // comments that belong to early non-erased statements.
        let mut first_erased_stmt_pos: Option<u32> = None;
        // Track if the first erased statement is an import/export (not an interface/type).
        // Reference directives in leading trivia should only be stripped when attached
        // to an erased import/export, not to an erased interface/type alias.
        let mut first_erased_is_import_export = false;
        if !self.ctx.flags.in_declaration_emit && !self.all_comments.is_empty() {
            let mut erased_ranges: Vec<(u32, u32)> = Vec::new();
            let mut prev_erased_end: Option<u32> = None;
            let mut seen_non_erased = false;
            let stmt_nodes = &statements.nodes;
            for (stmt_i, &stmt_idx) in stmt_nodes.iter().enumerate() {
                if let Some(stmt_node) = self.arena.get(stmt_idx) {
                    // Cap the end at the next statement's pos to prevent
                    // find_token_end_before_trivia from scanning into the next
                    // statement's territory (our parser can set node.end past
                    // the current statement's actual last token for ASI cases).
                    let scan_end = stmt_nodes
                        .get(stmt_i + 1)
                        .and_then(|&next_idx| self.arena.get(next_idx))
                        .map_or(stmt_node.end, |next_node| next_node.pos);
                    let stmt_token_end = self.find_token_end_before_trivia(stmt_node.pos, scan_end);
                    // Check if statement is erased in JS emit (type-only, ambient, etc.)
                    let mut is_erased = self.is_erased_statement(stmt_node);
                    // Also check if it's an export declaration wrapping an erased declaration
                    if !is_erased
                        && stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                        && let Some(export) = self.arena.get_export_decl(stmt_node)
                        && let Some(inner_node) = self.arena.get(export.export_clause)
                        && self.is_erased_statement(inner_node)
                    {
                        is_erased = true;
                    }

                    if is_erased {
                        // For the erased range start:
                        // - First erased stmt: use actual token start to preserve
                        //   file-level comments in leading trivia.
                        // - Consecutive erased stmts: extend from previous erased end
                        //   to capture comments between them.
                        // - Erased stmt after non-erased: use stmt_node.pos to only
                        //   capture this statement's own leading trivia, not comments
                        //   belonging to the previous non-erased statement.
                        let range_start = if let Some(pe) = prev_erased_end {
                            pe
                        } else if first_erased_stmt_pos.is_none() && !seen_non_erased {
                            // Only track for header comment filtering when the
                            // erased statement is at the very start of the file.
                            let actual_start =
                                self.skip_trivia_forward(stmt_node.pos, stmt_node.end);
                            first_erased_stmt_pos = Some(actual_start);
                            first_erased_is_import_export = matches!(
                                stmt_node.kind,
                                syntax_kind_ext::IMPORT_DECLARATION
                                    | syntax_kind_ext::EXPORT_DECLARATION
                            );
                            actual_start
                        } else {
                            stmt_node.pos
                        };
                        erased_ranges.push((range_start, stmt_token_end));
                        prev_erased_end = Some(stmt_token_end);
                    } else {
                        prev_erased_end = None;
                        seen_non_erased = true;
                    }
                }
            }
            if !erased_ranges.is_empty() {
                // Also strip `/// <reference ...>` directives that are "attached" to
                // an erased import/export (immediately preceding it, no blank line gap).
                // tsc preserves detached references (file-level) and preserve="true".
                // We look at the leading trivia of the first erased statement (position 0
                // up to the token start) and strip reference directives that are
                // immediately adjacent (no blank line before the erased token).
                self.all_comments.retain(|c| {
                    // Filter comments inside erased ranges
                    if erased_ranges
                        .iter()
                        .any(|&(start, end)| c.pos >= start && c.end <= end)
                    {
                        return false;
                    }
                    // Strip reference directives in leading trivia before the first
                    // erased statement, but only when:
                    // 1. The first erased statement is an import/export (not interface/type)
                    // 2. No blank line between reference and erased statement
                    // 3. The reference doesn't have preserve="true"
                    if let Some(fep) = first_erased_stmt_pos
                        && first_erased_is_import_export
                        && c.end <= fep
                        && let Some(text) = self.source_text
                    {
                        let comment_text = c.get_text(text);
                        let trimmed = comment_text.trim_start_matches('/');
                        let trimmed = trimmed.trim_start();
                        if trimmed.starts_with("<reference") {
                            // Skip preserve="true" references — always keep them.
                            if comment_text.contains("preserve=\"true\"") {
                                return true;
                            }
                            // Check for blank line between reference end and erased
                            // stmt start. If there's a blank line, the reference is
                            // "detached" (file-level) and should be preserved.
                            let has_blank_line =
                                crate::safe_slice::slice(text, c.end as usize, fep as usize)
                                    .is_ok_and(|gap| {
                                        gap.contains("\n\n") || gap.contains("\r\n\r\n")
                                    });
                            return has_blank_line;
                        }
                    }
                    true
                });
            }
        }

        SourceFileCommentSetup {
            inside_module_wrapper,
            first_erased_stmt_pos,
            first_erased_is_import_export,
        }
    }

    pub(in crate::emitter) fn source_file_has_use_strict(&self, statements: &NodeList) -> bool {
        let mut found = false;
        for &idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(idx) else {
                break;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                break; // non-expression-statement ends the prologue zone
            }
            let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) else {
                break;
            };
            let Some(expr_node) = self.arena.get(expr_stmt.expression) else {
                break;
            };
            if !expr_node.is_string_literal() {
                break; // non-string-literal ends the prologue zone
            }
            // Check the literal text
            let is_use_strict = if let Some(lit) = self.arena.get_literal(expr_node) {
                lit.text == "use strict"
            } else if let Some(text) = self.source_text {
                crate::safe_slice::slice(text, expr_node.pos as usize, expr_node.end as usize)
                    .is_ok_and(|s| s == "\"use strict\"" || s == "'use strict'")
            } else {
                false
            };
            if is_use_strict {
                found = true;
                break;
            }
            // Other string literal prologue — continue scanning
        }
        found
    }

    pub(in crate::emitter) fn source_file_prologue_directive_count(
        &self,
        statements: &NodeList,
    ) -> usize {
        statements
            .nodes
            .iter()
            .take_while(|&&idx| {
                let Some(stmt_node) = self.arena.get(idx) else {
                    return false;
                };
                if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                    return false;
                }
                let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) else {
                    return false;
                };
                self.arena
                    .get(expr_stmt.expression)
                    .is_some_and(|expr_node| expr_node.is_string_literal())
            })
            .count()
    }

    pub(in crate::emitter) fn source_file_has_module_wrapper_statement(
        &self,
        statements: &NodeList,
    ) -> bool {
        statements.nodes.iter().any(|&idx| {
            let callee_idx = self
                .arena
                .get(idx)
                .and_then(|stmt| self.arena.get_expression_statement(stmt))
                .and_then(|expr_stmt| self.arena.get(expr_stmt.expression))
                .and_then(|expr| self.arena.get_call_expr(expr))
                .map(|call| call.expression);
            let Some(callee_idx) = callee_idx else {
                return false;
            };
            let Some(callee_node) = self.arena.get(callee_idx) else {
                return false;
            };
            // Check direct calls: `define(...)`
            if let Some(ident) = self.arena.get_identifier(callee_node) {
                return ident.escaped_text.as_str() == "define";
            }
            // Check property access calls: `System.register(...)`
            if let Some(access) = self.arena.get_access_expr(callee_node) {
                let obj_is_system = self
                    .arena
                    .get(access.expression)
                    .and_then(|obj| self.arena.get_identifier(obj))
                    .is_some_and(|ident| ident.escaped_text.as_str() == "System");
                let prop_is_register = self
                    .arena
                    .get(access.name_or_argument)
                    .and_then(|name| self.arena.get_identifier(name))
                    .is_some_and(|ident| ident.escaped_text.as_str() == "register");
                return obj_is_system && prop_is_register;
            }
            false
        })
    }

    pub(in crate::emitter) fn source_file_use_strict_text(
        &self,
        statements: &NodeList,
        skip_source_use_strict: bool,
    ) -> Option<String> {
        if !skip_source_use_strict {
            return None;
        }
        statements.nodes.iter().find_map(|&idx| {
            let stmt_node = self.arena.get(idx)?;
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                return None;
            }
            let expr_stmt = self.arena.get_expression_statement(stmt_node)?;
            let expr_node = self.arena.get(expr_stmt.expression)?;
            if !expr_node.is_string_literal() {
                return None;
            }
            let is_use_strict = self
                .arena
                .get_literal(expr_node)
                .is_some_and(|lit| lit.text == "use strict");
            if !is_use_strict {
                return None;
            }
            let text = self.source_text?;
            crate::safe_slice::slice(text, expr_node.pos as usize, expr_node.end as usize)
                .ok()
                .map(str::to_string)
        })
    }

    pub(in crate::emitter) fn emit_pinned_header_comments_before_first_statement(
        &mut self,
        first_stmt_pos: u32,
    ) {
        if self.ctx.options.remove_comments
            && let Some(text) = self.source_text
        {
            let all_comments = tsz_common::comments::get_comment_ranges(text);
            // Collect pinned comments before the first statement
            let pinned: Vec<_> = all_comments
                .iter()
                .filter(|c| {
                    let content = c.get_text(text);
                    c.end <= first_stmt_pos && content.starts_with("/*!")
                })
                .collect();
            // Only emit pinned comments that are "detached" — followed by a
            // blank line before the next comment/statement.
            for (pi, comment) in pinned.iter().enumerate() {
                let next_start = pinned
                    .get(pi + 1)
                    .map_or(first_stmt_pos, |next_c| next_c.pos);
                let is_detached =
                    crate::safe_slice::slice(text, comment.end as usize, next_start as usize)
                        .is_ok_and(|between| {
                            between.contains("\n\n") || between.contains("\r\n\r\n")
                        });
                if is_detached
                    && let Ok(comment_text) =
                        crate::safe_slice::slice(text, comment.pos as usize, comment.end as usize)
                {
                    self.write_comment_with_reindent(comment_text, Some(comment.pos));
                    if comment.has_trailing_new_line {
                        self.write_line();
                    }
                }
            }
        }
    }

    pub(in crate::emitter) fn erased_statement_has_recovered_import_type_tail(
        &self,
        stmt_node: &Node,
    ) -> bool {
        if stmt_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
            return self.type_alias_has_recovered_import_type_tail(stmt_node);
        }

        if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
            && let Some(export) = self.arena.get_export_decl(stmt_node)
            && let Some(inner_node) = self.arena.get(export.export_clause)
            && inner_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
        {
            return self.type_alias_has_recovered_import_type_tail(inner_node);
        }

        false
    }

    fn type_alias_has_recovered_import_type_tail(&self, alias_node: &Node) -> bool {
        let Some(alias) = self.arena.get_type_alias(alias_node) else {
            return false;
        };
        let Some(type_node) = self.arena.get(alias.type_node) else {
            return false;
        };
        if type_node.kind != syntax_kind_ext::TYPE_QUERY {
            return false;
        }
        let Some(type_query) = self.arena.get_type_query(type_node) else {
            return false;
        };
        self.type_query_import_call_has_recovered_tail(type_node, type_query.expr_name)
    }

    fn type_query_import_call_has_recovered_tail(
        &self,
        type_query_node: &Node,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION
            || expr_node.end <= type_query_node.end
        {
            return false;
        }
        let Some(call) = self.arena.get_call_expr(expr_node) else {
            return false;
        };
        let Some(callee) = self.arena.get(call.expression) else {
            return false;
        };
        callee.kind == SyntaxKind::ImportKeyword as u16
            && call
                .arguments
                .as_ref()
                .is_some_and(|args| args.nodes.len() >= 2)
    }
}

pub(in crate::emitter) fn jsx_dev_file_name(file_name: &str) -> String {
    let normalized = file_name.replace('\\', "/");
    if let Some(src_start) = normalized.find("/.src/") {
        return normalized[src_start..].to_string();
    }
    if let Some(stripped) = normalized.strip_prefix(".src/") {
        return format!("/.src/{stripped}");
    }
    normalized
        .rsplit('/')
        .next()
        .unwrap_or(&normalized)
        .to_string()
}
