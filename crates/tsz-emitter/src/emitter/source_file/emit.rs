use super::super::Printer;
use super::super::core::JsxEmit;
use rustc_hash::FxHashSet;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    // =========================================================================
    // Source File
    // =========================================================================

    pub(in crate::emitter) fn emit_source_file(&mut self, node: &Node, source_idx: NodeIndex) {
        let Some(source) = self.arena.get_source_file(node) else {
            return;
        };

        self.writer
            .ensure_output_capacity(Self::estimate_output_capacity(source.text.len()));

        // Auto-detect module: if enabled and module is None (not explicitly set),
        // switch to CommonJS when file has imports/exports.
        // Do NOT override explicit module targets like ES2015/ESNext.
        if self.ctx.auto_detect_module
            && matches!(self.ctx.options.module, ModuleKind::None)
            && self.file_is_module(&source.statements)
        {
            self.ctx.options.module = ModuleKind::CommonJS;
        }

        // Node16/NodeNext default to CommonJS for `.ts`/`.js` unless the file
        // extension explicitly opts into ESM (`.mts`/`.mjs`).
        if matches!(
            self.ctx.options.module,
            ModuleKind::Node16 | ModuleKind::Node18 | ModuleKind::Node20 | ModuleKind::NodeNext
        ) {
            let file_name = source.file_name.to_ascii_lowercase();
            let is_explicit_esm = file_name.ends_with(".mts") || file_name.ends_with(".mjs");
            self.ctx.options.module = if is_explicit_esm {
                ModuleKind::ESNext
            } else {
                ModuleKind::CommonJS
            };
        }

        // Track whether this is a JavaScript source file. JS files do not
        // undergo import elision since the checker treats all imports as values.
        {
            let file_name = source.file_name.to_ascii_lowercase();
            self.source_is_js_file = file_name.ends_with(".js")
                || file_name.ends_with(".jsx")
                || file_name.ends_with(".cjs")
                || file_name.ends_with(".mjs");
        }

        // Detect export assignment (export =) to suppress other exports
        if self.has_export_assignment(&source.statements) {
            self.ctx.module_state.has_export_assignment = true;
        }

        // Store file name for jsx=react-jsxdev source location emission
        if matches!(self.ctx.options.jsx, JsxEmit::ReactJsxDev) {
            // Extract just the basename from the full file path
            let base_name = source
                .file_name
                .rsplit('/')
                .next()
                .unwrap_or(&source.file_name);
            self.jsx_dev_file_name = Some(base_name.to_string());
        }

        // Collect all identifiers in the file for temp name collision detection.
        // This mirrors TypeScript's `sourceFile.identifiers` used by `makeUniqueName`.
        self.file_identifiers.clear();
        for ident in &self.arena.identifiers {
            self.file_identifiers.insert(ident.escaped_text.clone());
        }
        if !matches!(
            self.ctx.original_module_kind,
            Some(ModuleKind::AMD | ModuleKind::UMD | ModuleKind::System)
        ) {
            self.commonjs_named_import_substitutions.clear();
        }
        self.generated_temp_names.clear();
        self.first_for_of_emitted = false;

        // Pre-pass: collect const enum values for inlining at usage sites.
        // tsc replaces property/element access to const enum members with their
        // literal values (e.g., `Direction.Up` → `1 /* Direction.Up */`).
        // Note: `preserveConstEnums` preserves the enum DECLARATION but still
        // inlines usages. So we always collect values regardless of that flag.
        // However, `isolatedModules`/`verbatimModuleSyntax` disable inlining
        // entirely (since const enums can't be inlined across file boundaries).
        if !self.ctx.options.no_const_enum_inlining {
            self.collect_const_enum_values(&source.statements);
        }

        // Enter root scope for block-scoped variable tracking and `var` scope boundaries.
        // This ensures variables declared throughout the file are tracked for renaming.
        self.ctx.block_scope_state.enter_function_scope();

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
                tsz_common::comments::get_comment_ranges(text)
                    .into_iter()
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
            let stmt_nodes = &source.statements.nodes;
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
                            let gap = crate::safe_slice::slice(text, c.end as usize, fep as usize);
                            if gap.contains("\n\n") || gap.contains("\r\n\r\n") {
                                return true;
                            }
                            return false;
                        }
                    }
                    true
                });
            }
        }

        self.comment_emit_idx = 0;

        // Emit shebang line if present (must be the very first line of output)
        if let Some(text) = self.source_text
            && text.starts_with("#!")
        {
            if let Some(newline_pos) = text.find('\n') {
                self.write(text[..newline_pos].trim_end());
            } else {
                self.write(text.trim_end());
            }
            self.write_line();
        }

        // Emit "use strict" FIRST (before comments and helpers)
        // TypeScript emits "use strict" when:
        // 1. Module is CommonJS/AMD/UMD (always)
        // 2. alwaysStrict compiler option is enabled (for non-ES-module output)
        // But NOT when:
        // - The source already has "use strict" (avoid duplication)
        // - ES module output (ES2015/ESNext module kind) since ESM is implicitly strict
        let is_top_level_cjs = matches!(self.ctx.options.module, ModuleKind::CommonJS);
        let is_es_module_output = matches!(
            self.ctx.options.module,
            ModuleKind::ES2015
                | ModuleKind::ES2020
                | ModuleKind::ES2022
                | ModuleKind::ESNext
                | ModuleKind::Preserve
                | ModuleKind::Node16
                | ModuleKind::Node18
                | ModuleKind::Node20
                | ModuleKind::NodeNext
        );
        let is_amd_or_umd = matches!(self.ctx.options.module, ModuleKind::AMD | ModuleKind::UMD);

        // Check if source already has "use strict" as a prologue directive.
        // Prologue directives are string literal expression statements that appear
        // BEFORE any non-string-literal statements. Once a non-string-literal
        // statement is seen, the prologue zone ends.
        // We must check the AST rather than raw text because there may be comments
        // before the prologue that would fool a text-based check.
        let source_has_use_strict = {
            let mut found = false;
            for &idx in &source.statements.nodes {
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
                if expr_node.kind != SyntaxKind::StringLiteral as u16 {
                    break; // non-string-literal ends the prologue zone
                }
                // Check the literal text
                let is_use_strict = if let Some(lit) = self.arena.get_literal(expr_node) {
                    lit.text == "use strict"
                } else if let Some(text) = self.source_text {
                    let s = crate::safe_slice::slice(
                        text,
                        expr_node.pos as usize,
                        expr_node.end as usize,
                    );
                    s == "\"use strict\"" || s == "'use strict'"
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
        };

        // Pre-compute whether JSX auto-import will generate import/require statements.
        // `jsx_will_add_any_import` is true for both ESM and CJS.
        // `jsx_will_add_esm_imports` is only true for ESM (non-CJS).
        let jsx_will_add_any_import = matches!(
            self.ctx.options.jsx,
            JsxEmit::ReactJsx | JsxEmit::ReactJsxDev
        ) && {
            let usage = self.scan_jsx_usage();
            usage.needs_jsx
                || usage.needs_jsxs
                || usage.needs_fragment
                || usage.needs_create_element
        };
        // When JSX adds ESM imports, the output becomes an ES module (implicitly
        // strict), so we must suppress "use strict" and reorder header comments.
        let jsx_will_add_esm_imports =
            jsx_will_add_any_import && !self.ctx.is_effectively_commonjs();

        // TypeScript emits "use strict" when:
        // 1. CommonJS AND the file is actually an ES module (has import/export).
        //    Script files (no import/export) don't get "use strict".
        // 2. AMD/UMD module files get "use strict" inside their define() wrapper,
        //    NOT at the top level. Non-module scripts under AMD/UMD don't get it.
        //    Pre-bundled files with define() wrappers already have it inside.
        // 3. alwaysStrict is on AND the file is not already an ES module output.
        let is_file_module = self.file_is_module(&source.statements);
        self.ctx.file_is_module = is_file_module;
        let has_module_wrapper_stmt = source.statements.nodes.iter().any(|&idx| {
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
        });
        // Note: AMD/UMD/System module files are handled by emit_module_wrapper() before
        // reaching here, so conditions below only affect:
        //   - non-module scripts under any module kind
        //   - outFile bundles with pre-existing define()/System.register() wrappers
        // Whether we need "use strict" at the top of this output.
        // For .cts/.cjs files where module was overridden from ESM to CJS,
        // tsc does NOT add "use strict" — the file is emitted as plain CJS.
        // But when inside AMD/UMD wrappers (original_module_kind is AMD/UMD),
        // "use strict" IS needed inside the define() callback body.
        let is_suppressed_cts_override = self.ctx.original_module_kind.is_some_and(|mk| {
            !matches!(mk, ModuleKind::AMD | ModuleKind::UMD | ModuleKind::System)
        });
        let needs_use_strict_cjs =
            is_top_level_cjs && is_file_module && !is_suppressed_cts_override;
        let needs_use_strict_amd_umd = is_amd_or_umd && is_file_module && !has_module_wrapper_stmt;
        // When emitting the body of an AMD/UMD wrapper, emit_module_wrapper_body()
        // temporarily sets module=CommonJS and original_module_kind=Some(AMD/UMD).
        // tsc emits "use strict" as the first line inside the wrapper callback,
        // so we need to detect this case and emit it here.
        let needs_use_strict_inside_wrapper = is_top_level_cjs
            && is_file_module
            && matches!(
                self.ctx.original_module_kind,
                Some(ModuleKind::AMD) | Some(ModuleKind::UMD)
            );
        let needs_use_strict_always = self.ctx.options.always_strict
            && !has_module_wrapper_stmt
            && self.ctx.original_module_kind.is_none()
            && !(is_es_module_output && is_file_module)
            && !jsx_will_add_esm_imports;

        let should_emit_use_strict = !source_has_use_strict
            && !self.ctx.options.suppress_use_strict
            && (needs_use_strict_cjs
                || needs_use_strict_amd_umd
                || needs_use_strict_inside_wrapper
                || needs_use_strict_always);

        // When the source has its own "use strict" prologue AND this is a CJS
        // module file, we must emit "use strict" at the correct position (before
        // __esModule marker / exports preamble) and skip the source's own
        // directive during statement iteration to avoid duplication.
        let skip_source_use_strict =
            source_has_use_strict && (needs_use_strict_cjs || needs_use_strict_inside_wrapper);

        // Emit "use strict" when either:
        // - we need to add it (source doesn't have it), or
        // - the source has it but needs repositioning (CJS: before helpers/exports)
        // But NOT when suppress_use_strict is set (wrapper already emitted it).
        if should_emit_use_strict
            || (skip_source_use_strict && !self.ctx.options.suppress_use_strict)
        {
            self.write("\"use strict\";");
            self.write_line();
        }

        // Emit header comments AFTER "use strict" but BEFORE helpers.
        // Use skip_trivia_forward to find the actual token start since
        // node.pos may include leading trivia (where comments live).
        let first_stmt_pos = source
            .statements
            .nodes
            .first()
            .and_then(|&idx| self.arena.get(idx))
            .map_or(node.end, |n| self.skip_trivia_forward(n.pos, n.end));

        // When removeComments is true, tsc still emits "pinned" comments
        // (/*! ... */) that are detached from the first statement (i.e.,
        // separated by a blank line). These are typically copyright notices.
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
                let between =
                    crate::safe_slice::slice(text, comment.end as usize, next_start as usize);
                let is_detached = between.contains("\n\n") || between.contains("\r\n\r\n");
                if is_detached {
                    let comment_text =
                        crate::safe_slice::slice(text, comment.pos as usize, comment.end as usize);
                    self.write_comment_with_reindent(comment_text, Some(comment.pos));
                    if comment.has_trailing_new_line {
                        self.write_line();
                    }
                }
            }
        }
        let first_stmt_is_auto_accessor_class = source
            .statements
            .nodes
            .iter()
            .filter_map(|&idx| self.arena.get(idx))
            .find(|stmt_node| {
                !self.ctx.flags.in_declaration_emit && !self.is_erased_statement(stmt_node)
            })
            .is_some_and(|stmt_node| {
                stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION
                    && self
                        .arena
                        .get_class(stmt_node)
                        .is_some_and(|class| self.class_has_auto_accessor_members(class))
            });

        let mut deferred_header_comments: Vec<(String, bool)> = Vec::new();
        let mut jsx_deferred_comments: Vec<(String, bool)> = Vec::new();
        let is_commonjs = self.ctx.is_commonjs();
        // Check upfront if runtime helpers will be injected — this affects
        // whether attached header comments should be deferred to after helpers.
        let will_emit_helpers = !self.ctx.options.no_emit_helpers
            && self.transforms.helpers_populated()
            && self.transforms.helpers().any_needed();

        // Pre-compute the detached comment boundary for erased first statements.
        // tsc's algorithm: scan header comment ranges, find the FIRST blank-line
        // gap between consecutive ranges. Ranges before the gap are "detached"
        // (file-level, preserved). Ranges after are "attached" to the erased
        // declaration and should be stripped.
        let erased_detach_boundary: u32 = if first_erased_stmt_pos.is_some() {
            if let Some(text) = self.source_text {
                let mut idx = self.comment_emit_idx;
                let mut ranges: Vec<(u32, u32)> = Vec::new();
                while idx < self.all_comments.len() {
                    let c = &self.all_comments[idx];
                    if c.end <= first_stmt_pos {
                        ranges.push((c.pos, c.end));
                        idx += 1;
                    } else {
                        break;
                    }
                }
                let mut detach_after: Option<usize> = None;
                for i in 0..ranges.len() {
                    let range_end = ranges[i].1;
                    let next_start = if i + 1 < ranges.len() {
                        ranges[i + 1].0
                    } else {
                        first_stmt_pos
                    };
                    let between = &text[range_end as usize..next_start as usize];
                    if between.contains("\n\n") || between.contains("\r\n\r\n") {
                        detach_after = Some(i);
                        break;
                    }
                }
                if let Some(last_detached_idx) = detach_after {
                    if last_detached_idx + 1 < ranges.len() {
                        ranges[last_detached_idx + 1].0
                    } else {
                        first_stmt_pos
                    }
                } else if ranges.is_empty() {
                    first_stmt_pos
                } else {
                    // No blank-line gap found — all comments are attached
                    ranges[0].0
                }
            } else {
                first_stmt_pos
            }
        } else {
            u32::MAX
        };

        if let Some(text) = self.source_text {
            while self.comment_emit_idx < self.all_comments.len() {
                let c_end = self.all_comments[self.comment_emit_idx].end;
                if c_end <= first_stmt_pos {
                    let c_pos = self.all_comments[self.comment_emit_idx].pos;
                    let c_trailing = self.all_comments[self.comment_emit_idx].has_trailing_new_line;
                    let comment_text =
                        crate::safe_slice::slice(text, c_pos as usize, c_end as usize);
                    let trimmed_comment = comment_text.trim_start();
                    let is_triple_slash_reference = trimmed_comment.starts_with("///<reference")
                        || trimmed_comment.starts_with("/// <reference");
                    let is_amd_dependency = trimmed_comment.contains("<amd-dependency");

                    // When JSX auto-import will generate import/require statements,
                    // tsc's transform creates a synthetic statement list (pos = -1),
                    // which causes emitDetachedComments to skip all leading comments
                    // including triple-slash directives. If the first statement is
                    // erased (e.g., unused `import React`), strip triple-slash
                    // reference directives to match tsc behavior.
                    if is_triple_slash_reference
                        && jsx_will_add_any_import
                        && first_erased_stmt_pos.is_some()
                        && first_erased_is_import_export
                    {
                        self.comment_emit_idx += 1;
                        continue;
                    }

                    // Auto-accessor class declarations emit comments themselves right
                    // after helper storage declarations. Keep their leading comments
                    // in the cursor queue so declarations_class.rs can place them.
                    if first_stmt_is_auto_accessor_class
                        && first_erased_stmt_pos.is_none()
                        && !is_triple_slash_reference
                        && !is_amd_dependency
                    {
                        break;
                    }

                    // Skip comments that are attached to an erased first statement.
                    // The boundary was pre-computed above: comments at or after
                    // `erased_detach_boundary` are attached and should be stripped.
                    // Exception: `/// <reference` directives are always preserved
                    // (they are file-level directives, not attached to any declaration).
                    if first_erased_stmt_pos.is_some()
                        && c_pos >= erased_detach_boundary
                        && !is_triple_slash_reference
                    {
                        // Skip all remaining header comments (they're all attached)
                        while self.comment_emit_idx < self.all_comments.len()
                            && self.all_comments[self.comment_emit_idx].end <= first_stmt_pos
                        {
                            self.comment_emit_idx += 1;
                        }
                        break;
                    }

                    // Note: `// @option` comments are NOT stripped here.
                    // tsc preserves all source-level comments in JS output,
                    // including ones that look like directives (e.g. `// @ts-ignore`,
                    // `// @strict`, `// @noErrorTruncation`). The test harness
                    // strips actual test directives from the baseline source
                    // before it reaches the emitter, so any `// @` comment
                    // remaining in the source is a legitimate source comment
                    // that should be preserved.
                    if matches!(
                        self.ctx.original_module_kind,
                        Some(ModuleKind::AMD | ModuleKind::UMD)
                    ) && trimmed_comment.contains("<amd-dependency")
                    {
                        self.comment_emit_idx += 1;
                        continue;
                    }
                    // In CommonJS mode, "detached" comments (followed by a blank
                    // line before the next content) are file-level and go BEFORE
                    // the __esModule marker. "Attached" comments (no blank line
                    // after them) are deferred to AFTER the preamble.
                    let next_content_pos = self
                        .all_comments
                        .get(self.comment_emit_idx + 1)
                        .filter(|next_c| next_c.end <= first_stmt_pos)
                        .map_or(first_stmt_pos, |next_c| next_c.pos);
                    let between_after = &text[c_end as usize..next_content_pos as usize];
                    let is_detached =
                        between_after.contains("\n\n") || between_after.contains("\r\n\r\n");
                    let is_amd_dependency = trimmed_comment.contains("<amd-dependency");
                    // Use the original narrow `///<reference` (no space) check
                    // for the CJS deferral decision. Detached `/// <reference`
                    // (with space) must follow the normal detached logic so they
                    // appear BEFORE `__esModule`, matching tsc behavior.
                    let is_triple_slash_no_space = trimmed_comment.starts_with("///<reference");
                    // Defer "attached" comments (no blank line after) in two cases:
                    // 1. CJS mode: always defer attached comments + triple-slash refs
                    //    so they appear after __esModule/exports preamble.
                    // 2. Any mode with helpers: defer attached comments so they
                    //    appear after injected helpers (__awaiter, __decorate, etc.),
                    //    matching tsc's behavior of keeping comments attached to
                    //    the first real statement.
                    let should_defer = (is_commonjs
                        && (is_triple_slash_no_space || (!is_detached && !is_amd_dependency)))
                        || (will_emit_helpers && !is_detached && !is_amd_dependency);
                    // When JSX auto-import will generate ESM imports, defer
                    // /// <reference> directives so they appear AFTER the import,
                    // matching tsc's ordering.
                    let should_defer_for_jsx =
                        jsx_will_add_esm_imports && is_triple_slash_reference;
                    if should_defer_for_jsx {
                        jsx_deferred_comments.push((comment_text.to_string(), c_trailing));
                    } else if should_defer {
                        deferred_header_comments.push((comment_text.to_string(), c_trailing));
                    } else {
                        self.write_comment_with_reindent(comment_text, Some(c_pos));
                        if c_trailing {
                            self.write_line();
                        } else if comment_text.starts_with("/*") {
                            self.pending_block_comment_space = true;
                        }
                    }
                    self.comment_emit_idx += 1;
                } else {
                    break;
                }
            }
        }

        // Emit JSX auto-import for jsx=react-jsx / react-jsxdev (ESM only here;
        // CJS require() is emitted after __esModule below)
        let jsx_import_text = self.jsx_auto_import_text();
        let mut emitted_jsx_esm_import = false;
        if !self.ctx.is_commonjs()
            && let Some(ref jsx_import) = jsx_import_text
        {
            self.write(jsx_import);
            emitted_jsx_esm_import = true;
            // Emit comments that were deferred to appear after the JSX import
            for (comment, has_trailing_new_line) in &jsx_deferred_comments {
                self.write_comment(comment);
                if *has_trailing_new_line {
                    self.write_line();
                }
            }
        }

        // Emit runtime helpers (must come BEFORE __esModule marker)
        // Order: "use strict" → jsx-import(ESM) → tslib-import(ESM) → helpers → __esModule → tslib-require(CJS) → exports init

        // Use helpers from TransformContext (populated during lowering pass)
        // This eliminates O(N) arena scans - all helpers are detected in Phase 1
        let mut helpers = if self.transforms.helpers_populated() {
            self.transforms.helpers().clone()
        } else {
            // Fallback for non-transforming emits (should be rare)
            // In normal operation, LoweringPass always marks helpers_populated = true
            crate::transforms::helpers::HelpersNeeded::default()
        };

        let has_es5_transforms = self.has_es5_transforms();

        // When inside a module wrapper (AMD/UMD/System), import interop
        // helpers are already emitted by `emit_wrapped_import_helpers`
        // before the wrapper. Suppress them here to avoid double emission.
        if inside_module_wrapper {
            helpers.create_binding = false;
            helpers.import_star = false;
            helpers.import_default = false;
        }

        // Emit all needed helpers (unless no_emit_helpers is set)
        if !(self.ctx.options.no_emit_helpers || self.ctx.options.import_helpers && is_file_module)
        {
            let helpers_code = crate::transforms::helpers::emit_helpers(&helpers);
            if !helpers_code.is_empty() {
                self.write(&helpers_code);
                // emit_helpers() already adds newlines, no need to add more
            }
        }

        // For ESM with importHelpers, emit `import { __helper, ... } from "tslib";`
        let mut emitted_tslib_esm_import = false;
        if self.ctx.options.import_helpers && !self.ctx.is_commonjs() && helpers.any_needed() {
            let names = helpers.needed_names();
            if !names.is_empty() {
                self.write("import { ");
                self.write(&names.join(", "));
                self.write(" } from \"tslib\";");
                self.write_line();
                emitted_tslib_esm_import = true;
            }
        }

        if has_es5_transforms && helpers.make_template_object && is_file_module {
            self.build_tagged_template_var_map();
        }

        // Build value declaration names for filtering type-only export specifiers.
        // This is stored in module state so that `export { I }` handlers (both CJS
        // and ESM) can skip specifiers that refer to interfaces/type-aliases/etc.
        // Must be computed before any module-specific export handling.
        {
            use crate::transforms::module_commonjs;
            self.ctx.module_state.value_declaration_names =
                module_commonjs::build_value_declaration_names(
                    self.arena,
                    &source.statements.nodes,
                    self.ctx.options.preserve_const_enums,
                );
            self.ctx.module_state.value_decl_names_computed = true;
        }

        let has_top_level_using = !self.ctx.options.target.supports_es2025()
            && source
                .statements
                .nodes
                .iter()
                .filter_map(|&stmt_idx| self.arena.get(stmt_idx))
                .any(|stmt_node| self.statement_is_top_level_using(stmt_node));

        // CommonJS: Emit __esModule and exports initialization (AFTER helpers)
        if self.ctx.is_commonjs() {
            use crate::transforms::module_commonjs;

            // Save insertion point for CJS destructuring export temps (var _a, _b;).
            // tsc places these BEFORE the __esModule marker.
            self.cjs_destr_hoist_byte_offset = self.writer.len();
            self.cjs_destr_hoist_line = self.writer.current_line();
            self.cjs_destructuring_export_temps.clear();

            // Emit __esModule if this is an ES module.
            // Also emit it when JSX auto-import synthesizes a require() — tsc
            // considers the synthesized import as ESM syntax that triggers __esModule.
            if self.should_emit_es_module_marker(&source.statements) || jsx_import_text.is_some() {
                self.write("Object.defineProperty(exports, \"__esModule\", { value: true });");
                self.write_line();
            }

            // Collect and emit exports initialization
            // Function exports get direct assignment (hoisted), others get void 0
            let (func_exports, mut other_exports, default_func_exports) =
                module_commonjs::collect_export_names_categorized(
                    self.arena,
                    &source.statements.nodes,
                    self.ctx.options.preserve_const_enums,
                );

            if has_top_level_using
                && source.statements.nodes.iter().any(|&stmt_idx| {
                    self.arena.get(stmt_idx).is_some_and(|stmt_node| {
                        (stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                            && self
                                .arena
                                .get_export_assignment(stmt_node)
                                .is_some_and(|assign| !assign.is_export_equals))
                            || (stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                                && self.arena.get_export_decl(stmt_node).is_some_and(|export| {
                                    export.is_default_export
                                        && export.module_specifier.is_none()
                                        && self.arena.get(export.export_clause).is_some_and(
                                            |clause_node| {
                                                clause_node.kind
                                                    != syntax_kind_ext::FUNCTION_DECLARATION
                                                    && clause_node.kind
                                                        != syntax_kind_ext::CLASS_DECLARATION
                                            },
                                        )
                                }))
                    })
                })
                && !other_exports.iter().any(|name| name == "default")
            {
                let insert_at = other_exports.len().saturating_sub(1);
                other_exports.insert(insert_at, "default".to_string());
            }

            // Collect inline-exported variable names for read substitution.
            // In CJS, tsc rewrites all references to `export let/const/var` names
            // as `exports.X` (both reads and writes).
            let inline_var_names = module_commonjs::collect_inline_exported_var_names(
                self.arena,
                &source.statements.nodes,
            );
            self.commonjs_exported_var_names.extend(inline_var_names);
            // When `export =` is present, suppress hoisted function exports
            // (exports.f = f;) since module.exports replaces them, but keep
            // void 0 initialization for non-function exports (tsc behavior).
            let func_exports = if self.ctx.module_state.has_export_assignment {
                Vec::new()
            } else {
                func_exports
            };
            // Track non-hoisted exports (vars/classes/enums/modules) so default export
            // assignments can preserve live bindings (`exports.default = exports.x`).
            self.ctx.module_state.pending_exports = other_exports.clone();
            // Track function exports so `export { f }` clauses can skip
            // duplicate inline emission (already handled in preamble).
            self.ctx.module_state.hoisted_func_exports = func_exports.clone();
            // Emit other exports: exports.X = exports.Y = void 0;
            // tsc chunks into groups of 50 and reverses each chunk (reduceLeft).
            // Names that are not valid JS identifiers use bracket notation.
            if !other_exports.is_empty() {
                for chunk in other_exports.chunks(50) {
                    for (i, name) in chunk.iter().rev().enumerate() {
                        if i > 0 {
                            self.write(" = ");
                        }
                        self.write_export_property_access(name);
                    }
                    self.write(" = void 0;");
                    self.write_line();
                }
            }
            // Emit hoisted default function exports: exports.default = funcName;
            // `export default function func() {}` is hoisted like named exports.
            // tsc emits default exports before named function exports.
            // Multiple defaults can exist in error recovery (tsc emits all of them).
            let default_func_exports = if self.ctx.module_state.has_export_assignment {
                Vec::new()
            } else {
                default_func_exports
            };
            // Merge default and named function exports, preserving source order.
            // tsc emits function exports in declaration order, not default-first.
            {
                // Build a merged list of (source_position, export_name, local_name)
                let mut all_func_exports: Vec<(u32, String, String)> = Vec::new();
                for name in &default_func_exports {
                    // Find source position of the default function export
                    let pos = source
                        .statements
                        .nodes
                        .iter()
                        .find_map(|&idx| {
                            let node = self.arena.get(idx)?;
                            if node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                                let export = self.arena.get_export_decl(node)?;
                                if export.is_default_export {
                                    let clause = self.arena.get(export.export_clause)?;
                                    if clause.kind == syntax_kind_ext::FUNCTION_DECLARATION {
                                        let func = self.arena.get_function(clause)?;
                                        let fn_name = self.get_identifier_text_idx(func.name);
                                        if &fn_name == name {
                                            return Some(node.pos);
                                        }
                                    }
                                }
                            }
                            None
                        })
                        .unwrap_or(0);
                    all_func_exports.push((pos, "default".to_string(), name.clone()));
                }
                for (exported_name, local_name) in &func_exports {
                    let pos = source
                        .statements
                        .nodes
                        .iter()
                        .find_map(|&idx| {
                            let node = self.arena.get(idx)?;
                            match node.kind {
                                k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                                    let func = self.arena.get_function(node)?;
                                    let fn_name = self.get_identifier_text_idx(func.name);
                                    if &fn_name == local_name {
                                        Some(node.pos)
                                    } else {
                                        None
                                    }
                                }
                                k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                                    let export = self.arena.get_export_decl(node)?;
                                    let clause = self.arena.get(export.export_clause)?;
                                    if clause.kind == syntax_kind_ext::FUNCTION_DECLARATION {
                                        let func = self.arena.get_function(clause)?;
                                        let fn_name = self.get_identifier_text_idx(func.name);
                                        if &fn_name == local_name {
                                            Some(node.pos)
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            }
                        })
                        .unwrap_or(0);
                    all_func_exports.push((pos, exported_name.clone(), local_name.clone()));
                }
                // Sort by source position, with alphabetical tiebreaker for
                // exports referencing the same function (same position).
                // This matches tsc: `exports.j = j;` before `exports.jj = j;`
                // and `exports.default = f;` before `exports.f = f;`.
                all_func_exports.sort_by(|(pos_a, name_a, _), (pos_b, name_b, _)| {
                    pos_a.cmp(pos_b).then_with(|| name_a.cmp(name_b))
                });
                // Emit in source order
                for (_, exported_name, local_name) in &all_func_exports {
                    self.write("exports.");
                    self.write(exported_name);
                    self.write(" = ");
                    self.write(local_name);
                    self.write(";");
                    self.write_line();
                }
            }
            self.ctx.module_state.default_func_export_hoisted = !default_func_exports.is_empty();

            // Emit CJS JSX runtime require() after exports preamble
            if let Some(ref jsx_import) = jsx_import_text {
                self.write(jsx_import);
            }

            // Emit CJS tslib require after exports preamble
            if self.ctx.options.import_helpers && helpers.any_needed() {
                if self.ctx.options.target.is_es5() {
                    self.write("var tslib_1 = require(\"tslib\");");
                } else {
                    self.write("const tslib_1 = require(\"tslib\");");
                }
                self.write_line();
            }
        }

        // Save position before deferred header comments so they can be undone
        // if the first statement produces no output (e.g., `export var b: number;`
        // in CJS where the preamble `exports.b = void 0;` is all that's needed).
        let pre_deferred_comments_len = self.writer.len();
        let mut has_deferred_header = !deferred_header_comments.is_empty();
        if has_deferred_header {
            for (comment, has_trailing_new_line) in &deferred_header_comments {
                self.write_comment(comment);
                if *has_trailing_new_line {
                    self.write_line();
                } else if comment.starts_with("/*") {
                    self.pending_block_comment_space = true;
                }
            }
        }

        // Emit `var _this = this;` for top-level arrow functions that capture `this`
        if let Some(capture_name) = self
            .transforms
            .this_capture_name(source_idx)
            .map(std::string::ToString::to_string)
        {
            self.write("var ");
            self.write(&capture_name);
            self.write(" = this;");
            self.write_line();
        } else {
            tracing::debug!("[emit] no top-level this capture for source {source_idx:?}");
        }
        self.emit_wrapped_import_interop_prologue(&source.statements);

        // Save position for hoisted temp var declarations (assignment destructuring).
        // After emitting all statements, we'll insert `var _a, _b, ...;` here if needed.
        self.hoisted_assignment_temps.clear();
        self.hoisted_assignment_value_temps.clear();
        self.preallocated_logical_assignment_value_temps.clear();
        self.preallocated_assignment_temps.clear();
        self.hoisted_for_of_temps.clear();
        self.preallocated_temp_names.clear();
        self.reserved_iterator_return_temps.clear();
        self.iterator_for_of_depth = 0;

        self.prepare_logical_assignment_value_temps(source_idx);

        let mut hoisted_var_byte_offset = self.writer.len();
        let mut hoisted_var_line = self.writer.current_line();

        // Emit statements with their leading comments.
        // In this parser, node.pos includes leading trivia (whitespace + comments).
        // Between-statement comments are part of the next node's leading trivia.
        // We find each statement's "actual token start" by scanning forward past
        // trivia, then emit all comments before that position.
        // Pre-scan: collect local names from `export { x, y }` clauses for inline
        // CJS export assignment. tsc emits `exports.X = X;` right after var/class
        // declarations, not at the `export { }` clause position.
        let cjs_deferred_export_names: rustc_hash::FxHashSet<String> = if is_top_level_cjs {
            self.collect_cjs_deferred_export_names(&source.statements)
        } else {
            rustc_hash::FxHashSet::default()
        };
        let cjs_deferred_export_bindings = if is_top_level_cjs {
            self.collect_cjs_deferred_export_bindings(&source.statements)
        } else {
            rustc_hash::FxHashMap::default()
        };

        let mut last_erased_stmt_end: Option<u32> = None;
        let mut last_erased_was_shorthand_module = false;
        let mut deferred_commonjs_export_equals: Vec<NodeIndex> = Vec::new();
        let has_synthesized_esm_import = emitted_tslib_esm_import || emitted_jsx_esm_import;
        let mut has_runtime_module_syntax = has_synthesized_esm_import;
        let mut has_non_empty_runtime_export = has_synthesized_esm_import;
        let mut has_deferred_empty_export = false;
        for (stmt_i, &stmt_idx) in source.statements.nodes.iter().enumerate() {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            if has_top_level_using
                && stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export_decl) = self.arena.get_export_decl(stmt_node)
                && export_decl.module_specifier.is_none()
                && !export_decl.is_default_export
                && let Some(clause_node) = self.arena.get(export_decl.export_clause)
                && clause_node.kind == syntax_kind_ext::NAMED_EXPORTS
                && (is_es_module_output
                    || (!self.has_aliased_value_named_exports(clause_node)
                        && !self.named_exports_have_prior_runtime_declaration(
                            &source.statements,
                            stmt_i,
                            clause_node,
                        )))
            {
                continue;
            }

            if !self.ctx.options.target.supports_es2025()
                && self.statement_is_top_level_using(stmt_node)
            {
                if is_es_module_output
                    && self.has_pre_top_level_using_named_exports(&source.statements, stmt_i)
                {
                    has_runtime_module_syntax = true;
                    has_non_empty_runtime_export = true;
                }
                if is_es_module_output
                    && self.top_level_using_scope_has_runtime_export(&source.statements, stmt_i)
                {
                    has_runtime_module_syntax = true;
                    has_non_empty_runtime_export = true;
                }
                self.emit_top_level_using_scope(
                    &source.statements,
                    stmt_i,
                    is_es_module_output,
                    &cjs_deferred_export_names,
                );
                break;
            }

            let stmt_node_pos = stmt_node.pos;
            // Skip source-level "use strict" prologue when we already emitted it
            // at the correct position (before __esModule/exports preamble).
            if skip_source_use_strict
                && stmt_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
                && let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node)
                && let Some(expr_node) = self.arena.get(expr_stmt.expression)
                && expr_node.kind == SyntaxKind::StringLiteral as u16
            {
                let is_strict = if let Some(lit) = self.arena.get_literal(expr_node) {
                    lit.text == "use strict"
                } else if let Some(text) = self.source_text {
                    let s = crate::safe_slice::slice(
                        text,
                        expr_node.pos as usize,
                        expr_node.end as usize,
                    );
                    s == "\"use strict\"" || s == "'use strict'"
                } else {
                    false
                };
                if is_strict {
                    self.skip_comments_for_erased_node(stmt_node);
                    continue;
                }
            }

            if stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT {
                let is_type_only =
                    self.export_assignment_identifier_is_type_only(stmt_node, &source.statements);

                // Type-only export= (e.g. `export = SomeInterface`) is erased entirely.
                if is_type_only {
                    self.skip_comments_for_erased_node(stmt_node);
                    last_erased_stmt_end = None;
                    last_erased_was_shorthand_module = false;
                    continue;
                }

                let is_export_equals = self
                    .arena
                    .get_export_assignment(stmt_node)
                    .is_some_and(|ea| ea.is_export_equals);

                // In CJS mode, value `export = X` is deferred for `module.exports = X` emission.
                if self.ctx.is_commonjs() && is_export_equals {
                    deferred_commonjs_export_equals.push(stmt_idx);
                    last_erased_stmt_end = None;
                    last_erased_was_shorthand_module = false;
                    continue;
                }
            }

            // Defer `export {}` (empty named exports, no module specifier) to end
            // of file. TSC places these at the end as ESM markers.
            if is_es_module_output
                && stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export) = self.arena.get_export_decl(stmt_node)
                && export.module_specifier.is_none()
                && let Some(clause_node) = self.arena.get(export.export_clause)
                && clause_node.kind == syntax_kind_ext::NAMED_EXPORTS
                && let Some(named_exports) = self.arena.get_named_imports(clause_node)
                && named_exports.elements.nodes.is_empty()
            {
                has_deferred_empty_export = true;
                has_runtime_module_syntax = true;
                self.skip_comments_for_erased_node(stmt_node);
                last_erased_stmt_end = None;
                last_erased_was_shorthand_module = false;
                continue;
            }

            // For erased declarations (type-only, ambient, etc.) in JS emit mode,
            // skip their leading comments entirely - they should not appear in output.
            let is_erased =
                !self.ctx.flags.in_declaration_emit && self.is_erased_statement(stmt_node);

            // Skip empty statements (`;`) that follow an erased shorthand module
            // declaration (`declare module "foo";`). The shorthand module syntax
            // parses as MODULE_DECLARATION + EMPTY_STATEMENT, and the trailing
            // `;` should be erased along with the declaration.
            if !is_erased
                && stmt_node.kind == syntax_kind_ext::EMPTY_STATEMENT
                && last_erased_was_shorthand_module
            {
                last_erased_was_shorthand_module = false;
                continue;
            }

            // Detect whether this statement is a module-indicating statement
            // (import/export). We track this BEFORE emit but only confirm it as
            // "runtime module syntax" AFTER emit, because the emit step may decide
            // to erase it (e.g., text heuristic determines all imported names are
            // type-only and drops the import).
            let is_module_indicating_stmt = if !is_erased {
                let k = stmt_node.kind;
                if k == syntax_kind_ext::IMPORT_DECLARATION
                    || k == syntax_kind_ext::EXPORT_DECLARATION
                    || k == syntax_kind_ext::EXPORT_ASSIGNMENT
                {
                    true
                } else if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                    // Only external module imports (`import x = require("mod")`)
                    // count as runtime module syntax. Namespace aliases
                    // (`import x = M.A`) are erased and should not suppress
                    // deferred `export {};` emission.
                    self.arena
                        .get_import_decl(stmt_node)
                        .and_then(|import_data| self.arena.get(import_data.module_specifier))
                        .is_some_and(|spec_node| {
                            spec_node.kind == SyntaxKind::StringLiteral as u16
                                || spec_node.kind == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE
                        })
                } else {
                    false
                }
            } else {
                false
            };

            // Find the actual start of the statement's first token by
            // scanning forward from node.pos past ALL trivia (whitespace AND
            // comments). This way `c_end <= actual_start` correctly identifies
            // every leading comment whose text ends before the real token.
            // Previously we used skip_whitespace_forward which stopped at
            // comments, causing `c_end > actual_start` for the comment itself.
            let actual_start = self.skip_trivia_forward(stmt_node.pos, stmt_node.end);

            if is_erased {
                // Skip erased declarations. Their leading comments were already
                // filtered out of all_comments during initialization.
                // Also consume trailing same-line comments for the erased statement
                // (e.g., `declare var a: boolean; // comment` should be erased too).
                // We use the end-of-line of the last token as the boundary:
                //   - Comments on the same line as the last token → consume (erase)
                //   - Comments on subsequent lines → keep for the next statement
                // Cap the scan end at the next statement's pos to avoid scanning
                // into subsequent statements (same fix as initialization phase).
                let next_stmt_node = source
                    .statements
                    .nodes
                    .get(stmt_i + 1)
                    .and_then(|&next_idx| self.arena.get(next_idx));
                let scan_end = next_stmt_node.map_or(stmt_node.end, |n| n.pos);
                let stmt_token_end = self.find_token_end_before_trivia(stmt_node.pos, scan_end);
                let mut line_end = if let Some(text) = self.source_text {
                    let bytes = text.as_bytes();
                    let mut pos = stmt_token_end as usize;
                    while pos < bytes.len() && bytes[pos] != b'\n' && bytes[pos] != b'\r' {
                        pos += 1;
                    }
                    pos as u32
                } else {
                    stmt_token_end
                };
                // If the next sibling statement is on the same line and is NOT
                // erased, cap the comment consumption at the next statement's start.
                // Comments after a non-erased sibling (e.g., `interface Foo {}; // Error`)
                // belong to that sibling, not the erased declaration.
                if let Some(next_node) = next_stmt_node {
                    let next_is_erased = self.is_erased_statement(next_node);
                    if !next_is_erased && scan_end < line_end {
                        // The next statement starts before the line ends — it's on
                        // the same line. Cap to avoid consuming its trailing comments.
                        line_end = scan_end;
                    }
                }
                while self.comment_emit_idx < self.all_comments.len() {
                    let c_end = self.all_comments[self.comment_emit_idx].end;
                    if c_end <= line_end {
                        self.comment_emit_idx += 1;
                    } else {
                        break;
                    }
                }
                last_erased_stmt_end = Some(line_end);
                // Track if this is a shorthand module declaration (no body),
                // so we can skip the trailing EMPTY_STATEMENT (`;`).
                last_erased_was_shorthand_module = stmt_node.kind
                    == syntax_kind_ext::MODULE_DECLARATION
                    && self
                        .arena
                        .get_module(stmt_node)
                        .is_some_and(|m| m.body.is_none());
                continue;
            }

            // Emit comments whose end position is at or before the actual token start.
            // These are truly "leading" comments for this statement.
            // Comments inside expressions (like call arguments) have positions AFTER
            // the statement's first token, so they won't be emitted here.
            let defer_for_of_comments = stmt_node.kind == syntax_kind_ext::FOR_OF_STATEMENT
                && self.should_defer_for_of_comments(stmt_node);
            let skip_auto_accessor_leading_comments = stmt_node.kind
                == syntax_kind_ext::CLASS_DECLARATION
                && self
                    .arena
                    .get_class(stmt_node)
                    .is_some_and(|class| self.class_has_auto_accessor_members(class));
            // Save state before leading comments so we can undo them if the
            // statement produces no output (same pattern as emit_block_body).
            let pre_comment_writer_len = self.writer.len();
            let pre_comment_idx = self.comment_emit_idx;
            if !defer_for_of_comments
                && !skip_auto_accessor_leading_comments
                && let Some(text) = self.source_text
            {
                while self.comment_emit_idx < self.all_comments.len() {
                    let c_pos = self.all_comments[self.comment_emit_idx].pos;
                    let c_end = self.all_comments[self.comment_emit_idx].end;
                    let c_trailing = self.all_comments[self.comment_emit_idx].has_trailing_new_line;
                    if let Some(erased_end) = last_erased_stmt_end
                        && c_end <= erased_end
                    {
                        // Comment was part of a recently erased declaration; discard it.
                        self.comment_emit_idx += 1;
                        continue;
                    }
                    // Only emit if this comment ends before the statement's first token
                    // AND hasn't been emitted by a nested expression emitter
                    if c_end <= actual_start {
                        let comment_text =
                            crate::safe_slice::slice(text, c_pos as usize, c_end as usize);
                        self.write_comment_with_reindent(comment_text, Some(c_pos));
                        if c_trailing {
                            self.write_line();
                        } else if comment_text.starts_with("/*") {
                            self.pending_block_comment_space = true;
                        }
                        self.comment_emit_idx += 1;
                    } else {
                        break;
                    }
                }
            }
            last_erased_stmt_end = None;
            last_erased_was_shorthand_module = false;
            let next_stmt_pos = source
                .statements
                .nodes
                .get(stmt_i + 1)
                .and_then(|&idx| self.arena.get(idx))
                .map(|next_node| next_node.pos);

            let before_len = self.writer.len();
            self.emit(stmt_idx);
            let emitted_output = self.writer.len() > before_len;
            let mut handled_legacy_decorated_deferred_export = false;

            if emitted_output
                && is_top_level_cjs
                && self.ctx.options.legacy_decorators
                && !self.ctx.target_es5
                && stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION
                && let Some(class_decl) = self.arena.get_class(stmt_node)
                && !self
                    .collect_class_decorators(&class_decl.modifiers)
                    .is_empty()
                && let Some(local_name) = self.get_identifier_text_opt(class_decl.name)
                && let Some(export_name) = cjs_deferred_export_bindings.get(&local_name)
            {
                let after_len = self.writer.len();
                let full_output = self.writer.get_output().to_string();
                let emitted = full_output[before_len..after_len].to_string();
                let rewritten = self.rewrite_legacy_top_level_using_class_export(
                    emitted,
                    &local_name,
                    export_name,
                );
                self.writer.truncate(before_len);
                self.write(&rewritten);
                self.ctx
                    .module_state
                    .inline_exported_names
                    .insert(export_name.clone());
                handled_legacy_decorated_deferred_export = true;
            }

            // CJS: emit inline `exports.X = X;` right after var/class declarations
            // whose names appear in a later `export { X }` clause. This matches
            // tsc's interleaved ordering where exports follow their declarations.
            if emitted_output
                && !handled_legacy_decorated_deferred_export
                && !cjs_deferred_export_names.is_empty()
            {
                let names = self.get_declaration_export_names(stmt_node);
                for name in names {
                    if cjs_deferred_export_names.contains(name.as_str())
                        && !self.ctx.module_state.iife_exported_names.contains(&name)
                    {
                        if !self.writer.is_at_line_start() {
                            self.write_line();
                        }
                        self.write("exports.");
                        self.write(&name);
                        self.write(" = ");
                        self.write(&name);
                        self.write(";");
                        self.ctx.module_state.inline_exported_names.insert(name);
                    }
                }
            }

            // Track runtime module syntax AFTER emission: if a module-indicating
            // statement actually produced output, it contributes runtime module
            // syntax. If the emit erased it (e.g., text heuristic determined all
            // imported names are type-only), it should not prevent `export {};`.
            if is_module_indicating_stmt && emitted_output {
                has_runtime_module_syntax = true;
                has_non_empty_runtime_export = true;
            }

            // Only add newline if something was actually emitted
            if emitted_output {
                // Once a real statement produces output, its deferred header
                // comments are "claimed" and should not be undone.
                has_deferred_header = false;
                // Emit trailing comments on the same line as the statement.
                // Use the next statement's pos as upper bound to avoid scanning
                // into the next statement's trivia (same pattern as emit_block_body).
                if self.writer.is_at_line_start() {
                    // The emission already wrote a final newline (e.g., CJS inline
                    // export, transform dispatch). Undo it so trailing comments
                    // can be appended to the last output line, then re-add the
                    // newline after.
                    if !self.ctx.options.remove_comments {
                        let saved_idx = self.comment_emit_idx;
                        let upper_bound = next_stmt_pos.unwrap_or(stmt_node.end);
                        let token_end =
                            self.find_token_end_before_trivia(stmt_node_pos, upper_bound);
                        // Peek: check if there are trailing comments to emit.
                        // Only backtrack if there actually are comments to add.
                        if self.has_trailing_comment_on_same_line(token_end, upper_bound) {
                            self.comment_emit_idx = saved_idx;
                            self.writer.undo_last_write_line();
                            self.emit_trailing_comments_before(token_end, upper_bound);
                            self.write_line();
                        }
                    }
                } else {
                    let upper_bound = next_stmt_pos.unwrap_or(stmt_node.end);
                    let token_end = self.find_token_end_before_trivia(stmt_node_pos, upper_bound);
                    self.emit_trailing_comments_before(token_end, upper_bound);
                    self.write_line();
                }
            } else if !is_erased {
                // Statement produced no output but wasn't formally erased (e.g.,
                // `export var x: Type;` in CJS where the export was hoisted to the
                // preamble, or an import that was elided by the text heuristic).
                // Undo any leading comments we emitted before it, then consume
                // trailing same-line comments so they don't leak to the next
                // statement's leading comment emission.
                //
                // Also undo deferred header comments if this is the first statement
                // they were attached to (e.g., `/** b's comment*/` before
                // `export var b: number;` in CJS/AMD mode).
                let truncate_to =
                    if has_deferred_header && self.writer.len() > pre_deferred_comments_len {
                        has_deferred_header = false;
                        pre_deferred_comments_len
                    } else if self.writer.len() > pre_comment_writer_len {
                        pre_comment_writer_len
                    } else {
                        self.writer.len()
                    };
                if truncate_to < self.writer.len() {
                    self.writer.truncate(truncate_to);
                    self.comment_emit_idx = pre_comment_idx;
                }
                // Skip leading comments (advance past them without emitting)
                while self.comment_emit_idx < self.all_comments.len() {
                    if self.all_comments[self.comment_emit_idx].end <= actual_start {
                        self.comment_emit_idx += 1;
                    } else {
                        break;
                    }
                }
                let scan_end = next_stmt_pos.unwrap_or(stmt_node.end);
                let stmt_token_end = self.find_token_end_before_trivia(stmt_node_pos, scan_end);
                let line_end = if let Some(text) = self.source_text {
                    let bytes = text.as_bytes();
                    let mut pos = stmt_token_end as usize;
                    while pos < bytes.len() && bytes[pos] != b'\n' && bytes[pos] != b'\r' {
                        pos += 1;
                    }
                    pos as u32
                } else {
                    stmt_token_end
                };
                while self.comment_emit_idx < self.all_comments.len() {
                    let c_end = self.all_comments[self.comment_emit_idx].end;
                    if c_end <= line_end {
                        self.comment_emit_idx += 1;
                    } else {
                        break;
                    }
                }
                last_erased_stmt_end = Some(line_end);
            }

            // Note: We do NOT skip inner comments here. The "emit comments before
            // statement" logic (above) uses actual_start which is computed by
            // skip_trivia_forward. Inner comments (inside function/class bodies)
            // have positions that are BEFORE the next top-level statement's actual
            // start, so they won't be emitted at the wrong level. They'll be
            // naturally consumed when we encounter the statement that contains them.

            // After emitting a prologue directive (string literal expression statement
            // like "use strict"), update the hoisted var insertion point to AFTER it.
            // tsc places hoisted temp vars after all prologue directives.
            if emitted_output
                && stmt_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
                && let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node)
                && let Some(expr_node) = self.arena.get(expr_stmt.expression)
                && expr_node.kind == SyntaxKind::StringLiteral as u16
            {
                hoisted_var_byte_offset = self.writer.len();
                hoisted_var_line = self.writer.current_line();
            }
        }

        // TypeScript emits CommonJS `export =` assignments after declaration output,
        // even when they appear earlier in source.
        // tsc only emits the FIRST `export =` when multiple exist (duplicates are errors).
        if let Some(&first_export_eq) = deferred_commonjs_export_equals.first() {
            let before_len = self.writer.len();
            self.emit(first_export_eq);
            if self.writer.len() > before_len && !self.writer.is_at_line_start() {
                self.write_line();
            }
        }

        // Emit deferred `export {}` at end of file (moved from its source position),
        // but only when no other non-erased import/export statements exist. When the
        // file has real exports (e.g. `export { C };`), the `export {};` is redundant
        // and tsc omits it.
        if has_deferred_empty_export && !has_non_empty_runtime_export {
            self.write("export {};");
            self.write_line();
        }

        // When a file is an ES module but all import/export statements were erased
        // (all type-only), emit `export {};` to preserve module semantics.
        // This matches tsc behavior: the file must remain an ES module even if
        // all its import/export syntax was type-only and got stripped.
        if is_file_module && is_es_module_output && !has_runtime_module_syntax {
            self.write("export {};");
            self.write_line();
        }

        // Emit remaining trailing comments at the end of file
        if let Some(text) = self.source_text {
            while self.comment_emit_idx < self.all_comments.len() {
                let c_pos = self.all_comments[self.comment_emit_idx].pos;
                let c_end = self.all_comments[self.comment_emit_idx].end;
                let c_trailing = self.all_comments[self.comment_emit_idx].has_trailing_new_line;
                let comment_text = crate::safe_slice::slice(text, c_pos as usize, c_end as usize);
                self.write_comment_with_reindent(comment_text, Some(c_pos));
                if c_trailing {
                    self.write_line();
                }
                self.comment_emit_idx += 1;
            }
        }

        // Insert hoisted temp declarations (for-of iterator lowering + assignment destructuring).
        let mut ref_vars = Vec::new();
        ref_vars.extend(self.hoisted_assignment_temps.iter().cloned());
        ref_vars.extend(self.hoisted_for_of_temps.iter().cloned());

        if !ref_vars.is_empty() {
            let var_decl = format!("var {};", ref_vars.join(", "));
            self.writer
                .insert_line_at(hoisted_var_byte_offset, hoisted_var_line, &var_decl);
        }

        if !self.hoisted_assignment_value_temps.is_empty() {
            let var_decl = format!("var {};", self.hoisted_assignment_value_temps.join(", "));
            self.writer
                .insert_line_at(hoisted_var_byte_offset, hoisted_var_line, &var_decl);
        }

        // Insert CJS destructuring export temps before the __esModule marker.
        if !self.cjs_destructuring_export_temps.is_empty() {
            let var_decl = format!("var {};", self.cjs_destructuring_export_temps.join(", "));
            self.writer.insert_line_at(
                self.cjs_destr_hoist_byte_offset,
                self.cjs_destr_hoist_line,
                &var_decl,
            );
        }

        // Emit cached template object variables at the END of the file for modules.
        if has_es5_transforms && helpers.make_template_object && is_file_module {
            let template_vars = self.collect_tagged_template_vars();
            if !template_vars.is_empty() {
                if !self.writer.is_at_line_start() {
                    self.write_line();
                }
                self.write("var ");
                self.write(&template_vars.join(", "));
                self.write(";");
                self.write_line();
            }
        }

        // Ensure output ends with a newline (matching tsc behavior)
        if !self.writer.is_at_line_start() {
            self.write_line();
        }

        // Exit root scope for block-scoped variable tracking
        self.ctx.block_scope_state.exit_scope();
    }
}
