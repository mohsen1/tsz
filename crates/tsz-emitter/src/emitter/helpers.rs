use super::{JsxEmit, ModuleKind, Printer};
use crate::output::source_writer::SourcePosition;
use crate::safe_slice;
use tsz_parser::parser::node::{Node, NodeAccess};
use tsz_parser::parser::{NodeIndex, node_flags, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(super) const fn take_pending_source_pos(&mut self) -> Option<SourcePosition> {
        self.pending_source_pos.take()
    }

    // =========================================================================
    // Output Helpers (delegate to SourceWriter)
    // pub(super) for access from submodules (expressions, statements, declarations)
    // =========================================================================

    /// Write text to output.
    pub(super) fn write(&mut self, text: &str) {
        // If an inline block comment was just emitted, insert a separating space
        // before non-whitespace text to match tsc output (e.g. `/*comment*/ yield`).
        if self.pending_block_comment_space {
            self.pending_block_comment_space = false;
            if !text.is_empty()
                && !text.starts_with(' ')
                && !text.starts_with('\n')
                && !text.starts_with('\r')
            {
                self.writer.write_space();
            }
        }
        if let Some(source_pos) = self.take_pending_source_pos() {
            self.writer.write_node(text, source_pos);
        } else {
            self.writer.write(text);
        }
    }

    /// Write a mapped token and also emit an end-of-token mapping.
    /// tsc emits these for single-character tokens like `;`, `{`, `}`.
    pub(super) fn write_with_end_marker(&mut self, text: &str) {
        // Handle pending block comment space (e.g., `/*comment*/ ;`).
        if self.pending_block_comment_space {
            self.pending_block_comment_space = false;
            if !text.is_empty()
                && !text.starts_with(' ')
                && !text.starts_with('\n')
                && !text.starts_with('\r')
            {
                self.writer.write_space();
            }
        }
        if let Some(source_pos) = self.take_pending_source_pos() {
            self.writer.write_node_with_end(text, source_pos);
        } else {
            self.writer.write(text);
        }
    }

    /// Write identifier text to output with name mapping when available.
    pub(super) fn write_identifier(&mut self, text: &str) {
        // Handle pending block comment space (e.g., `/** comment */ identifier`).
        if self.pending_block_comment_space {
            self.pending_block_comment_space = false;
            if !text.is_empty()
                && !text.starts_with(' ')
                && !text.starts_with('\n')
                && !text.starts_with('\r')
            {
                self.writer.write_space();
            }
        }
        if let Some(source_pos) = self.take_pending_source_pos() {
            self.writer.write_node_with_name(text, source_pos, text);
        } else {
            self.writer.write(text);
        }
    }

    /// Emit a node as a declaration name (suppress namespace qualification).
    pub(super) fn emit_decl_name(&mut self, idx: NodeIndex) {
        let prev = self.suppress_ns_qualification;
        self.suppress_ns_qualification = true;
        self.emit(idx);
        self.suppress_ns_qualification = prev;
    }

    /// Write a single character.
    pub(super) fn write_char(&mut self, ch: char) {
        if let Some(source_pos) = self.take_pending_source_pos() {
            let mut buf = [0u8; 4];
            let text = ch.encode_utf8(&mut buf);
            self.writer.write_node(text, source_pos);
        } else {
            self.writer.write_char(ch);
        }
    }

    /// Write a runtime helper call name (e.g. `__awaiter`).
    /// When `importHelpers` is active and the module is CJS, this prefixes with `tslib_1.`.
    /// For ESM with importHelpers, the helpers are imported directly so no prefix is needed.
    pub(super) fn write_helper(&mut self, name: &str) {
        if self.ctx.options.import_helpers && self.ctx.is_effectively_commonjs() {
            self.write("tslib_1.");
        }
        self.write(name);
    }

    /// Emit an expression, unwrapping `ExpressionWithTypeArguments` without parens.
    ///
    /// When a `PropertyAccessExpression` or `CallExpression` has an
    /// `ExpressionWithTypeArguments` as its base expression (e.g.,
    /// `List<number>.makeChild()`), the type arguments should be stripped
    /// and the inner expression emitted WITHOUT wrapping parens, since the
    /// parent's `.` or `()` already provides grouping.
    ///
    /// In other positions (assignment target, `instanceof`, decorator),
    /// the standalone `EXPRESSION_WITH_TYPE_ARGUMENTS` handler in `emit_node`
    /// adds parens, matching tsc behavior.
    pub(super) fn emit_unwrapping_type_args(&mut self, idx: NodeIndex) {
        if let Some(node) = self.arena.get(idx)
            && node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
            && let Some(data) = self.arena.get_expr_type_args(node)
        {
            // If the inner expression is an optional chain, we need parens to
            // preserve the chain boundary. Without parens, a subsequent `.prop`
            // or `()` would become part of the optional chain, changing semantics.
            // Example: `a?.b<c>.d` must emit `(a?.b).d`, not `a?.b.d`.
            let needs_parens = if let Some(inner) = self.arena.get(data.expression) {
                if let Some(access) = self.arena.get_access_expr(inner) {
                    access.question_dot_token
                } else {
                    (inner.flags as u32) & node_flags::OPTIONAL_CHAIN != 0
                }
            } else {
                false
            };
            if needs_parens {
                self.write("(");
            }
            self.emit(data.expression);
            if needs_parens {
                self.write(")");
            }
        } else {
            self.emit(idx);
        }
    }

    /// Write a newline.
    pub(super) fn write_line(&mut self) {
        self.pending_block_comment_space = false;
        self.writer.write_line();
    }

    /// Write a space.
    pub(super) fn write_space(&mut self) {
        self.pending_block_comment_space = false;
        self.writer.write_space();
    }

    /// Write an unsigned integer.
    pub(super) fn write_usize(&mut self, value: usize) {
        if let Some(source_pos) = self.take_pending_source_pos() {
            self.writer.write_node_usize(value, source_pos);
        } else {
            self.writer.write_usize(value);
        }
    }

    /// Write a semicolon (respecting options).
    pub(super) fn write_semicolon(&mut self) {
        if !self.ctx.options.omit_trailing_semicolon {
            self.write_with_end_marker(";");
        }
    }

    /// Check whether the output buffer's last non-whitespace character is a semicolon.
    pub(super) fn output_ends_with_semicolon(&self) -> bool {
        self.writer.last_non_whitespace_byte() == Some(b';')
    }

    /// Increase indentation.
    pub(super) const fn increase_indent(&mut self) {
        self.writer.increase_indent();
    }

    /// Decrease indentation.
    pub(super) const fn decrease_indent(&mut self) {
        self.writer.decrease_indent();
    }

    // =========================================================================
    // Source Map Helpers
    // =========================================================================

    /// Set `pending_source_pos` to an exact byte offset in the source text.
    pub(super) fn map_source_offset(&mut self, offset: u32) {
        if self.source_text_for_map().is_some() {
            self.pending_source_pos = self.fast_source_position(offset);
        }
    }

    /// Set `pending_source_pos` to the opening `{` position of a block/node.
    /// Scans forward from node.pos to find the `{` in the source text.
    pub(super) fn map_opening_brace(&mut self, node: &Node) {
        if let Some(text) = self.source_text_for_map() {
            let bytes = text.as_bytes();
            let start = node.pos as usize;
            let end = (node.end as usize).min(bytes.len());
            if let Some(offset) = bytes[start..end].iter().position(|&b| b == b'{') {
                self.pending_source_pos = self.fast_source_position((start + offset) as u32);
            }
        }
    }

    /// Set `pending_source_pos` to the first occurrence of `token` byte found
    /// by scanning forward from `from_pos` within the source text.
    /// Like `map_token_after`, but scans backward from `from_pos` (exclusive)
    /// down to `limit` (inclusive) looking for `token`. Used when the parser
    /// includes a separator (like `,`) in the preceding node's range.
    pub(super) fn map_token_before(&mut self, from_pos: u32, limit: u32, token: u8) {
        if let Some(text) = self.source_text_for_map() {
            let bytes = text.as_bytes();
            let start = (limit as usize).min(bytes.len());
            let end = (from_pos as usize).min(bytes.len());
            for i in (start..end).rev() {
                if bytes[i] == token {
                    self.pending_source_pos = self.fast_source_position(i as u32);
                    return;
                }
            }
        }
    }

    pub(super) fn map_token_after(&mut self, from_pos: u32, limit: u32, token: u8) {
        if let Some(text) = self.source_text_for_map() {
            let bytes = text.as_bytes();
            let start = from_pos as usize;
            let end = (limit as usize).min(bytes.len());
            if let Some(offset) = bytes
                .get(start..end)
                .and_then(|s| s.iter().position(|&b| b == token))
            {
                self.pending_source_pos = self.fast_source_position((start + offset) as u32);
            }
        }
    }

    /// Set `pending_source_pos` to the first non-whitespace character after
    /// `from_pos`, scanning up to `limit`. Used for mapping operator tokens
    /// between subexpressions.
    pub(super) fn map_token_after_skipping_whitespace(&mut self, from_pos: u32, limit: u32) {
        if let Some(text) = self.source_text_for_map() {
            let bytes = text.as_bytes();
            let start = from_pos as usize;
            let end = (limit as usize).min(bytes.len());
            if let Some(offset) = bytes
                .get(start..end)
                .and_then(|s| s.iter().position(|&b| !b.is_ascii_whitespace()))
            {
                self.pending_source_pos = self.fast_source_position((start + offset) as u32);
            }
        }
    }

    /// Set `pending_source_pos` to the closing `}` position of a block/node.
    /// Scans backwards from node.end to find the `}` in the source text.
    pub(super) fn map_closing_brace(&mut self, node: &Node) {
        if let Some(text) = self.source_text_for_map() {
            let bytes = text.as_bytes();
            let start = self.skip_trivia_forward(node.pos, node.end) as usize;
            let end = (node.end as usize).min(bytes.len());
            // Find the matching `}` by tracking brace depth from the opening `{`
            let mut depth: i32 = 0;
            let mut closing_pos = None;
            let mut i = start;
            while i < end {
                match bytes[i] {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            closing_pos = Some(i);
                            break;
                        }
                    }
                    b'"' | b'\'' | b'`' => {
                        // Skip string literals to avoid counting braces inside strings
                        let quote = bytes[i];
                        i += 1;
                        while i < end && bytes[i] != quote {
                            if bytes[i] == b'\\' {
                                i += 1; // skip escaped char
                            }
                            i += 1;
                        }
                    }
                    b'/' if i + 1 < end && bytes[i + 1] == b'/' => {
                        // Skip line comments
                        while i < end && bytes[i] != b'\n' {
                            i += 1;
                        }
                    }
                    b'/' if i + 1 < end && bytes[i + 1] == b'*' => {
                        // Skip block comments
                        i += 2;
                        while i + 1 < end && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                            i += 1;
                        }
                        if i + 1 < end {
                            i += 1; // skip past */
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            if let Some(pos) = closing_pos {
                self.pending_source_pos = self.fast_source_position(pos as u32);
            }
        }
    }

    /// Set `pending_source_pos` to the closing `)` of a node (e.g., call expression).
    /// Scans backward from node.end to find the last `)`.
    pub(super) fn map_closing_paren(&mut self, node: &Node) {
        if let Some(text) = self.source_text_for_map() {
            let bytes = text.as_bytes();
            let end = (node.end as usize).min(bytes.len());
            let start = node.pos as usize;
            // Scan backward to find the last `)`
            let mut i = end;
            while i > start {
                i -= 1;
                if bytes[i] == b')' {
                    self.pending_source_pos = self.fast_source_position(i as u32);
                    return;
                }
            }
        }
    }

    /// Set `pending_source_pos` to the `)` found by scanning backward from
    /// `search_end` to `search_start`. Use this for control-flow closing parens
    /// where the parser may include `)` in the expression node's range.
    pub(super) fn map_closing_paren_backward(&mut self, search_start: u32, search_end: u32) {
        if let Some(text) = self.source_text_for_map() {
            let bytes = text.as_bytes();
            let end = (search_end as usize).min(bytes.len());
            let start = search_start as usize;
            let mut i = end;
            while i > start {
                i -= 1;
                if bytes[i] == b')' {
                    self.pending_source_pos = self.fast_source_position(i as u32);
                    return;
                }
            }
        }
    }

    /// Set `pending_source_pos` to the trailing `;` of a statement node.
    /// Uses `find_token_end_before_trivia` to locate the last significant token,
    /// then checks if that token was `;`.
    pub(super) fn map_trailing_semicolon(&mut self, node: &Node) {
        if let Some(text) = self.source_text_for_map() {
            let bytes = text.as_bytes();
            let start = node.pos as usize;
            let end = (node.end as usize).min(bytes.len());
            let mut depth: i32 = 0;
            let mut last_semi = None;
            let mut i = start;
            while i < end {
                match bytes[i] {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth < 0 {
                            break;
                        }
                    }
                    b';' if depth == 0 => last_semi = Some(i),
                    b'\'' | b'"' | b'`' => {
                        let quote = bytes[i];
                        i += 1;
                        while i < end && bytes[i] != quote {
                            if bytes[i] == b'\\' {
                                i += 1;
                            }
                            i += 1;
                        }
                    }
                    b'/' if i + 1 < end && bytes[i + 1] == b'/' => {
                        i += 2;
                        while i < end && bytes[i] != b'\n' {
                            i += 1;
                        }
                    }
                    b'/' if i + 1 < end && bytes[i + 1] == b'*' => {
                        i += 2;
                        while i + 1 < end && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                            i += 1;
                        }
                        if i + 1 < end {
                            i += 1;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            if let Some(pos) = last_semi {
                self.pending_source_pos = self.fast_source_position(pos as u32);
            }
        }
    }

    // =========================================================================
    // Identifier Helpers
    // =========================================================================

    pub(super) fn has_identifier_text(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        self.arena.get_identifier(node).is_some()
    }

    pub(super) fn write_identifier_text(&mut self, idx: NodeIndex) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };
        if let Some(ident) = self.arena.get_identifier(node) {
            self.write_identifier(&ident.escaped_text);
        }
    }

    // =========================================================================
    // Unique Name Generation (mirrors TypeScript's makeUniqueName)
    // =========================================================================

    /// Save the current temp naming state and start a fresh scope.
    /// Used when entering a function to reset temp names (_a, _b, etc.)
    /// since each function scope has its own temp naming.
    pub(super) fn push_temp_scope(&mut self) {
        let saved_counter = self.ctx.destructuring_state.temp_var_counter;
        let saved_names = std::mem::take(&mut self.generated_temp_names);
        let saved_for_of = self.first_for_of_emitted;
        let saved_preallocated = std::mem::take(&mut self.preallocated_temp_names);
        let saved_preallocated_assignment_temps =
            std::mem::take(&mut self.preallocated_assignment_temps);
        let saved_preallocated_logical_value_temps =
            std::mem::take(&mut self.preallocated_logical_assignment_value_temps);
        let saved_hoisted = std::mem::take(&mut self.hoisted_assignment_temps);
        let saved_value_temps = std::mem::take(&mut self.hoisted_assignment_value_temps);
        self.temp_scope_stack.push(super::TempScopeState {
            temp_var_counter: saved_counter,
            generated_temp_names: saved_names,
            first_for_of_emitted: saved_for_of,
            preallocated_temp_names: saved_preallocated,
            preallocated_assignment_temps: saved_preallocated_assignment_temps,
            preallocated_logical_assignment_value_temps: saved_preallocated_logical_value_temps,
            hoisted_assignment_value_temps: saved_value_temps,
            hoisted_assignment_temps: saved_hoisted,
        });
        self.ctx.destructuring_state.temp_var_counter = 0;
        self.first_for_of_emitted = false;
    }

    /// Restore the previous temp naming state when leaving a function scope.
    pub(super) fn pop_temp_scope(&mut self) {
        if let Some(state) = self.temp_scope_stack.pop() {
            self.ctx.destructuring_state.temp_var_counter = state.temp_var_counter;
            self.generated_temp_names = state.generated_temp_names;
            self.first_for_of_emitted = state.first_for_of_emitted;
            self.preallocated_temp_names = state.preallocated_temp_names;
            self.preallocated_assignment_temps = state.preallocated_assignment_temps;
            self.preallocated_logical_assignment_value_temps =
                state.preallocated_logical_assignment_value_temps;
            self.hoisted_assignment_value_temps = state.hoisted_assignment_value_temps;
            self.hoisted_assignment_temps = state.hoisted_assignment_temps;
        }
    }

    /// Generate a unique temp name that doesn't collide with any identifier in the source file
    /// or any previously generated temp name. Uses a single global counter like TypeScript.
    ///
    /// Generates names: _a, _b, _c, ..., _z, _0, _1, ...
    /// Skips counts 8 (_i) and 13 (_n) which TypeScript reserves for dedicated `TempFlags`.
    /// Also skips names that appear in `file_identifiers` or `generated_temp_names`.
    fn generate_fresh_temp_name(&mut self) -> String {
        loop {
            let counter = self.ctx.destructuring_state.temp_var_counter;
            self.ctx.destructuring_state.temp_var_counter += 1;

            // TypeScript skips counts 8 (_i) and 13 (_n) - these are reserved for
            // dedicated TempFlags._i and TempFlags._n used by specific transforms
            if counter < 26 && (counter == 8 || counter == 13) {
                continue;
            }

            let name = if counter < 26 {
                format!("_{}", (b'a' + counter as u8) as char)
            } else {
                format!("_{}", counter - 26)
            };

            if !self.file_identifiers.contains(&name) && !self.generated_temp_names.contains(&name)
            {
                self.generated_temp_names.insert(name.clone());
                return name;
            }
            // Name collides, try next
        }
    }

    pub(super) fn make_unique_name(&mut self) -> String {
        if let Some(name) = self.preallocated_temp_names.pop_front() {
            return name;
        }
        self.generate_fresh_temp_name()
    }

    pub(super) fn make_unique_name_fresh(&mut self) -> String {
        self.generate_fresh_temp_name()
    }

    pub(super) fn make_unique_name_from_base(&mut self, base: &str) -> String {
        for suffix in 1..=1000 {
            let candidate = format!("{base}_{suffix}");
            if !self.file_identifiers.contains(&candidate)
                && !self.generated_temp_names.contains(&candidate)
            {
                self.generated_temp_names.insert(candidate.clone());
                self.ctx.block_scope_state.reserve_name(candidate.clone());
                return candidate;
            }
        }

        let name = self.make_unique_name_fresh();
        self.ctx.block_scope_state.reserve_name(name.clone());
        name
    }

    pub(super) fn preallocate_temp_names(&mut self, count: usize) {
        for _ in 0..count {
            let name = self.generate_fresh_temp_name();
            self.preallocated_temp_names.push_back(name);
        }
    }

    pub(super) fn preallocate_assignment_temps(&mut self, count: usize) {
        for _ in 0..count {
            let name = self.generate_fresh_temp_name();
            self.preallocated_assignment_temps.push_back(name);
        }
    }

    pub(super) fn make_unique_name_hoisted_assignment(&mut self) -> String {
        let name = if let Some(name) = self.preallocated_assignment_temps.pop_front() {
            name
        } else {
            self.make_unique_name()
        };
        self.hoisted_assignment_temps.push(name.clone());
        name
    }

    pub(super) fn preallocate_logical_assignment_value_temps(&mut self, count: usize) {
        self.preallocated_logical_assignment_value_temps.clear();
        for _ in 0..count {
            let name = self.generate_fresh_temp_name();
            self.preallocated_logical_assignment_value_temps
                .push_back(name);
        }
    }

    fn count_logical_assignment_value_temps(&self, node_idx: NodeIndex) -> usize {
        if self.ctx.options.target.supports_es2020() || node_idx.is_none() {
            return 0;
        }

        let mut count = 0usize;
        let mut stack = vec![node_idx];

        while let Some(current) = stack.pop() {
            let Some(node) = self.arena.get(current) else {
                continue;
            };

            if let Some(binary) = self.arena.get_binary_expr(node)
                && binary.operator_token == SyntaxKind::QuestionQuestionEqualsToken as u16
            {
                count += 1;
            }

            if self.is_logical_assignment_temp_scope_boundary(node) {
                continue;
            }

            for child in self.arena.get_children(current) {
                stack.push(child);
            }
        }

        count
    }

    fn is_logical_assignment_temp_scope_boundary(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> bool {
        self.arena.get_function(node).is_some()
            || self.arena.get_method_decl(node).is_some()
            || self.arena.get_constructor(node).is_some()
            || self.arena.get_accessor(node).is_some()
            || self.arena.get_class(node).is_some()
    }

    pub(super) fn prepare_logical_assignment_value_temps(&mut self, node_idx: NodeIndex) {
        if self.ctx.options.target.supports_es2020() {
            return;
        }

        let count = self.count_logical_assignment_value_temps(node_idx);
        if count > 0 {
            self.preallocate_logical_assignment_value_temps(count);
        }
    }

    /// Like `make_unique_name` but also records the temp for hoisting as a `var` declaration.
    /// Used for assignment destructuring temps which need `var _a, _b, ...;` at scope top.
    pub(super) fn make_unique_name_hoisted(&mut self) -> String {
        let name = self.make_unique_name();
        self.hoisted_assignment_temps.push(name.clone());
        name
    }

    /// Like `make_unique_name` but records the temp for CJS destructuring export hoisting.
    /// These temps are emitted as `var _a;` BEFORE the `__esModule` marker.
    pub(super) fn make_unique_name_cjs_destructuring(&mut self) -> String {
        let name = self.make_unique_name();
        self.cjs_destructuring_export_temps.push(name.clone());
        name
    }

    /// Like `make_unique_name` but also records the temp for hoisting before references.
    /// Used for assignment target values in logical-assignment lowering.
    pub(super) fn make_unique_name_hoisted_value(&mut self) -> String {
        let name = if let Some(name) = self.preallocated_logical_assignment_value_temps.pop_front()
        {
            name
        } else {
            self.make_unique_name()
        };
        self.hoisted_assignment_value_temps.push(name.clone());
        name
    }

    // =========================================================================
    // Emitter Helpers
    // =========================================================================

    pub(super) fn emit_comma_separated(&mut self, nodes: &[NodeIndex]) {
        let mut first = true;
        let mut prev_end: Option<u32> = None;
        for &idx in nodes {
            if !first {
                // Map the `,` separator to its source position.
                // Try forward scan first; if not found (parser may include `,`
                // in the preceding node's range), scan backward from prev_end.
                if let Some(pe) = prev_end
                    && let Some(node) = self.arena.get(idx)
                {
                    self.map_token_after(pe, node.pos, b',');
                    if self.pending_source_pos.is_none() {
                        self.map_token_before(pe, pe.saturating_sub(2), b',');
                    }
                }
                self.write(", ");
            }
            // Emit comments between the previous node/comma and this node.
            // This handles comments like: func(a, /*comment*/ b, c) or func(/*c*/ a)
            if let Some(node) = self.arena.get(idx)
                && let Some(prev_end) = prev_end
            {
                // For non-first nodes, emit comments between previous node end and current node start
                self.emit_unemitted_comments_between(prev_end, node.pos);
            }
            first = false;
            if let Some(node) = self.arena.get(idx) {
                prev_end = Some(node.end);
            }
            self.emit(idx);
        }
    }

    /// Emit comments between two positions that haven't been emitted yet.
    /// This is used for comments in expression contexts (e.g., between function arguments).
    ///
    /// Returns `true` if the last emitted comment was a line comment (has trailing newline),
    /// meaning a newline was already written — callers should NOT write an additional newline.
    pub(crate) fn emit_unemitted_comments_between(&mut self, from_pos: u32, to_pos: u32) -> bool {
        self.emit_unemitted_comments_between_impl(from_pos, to_pos, true)
    }

    fn emit_unemitted_comments_between_impl(
        &mut self,
        from_pos: u32,
        to_pos: u32,
        emit_trailing_space: bool,
    ) -> bool {
        if self.ctx.options.remove_comments {
            return false;
        }

        let Some(text) = self.source_text else {
            return false;
        };

        // Scan through all_comments to find ones in range [from_pos, to_pos)
        // that come after the current comment_emit_idx position.
        // We use a temporary index to scan without modifying comment_emit_idx,
        // since we're looking for comments that may be ahead of the current
        // emission position.
        let mut scan_idx = self.comment_emit_idx;
        let mut last_had_trailing_newline = false;
        while scan_idx < self.all_comments.len() {
            let c = &self.all_comments[scan_idx];
            if c.pos >= from_pos && c.end <= to_pos {
                // Found a comment in our range - emit it
                let comment_text = safe_slice::slice(text, c.pos as usize, c.end as usize);
                let has_trailing_new_line = c.has_trailing_new_line;
                if !comment_text.is_empty() {
                    self.write_comment_with_reindent(comment_text, Some(c.pos));
                    if has_trailing_new_line {
                        self.write_line();
                    } else if emit_trailing_space {
                        self.write_space();
                    }
                    last_had_trailing_newline = has_trailing_new_line;
                }
                // Advance the main index past this comment
                self.comment_emit_idx = scan_idx + 1;
                scan_idx += 1;
            } else if c.end <= from_pos {
                // Comment is before our range - already handled by statement-level emission
                scan_idx += 1;
            } else if c.pos >= to_pos {
                // Comment is past our target position, stop scanning
                break;
            } else {
                // Comment overlaps with range boundaries, skip it
                scan_idx += 1;
            }
        }
        last_had_trailing_newline
    }

    pub(super) fn emit_heritage_expression(&mut self, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }

        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if let Some(expr) = self.arena.get_expr_type_args(node) {
            // ExpressionWithTypeArguments wrapper.
            // When the inner expression is an optional chain (A?.B) and target < ES2020,
            // the chain is lowered to a conditional expression that needs parens in extends.
            let needs_parens = self.heritage_expr_needs_optional_chain_parens(expr.expression);
            if needs_parens {
                self.write("(");
            }
            self.emit(expr.expression);
            if needs_parens {
                self.write(")");
            }
            // Type arguments are erased in JS output since JavaScript doesn't
            // support generics at runtime. Skip any comments inside the erased
            // type arguments so they don't leak into subsequent output.
            if !self.ctx.options.remove_comments
                && let Some(ref type_args) = expr.type_arguments
            {
                for &ta_idx in &type_args.nodes {
                    if let Some(ta_node) = self.arena.get(ta_idx) {
                        self.skip_comments_in_range(ta_node.pos, ta_node.end);
                    }
                }
            }
        } else {
            // Direct expression (no ExpressionWithTypeArguments wrapper).
            let needs_parens = self.heritage_expr_needs_optional_chain_parens(idx);
            if needs_parens {
                self.write("(");
            }
            self.emit(idx);
            if needs_parens {
                self.write(")");
            }
        }
    }

    /// Check if a heritage expression node is an optional chain that will be lowered
    /// and thus needs parenthesization in an `extends` clause.
    fn heritage_expr_needs_optional_chain_parens(&self, idx: NodeIndex) -> bool {
        if self.ctx.options.target.supports_es2020() {
            return false;
        }
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        // Check for PropertyAccessExpression/ElementAccessExpression with question_dot_token
        if let Some(access) = self.arena.get_access_expr(node) {
            return access.question_dot_token;
        }
        // Check for CallExpression with OPTIONAL_CHAIN flag
        (node.flags as u32) & node_flags::OPTIONAL_CHAIN != 0
    }

    // =========================================================================
    // Modifier Helpers
    // =========================================================================

    /// Check if a top-level statement is erased in JS emit (type-only, ambient, etc.).
    /// This includes interfaces, type aliases, declare function/class/enum/module/var,
    /// const enums, and function overload signatures (no body).
    pub(super) fn is_erased_statement(&self, node: &Node) -> bool {
        let is_es_module_output = matches!(
            self.ctx.options.module,
            ModuleKind::ES2015 | ModuleKind::ES2020 | ModuleKind::ES2022 | ModuleKind::ESNext
        );

        match node.kind {
            syntax_kind_ext::INTERFACE_DECLARATION
            | syntax_kind_ext::TYPE_ALIAS_DECLARATION
            | syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION => true,
            syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = self.arena.get_function(node) {
                    self.arena
                        .has_modifier(&func.modifiers, SyntaxKind::DeclareKeyword)
                        || func.body.is_none()
                } else {
                    false
                }
            }
            syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = self.arena.get_class(node) {
                    self.arena
                        .has_modifier(&class.modifiers, SyntaxKind::DeclareKeyword)
                } else {
                    false
                }
            }
            syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = self.arena.get_enum(node) {
                    self.arena
                        .has_modifier(&enum_decl.modifiers, SyntaxKind::DeclareKeyword)
                        || (self
                            .arena
                            .has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword)
                            && !self.ctx.options.preserve_const_enums)
                } else {
                    false
                }
            }
            syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(module) = self.arena.get_module(node) {
                    self.arena
                        .has_modifier(&module.modifiers, SyntaxKind::DeclareKeyword)
                        || !self.is_instantiated_module(module.body)
                } else {
                    false
                }
            }
            syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = self.arena.get_variable(node) {
                    if self
                        .arena
                        .has_modifier(&var_stmt.modifiers, SyntaxKind::DeclareKeyword)
                    {
                        return true;
                    }
                    // In CJS mode with `export =`, exported variables with no
                    // initializers are already covered by the `exports.X = void 0`
                    // preamble -- the bare `var X;` is redundant and should be erased.
                    if self.ctx.is_commonjs()
                        && self.ctx.module_state.has_export_assignment
                        && self.all_declarations_lack_initializer(&var_stmt.declarations)
                        && self.all_declaration_names_in_exported_set(&var_stmt.declarations)
                    {
                        return true;
                    }
                    false
                } else {
                    false
                }
            }
            syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export_data) = self.arena.get_export_decl(node) {
                    // `export type { ... }` is always erased
                    if export_data.is_type_only {
                        return true;
                    }
                    // `export <declaration>` - check if the inner declaration is erased
                    // (e.g., `export declare namespace Foo { ... }`, `export interface Bar { }`)
                    if let Some(inner_node) = self.arena.get(export_data.export_clause) {
                        // `export { type A, type B }` — all specifiers individually type-only
                        if inner_node.kind == syntax_kind_ext::NAMED_EXPORTS
                            && let Some(named_exports) = self.arena.get_named_imports(inner_node)
                        {
                            let all_type_only = !named_exports.elements.nodes.is_empty()
                                && named_exports.elements.nodes.iter().all(|&spec_idx| {
                                    if let Some(spec_node) = self.arena.get(spec_idx)
                                        && let Some(spec) = self.arena.get_specifier(spec_node)
                                    {
                                        spec.is_type_only
                                            || self.ctx.options.type_only_nodes.contains(&spec_idx)
                                    } else {
                                        false
                                    }
                                });
                            if all_type_only {
                                return true;
                            }
                        }
                        // `export default m` where `m` refers to a type-only entity
                        // (e.g., a non-instantiated namespace or interface) — the whole
                        // statement should be erased since there is nothing to export.
                        if export_data.is_default_export
                            && (inner_node.kind == SyntaxKind::Identifier as u16
                                || inner_node.kind == syntax_kind_ext::QUALIFIED_NAME)
                            && !self
                                .export_default_target_has_runtime_value(export_data.export_clause)
                        {
                            return true;
                        }
                        return self.is_erased_statement(inner_node);
                    }
                }
                false
            }
            syntax_kind_ext::IMPORT_DECLARATION => {
                if let Some(import_data) = self.arena.get_import_decl(node)
                    && let Some(clause_node) = self.arena.get(import_data.import_clause)
                    && let Some(clause) = self.arena.get_import_clause(clause_node)
                {
                    // `import type { ... } from '...'` is always erased
                    if clause.is_type_only {
                        return true;
                    }
                    // `import { type A, type B } from '...'` — all specifiers
                    // individually type-only → the whole import is erased.
                    if let Some(named_node) = self.arena.get(clause.named_bindings)
                        && named_node.kind == syntax_kind_ext::NAMED_IMPORTS
                        && let Some(named_imports) = self.arena.get_named_imports(named_node)
                        && clause.name.is_none()
                    {
                        let all_type_only = !named_imports.elements.nodes.is_empty()
                            && named_imports.elements.nodes.iter().all(|&spec_idx| {
                                if let Some(spec_node) = self.arena.get(spec_idx)
                                    && let Some(spec) = self.arena.get_specifier(spec_node)
                                {
                                    spec.is_type_only
                                        || self.ctx.options.type_only_nodes.contains(&spec_idx)
                                } else {
                                    false
                                }
                            });
                        if all_type_only {
                            return true;
                        }
                    }
                    // When JSX mode requires a factory, check if any binding
                    // matches the factory root name — JSX elements reference
                    // it implicitly and the text heuristic won't find it.
                    if matches!(
                        self.ctx.options.jsx,
                        JsxEmit::Preserve | JsxEmit::React | JsxEmit::ReactNative
                    ) {
                        let factory_root = self
                            .ctx
                            .options
                            .jsx_factory
                            .as_deref()
                            .and_then(|f| f.split('.').next())
                            .unwrap_or("React");
                        // Check default import name
                        if clause.name.is_some() {
                            let name = self.get_identifier_text_idx(clause.name);
                            if name == factory_root {
                                return false;
                            }
                        }
                        // Check namespace import name (`import * as React`)
                        if let Some(bindings_node) = self.arena.get(clause.named_bindings)
                            && let Some(named) = self.arena.get_named_imports(bindings_node)
                            && named.name.is_some()
                            && named.elements.nodes.is_empty()
                        {
                            let ns_name = self.get_identifier_text_idx(named.name);
                            if ns_name == factory_root {
                                return false;
                            }
                        }
                    }
                    // With --verbatimModuleSyntax, non-type-only imports are
                    // always preserved (no heuristic elision).
                    if self.ctx.options.verbatim_module_syntax {
                        return false;
                    }
                    // JS files (.js/.jsx/.cjs/.mjs) do not undergo import elision;
                    // tsc's checker treats all imports in JS files as value imports.
                    if self.source_is_js_file {
                        return false;
                    }
                    // In both CJS and ESM modes, erase imports whose bindings
                    // have no value-level usage in the rest of the file.
                    // In --noCheck mode this uses a text-based heuristic.
                    return !self.import_has_value_usage_after_node(node, clause);
                }
                false
            }
            syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                if let Some(import_data) = self.arena.get_import_decl(node) {
                    if import_data.is_type_only {
                        return true;
                    }
                    let is_external =
                        self.arena
                            .get(import_data.module_specifier)
                            .is_some_and(|module_node| {
                                module_node.kind == SyntaxKind::StringLiteral as u16
                                    || module_node.kind
                                        == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE
                            });
                    // TS1147: import = require() inside a namespace is always invalid;
                    // tsc erases these regardless of usage.
                    if is_external && self.in_namespace_iife {
                        return true;
                    }
                    if is_es_module_output && is_external {
                        return true;
                    }
                    if self.ctx.is_commonjs() && is_external {
                        // With --verbatimModuleSyntax or in JS files, non-type-only
                        // import-equals declarations are always preserved.
                        if self.ctx.options.verbatim_module_syntax || self.source_is_js_file {
                            return false;
                        }
                        // When JSX mode requires a factory (React by default),
                        // don't erase imports matching the factory name — JSX
                        // elements implicitly reference it but the text-based
                        // heuristic won't find it in the source.
                        if matches!(
                            self.ctx.options.jsx,
                            JsxEmit::Preserve | JsxEmit::React | JsxEmit::ReactNative
                        ) {
                            let import_name =
                                self.get_identifier_text_idx(import_data.import_clause);
                            let factory_root = self
                                .ctx
                                .options
                                .jsx_factory
                                .as_deref()
                                .and_then(|f| f.split('.').next())
                                .unwrap_or("React");
                            if import_name == factory_root {
                                return false;
                            }
                        }
                        return !self.import_equals_has_value_usage_after_node(node, import_data);
                    }
                }
                false
            }
            syntax_kind_ext::EXPORT_ASSIGNMENT => {
                // In ES module emit, legacy `export =` is erased — but only at the
                // top level (source-file statements).  When `export =` appears inside
                // a function body or namespace IIFE (syntactically invalid positions),
                // tsc emits it verbatim, so we must not erase it.
                is_es_module_output
                    && self.function_scope_depth == 0
                    && !self.in_namespace_iife
                    && self
                        .arena
                        .get_export_assignment(node)
                        .is_some_and(|export_assign| export_assign.is_export_equals)
            }
            syntax_kind_ext::EXPRESSION_STATEMENT => {
                // When the parser fails to recognize `declare` as a modifier prefix
                // (e.g., `declare import a = b;` or `declare declare var x;`), it may
                // produce a bare `declare;` expression statement. We check the source to
                // confirm this was a modifier (followed by a keyword on the same line),
                // not a legitimate `declare` identifier expression.
                if let Some(expr_stmt) = self.arena.get_expression_statement(node)
                    && let Some(expr_node) = self.arena.get(expr_stmt.expression)
                    && let Some(ident) = self.arena.get_identifier(expr_node)
                    && ident.escaped_text == "declare"
                {
                    self.is_declare_modifier_artifact(node)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Check if a `declare;` expression statement is an artifact of the parser not
    /// recognizing `declare` as a modifier before certain keywords. Looks at the source
    /// text after `declare` to see if the next non-whitespace content on the same line
    /// is a keyword (import, export, declare, await, using, etc.) rather than `;` or a
    /// newline, which would indicate a legitimate expression statement.
    pub(super) fn is_declare_modifier_artifact(&self, node: &Node) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let bytes = text.as_bytes();
        // Start scanning after the `declare` keyword (7 chars: "declare")
        let declare_end = node.pos as usize + 7;
        let node_end = node.end as usize;
        if declare_end >= bytes.len() || declare_end >= node_end {
            return false;
        }
        // Skip leading trivia (whitespace) to find where `declare` actually starts
        let mut pos = node.pos as usize;
        while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
            pos += 1;
        }
        // Verify this actually starts with "declare"
        if pos + 7 > bytes.len() || &bytes[pos..pos + 7] != b"declare" {
            return false;
        }
        pos += 7;
        // Skip spaces/tabs after "declare" (but NOT newlines — a newline means ASI)
        while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
            pos += 1;
        }
        // If we hit a newline, semicolon, or end of source, this is a real expression
        if pos >= bytes.len() || bytes[pos] == b'\n' || bytes[pos] == b'\r' || bytes[pos] == b';' {
            return false;
        }
        // Check if the next token is a keyword that `declare` should modify
        let remaining = &text[pos..];
        remaining.starts_with("import")
            || remaining.starts_with("export")
            || remaining.starts_with("declare")
            || remaining.starts_with("function")
            || remaining.starts_with("class")
            || remaining.starts_with("abstract")
            || remaining.starts_with("interface")
            || remaining.starts_with("type")
            || remaining.starts_with("enum")
            || remaining.starts_with("namespace")
            || remaining.starts_with("module")
            || remaining.starts_with("var")
            || remaining.starts_with("let")
            || remaining.starts_with("const")
            || remaining.starts_with("async")
            || remaining.starts_with("await")
            || remaining.starts_with("using")
            || remaining.starts_with("global")
    }

    /// Check if a module/namespace has any value-producing (instantiated) members.
    /// A module is NOT instantiated if it only contains type-only declarations
    /// (interfaces, type aliases, import type, etc.) or is empty.
    /// TypeScript skips emitting IIFE wrappers for non-instantiated modules.
    pub(super) fn is_instantiated_module(&self, module_body: NodeIndex) -> bool {
        crate::transforms::emit_utils::is_instantiated_module_ext(
            self.arena,
            module_body,
            self.ctx.options.preserve_const_enums,
        )
    }

    /// Scan forward from `pos` past whitespace and comments to find the actual
    /// token start. Used because node.pos includes leading trivia.
    pub(super) fn skip_trivia_forward(&self, start: u32, end: u32) -> u32 {
        crate::transforms::emit_utils::skip_trivia_forward(self.source_text, start, end)
    }

    /// Scan forward from `pos` past whitespace only (preserving comments).
    /// Used to find the start of a statement while preserving comments
    /// that may belong to nested expressions.
    pub fn skip_whitespace_forward(&self, start: u32, end: u32) -> u32 {
        let Some(text) = self.source_text else {
            return start;
        };
        let bytes = text.as_bytes();
        let mut pos = start as usize;
        let end = std::cmp::min(end as usize, bytes.len());
        while pos < end {
            match bytes[pos] {
                b' ' | b'\t' | b'\r' | b'\n' => pos += 1,
                _ => break,
            }
        }
        pos as u32
    }

    /// Returns true if the source character just before `c_pos` (skipping spaces/tabs)
    /// is a newline — meaning the comment at `c_pos` starts on its own line rather than
    /// being a trailing same-line comment.
    pub(super) fn comment_preceded_by_newline(&self, c_pos: u32) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let bytes = text.as_bytes();
        let mut i = c_pos as usize;
        while i > 0 {
            i -= 1;
            match bytes[i] {
                b' ' | b'\t' => continue,
                b'\n' | b'\r' => return true,
                _ => return false,
            }
        }
        false
    }

    /// Find the position of a specific byte in source text between `from` and `to`.
    pub(super) fn find_char_after(&self, from: u32, to: u32, ch: u8) -> Option<u32> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let end = (to as usize).min(bytes.len());
        let mut i = from as usize;
        while i < end {
            if bytes[i] == ch {
                return Some(i as u32);
            }
            i += 1;
        }
        None
    }

    /// Find the position of the first top-level ',' in source text after `from` and before `to`.
    /// Skips over nested brackets, strings, and comments so we don't match commas inside
    /// nested expressions (e.g. `[a, [b, c], d]` — the inner comma is skipped).
    pub(super) fn find_comma_pos_after(&self, from: u32, to: u32) -> Option<u32> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let to = to as usize;
        let mut i = from as usize;
        let mut depth = 0i32;
        while i < to.min(bytes.len()) {
            match bytes[i] {
                b',' if depth == 0 => return Some(i as u32),
                b'(' | b'[' | b'{' => {
                    depth += 1;
                    i += 1;
                }
                b')' | b']' | b'}' => {
                    if depth > 0 {
                        depth -= 1;
                    } else {
                        break; // exited our scope
                    }
                    i += 1;
                }
                b'\'' | b'"' => {
                    let q = bytes[i];
                    i += 1;
                    while i < to.min(bytes.len()) {
                        if bytes[i] == b'\\' {
                            i += 2;
                        } else if bytes[i] == q {
                            i += 1;
                            break;
                        } else {
                            i += 1;
                        }
                    }
                }
                b'`' => {
                    i += 1;
                    while i < to.min(bytes.len()) {
                        if bytes[i] == b'\\' {
                            i += 2;
                        } else if bytes[i] == b'`' {
                            i += 1;
                            break;
                        } else {
                            i += 1;
                        }
                    }
                }
                b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                    i += 2;
                    while i < to.min(bytes.len()) && bytes[i] != b'\n' {
                        i += 1;
                    }
                }
                b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                    i += 2;
                    while i + 1 < to.min(bytes.len()) {
                        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => {
                    i += 1;
                }
            }
        }
        None
    }

    /// Check if the source text has a trailing comma after the last element
    /// in a list (object literal, array literal, etc.)
    ///
    /// Scans backwards from the closing bracket/brace to find if there's a
    /// comma before it (skipping whitespace). The parser includes the trailing
    /// comma in the last element's `end` position, so we scan backwards from
    /// the container's closing delimiter instead.
    pub(super) fn has_trailing_comma_in_source(
        &self,
        container: &tsz_parser::parser::node::Node,
        elements: &[NodeIndex],
    ) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };

        let end = std::cmp::min(container.end as usize, text.len());
        if end == 0 {
            return false;
        }

        let bytes = text.as_bytes();

        // Find the closing bracket/brace by scanning backwards from the container end
        let mut pos = end;
        while pos > 0 {
            pos -= 1;
            match bytes[pos] {
                b'}' | b']' | b')' => break,
                _ => continue,
            }
        }

        // Scan backwards from the closing bracket to find comma (skipping whitespace and comments).
        // This matches TypeScript behavior for cases like `yield 1, /*comment*/`.
        while pos > 0 {
            pos -= 1;
            if bytes[pos].is_ascii_whitespace() {
                continue;
            }

            // Skip block comments when scanning backwards.
            // We land on the `/` of `*/` when scanning right-to-left.
            if bytes[pos] == b'/' && pos > 0 && bytes[pos - 1] == b'*' {
                pos -= 1; // now at '*'
                // Find the matching `/*`
                while pos > 1 {
                    pos -= 1;
                    if bytes[pos] == b'*' && pos > 0 && bytes[pos - 1] == b'/' {
                        pos -= 1; // now at '/'
                        break;
                    }
                }
                continue;
            }

            // Skip line comments: the current `pos` might be inside a `//`
            // comment that either starts the line or appears inline after code
            // (e.g. `value, // comment`).  Scan forwards from the start of the
            // line to find the first `//` that is not inside a string/regex,
            // and if `pos` is at or after it, rewind to just before the `//`.
            {
                // Find the start of the current line.
                let line_start = {
                    let mut ls = pos;
                    while ls > 0 && bytes[ls - 1] != b'\n' {
                        ls -= 1;
                    }
                    ls
                };

                // Scan forward through the line to find an unquoted `//`.
                // We do a simplified scan: track single/double quotes and
                // skip escaped characters.  Regex literals could in theory
                // contain `//` but that is extremely rare and would require a
                // full parser rescan; the simplified approach is sufficient for
                // the trailing-comma detection use case.
                let mut scan = line_start;
                let mut found_line_comment = None;
                while scan < pos {
                    let b = bytes[scan];
                    if b == b'/' && scan + 1 < bytes.len() && bytes[scan + 1] == b'/' {
                        found_line_comment = Some(scan);
                        break;
                    }
                    // Skip string literals so `"//"` doesn't trigger.
                    if b == b'"' || b == b'\'' || b == b'`' {
                        scan += 1;
                        while scan < bytes.len() && bytes[scan] != b {
                            if bytes[scan] == b'\\' {
                                scan += 1; // skip escaped char
                            }
                            scan += 1;
                        }
                        // skip closing quote
                        scan += 1;
                        continue;
                    }
                    scan += 1;
                }

                if let Some(comment_start) = found_line_comment
                    && pos >= comment_start
                {
                    // `pos` is inside (or at) the line comment; rewind
                    // to just before the `//`.
                    pos = comment_start;
                    // Now continue the outer loop which will decrement
                    // pos and re-check.
                    continue;
                }
            }

            return bytes[pos] == b',';
        }

        // Fallback for recovery/edge cases: if source between the last element
        // and the container close contains a comma, treat it as trailing comma.
        if let Some(&last_idx) = elements.last()
            && let Some(last_node) = self.arena.get(last_idx)
        {
            let start = std::cmp::min(last_node.end as usize, text.len());
            let end = std::cmp::min(container.end as usize, text.len());
            if start < end && text[start..end].contains(',') {
                return true;
            }
        }

        false
    }
}
