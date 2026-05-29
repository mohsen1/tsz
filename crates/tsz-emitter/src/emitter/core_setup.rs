use crate::context::emit::EmitContext;
use crate::context::plan::EmitPlan;
use crate::context::transform::TransformContext;
use crate::output::source_writer::{
    LineMap, SourcePosition, SourceWriter, source_position_from_offset,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;
use std::sync::Arc;
use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{Node, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::core::{Printer, PrinterOptions};

impl<'a> Printer<'a> {
    const DEFAULT_OUTPUT_CAPACITY: usize = 1024;

    pub(crate) fn estimate_output_capacity(source_len: usize) -> usize {
        // Emit output can be slightly smaller (type erasure) or significantly larger
        // (downlevel transforms/helpers). Bias toward ~1.5x while keeping a sane floor.
        source_len
            .saturating_mul(3)
            .saturating_div(2)
            .max(Self::DEFAULT_OUTPUT_CAPACITY)
    }

    /// Create a new Printer.
    pub fn new(arena: &'a NodeArena) -> Self {
        Self::with_options(arena, PrinterOptions::default())
    }

    /// Create a new Printer with options and source-length-informed preallocation.
    pub fn with_source_text_len_and_options(
        arena: &'a NodeArena,
        source_text_len: usize,
        options: PrinterOptions,
    ) -> Self {
        let capacity = Self::estimate_output_capacity(source_text_len);
        Self::with_capacity_and_options(arena, capacity, options)
    }

    /// Create a new Printer with source-length-informed preallocation.
    pub fn with_source_text_len(arena: &'a NodeArena, source_text_len: usize) -> Self {
        Self::with_source_text_len_and_options(arena, source_text_len, PrinterOptions::default())
    }

    /// Create a new Printer with options and root-node-informed preallocation.
    pub fn with_root_and_options(
        arena: &'a NodeArena,
        root: NodeIndex,
        options: PrinterOptions,
    ) -> Self {
        let source_text_len = arena
            .get(root)
            .and_then(|node| arena.get_source_file(node))
            .map_or(0, |source| source.text.len());
        Self::with_source_text_len_and_options(arena, source_text_len, options)
    }

    /// Create a new Printer with root-node-informed preallocation.
    pub fn with_root(arena: &'a NodeArena, root: NodeIndex) -> Self {
        Self::with_root_and_options(arena, root, PrinterOptions::default())
    }

    /// Create a new Printer with pre-allocated output capacity
    /// This reduces allocations when the expected output size is known (e.g., ~1.5x source size)
    pub fn with_capacity(arena: &'a NodeArena, capacity: usize) -> Self {
        Self::with_capacity_and_options(arena, capacity, PrinterOptions::default())
    }

    /// Create a new Printer with options.
    pub fn with_options(arena: &'a NodeArena, options: PrinterOptions) -> Self {
        Self::with_capacity_and_options(arena, Self::DEFAULT_OUTPUT_CAPACITY, options)
    }

    pub(in crate::emitter) fn emit_recovered_invalid_import_expression(
        &mut self,
        node: &Node,
    ) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        let end = (node.end as usize).min(text.len());
        if start >= end {
            return false;
        }

        let line = &text[start..end];
        let trimmed = line.trim_start();
        let Some(after_import) = trimmed.strip_prefix("import") else {
            return false;
        };
        if !after_import
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_whitespace)
        {
            return false;
        }

        let expr = after_import.trim_start().trim_end_matches(';').trim_end();
        if expr.is_empty() {
            return false;
        }

        self.write(expr);
        self.write_semicolon();
        true
    }

    pub(in crate::emitter) fn recovered_jsdoc_type_arguments_text(
        &self,
        type_arg_nodes: &[NodeIndex],
    ) -> Option<String> {
        if type_arg_nodes.is_empty() {
            return None;
        }

        let source = self.source_text?;
        let mut saw_recovered_jsdoc = false;
        let mut parts = Vec::with_capacity(type_arg_nodes.len());

        for type_arg in type_arg_nodes {
            let node = self.arena.get(*type_arg)?;
            let raw = source.get(node.pos as usize..node.end as usize)?.trim();
            if raw.is_empty() {
                return None;
            }

            if let Some(recovered) = self.recovered_jsdoc_type_argument_text(node, raw) {
                saw_recovered_jsdoc = true;
                parts.push(recovered);
            } else {
                parts.push(raw.to_string());
            }
        }

        saw_recovered_jsdoc.then(|| format!("<{}>", parts.join(", ")))
    }

    fn recovered_jsdoc_type_argument_text(&self, node: &Node, raw: &str) -> Option<String> {
        if raw == "?" {
            return Some("?".to_string());
        }

        let has_prefix = raw.starts_with('?');
        let has_postfix = self.is_jsdoc_postfix_nullable_type(node, raw);
        if !has_prefix && !has_postfix {
            return None;
        }

        let mut question_count = 0;
        let mut body = raw;
        if has_prefix {
            question_count += 1;
            body = body.strip_prefix('?')?.trim_start();
        }
        if has_postfix {
            question_count += 1;
            body = body.strip_suffix('?')?.trim_end();
        }

        if body.is_empty() {
            return Some("?".repeat(question_count.max(1)));
        }

        Some(format!("{}{}", "?".repeat(question_count), body))
    }

    fn is_jsdoc_postfix_nullable_type(&self, node: &Node, raw: &str) -> bool {
        if !raw.ends_with('?') || node.kind != syntax_kind_ext::UNION_TYPE {
            return false;
        }

        self.arena
            .get_composite_type(node)
            .and_then(|composite| composite.types.nodes.last())
            .and_then(|last| self.arena.get(*last))
            .is_some_and(|last| last.kind == SyntaxKind::NullKeyword as u16)
    }

    pub(in crate::emitter) fn emit_recovered_let_array_assignment(&mut self, node: &Node) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let bytes = text.as_bytes();
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        if start >= bytes.len() {
            return false;
        }

        let mut line_end = start;
        while line_end < bytes.len() && bytes[line_end] != b'\n' && bytes[line_end] != b'\r' {
            line_end += 1;
        }

        let line = &text[start..line_end];
        let trimmed = line.trim_start();
        if !trimmed.starts_with("let[") && !trimmed.starts_with("let [") {
            return false;
        }

        let Some(open_rel) = line.find('[') else {
            return false;
        };
        let Some(close_rel_from_open) = line[open_rel + 1..].find(']') else {
            return false;
        };
        let close_rel = open_rel + 1 + close_rel_from_open;
        let after_close = &line[close_rel + 1..];
        let Some(eq_rel_after_close) = after_close.find('=') else {
            return false;
        };
        if !after_close[..eq_rel_after_close].trim().is_empty() {
            return false;
        }

        let element = line[open_rel + 1..close_rel].trim().to_string();
        let value = after_close[eq_rel_after_close + 1..]
            .trim()
            .trim_end_matches(';')
            .trim()
            .to_string();
        if element.is_empty() || value.is_empty() {
            return false;
        }

        // Recovery for `let[0] = 100`: TSC treats it as a malformed lexical
        // declaration followed by recovered expression statements.
        self.write("let [];");
        self.write_line();
        self.write(&element);
        self.write_semicolon();
        self.write_line();
        self.write(&value);
        self.write_semicolon();
        true
    }

    /// Create a new Printer with pre-allocated capacity and options.
    pub fn with_capacity_and_options(
        arena: &'a NodeArena,
        capacity: usize,
        options: PrinterOptions,
    ) -> Self {
        let mut writer = SourceWriter::with_capacity(capacity);
        writer.set_new_line_kind(options.new_line);

        // Create EmitContext from options (target controls feature gates)
        let ctx = EmitContext::with_options(options);
        let emit_plan = EmitPlan::empty(&ctx.options);

        Printer {
            arena,
            writer,
            ctx,
            transforms: TransformContext::new(), // Empty by default, can be set later
            emit_plan,
            emit_missing_initializer_as_void_0: false,
            lexical_block_missing_initializer_function_depth: None,
            lexical_block_missing_initializer_is_loop_body: false,
            in_for_initializer: false,
            source_text: None,
            jsx_pragmas: crate::jsx_pragmas::JsxPragmaFacts::default(),
            source_map_text: None,
            line_map: None,
            pending_source_pos: None,
            emit_recursion_depth: 0,
            all_comments: Vec::new(),
            source_comment_ranges: Vec::new(),
            comment_emit_idx: 0,
            file_identifiers: FxHashSet::default(),
            helper_import_aliases: FxHashMap::default(),
            commonjs_tslib_import_binding: "tslib_1".to_string(),
            node_esm_create_require_names: None,
            generated_temp_names: FxHashSet::default(),
            temp_scope_stack: Vec::new(),
            pending_object_rest_params: Vec::new(),
            pending_object_rest_param_defaults: Vec::new(),
            consumed_recovered_expression_statement_span: None,
            pending_lowered_async_arrow_super_capture: None,
            function_scope_depth: 0,
            arrow_function_scope_depth: 0,
            first_for_of_emitted: false,
            in_namespace_iife: false,
            recovered_module_syntax_block_depth: 0,
            namespace_scope_end: u32::MAX,
            enum_namespace_export: None,
            namespace_export_inner: false,
            emitting_function_body_block: false,
            pending_function_body_parameters: Vec::new(),
            current_new_target_substitution: None,
            pending_new_target_capture_initializer: None,
            current_namespace_name: None,
            parent_namespace_name: None,
            current_namespace_source_path: None,
            anonymous_default_export_name: None,
            next_anonymous_default_index: 0,
            next_disposable_env_id: 1,
            next_dynamic_import_promise_id: 1,
            async_generator_inner_name_counts: FxHashMap::default(),
            reserved_disposable_env_names: FxHashMap::default(),
            reserved_top_level_using_class_result_temps: FxHashMap::default(),
            hoisted_deferred_static_class_result_temps: Vec::new(),
            block_using_env: None,
            in_top_level_using_scope: false,
            in_system_top_level_using_prelude: false,
            metadata_class_type_params: None,
            pending_block_comment_space: false,
            pending_cjs_namespace_export_fold: false,
            pending_cjs_namespace_export_names: Vec::new(),
            pending_system_namespace_export_fold: None,
            suppress_default_export_merge_iife: false,
            pending_commonjs_class_export_name: None,
            declared_namespace_names: FxHashSet::default(),
            namespace_iife_param_counter: FxHashMap::default(),
            namespace_prior_exports: FxHashMap::default(),
            namespace_prior_class_fn_enum_exports: FxHashMap::default(),
            namespace_all_exported_names: FxHashMap::default(),
            namespace_exported_names: FxHashSet::default(),
            namespace_parent_exported_names: FxHashSet::default(),
            namespace_ancestor_export_qualifiers: FxHashMap::default(),
            namespace_current_class_fn_enum_names: FxHashSet::default(),
            namespace_local_var_shadow_stack: Vec::new(),
            commonjs_exported_var_names: FxHashSet::default(),
            commonjs_exported_var_shadow_stack: Vec::new(),
            deferred_local_export_bindings: None,
            deferred_local_export_bindings_all: None,
            suppress_ns_qualification: false,
            suppress_commonjs_named_import_substitution: false,
            arrow_concise_body_trailing_comment_defer_range: None,
            pending_class_field_inits: Vec::new(),
            pending_auto_accessor_inits: Vec::new(),
            next_auto_accessor_name_index: 0,
            hoisted_assignment_value_temps: Vec::new(),
            preallocated_logical_assignment_value_temps: VecDeque::new(),
            preallocated_assignment_temps: VecDeque::new(),
            hoisted_assignment_temps: Vec::new(),
            hoisted_file_level_class_temps: Vec::new(),
            block_scoped_private_temps: Vec::new(),
            cjs_destructuring_export_temps: Vec::new(),
            system_empty_binding_temps: FxHashMap::default(),
            system_object_rest_export_temps: FxHashMap::default(),
            system_binding_pattern_temps: FxHashMap::default(),
            preplanned_legacy_decorated_class_aliases: FxHashMap::default(),
            cjs_destr_hoist_byte_offset: 0,
            cjs_destr_hoist_line: 0_u32,
            preallocated_temp_names: VecDeque::new(),
            preallocated_hoisted_temp_names: VecDeque::new(),
            reserved_nested_temp_names: FxHashSet::default(),
            file_level_class_temp_reservation_plan: Vec::new(),
            file_level_class_temp_reservations: FxHashMap::default(),
            completed_file_level_class_temp_reservations: FxHashSet::default(),
            hoisted_for_of_temps: Vec::new(),
            commonjs_named_import_substitutions: FxHashMap::default(),
            wrapped_export_module_substitutions: FxHashMap::default(),
            reserved_iterator_return_temps: FxHashMap::default(),
            iterator_for_of_depth: 0,
            destructuring_read_depth: 0,
            paren_in_access_position: false,
            in_system_execute_body: false,
            system_reexported_names: FxHashMap::default(),
            system_reexported_name_lists: FxHashMap::default(),
            system_folded_export_names: FxHashSet::default(),
            paren_in_new_callee: false,
            paren_is_direct_call_callee: false,
            object_literal_accessor_depth: 0,
            class_member_emit_depth: 0,
            es5_super_home_function_depth: None,
            es5_super_home_is_static: false,
            is_current_root_js_source: false,
            const_enum_values: FxHashMap::default(),
            const_enum_import_aliases: FxHashMap::default(),
            prior_enum_member_values: FxHashMap::default(),
            prior_enum_string_members: FxHashMap::default(),
            prior_enum_string_values: FxHashMap::default(),
            private_field_weakmaps: FxHashMap::default(),
            generated_private_names: None,
            private_member_info: FxHashMap::default(),
            pending_weakmap_inits: Vec::new(),
            pending_static_private_inits: Vec::new(),
            pending_private_class_alias: None,
            pending_private_field_constructor_inits: Vec::new(),
            pending_instances_weakset_add: None,
            pending_private_method_defs: Vec::new(),
            pending_private_accessor_defs: Vec::new(),
            private_members_to_skip: FxHashSet::default(),
            private_static_class_alias: None,
            private_static_class_alias_shadow_depth: 0,
            defer_class_static_blocks: false,
            deferred_class_static_blocks: Vec::new(),
            jsx_dev_file_name: None,
            jsx_legacy_cjs_runtime_var: None,
            source_is_js_file: false,
            computed_prop_temp_map: FxHashMap::default(),
            legacy_decorator_computed_name_temp_map: FxHashMap::default(),
            scoped_static_this_alias: None,
            scoped_static_super_direct_access: false,
            scoped_static_super_base_alias: None,
            scoped_static_super_index_alias: None,
            scoped_static_super_index_value_access: false,
            scoped_static_super_assignment_target: false,
            scoped_class_expression_self_alias: None,
            pending_tc39_class_expression_name: None,
            es5_class_expression_extends_this_captured: false,
            tagged_template_var_map: FxHashMap::default(),
        }
    }

    /// Set whether the current root source file has a JavaScript-like extension.
    pub(crate) const fn set_current_root_js_source(&mut self, is_js_source: bool) {
        self.is_current_root_js_source = is_js_source;
    }

    /// Whether an accessor node is currently being emitted from object-literal syntax.
    pub(crate) const fn is_emitting_object_literal_accessor(&self) -> bool {
        self.object_literal_accessor_depth > 0
    }

    pub(crate) const fn set_es5_class_expression_extends_this_captured(&mut self, captured: bool) {
        self.es5_class_expression_extends_this_captured = captured;
    }

    /// Emit an object-literal property node, marking accessor members to enable
    /// JS pass-through formatting rules (e.g., empty accessor-body spacing).
    pub(crate) fn emit_object_property(&mut self, property_idx: NodeIndex) {
        let Some(node) = self.arena.get(property_idx) else {
            return;
        };

        if self.ctx.target_es5
            && node.kind == syntax_kind_ext::METHOD_DECLARATION
            && let Some(method) = self.arena.get_method_decl(node)
            && self
                .arena
                .has_modifier(&method.modifiers, tsz_scanner::SyntaxKind::AsyncKeyword)
        {
            let has_generator_asterisk = method.asterisk_token
                || crate::transforms::emit_utils::source_header_has_async_generator_asterisk(
                    self.source_text,
                    node.pos,
                    self.arena
                        .get(method.body)
                        .map_or(node.end, |body| body.pos),
                );
            if has_generator_asterisk {
                let property_name = crate::transforms::emit_utils::identifier_text_or_empty(
                    self.arena,
                    method.name,
                );
                self.emit_async_generator_es5_object_method_property(
                    &property_name,
                    &method.parameters.nodes,
                    method.body,
                );
                return;
            }
        }

        if node.kind == syntax_kind_ext::METHOD_DECLARATION
            && self
                .arena
                .get_method_decl(node)
                .is_some_and(|method| method.body.is_none())
        {
            self.emit_recovered_object_method_without_body(node);
            return;
        }

        let is_accessor = node.kind == syntax_kind_ext::GET_ACCESSOR
            || node.kind == syntax_kind_ext::SET_ACCESSOR;

        if is_accessor {
            self.object_literal_accessor_depth =
                self.object_literal_accessor_depth.saturating_add(1);
        }

        self.emit(property_idx);

        if is_accessor {
            self.object_literal_accessor_depth =
                self.object_literal_accessor_depth.saturating_sub(1);
        }
    }

    /// Create a new Printer with transform directives.
    /// This is the Phase 2 constructor that accepts pre-computed transforms.
    pub fn with_transforms(arena: &'a NodeArena, transforms: TransformContext) -> Self {
        let mut printer = Self::new(arena);
        printer.set_emit_plan(EmitPlan::from_transforms(&printer.ctx.options, transforms));
        printer
    }

    /// Create a new Printer with transforms and options.
    pub fn with_transforms_and_options(
        arena: &'a NodeArena,
        transforms: TransformContext,
        options: PrinterOptions,
    ) -> Self {
        let mut printer = Self::with_options(arena, options);
        printer.set_emit_plan(EmitPlan::from_transforms(&printer.ctx.options, transforms));
        printer
    }

    /// Create a new Printer with an explicit emit plan and options.
    pub fn with_emit_plan_and_options(
        arena: &'a NodeArena,
        plan: EmitPlan,
        options: PrinterOptions,
    ) -> Self {
        let mut printer = Self::with_options(arena, options);
        printer.set_emit_plan(plan);
        printer
    }

    fn set_emit_plan(&mut self, plan: EmitPlan) {
        self.transforms = plan.transforms.clone();
        self.emit_plan = plan;
    }

    /// Seed a nested printer with outer function-scope names that nested
    /// block-scoped declarations must not capture when lowered to ES5 `var`.
    pub(crate) fn seed_function_scope_shadowed_names(&mut self, names: &[String]) {
        if names.is_empty() {
            return;
        }

        self.ctx.block_scope_state.enter_function_scope();
        for name in names {
            self.ctx
                .block_scope_state
                .register_function_scope_shadowed_name(name);
        }
    }

    pub(crate) fn seed_block_scope_reserved_names(&mut self, names: &[String]) {
        self.ctx
            .block_scope_state
            .reserve_names(names.iter().cloned());
    }

    pub(crate) fn block_scope_reserved_names(&self) -> Vec<String> {
        self.ctx.block_scope_state.visible_reserved_names()
    }

    #[must_use]
    pub const fn emit_plan(&self) -> &EmitPlan {
        &self.emit_plan
    }

    /// Set whether to target ES5 behavior.
    ///
    /// This updates both the legacy `target_es5` bool and all derived
    /// per-version lowering gates in the shared context.
    pub const fn set_target_es5(&mut self, es5: bool) {
        self.ctx.set_target_es5(es5);
    }

    /// Set the full script target.
    ///
    /// This keeps all derived feature gates synchronized, including `target_es5`.
    pub const fn set_target(&mut self, target: ScriptTarget) {
        self.ctx.set_target(target);
    }

    /// Set the module kind (`CommonJS`, ESM, etc.).
    pub const fn set_module_kind(&mut self, kind: ModuleKind) {
        self.ctx.options.module = kind;
    }

    /// Set auto-detect module mode. When enabled, the emitter will detect if
    /// the source file contains import/export statements and apply `CommonJS`
    /// transforms automatically.
    pub const fn set_auto_detect_module(&mut self, enabled: bool) {
        self.ctx.auto_detect_module = enabled;
    }

    /// Mark this printer as emitting a `--module none --outFile` bundle.
    pub const fn set_module_none_out_file(&mut self, enabled: bool) {
        self.ctx.module_none_out_file = enabled;
    }

    /// Set the source text (for detecting single-line constructs).
    pub fn set_source_text(&mut self, text: &'a str) {
        self.source_text = Some(text);
        self.jsx_pragmas = crate::jsx_pragmas::JsxPragmaFacts::from_source(text);
        self.source_comment_ranges = if self.ctx.options.remove_comments {
            Vec::new()
        } else {
            tsz_common::comments::get_comment_ranges(text)
        };
        self.line_map = Some(LineMap::new(text));
        let estimated = Self::estimate_output_capacity(text.len());
        self.writer.ensure_output_capacity(estimated);
    }

    /// Enable declaration emit mode for `.d.ts` output.
    ///
    /// Declaration mode changes emission behavior in multiple nodes, such as:
    /// - Skipping JS-only constructs
    /// - Emitting `declare` signatures instead of values
    /// - Keeping type-only information
    pub const fn set_declaration_emit(&mut self, enabled: bool) {
        self.ctx.flags.in_declaration_emit = enabled;
    }

    /// Set source text for source map generation without enabling comment emission.
    pub const fn set_source_map_text(&mut self, text: &'a str) {
        self.source_map_text = Some(text);
    }

    /// Enable source map generation and register the current source file.
    pub fn enable_source_map(&mut self, output_name: &str, source_name: &str) {
        self.writer.enable_source_map(output_name.to_string());
        let content = self
            .source_text_for_map()
            .map(std::string::ToString::to_string);
        self.writer.add_source(source_name.to_string(), content);
    }

    /// Generate source map JSON (if enabled).
    pub fn generate_source_map_json(&mut self) -> Option<String> {
        self.writer.generate_source_map_json()
    }

    pub(crate) fn source_text_for_map(&self) -> Option<&'a str> {
        self.source_map_text.or(self.source_text)
    }

    /// Returns `source_text.len()` as `u32`, or `fallback` when no source text is attached.
    pub(crate) fn source_text_end_or(&self, fallback: u32) -> u32 {
        self.source_text.map_or(fallback, |t| t.len() as u32)
    }

    /// Compute a `SourcePosition` from a byte offset, using the precomputed
    /// line map for O(log n) lookup when available, falling back to the O(n)
    /// linear scan otherwise.
    pub(crate) fn fast_source_position(&self, pos: u32) -> Option<SourcePosition> {
        if let Some(ref lm) = self.line_map {
            Some(lm.source_position(pos))
        } else {
            self.source_text_for_map()
                .map(|text| source_position_from_offset(text, pos))
        }
    }

    pub(in crate::emitter) fn queue_source_mapping(&mut self, node: &Node) {
        if !self.writer.has_source_map() {
            self.pending_source_pos = None;
            return;
        }

        self.pending_source_pos = self.fast_source_position(node.pos);
    }

    /// Check if a node spans a single line in the source.
    /// Finds the first `{` and last `}` within the node's source span and checks
    /// if there's a newline between them. Uses depth counting to handle nested braces correctly.
    pub(crate) fn is_single_line(&self, node: &Node) -> bool {
        if let Some(text) = self.source_text {
            let actual_start = self.skip_trivia_forward(node.pos, node.end) as usize;
            // Use actual token end, not node.end which includes trailing trivia.
            // For example, `{ return x; }\n` has trailing newline in node.end,
            // but we want to check only `{ return x; }`.
            let token_end = self.find_token_end_before_trivia(node.pos, node.end);
            let end = std::cmp::min(token_end as usize, text.len());
            if actual_start < end {
                let slice = &text[actual_start..end];
                // Find the first `{` and its matching `}` using depth counting
                // to handle nested braces (e.g., `{ return new Line({ x: 0 }, p); }`)
                if let Some(open) = slice.find('{') {
                    let mut depth = 1;
                    let mut close = None;
                    for (i, ch) in slice[open + 1..].char_indices() {
                        match ch {
                            '{' => depth += 1,
                            '}' => {
                                depth -= 1;
                                if depth == 0 {
                                    close = Some(open + 1 + i);
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    if let Some(close) = close {
                        let inner = &slice[open..close + 1];
                        return !inner.contains('\n');
                    }
                }
                return !slice.contains('\n');
            }
        }
        false
    }

    /// Check if two nodes are on the same line in the source.
    pub(crate) fn are_on_same_line_in_source(
        &self,
        node1: tsz_parser::parser::NodeIndex,
        node2: tsz_parser::parser::NodeIndex,
    ) -> bool {
        if let Some(text) = self.source_text
            && let (Some(n1), Some(n2)) = (self.arena.get(node1), self.arena.get(node2))
        {
            let start = std::cmp::min(n1.end as usize, text.len());
            let end = std::cmp::min(n2.pos as usize, text.len());
            if start < end {
                // Check if there's a newline between the two nodes
                return !text[start..end].contains('\n');
            }
        }
        false
    }

    /// Get the output.
    pub fn get_output(&self) -> &str {
        self.writer.get_output()
    }

    /// Take the output.
    pub fn take_output(self) -> String {
        self.writer.take_output()
    }

    /// Returns AMD factory-parameter counters accumulated during emission, for
    /// threading into the next file's `PrinterOptions::bundle_module_counters`.
    pub const fn bundle_module_counters(&self) -> &FxHashMap<String, u32> {
        &self.ctx.module_state.module_temp_counters
    }
    // =========================================================================
    // Main Emit Method
    // =========================================================================

    /// Emit a node by index.
    pub fn emit(&mut self, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }

        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if let Some(source) = self.arena.get_source_file(node) {
            let file_name = source.file_name.to_ascii_lowercase();
            let is_js_source = file_name.ends_with(".js")
                || file_name.ends_with(".jsx")
                || file_name.ends_with(".mjs")
                || file_name.ends_with(".cjs");
            self.set_current_root_js_source(is_js_source);
            // The generated private-helper name set is file-scoped: reset it so a
            // reused printer re-seeds from this file's enclosing source bindings.
            self.generated_private_names = None;
        }

        if let Some(source) = self.arena.get_source_file(node)
            && self.transforms.is_empty()
        {
            let format = match self.ctx.options.module {
                ModuleKind::AMD => Some(crate::context::transform::ModuleFormat::AMD),
                ModuleKind::UMD => Some(crate::context::transform::ModuleFormat::UMD),
                ModuleKind::System => Some(crate::context::transform::ModuleFormat::System),
                _ => None,
            };
            if let Some(format) = format
                && self.file_is_module(&source.statements)
            {
                let dependencies = self.collect_module_dependencies(&source.statements.nodes);
                self.emit_module_wrapper(format, &dependencies, node, source, idx);
                return;
            }
        }

        self.emit_node(node, idx);
    }

    /// Emit a node in an expression context.
    /// If the node is missing or an error/unknown node, emits nothing (matching tsc behavior
    /// for parse error recovery — e.g. `const x = ;` rather than `const x = void 0;`).
    pub fn emit_expression(&mut self, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }

        let Some(node) = self.arena.get(idx) else {
            return;
        };

        // Check if this is an error/unknown node
        use tsz_scanner::SyntaxKind;
        if node.kind == SyntaxKind::Unknown as u16 {
            return;
        }

        // Otherwise, emit normally
        self.emit_node(node, idx);
    }

    #[allow(dead_code)]
    pub(crate) fn emit_expression_with_scoped_static_initializer(
        &mut self,
        idx: NodeIndex,
        this_alias: Option<&str>,
        super_base_alias: Option<&str>,
    ) {
        self.emit_expression_with_scoped_static_initializer_mode(
            idx,
            this_alias,
            super_base_alias,
            false,
        );
    }

    pub(crate) fn emit_expression_with_scoped_static_initializer_mode(
        &mut self,
        idx: NodeIndex,
        this_alias: Option<&str>,
        super_base_alias: Option<&str>,
        super_direct_access: bool,
    ) {
        let prev_this_alias = self.scoped_static_this_alias.clone();
        let prev_super_direct_access = self.scoped_static_super_direct_access;
        let prev_super_alias = self.scoped_static_super_base_alias.clone();
        let prev_super_index_alias = self.scoped_static_super_index_alias.clone();
        let prev_super_index_value = self.scoped_static_super_index_value_access;
        let prev_super_assignment_target = self.scoped_static_super_assignment_target;

        self.scoped_static_this_alias = this_alias.map(Arc::from);
        self.scoped_static_super_direct_access = super_direct_access;
        self.scoped_static_super_base_alias = super_base_alias.map(Arc::from);
        self.scoped_static_super_index_alias = None;
        self.scoped_static_super_index_value_access = false;
        self.scoped_static_super_assignment_target = false;

        self.emit_expression(idx);
        self.scoped_static_this_alias = prev_this_alias;
        self.scoped_static_super_direct_access = prev_super_direct_access;
        self.scoped_static_super_base_alias = prev_super_alias;
        self.scoped_static_super_index_alias = prev_super_index_alias;
        self.scoped_static_super_index_value_access = prev_super_index_value;
        self.scoped_static_super_assignment_target = prev_super_assignment_target;
    }

    /// Enter the CommonJS-export-body mask while emitting `f`: clear
    /// `options.module` to `None` (so inner statements do not re-apply
    /// module-level transforms) and save the outer module on
    /// `cjs_export_body_outer_module` (so dynamic-import lowering, helper
    /// detection, and sub-emitters still see the outer kind through the
    /// `outer_module_kind()` / `is_effectively_commonjs()` predicates).
    pub(in crate::emitter) fn with_cjs_export_body_mask<R>(
        &mut self,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let prev_module = self.ctx.options.module;
        let prev_outer = self.ctx.cjs_export_body_outer_module;
        self.ctx.cjs_export_body_outer_module = Some(self.ctx.outer_module_kind());
        self.ctx.options.module = ModuleKind::None;
        let result = f(self);
        self.ctx.options.module = prev_module;
        self.ctx.cjs_export_body_outer_module = prev_outer;
        result
    }

    pub(in crate::emitter) fn with_scoped_static_initializer_context_cleared<R>(
        &mut self,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let prev_this_alias = self.scoped_static_this_alias.take();
        let prev_super_direct_access = self.scoped_static_super_direct_access;
        let prev_super_alias = self.scoped_static_super_base_alias.take();
        let prev_super_index_alias = self.scoped_static_super_index_alias.take();
        let prev_super_index_value = self.scoped_static_super_index_value_access;
        let prev_super_assignment_target = self.scoped_static_super_assignment_target;
        self.scoped_static_super_direct_access = false;
        self.scoped_static_super_index_value_access = false;
        self.scoped_static_super_assignment_target = false;
        let result = f(self);
        self.scoped_static_this_alias = prev_this_alias;
        self.scoped_static_super_direct_access = prev_super_direct_access;
        self.scoped_static_super_base_alias = prev_super_alias;
        self.scoped_static_super_index_alias = prev_super_index_alias;
        self.scoped_static_super_index_value_access = prev_super_index_value;
        self.scoped_static_super_assignment_target = prev_super_assignment_target;
        result
    }
}
