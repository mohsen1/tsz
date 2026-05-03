use crate::output::source_writer::SourceWriter;
use crate::type_cache_view::TypeCacheView;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tracing::debug;
use tsz_binder::{BinderState, SymbolId};
use tsz_common::diagnostics::Diagnostic;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeInterner;

use super::{DeclarationEmitter, ImportPlan, SourceMapState};

impl<'a> DeclarationEmitter<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        DeclarationEmitter {
            arena,
            writer: SourceWriter::with_capacity(4096),
            indent_level: 0,
            source_map_text: None,
            source_map_state: None,
            pending_source_pos: None,
            source_is_declaration_file: false,
            source_is_js_file: false,
            emit_public_api_only: false,
            public_api_scope_depth: 0,
            source_file_text: None,
            type_cache: None,
            current_source_file_idx: None,
            type_interner: None,
            binder: None,
            export_surface: None,
            used_symbols: None,
            foreign_symbols: None,
            current_arena: None,
            current_file_path: None,
            arena_to_path: FxHashMap::default(),
            file_idx_to_path: FxHashMap::default(),
            global_symbol_arenas: FxHashMap::default(),
            required_imports: FxHashMap::default(),
            reserved_names: FxHashSet::default(),
            import_string_aliases: FxHashMap::default(),
            import_symbol_map: FxHashMap::default(),
            import_name_map: FxHashMap::default(),
            symbol_module_specifier_cache: FxHashMap::default(),
            import_plan: ImportPlan::default(),
            inside_declare_namespace: false,
            enclosing_namespace_symbol: None,
            inside_non_ambient_namespace: false,
            in_constructor_params: false,
            function_names_with_overloads: FxHashSet::default(),
            class_has_constructor_overloads: false,
            class_extends_another: false,
            method_names_with_overloads: FxHashSet::default(),
            all_comments: Vec::new(),
            comment_emit_idx: 0,
            current_statement_jsdoc_chain: Vec::new(),
            remove_comments: false,
            strip_internal: false,
            files_with_augmentations: FxHashSet::default(),
            emitted_non_exported_declaration: false,
            emitted_scope_marker: false,
            emitted_module_indicator: false,
            ambient_module_has_scope_marker: false,
            js_named_export_names: FxHashSet::default(),
            js_folded_named_export_statements: FxHashMap::default(),
            js_deferred_named_export_statements: FxHashSet::default(),
            js_local_export_aliases: Vec::new(),
            js_skipped_local_export_aliases: FxHashSet::default(),
            js_export_equals_names: FxHashSet::default(),
            emitted_js_export_equals_names: FxHashSet::default(),
            js_export_default_names: FxHashSet::default(),
            emitted_js_export_default_names: FxHashSet::default(),
            js_shadowed_export_equals_local_aliases: FxHashMap::default(),
            js_namespace_export_aliases: FxHashMap::default(),
            js_cjs_export_aliases: Vec::new(),
            js_cjs_export_alias_statements: FxHashSet::default(),
            js_module_exports_object_stmts: FxHashSet::default(),
            js_require_property_import_aliases: Vec::new(),
            js_deferred_function_export_statements: FxHashMap::default(),
            js_deferred_value_export_statements: FxHashMap::default(),
            js_deferred_prototype_method_statements: FxHashMap::default(),
            js_class_like_prototype_members: FxHashMap::default(),
            js_class_like_prototype_stmts: FxHashSet::default(),
            js_static_method_augmentation_statements: FxHashMap::default(),
            js_skipped_static_method_augmentation_statements: FxHashSet::default(),
            js_augmented_static_method_nodes: FxHashSet::default(),
            js_grouped_reexports: FxHashMap::default(),
            js_skipped_reexports: FxHashSet::default(),
            js_hoisted_function_declarations: FxHashSet::default(),
            emitted_jsdoc_type_aliases: FxHashSet::default(),
            use_jsdoc_satisfies_parameter_fallback: false,
            normalize_string_literal_type_quotes: false,
            suppress_current_statement_jsdoc_comments: false,
            emitted_synthetic_dependency_symbols: FxHashSet::default(),
            diagnostics: Vec::new(),
            skip_portability_check: false,
            strict_null_checks: false,
            isolated_declarations: false,
            all_enum_values: FxHashMap::default(),
        }
    }

    pub fn with_type_info(
        arena: &'a NodeArena,
        type_cache: TypeCacheView,
        type_interner: &'a TypeInterner,
        binder: &'a BinderState,
    ) -> Self {
        DeclarationEmitter {
            arena,
            writer: SourceWriter::with_capacity(4096),
            indent_level: 0,
            source_map_text: None,
            source_map_state: None,
            pending_source_pos: None,
            source_is_declaration_file: false,
            source_is_js_file: false,
            emit_public_api_only: false,
            public_api_scope_depth: 0,
            source_file_text: None,
            type_cache: Some(type_cache),
            current_source_file_idx: None,
            type_interner: Some(type_interner),
            binder: Some(binder),
            export_surface: None,
            used_symbols: None,
            foreign_symbols: None,
            current_arena: None,
            current_file_path: None,
            arena_to_path: FxHashMap::default(),
            file_idx_to_path: FxHashMap::default(),
            global_symbol_arenas: FxHashMap::default(),
            required_imports: FxHashMap::default(),
            reserved_names: FxHashSet::default(),
            import_string_aliases: FxHashMap::default(),
            import_symbol_map: FxHashMap::default(),
            import_name_map: FxHashMap::default(),
            symbol_module_specifier_cache: FxHashMap::default(),
            import_plan: ImportPlan::default(),
            inside_declare_namespace: false,
            enclosing_namespace_symbol: None,
            inside_non_ambient_namespace: false,
            in_constructor_params: false,
            function_names_with_overloads: FxHashSet::default(),
            class_has_constructor_overloads: false,
            class_extends_another: false,
            method_names_with_overloads: FxHashSet::default(),
            all_comments: Vec::new(),
            comment_emit_idx: 0,
            current_statement_jsdoc_chain: Vec::new(),
            remove_comments: false,
            strip_internal: false,
            files_with_augmentations: FxHashSet::default(),
            emitted_non_exported_declaration: false,
            emitted_scope_marker: false,
            emitted_module_indicator: false,
            ambient_module_has_scope_marker: false,
            js_named_export_names: FxHashSet::default(),
            js_folded_named_export_statements: FxHashMap::default(),
            js_deferred_named_export_statements: FxHashSet::default(),
            js_local_export_aliases: Vec::new(),
            js_skipped_local_export_aliases: FxHashSet::default(),
            js_export_equals_names: FxHashSet::default(),
            emitted_js_export_equals_names: FxHashSet::default(),
            js_export_default_names: FxHashSet::default(),
            emitted_js_export_default_names: FxHashSet::default(),
            js_shadowed_export_equals_local_aliases: FxHashMap::default(),
            js_namespace_export_aliases: FxHashMap::default(),
            js_cjs_export_aliases: Vec::new(),
            js_cjs_export_alias_statements: FxHashSet::default(),
            js_module_exports_object_stmts: FxHashSet::default(),
            js_require_property_import_aliases: Vec::new(),
            js_deferred_function_export_statements: FxHashMap::default(),
            js_deferred_value_export_statements: FxHashMap::default(),
            js_deferred_prototype_method_statements: FxHashMap::default(),
            js_class_like_prototype_members: FxHashMap::default(),
            js_class_like_prototype_stmts: FxHashSet::default(),
            js_static_method_augmentation_statements: FxHashMap::default(),
            js_skipped_static_method_augmentation_statements: FxHashSet::default(),
            js_augmented_static_method_nodes: FxHashSet::default(),
            js_grouped_reexports: FxHashMap::default(),
            js_skipped_reexports: FxHashSet::default(),
            js_hoisted_function_declarations: FxHashSet::default(),
            emitted_jsdoc_type_aliases: FxHashSet::default(),
            use_jsdoc_satisfies_parameter_fallback: false,
            normalize_string_literal_type_quotes: false,
            suppress_current_statement_jsdoc_comments: false,
            emitted_synthetic_dependency_symbols: FxHashSet::default(),
            diagnostics: Vec::new(),
            skip_portability_check: false,
            strict_null_checks: false,
            isolated_declarations: false,
            all_enum_values: FxHashMap::default(),
        }
    }

    pub const fn set_source_map_text(&mut self, text: &'a str) {
        self.source_map_text = Some(text);
    }

    pub fn enable_source_map(&mut self, output_name: &str, source_name: &str) {
        self.source_map_state = Some(SourceMapState {
            output_name: output_name.to_string(),
            source_name: source_name.to_string(),
        });
    }

    pub fn generate_source_map_json(&mut self) -> Option<String> {
        self.writer.generate_source_map_json()
    }

    /// Set the set of used symbols for import/export elision.
    ///
    /// When this is set, the emitter will filter out imports that are not
    /// referenced in the exported API surface.
    pub fn set_used_symbols(
        &mut self,
        symbols: FxHashMap<SymbolId, crate::declaration_emitter::usage_analyzer::UsageKind>,
    ) {
        self.used_symbols = Some(symbols);
    }

    /// Set the set of foreign symbols for auto-generation.
    ///
    /// This enables automatic import generation for symbols from other modules.
    pub fn set_foreign_symbols(&mut self, symbols: FxHashSet<SymbolId>) {
        self.foreign_symbols = Some(symbols);
    }

    /// Set the binder state for symbol resolution.
    ///
    /// This enables `UsageAnalyzer` to resolve symbols during import/export elision.
    pub const fn set_binder(&mut self, binder: Option<&'a BinderState>) {
        self.binder = binder;
    }

    /// Set a precomputed export surface summary.
    ///
    /// When set, the emitter uses the summary's overload pre-scan instead
    /// of discovering overloads incrementally during the emit walk.
    pub fn set_export_surface(&mut self, surface: tsz_binder::ExportSurface) {
        self.export_surface = Some(surface);
    }

    /// Set the current file's arena and path for distinguishing local vs foreign symbols.
    ///
    /// This enables `UsageAnalyzer` to track which symbols need imports.
    pub fn set_current_arena(&mut self, arena: Arc<NodeArena>, file_path: String) {
        self.current_arena = Some(arena);
        self.current_file_path = Some(file_path);
    }

    /// Set the mapping from arena address to file path.
    ///
    /// This enables resolving foreign symbols to their source files.
    pub fn set_arena_to_path(&mut self, arena_to_path: FxHashMap<usize, String>) {
        self.arena_to_path = arena_to_path;
    }

    /// Set the mapping from file index to file path.
    ///
    /// This enables resolving symbol source paths via `decl_file_idx` when
    /// the symbol is not in `symbol_arenas` (e.g., for cross-file interface
    /// references created during type checking).
    pub fn set_file_idx_to_path(&mut self, file_idx_to_path: FxHashMap<u32, String>) {
        self.file_idx_to_path = file_idx_to_path;
    }

    /// Set the global symbol-to-arena mapping from all program files.
    ///
    /// This enables `get_symbol_source_path` to resolve cross-file symbols
    /// whose arenas are not in the current file's binder, which is required
    /// for TS2883 portability checks on imported types.
    pub fn set_global_symbol_arenas(
        &mut self,
        global_symbol_arenas: FxHashMap<SymbolId, Arc<NodeArena>>,
    ) {
        self.global_symbol_arenas = global_symbol_arenas;
    }

    pub const fn set_remove_comments(&mut self, remove: bool) {
        self.remove_comments = remove;
    }

    pub const fn set_strip_internal(&mut self, strip: bool) {
        self.strip_internal = strip;
    }

    /// Set the collection of file paths that contain module augmentations.
    pub fn set_files_with_augmentations(&mut self, files: FxHashSet<String>) {
        self.files_with_augmentations = files;
    }

    /// Skip TS2883 non-portable type reference checks.
    /// Use for node16/nodenext module modes where module resolution already
    /// enforces portability via the exports map.
    pub const fn set_skip_portability_check(&mut self, skip: bool) {
        self.skip_portability_check = skip;
    }

    pub const fn set_strict_null_checks(&mut self, strict: bool) {
        self.strict_null_checks = strict;
    }

    pub const fn set_isolated_declarations(&mut self, isolated: bool) {
        self.isolated_declarations = isolated;
    }

    /// Take diagnostics collected during declaration emit (e.g., TS2883).
    pub fn take_diagnostics(&mut self) -> Vec<Diagnostic> {
        Self::normalize_portability_diagnostics(std::mem::take(&mut self.diagnostics))
    }

    /// Build a map of imported `SymbolId` -> `ModuleSpecifier` for elision.
    ///
    /// Walks all import statements and tracks which module each imported
    /// symbol claims to come from. This enables elision of unused imports.
    pub(in crate::declaration_emitter) fn prepare_import_metadata(&mut self, root_idx: NodeIndex) {
        let binder = match &self.binder {
            Some(b) => b,
            None => return,
        };

        let Some(root_node) = self.arena.get(root_idx) else {
            return;
        };
        let Some(source_file) = self.arena.get_source_file(root_node) else {
            return;
        };

        // Walk all statements to find import declarations
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            // Handle regular import declarations
            if stmt_node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                let Some(import) = self.arena.get_import_decl(stmt_node) else {
                    continue;
                };

                // Extract module specifier
                let module_specifier = if let Some(spec) = self.arena.get(import.module_specifier) {
                    match self.arena.get_literal(spec) {
                        Some(lit) => lit.text.clone(),
                        None => continue,
                    }
                } else {
                    continue;
                };

                // Walk import clause to extract imported symbols
                if import.import_clause.is_some() {
                    // Collect symbols to insert after binder is dropped
                    let symbols = self.collect_imported_symbols_from_clause(
                        self.arena,
                        binder,
                        import.import_clause,
                    );
                    for (name, sym_id) in symbols {
                        debug!(
                            "[DEBUG] prepare_import_metadata: inserting {} -> SymbolId({:?}) -> '{}'",
                            name, sym_id, module_specifier
                        );
                        self.import_name_map.insert(name.clone(), sym_id);
                        self.import_symbol_map
                            .insert(sym_id, module_specifier.clone());
                    }
                }
            }
            // Handle import equals declarations (import x = require('y'))
            else if stmt_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                let Some(import_eq) = self.arena.get_import_decl(stmt_node) else {
                    continue;
                };

                // Extract module specifier
                let module_specifier =
                    if let Some(spec) = self.arena.get(import_eq.module_specifier) {
                        match self.arena.get_literal(spec) {
                            Some(lit) => lit.text.clone(),
                            None => continue,
                        }
                    } else {
                        continue;
                    };

                // Get the imported symbol from the import clause name
                // For ImportEqualsDeclaration, import_clause points directly to Identifier node
                if import_eq.import_clause.is_some() {
                    // For ImportEquals, the 'import_clause' field points directly to the Identifier node.
                    // We just need its SymbolId from the binder using the NodeIndex's raw u32 (.0).
                    if let Some(&sym_id) = binder.node_symbols.get(&import_eq.import_clause.0) {
                        self.import_symbol_map.insert(sym_id, module_specifier);
                    }
                }
            }
        }
    }

    pub(in crate::declaration_emitter) fn normalize_portability_diagnostics(
        diagnostics: Vec<Diagnostic>,
    ) -> Vec<Diagnostic> {
        let mut exact_seen: FxHashMap<(u32, String, u32, u32, String), usize> =
            FxHashMap::default();
        let mut unique: Vec<(Diagnostic, bool)> = Vec::new();

        for diagnostic in diagnostics {
            let mut diagnostic = diagnostic;
            let mut was_canonicalized = false;
            if diagnostic.code == 2883 {
                if let Some(message) =
                    Self::canonical_ts2883_named_reference_message(&diagnostic.message_text)
                {
                    diagnostic.message_text = message;
                    was_canonicalized = true;
                }
            }
            let exact_key = (
                diagnostic.code,
                diagnostic.file.clone(),
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.clone(),
            );
            if let Some(&existing_idx) = exact_seen.get(&exact_key) {
                if !was_canonicalized && unique[existing_idx].1 {
                    unique[existing_idx] = (diagnostic, was_canonicalized);
                }
                continue;
            }

            exact_seen.insert(exact_key, unique.len());
            unique.push((diagnostic, was_canonicalized));
        }

        let surviving_canonical_sites: FxHashSet<_> = unique
            .iter()
            .filter_map(|(diagnostic, was_canonicalized)| {
                if diagnostic.code != 2883 || *was_canonicalized {
                    return None;
                }
                let (first, second) =
                    Self::parse_ts2883_named_reference_message(&diagnostic.message_text)?;
                (!Self::looks_like_module_path(&first) && Self::looks_like_module_path(&second))
                    .then(|| (diagnostic.file.clone(), diagnostic.start, diagnostic.length))
            })
            .collect();

        unique
            .into_iter()
            .filter_map(|(diagnostic, was_canonicalized)| {
                if diagnostic.code != 2883 {
                    return Some(diagnostic);
                }

                let Some((first, second)) =
                    Self::parse_ts2883_named_reference_message(&diagnostic.message_text)
                else {
                    return Some(diagnostic);
                };

                if !was_canonicalized
                    || Self::looks_like_module_path(&first)
                    || !Self::looks_like_module_path(&second)
                {
                    return Some(diagnostic);
                }

                (!surviving_canonical_sites.contains(&(
                    diagnostic.file.clone(),
                    diagnostic.start,
                    diagnostic.length,
                )))
                .then_some(diagnostic)
            })
            .collect()
    }

    pub(in crate::declaration_emitter) fn parse_ts2883_named_reference_message(
        message: &str,
    ) -> Option<(String, String)> {
        let prefix = "cannot be named without a reference to '";
        let start = message.find(prefix)? + prefix.len();
        let rest = &message[start..];
        let (first, tail) = rest.split_once("' from '")?;
        let (second, _) = tail.split_once('\'')?;
        Some((first.to_string(), second.to_string()))
    }

    pub(in crate::declaration_emitter) fn canonical_ts2883_named_reference_message(
        message: &str,
    ) -> Option<String> {
        let (first, second) = Self::parse_ts2883_named_reference_message(message)?;
        if !Self::looks_like_module_path(&first) || Self::looks_like_module_path(&second) {
            return None;
        }

        Some(message.replace(
            &format!("reference to '{first}' from '{second}'"),
            &format!("reference to '{second}' from '{first}'"),
        ))
    }

    pub(in crate::declaration_emitter) fn looks_like_module_path(text: &str) -> bool {
        text.starts_with('.')
            || text.starts_with('/')
            || text.contains('/')
            || text.contains('\\')
            || text.contains("node_modules")
    }

    /// Collect all imported symbols from an `ImportClause`.
    ///
    /// Returns a Vec of (name, `SymbolId`) pairs that were found in the import clause.
    pub(in crate::declaration_emitter) fn collect_imported_symbols_from_clause(
        &self,
        arena: &NodeArena,
        binder: &BinderState,
        clause_idx: NodeIndex,
    ) -> Vec<(String, SymbolId)> {
        let mut symbols = Vec::new();

        let Some(clause) = arena.get_import_clause_at(clause_idx) else {
            return symbols;
        };

        // Default import: import Def from './mod'
        if clause.name.is_some()
            && let Some(&sym_id) = binder.node_symbols.get(&clause.name.0)
        {
            // Get the name from the symbol
            if let Some(symbol) = binder.symbols.get(sym_id) {
                symbols.push((symbol.escaped_name.clone(), sym_id));
            }
        }

        // Named imports: import { A, B, C as D } from './mod'
        if clause.named_bindings.is_some()
            && let Some(bindings) = arena.get_named_imports_at(clause.named_bindings)
        {
            if bindings.name.is_some() && bindings.elements.nodes.is_empty() {
                if let Some(&sym_id) = binder.node_symbols.get(&bindings.name.0)
                    && let Some(symbol) = binder.symbols.get(sym_id)
                {
                    symbols.push((symbol.escaped_name.clone(), sym_id));
                }
                return symbols;
            }

            // Process each specifier
            for &spec_idx in &bindings.elements.nodes {
                if let Some(spec) = arena.get_specifier_at(spec_idx) {
                    // Track the local binding name, mirroring binder import symbol creation.
                    // For `import { foo as bar }`, the symbol exposed to usage analysis is `bar`,
                    // not the imported property name `foo`.
                    let name_idx = if spec.name.is_some() {
                        spec.name
                    } else {
                        spec.property_name
                    };

                    if let Some(&sym_id) = binder.node_symbols.get(&name_idx.0) {
                        // Get the name from the symbol
                        if let Some(symbol) = binder.symbols.get(sym_id) {
                            symbols.push((symbol.escaped_name.clone(), sym_id));
                        }
                    }
                }
            }
        }

        symbols
    }
}
