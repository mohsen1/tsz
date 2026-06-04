impl<'a> IRPrinter<'a> {
    /// Check if a generator switch case should stay on the `case N:` line.
    fn is_generator_inline_case_statement(node: &IRNode) -> bool {
        match node {
            IRNode::ThrowStatement(expr) => Self::is_generator_inline_throw_expression(expr),
            IRNode::ReturnStatement(Some(expr)) => {
                matches!(expr.as_ref(), IRNode::GeneratorOp { .. })
            }
            _ => false,
        }
    }

    fn is_generator_break_return(node: &IRNode) -> bool {
        matches!(
            node,
            IRNode::ReturnStatement(Some(expr))
                if matches!(expr.as_ref(), IRNode::GeneratorOp { opcode: 3, .. })
        )
    }

    fn is_generator_sent_assignment(node: &IRNode) -> bool {
        matches!(
            node,
            IRNode::ExpressionStatement(expr)
                if matches!(
                    expr.as_ref(),
                    IRNode::BinaryExpr { right, .. } if matches!(right.as_ref(), IRNode::GeneratorSent)
                )
        )
    }

    const fn is_generator_inline_throw_expression(expr: &IRNode) -> bool {
        matches!(
            expr,
            IRNode::Identifier(_) | IRNode::CallExpr { .. } | IRNode::GeneratorSent
        )
    }

    pub(crate) fn emit_es5_class_expression(
        &mut self,
        name: &str,
        base_class: Option<&IRNode>,
        super_param: Option<&str>,
        body: &[IRNode],
    ) {
        if !self.remove_comments {
            self.write("/** @class */ ");
        }
        self.write("(function (");
        if base_class.is_some() {
            self.write(super_param.unwrap_or("_super"));
        }
        self.write(") {");
        self.write_line();
        self.increase_indent();

        let prev_iife_name = self.current_class_iife_name.replace(name.to_string());
        for stmt in body {
            self.write_indent();
            self.emit_node(stmt);
            self.write_line();
        }
        self.current_class_iife_name = prev_iife_name;

        self.decrease_indent();
        self.write_indent();
        self.write("}(");
        if let Some(base) = base_class {
            self.emit_node(base);
        }
        self.write("))");
    }

    fn emit_static_block_iife_expression(&mut self, statements: &[IRNode]) {
        self.write("(function () {");
        if statements.is_empty() {
            self.write(" })()");
            return;
        }

        self.write_line();
        self.increase_indent();
        for stmt in statements {
            self.write_indent();
            self.emit_node(stmt);
            self.write_line();
        }
        self.decrease_indent();
        self.write_indent();
        self.write("})()");
    }

    fn extract_trailing_comment_from_function(&self, function: &IRNode) -> Option<String> {
        let source_text = self.source_text?;
        let (body_start, body_end) = match function {
            IRNode::FunctionExpr {
                body_source_range: Some((body_start, body_end)),
                ..
            }
            | IRNode::FunctionDecl {
                body_source_range: Some((body_start, body_end)),
                ..
            } => (*body_start, *body_end),
            _ => return None,
        };
        let bytes = source_text.as_bytes();
        let start = body_start as usize;
        let end = (body_end as usize).min(bytes.len());
        if start >= end {
            return None;
        }
        let open_brace = bytes[start..end].iter().position(|&byte| byte == b'{')?;
        let mut depth = 1usize;
        let mut close_brace = None;
        for offset in open_brace + 1..end - start {
            match bytes[start + offset] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        close_brace = Some(start + offset);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close_brace = close_brace?;
        let comments = crate::emitter::get_trailing_comment_ranges(source_text, close_brace + 1);
        if comments.is_empty() {
            return None;
        }

        Some(
            comments
                .iter()
                .map(|comment| source_text[comment.pos as usize..comment.end as usize].to_string())
                .collect::<Vec<_>>()
                .join(" "),
        )
    }

    fn should_indent_sequence_child(node: &IRNode) -> bool {
        match node {
            IRNode::NamespaceIIFE {
                skip_sequence_indent,
                ..
            } => !skip_sequence_indent,
            _ => true,
        }
    }

    fn generator_state_name_for_function_body(body: &[IRNode]) -> Option<&'static str> {
        if !body
            .iter()
            .any(|node| matches!(node, IRNode::GeneratorBody { .. }))
        {
            return None;
        }

        let mut hoisted_vars = Vec::new();
        for stmt in body {
            match stmt {
                IRNode::VarDeclList(decls) => {
                    for decl in decls {
                        if let IRNode::VarDecl { name, .. } = decl {
                            hoisted_vars.push(name.as_ref());
                        }
                    }
                }
                IRNode::VarDecl { name, .. } => hoisted_vars.push(name.as_ref()),
                IRNode::GeneratorBody { .. } => break,
                _ => {}
            }
        }

        (!hoisted_vars.is_empty()).then(|| Self::generator_state_name_for_hoisted(&hoisted_vars))
    }

    fn is_noop_statement(node: &IRNode) -> bool {
        match node {
            IRNode::Sequence(nodes) if nodes.is_empty() => true,
            IRNode::EmptyStatement => true,
            IRNode::Raw(text) => text.trim().is_empty(),
            _ => false,
        }
    }

    fn write_embedded_output(&mut self, output: &str) {
        let mut lines = output.split('\n');
        if let Some(first) = lines.next() {
            self.write(first);
        }
        for line in lines {
            self.write_line();
            if !line.is_empty() {
                self.write_indent();
                self.write(line);
            }
        }
    }

    /// Create a new IR printer
    pub fn new() -> Self {
        Self {
            output: String::with_capacity(4096),
            indent_level: 0,
            indent_str: "    ",
            arena: None,
            source_text: None,
            transforms: None,
            suppress_function_trailing_extraction: false,
            last_emit_ended_with_line_comment: false,
            ast_arrow_comment_defer_end: None,
            current_class_iife_name: None,
            force_iife_multiline_empty: false,
            in_namespace_iife_body: false,
            target_es5: false,
            remove_comments: false,
            tslib_prefix: false,
            tslib_import_binding: "tslib_1".to_string(),
            commonjs_import_substitutions: rustc_hash::FxHashMap::default(),
            system_import_meta: false,
            base_printer_options: None,
            generator_state_name: "_a",
            generator_this_arg: "this".to_string(),
            outer_reserved_for_generator_state: Vec::new(),
            namespace_ast_name: None,
            namespace_ast_exported_names: rustc_hash::FxHashSet::default(),
            block_scope_shadowed_names: Vec::new(),
            block_scope_reserved_names: Vec::new(),
            pending_commonjs_class_export_name: None,
        }
    }

    /// Create an IR printer with an arena for `ASTRef` handling
    pub fn with_arena(arena: &'a NodeArena) -> Self {
        Self {
            output: String::with_capacity(4096),
            indent_level: 0,
            indent_str: "    ",
            arena: Some(arena),
            source_text: None,
            transforms: None,
            suppress_function_trailing_extraction: false,
            last_emit_ended_with_line_comment: false,
            ast_arrow_comment_defer_end: None,
            current_class_iife_name: None,
            force_iife_multiline_empty: false,
            in_namespace_iife_body: false,
            target_es5: false,
            remove_comments: false,
            tslib_prefix: false,
            tslib_import_binding: "tslib_1".to_string(),
            commonjs_import_substitutions: rustc_hash::FxHashMap::default(),
            system_import_meta: false,
            base_printer_options: None,
            generator_state_name: "_a",
            generator_this_arg: "this".to_string(),
            outer_reserved_for_generator_state: Vec::new(),
            namespace_ast_name: None,
            namespace_ast_exported_names: rustc_hash::FxHashSet::default(),
            block_scope_shadowed_names: Vec::new(),
            block_scope_reserved_names: Vec::new(),
            pending_commonjs_class_export_name: None,
        }
    }

    /// Create an IR printer with both arena and source text for `ASTRef` emission
    pub fn with_arena_and_source(arena: &'a NodeArena, source_text: &'a str) -> Self {
        Self {
            output: String::with_capacity(4096),
            indent_level: 0,
            indent_str: "    ",
            arena: Some(arena),
            source_text: Some(source_text),
            transforms: None,
            suppress_function_trailing_extraction: false,
            last_emit_ended_with_line_comment: false,
            ast_arrow_comment_defer_end: None,
            current_class_iife_name: None,
            force_iife_multiline_empty: false,
            in_namespace_iife_body: false,
            target_es5: false,
            remove_comments: false,
            tslib_prefix: false,
            tslib_import_binding: "tslib_1".to_string(),
            commonjs_import_substitutions: rustc_hash::FxHashMap::default(),
            system_import_meta: false,
            base_printer_options: None,
            generator_state_name: "_a",
            generator_this_arg: "this".to_string(),
            outer_reserved_for_generator_state: Vec::new(),
            namespace_ast_name: None,
            namespace_ast_exported_names: rustc_hash::FxHashSet::default(),
            block_scope_shadowed_names: Vec::new(),
            block_scope_reserved_names: Vec::new(),
            pending_commonjs_class_export_name: None,
        }
    }

    pub fn set_pending_commonjs_class_export_name(&mut self, name: Option<String>) {
        self.pending_commonjs_class_export_name = name.map(|name| (name.clone(), vec![name]));
    }

    pub fn set_pending_commonjs_class_export_bindings(
        &mut self,
        local_name: String,
        export_names: Vec<String>,
    ) {
        self.pending_commonjs_class_export_name = Some((local_name, export_names));
    }

    pub(super) const fn take_pending_commonjs_class_export_name(
        &mut self,
    ) -> Option<(String, Vec<String>)> {
        self.pending_commonjs_class_export_name.take()
    }

    pub fn set_transforms(&mut self, transforms: TransformContext) {
        self.transforms = Some(transforms);
    }

    /// Enable `tslib_1.` prefix for runtime helper calls (importHelpers + CJS).
    pub const fn set_tslib_prefix(&mut self, enable: bool) {
        self.tslib_prefix = enable;
    }

    pub fn set_tslib_import_binding(&mut self, binding: String) {
        self.tslib_import_binding = binding;
    }

    pub fn set_commonjs_import_substitutions(
        &mut self,
        subs: rustc_hash::FxHashMap<String, String>,
    ) {
        self.commonjs_import_substitutions = subs;
    }

    pub const fn set_system_import_meta(&mut self, enabled: bool) {
        self.system_import_meta = enabled;
    }

    pub fn set_namespace_ast_qualification(
        &mut self,
        namespace: String,
        names: std::collections::HashSet<String>,
    ) {
        self.namespace_ast_name = Some(namespace);
        self.namespace_ast_exported_names = names.into_iter().collect();
    }

    pub fn set_block_scope_shadowed_names(&mut self, names: Vec<String>) {
        self.block_scope_shadowed_names = names;
    }

    pub fn set_block_scope_reserved_names(&mut self, names: Vec<String>) {
        self.block_scope_reserved_names = names;
    }

    pub fn block_scope_reserved_names(&self) -> Vec<String> {
        let mut names = self.block_scope_reserved_names.clone();
        names.sort();
        names.dedup();
        names
    }

    fn merge_ast_printer_block_scope_reserved_names(&mut self, printer: &AstPrinter<'a>) {
        self.block_scope_reserved_names
            .extend(printer.block_scope_reserved_names());
        self.block_scope_reserved_names.sort();
        self.block_scope_reserved_names.dedup();
    }

    fn configure_ast_printer_namespace(&self, printer: &mut AstPrinter<'a>) {
        if let Some(namespace) = self.namespace_ast_name.clone() {
            printer.in_namespace_iife = true;
            printer.current_namespace_name = Some(namespace);
            printer.namespace_exported_names = self.namespace_ast_exported_names.clone();
        }
    }

    /// Build a nested `AstPrinter` that inherits this IR printer's transforms,
    /// printer options, and source text. Callers that need namespace
    /// qualification on the embedded output must invoke
    /// `configure_ast_printer_namespace` themselves; keeping it opt-in avoids
    /// silently changing emission for arms (e.g. `ASTRefWithGeneratorThis`)
    /// that historically ran without namespace context.
    fn build_nested_ast_printer(&self, arena: &'a NodeArena) -> AstPrinter<'a> {
        let transforms = self.transforms.clone().unwrap_or_default();
        let mut printer = AstPrinter::with_transforms_and_options(
            arena,
            transforms,
            self.make_ast_printer_options(),
        );
        if let Some(source_text) = self.source_text {
            printer.set_source_text(source_text);
        }
        printer.seed_function_scope_shadowed_names(&self.block_scope_shadowed_names);
        printer.seed_block_scope_reserved_names(&self.block_scope_reserved_names);
        printer
    }

    /// Write a runtime helper name, prefixing with `tslib_1.` when `tslib_prefix` is active.
    fn write_helper(&mut self, name: &str) {
        if self.tslib_prefix {
            self.output.push_str(&self.tslib_import_binding);
            self.output.push('.');
        }
        self.output.push_str(name);
    }

    /// Set the source text for `ASTRef` emission
    pub const fn set_source_text(&mut self, text: &'a str) {
        self.source_text = Some(text);
    }

    /// Set the indentation level
    pub const fn set_indent_level(&mut self, level: u32) {
        self.indent_level = level;
    }

    /// Mark this printer as targeting ES5 (disables `let`/`const` emission).
    pub const fn set_target_es5(&mut self, es5: bool) {
        self.target_es5 = es5;
    }

    pub const fn set_generator_state_name(&mut self, name: &'static str) {
        self.generator_state_name = name;
    }

    pub fn set_generator_this_arg(&mut self, arg: String) {
        self.generator_this_arg = arg;
    }

    /// Set names that must not be chosen as the `__generator` state variable.
    pub fn set_outer_reserved_for_generator_state(&mut self, names: Vec<String>) {
        self.outer_reserved_for_generator_state = names;
    }

    /// When true, suppress comment annotations like `/** @class */` in output.
    pub const fn set_remove_comments(&mut self, remove: bool) {
        self.remove_comments = remove;
    }

    pub fn set_base_printer_options(&mut self, options: PrinterOptions) {
        self.base_printer_options = Some(options);
    }

    fn make_ast_printer_options(&self) -> PrinterOptions {
        if let Some(ref base) = self.base_printer_options {
            let mut opts = base.clone();
            if self.target_es5 {
                opts.target = crate::emitter::ScriptTarget::ES5;
            }
            opts
        } else {
            PrinterOptions {
                target: if self.target_es5 {
                    crate::emitter::ScriptTarget::ES5
                } else {
                    PrinterOptions::default().target
                },
                ..PrinterOptions::default()
            }
        }
    }

    /// Get the output
    pub fn get_output(&self) -> &str {
        &self.output
    }

    /// Take the output
    pub fn take_output(self) -> String {
        self.output
    }

    /// Emit an IR node to a string
    pub fn emit(&mut self, node: &IRNode) -> &str {
        // For top-level Sequences, add newlines between statements
        if let IRNode::Sequence(nodes) = node {
            let mut i = 0;
            while i < nodes.len() {
                if i > 0 {
                    self.write_line();
                    if Self::should_indent_sequence_child(&nodes[i]) {
                        self.write_indent();
                    }
                }
                if i + 1 < nodes.len()
                    && let Some((enum_name, members, namespace)) =
                        Self::enum_with_matching_namespace_export(&nodes[i], &nodes[i + 1])
                {
                    self.emit_namespace_bound_enum_iife(enum_name, members, namespace);
                    i += 2;
                    continue;
                }
                let suppress_for_this_node = i + 1 < nodes.len()
                    && matches!(&nodes[i], IRNode::FunctionDecl { .. })
                    && matches!(&nodes[i + 1], IRNode::TrailingComment(_));
                let prev_suppress = self.suppress_function_trailing_extraction;
                self.suppress_function_trailing_extraction = suppress_for_this_node;
                self.emit_node(&nodes[i]);
                self.suppress_function_trailing_extraction = prev_suppress;
                i += 1;
            }
        } else {
            self.emit_node(node);
        }
        &self.output
    }

    /// Emit an IR node and return the output
    pub fn emit_to_string(node: &IRNode) -> String {
        let mut printer = Self::new();
        printer.emit(node);
        printer.output
    }

    /// Check whether a property access on `node` needs `..` instead of `.`.
    /// Plain decimal integer literals need `..` because `0.x` would be
    /// parsed as the float `0.` followed by identifier `x`.
    fn ir_node_needs_double_dot(node: &IRNode) -> bool {
        match node {
            IRNode::NumericLiteral(n) => {
                let num_text = n.trim();
                let is_prefixed = num_text.starts_with("0x")
                    || num_text.starts_with("0X")
                    || num_text.starts_with("0o")
                    || num_text.starts_with("0O")
                    || num_text.starts_with("0b")
                    || num_text.starts_with("0B");
                !is_prefixed
                    && !num_text.contains('.')
                    && !num_text.contains('e')
                    && !num_text.contains('E')
            }
            // Other expressions (including parenthesized) never need double-dot
            // because the closing paren already disambiguates: `(1).foo` is valid JS.
            _ => false,
        }
    }

    fn emit_sent_aware(&mut self, node: &IRNode) {
        if matches!(node, IRNode::GeneratorSent) {
            self.write("(");
            self.emit_node(node);
            self.write(")");
        } else {
            self.emit_node(node);
        }
    }
}
