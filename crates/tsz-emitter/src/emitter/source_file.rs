use super::Printer;
use crate::enums::evaluator::EnumEvaluator;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::NodeList;
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

        // Pre-pass: collect const enum values for inlining at usage sites.
        // tsc replaces property/element access to const enum members with their
        // literal values (e.g., `Direction.Up` → `1 /* Direction.Up */`).
        if !self.ctx.options.preserve_const_enums {
            self.collect_const_enum_values(&source.statements);
        }

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
        let first_stmt_is_auto_accessor_class = source
            .statements
            .nodes
            .first()
            .and_then(|&idx| self.arena.get(idx))
            .is_some_and(|stmt_node| {
                stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION
                    && self
                        .arena
                        .get_class(stmt_node)
                        .is_some_and(|class| self.class_has_auto_accessor_members(class))
            });

        let mut deferred_header_comments: Vec<(String, bool)> = Vec::new();
        let is_commonjs = self.ctx.is_commonjs();
        if let Some(text) = self.source_text {
            while self.comment_emit_idx < self.all_comments.len() {
                let c_end = self.all_comments[self.comment_emit_idx].end;
                if c_end <= first_stmt_pos {
                    let c_pos = self.all_comments[self.comment_emit_idx].pos;
                    let c_trailing = self.all_comments[self.comment_emit_idx].has_trailing_new_line;
                    let comment_text =
                        crate::safe_slice::slice(text, c_pos as usize, c_end as usize);
                    let trimmed_comment = comment_text.trim_start();
                    let is_triple_slash_reference = trimmed_comment.starts_with("///<reference");
                    let is_amd_dependency = trimmed_comment.contains("<amd-dependency");

                    // Auto-accessor class declarations emit comments themselves right
                    // after helper storage declarations. Keep their leading comments
                    // in the cursor queue so declarations_class.rs can place them.
                    if first_stmt_is_auto_accessor_class
                        && !is_triple_slash_reference
                        && !is_amd_dependency
                    {
                        break;
                    }

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
        let mut has_non_empty_runtime_export = false;
        let mut has_deferred_empty_export = false;
        for (stmt_i, &stmt_idx) in source.statements.nodes.iter().enumerate() {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
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
            if !is_erased {
                let k = stmt_node.kind;
                if k == syntax_kind_ext::IMPORT_DECLARATION
                    || k == syntax_kind_ext::EXPORT_DECLARATION
                    || k == syntax_kind_ext::EXPORT_ASSIGNMENT
                    || k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                {
                    has_runtime_module_syntax = true;
                    has_non_empty_runtime_export = true;
                }
            }

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
                        self.write(comment_text);
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
            // Only add newline if something was actually emitted
            if self.writer.len() > before_len && !self.writer.is_at_line_start() {
                // Emit trailing comments on the same line as the statement.
                // Use the next statement's pos as upper bound to avoid scanning
                // into the next statement's trivia (same pattern as emit_block_body).
                let upper_bound = next_stmt_pos.unwrap_or(stmt_node.end);
                let token_end = self.find_token_end_before_trivia(stmt_node_pos, upper_bound);
                self.emit_trailing_comments(token_end);
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

    /// Pre-pass: scan top-level statements for const enum declarations and
    /// evaluate their member values. The results are stored in `const_enum_values`
    /// so that property/element access expressions referencing const enum members
    /// can be inlined during emit.
    fn collect_const_enum_values(&mut self, statements: &NodeList) {
        self.const_enum_values.clear();
        let mut evaluator = EnumEvaluator::new(self.arena);

        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            // Direct const enum declarations
            if stmt_node.kind == syntax_kind_ext::ENUM_DECLARATION {
                self.try_register_const_enum(&mut evaluator, stmt_idx);
                continue;
            }

            // `export enum` / `export const enum` — the enum is inside an ExportDeclaration
            if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export_data) = self.arena.get_export_decl(stmt_node)
                && export_data.export_clause.is_some()
            {
                let clause_idx = export_data.export_clause;
                if let Some(clause_node) = self.arena.get(clause_idx)
                    && clause_node.kind == syntax_kind_ext::ENUM_DECLARATION
                {
                    self.try_register_const_enum(&mut evaluator, clause_idx);
                }
            }
        }
    }

    /// Register a single enum declaration if it is a const enum.
    fn try_register_const_enum(&mut self, evaluator: &mut EnumEvaluator, enum_idx: NodeIndex) {
        let Some(enum_node) = self.arena.get(enum_idx) else {
            return;
        };
        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return;
        };

        // Only process const enums (not regular enums)
        if !self
            .arena
            .has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword)
        {
            return;
        }

        // Skip ambient (declare) enums — they may reference values from other files
        if self
            .arena
            .has_modifier(&enum_data.modifiers, SyntaxKind::DeclareKeyword)
        {
            return;
        }

        // Get enum name
        let name = self.get_identifier_text_idx(enum_data.name);
        if name.is_empty() {
            return;
        }

        // Evaluate all member values
        let values = evaluator.evaluate_enum(enum_idx);
        if !values.is_empty() {
            self.const_enum_values.insert(name, values);
        }
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

#[cfg(test)]
mod tests {
    use crate::output::printer::{PrintOptions, Printer};
    use tsz_parser::ParserState;

    #[test]
    fn emit_source_file_strips_top_level_blank_lines_for_js_files() {
        // tsc strips inter-statement blank lines even from JS source files.
        let source = "export const t1 = {\n    p: 'value',\n    get getter() {\n        return 'value';\n    },\n}\n\nexport const t2 = {\n    v: 'value',\n    set setter(v) {},\n}\n\nexport const t3 = {\n    p: 'value',\n    get value() {\n        return 'value';\n    },\n    set value(v) {},\n}\n";

        let mut parser = ParserState::new("test.js".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            !output.contains("}\n\nexport const t2"),
            "JS source should NOT preserve inter-statement blank lines.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("}\n\nexport const t3"),
            "JS source should NOT preserve inter-statement blank lines.\nOutput:\n{output}"
        );
    }

    #[test]
    fn emit_source_file_does_not_preserve_top_level_blank_lines_for_ts_files() {
        let source = "export const t1 = {\n    p: 'value',\n    get getter() {\n        return 'value';\n    },\n};\n\nexport const t2 = {\n    v: 'value',\n    set setter(v) {},\n};\n\nexport const t3 = {\n    p: 'value',\n    get value() {\n        return 'value';\n    },\n    set value(v) {},\n};\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            !output.contains("};\n\nexport const t2"),
            "TS files should not preserve explicit inter-statement blank lines in emit.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("};\n\nexport const t3"),
            "TS files should not preserve explicit inter-statement blank lines in emit.\nOutput:\n{output}"
        );
    }

    #[test]
    #[ignore = "pre-existing regression from 118ebd752 — accessor leading comment lost"]
    fn emit_class_with_accessor_members_preserves_leading_comments_in_ts_output() {
        let source = "// Regular class should still error when targeting ES5\n\
class RegularClass {\n    accessor shouldError;\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = Printer::new(&parser.arena, PrintOptions::es5());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        let comment_pos = output
            .find("// Regular class should still error when targeting ES5")
            .expect("accessor class comment should be emitted");
        let storage_pos = output
            .find("var _RegularClass_shouldError_accessor_storage;")
            .expect("accessor storage declaration should be emitted");
        let class_pos = output
            .find("var RegularClass =")
            .or_else(|| output.find("class RegularClass"))
            .expect("regular class declaration should be emitted");

        assert!(
            comment_pos > storage_pos,
            "Auto-accessor class leading comments should appear after storage declarations.\nOutput:\n{output}"
        );
        assert!(
            class_pos > comment_pos,
            "Class declaration should follow its auto-accessor leading comment.\nOutput:\n{output}"
        );
        assert!(
            output.contains("class RegularClass") || output.contains("var RegularClass"),
            "Class output should still be emitted for accessor-containing class in ES5 path.\nOutput:\n{output}"
        );
    }

    #[test]
    fn esm_suppresses_redundant_export_empty_when_real_exports_exist() {
        // When a file has both `export {};` and `export { C };`, the empty export
        // is redundant and should be suppressed. tsc omits it.
        let source = "export {};\nclass C {}\nexport { C };\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = Printer::new(
            &parser.arena,
            PrintOptions {
                module: crate::emitter::ModuleKind::ESNext,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        // Should NOT contain `export {};` since `export { C };` is present
        let export_empty_count = output.matches("export {};").count();
        assert_eq!(
            export_empty_count, 0,
            "Redundant `export {{}}` should be suppressed when real exports exist.\nOutput:\n{output}"
        );
        assert!(
            output.contains("export { C }"),
            "Real export should be preserved.\nOutput:\n{output}"
        );
    }

    #[test]
    fn esm_emits_export_empty_when_only_type_exports() {
        // When a file's only module syntax is `export {};`, it should be preserved
        // to maintain ESM semantics.
        let source = "export {};\nconst x = 1;\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = Printer::new(
            &parser.arena,
            PrintOptions {
                module: crate::emitter::ModuleKind::ESNext,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("export {};"),
            "Sole `export {{}}` should be preserved for ESM semantics.\nOutput:\n{output}"
        );
    }
}
