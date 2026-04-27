use std::sync::Arc;

use rustc_hash::FxHashMap;
use wasm_bindgen::prelude::{JsValue, wasm_bindgen};

use crate::CheckerState;
use crate::ScriptTarget;
use crate::WasmTransformContext;
use crate::api::wasm::code_actions::{default_code_action_context, parse_code_action_context};
use crate::api::wasm::compiler_options::{CompilerOptions, parse_compiler_options_json};
use crate::binder::BinderState;
use crate::checker;
use crate::checker::context::LibContext;
use crate::common::ModuleKind;
use crate::context::emit::EmitContext;
use crate::context::transform::TransformContext;
use crate::emitter::{Printer, PrinterOptions};
use crate::lib_loader::LibFile;
use crate::lowering::LoweringPass;
use crate::lsp::diagnostics::convert_diagnostic;
use crate::lsp::position::{LineMap, Position, Range};
use crate::lsp::provider_macro::FullProviderOptions;
use crate::lsp::resolver::ScopeCache;
use crate::lsp::{
    CodeActionProvider, Completions, DocumentSymbolProvider, FindReferences, GoToDefinition,
    HoverProvider, RenameProvider, SemanticTokensProvider, SignatureHelpProvider,
};
use crate::parser;
use crate::parser::ParserState;
use tsz_solver::TypeInterner;

/// High-performance parser using Node architecture (16 bytes/node).
/// This is the optimized path for Phase 8 test suite evaluation.
#[wasm_bindgen]
pub struct Parser {
    parser: ParserState,
    source_file_idx: Option<parser::NodeIndex>,
    binder: Option<BinderState>,
    /// Local type interner for single-file checking.
    /// For multi-file compilation, use `MergedProgram.type_interner` instead.
    type_interner: TypeInterner,
    /// Line map for LSP position conversion (lazy initialized)
    line_map: Option<LineMap>,
    /// Persistent cache for type checking results across LSP queries.
    /// Invalidated when the file changes.
    type_cache: Option<checker::TypeCache>,
    /// Persistent cache for scope resolution across LSP queries.
    /// Invalidated when the file changes.
    scope_cache: ScopeCache,
    /// Pre-loaded lib files (parsed and bound) for global type resolution
    lib_files: Vec<Arc<LibFile>>,
    /// Compiler options for type checking
    compiler_options: CompilerOptions,
}

#[wasm_bindgen]
impl Parser {
    /// Create a new Parser for the given source file.
    #[wasm_bindgen(constructor)]
    pub fn new(file_name: String, source_text: String) -> Self {
        Self {
            parser: ParserState::new(file_name, source_text),
            source_file_idx: None,
            binder: None,
            type_interner: TypeInterner::new(),
            line_map: None,
            type_cache: None,
            scope_cache: ScopeCache::default(),
            lib_files: Vec::new(),
            compiler_options: CompilerOptions::default(),
        }
    }

    /// Set compiler options from JSON.
    ///
    /// # Arguments
    /// * `options_json` - JSON string containing compiler options
    ///
    /// # Example
    /// ```javascript
    /// const parser = new Parser("file.ts", "const x = 1;");
    /// parser.setCompilerOptions(JSON.stringify({
    ///   strict: true,
    ///   noImplicitAny: true,
    ///   strictNullChecks: true
    /// }));
    /// ```
    #[wasm_bindgen(js_name = setCompilerOptions)]
    pub fn set_compiler_options(&mut self, options_json: &str) -> Result<(), JsValue> {
        let options = parse_compiler_options_json(options_json)?;
        self.compiler_options = options;
        // Invalidate type cache when compiler options change
        self.type_cache = None;
        Ok(())
    }

    /// Add a lib file (e.g., lib.es5.d.ts) for global type resolution.
    /// The lib file will be parsed and bound, and its global symbols will be
    /// available during binding and type checking.
    ///
    /// Routes through the global `get_or_create_lib_file` cache so multiple
    /// `Parser` instances created in a single WASM session share one parsed-
    /// and-bound representation per (`file_name`, `content_hash`) pair instead of
    /// each rebuilding their own. The cache is keyed on a content hash, so
    /// calls with different content for the same file name still get distinct,
    /// freshly-bound lib files (no aliasing risk). Mirrors what
    /// `WasmProgram::checkAll` already does.
    #[wasm_bindgen(js_name = addLibFile)]
    pub fn add_lib_file(&mut self, file_name: String, source_text: String) {
        let lib_file = crate::api::wasm::lib_cache::get_or_create_lib_file(file_name, source_text);

        self.lib_files.push(lib_file);

        // Invalidate binder since we have new global symbols
        self.binder = None;
        self.type_cache = None;
    }

    /// Parse the source file and return the root node index.
    #[wasm_bindgen(js_name = parseSourceFile)]
    pub fn parse_source_file(&mut self) -> u32 {
        let idx = self.parser.parse_source_file();
        self.source_file_idx = Some(idx);
        // Invalidate derived state on re-parse
        self.line_map = None;
        self.binder = None;
        self.type_cache = None; // Invalidate type cache when file changes
        self.scope_cache.clear();
        idx.0
    }

    /// Get the number of nodes in the AST.
    #[allow(clippy::missing_const_for_fn)] // wasm_bindgen does not support const fn
    #[wasm_bindgen(js_name = getNodeCount)]
    pub fn get_node_count(&self) -> usize {
        self.parser.get_node_count()
    }

    /// Get parse diagnostics as JSON.
    #[wasm_bindgen(js_name = getDiagnosticsJson)]
    pub fn get_diagnostics_json(&self) -> String {
        let diags: Vec<_> = self
            .parser
            .get_diagnostics()
            .iter()
            .map(|d| {
                serde_json::json!({
                    "message": d.message,
                    "start": d.start,
                    "length": d.length,
                    "code": d.code,
                })
            })
            .collect();
        serde_json::to_string(&diags).unwrap_or_else(|_| "[]".to_string())
    }

    /// Bind the source file and return symbol count.
    #[wasm_bindgen(js_name = bindSourceFile)]
    pub fn bind_source_file(&mut self) -> String {
        if let Some(root_idx) = self.source_file_idx {
            let mut binder = BinderState::new();
            // Use bind_source_file_with_libs to merge lib symbols into the binder
            // This properly remaps SymbolIds to avoid collisions across lib files
            binder.bind_source_file_with_libs(self.parser.get_arena(), root_idx, &self.lib_files);

            // Collect symbol names for the result
            let symbols: FxHashMap<String, u32> = binder
                .file_locals
                .iter()
                .map(|(name, id)| (name.clone(), id.0))
                .collect();

            let result = serde_json::json!({
                "symbols": symbols,
                "symbolCount": binder.symbols.len(),
            });

            self.binder = Some(binder);
            self.scope_cache.clear();
            serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
        } else {
            r#"{"error": "Source file not parsed"}"#.to_string()
        }
    }

    /// Type check the source file and return diagnostics.
    #[wasm_bindgen(js_name = checkSourceFile)]
    pub fn check_source_file(&mut self) -> String {
        if self.binder.is_none() {
            // Auto-bind if not done yet
            if self.source_file_idx.is_some() {
                self.bind_source_file();
            }
        }

        if let (Some(root_idx), Some(binder)) = (self.source_file_idx, &self.binder) {
            let file_name = self.parser.get_file_name().to_string();

            // Get compiler options
            let checker_options = self.compiler_options.to_checker_options();
            let mut checker = if let Some(cache) = self.type_cache.take() {
                CheckerState::with_cache_and_options(
                    self.parser.get_arena(),
                    binder,
                    &self.type_interner,
                    file_name,
                    cache,
                    &checker_options,
                )
            } else {
                CheckerState::with_options(
                    self.parser.get_arena(),
                    binder,
                    &self.type_interner,
                    file_name,
                    &checker_options,
                )
            };

            // Set up lib contexts for global type resolution (Object, Array, etc.)
            if !self.lib_files.is_empty() {
                let lib_contexts: Vec<LibContext> = self
                    .lib_files
                    .iter()
                    .map(|lib| LibContext {
                        arena: Arc::clone(&lib.arena),
                        binder: Arc::clone(&lib.binder),
                    })
                    .collect();
                checker.ctx.set_lib_contexts(lib_contexts);
            }

            // Full source file type checking - traverse all statements
            checker.check_source_file(root_idx);

            let diagnostics = checker
                .ctx
                .diagnostics
                .iter()
                .map(|d| {
                    serde_json::json!({
                        "message_text": d.message_text.clone(),
                        "code": d.code,
                        "start": d.start,
                        "length": d.length,
                        "category": format!("{:?}", d.category),
                    })
                })
                .collect::<Vec<_>>();

            self.type_cache = Some(checker.extract_cache());

            let result = serde_json::json!({
                "typeCount": self.type_interner.len(),
                "diagnostics": diagnostics,
            });

            serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
        } else {
            r#"{"error": "Source file not parsed or bound"}"#.to_string()
        }
    }

    /// Get the type of a node as a string.
    #[wasm_bindgen(js_name = getTypeOfNode)]
    pub fn get_type_of_node(&mut self, node_idx: u32) -> String {
        if let (Some(_), Some(binder)) = (self.source_file_idx, &self.binder) {
            let file_name = self.parser.get_file_name().to_string();

            // Get compiler options
            let checker_options = self.compiler_options.to_checker_options();
            let mut checker = if let Some(cache) = self.type_cache.take() {
                CheckerState::with_cache_and_options(
                    self.parser.get_arena(),
                    binder,
                    &self.type_interner,
                    file_name,
                    cache,
                    &checker_options,
                )
            } else {
                CheckerState::with_options(
                    self.parser.get_arena(),
                    binder,
                    &self.type_interner,
                    file_name,
                    &checker_options,
                )
            };

            let type_id = checker.get_type_of_node(parser::NodeIndex(node_idx));
            // Use format_type for human-readable output
            let result = checker.format_type(type_id);
            self.type_cache = Some(checker.extract_cache());
            result
        } else {
            "unknown".to_string()
        }
    }

    /// Emit the source file as JavaScript (ES5 target, auto-detect CommonJS for modules).
    #[wasm_bindgen(js_name = emit)]
    pub fn emit(&self) -> String {
        if let Some(root_idx) = self.source_file_idx {
            let options = PrinterOptions {
                target: ScriptTarget::ES5,
                ..Default::default()
            };

            let mut ctx = EmitContext::with_options(options);
            ctx.auto_detect_module = true;

            self.emit_with_context(root_idx, ctx)
        } else {
            String::new()
        }
    }

    /// Emit the source file as JavaScript (ES6+ modern output).
    #[wasm_bindgen(js_name = emitModern)]
    pub fn emit_modern(&self) -> String {
        if let Some(root_idx) = self.source_file_idx {
            let options = PrinterOptions {
                target: ScriptTarget::ES2015,
                ..Default::default()
            };

            let ctx = EmitContext::with_options(options);

            self.emit_with_context(root_idx, ctx)
        } else {
            String::new()
        }
    }

    fn emit_with_context(&self, root_idx: parser::NodeIndex, ctx: EmitContext) -> String {
        let transforms = LoweringPass::new(self.parser.get_arena(), &ctx).run(root_idx);

        let mut printer = Printer::with_transforms_and_options(
            self.parser.get_arena(),
            transforms,
            ctx.options.clone(),
        );
        printer.set_target_es5(ctx.target_es5);
        printer.set_auto_detect_module(ctx.auto_detect_module);
        printer.set_source_text(self.parser.get_source_text());
        printer.emit(root_idx);
        printer.get_output().to_string()
    }

    /// Generate transform directives based on compiler options.
    #[wasm_bindgen(js_name = generateTransforms)]
    pub fn generate_transforms(&self, target: u32, module: u32) -> WasmTransformContext {
        let options = PrinterOptions {
            target: ScriptTarget::from_ts_numeric(target).unwrap_or(ScriptTarget::ESNext),
            module: ModuleKind::from_ts_numeric(module).unwrap_or(ModuleKind::None),
            ..Default::default()
        };

        let ctx = EmitContext::with_options(options);
        let transforms = if let Some(root_idx) = self.source_file_idx {
            let lowering = LoweringPass::new(self.parser.get_arena(), &ctx);
            lowering.run(root_idx)
        } else {
            TransformContext::new()
        };

        WasmTransformContext {
            inner: transforms,
            target_es5: ctx.target_es5,
            module_kind: ctx.options.module,
        }
    }

    /// Emit the source file using pre-computed transforms.
    #[wasm_bindgen(js_name = emitWithTransforms)]
    pub fn emit_with_transforms(&self, context: &WasmTransformContext) -> String {
        if let Some(root_idx) = self.source_file_idx {
            let mut printer =
                Printer::with_transforms(self.parser.get_arena(), context.inner.clone());
            printer.set_target_es5(context.target_es5);
            printer.set_module_kind(context.module_kind);
            printer.set_source_text(self.parser.get_source_text());
            printer.emit(root_idx);
            printer.get_output().to_string()
        } else {
            String::new()
        }
    }

    /// Get the AST as JSON (for debugging).
    #[wasm_bindgen(js_name = getAstJson)]
    pub fn get_ast_json(&self) -> String {
        if let Some(root_idx) = self.source_file_idx {
            let arena = self.parser.get_arena();
            format!(
                "{{\"nodeCount\": {}, \"rootIdx\": {}}}",
                arena.len(),
                root_idx.0
            )
        } else {
            "{}".to_string()
        }
    }

    /// Debug type lowering - trace what happens when lowering an interface type
    #[wasm_bindgen(js_name = debugTypeLowering)]
    pub fn debug_type_lowering(&self, interface_name: &str) -> String {
        use parser::syntax_kind_ext;
        use tsz_lowering::TypeLowering;
        use tsz_solver::TypeData;

        let arena = self.parser.get_arena();
        let mut result = Vec::new();

        // Find the interface declaration
        let mut interface_decls = Vec::new();
        for i in 0..arena.len() {
            let idx = parser::NodeIndex(i as u32);
            if let Some(node) = arena.get(idx)
                && node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                && let Some(interface) = arena.get_interface(node)
                && let Some(name_node) = arena.get(interface.name)
                && let Some(ident) = arena.get_identifier(name_node)
                && ident.escaped_text == interface_name
            {
                interface_decls.push(idx);
            }
        }

        if interface_decls.is_empty() {
            return format!("Interface '{interface_name}' not found");
        }

        result.push(format!(
            "Found {} declaration(s) for '{}'",
            interface_decls.len(),
            interface_name
        ));

        // Lower the interface
        let lowering = TypeLowering::new(arena, &self.type_interner);
        let type_id = lowering.lower_interface_declarations(&interface_decls);

        result.push(format!("Lowered type ID: {type_id:?}"));

        // Inspect the result
        if let Some(key) = self.type_interner.lookup(type_id) {
            result.push(format!("Type key: {key:?}"));
            if let TypeData::Object(shape_id) = key {
                let shape = self.type_interner.object_shape(shape_id);
                result.push(format!(
                    "Object shape properties: {}",
                    shape.properties.len()
                ));
                for prop in &shape.properties {
                    let name = self.type_interner.resolve_atom(prop.name);
                    result.push(format!(
                        "  Property '{}': type_id={:?}, optional={}",
                        name, prop.type_id, prop.optional
                    ));
                    // Try to show what the type_id resolves to
                    if let Some(prop_key) = self.type_interner.lookup(prop.type_id) {
                        result.push(format!("    -> {prop_key:?}"));
                    }
                }
            }
        }

        result.join("\n")
    }

    /// Debug interface parsing - dump interface members for diagnostics
    #[wasm_bindgen(js_name = debugInterfaceMembers)]
    pub fn debug_interface_members(&self, interface_name: &str) -> String {
        use parser::syntax_kind_ext;

        let arena = self.parser.get_arena();
        let mut result = Vec::new();

        for i in 0..arena.len() {
            let idx = parser::NodeIndex(i as u32);
            if let Some(node) = arena.get(idx)
                && node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                && let Some(interface) = arena.get_interface(node)
                && let Some(name_node) = arena.get(interface.name)
                && let Some(ident) = arena.get_identifier(name_node)
                && ident.escaped_text == interface_name
            {
                result.push(format!("Interface '{interface_name}' found at node {i}"));
                result.push(format!("  members list: {:?}", interface.members.nodes));

                for (mi, &member_idx) in interface.members.nodes.iter().enumerate() {
                    if let Some(member_node) = arena.get(member_idx) {
                        result.push(format!(
                            "  Member {} (idx {}): kind={}",
                            mi, member_idx.0, member_node.kind
                        ));
                        result.push(format!("    data_index: {}", member_node.data_index));
                        if let Some(sig) = arena.get_signature(member_node) {
                            result.push(format!("    name_idx: {:?}", sig.name));
                            result.push(format!(
                                "    type_annotation_idx: {:?}",
                                sig.type_annotation
                            ));

                            // Get name text
                            if let Some(name_n) = arena.get(sig.name) {
                                if let Some(name_id) = arena.get_identifier(name_n) {
                                    result
                                        .push(format!("    name_text: '{}'", name_id.escaped_text));
                                } else {
                                    result.push(format!("    name_node kind: {}", name_n.kind));
                                }
                            }

                            // Get type annotation text
                            if let Some(type_n) = arena.get(sig.type_annotation) {
                                if let Some(type_id) = arena.get_identifier(type_n) {
                                    result
                                        .push(format!("    type_text: '{}'", type_id.escaped_text));
                                } else {
                                    result.push(format!("    type_node kind: {}", type_n.kind));
                                }
                            }
                        }
                    }
                }
            }
        }

        if result.is_empty() {
            format!("Interface '{interface_name}' not found")
        } else {
            result.join("\n")
        }
    }

    /// Debug namespace scoping - dump scope info for all scopes
    #[wasm_bindgen(js_name = debugScopes)]
    pub fn debug_scopes(&self) -> String {
        let Some(binder) = &self.binder else {
            return "Binder not initialized. Call parseSourceFile and bindSourceFile first."
                .to_string();
        };

        let mut result = Vec::new();
        result.push(format!(
            "=== Persistent Scopes ({}) ===",
            binder.scopes.len()
        ));

        for (i, scope) in binder.scopes.iter().enumerate() {
            result.push(format!(
                "\nScope {} (parent: {:?}, kind: {:?}):",
                i, scope.parent, scope.kind
            ));
            result.push(format!("  table entries: {}", scope.table.len()));
            for (name, sym_id) in scope.table.iter() {
                if let Some(sym) = binder.symbols.get(*sym_id) {
                    result.push(format!(
                        "    '{}' -> SymbolId({}) [flags: 0x{:x}]",
                        name, sym_id.0, sym.flags
                    ));
                } else {
                    result.push(format!(
                        "    '{}' -> SymbolId({}) [MISSING SYMBOL]",
                        name, sym_id.0
                    ));
                }
            }
        }

        result.push(format!(
            "\n=== Node -> Scope Mappings ({}) ===",
            binder.node_scope_ids.len()
        ));
        for (&node_idx, &scope_id) in binder.node_scope_ids.iter() {
            result.push(format!(
                "  NodeIndex({}) -> ScopeId({})",
                node_idx, scope_id.0
            ));
        }

        result.push(format!(
            "\n=== File Locals ({}) ===",
            binder.file_locals.len()
        ));
        for (name, sym_id) in binder.file_locals.iter() {
            result.push(format!("  '{}' -> SymbolId({})", name, sym_id.0));
        }

        result.join("\n")
    }

    /// Trace the parent chain for a node at a given position
    #[wasm_bindgen(js_name = traceParentChain)]
    pub fn trace_parent_chain(&self, pos: u32) -> String {
        const IDENTIFIER_KIND: u16 = 80; // SyntaxKind::Identifier
        let arena = self.parser.get_arena();
        let binder = match &self.binder {
            Some(b) => b,
            None => return "Binder not initialized".to_string(),
        };

        let mut result = Vec::new();
        result.push(format!("=== Tracing parent chain for position {pos} ==="));

        // Find node at position
        let mut target_node = None;
        for i in 0..arena.len() {
            let idx = parser::NodeIndex(i as u32);
            if let Some(node) = arena.get(idx)
                && node.pos <= pos
                && pos < node.end
                && node.kind == IDENTIFIER_KIND
            {
                target_node = Some(idx);
                // Don't break - prefer smaller range
            }
        }

        let start_idx = match target_node {
            Some(idx) => idx,
            None => return format!("No identifier node found at position {pos}"),
        };

        result.push(format!("Starting node: {start_idx:?}"));

        let mut current = start_idx;
        let mut depth = 0;
        while current.is_some() && depth < 20 {
            if let Some(node) = arena.get(current) {
                let kind_name = format!("kind={}", node.kind);
                let scope_info = if let Some(&scope_id) = binder.node_scope_ids.get(&current.0) {
                    format!(" -> ScopeId({})", scope_id.0)
                } else {
                    String::new()
                };
                result.push(format!(
                    "  [{}] NodeIndex({}) {} [pos:{}-{}]{}",
                    depth, current.0, kind_name, node.pos, node.end, scope_info
                ));
            }

            if let Some(ext) = arena.get_extended(current) {
                if ext.parent.is_none() {
                    result.push(format!("  [{}] Parent is NodeIndex::NONE", depth + 1));
                    break;
                }
                current = ext.parent;
            } else {
                result.push(format!(
                    "  [{}] No extended info for NodeIndex({})",
                    depth + 1,
                    current.0
                ));
                break;
            }
            depth += 1;
        }

        result.join("\n")
    }

    /// Dump variable declaration info for debugging
    #[wasm_bindgen(js_name = dumpVarDecl)]
    pub fn dump_var_decl(&self, var_decl_idx: u32) -> String {
        let arena = self.parser.get_arena();
        let idx = parser::NodeIndex(var_decl_idx);

        let Some(node) = arena.get(idx) else {
            return format!("NodeIndex({var_decl_idx}) not found");
        };

        let Some(var_decl) = arena.get_variable_declaration(node) else {
            return format!(
                "NodeIndex({}) is not a VARIABLE_DECLARATION (kind={})",
                var_decl_idx, node.kind
            );
        };

        format!(
            "VariableDeclaration({}):\n  name: NodeIndex({})\n  type_annotation: NodeIndex({}) (is_none={})\n  initializer: NodeIndex({})",
            var_decl_idx,
            var_decl.name.0,
            var_decl.type_annotation.0,
            var_decl.type_annotation.is_none(),
            var_decl.initializer.0
        )
    }

    /// Dump all nodes for debugging
    #[wasm_bindgen(js_name = dumpAllNodes)]
    pub fn dump_all_nodes(&self, start: u32, count: u32) -> String {
        let arena = self.parser.get_arena();
        let mut result = Vec::new();

        for i in start..(start + count).min(arena.len() as u32) {
            let idx = parser::NodeIndex(i);
            if let Some(node) = arena.get(idx) {
                let parent_str = if let Some(ext) = arena.get_extended(idx) {
                    if ext.parent.is_none() {
                        "parent:NONE".to_string()
                    } else {
                        format!("parent:{}", ext.parent.0)
                    }
                } else {
                    "no-ext".to_string()
                };
                // Add identifier text if available
                let extra = if let Some(ident) = arena.get_identifier(node) {
                    format!(" \"{}\"", ident.escaped_text)
                } else {
                    String::new()
                };
                result.push(format!(
                    "  NodeIndex({}) kind={} [pos:{}-{}] {}{}",
                    i, node.kind, node.pos, node.end, parent_str, extra
                ));
            }
        }

        result.join("\n")
    }

    // =========================================================================
    // LSP Feature Methods
    // =========================================================================

    /// Ensure internal `LineMap` is built.
    fn ensure_line_map(&mut self) {
        if self.line_map.is_none() {
            self.line_map = Some(LineMap::build(self.parser.get_source_text()));
        }
    }

    fn lib_contexts(&self) -> Vec<LibContext> {
        self.lib_files
            .iter()
            .map(|lib| LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect()
    }

    /// Ensure source file is parsed and bound.
    fn ensure_bound(&mut self) -> Result<(), JsValue> {
        if self.source_file_idx.is_none() {
            return Err(JsValue::from_str("Source file not parsed"));
        }
        if self.binder.is_none() {
            self.bind_source_file();
        }
        Ok(())
    }

    /// Go to Definition: Returns array of Location objects.
    #[wasm_bindgen(js_name = getDefinitionAtPosition)]
    pub fn get_definition_at_position(
        &mut self,
        line: u32,
        character: u32,
    ) -> Result<JsValue, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let root = self
            .source_file_idx
            .ok_or_else(|| JsValue::from_str("Source file not available"))?;
        let binder = self
            .binder
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Binder not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Line map not available"))?;
        let file_name = self.parser.get_file_name().to_string();
        let source_text = self.parser.get_source_text();

        let provider = GoToDefinition::new(
            self.parser.get_arena(),
            binder,
            line_map,
            file_name,
            source_text,
        );
        let pos = Position::new(line, character);

        let result =
            provider.get_definition_with_scope_cache(root, pos, &mut self.scope_cache, None);
        Ok(serde_wasm_bindgen::to_value(&result)?)
    }

    /// Find References: Returns array of Location objects.
    #[wasm_bindgen(js_name = getReferencesAtPosition)]
    pub fn get_references_at_position(
        &mut self,
        line: u32,
        character: u32,
    ) -> Result<JsValue, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let root = self
            .source_file_idx
            .ok_or_else(|| JsValue::from_str("Source file not available"))?;
        let binder = self
            .binder
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Binder not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Line map not available"))?;
        let file_name = self.parser.get_file_name().to_string();
        let source_text = self.parser.get_source_text();

        let provider = FindReferences::new(
            self.parser.get_arena(),
            binder,
            line_map,
            file_name,
            source_text,
        );
        let pos = Position::new(line, character);

        let result =
            provider.find_references_with_scope_cache(root, pos, &mut self.scope_cache, None);
        Ok(serde_wasm_bindgen::to_value(&result)?)
    }

    /// Completions: Returns array of `CompletionItem` objects.
    #[wasm_bindgen(js_name = getCompletionsAtPosition)]
    pub fn get_completions_at_position(
        &mut self,
        line: u32,
        character: u32,
    ) -> Result<JsValue, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let root = self
            .source_file_idx
            .ok_or_else(|| JsValue::from_str("Source file not available"))?;
        let binder = self
            .binder
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Binder not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Line map not available"))?;
        let source_text = self.parser.get_source_text();
        let file_name = self.parser.get_file_name().to_string();
        let checker_options = self.compiler_options.to_checker_options();
        let lib_contexts = self.lib_contexts();

        let provider = Completions::with_options_and_lib_contexts(
            self.parser.get_arena(),
            binder,
            line_map,
            &self.type_interner,
            source_text,
            file_name,
            FullProviderOptions {
                strict: checker_options.strict,
                sound_mode: checker_options.sound_mode,
                lib_contexts: &lib_contexts,
            },
        );
        let pos = Position::new(line, character);

        let result = provider.get_completions_with_caches(
            root,
            pos,
            &mut self.type_cache,
            &mut self.scope_cache,
            None,
        );
        Ok(serde_wasm_bindgen::to_value(&result)?)
    }

    /// Hover: Returns `HoverInfo` object.
    #[wasm_bindgen(js_name = getHoverAtPosition)]
    pub fn get_hover_at_position(&mut self, line: u32, character: u32) -> Result<JsValue, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let root = self
            .source_file_idx
            .ok_or_else(|| JsValue::from_str("Source file not available"))?;
        let binder = self
            .binder
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Binder not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Line map not available"))?;
        let source_text = self.parser.get_source_text();
        let file_name = self.parser.get_file_name().to_string();
        let checker_options = self.compiler_options.to_checker_options();
        let lib_contexts = self.lib_contexts();

        let provider = HoverProvider::with_options_and_lib_contexts(
            self.parser.get_arena(),
            binder,
            line_map,
            &self.type_interner,
            source_text,
            file_name,
            FullProviderOptions {
                strict: checker_options.strict,
                sound_mode: checker_options.sound_mode,
                lib_contexts: &lib_contexts,
            },
        );
        let pos = Position::new(line, character);

        let result = provider.get_hover_with_scope_cache(
            root,
            pos,
            &mut self.type_cache,
            &mut self.scope_cache,
            None,
        );
        Ok(serde_wasm_bindgen::to_value(&result)?)
    }

    /// Signature Help: Returns `SignatureHelp` object.
    #[wasm_bindgen(js_name = getSignatureHelpAtPosition)]
    pub fn get_signature_help_at_position(
        &mut self,
        line: u32,
        character: u32,
    ) -> Result<JsValue, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let root = self
            .source_file_idx
            .ok_or_else(|| JsValue::from_str("Source file not available"))?;
        let binder = self
            .binder
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Binder not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Line map not available"))?;
        let source_text = self.parser.get_source_text();
        let file_name = self.parser.get_file_name().to_string();
        let checker_options = self.compiler_options.to_checker_options();
        let lib_contexts = self.lib_contexts();

        let provider = SignatureHelpProvider::with_options_and_lib_contexts(
            self.parser.get_arena(),
            binder,
            line_map,
            &self.type_interner,
            source_text,
            file_name,
            FullProviderOptions {
                strict: checker_options.strict,
                sound_mode: checker_options.sound_mode,
                lib_contexts: &lib_contexts,
            },
        );
        let pos = Position::new(line, character);

        let result = provider.get_signature_help_with_scope_cache(
            root,
            pos,
            &mut self.type_cache,
            &mut self.scope_cache,
            None,
        );
        Ok(serde_wasm_bindgen::to_value(&result)?)
    }

    /// Document Symbols: Returns array of `DocumentSymbol` objects.
    #[wasm_bindgen(js_name = getDocumentSymbols)]
    pub fn get_document_symbols(&mut self) -> Result<JsValue, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let root = self
            .source_file_idx
            .ok_or_else(|| JsValue::from_str("Source file not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Line map not available"))?;
        let source_text = self.parser.get_source_text();

        let provider = DocumentSymbolProvider::new(self.parser.get_arena(), line_map, source_text);

        let result = provider.get_document_symbols(root);
        Ok(serde_wasm_bindgen::to_value(&result)?)
    }

    /// Semantic Tokens: Returns flat array of u32 (delta encoded).
    #[wasm_bindgen(js_name = getSemanticTokens)]
    pub fn get_semantic_tokens(&mut self) -> Result<Vec<u32>, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let root = self
            .source_file_idx
            .ok_or_else(|| JsValue::from_str("Source file not available"))?;
        let binder = self
            .binder
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Binder not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Line map not available"))?;
        let source_text = self.parser.get_source_text();

        let mut provider =
            SemanticTokensProvider::new(self.parser.get_arena(), binder, line_map, source_text);

        Ok(provider.get_semantic_tokens(root))
    }

    /// Rename - Prepare: Check if rename is valid at position.
    #[wasm_bindgen(js_name = prepareRename)]
    pub fn prepare_rename(&mut self, line: u32, character: u32) -> Result<JsValue, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let binder = self
            .binder
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Internal error: binder not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Internal error: line map not available"))?;
        let file_name = self.parser.get_file_name().to_string();
        let source_text = self.parser.get_source_text();

        let provider = RenameProvider::new(
            self.parser.get_arena(),
            binder,
            line_map,
            file_name,
            source_text,
        );
        let pos = Position::new(line, character);

        let result = provider.prepare_rename(pos);
        Ok(serde_wasm_bindgen::to_value(&result)?)
    }

    /// Rename - Edits: Get workspace edits for rename.
    #[wasm_bindgen(js_name = getRenameEdits)]
    pub fn get_rename_edits(
        &mut self,
        line: u32,
        character: u32,
        new_name: String,
    ) -> Result<JsValue, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let root = self
            .source_file_idx
            .ok_or_else(|| JsValue::from_str("Source file not available"))?;
        let binder = self
            .binder
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Binder not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Line map not available"))?;
        let file_name = self.parser.get_file_name().to_string();
        let source_text = self.parser.get_source_text();

        let provider = RenameProvider::new(
            self.parser.get_arena(),
            binder,
            line_map,
            file_name,
            source_text,
        );
        let pos = Position::new(line, character);

        match provider.provide_rename_edits_with_scope_cache(
            root,
            pos,
            new_name,
            &mut self.scope_cache,
            None,
        ) {
            Ok(edit) => Ok(serde_wasm_bindgen::to_value(&edit)?),
            Err(e) => Err(JsValue::from_str(&e)),
        }
    }

    /// Code Actions: Get code actions for a range.
    #[wasm_bindgen(js_name = getCodeActions)]
    pub fn get_code_actions(
        &mut self,
        start_line: u32,
        start_char: u32,
        end_line: u32,
        end_char: u32,
    ) -> Result<JsValue, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let root = self
            .source_file_idx
            .ok_or_else(|| JsValue::from_str("Source file not available"))?;
        let binder = self
            .binder
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Binder not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Line map not available"))?;
        let file_name = self.parser.get_file_name().to_string();
        let source_text = self.parser.get_source_text();

        let provider = CodeActionProvider::new(
            self.parser.get_arena(),
            binder,
            line_map,
            file_name,
            source_text,
        );

        let range = Range::new(
            Position::new(start_line, start_char),
            Position::new(end_line, end_char),
        );

        let context = default_code_action_context();

        let result = provider.provide_code_actions(root, range, context);
        Ok(serde_wasm_bindgen::to_value(&result)?)
    }

    /// Code Actions: Get code actions for a range with diagnostics context.
    #[wasm_bindgen(js_name = getCodeActionsWithContext)]
    pub fn get_code_actions_with_context(
        &mut self,
        start_line: u32,
        start_char: u32,
        end_line: u32,
        end_char: u32,
        context: JsValue,
    ) -> Result<JsValue, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let context = parse_code_action_context(context)?;

        let root = self
            .source_file_idx
            .ok_or_else(|| JsValue::from_str("Source file not available"))?;
        let binder = self
            .binder
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Binder not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Line map not available"))?;
        let file_name = self.parser.get_file_name().to_string();
        let source_text = self.parser.get_source_text();

        let provider = CodeActionProvider::new(
            self.parser.get_arena(),
            binder,
            line_map,
            file_name,
            source_text,
        );

        let range = Range::new(
            Position::new(start_line, start_char),
            Position::new(end_line, end_char),
        );

        let result = provider.provide_code_actions(root, range, context);
        Ok(serde_wasm_bindgen::to_value(&result)?)
    }

    /// Diagnostics: Get checker diagnostics in LSP format.
    #[wasm_bindgen(js_name = getLspDiagnostics)]
    pub fn get_lsp_diagnostics(&mut self) -> Result<JsValue, JsValue> {
        self.ensure_bound()?;
        self.ensure_line_map();

        let root = self
            .source_file_idx
            .ok_or_else(|| JsValue::from_str("Source file not available"))?;
        let binder = self
            .binder
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Binder not available"))?;
        let line_map = self
            .line_map
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Line map not available"))?;
        let file_name = self.parser.get_file_name().to_string();
        let source_text = self.parser.get_source_text();

        // Get compiler options
        let checker_options = self.compiler_options.to_checker_options();

        let mut checker = if let Some(cache) = self.type_cache.take() {
            CheckerState::with_cache_and_options(
                self.parser.get_arena(),
                binder,
                &self.type_interner,
                file_name,
                cache,
                &checker_options,
            )
        } else {
            CheckerState::with_options(
                self.parser.get_arena(),
                binder,
                &self.type_interner,
                file_name,
                &checker_options,
            )
        };

        checker.check_source_file(root);

        let lsp_diagnostics: Vec<_> = checker
            .ctx
            .diagnostics
            .iter()
            .map(|diag| convert_diagnostic(diag, line_map, source_text))
            .collect();

        self.type_cache = Some(checker.extract_cache());

        Ok(serde_wasm_bindgen::to_value(&lsp_diagnostics)?)
    }
}
