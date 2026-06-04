impl<'a> ES5ClassTransformer<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            class_name: String::new(),
            has_extends: false,
            extends_null: false,
            super_name: "_super".to_string(),
            private_fields: Vec::new(),
            private_accessors: Vec::new(),
            private_methods: Vec::new(),
            private_instances_weakset_name: None,
            auto_accessors: Vec::new(),
            transforms: None,
            source_text: None,
            class_decorators: Vec::new(),
            legacy_decorators: false,
            emit_decorator_metadata: false,
            tc39_decorators: false,
            tc39_has_instance_member_decorators: false,
            tc39_es5_member_decorators: Vec::new(),
            indent_base: 0,
            temp_var_counter: Cell::new(0),
            computed_prop_temp_map: std::collections::HashMap::new(),
            current_static_class_alias: None,
            class_self_reference_alias: None,
            extends_this_captured: false,
            skip_static_field_initializers: false,
            use_define_for_class_fields: false,
            tslib_prefix: false,
            tslib_import_binding: "tslib_1".to_string(),
            commonjs_import_substitutions: FxHashMap::default(),
            module_kind: ModuleKind::None,
            target_es5: false,
            downlevel_iteration: false,
            dynamic_import_promise_counter: Cell::new(1),
            async_generator_inner_name_counts: RefCell::new(FxHashMap::default()),
            disposable_env_counter: Cell::new(1),
            blocked_disposable_env_names: RefCell::new(FxHashSet::default()),
            generated_disposable_env_names: RefCell::new(Vec::new()),
            extra_hoisted_temps: RefCell::new(Vec::new()),
            emit_computed_props_outside: Cell::new(false),
            outer_rename_map: FxHashMap::default(),
            inherited_computed_name_super: None,
            inherited_computed_name_this: None,
        }
    }

    /// Set the outer block-scope rename map (original → emitted name for
    /// variables renamed during ES5 lowering in enclosing scopes).
    pub fn set_outer_rename_map(&mut self, map: FxHashMap<String, String>) {
        self.outer_rename_map = map;
    }

    /// Record the super name of an enclosing *instance* member when this nested
    /// class is lowered inside that member's body. Enables prototype-qualified
    /// `super` lowering for `super` references that appear in this class's
    /// computed property names.
    pub fn set_inherited_computed_name_super(&mut self, super_name: String) {
        self.inherited_computed_name_super = Some(super_name);
    }

    pub fn set_inherited_computed_name_this(&mut self, this_alias: String) {
        self.inherited_computed_name_this = Some(this_alias);
    }

    pub const fn set_use_define_for_class_fields(&mut self, enable: bool) {
        self.use_define_for_class_fields = enable;
    }

    pub fn set_emit_computed_props_outside(&self, val: bool) {
        self.emit_computed_props_outside.set(val);
    }

    pub const fn set_tc39_decorators(&mut self, enabled: bool) {
        self.tc39_decorators = enabled;
    }

    pub const fn set_skip_static_members(&mut self, skip: bool) {
        self.skip_static_field_initializers = skip;
    }

    pub fn set_class_self_reference_alias(&mut self, alias: String) {
        self.class_self_reference_alias = Some(alias);
    }

    pub const fn set_extends_this_captured(&mut self, captured: bool) {
        self.extends_this_captured = captured;
    }

    pub fn set_commonjs_import_substitutions(&mut self, subs: FxHashMap<String, String>) {
        self.commonjs_import_substitutions = subs;
    }

    pub const fn set_tslib_prefix(&mut self, enable: bool) {
        self.tslib_prefix = enable;
    }

    pub fn set_tslib_import_binding(&mut self, binding: String) {
        self.tslib_import_binding = binding;
    }

    pub const fn set_module_kind(&mut self, module_kind: ModuleKind) {
        self.module_kind = module_kind;
    }

    pub const fn set_target_es5(&mut self, es5: bool) {
        self.target_es5 = es5;
    }

    pub const fn set_downlevel_iteration(&mut self, downlevel_iteration: bool) {
        self.downlevel_iteration = downlevel_iteration;
    }

    pub fn set_dynamic_import_promise_counter(&self, next_id: u32) {
        self.dynamic_import_promise_counter.set(next_id);
    }

    pub const fn dynamic_import_promise_counter(&self) -> u32 {
        self.dynamic_import_promise_counter.get()
    }

    pub fn set_async_generator_inner_name_counts(&mut self, counts: FxHashMap<String, u32>) {
        *self.async_generator_inner_name_counts.borrow_mut() = counts;
    }

    pub fn take_async_generator_inner_name_counts(&self) -> FxHashMap<String, u32> {
        std::mem::take(&mut *self.async_generator_inner_name_counts.borrow_mut())
    }

    fn next_async_generator_inner_name(&self, base: &str) -> String {
        loop {
            let candidate = {
                let mut counts = self.async_generator_inner_name_counts.borrow_mut();
                let count = counts
                    .entry(base.to_string())
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
                format!("{base}_{count}")
            };
            if !self
                .arena
                .identifiers
                .iter()
                .any(|identifier| identifier.escaped_text == candidate)
            {
                return candidate;
            }
        }
    }

    pub fn set_temp_var_counter(&mut self, counter: u32) {
        self.temp_var_counter.set(counter);
    }

    pub const fn temp_var_counter(&self) -> u32 {
        self.temp_var_counter.get()
    }

    pub fn set_disposable_env_context<I>(&mut self, next_id: u32, blocked_names: I)
    where
        I: IntoIterator<Item = String>,
    {
        self.disposable_env_counter.set(next_id);
        *self.blocked_disposable_env_names.borrow_mut() = blocked_names.into_iter().collect();
        self.generated_disposable_env_names.borrow_mut().clear();
    }

    pub const fn disposable_env_counter(&self) -> u32 {
        self.disposable_env_counter.get()
    }

    pub fn take_generated_disposable_env_names(&self) -> Vec<String> {
        std::mem::take(&mut *self.generated_disposable_env_names.borrow_mut())
    }

    fn configure_async_disposable_context(&self, transformer: &mut AsyncES5Transformer<'a>) {
        transformer.set_disposable_env_context(
            self.disposable_env_counter.get(),
            self.blocked_disposable_env_names.borrow().iter().cloned(),
        );
    }

    fn sync_async_disposable_context(&self, transformer: &mut AsyncES5Transformer<'a>) {
        self.disposable_env_counter
            .set(transformer.disposable_env_counter());
        let generated = transformer.take_generated_disposable_env_names();
        let mut blocked = self.blocked_disposable_env_names.borrow_mut();
        let mut all_generated = self.generated_disposable_env_names.borrow_mut();
        for name in generated {
            blocked.insert(name.clone());
            all_generated.push(name);
        }
    }

    fn fresh_super_name(&self) -> String {
        let mut suffix = 0usize;
        loop {
            let candidate = if suffix == 0 {
                "_super".to_string()
            } else {
                format!("_super_{suffix}")
            };
            if !self
                .arena
                .identifiers
                .iter()
                .any(|identifier| identifier.escaped_text == candidate)
            {
                return candidate;
            }
            suffix += 1;
        }
    }

    /// Check if an expression (possibly wrapped in type assertions) is side-effect-free.
    fn is_expr_side_effect_free(arena: &NodeArena, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = arena.get(expr_idx) else {
            return true;
        };
        let k = expr_node.kind;
        if k == SyntaxKind::Identifier as u16
            || k == SyntaxKind::PrivateIdentifier as u16
            || k == SyntaxKind::StringLiteral as u16
            || k == SyntaxKind::NumericLiteral as u16
            || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            || k == SyntaxKind::TrueKeyword as u16
            || k == SyntaxKind::FalseKeyword as u16
            || k == SyntaxKind::NullKeyword as u16
            || k == SyntaxKind::UndefinedKeyword as u16
        {
            return true;
        }
        // Look through type assertions
        if (k == syntax_kind_ext::TYPE_ASSERTION || k == syntax_kind_ext::AS_EXPRESSION)
            && let Some(a) = arena.get_type_assertion(expr_node)
        {
            return Self::is_expr_side_effect_free(arena, a.expression);
        }
        // Look through parenthesized expressions
        if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(p) = arena.get_parenthesized(expr_node)
        {
            return Self::is_expr_side_effect_free(arena, p.expression);
        }
        false
    }

    /// Generate a unique temp variable name using TypeScript's ES5 temp sequence.
    fn generate_temp_name(&self) -> String {
        loop {
            let idx = self.temp_var_counter.get();
            self.temp_var_counter.set(idx + 1);
            if idx < 26 && (idx == 8 || idx == 13) {
                continue;
            }
            return es5_temp_name(idx);
        }
    }

    /// Set the base indent level for nested contexts (e.g., 1 for class inside namespace)
    pub const fn set_indent_base(&mut self, level: u32) {
        self.indent_base = level;
    }

    /// Set class-level decorators to emit inside the IIFE
    pub fn set_class_decorators(&mut self, decorators: Vec<NodeIndex>) {
        self.class_decorators = decorators;
    }

    /// Enable legacy decorator lowering (emits __decorate calls for members inside the IIFE)
    pub const fn set_legacy_decorators(&mut self, enabled: bool) {
        self.legacy_decorators = enabled;
    }

    /// Enable `__metadata` emission in `__decorate` arrays
    pub const fn set_emit_decorator_metadata(&mut self, enabled: bool) {
        self.emit_decorator_metadata = enabled;
    }

    /// Set transform directives from `LoweringPass`
    pub fn set_transforms(&mut self, transforms: TransformContext) {
        self.transforms = Some(transforms);
    }

    /// Set source text for comment extraction
    pub const fn set_source_text(&mut self, source_text: &'a str) {
        self.source_text = Some(source_text);
    }

    /// Append the property's immediately-preceding leading comment (if any)
    /// to `body`. When a class property's initializer is lifted into the
    /// constructor, the comment that decorated the property in source must move
    /// with it — otherwise the user-authored documentation silently disappears.
    fn emit_property_leading_comment(&self, body: &mut Vec<IRNode>, prop_idx: NodeIndex) {
        let Some(prop_node) = self.arena.get(prop_idx) else {
            return;
        };
        let prop_name_pos = self
            .arena
            .get_property_decl(prop_node)
            .and_then(|prop| self.arena.get(prop.name))
            .map(|name| name.pos as usize);
        let Some(text) = self.source_text else {
            return;
        };
        let bytes = text.as_bytes();
        let scan_positions = prop_name_pos
            .into_iter()
            .chain(std::iter::once(prop_node.pos as usize));
        for scan_pos in scan_positions {
            if let Some(comment) = Self::property_leading_comment_before(text, bytes, scan_pos) {
                body.push(IRNode::Raw(comment.into()));
                return;
            }
        }
    }

    fn property_leading_comment_before(
        text: &str,
        bytes: &[u8],
        scan_pos: usize,
    ) -> Option<String> {
        let mut i = scan_pos;
        if i > bytes.len() {
            return None;
        }
        while i > 0 && matches!(bytes[i - 1], b' ' | b'\t' | b'\n' | b'\r') {
            i -= 1;
        }
        let line_start = text[..i].rfind('\n').map_or(0, |idx| idx + 1);
        if text[line_start..i].trim_start().starts_with("//") {
            return Some(text[line_start..i].to_string());
        }
        if i < 2 || &bytes[i - 2..i] != b"*/" {
            return None;
        }
        let comment_end = i;
        let mut start = i.saturating_sub(2);
        loop {
            if start + 2 <= bytes.len() && &bytes[start..start + 2] == b"/*" {
                let comment_text = &text[start..comment_end];
                return Some(comment_text.to_string());
            }
            if start == 0 {
                return None;
            }
            start -= 1;
        }
    }

    fn emit_leading_statement_comments(
        &self,
        body: &mut Vec<IRNode>,
        prev_end: u32,
        stmt_pos: u32,
    ) {
        let Some(source_text) = self.source_text else {
            return;
        };
        let start = std::cmp::min(prev_end as usize, source_text.len());
        let end = std::cmp::min(stmt_pos as usize, source_text.len());
        if start >= end {
            return;
        }
        let segment = &source_text[start..end];
        let mut block_lines: Option<Vec<String>> = None;
        for line in segment.lines() {
            if let Some(ref mut acc) = block_lines {
                acc.push(line.trim_end().to_string());
                if line.contains("*/") {
                    let collected = block_lines.take().expect("block was active");
                    body.push(IRNode::Raw(collected.join("\n").into()));
                }
                continue;
            }

            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                body.push(IRNode::Raw(trimmed.to_string().into()));
            } else if trimmed.starts_with("/*") {
                if trimmed.contains("*/") {
                    body.push(IRNode::Raw(trimmed.to_string().into()));
                } else {
                    // Begin a multi-line block comment. Preserve indentation on
                    // the opening line so subsequent lines retain their relative
                    // alignment when rejoined.
                    block_lines = Some(vec![line.trim_end().to_string()]);
                }
            }
        }
    }

    fn emit_empty_block_comments(
        &self,
        body: &mut Vec<IRNode>,
        block_node: &tsz_parser::parser::node::Node,
    ) {
        let Some(source_text) = self.source_text else {
            return;
        };
        let bytes = source_text.as_bytes();
        let start = block_node.pos as usize;
        let end = std::cmp::min(block_node.end as usize, bytes.len());
        if start >= end {
            return;
        }
        let Some(open_offset) = bytes[start..end].iter().position(|&b| b == b'{') else {
            return;
        };
        let comment_start = start + open_offset + 1;
        for comment in crate::emitter::get_leading_comment_ranges(source_text, comment_start) {
            if comment.end as usize > end {
                break;
            }
            if !source_text[comment_start..comment.pos as usize].contains('\n') {
                continue;
            }
            let text = &source_text[comment.pos as usize..comment.end as usize];
            let normalized = text
                .lines()
                .map(str::trim_end)
                .collect::<Vec<_>>()
                .join("\n");
            body.push(IRNode::Raw(normalized.into()));
        }
    }

    fn source_has_semicolon_between(&self, start: u32, end: u32) -> bool {
        let Some(source_text) = self.source_text else {
            return false;
        };
        let start = std::cmp::min(start as usize, source_text.len());
        let end = std::cmp::min(end as usize, source_text.len());
        start < end && source_text[start..end].contains(';')
    }

    /// Extract leading `JSDoc` comment from a node (if any).
    /// Returns the comment text including the `/** ... */` delimiters.
    ///
    /// Scans backward from `node.pos` (the token start, not including trivia)
    /// looking for an immediately adjacent block comment separated only by
    /// whitespace.  This avoids the pitfall of the old forward-scan approach
    /// which was confused when `node.end` of the previous sibling included
    /// the current member's trivia.
    fn extract_leading_comment(&self, node: &tsz_parser::parser::node::Node) -> Option<String> {
        let source_text = self.source_text?;
        let bytes = source_text.as_bytes();
        let pos = node.pos as usize;
        if pos == 0 {
            return None;
        }

        // Scan backward from `pos` skipping whitespace/newlines.
        // If we find `*/` we look further back for the matching `/*`.
        let mut i = pos;
        // Skip trailing whitespace/newlines before the token
        while i > 0 && matches!(bytes[i - 1], b' ' | b'\t' | b'\r' | b'\n') {
            i -= 1;
        }

        // Check if we landed on `*/` (end of a block comment)
        if i >= 2 && bytes[i - 1] == b'/' && bytes[i - 2] == b'*' {
            let comment_end = i; // exclusive end of comment text
            // Scan backwards to find the matching `/*`
            // We look for the LAST `/*` before this position that is a true
            // comment opener (not inside a string — simplified scan).
            let mut j = i - 2; // j points at `*` of `*/`
            loop {
                if j < 2 {
                    break;
                }
                // Look for `/*` or `/**`
                if bytes[j - 1] == b'/' && bytes[j] == b'*' {
                    // Found `/*` at j-1..j+1
                    let comment_start = j - 1;
                    let comment_text = &source_text[comment_start..comment_end];
                    if comment_text.starts_with("/**") && !comment_text.starts_with("/***") {
                        return Some(comment_text.to_string());
                    }
                    if comment_text.starts_with("/*") {
                        return Some(comment_text.to_string());
                    }
                    break;
                }
                j -= 1;
            }
        }

        // Check for line comment (`// ...`).
        // At this point `i` is just past the last non-whitespace char before the node.
        // Scan backward to find the start of that line, then check for `//`.
        if i > 0 {
            let line_end = i;
            let mut line_start = i;
            while line_start > 0 && bytes[line_start - 1] != b'\n' {
                line_start -= 1;
            }
            let line = source_text[line_start..line_end].trim_start();
            if line.starts_with("//") {
                return Some(line.to_string());
            }
        }

        None
    }

    /// Extract trailing comment on the same line as a class method's closing `}`.
    ///
    /// Finds the first `}` at brace depth 0 within the body block — that is, the
    /// actual closing brace of the function body — and returns any trailing comment
    /// on the same line.  Previous code scanned the entire body range and picked the
    /// LAST `}` with a trailing comment, which could accidentally pick up the class's
    /// closing brace comment instead of the method's own comment.
    fn extract_trailing_comment_for_method(&self, body_idx: NodeIndex) -> Option<String> {
        let source_text = self.source_text?;
        let close_brace = self.body_closing_brace_pos(body_idx)?;
        crate::emitter::get_trailing_comment_ranges(source_text, close_brace + 1)
            .first()
            .map(|c| source_text[c.pos as usize..c.end as usize].to_string())
    }

    fn body_closing_brace_pos(&self, body_idx: NodeIndex) -> Option<usize> {
        let source_text = self.source_text?;
        let body_node = self.arena.get(body_idx)?;
        let bytes = source_text.as_bytes();
        let start = body_node.pos as usize;
        let end = (body_node.end as usize).min(bytes.len());
        if start >= end {
            return None;
        }
        // Track brace depth starting from the opening `{` of the block.
        // We skip the initial opening brace (depth stays 0 initially).
        // For each `{` after that, depth increments; for each `}`, if depth==0
        // we have found the matching closing brace of the block; otherwise decrement.
        let mut depth: usize = 0;
        let mut in_string: Option<u8> = None; // `'` or `"`
        let mut i = start;
        while i < end {
            let byte = bytes[i];
            // Rudimentary string/template literal skip to avoid counting braces inside strings
            if in_string.is_none() {
                match byte {
                    b'{' => {
                        // Skip the opening brace of the body block itself (depth stays 0)
                        if i == start {
                            // opening brace of the block — don't count
                        } else {
                            depth += 1;
                        }
                    }
                    b'}' => {
                        if depth == 0 {
                            return Some(i);
                        }
                        depth -= 1;
                    }
                    b'\'' | b'"' | b'`' => {
                        in_string = Some(byte);
                    }
                    _ => {}
                }
            } else if let Some(delim) = in_string {
                if byte == b'\\' {
                    i += 1; // skip escaped char
                } else if byte == delim {
                    in_string = None;
                }
            }
            i += 1;
        }
        None
    }

    fn extract_trailing_comment_for_node(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> Option<String> {
        let source_text = self.source_text?;
        for comment in crate::emitter::get_trailing_comment_ranges(source_text, node.end as usize) {
            let comment_text = &source_text[comment.pos as usize..comment.end as usize];
            let trimmed = comment_text.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("/*") {
                return Some(comment_text.to_string());
            }
        }

        None
    }

    pub(super) fn extract_trailing_comment_for_class_field(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> Option<String> {
        if let Some(comment) = self.extract_trailing_comment_for_node(node) {
            return Some(comment);
        }

        let source_text = self.source_text?;
        let start = node.pos as usize;
        let end = (node.end as usize).min(source_text.len());
        if start >= end {
            return None;
        }

        let line_end = source_text[end..]
            .find(['\n', '\r'])
            .map_or(source_text.len(), |offset| end + offset);
        let mut after_field = end;
        while after_field < line_end {
            let ch = source_text[after_field..].chars().next()?;
            if ch.is_whitespace() {
                after_field += ch.len_utf8();
                continue;
            }
            if ch == ';' {
                for comment in
                    crate::emitter::get_trailing_comment_ranges(source_text, after_field + 1)
                {
                    let comment_text = &source_text[comment.pos as usize..comment.end as usize];
                    let trimmed = comment_text.trim_start();
                    if trimmed.starts_with("//") || trimmed.starts_with("/*") {
                        return Some(comment_text.to_string());
                    }
                }
            }
            break;
        }

        for comment in tsz_common::comments::get_comment_ranges(&source_text[start..end]) {
            let comment_pos = start + comment.pos as usize;
            let comment_end = start + comment.end as usize;
            let line_start = source_text[..comment_pos]
                .rfind(['\n', '\r'])
                .map_or(0, |pos| pos + 1);
            if !source_text[line_start..comment_pos].trim().is_empty() {
                return Some(source_text[comment_pos..comment_end].to_string());
            }
        }

        None
    }

    /// Create a base `AstToIr` converter with shared temp var counter and transforms
    fn make_converter(&self) -> AstToIr<'a> {
        let mut converter = AstToIr::new(self.arena)
            .with_super(self.has_extends)
            .with_super_name(self.super_name.clone())
            .with_temp_var_counter(self.temp_var_counter.get())
            .with_disposable_env_context(
                self.disposable_env_counter.get(),
                self.blocked_disposable_env_names.borrow().iter().cloned(),
            )
            .with_dynamic_import_promise_counter(self.dynamic_import_promise_counter.get())
            .with_class_transformer_indent_base(self.indent_base + 2)
            .with_downlevel_iteration(self.downlevel_iteration)
            .with_module_kind(self.module_kind)
            .with_target_es5(self.target_es5)
            .with_private_field_maps(&self.private_fields, &self.private_accessors);
        if let Some(source_text) = self.source_text {
            converter = converter.with_source_text(source_text);
        }
        if let Some(ref transforms) = self.transforms {
            converter = converter.with_transforms(transforms.clone());
        }
        if !self.outer_rename_map.is_empty() {
            converter = converter.with_outer_rename_map(self.outer_rename_map.clone());
        }
        converter
    }

    fn convert_statement_with_context(
        &self,
        idx: NodeIndex,
        is_static: bool,
        emit_await_as_yield: bool,
        class_alias: Option<&str>,
        lexical_this_capture_alias: Option<&str>,
        trailing_comment_limit: Option<u32>,
    ) -> IRNode {
        let mut converter = self
            .make_converter()
            .with_trailing_comment_limit(trailing_comment_limit);
        if is_static {
            converter = converter.with_static(true);
        }
        if emit_await_as_yield {
            converter = converter.with_await_as_yield(true);
        }
        if let Some(alias) = class_alias {
            converter = converter.with_class_alias(Some(alias.to_string()));
        }
        if let Some(alias) = lexical_this_capture_alias {
            converter = converter.with_lexical_this_capture_alias(Some(alias.to_string()));
        }
        if let Some(alias) = self.class_self_reference_alias.as_ref() {
            converter =
                converter.with_identifier_substitution(self.class_name.clone(), alias.clone());
        }
        let result = converter.convert_statement(idx);
        self.collect_from_converter(&converter);
        if matches!(result, IRNode::ASTRef(_) | IRNode::Raw(_))
            && let Some(operand) = self.recovered_throw_operand_text(idx)
        {
            IRNode::ThrowStatement(Box::new(IRNode::Raw(operand.into())))
        } else {
            result
        }
    }

    fn recovered_throw_operand_text(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        if node.kind != syntax_kind_ext::THROW_STATEMENT {
            return None;
        }

        let source_text = self.source_text?;
        let start = (node.pos as usize).min(source_text.len());
        let end = (node.end as usize).min(source_text.len());
        let statement = source_text[start..end].trim();
        let operand = statement.strip_prefix("throw")?.trim();
        let operand = operand.trim_end_matches(';').trim_end();
        operand.ends_with('.').then(|| operand.to_string())
    }

    /// Collect hoisted temps from a converter and update our temp counter
    fn collect_from_converter(&self, converter: &AstToIr<'a>) {
        self.temp_var_counter.set(converter.temp_var_counter());
        self.disposable_env_counter
            .set(converter.disposable_env_counter());
        self.dynamic_import_promise_counter
            .set(converter.dynamic_import_promise_counter());
        let generated = converter.take_generated_disposable_env_names();
        if !generated.is_empty() {
            let mut blocked = self.blocked_disposable_env_names.borrow_mut();
            let mut all_generated = self.generated_disposable_env_names.borrow_mut();
            for name in generated {
                blocked.insert(name.clone());
                all_generated.push(name);
            }
        }
        self.extra_hoisted_temps
            .borrow_mut()
            .extend(converter.take_hoisted_temps());
    }

    /// Convert an AST statement to IR (avoids `ASTRef` when possible)
    fn convert_statement(&self, idx: NodeIndex) -> IRNode {
        let converter = self.make_converter();
        let result = converter.convert_statement(idx);
        self.collect_from_converter(&converter);
        result
    }

    /// Convert an AST statement to IR with `this` captured as `_this`.
    /// Used in derived constructors after `super()` where `this` → `_this`.
    fn convert_statement_this_captured(&self, idx: NodeIndex) -> IRNode {
        let converter = self.make_converter().with_this_captured(true);
        let mut result = converter.convert_statement(idx);
        Self::rewrite_bare_constructor_returns_to_this(&mut result);
        self.collect_from_converter(&converter);
        result
    }

    /// Convert an AST statement to IR with `this` captured as `_this`, without
    /// changing bare constructor returns. Used for invalid-but-emitted pre-super
    /// statements in derived constructors.
    fn convert_statement_pre_super_this_captured(&self, idx: NodeIndex) -> IRNode {
        let converter = self.make_converter().with_this_captured(true);
        let result = converter.convert_statement(idx);
        self.collect_from_converter(&converter);
        result
    }

    fn rewrite_bare_constructor_returns_to_this(node: &mut IRNode) {
        if matches!(
            node,
            IRNode::FunctionExpr { .. }
                | IRNode::FunctionDecl { .. }
                | IRNode::ES5ClassIIFE { .. }
                | IRNode::ES5ClassAssignment { .. }
                | IRNode::StaticBlockIIFE { .. }
                | IRNode::AwaiterCall { .. }
                | IRNode::GeneratorBody { .. }
        ) {
            return;
        }

        match node {
            IRNode::ReturnStatement(expr @ None) => {
                *expr = Some(Box::new(IRNode::id("_this")));
            }
            IRNode::IfStatement {
                then_branch,
                else_branch,
                ..
            } => {
                Self::rewrite_bare_constructor_returns_to_this(then_branch);
                if let Some(else_branch) = else_branch {
                    Self::rewrite_bare_constructor_returns_to_this(else_branch);
                }
            }
            IRNode::Block(statements) | IRNode::Sequence(statements) => {
                for statement in statements {
                    Self::rewrite_bare_constructor_returns_to_this(statement);
                }
            }
            IRNode::SwitchStatement { cases, .. } => {
                for case in cases {
                    for statement in &mut case.statements {
                        Self::rewrite_bare_constructor_returns_to_this(statement);
                    }
                }
            }
            IRNode::ForStatement { body, .. }
            | IRNode::ForInOfStatement { body, .. }
            | IRNode::WhileStatement { body, .. }
            | IRNode::DoWhileStatement { body, .. }
            | IRNode::LabeledStatement {
                statement: body, ..
            } => {
                Self::rewrite_bare_constructor_returns_to_this(body);
            }
            IRNode::TryStatement {
                try_block,
                catch_clause,
                finally_block,
            } => {
                Self::rewrite_bare_constructor_returns_to_this(try_block);
                if let Some(catch_clause) = catch_clause {
                    for statement in &mut catch_clause.body {
                        Self::rewrite_bare_constructor_returns_to_this(statement);
                    }
                }
                if let Some(finally_block) = finally_block {
                    Self::rewrite_bare_constructor_returns_to_this(finally_block);
                }
            }
            _ => {}
        }
    }

    /// Convert an AST expression to IR (avoids `ASTRef` when possible)
    fn convert_expression(&self, idx: NodeIndex) -> IRNode {
        let converter = self.make_converter();
        let result = converter.convert_expression(idx);
        self.collect_from_converter(&converter);
        result
    }

    fn convert_expression_this_captured(&self, idx: NodeIndex) -> IRNode {
        let converter = self.make_converter().with_this_captured(true);
        let result = converter.convert_expression(idx);
        self.collect_from_converter(&converter);
        result
    }

    fn convert_expression_with_lexical_this_capture(&self, idx: NodeIndex) -> IRNode {
        let converter = self
            .make_converter()
            .with_lexical_this_capture_alias(Some("_this".to_string()));
        let result = converter.convert_expression(idx);
        self.collect_from_converter(&converter);
        result
    }

    fn convert_expression_with_context(
        &self,
        idx: NodeIndex,
        is_static: bool,
        class_alias: Option<&str>,
        lexical_this_capture_alias: Option<&str>,
    ) -> IRNode {
        let mut converter = self.make_converter();
        if is_static {
            converter = converter.with_static(true);
        }
        if let Some(alias) = class_alias {
            converter = converter.with_class_alias(Some(alias.to_string()));
        }
        if let Some(alias) = lexical_this_capture_alias {
            converter = converter.with_lexical_this_capture_alias(Some(alias.to_string()));
        }
        if let Some(alias) = self.class_self_reference_alias.as_ref() {
            converter =
                converter.with_identifier_substitution(self.class_name.clone(), alias.clone());
        }
        let result = converter.convert_expression(idx);
        self.collect_from_converter(&converter);
        result
    }

    /// Convert an AST expression to IR in static context
    fn convert_expression_static(&self, idx: NodeIndex) -> IRNode {
        let converter = self.make_converter().with_static(true);
        let result = converter.convert_expression(idx);
        self.collect_from_converter(&converter);
        result
    }

    /// Convert a computed-property-name expression that is evaluated inside an
    /// enclosing *instance* member body. A `super` reference here binds to the
    /// outer class's prototype home, so super access lowers in instance context
    /// (`<super>.prototype.m.call(this)`) using the inherited outer super name,
    /// instead of the default class-definition static context.
    fn convert_computed_name_expression_instance_super(
        &self,
        idx: NodeIndex,
        outer_super_name: &str,
    ) -> IRNode {
        let converter = self
            .make_converter()
            .with_super(true)
            .with_super_name(outer_super_name.to_string())
            .with_static(false);
        let result = converter.convert_expression(idx);
        self.collect_from_converter(&converter);
        result
    }

    /// Convert an AST expression to IR in static context with class alias for `this` substitution
    fn convert_expression_static_with_class_alias(
        &self,
        idx: NodeIndex,
        class_alias: &str,
    ) -> IRNode {
        if self
            .arena
            .get(idx)
            .and_then(|node| self.arena.get_function(node))
            .is_some_and(|function| function.is_async && function.equals_greater_than_token)
        {
            return IRNode::ASTRefWithGeneratorThis {
                node: idx,
                generator_this: class_alias.to_string().into(),
            };
        }

        let converter = self
            .make_converter()
            .with_static(true)
            .with_class_alias(Some(class_alias.to_string()));
        let result = converter.convert_expression(idx);
        self.collect_from_converter(&converter);
        result
    }

    /// Convert an AST expression to IR in static context with a raw `this` substitution.
    fn convert_expression_static_with_raw_this_substitution(
        &self,
        idx: NodeIndex,
        replacement: &str,
    ) -> IRNode {
        let converter = self
            .make_converter()
            .with_static(true)
            .with_raw_this_substitution(Some(replacement.to_string()));
        let result = converter.convert_expression(idx);
        self.collect_from_converter(&converter);
        result
    }

    /// Convert a static initializer for a legacy-decorated self-referencing class.
    ///
    /// TSC rewrites class-name references in static initializers to the decorator
    /// self alias (`C_1`) while still lowering static `this` to `void 0`.
    fn convert_expression_static_with_decorator_self_alias(
        &self,
        idx: NodeIndex,
        alias: &str,
    ) -> IRNode {
        let converter = self
            .make_converter()
            .with_static(true)
            .with_raw_this_substitution(Some("(void 0)".to_string()))
            .with_identifier_substitution(self.class_name.clone(), alias.to_string());
        let result = converter.convert_expression(idx);
        self.collect_from_converter(&converter);
        result
    }

    fn convert_computed_property_expression(&self, idx: NodeIndex, is_static: bool) -> IRNode {
        if let Some(raw) = self.raw_string_literal_source(idx) {
            return IRNode::Raw(raw.into());
        }

        if let Some(alias) = self.inherited_computed_name_this.as_ref() {
            return self.convert_expression_static_with_raw_this_substitution(idx, alias);
        }

        if is_static {
            self.convert_expression_static(idx)
        } else {
            self.convert_expression(idx)
        }
    }

    fn raw_string_literal_source(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        if node.kind != SyntaxKind::StringLiteral as u16 {
            return None;
        }
        let literal_text = self.arena.get_literal(node).map(|lit| lit.text.as_str())?;

        let source_text = self.source_text?;
        let bytes = source_text.as_bytes();
        let start = (node.pos as usize).min(bytes.len());
        let end = (node.end as usize).min(bytes.len());
        if start >= end {
            return self.find_raw_string_literal_near(node, literal_text);
        }

        let read_from_quote = |i: usize| -> Option<String> {
            let quote = bytes[i];
            let mut j = i + 1;
            while j < bytes.len() {
                if bytes[j] == b'\\' {
                    j = j.saturating_add(2);
                    continue;
                }
                if bytes[j] == quote {
                    return Some(source_text[i..=j].to_string());
                }
                if bytes[j] == b'\n' || bytes[j] == b'\r' {
                    break;
                }
                j += 1;
            }

            None
        };

        let mut i = start;
        while i < end {
            match bytes[i] {
                b'\'' | b'"' => break,
                b' ' | b'\t' | b'\r' | b'\n' | b'[' => i += 1,
                _ => {
                    let scan_start = start.saturating_sub(4);
                    for q in (scan_start..start).rev() {
                        if matches!(bytes[q], b'\'' | b'"') {
                            return read_from_quote(q);
                        }
                        if !matches!(bytes[q], b' ' | b'\t' | b'\r' | b'\n' | b'[') {
                            break;
                        }
                    }
                    return self.find_raw_string_literal_near(node, literal_text);
                }
            }
        }

        if i >= end {
            return self.find_raw_string_literal_near(node, literal_text);
        }

        read_from_quote(i).or_else(|| self.find_raw_string_literal_near(node, literal_text))
    }

    fn find_raw_string_literal_near(&self, node: &Node, literal_text: &str) -> Option<String> {
        let source_text = self.source_text?;
        let bytes = source_text.as_bytes();
        if bytes.is_empty() {
            return None;
        }

        let approx_start = (node.pos as usize).min(bytes.len());
        let approx_end = (node.end as usize).min(bytes.len());
        let start = approx_start.saturating_sub(128);
        let end = approx_end.saturating_add(128).min(bytes.len());

        let mut i = start;
        while i < end {
            let quote = bytes[i];
            if !matches!(quote, b'\'' | b'"') {
                i += 1;
                continue;
            }

            let mut j = i + 1;
            let mut escaped = false;
            while j < end {
                let b = bytes[j];
                if escaped {
                    escaped = false;
                    j += 1;
                    continue;
                }
                if b == b'\\' {
                    escaped = true;
                    j += 1;
                    continue;
                }
                if b == quote {
                    let raw = &source_text[i..=j];
                    let inner = &raw[1..raw.len() - 1];
                    if inner == literal_text {
                        return Some(raw.to_string());
                    }
                    break;
                }
                if b == b'\n' || b == b'\r' {
                    break;
                }
                j += 1;
            }

            i += 1;
        }

        None
    }

    fn convert_block_body_with_alias_impl(
        &self,
        block_idx: NodeIndex,
        class_alias: Option<String>,
        is_static: bool,
        emit_await_as_yield: bool,
    ) -> Vec<IRNode> {
        self.convert_block_body_with_alias_and_this_capture_impl(
            block_idx,
            class_alias,
            None,
            is_static,
            emit_await_as_yield,
        )
    }

    fn convert_block_body_with_this_capture_alias(
        &self,
        block_idx: NodeIndex,
        lexical_this_capture_alias: Option<String>,
    ) -> Vec<IRNode> {
        self.convert_block_body_with_alias_and_this_capture_impl(
            block_idx,
            None,
            lexical_this_capture_alias,
            false,
            false,
        )
    }

    fn convert_block_body_static_with_this_capture_alias(
        &self,
        block_idx: NodeIndex,
        lexical_this_capture_alias: Option<String>,
    ) -> Vec<IRNode> {
        // Static methods/accessors: is_static=true but await-recovery IIFE applies
        // only to CLASS_STATIC_BLOCK_DECLARATION, not ordinary static members.
        self.convert_block_body_with_alias_and_this_capture_impl(
            block_idx,
            None,
            lexical_this_capture_alias,
            true,
            false,
        )
    }

    fn convert_block_body_with_alias_and_this_capture_impl(
        &self,
        block_idx: NodeIndex,
        class_alias: Option<String>,
        lexical_this_capture_alias: Option<String>,
        is_static: bool,
        emit_await_as_yield: bool,
    ) -> Vec<IRNode> {
        // Snapshot hoisted temps before converting statements
        let hoisted_before = self.extra_hoisted_temps.borrow().len();
        let saved_temp_counter = self.temp_var_counter.get();
        self.temp_var_counter.set(0);

        let mut stmts = if let Some(block_node) = self.arena.get(block_idx)
            && let Some(block) = self.arena.get_block(block_node)
        {
            let trailing_comment_limit =
                self.body_closing_brace_pos(block_idx).map(|pos| pos as u32);
            if self.block_has_using_declarations(&block.statements) {
                self.convert_block_body_using_region(
                    block,
                    is_static,
                    emit_await_as_yield,
                    class_alias.as_deref(),
                    lexical_this_capture_alias.as_deref(),
                    trailing_comment_limit,
                )
            } else {
                let mut converted = Vec::new();
                let mut prev_stmt_end = block_node.pos;
                for &stmt_idx in &block.statements.nodes {
                    if let Some(stmt_node) = self.arena.get(stmt_idx) {
                        self.emit_leading_statement_comments(
                            &mut converted,
                            prev_stmt_end,
                            stmt_node.pos,
                        );
                        prev_stmt_end = stmt_node.end;
                    }
                    converted.push(self.convert_statement_with_context(
                        stmt_idx,
                        is_static,
                        emit_await_as_yield,
                        class_alias.as_deref(),
                        lexical_this_capture_alias.as_deref(),
                        trailing_comment_limit,
                    ));
                }
                converted
            }
        } else {
            vec![]
        };
        self.temp_var_counter.set(saved_temp_counter);

        // Collect any hoisted temps that were created during statement conversion.
        // These belong in THIS block's scope (e.g., method body), not the class IIFE.
        let hoisted_after = self.extra_hoisted_temps.borrow().len();
        if hoisted_after > hoisted_before {
            let block_temps: Vec<String> = self
                .extra_hoisted_temps
                .borrow_mut()
                .drain(hoisted_before..)
                .collect();
            let var_decls: Vec<IRNode> = block_temps
                .into_iter()
                .map(|name| IRNode::VarDecl {
                    name: name.into(),
                    initializer: None,
                })
                .collect();
            stmts.insert(0, IRNode::VarDeclList(var_decls));
        }

        // Non-static alias contexts capture the current receiver. Static blocks
        // already use the class alias from the surrounding class IIFE.
        if let Some(alias) = class_alias
            && !is_static
        {
            stmts.insert(
                0,
                IRNode::VarDecl {
                    name: alias.into(),
                    initializer: Some(Box::new(IRNode::This { captured: false })),
                },
            );
        }

        stmts
    }

    fn convert_block_body_using_region(
        &self,
        block: &tsz_parser::parser::node::BlockData,
        is_static: bool,
        emit_await_as_yield: bool,
        class_alias: Option<&str>,
        lexical_this_capture_alias: Option<&str>,
        trailing_comment_limit: Option<u32>,
    ) -> Vec<IRNode> {
        let (env_name, error_name) = self.next_constructor_disposable_env_names();
        let mut try_body = Vec::new();

        for &stmt_idx in &block.statements.nodes {
            if let Some(stmt_node) = self.arena.get(stmt_idx)
                && let Some(comment) = self.extract_leading_comment(stmt_node)
            {
                try_body.push(IRNode::Raw(comment.into()));
            }

            if let Some(ir) = self.convert_using_variable_statement_for_env_with_context(
                stmt_idx,
                &env_name,
                is_static,
                class_alias,
                lexical_this_capture_alias,
            ) {
                try_body.push(ir);
            } else {
                try_body.push(self.convert_statement_with_context(
                    stmt_idx,
                    is_static,
                    emit_await_as_yield,
                    class_alias,
                    lexical_this_capture_alias,
                    trailing_comment_limit,
                ));
            }
        }

        vec![
            IRNode::var_decl(
                env_name.clone(),
                Some(Self::disposable_env_initializer_ir()),
            ),
            Self::using_try_statement_ir(env_name, error_name, try_body),
        ]
    }

    fn this_capture_alias_for_body(
        &self,
        body_idx: NodeIndex,
        params: Option<&NodeList>,
    ) -> Option<String> {
        if !self.constructor_needs_this_capture(body_idx) {
            return None;
        }

        let mut suffix = 0usize;
        loop {
            let candidate = if suffix == 0 {
                "_this".to_string()
            } else {
                format!("_this_{suffix}")
            };
            if !self.body_or_params_has_binding_name(body_idx, params, &candidate) {
                return Some(candidate);
            }
            suffix += 1;
        }
    }

    fn body_or_params_has_binding_name(
        &self,
        body_idx: NodeIndex,
        params: Option<&NodeList>,
        name: &str,
    ) -> bool {
        params.is_some_and(|params| self.node_list_has_binding_name(params, name))
            || self.node_has_binding_name(body_idx, name)
    }

    fn node_list_has_binding_name(&self, nodes: &NodeList, name: &str) -> bool {
        nodes
            .nodes
            .iter()
            .any(|&idx| self.node_has_binding_name(idx, name))
    }

    fn node_has_binding_name(&self, idx: NodeIndex, name: &str) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        if node.kind == SyntaxKind::Identifier as u16
            && self.arena.get_identifier_text(idx) == Some(name)
        {
            return true;
        }

        if let Some(param) = self.arena.get_parameter(node)
            && self.node_has_binding_name(param.name, name)
        {
            return true;
        }
        if let Some(decl) = self.arena.get_variable_declaration(node)
            && self.node_has_binding_name(decl.name, name)
        {
            return true;
        }
        if let Some(function) = self.arena.get_function(node)
            && self.node_has_binding_name(function.name, name)
        {
            return true;
        }
        if let Some(class) = self.arena.get_class(node)
            && self.node_has_binding_name(class.name, name)
        {
            return true;
        }
        if let Some(pattern) = self.arena.get_binding_pattern(node) {
            for &element_idx in &pattern.elements.nodes {
                let Some(element_node) = self.arena.get(element_idx) else {
                    continue;
                };
                if let Some(element) = self.arena.get_binding_element(element_node)
                    && self.node_has_binding_name(element.name, name)
                {
                    return true;
                }
            }
        }

        self.arena
            .get_children(idx)
            .into_iter()
            .any(|child| self.node_has_binding_name(child, name))
    }

    /// Transform a class declaration to IR
    pub fn transform_class_to_ir(&mut self, class_idx: NodeIndex) -> Option<IRNode> {
        self.transform_class_to_ir_with_name(class_idx, None)
    }
}
