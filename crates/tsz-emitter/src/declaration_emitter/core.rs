use crate::enums::evaluator::EnumEvaluator;
use crate::output::source_writer::{SourcePosition, SourceWriter};
use crate::type_cache_view::TypeCacheView;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tracing::debug;
use tsz_binder::{BinderState, SymbolId};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeInterner;
use tsz_solver::type_queries;

/// Declaration emitter for .d.ts files
pub struct DeclarationEmitter<'a> {
    pub(super) arena: &'a NodeArena,
    pub(super) writer: SourceWriter,
    pub(super) indent_level: u32,
    pub(super) source_map_text: Option<&'a str>,
    pub(super) source_map_state: Option<SourceMapState>,
    pub(super) pending_source_pos: Option<SourcePosition>,
    /// Whether we're currently emitting a declaration file.
    pub(super) source_is_declaration_file: bool,
    /// If true, only emit declarations that are part of the public API surface.
    pub(super) emit_public_api_only: bool,
    /// Track whether we're currently emitting inside a public-API namespace/module.
    pub(super) public_api_scope_depth: u32,
    /// Raw source text for this source file, used for keyword fallback emission.
    pub(super) source_file_text: Option<Arc<str>>,
    /// Type cache for looking up inferred types
    pub(super) type_cache: Option<TypeCacheView>,
    /// Type interner for printing types
    pub(super) type_interner: Option<&'a TypeInterner>,
    /// Binder state for symbol resolution (used by `UsageAnalyzer`)
    pub(super) binder: Option<&'a BinderState>,
    /// Map of symbols to their usage kind (Type, Value, or Both) for import elision
    pub(super) used_symbols:
        Option<FxHashMap<SymbolId, crate::declaration_emitter::usage_analyzer::UsageKind>>,
    /// Set of foreign symbols that need imports (for import generation)
    pub(super) foreign_symbols: Option<FxHashSet<SymbolId>>,
    /// The current file's arena (for distinguishing local vs foreign symbols)
    pub(super) current_arena: Option<Arc<NodeArena>>,
    /// The current file's path (for calculating relative import paths)
    pub(super) current_file_path: Option<String>,
    /// Map of arena address -> file path (for resolving foreign symbol locations)
    pub(super) arena_to_path: FxHashMap<usize, String>,
    /// Map of module → symbol names to auto-generate imports for
    /// Pre-calculated in driver where `MergedProgram` is available
    pub(super) required_imports: FxHashMap<String, Vec<String>>,
    /// Tracks names that are taken in the top-level scope of the file
    /// (includes local declarations and imported names)
    pub(super) reserved_names: FxHashSet<String>,
    /// Maps (`ModulePath`, `ExportName`) -> `AliasName` for string-based imports
    pub(super) import_string_aliases: FxHashMap<(String, String), String>,
    /// Map of imported `SymbolId` -> `ModuleSpecifier` for elision
    /// Tracks which module each imported symbol claims to come from
    pub(super) import_symbol_map: FxHashMap<SymbolId, String>,
    /// Map of imported name -> `SymbolId` for resolving type references
    /// Helps bridge the gap between type references and import symbols
    pub(super) import_name_map: FxHashMap<String, SymbolId>,
    /// Cache of `SymbolId` -> resolved module specifier.
    pub(super) symbol_module_specifier_cache: FxHashMap<SymbolId, Option<String>>,
    /// Precomputed import emission plan for the current file.
    pub(super) import_plan: ImportPlan,
    /// Whether we're inside a declare namespace (don't emit 'declare' keyword inside)
    pub(super) inside_declare_namespace: bool,
    /// Whether we're emitting constructor parameters (don't emit accessibility modifiers)
    pub(super) in_constructor_params: bool,
    /// Track function names that have overload signatures (to skip implementation signatures)
    pub(super) function_names_with_overloads: FxHashSet<String>,
    /// Track whether current class has constructor overloads (to skip implementation constructor)
    pub(super) class_has_constructor_overloads: bool,
    /// Track method names that have overload signatures in current class (to skip implementation signatures)
    pub(super) method_names_with_overloads: FxHashSet<String>,
}

pub(super) struct SourceMapState {
    pub(super) output_name: String,
    pub(super) source_name: String,
}

#[derive(Clone, Debug)]
pub(crate) struct PlannedImportSymbol {
    pub(crate) name: String,
    pub(crate) alias: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct PlannedImportModule {
    pub(crate) module: String,
    pub(crate) symbols: Vec<PlannedImportSymbol>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ImportPlan {
    pub(crate) required: Vec<PlannedImportModule>,
    pub(crate) auto_generated: Vec<PlannedImportModule>,
}

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
            emit_public_api_only: false,
            public_api_scope_depth: 0,
            source_file_text: None,
            type_cache: None,
            type_interner: None,
            binder: None,
            used_symbols: None,
            foreign_symbols: None,
            current_arena: None,
            current_file_path: None,
            arena_to_path: FxHashMap::default(),
            required_imports: FxHashMap::default(),
            reserved_names: FxHashSet::default(),
            import_string_aliases: FxHashMap::default(),
            import_symbol_map: FxHashMap::default(),
            import_name_map: FxHashMap::default(),
            symbol_module_specifier_cache: FxHashMap::default(),
            import_plan: ImportPlan::default(),
            inside_declare_namespace: false,
            in_constructor_params: false,
            function_names_with_overloads: FxHashSet::default(),
            class_has_constructor_overloads: false,
            method_names_with_overloads: FxHashSet::default(),
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
            emit_public_api_only: false,
            public_api_scope_depth: 0,
            source_file_text: None,
            type_cache: Some(type_cache),
            type_interner: Some(type_interner),
            binder: Some(binder),
            used_symbols: None,
            foreign_symbols: None,
            current_arena: None,
            current_file_path: None,
            arena_to_path: FxHashMap::default(),
            required_imports: FxHashMap::default(),
            reserved_names: FxHashSet::default(),
            import_string_aliases: FxHashMap::default(),
            import_symbol_map: FxHashMap::default(),
            import_name_map: FxHashMap::default(),
            symbol_module_specifier_cache: FxHashMap::default(),
            import_plan: ImportPlan::default(),
            inside_declare_namespace: false,
            in_constructor_params: false,
            function_names_with_overloads: FxHashSet::default(),
            class_has_constructor_overloads: false,
            method_names_with_overloads: FxHashSet::default(),
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

    /// Build a map of imported `SymbolId` -> `ModuleSpecifier` for elision.
    ///
    /// Walks all import statements and tracks which module each imported
    /// symbol claims to come from. This enables elision of unused imports.
    fn prepare_import_metadata(&mut self, root_idx: NodeIndex) {
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

    /// Collect all imported symbols from an `ImportClause`.
    ///
    /// Returns a Vec of (name, `SymbolId`) pairs that were found in the import clause.
    pub(super) fn collect_imported_symbols_from_clause(
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
            // Process each specifier
            for &spec_idx in &bindings.elements.nodes {
                if let Some(spec) = arena.get_specifier_at(spec_idx) {
                    // Use the property_name if present (for 'as' imports), otherwise use name
                    let name_idx = if spec.property_name.is_some() {
                        spec.property_name
                    } else {
                        spec.name
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

    /// Emit declaration for a source file
    pub fn emit(&mut self, root_idx: NodeIndex) -> String {
        // Reset per-file emission state
        self.used_symbols = None;
        self.foreign_symbols = None;
        self.import_name_map.clear();
        self.import_symbol_map.clear();
        self.import_string_aliases.clear();
        self.reserved_names.clear();
        self.symbol_module_specifier_cache.clear();
        self.import_plan = ImportPlan::default();

        self.reset_writer();
        self.indent_level = 0;

        // Prepare import metadata for elision BEFORE running UsageAnalyzer
        // This builds the SymbolId -> ModuleSpecifier map from existing imports
        self.prepare_import_metadata(root_idx);

        // Run usage analyzer if we have all required components AND haven't run yet
        if self.used_symbols.is_none() {
            debug!(
                "[DEBUG] emit: type_cache.is_none()={}",
                self.type_cache.is_none()
            );
            debug!(
                "[DEBUG] emit: type_interner.is_none()={}",
                self.type_interner.is_none()
            );
            debug!(
                "[DEBUG] emit: current_arena.is_none()={}",
                self.current_arena.is_none()
            );

            if let (Some(cache), Some(interner), Some(binder), Some(current_arena)) = (
                &self.type_cache,
                self.type_interner,
                self.binder,
                &self.current_arena,
            ) {
                debug!(
                    "[DEBUG] emit: import_name_map has {} entries: {:?}",
                    self.import_name_map.len(),
                    self.import_name_map
                );
                let mut analyzer = super::usage_analyzer::UsageAnalyzer::new(
                    self.arena,
                    binder,
                    cache,
                    interner,
                    std::sync::Arc::clone(current_arena),
                    &self.import_name_map,
                );
                let used = analyzer.analyze(root_idx).clone();
                let foreign = analyzer.get_foreign_symbols();
                debug!(
                    "[DEBUG] emit: foreign_symbols has {} symbols",
                    foreign.len()
                );
                self.used_symbols = Some(used);
                self.foreign_symbols = Some(foreign.clone());
            }
        }

        // Prepare aliases and build the import plan before emitting anything
        self.prepare_import_aliases(root_idx);
        self.prepare_import_plan();

        let Some(root_node) = self.arena.get(root_idx) else {
            return String::new();
        };

        let Some(source_file) = self.arena.get_source_file(root_node) else {
            return String::new();
        };

        self.source_file_text = Some(source_file.text.clone());
        self.source_is_declaration_file = source_file.is_declaration_file;
        self.emit_public_api_only = self.has_public_api_exports(source_file);

        debug!(
            "[DEBUG] source_file has {} comments",
            source_file.comments.len()
        );

        // Emit triple-slash directives at the very top (before imports)
        self.emit_triple_slash_directives(source_file);

        // Emit required imports first (before other declarations)
        self.emit_required_imports();

        // Emit auto-generated imports for foreign symbols
        self.emit_auto_imports();

        for &stmt_idx in &source_file.statements.nodes {
            self.emit_statement(stmt_idx);
        }

        if self.needs_empty_export_marker(source_file) {
            self.write("export {};");
        }

        self.writer.get_output().to_string()
    }

    /// Emits triple-slash directives at the top of the .d.ts file.
    ///
    /// TypeScript uses triple-slash directives for:
    /// - File references: `/// <reference path="other.ts" />`
    /// - Type references: `/// <reference types="node" />`
    /// - Lib references: `/// <reference lib="es2015" />`
    /// - AMD directives: `/// <amd-module />`, `/// <amd-dependency />`
    ///
    /// These must appear at the very top of the file, before any imports or declarations.
    fn emit_triple_slash_directives(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        for comment in &source_file.comments {
            // Extract the comment text from the source file
            let text = &source_file.text[comment.pos as usize..comment.end as usize];

            // Triple-slash directives start with ///
            if let Some(stripped) = text.strip_prefix("///") {
                let trimmed = stripped.trim_start();

                // Check if this is a directive we should preserve
                if trimmed.starts_with("<reference")
                    || trimmed.starts_with("<amd-module")
                    || trimmed.starts_with("<amd-dependency")
                {
                    self.write(text);
                    self.write_line();
                }
            }
        }
    }

    pub(super) fn emit_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let before_len = self.writer.len();
        self.queue_source_mapping(stmt_node);

        let kind = stmt_node.kind;
        debug!(
            "[DEBUG STATEMENT] kind={}, syntax_kind_ext::EXPORT_ASSIGNMENT={}",
            kind,
            syntax_kind_ext::EXPORT_ASSIGNMENT
        );

        match kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.emit_function_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.emit_class_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                self.emit_interface_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                self.emit_type_alias_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.emit_enum_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.emit_variable_declaration_statement(stmt_idx);
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                self.emit_export_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                self.emit_export_assignment(stmt_idx);
            }
            k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                // Skip emitting import declarations here - they're handled by import elision
                // via emit_auto_imports() which only emits imports for symbols that are actually used
                // The import_symbol_map tracks which imports are part of the elision system
                // We still need to emit declarations that are NOT in import_symbol_map (but those should be rare)
                self.emit_import_declaration_if_needed(stmt_idx);
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.emit_module_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                self.emit_import_equals_declaration(stmt_idx, false);
            }
            k if k == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION => {
                self.emit_namespace_export_declaration(stmt_idx);
            }
            _ => {}
        }

        if self.writer.len() == before_len {
            self.pending_source_pos = None;
        }
    }

    fn emit_function_declaration(&mut self, func_idx: NodeIndex) {
        let Some(func_node) = self.arena.get(func_idx) else {
            return;
        };
        let Some(func) = self.arena.get_function(func_node) else {
            return;
        };

        // Check for export modifier
        let is_exported = self
            .arena
            .has_modifier(&func.modifiers, SyntaxKind::ExportKeyword);

        if !self.should_emit_public_api_member(&func.modifiers)
            && !self.should_emit_public_api_dependency(func.name)
        {
            return;
        }

        // Get function name as string for overload tracking
        let function_name = self.get_function_name(func_idx);

        // Check if this is an overload (no body) or implementation (has body)
        let is_overload = func.body.is_none();
        let is_implementation = !is_overload;

        // Overload handling:
        // - If this is an overload, emit it and mark that this function has overloads
        // - If this is an implementation and the function already has overloads, skip it
        // - If this is an implementation with no overloads, emit it
        if is_overload {
            // Mark that this function name has overload signatures
            if let Some(ref name) = function_name {
                self.function_names_with_overloads.insert(name.clone());
            }
        } else if is_implementation {
            // This is an implementation - check if we've seen overloads for this name
            if let Some(ref name) = function_name
                && self.function_names_with_overloads.contains(name)
            {
                // Skip implementation signature when overloads exist
                return;
            }
        }

        self.write_indent();
        if !self.inside_declare_namespace {
            if is_exported {
                self.write("export ");
            }
            self.write("declare ");
        }
        self.write("function ");

        // Function name
        self.emit_node(func.name);

        // Type parameters
        if let Some(ref type_params) = func.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        // Parameters
        self.write("(");
        self.emit_parameters(&func.parameters);
        self.write(")");

        // Return type
        let func_body = func.body;
        let func_name = func.name;
        if func.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(func.type_annotation);
        } else if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache) {
            // No explicit return type, try to infer it
            let func_type_id = cache
                .node_types
                .get(&func_idx.0)
                .copied()
                .or_else(|| self.get_node_type_or_names(&[func_name]));

            if let Some(func_type_id) = func_type_id
                && let Some(return_type_id) = type_queries::get_return_type(*interner, func_type_id)
            {
                self.write(": ");
                self.write(&self.print_type_id(return_type_id));
            } else if func_body.is_some() && self.body_returns_void(func_body) {
                self.write(": void");
            }
        } else if func_body.is_some() && self.body_returns_void(func_body) {
            // No type cache available, but we can check the body
            self.write(": void");
        }

        self.write(";");
        self.write_line();
    }

    fn emit_class_declaration(&mut self, class_idx: NodeIndex) {
        let Some(class_node) = self.arena.get(class_idx) else {
            return;
        };
        let Some(class) = self.arena.get_class(class_node) else {
            return;
        };

        let is_exported = self
            .arena
            .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword);
        if !self.should_emit_public_api_member(&class.modifiers)
            && !self.should_emit_public_api_dependency(class.name)
        {
            return;
        }
        let is_abstract = self
            .arena
            .has_modifier(&class.modifiers, SyntaxKind::AbstractKeyword);

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if !self.inside_declare_namespace {
            self.write("declare ");
        }
        if is_abstract {
            self.write("abstract ");
        }
        self.write("class ");

        // Class name
        self.emit_node(class.name);

        // Type parameters
        if let Some(ref type_params) = class.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        // Heritage clauses (extends, implements)
        if let Some(ref heritage) = class.heritage_clauses {
            self.emit_heritage_clauses(heritage);
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();

        // Reset constructor and method overload tracking for this class
        self.class_has_constructor_overloads = false;
        self.method_names_with_overloads = FxHashSet::default();

        // Emit parameter properties from constructor first (before other members)
        self.emit_parameter_properties(&class.members);

        // Emit `#private;` if any member has a private identifier name (e.g., #foo)
        if self.class_has_private_identifier_member(&class.members) {
            self.write_indent();
            self.write("#private;");
            self.write_line();
        }

        // Members
        for &member_idx in &class.members.nodes {
            self.emit_class_member(member_idx);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    pub(super) fn emit_class_member(&mut self, member_idx: NodeIndex) {
        let Some(member_node) = self.arena.get(member_idx) else {
            return;
        };

        // Skip members with private identifier names (#foo) - these are replaced by `#private;`
        if self.member_has_private_identifier_name(member_idx) {
            return;
        }

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                self.emit_property_declaration(member_idx);
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                self.emit_method_declaration(member_idx);
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                self.emit_constructor_declaration(member_idx);
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                self.emit_accessor_declaration(member_idx, true);
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                self.emit_accessor_declaration(member_idx, false);
            }
            k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                self.emit_index_signature(member_idx);
            }
            _ => {}
        }
    }

    /// Check if a member has a private identifier (#foo) name.
    fn member_has_private_identifier_name(&self, member_idx: NodeIndex) -> bool {
        let Some(member_node) = self.arena.get(member_idx) else {
            return false;
        };
        let name_idx = if let Some(prop) = self.arena.get_property_decl(member_node) {
            Some(prop.name)
        } else if let Some(method) = self.arena.get_method_decl(member_node) {
            Some(method.name)
        } else {
            self.arena
                .get_accessor(member_node)
                .map(|accessor| accessor.name)
        };
        if let Some(name_idx) = name_idx
            && let Some(name_node) = self.arena.get(name_idx)
        {
            return name_node.kind == SyntaxKind::PrivateIdentifier as u16;
        }
        false
    }

    fn emit_property_declaration(&mut self, prop_idx: NodeIndex) {
        let Some(prop_node) = self.arena.get(prop_idx) else {
            return;
        };
        let Some(prop) = self.arena.get_property_decl(prop_node) else {
            return;
        };

        self.write_indent();

        // Check if abstract for special handling
        let is_abstract = self
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword);
        // Check if private for type annotation omission
        let is_private = self
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::PrivateKeyword);

        // Modifiers
        self.emit_member_modifiers(&prop.modifiers);

        // Name
        self.emit_node(prop.name);

        // Optional marker
        if prop.question_token {
            self.write("?");
        }

        // Type - use explicit annotation if present, otherwise use inferred type
        // SPECIAL CASE: For private properties, TypeScript omits type annotations in .d.ts
        if prop.type_annotation.is_some() && !is_private {
            self.write(": ");
            self.emit_type(prop.type_annotation);
        } else if !is_private && (is_abstract || prop.initializer.is_some()) {
            // For abstract properties OR properties with initializers (non-private), use inferred type
            // Private properties never get inferred types (prevents type leak)
            if let Some(type_id) = self.get_node_type_or_names(&[prop_idx, prop.name]) {
                self.write(": ");
                self.write(&self.print_type_id(type_id));
            } else if let Some(type_text) = self.infer_fallback_type_text(prop.initializer) {
                self.write(": ");
                self.write(&type_text);
            }
        }

        self.write(";");
        self.write_line();
    }

    fn emit_method_declaration(&mut self, method_idx: NodeIndex) {
        let Some(method_node) = self.arena.get(method_idx) else {
            return;
        };
        let Some(method) = self.arena.get_method_decl(method_node) else {
            return;
        };

        // Get method name as string for overload tracking
        let method_name = self.get_function_name(method_idx);

        // Check if this is an overload (no body) or implementation (has body)
        let is_overload = method.body.is_none();
        let is_implementation = !is_overload;

        // Check if private
        let is_private = self
            .arena
            .has_modifier(&method.modifiers, SyntaxKind::PrivateKeyword);

        // Method overload handling:
        // - If this is an overload, emit it and mark that this method has overloads
        // - If this is an implementation and the method already has overloads, skip it
        // - If this is an implementation with no overloads, emit it
        // SPECIAL: For private methods with overloads, emit just `private foo;`
        if is_overload {
            // Mark that this method name has overload signatures
            if let Some(ref name) = method_name {
                self.method_names_with_overloads.insert(name.clone());
            }
            // For private methods, skip all overload signatures
            // (will emit single `private foo;` at implementation)
            if is_private {
                return;
            }
        } else if is_implementation {
            // This is an implementation - check if we've seen overloads for this name
            if let Some(ref name) = method_name
                && self.method_names_with_overloads.contains(name)
            {
                if is_private {
                    // Private method with overloads: emit just `private foo;`
                    self.write_indent();
                    self.write("private ");
                    self.emit_node(method.name);
                    self.write(";");
                    self.write_line();
                }
                // Skip implementation signature when overloads exist
                return;
            }
        }

        self.write_indent();

        // Modifiers
        self.emit_member_modifiers(&method.modifiers);

        // Name
        self.emit_node(method.name);

        // For private methods (no overloads), emit just the name without signature
        if is_private {
            self.write(";");
            self.write_line();
            return;
        }

        // Type parameters
        if let Some(ref type_params) = method.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        // Parameters
        self.write("(");
        self.emit_parameters(&method.parameters);
        self.write(")");

        // Return type - SPECIAL CASE: For private methods, TypeScript omits return type in .d.ts
        let method_body = method.body;
        let method_name = method.name;
        if method.type_annotation.is_some() && !is_private {
            self.write(": ");
            self.emit_type(method.type_annotation);
        } else if !is_private
            && let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache)
        {
            let method_type_id = cache
                .node_types
                .get(&method_idx.0)
                .copied()
                .or_else(|| self.get_node_type_or_names(&[method_name]));

            if let Some(method_type_id) = method_type_id
                && let Some(return_type_id) =
                    type_queries::get_return_type(*interner, method_type_id)
            {
                self.write(": ");
                self.write(&self.print_type_id(return_type_id));
            } else if method_body.is_some() && self.body_returns_void(method_body) {
                self.write(": void");
            }
        } else if !is_private && method_body.is_some() && self.body_returns_void(method_body) {
            self.write(": void");
        }

        self.write(";");
        self.write_line();
    }

    fn emit_constructor_declaration(&mut self, ctor_idx: NodeIndex) {
        let Some(ctor_node) = self.arena.get(ctor_idx) else {
            return;
        };
        let Some(ctor) = self.arena.get_constructor(ctor_node) else {
            return;
        };

        // Check if this is an overload (no body) or implementation (has body)
        let is_overload = ctor.body.is_none();
        let is_implementation = !is_overload;

        // Constructor overload handling:
        // - If this is an overload, emit it and mark that the class has constructor overloads
        // - If this is an implementation and the class already has constructor overloads, skip it
        // - If this is an implementation with no overloads, emit it
        if is_overload {
            // Mark that this class has constructor overloads
            self.class_has_constructor_overloads = true;
        } else if is_implementation {
            // This is an implementation - check if we've seen constructor overloads
            if self.class_has_constructor_overloads {
                // Skip implementation constructor when overloads exist
                return;
            }
        }

        self.write_indent();

        // Emit visibility modifiers (private, protected) on the constructor
        if let Some(ref mods) = ctor.modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    match mod_node.kind {
                        k if k == SyntaxKind::PrivateKeyword as u16 => self.write("private "),
                        k if k == SyntaxKind::ProtectedKeyword as u16 => self.write("protected "),
                        _ => {}
                    }
                }
            }
        }

        self.write("constructor(");
        // tsc strips parameters from private constructors in .d.ts output
        let is_private = ctor.modifiers.as_ref().is_some_and(|mods| {
            mods.nodes.iter().any(|&mod_idx| {
                self.arena
                    .get(mod_idx)
                    .is_some_and(|n| n.kind == SyntaxKind::PrivateKeyword as u16)
            })
        });
        if !is_private {
            // Set flag to strip accessibility modifiers from constructor parameters
            self.in_constructor_params = true;
            self.emit_parameters(&ctor.parameters);
            self.in_constructor_params = false;
        }
        self.write(");");
        self.write_line();
    }

    /// Emit parameter properties from constructor as class properties
    /// Parameter properties (e.g., `constructor(public x: number)`) should be emitted
    /// as property declarations in the class body, then stripped from constructor params
    pub(super) fn emit_parameter_properties(&mut self, members: &tsz_parser::parser::NodeList) {
        use tsz_scanner::SyntaxKind;

        // Find the constructor
        let ctor_idx = members.nodes.iter().find(|&&idx| {
            self.arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::CONSTRUCTOR)
        });

        let Some(&ctor_idx) = ctor_idx else {
            return;
        };

        let Some(ctor_node) = self.arena.get(ctor_idx) else {
            return;
        };
        let Some(ctor) = self.arena.get_constructor(ctor_node) else {
            return;
        };

        // Emit parameter properties
        for &param_idx in &ctor.parameters.nodes {
            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_parameter(param_node)
            {
                // Check if parameter has accessibility modifiers or readonly
                let has_modifier = param.modifiers.as_ref().is_some_and(|mods| {
                    mods.nodes.iter().any(|&mod_idx| {
                        if let Some(mod_node) = self.arena.get(mod_idx) {
                            let k = mod_node.kind;
                            k == SyntaxKind::PublicKeyword as u16
                                || k == SyntaxKind::PrivateKeyword as u16
                                || k == SyntaxKind::ProtectedKeyword as u16
                                || k == SyntaxKind::ReadonlyKeyword as u16
                                || k == SyntaxKind::OverrideKeyword as u16
                        } else {
                            false
                        }
                    })
                });

                if has_modifier {
                    // Emit as a property declaration
                    self.write_indent();

                    // Track if we have private modifier (special handling: no type annotation)
                    let mut is_private = false;

                    // Emit modifiers (keep readonly, strip accessibility in property)
                    if let Some(ref modifiers) = param.modifiers {
                        for &mod_idx in &modifiers.nodes {
                            if let Some(mod_node) = self.arena.get(mod_idx) {
                                match mod_node.kind {
                                    k if k == SyntaxKind::PrivateKeyword as u16 => {
                                        self.write("private ");
                                        is_private = true;
                                    }
                                    k if k == SyntaxKind::ProtectedKeyword as u16 => {
                                        self.write("protected ");
                                    }
                                    k if k == SyntaxKind::ReadonlyKeyword as u16 => {
                                        self.write("readonly ");
                                    }
                                    // Skip public - it's the default and omitted
                                    _ => {}
                                }
                            }
                        }
                    }

                    // Parameter name
                    self.emit_node(param.name);

                    // Optional
                    if param.question_token {
                        self.write("?");
                    }

                    // Type annotation (omit for private properties, include for others)
                    if !is_private && param.type_annotation.is_some() {
                        self.write(": ");
                        self.emit_type(param.type_annotation);
                    }

                    // Note: No initializer for parameter properties in .d.ts
                    self.write(";");
                    self.write_line();
                }
            }
        }
    }

    fn emit_accessor_declaration(&mut self, accessor_idx: NodeIndex, is_getter: bool) {
        let Some(accessor_node) = self.arena.get(accessor_idx) else {
            return;
        };
        let Some(accessor) = self.arena.get_accessor(accessor_node) else {
            return;
        };

        // Check if this accessor is private
        let is_private = self
            .arena
            .has_modifier(&accessor.modifiers, SyntaxKind::PrivateKeyword);

        self.write_indent();

        // Modifiers
        self.emit_member_modifiers(&accessor.modifiers);

        if is_getter {
            self.write("get ");
        } else {
            self.write("set ");
        }

        // Name
        self.emit_node(accessor.name);

        // Parameters - omit types for private accessors
        self.write("(");
        if is_private && !is_getter {
            // TypeScript emits a canonical `value` identifier for private setters in `.d.ts`
            // and intentionally strips the source identifier.
            if let Some(first_param_idx) = accessor.parameters.nodes.first()
                && let Some(first_param_node) = self.arena.get(*first_param_idx)
                && let Some(first_param) = self.arena.get_parameter(first_param_node)
            {
                if first_param.dot_dot_dot_token {
                    self.write("...");
                }

                self.write("value");

                if first_param.question_token {
                    self.write("?");
                }
            }
        } else {
            self.emit_parameters_without_types(&accessor.parameters, is_private);
        }
        self.write(")");

        // Return type (for getters) - omit for private accessors
        if is_getter && !is_private && accessor.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(accessor.type_annotation);
        } else if is_getter
            && !is_private
            && let Some(type_id) = self.get_node_type_or_names(&[accessor_idx, accessor.name])
        {
            self.write(": ");
            self.write(&self.print_type_id(type_id));
        }

        self.write(";");
        self.write_line();
    }

    fn emit_index_signature(&mut self, sig_idx: NodeIndex) {
        let Some(sig_node) = self.arena.get(sig_idx) else {
            return;
        };
        let Some(sig) = self.arena.get_index_signature(sig_node) else {
            return;
        };

        self.write_indent();

        // Modifiers
        self.emit_member_modifiers(&sig.modifiers);

        self.write("[");
        self.emit_parameters(&sig.parameters);
        self.write("]");

        if sig.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(sig.type_annotation);
        }

        self.write(";");
        self.write_line();
    }

    fn emit_type_alias_declaration(&mut self, alias_idx: NodeIndex) {
        let Some(alias_node) = self.arena.get(alias_idx) else {
            return;
        };
        let Some(alias) = self.arena.get_type_alias(alias_node) else {
            return;
        };

        let is_exported = self
            .arena
            .has_modifier(&alias.modifiers, SyntaxKind::ExportKeyword);
        if !self.should_emit_public_api_member(&alias.modifiers)
            && !self.should_emit_public_api_dependency(alias.name)
        {
            return;
        }

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        self.write("type ");

        // Name
        self.emit_node(alias.name);

        // Type parameters
        if let Some(ref type_params) = alias.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        self.write(" = ");
        self.emit_type(alias.type_node);
        self.write(";");
        self.write_line();
    }

    fn emit_enum_declaration(&mut self, enum_idx: NodeIndex) {
        let Some(enum_node) = self.arena.get(enum_idx) else {
            return;
        };
        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return;
        };

        let is_exported = self
            .arena
            .has_modifier(&enum_data.modifiers, SyntaxKind::ExportKeyword);
        if !self.should_emit_public_api_member(&enum_data.modifiers)
            && !self.should_emit_public_api_dependency(enum_data.name)
        {
            return;
        }
        let is_const = self
            .arena
            .has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword);

        self.write_indent();
        if !self.inside_declare_namespace {
            if is_exported {
                self.write("export ");
            }
            self.write("declare ");
        }
        if is_const {
            self.write("const ");
        }
        self.write("enum ");

        self.emit_node(enum_data.name);

        self.write(" {");
        self.write_line();
        self.increase_indent();

        // Evaluate enum member values to get correct auto-increment behavior
        let mut evaluator = EnumEvaluator::new(self.arena);
        let member_values = evaluator.evaluate_enum(enum_idx);

        for (i, &member_idx) in enum_data.members.nodes.iter().enumerate() {
            self.write_indent();
            if let Some(member_node) = self.arena.get(member_idx)
                && let Some(member) = self.arena.get_enum_member(member_node)
            {
                self.emit_node(member.name);
                let member_name = self.get_enum_member_name(member.name);
                if let Some(value) = member_values.get(&member_name) {
                    match value {
                        crate::enums::evaluator::EnumValue::Computed => {
                            // Computed values: no initializer in .d.ts
                        }
                        _ => {
                            self.write(" = ");
                            self.emit_enum_value(value);
                        }
                    }
                } else {
                    // Fallback to index if evaluation failed
                    self.write(" = ");
                    self.write(&i.to_string());
                }
            }
            if i < enum_data.members.nodes.len() - 1 {
                self.write(",");
            }
            self.write_line();
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    /// Check if an initializer expression is a `Symbol()` call (for unique symbol detection)
    pub(super) fn is_symbol_call(&self, initializer: NodeIndex) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };

        // Check if it's a call expression
        if init_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }

        let Some(call_expr) = self.arena.get_call_expr(init_node) else {
            return false;
        };

        // Check if the function being called is named "Symbol"
        let Some(expr_node) = self.arena.get(call_expr.expression) else {
            return false;
        };

        // Handle both simple identifiers and property access like global.Symbol
        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(ident) = self.arena.get_identifier(expr_node) {
                    return ident.escaped_text == "Symbol";
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                // Handle things like global.Symbol or Symbol.constructor
                if let Some(prop_access) = self.arena.get_access_expr(expr_node) {
                    // Check if the property name is "Symbol"
                    let Some(name_node) = self.arena.get(prop_access.name_or_argument) else {
                        return false;
                    };
                    if let Some(ident) = self.arena.get_identifier(name_node) {
                        return ident.escaped_text == "Symbol";
                    }
                }
            }
            _ => {}
        }

        false
    }

    /// Check if a `PrefixUnaryExpression` node is a negative numeric/bigint literal (e.g., `-123`, `-12n`)
    pub(super) fn is_negative_literal(&self, node: &tsz_parser::parser::node::Node) -> bool {
        if let Some(unary) = self.arena.get_unary_expr(node)
            && unary.operator == SyntaxKind::MinusToken as u16
            && let Some(operand_node) = self.arena.get(unary.operand)
        {
            let k = operand_node.kind;
            return k == SyntaxKind::NumericLiteral as u16 || k == SyntaxKind::BigIntLiteral as u16;
        }
        false
    }

    /// Check if a class has any member with a `#private` identifier name.
    /// TypeScript collapses all private-name members into a single `#private;` field.
    pub(super) fn class_has_private_identifier_member(
        &self,
        members: &tsz_parser::parser::NodeList,
    ) -> bool {
        for &member_idx in &members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            // Check property declarations
            if let Some(prop) = self.arena.get_property_decl(member_node)
                && let Some(name_node) = self.arena.get(prop.name)
                && name_node.kind == SyntaxKind::PrivateIdentifier as u16
            {
                return true;
            }
            // Check method declarations
            if let Some(method) = self.arena.get_method_decl(member_node)
                && let Some(name_node) = self.arena.get(method.name)
                && name_node.kind == SyntaxKind::PrivateIdentifier as u16
            {
                return true;
            }
            // Check accessors
            if let Some(accessor) = self.arena.get_accessor(member_node)
                && let Some(name_node) = self.arena.get(accessor.name)
                && name_node.kind == SyntaxKind::PrivateIdentifier as u16
            {
                return true;
            }
        }
        false
    }

    /// Check if a function body has any return statements with value expressions.
    /// Returns true if all returns are bare `return;` or there are no return statements,
    /// meaning the function effectively returns void.
    pub(super) fn body_returns_void(&self, body_idx: NodeIndex) -> bool {
        let Some(body_node) = self.arena.get(body_idx) else {
            return true;
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return true;
        };
        self.block_returns_void(&block.statements)
    }

    fn block_returns_void(&self, statements: &tsz_parser::parser::NodeList) -> bool {
        for &stmt_idx in &statements.nodes {
            if !self.stmt_returns_void(stmt_idx) {
                return false;
            }
        }
        true
    }

    fn stmt_returns_void(&self, stmt_idx: NodeIndex) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return true;
        };
        match stmt_node.kind {
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                // Return with expression → non-void
                if let Some(ret) = self.arena.get_return_statement(stmt_node) {
                    return ret.expression.is_none();
                }
                true
            }
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.arena.get_block(stmt_node) {
                    self.block_returns_void(&block.statements)
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_data) = self.arena.get_if_statement(stmt_node) {
                    // Must check both branches; an if without else can still
                    // contain `return expr;` in the then-branch
                    self.stmt_returns_void(if_data.then_statement)
                        && (if_data.else_statement.is_none()
                            || self.stmt_returns_void(if_data.else_statement))
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.arena.get_try(stmt_node) {
                    self.stmt_returns_void(try_data.try_block)
                        && (try_data.catch_clause.is_none()
                            || self.stmt_returns_void(try_data.catch_clause))
                        && (try_data.finally_block.is_none()
                            || self.stmt_returns_void(try_data.finally_block))
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_data) = self.arena.get_catch_clause(stmt_node) {
                    self.stmt_returns_void(catch_data.block)
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
                if let Some(clause) = self.arena.get_case_clause(stmt_node) {
                    self.block_returns_void(&clause.statements)
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                // Check all case clauses inside the switch's case block
                if let Some(switch_data) = self.arena.get_switch(stmt_node) {
                    if let Some(case_block_node) = self.arena.get(switch_data.case_block)
                        && let Some(block) = self.arena.get_block(case_block_node)
                    {
                        self.block_returns_void(&block.statements)
                    } else {
                        true
                    }
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT =>
            {
                if let Some(loop_data) = self.arena.get_loop(stmt_node) {
                    self.stmt_returns_void(loop_data.statement)
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT =>
            {
                if let Some(for_data) = self.arena.get_for_in_of(stmt_node) {
                    self.stmt_returns_void(for_data.statement)
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = self.arena.get_labeled_statement(stmt_node) {
                    self.stmt_returns_void(labeled.statement)
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::WITH_STATEMENT => {
                if let Some(with_data) = self.arena.get_with_statement(stmt_node) {
                    // with_statement stores its body in then_statement
                    self.stmt_returns_void(with_data.then_statement)
                } else {
                    true
                }
            }
            // Non-compound statements (expression statements, variable declarations, etc.)
            // cannot contain return statements, so they're void-safe.
            _ => true,
        }
    }

    fn emit_variable_declaration_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
            return;
        };

        let is_exported = self
            .arena
            .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword);
        if !self.should_emit_public_api_member(&var_stmt.modifiers) {
            // Check if any individual variable is referenced by the public API
            let has_dependency = var_stmt.declarations.nodes.iter().any(|&decl_list_idx| {
                if let Some(decl_list_node) = self.arena.get(decl_list_idx)
                    && decl_list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                    && let Some(decl_list) = self.arena.get_variable(decl_list_node)
                {
                    decl_list.declarations.nodes.iter().any(|&decl_idx| {
                        if let Some(decl_node) = self.arena.get(decl_idx)
                            && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                        {
                            self.should_emit_public_api_dependency(decl.name)
                        } else {
                            false
                        }
                    })
                } else {
                    false
                }
            });
            if !has_dependency {
                return;
            }
        }

        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };

            if decl_list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                && let Some(decl_list) = self.arena.get_variable(decl_list_node)
            {
                // Determine let/const/var
                let flags = decl_list_node.flags as u32;
                let keyword = if flags & tsz_parser::parser::node_flags::CONST != 0 {
                    "const"
                } else if flags & tsz_parser::parser::node_flags::LET != 0 {
                    "let"
                } else {
                    "var"
                };

                // Separate destructuring from regular declarations
                let mut regular_decls = Vec::new();
                for &decl_idx in &decl_list.declarations.nodes {
                    if let Some(decl_node) = self.arena.get(decl_idx)
                        && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                    {
                        let name_node = self.arena.get(decl.name);
                        let is_destructuring = name_node.is_some()
                            && (name_node.unwrap().kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                || name_node.unwrap().kind
                                    == syntax_kind_ext::ARRAY_BINDING_PATTERN);

                        if is_destructuring {
                            // Emit destructuring as individual declarations
                            self.emit_flattened_variable_declaration(
                                decl.name,
                                keyword,
                                is_exported,
                            );
                        } else {
                            regular_decls.push((decl_idx, decl_node, decl));
                        }
                    }
                }

                // Emit all regular declarations together on one line
                if !regular_decls.is_empty() {
                    self.write_indent();
                    if !self.inside_declare_namespace {
                        if is_exported {
                            self.write("export ");
                        }
                        self.write("declare ");
                    }
                    self.write(keyword);
                    self.write(" ");

                    for (i, (decl_idx, _decl_node, decl)) in regular_decls.iter().enumerate() {
                        if i > 0 {
                            self.write(", ");
                        }

                        self.emit_node(decl.name);
                        self.emit_variable_decl_type_or_initializer(
                            keyword,
                            *decl_idx,
                            decl.name,
                            decl.type_annotation,
                            decl.initializer,
                            decl.type_annotation.is_some(),
                            decl.initializer.is_some(),
                        );
                    }

                    self.write(";");
                    self.write_line();
                }
            }
        }
    }

    /// Recursively collects all Identifier `NodeIndices` from a `BindingPattern`.
    ///
    /// This handles nested destructuring like `const { a: { b } } = obj;`
    /// by traversing into nested patterns and collecting all leaf identifiers.
    fn collect_bindings_recursive(&self, node_idx: NodeIndex, bindings: &mut Vec<NodeIndex>) {
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };

        match node.kind {
            // Leaf case: simple identifier
            k if k == SyntaxKind::Identifier as u16 => {
                bindings.push(node_idx);
            }
            // Recursive case: object or array binding pattern
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    for &element_idx in &pattern.elements.nodes {
                        // element_idx is the NodeIndex for the BindingElement
                        // Recurse into the BindingElement node
                        self.collect_bindings_recursive(element_idx, bindings);
                    }
                }
            }
            // BindingElement: recurse into its name (which might be a pattern)
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                if let Some(element) = self.arena.get_binding_element(node) {
                    self.collect_bindings_recursive(element.name, bindings);
                }
            }
            _ => {}
        }
    }

    /// Emits flattened variable declarations for destructuring patterns.
    ///
    /// In .d.ts files, destructuring like `export const { a, b } = obj;`
    /// must be flattened into individual declarations:
    /// `export declare const a: Type;`
    /// `export declare const b: Type;`
    pub(super) fn emit_flattened_variable_declaration(
        &mut self,
        pattern_idx: NodeIndex,
        keyword: &str,
        is_exported: bool,
    ) {
        let mut bindings = Vec::new();
        self.collect_bindings_recursive(pattern_idx, &mut bindings);

        for ident_idx in bindings {
            self.write_indent();
            if !self.inside_declare_namespace {
                if is_exported {
                    self.write("export ");
                }
                self.write("declare ");
            }
            self.write(keyword);
            self.write(" ");

            // Emit the identifier name
            self.emit_node(ident_idx);

            // Get the type of the specific identifier from the cache
            // The checker associates the inferred type with the Identifier node
            if let Some(type_id) = self.get_node_type(ident_idx) {
                self.write(": ");
                self.write(&self.print_type_id(type_id));
            } else {
                // Fallback to 'any' if no type information available
                self.write(": any");
            }

            self.write(";");
            self.write_line();
        }
    }

    // Export/import emission → exports.rs
    // Type emission and utility helpers → helpers.rs
}
