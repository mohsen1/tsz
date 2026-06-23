//! Construction and configuration helpers for the IR printer.

use super::{
    AstPrinter, IRNode, IRParam, IRPrinter, NodeArena, PrinterOptions, TransformContext,
    TslibHelperNaming,
};

impl<'a> IRPrinter<'a> {
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
            tslib_helpers: TslibHelperNaming::default(),
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
            mappings: Vec::new(),
            source_index: 0,
            capture_mappings: false,
            suppress_ast_ref_mapping_at_output_len: None,
            plain_generator_wrapper: false,
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
            tslib_helpers: TslibHelperNaming::default(),
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
            mappings: Vec::new(),
            source_index: 0,
            capture_mappings: false,
            suppress_ast_ref_mapping_at_output_len: None,
            plain_generator_wrapper: false,
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
            tslib_helpers: TslibHelperNaming::default(),
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
            mappings: Vec::new(),
            source_index: 0,
            capture_mappings: false,
            suppress_ast_ref_mapping_at_output_len: None,
            plain_generator_wrapper: false,
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
        self.tslib_helpers.set_prefix(enable);
    }

    pub fn set_tslib_import_binding(&mut self, binding: String) {
        self.tslib_helpers.set_binding(binding);
    }

    /// Set per-file helper import renames (e.g. `__awaiter` -> `__awaiter_1`)
    /// so helper references printed from IR match the import-site aliases.
    pub fn set_helper_import_aliases(&mut self, aliases: rustc_hash::FxHashMap<String, String>) {
        self.tslib_helpers.set_aliases(aliases);
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

    pub(super) fn merge_ast_printer_block_scope_reserved_names(
        &mut self,
        printer: &AstPrinter<'a>,
    ) {
        self.block_scope_reserved_names
            .extend(printer.block_scope_reserved_names());
        self.block_scope_reserved_names.sort();
        self.block_scope_reserved_names.dedup();
    }

    pub(super) fn configure_ast_printer_namespace(&self, printer: &mut AstPrinter<'a>) {
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
    pub(super) fn build_nested_ast_printer(&self, arena: &'a NodeArena) -> AstPrinter<'a> {
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

    /// Write a runtime helper name, prefixing with `tslib_1.` when `tslib_prefix` is
    /// active, or substituting the import-site alias (e.g. `__awaiter_1`) when ESM
    /// importHelpers renamed the helper. Mirrors `Printer::write_helper`.
    pub(super) fn write_helper(&mut self, name: &str) {
        self.tslib_helpers.write_into(&mut self.output, name);
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

    /// Mark this printer as emitting a down-leveled plain `function*` generator
    /// wrapper, so its single-statement `return __generator(...)` body is
    /// emitted as a single-line block to match `tsc`. See the field doc on
    /// [`IRPrinter::plain_generator_wrapper`].
    pub const fn set_plain_generator_wrapper(&mut self, value: bool) {
        self.plain_generator_wrapper = value;
    }

    /// Emit a plain `function*` wrapper body as a single-line block when it is
    /// exactly the synthesized `[ <hoisted var decls>*, GeneratorBody ]` body
    /// (no default-parameter checks or rest prologue). `tsc` hugs the braces
    /// around those statements - `function g() { var x; return __generator(...); }`
    /// - even though the generator state machine inside spans multiple lines.
    ///
    /// Returns `true` when it emitted the body (caller is done), `false` when
    /// the caller should fall back to the normal multi-line body emission (e.g.
    /// a default-parameter prologue makes `tsc` emit the wrapper multi-line, and
    /// a nested-function / `new.target` / `arguments` capture is not the shape
    /// `tsc` hugs). Only fires when [`Self::plain_generator_wrapper`] is set, so
    /// the shape-identical async-generator inner wrapper is unaffected.
    pub(super) fn try_emit_plain_generator_wrapper(
        &mut self,
        parameters: &[IRParam],
        body: &[IRNode],
    ) -> bool {
        if !self.plain_generator_wrapper {
            return false;
        }
        // A default-parameter check (`if (a === void 0) ...`) or an ES5 rest
        // prologue makes the wrapper body multi-statement, which `tsc` emits
        // multi-line, so only the parameter-prologue-free wrapper hugs.
        if parameters
            .iter()
            .any(|p| p.default_value.is_some() || p.rest)
        {
            return false;
        }
        // The body must be the synthesized hoisted-var declarations followed by
        // the lone `return __generator(...)` (`GeneratorBody`). Any other
        // statement shape is not the simple wrapper `tsc` hugs.
        let Some((last, leading)) = body.split_last() else {
            return false;
        };
        if !matches!(last, IRNode::GeneratorBody { .. }) {
            return false;
        }
        if !leading
            .iter()
            .all(|n| matches!(n, IRNode::VarDecl { .. } | IRNode::VarDeclList(_)))
        {
            return false;
        }
        let previous_generator_state_name = self.generator_state_name;
        if let Some(name) = Self::generator_state_name_for_function_body(body) {
            self.generator_state_name = name;
        }
        self.write("{ ");
        for stmt in leading {
            self.emit_node(stmt);
            self.write(" ");
        }
        self.emit_node(last);
        self.write(" }");
        self.generator_state_name = previous_generator_state_name;
        true
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

    pub(super) fn make_ast_printer_options(&self) -> PrinterOptions {
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
}
