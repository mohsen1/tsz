use super::Printer;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    // =========================================================================
    // Source File
    // =========================================================================

    pub(super) fn emit_source_file(&mut self, node: &Node, source_idx: NodeIndex) {
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
            ModuleKind::Node16 | ModuleKind::NodeNext
        ) {
            let file_name = source.file_name.to_ascii_lowercase();
            let is_explicit_esm = file_name.ends_with(".mts") || file_name.ends_with(".mjs");
            self.ctx.options.module = if is_explicit_esm {
                ModuleKind::ESNext
            } else {
                ModuleKind::CommonJS
            };
        }

        // Detect export assignment (export =) to suppress other exports
        if self.has_export_assignment(&source.statements) {
            self.ctx.module_state.has_export_assignment = true;
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

        // Enter root scope for block-scoped variable tracking and `var` scope boundaries.
        // This ensures variables declared throughout the file are tracked for renaming.
        self.ctx.block_scope_state.enter_function_scope();

        // Extract comments. Triple-slash references (/// <reference ...>) are
        // preserved in output — tsc keeps them in JS emit for most cases.
        // Only AMD-specific directives (/// <amd ...) are stripped.
        // When inside a module wrapper (AMD/UMD/System), `/// <reference` directives
        // are also stripped because they were already emitted before the wrapper.
        // Store on self so nested blocks can also distribute comments.
        let inside_module_wrapper = self.ctx.original_module_kind.is_some();
        self.all_comments = if !self.ctx.options.remove_comments {
            if let Some(text) = self.source_text {
                tsz_common::comments::get_comment_ranges(text)
                    .into_iter()
                    .filter(|c| {
                        let content = c.get_text(text);
                        if content.starts_with("/// <amd") {
                            return false;
                        }
                        // When inside a module wrapper, reference directives were
                        // already emitted before define()/wrapper — skip them here.
                        if inside_module_wrapper {
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
        let mut first_erased_stmt_pos: Option<u32> = None;
        if !self.ctx.flags.in_declaration_emit && !self.all_comments.is_empty() {
            let mut erased_ranges: Vec<(u32, u32)> = Vec::new();
            let mut prev_end: Option<u32> = None;
            for &stmt_idx in &source.statements.nodes {
                if let Some(stmt_node) = self.arena.get(stmt_idx) {
                    let stmt_token_end =
                        self.find_token_end_before_trivia(stmt_node.pos, stmt_node.end);
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
                        let range_start = if let Some(pe) = prev_end {
                            pe
                        } else {
                            // For the first erased statement, preserve file-level
                            // comments by starting the erased range at the statement
                            // itself. The header comment loop will filter out
                            // attached comments separately.
                            first_erased_stmt_pos = Some(stmt_node.pos);
                            stmt_node.pos
                        };
                        erased_ranges.push((range_start, stmt_token_end));
                    }
                    prev_end = Some(stmt_token_end);
                }
            }
            if !erased_ranges.is_empty() {
                self.all_comments.retain(|c| {
                    !erased_ranges
                        .iter()
                        .any(|&(start, end)| c.pos >= start && c.end <= end)
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

        // TypeScript emits "use strict" when:
        // 1. CommonJS AND the file is actually an ES module (has import/export).
        //    Script files (no import/export) don't get "use strict".
        // 2. AMD/UMD module files get "use strict" inside their define() wrapper,
        //    NOT at the top level. Non-module scripts under AMD/UMD don't get it.
        //    Pre-bundled files with define() wrappers already have it inside.
        // 3. alwaysStrict is on AND the file is not already an ES module output.
        let is_file_module = self.file_is_module(&source.statements);
        let has_define_wrapper_stmt = source.statements.nodes.iter().any(|&idx| {
            self.arena
                .get(idx)
                .and_then(|stmt| self.arena.get_expression_statement(stmt))
                .and_then(|expr_stmt| self.arena.get(expr_stmt.expression))
                .and_then(|expr| self.arena.get_call_expr(expr))
                .and_then(|call| self.arena.get(call.expression))
                .and_then(|callee| self.arena.get_identifier(callee))
                .is_some_and(|ident| ident.escaped_text.as_str() == "define")
        });
        // Note: AMD/UMD module files are handled by emit_module_wrapper() before
        // reaching here (line ~647), so conditions below only affect:
        //   - non-module scripts under any module kind
        //   - outFile bundles with pre-existing define() wrappers
        // Whether we need "use strict" at the top of this output.
        let needs_use_strict_cjs = is_top_level_cjs && is_file_module;
        let needs_use_strict_amd_umd = is_amd_or_umd && is_file_module && !has_define_wrapper_stmt;
        let needs_use_strict_always = self.ctx.options.always_strict
            && !has_define_wrapper_stmt
            && self.ctx.original_module_kind.is_none()
            && !(is_es_module_output && is_file_module);

        let should_emit_use_strict = !source_has_use_strict
            && (needs_use_strict_cjs || needs_use_strict_amd_umd || needs_use_strict_always);

        // When the source has its own "use strict" prologue AND this is a CJS
        // module file, we must emit "use strict" at the correct position (before
        // __esModule marker / exports preamble) and skip the source's own
        // directive during statement iteration to avoid duplication.
        let skip_source_use_strict = source_has_use_strict && needs_use_strict_cjs;

        if should_emit_use_strict || skip_source_use_strict {
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

        let mut deferred_header_comments: Vec<(String, bool)> = Vec::new();
        let is_commonjs = self.ctx.is_commonjs();
        if let Some(text) = self.source_text {
            while self.comment_emit_idx < self.all_comments.len() {
                let c_end = self.all_comments[self.comment_emit_idx].end;
                if c_end <= first_stmt_pos {
                    let c_pos = self.all_comments[self.comment_emit_idx].pos;
                    let c_trailing = self.all_comments[self.comment_emit_idx].has_trailing_new_line;

                    // Skip comments that are directly attached to an erased first
                    // statement (no blank line between comment and declaration).
                    // Detached comments (separated by blank line) are preserved.
                    if let Some(erased_pos) = first_erased_stmt_pos {
                        let between = &text[c_end as usize..erased_pos as usize];
                        let has_blank_line =
                            between.contains("\n\n") || between.contains("\r\n\r\n");
                        if !has_blank_line {
                            self.comment_emit_idx += 1;
                            continue;
                        }
                    }

                    let comment_text =
                        crate::safe_slice::slice(text, c_pos as usize, c_end as usize);
                    let trimmed_comment = comment_text.trim_start();
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
                        .map_or(first_stmt_pos, |next_c| next_c.pos);
                    let between_after = &text[c_end as usize..next_content_pos as usize];
                    let is_detached =
                        between_after.contains("\n\n") || between_after.contains("\r\n\r\n");
                    let is_amd_dependency = trimmed_comment.contains("<amd-dependency");
                    let is_triple_slash_reference = trimmed_comment.starts_with("///<reference");
                    if is_commonjs
                        && (is_triple_slash_reference || (!is_detached && !is_amd_dependency))
                    {
                        deferred_header_comments.push((comment_text.to_string(), c_trailing));
                    } else {
                        self.write_comment_with_reindent(comment_text, Some(c_pos));
                        if c_trailing {
                            self.write_line();
                        }
                    }
                    self.comment_emit_idx += 1;
                } else {
                    break;
                }
            }
        }

        // Emit runtime helpers (must come BEFORE __esModule marker)
        // Order: "use strict" → helpers → __esModule → exports init

        // Use helpers from TransformContext (populated during lowering pass)
        // This eliminates O(N) arena scans - all helpers are detected in Phase 1
        let helpers = if self.transforms.helpers_populated() {
            self.transforms.helpers().clone()
        } else {
            // Fallback for non-transforming emits (should be rare)
            // In normal operation, LoweringPass always marks helpers_populated = true
            crate::transforms::helpers::HelpersNeeded::default()
        };

        let has_es5_transforms = self.has_es5_transforms();

        // Emit all needed helpers (unless no_emit_helpers is set)
        if !self.ctx.options.no_emit_helpers {
            let helpers_code = crate::transforms::helpers::emit_helpers(&helpers);
            if !helpers_code.is_empty() {
                self.write(&helpers_code);
                // emit_helpers() already adds newlines, no need to add more
            }
        }

        if has_es5_transforms && helpers.make_template_object {
            let template_vars = self.collect_tagged_template_vars();
            if !template_vars.is_empty() {
                self.write("var ");
                self.write(&template_vars.join(", "));
                self.write(";");
                self.write_line();
            }
        }

        // CommonJS: Emit __esModule and exports initialization (AFTER helpers)
        if self.ctx.is_commonjs() {
            use crate::transforms::module_commonjs;

            // Emit __esModule if this is an ES module
            if self.should_emit_es_module_marker(&source.statements) {
                self.write("Object.defineProperty(exports, \"__esModule\", { value: true });");
                self.write_line();
            }

            // Collect and emit exports initialization
            // Function exports get direct assignment (hoisted), others get void 0
            let (func_exports, other_exports, default_func_export) =
                module_commonjs::collect_export_names_categorized(
                    self.arena,
                    &source.statements.nodes,
                );
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
            // Emit other exports first: exports.X = void 0;
            // TypeScript emits void 0 initialization before hoisted function exports
            if !other_exports.is_empty() {
                for (i, name) in other_exports.iter().enumerate() {
                    if i > 0 {
                        self.write(" = ");
                    }
                    self.write("exports.");
                    self.write(name);
                }
                self.write(" = void 0;");
                self.write_line();
            }
            // Emit function exports: exports.compile = compile;
            for name in &func_exports {
                self.write("exports.");
                self.write(name);
                self.write(" = ");
                self.write(name);
                self.write(";");
                self.write_line();
            }
            // Emit hoisted default function export: exports.default = funcName;
            // `export default function func() {}` is hoisted like named exports.
            let default_func_export = if self.ctx.module_state.has_export_assignment {
                None
            } else {
                default_func_export
            };
            if let Some(ref name) = default_func_export {
                self.write("exports.default = ");
                self.write(name);
                self.write(";");
                self.write_line();
            }
            self.ctx.module_state.default_func_export_hoisted = default_func_export.is_some();
        }

        if !deferred_header_comments.is_empty() {
            for (comment, has_trailing_new_line) in &deferred_header_comments {
                self.write_comment(comment);
                if *has_trailing_new_line {
                    self.write_line();
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

        let hoisted_var_byte_offset = self.writer.len();
        let hoisted_var_line = self.writer.current_line();

        // Emit statements with their leading comments.
        // In this parser, node.pos includes leading trivia (whitespace + comments).
        // Between-statement comments are part of the next node's leading trivia.
        // We find each statement's "actual token start" by scanning forward past
        // trivia, then emit all comments before that position.
        let mut last_erased_stmt_end: Option<u32> = None;
        let mut last_erased_was_shorthand_module = false;
        let mut deferred_commonjs_export_equals: Vec<NodeIndex> = Vec::new();
        let mut has_runtime_module_syntax = false;
        let mut has_deferred_empty_export = false;
        for &stmt_idx in &source.statements.nodes {
            if let Some(stmt_node) = self.arena.get(stmt_idx) {
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

                if stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                    && self.export_assignment_identifier_is_type_only(stmt_node, &source.statements)
                {
                    self.skip_comments_for_erased_node(stmt_node);
                    last_erased_stmt_end = None;
                    last_erased_was_shorthand_module = false;
                    continue;
                }

                if self.ctx.is_commonjs()
                    && stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                    && self
                        .arena
                        .get_export_assignment(stmt_node)
                        .is_some_and(|ea| ea.is_export_equals)
                {
                    deferred_commonjs_export_equals.push(stmt_idx);
                    last_erased_stmt_end = None;
                    last_erased_was_shorthand_module = false;
                    continue;
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

                // Track whether any non-erased module-indicating statement exists
                // (needed for `export {};` insertion at end of file)
                if !is_erased && !has_runtime_module_syntax {
                    let k = stmt_node.kind;
                    if k == syntax_kind_ext::IMPORT_DECLARATION
                        || k == syntax_kind_ext::EXPORT_DECLARATION
                        || k == syntax_kind_ext::EXPORT_ASSIGNMENT
                        || k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                    {
                        has_runtime_module_syntax = true;
                    }
                }

                // Find the actual start of the statement's first token by
                // scanning forward from node.pos past whitespace only.
                // We preserve comments here - they're handled either as leading
                // comments (if truly before the statement) or by nested expression emitters.
                let actual_start = self.skip_whitespace_forward(stmt_node.pos, stmt_node.end);

                if is_erased {
                    // Skip erased declarations. Their leading comments were already
                    // filtered out of all_comments during initialization.
                    // Also consume trailing same-line comments for the erased statement
                    // (e.g., `declare var a: boolean; // comment` should be erased too).
                    // We use the end-of-line of the last token as the boundary:
                    //   - Comments on the same line as the last token → consume (erase)
                    //   - Comments on subsequent lines → keep for the next statement
                    let stmt_token_end =
                        self.find_token_end_before_trivia(stmt_node.pos, stmt_node.end);
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
                if !defer_for_of_comments
                    && !skip_auto_accessor_leading_comments
                    && let Some(text) = self.source_text
                {
                    while self.comment_emit_idx < self.all_comments.len() {
                        let c_pos = self.all_comments[self.comment_emit_idx].pos;
                        let c_end = self.all_comments[self.comment_emit_idx].end;
                        let c_trailing =
                            self.all_comments[self.comment_emit_idx].has_trailing_new_line;
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
                            self.write(comment_text);
                            if c_trailing {
                                self.write_line();
                            }
                            self.comment_emit_idx += 1;
                        } else {
                            break;
                        }
                    }
                }
                last_erased_stmt_end = None;
                last_erased_was_shorthand_module = false;
            }

            let before_len = self.writer.len();
            self.emit(stmt_idx);
            // Only add newline if something was actually emitted
            if self.writer.len() > before_len && !self.writer.is_at_line_start() {
                // Emit trailing comments on the same line as the statement.
                // Use the next statement's pos as upper bound to avoid scanning
                // into the next statement's trivia (same pattern as emit_block_body).
                if let Some(stmt_node) = self.arena.get(stmt_idx) {
                    let stmts = &source.statements.nodes;
                    let stmt_i = stmts.iter().position(|&s| s == stmt_idx);
                    let next_pos = stmt_i.and_then(|i| {
                        stmts
                            .get(i + 1)
                            .and_then(|&next_idx| self.arena.get(next_idx))
                            .map(|n| n.pos)
                    });
                    let upper_bound = next_pos.unwrap_or(stmt_node.end);
                    let token_end = self.find_token_end_before_trivia(stmt_node.pos, upper_bound);
                    self.emit_trailing_comments(token_end);
                }
                self.write_line();
            }

            // Note: We do NOT skip inner comments here. The "emit comments before
            // statement" logic (above) uses actual_start which is computed by
            // skip_trivia_forward. Inner comments (inside function/class bodies)
            // have positions that are BEFORE the next top-level statement's actual
            // start, so they won't be emitted at the wrong level. They'll be
            // naturally consumed when we encounter the statement that contains them.
        }

        // TypeScript emits CommonJS `export =` assignments after declaration output,
        // even when they appear earlier in source.
        for stmt_idx in deferred_commonjs_export_equals {
            let before_len = self.writer.len();
            self.emit(stmt_idx);
            if self.writer.len() > before_len && !self.writer.is_at_line_start() {
                self.write_line();
            }
        }

        // Emit deferred `export {}` at end of file (moved from its source position).
        if has_deferred_empty_export {
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
                self.write(comment_text);
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

        // Ensure output ends with a newline (matching tsc behavior)
        if !self.writer.is_at_line_start() {
            self.write_line();
        }

        // Exit root scope for block-scoped variable tracking
        self.ctx.block_scope_state.exit_scope();
    }

    pub(super) fn should_defer_for_of_comments(&self, node: &Node) -> bool {
        let for_of = match self.arena.get_for_in_of(node) {
            Some(for_of) => for_of,
            None => return false,
        };

        if for_of.await_modifier {
            return !self.ctx.options.target.supports_es2018();
        }

        self.ctx.target_es5 && self.ctx.options.downlevel_iteration
    }
}
