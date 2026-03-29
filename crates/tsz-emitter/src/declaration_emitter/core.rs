use crate::enums::evaluator::EnumEvaluator;
use crate::output::source_writer::{SourcePosition, SourceWriter};
use crate::type_cache_view::TypeCacheView;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tracing::debug;
use tsz_binder::{BinderState, SymbolId};
use tsz_common::comments::CommentRange;
use tsz_common::diagnostics::Diagnostic;
use tsz_parser::parser::node::{MethodDeclData, Node, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
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
    /// Whether the source file being lowered is JavaScript-like (.js/.jsx/.mjs/.cjs).
    pub(super) source_is_js_file: bool,
    /// If true, only emit declarations that are part of the public API surface.
    pub(super) emit_public_api_only: bool,
    /// Track whether we're currently emitting inside a public-API namespace/module.
    pub(super) public_api_scope_depth: u32,
    /// Raw source text for this source file, used for keyword fallback emission.
    pub(super) source_file_text: Option<Arc<str>>,
    /// Type cache for looking up inferred types
    pub(super) type_cache: Option<TypeCacheView>,
    /// Root source-file node for the current emit pass.
    pub(super) current_source_file_idx: Option<NodeIndex>,
    /// Type interner for printing types
    pub(super) type_interner: Option<&'a TypeInterner>,
    /// Binder state for symbol resolution (used by `UsageAnalyzer`)
    pub(super) binder: Option<&'a BinderState>,
    /// Precomputed export surface summary (replaces ad-hoc re-extraction).
    pub(super) export_surface: Option<tsz_binder::ExportSurface>,
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
    /// Symbol of the innermost enclosing namespace (for context-relative type names)
    pub(super) enclosing_namespace_symbol: Option<SymbolId>,
    /// Whether we're inside a non-ambient namespace (filter non-exported members)
    pub(super) inside_non_ambient_namespace: bool,
    /// Whether we're emitting constructor parameters (don't emit accessibility modifiers)
    pub(super) in_constructor_params: bool,
    /// Track function names that have overload signatures (to skip implementation signatures)
    pub(super) function_names_with_overloads: FxHashSet<String>,
    /// Track whether current class has constructor overloads (to skip implementation constructor)
    pub(super) class_has_constructor_overloads: bool,
    /// Track whether current class extends another class
    pub(super) class_extends_another: bool,
    /// Track method names that have overload signatures in current class (to skip implementation signatures)
    pub(super) method_names_with_overloads: FxHashSet<String>,
    pub(super) all_comments: Vec<CommentRange>,
    pub(super) comment_emit_idx: usize,
    /// When true, strip all comments from .d.ts output (--removeComments)
    pub(super) remove_comments: bool,
    /// When true, strip declarations annotated with `@internal` (--stripInternal)
    pub(super) strip_internal: bool,
    /// Set of absolute file paths whose source contains module augmentations.
    pub(super) files_with_augmentations: FxHashSet<String>,
    /// Tracks whether any non-exported declaration was actually emitted
    /// (used for deciding whether `export {};` scope fix marker is needed)
    pub(super) emitted_non_exported_declaration: bool,
    /// Tracks whether any export statement was emitted that acts as a scope marker
    /// (`ExportDeclaration` with named/namespace exports, `ExportAssignment`, `NamespaceExportDeclaration`)
    pub(super) emitted_scope_marker: bool,
    /// Tracks whether any module indicator was emitted in the output
    /// (exported declarations, imports, scope markers)
    pub(super) emitted_module_indicator: bool,
    /// When true, the current ambient module/namespace body has a mix of
    /// exported and non-exported members, so `export` keywords should be
    /// preserved even though `inside_declare_namespace` is true.
    pub(super) ambient_module_has_scope_marker: bool,
    /// Top-level JS bindings that are re-exported via a foldable `export { x }` clause.
    pub(super) js_named_export_names: FxHashSet<String>,
    /// Foldable JS named export clauses mapped to deferred local statements.
    pub(super) js_folded_named_export_statements: FxHashMap<NodeIndex, Vec<NodeIndex>>,
    /// JS local statements skipped at their original position and re-emitted at
    /// a later `export { ... }` clause to preserve declaration order.
    pub(super) js_deferred_named_export_statements: FxHashSet<NodeIndex>,
    /// Top-level JS bindings referenced by an explicit `export = name` assignment.
    pub(super) js_export_equals_names: FxHashSet<String>,
    /// JS `export = name` assignments already emitted ahead of their declaration.
    pub(super) emitted_js_export_equals_names: FxHashSet<String>,
    /// JS namespace-like alias exports synthesized from expando assignments such
    /// as `foo.default = foo` and `module.exports.Bar = Bar`.
    pub(super) js_namespace_export_aliases: FxHashMap<String, Vec<(String, String)>>,
    /// CJS export aliases for `exports.X = Y` / `module.exports.X = Y`.
    pub(super) js_cjs_export_aliases: Vec<(String, String)>,
    /// Statements consumed by CJS export alias collection.
    pub(super) js_cjs_export_alias_statements: FxHashSet<NodeIndex>,
    /// Deferred JS CommonJS `Root.prop = function(){}` statements re-emitted as
    /// top-level synthetic function declarations.
    /// The boolean marks whether the synthetic declaration should be exported.
    pub(super) js_deferred_function_export_statements:
        FxHashMap<NodeIndex, (NodeIndex, NodeIndex, bool)>,
    /// Deferred JS CommonJS `Root.prop = value` statements re-emitted as
    /// top-level synthetic value declarations.
    /// The boolean marks whether the synthetic declaration should be exported.
    pub(super) js_deferred_value_export_statements:
        FxHashMap<NodeIndex, (NodeIndex, NodeIndex, bool)>,
    /// Deferred JS CommonJS `Root.prototype.method = function(){}` statements
    /// re-emitted as a synthetic `declare class Root { method(): ... }`.
    pub(super) js_deferred_prototype_method_statements:
        FxHashMap<String, Vec<(NodeIndex, NodeIndex)>>,
    /// JS `Clazz.method.prop = value` statements re-emitted as merged
    /// `namespace Clazz { function method(); namespace method { ... } }`.
    pub(super) js_static_method_augmentation_statements:
        FxHashMap<NodeIndex, crate::declaration_emitter::helpers::JsStaticMethodAugmentationGroup>,
    /// Extra JS static-method augmentation statements folded into an earlier
    /// synthetic namespace emit.
    pub(super) js_skipped_static_method_augmentation_statements: FxHashSet<NodeIndex>,
    /// Static class method nodes suppressed from class emit because an
    /// augmentation statement re-emits them as namespace members.
    pub(super) js_augmented_static_method_nodes: FxHashSet<NodeIndex>,
    /// Consecutive JS re-export declarations that should be merged at the first statement.
    pub(super) js_grouped_reexports: FxHashMap<NodeIndex, Vec<NodeIndex>>,
    /// JS re-export declarations skipped because they are emitted by an earlier merged group.
    pub(super) js_skipped_reexports: FxHashSet<NodeIndex>,
    /// Synthetic JSDoc type aliases already emitted for the current file.
    pub(super) emitted_jsdoc_type_aliases: FxHashSet<String>,
    /// Local declarations emitted on-demand to support synthetic class base aliases.
    pub(super) emitted_synthetic_dependency_symbols: FxHashSet<SymbolId>,
    /// Diagnostics collected during declaration emit (e.g., TS2883 for non-portable types).
    pub(super) diagnostics: Vec<Diagnostic>,
    /// When true, skip TS2883 non-portable type reference checks.
    /// Set for node16/nodenext module modes where module resolution already
    /// enforces portability via the exports map (TS2307).
    pub(super) skip_portability_check: bool,
    pub(super) strict_null_checks: bool,
    /// Accumulated enum values from all previously-evaluated enums in this file.
    /// Persists across enum declarations so cross-enum references (e.g., `B.Y = A.X`)
    /// can be resolved.
    pub(super) all_enum_values:
        FxHashMap<String, FxHashMap<String, crate::enums::evaluator::EnumValue>>,
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
            js_export_equals_names: FxHashSet::default(),
            emitted_js_export_equals_names: FxHashSet::default(),
            js_namespace_export_aliases: FxHashMap::default(),
            js_cjs_export_aliases: Vec::new(),
            js_cjs_export_alias_statements: FxHashSet::default(),
            js_deferred_function_export_statements: FxHashMap::default(),
            js_deferred_value_export_statements: FxHashMap::default(),
            js_deferred_prototype_method_statements: FxHashMap::default(),
            js_static_method_augmentation_statements: FxHashMap::default(),
            js_skipped_static_method_augmentation_statements: FxHashSet::default(),
            js_augmented_static_method_nodes: FxHashSet::default(),
            js_grouped_reexports: FxHashMap::default(),
            js_skipped_reexports: FxHashSet::default(),
            emitted_jsdoc_type_aliases: FxHashSet::default(),
            emitted_synthetic_dependency_symbols: FxHashSet::default(),
            diagnostics: Vec::new(),
            skip_portability_check: false,
            strict_null_checks: false,
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
            js_export_equals_names: FxHashSet::default(),
            emitted_js_export_equals_names: FxHashSet::default(),
            js_namespace_export_aliases: FxHashMap::default(),
            js_cjs_export_aliases: Vec::new(),
            js_cjs_export_alias_statements: FxHashSet::default(),
            js_deferred_function_export_statements: FxHashMap::default(),
            js_deferred_value_export_statements: FxHashMap::default(),
            js_deferred_prototype_method_statements: FxHashMap::default(),
            js_static_method_augmentation_statements: FxHashMap::default(),
            js_skipped_static_method_augmentation_statements: FxHashSet::default(),
            js_augmented_static_method_nodes: FxHashSet::default(),
            js_grouped_reexports: FxHashMap::default(),
            js_skipped_reexports: FxHashSet::default(),
            emitted_jsdoc_type_aliases: FxHashSet::default(),
            emitted_synthetic_dependency_symbols: FxHashSet::default(),
            diagnostics: Vec::new(),
            skip_portability_check: false,
            strict_null_checks: false,
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

    /// Take diagnostics collected during declaration emit (e.g., TS2883).
    pub fn take_diagnostics(&mut self) -> Vec<Diagnostic> {
        Self::normalize_portability_diagnostics(std::mem::take(&mut self.diagnostics))
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

    fn normalize_portability_diagnostics(diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
        let mut canonical_sites = FxHashSet::default();
        let mut exact_seen = FxHashSet::default();
        let mut unique = Vec::new();

        for diagnostic in diagnostics {
            let exact_key = (
                diagnostic.code,
                diagnostic.file.clone(),
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.clone(),
            );
            if !exact_seen.insert(exact_key) {
                continue;
            }

            if diagnostic.code == 2883
                && let Some((first, second)) =
                    Self::parse_ts2883_named_reference_message(&diagnostic.message_text)
                && !Self::looks_like_module_path(&first)
                && Self::looks_like_module_path(&second)
            {
                canonical_sites.insert((
                    diagnostic.file.clone(),
                    diagnostic.start,
                    diagnostic.length,
                ));
            }

            unique.push(diagnostic);
        }

        unique
            .into_iter()
            .filter(|diagnostic| {
                if diagnostic.code != 2883 {
                    return true;
                }

                let Some((first, second)) =
                    Self::parse_ts2883_named_reference_message(&diagnostic.message_text)
                else {
                    return true;
                };

                if !Self::looks_like_module_path(&first) || Self::looks_like_module_path(&second) {
                    return true;
                }

                !canonical_sites.contains(&(
                    diagnostic.file.clone(),
                    diagnostic.start,
                    diagnostic.length,
                ))
            })
            .collect()
    }

    fn parse_ts2883_named_reference_message(message: &str) -> Option<(String, String)> {
        let prefix = "cannot be named without a reference to '";
        let start = message.find(prefix)? + prefix.len();
        let rest = &message[start..];
        let (first, tail) = rest.split_once("' from '")?;
        let (second, _) = tail.split_once('\'')?;
        Some((first.to_string(), second.to_string()))
    }

    fn looks_like_module_path(text: &str) -> bool {
        text.starts_with('.')
            || text.starts_with('/')
            || text.contains('/')
            || text.contains('\\')
            || text.contains("node_modules")
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
        self.emitted_non_exported_declaration = false;
        self.emitted_scope_marker = false;
        self.emitted_module_indicator = false;

        // Seed overload tracking from precomputed ExportSurface if available.
        // This replaces the incremental on-the-fly detection for top-level
        // functions, ensuring overload grouping is correct even if the surface
        // was built in a previous pass.
        if let Some(ref surface) = self.export_surface {
            self.function_names_with_overloads = surface.overloaded_functions.clone();
        }

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

        let Some(root_node) = self.arena.get(root_idx) else {
            return String::new();
        };

        let Some(source_file) = self.arena.get_source_file(root_node) else {
            return String::new();
        };

        if !self.source_file_is_js(source_file) {
            self.retain_synthetic_class_extends_alias_dependencies_in_statements(
                &source_file.statements,
            );
        }

        // Prepare aliases and build the import plan before emitting anything
        self.prepare_import_aliases(root_idx);
        self.prepare_import_plan();

        self.source_file_text = Some(source_file.text.clone());
        self.source_is_declaration_file = source_file.is_declaration_file;
        self.source_is_js_file = self.source_file_is_js(source_file);
        self.current_source_file_idx = Some(root_idx);
        // Prefer the pre-computed flag from ExportSurface when available;
        // fall back to the existing AST walk for JS files (which need
        // CommonJS-specific detection the surface doesn't cover yet).
        self.emit_public_api_only = if let Some(ref surface) = self.export_surface {
            if !self.source_is_js_file {
                surface.has_public_api_scope
            } else {
                self.has_public_api_exports(source_file)
            }
        } else {
            self.has_public_api_exports(source_file)
        };
        self.all_comments = source_file.comments.clone();
        self.comment_emit_idx = 0;
        let (js_named_export_names, folded_named_exports, deferred_named_exports) =
            self.collect_js_folded_named_exports(source_file);
        self.js_named_export_names = js_named_export_names;
        self.js_folded_named_export_statements = folded_named_exports;
        self.js_deferred_named_export_statements = deferred_named_exports;
        self.js_export_equals_names = self.collect_js_export_equals_names(source_file);
        self.emitted_js_export_equals_names.clear();
        let (
            js_commonjs_named_export_names,
            js_commonjs_named_function_exports,
            js_commonjs_named_value_exports,
        ) = self.collect_js_commonjs_named_exports(source_file);
        self.js_named_export_names
            .extend(js_commonjs_named_export_names);
        let (cjs_aliases, cjs_alias_stmts) = self.collect_js_cjs_export_aliases(source_file);
        self.js_cjs_export_aliases = cjs_aliases;
        self.js_cjs_export_alias_statements = cjs_alias_stmts;
        // Mark CJS alias local names as used so they survive usage analysis pruning.
        if let Some(ref binder) = self.binder {
            if let Some(ref mut used) = self.used_symbols {
                for (_export_name, local_name) in &self.js_cjs_export_aliases {
                    if let Some(sym_id) = binder.file_locals.get(local_name) {
                        used.entry(sym_id).or_insert(
                            crate::declaration_emitter::usage_analyzer::UsageKind::VALUE
                                | crate::declaration_emitter::usage_analyzer::UsageKind::TYPE,
                        );
                    }
                }
            }
        }
        self.js_namespace_export_aliases =
            self.collect_js_namespace_export_aliases(source_file, &self.js_export_equals_names);
        let js_commonjs_expando_declarations = self
            .collect_js_commonjs_expando_declarations(source_file, &self.js_export_equals_names);
        self.js_deferred_function_export_statements = js_commonjs_expando_declarations
            .function_statements
            .into_iter()
            .map(|(stmt_idx, (name_idx, initializer))| (stmt_idx, (name_idx, initializer, false)))
            .collect();
        self.js_deferred_function_export_statements.extend(
            js_commonjs_named_function_exports.into_iter().map(
                |(stmt_idx, (name_idx, initializer))| (stmt_idx, (name_idx, initializer, true)),
            ),
        );
        self.js_deferred_value_export_statements = js_commonjs_expando_declarations
            .value_statements
            .into_iter()
            .map(|(stmt_idx, (name_idx, initializer))| (stmt_idx, (name_idx, initializer, false)))
            .collect();
        self.js_deferred_value_export_statements.extend(
            js_commonjs_named_value_exports.into_iter().map(
                |(stmt_idx, (name_idx, initializer))| (stmt_idx, (name_idx, initializer, true)),
            ),
        );
        // Remove CJS export alias statements from deferred maps.
        for &stmt_idx in &self.js_cjs_export_alias_statements {
            self.js_deferred_function_export_statements
                .remove(&stmt_idx);
            self.js_deferred_value_export_statements.remove(&stmt_idx);
        }
        self.js_deferred_prototype_method_statements =
            js_commonjs_expando_declarations.prototype_methods;
        let js_static_method_augmentations =
            self.collect_js_class_static_method_augmentations(source_file);
        self.js_static_method_augmentation_statements = js_static_method_augmentations.statements;
        self.js_skipped_static_method_augmentation_statements =
            js_static_method_augmentations.skipped_statements;
        self.js_augmented_static_method_nodes =
            js_static_method_augmentations.augmented_method_nodes;
        let (grouped_reexports, skipped_reexports) = self.collect_js_grouped_reexports(source_file);
        self.js_grouped_reexports = grouped_reexports;
        self.js_skipped_reexports = skipped_reexports;
        self.emitted_jsdoc_type_aliases.clear();
        self.emitted_synthetic_dependency_symbols.clear();
        let deferred_js_namespace_objects =
            self.collect_js_namespace_object_statements(source_file);

        debug!(
            "[DEBUG] source_file has {} comments",
            source_file.comments.len()
        );

        // Emit detached copyright comments (/*! ... */) at the very top
        self.emit_detached_copyright_comments(source_file);

        // Emit triple-slash directives at the very top (before imports)
        self.emit_triple_slash_directives(source_file);

        // Emit required imports first (before other declarations)
        let before_imports = self.writer.len();
        self.emit_required_imports();

        // Emit auto-generated imports for foreign symbols
        self.emit_auto_imports();
        if self.writer.len() > before_imports {
            // Auto-generated imports count as external module indicators
            self.emitted_module_indicator = true;
        }

        // Emit CJS export aliases before declarations.
        self.emit_js_cjs_export_aliases();

        for &stmt_idx in &source_file.statements.nodes {
            if deferred_js_namespace_objects.contains(&stmt_idx) {
                continue;
            }
            if self.js_cjs_export_alias_statements.contains(&stmt_idx) {
                continue;
            }
            self.emit_statement(stmt_idx);
        }
        for &stmt_idx in &source_file.statements.nodes {
            if deferred_js_namespace_objects.contains(&stmt_idx) {
                self.emit_statement(stmt_idx);
            }
        }

        self.emit_pending_top_level_jsdoc_type_aliases(source_file);
        self.emit_pending_jsdoc_callback_type_aliases(source_file);
        self.emit_trailing_top_level_jsdoc_type_aliases(source_file);

        // Add `export {};` scope fix marker when needed (mirrors tsc's transformDeclarations).
        // Uses emission-time tracking instead of source-file analysis.
        //
        // tsc logic: if isExternalModule(node) &&
        //   (!resultHasExternalModuleIndicator || (needsScopeFixMarker && !resultHasScopeMarker))
        let is_module = self.source_file_has_module_syntax(source_file);

        if is_module
            && (!self.emitted_module_indicator
                || (self.emitted_non_exported_declaration && !self.emitted_scope_marker))
        {
            self.write("export {};");
            self.write_line();
        }

        self.writer.get_output().to_string()
    }

    /// Emits detached copyright comments (`/*! ... */`) at the top of the .d.ts file.
    ///
    /// TSC preserves `/*!` comments (copyright notices) at the very start of the file
    /// in declaration output, even when `--removeComments` is set.
    fn emit_detached_copyright_comments(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        // Find the position of the first statement
        let first_stmt_pos = source_file
            .statements
            .nodes
            .first()
            .and_then(|&idx| self.arena.get(idx))
            .map(|n| n.pos);

        for comment in &source_file.comments {
            // Only consider comments that appear before the first statement
            if let Some(stmt_pos) = first_stmt_pos
                && comment.pos >= stmt_pos
            {
                break;
            }

            // Only preserve /*! ... */ copyright comments
            if !comment.is_multi_line {
                continue;
            }
            let text = comment.get_text(&source_file.text);
            if !text.starts_with("/*!") {
                continue;
            }

            self.write(text);
            self.write_line();
        }
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
            let text = &source_file.text[comment.pos as usize..comment.end as usize];

            // Triple-slash directives start with ///
            if let Some(stripped) = text.strip_prefix("///") {
                let trimmed = stripped.trim_start();

                // Preserve `<amd-module>` and `<amd-dependency>` directives.
                // Also preserve `<reference>` directives that have `preserve="true"`.
                let should_emit = trimmed.starts_with("<amd-module")
                    || trimmed.starts_with("<amd-dependency")
                    || (trimmed.starts_with("<reference") && trimmed.contains("preserve=\"true\""));

                if should_emit {
                    // Normalize spacing to match tsc:
                    // 1. Ensure space after `///`: `///<reference` → `/// <reference`
                    // 2. Ensure space before `/>`: `/>` → ` />`
                    let mut normalized = if !stripped.starts_with(' ') {
                        format!("/// {}", stripped.trim_start())
                    } else {
                        text.to_string()
                    };
                    if normalized.ends_with("/>") && !normalized.ends_with(" />") {
                        let base = &normalized[..normalized.len() - 2];
                        normalized = format!("{base} />");
                    }
                    self.write(&normalized);
                    self.write_line();
                }
            }
        }
    }

    pub(super) fn emit_statement(&mut self, stmt_idx: NodeIndex) {
        self.emit_statement_with_options(stmt_idx, false);
    }

    pub(crate) fn emit_deferred_js_named_export_statement(&mut self, stmt_idx: NodeIndex) {
        self.emit_statement_with_options(stmt_idx, true);
    }

    fn emit_statement_with_options(
        &mut self,
        stmt_idx: NodeIndex,
        allow_deferred_js_named_export: bool,
    ) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };

        if !allow_deferred_js_named_export
            && self.js_deferred_named_export_statements.contains(&stmt_idx)
        {
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            return;
        }
        if self
            .js_skipped_static_method_augmentation_statements
            .contains(&stmt_idx)
        {
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            return;
        }

        let kind = stmt_node.kind;

        // For non-declaration statements (expression statements, assignments, etc.),
        // skip their comments entirely rather than emitting them as leading JSDoc.
        let has_synthetic_js_expression_declaration = kind == syntax_kind_ext::EXPRESSION_STATEMENT
            && self.has_synthetic_js_expression_declaration(stmt_idx);
        let is_declaration_kind = kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::CLASS_DECLARATION
            || kind == syntax_kind_ext::INTERFACE_DECLARATION
            || kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
            || kind == syntax_kind_ext::ENUM_DECLARATION
            || kind == syntax_kind_ext::VARIABLE_STATEMENT
            || kind == syntax_kind_ext::EXPORT_DECLARATION
            || kind == syntax_kind_ext::EXPORT_ASSIGNMENT
            || kind == syntax_kind_ext::IMPORT_DECLARATION
            || kind == syntax_kind_ext::MODULE_DECLARATION
            || kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            || kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
            || has_synthetic_js_expression_declaration;

        if !is_declaration_kind {
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            return;
        }

        let is_variable_like_export = kind == syntax_kind_ext::VARIABLE_STATEMENT
            || (kind == syntax_kind_ext::EXPORT_DECLARATION
                && self
                    .arena
                    .get_export_decl(stmt_node)
                    .and_then(|export| self.arena.get(export.export_clause))
                    .is_some_and(|clause| clause.kind == syntax_kind_ext::VARIABLE_STATEMENT));
        if !is_variable_like_export {
            self.emit_leading_jsdoc_type_aliases_for_pos(stmt_node.pos);
        }

        // Save position before JSDoc comments so we can undo them if the
        // declaration turns out to be invisible (non-exported in namespace, etc.)
        let before_jsdoc_len = self.writer.len();
        let saved_comment_idx = self.comment_emit_idx;
        self.emit_leading_jsdoc_comments(stmt_node.pos);
        let before_len = self.writer.len();
        self.queue_source_mapping(stmt_node);

        let has_effective_export = self.statement_has_effective_export(stmt_idx);
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
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                self.emit_js_synthetic_expression_statement(stmt_idx);
            }
            _ => unreachable!(),
        }

        let did_emit = self.writer.len() != before_len;
        if !did_emit {
            // The handler didn't emit anything (e.g., non-exported declaration in namespace).
            // Undo the speculatively emitted JSDoc comments and skip all comments in this
            // statement's range so they don't leak to the next declaration.
            self.writer.truncate(before_jsdoc_len);
            self.comment_emit_idx = saved_comment_idx;
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            self.pending_source_pos = None;
        } else {
            // Track whether we emitted a scope marker or a non-exported declaration.
            // This is used to decide whether `export {};` is needed at the end.
            let is_scope_marker = kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                || kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                || (kind == syntax_kind_ext::EXPORT_DECLARATION && {
                    // Only pure export declarations count as scope markers,
                    // not `export class/function/etc` which are declarations with export
                    self.arena
                        .get(stmt_idx)
                        .and_then(|n| self.arena.get_export_decl(n))
                        .and_then(|ed| self.arena.get(ed.export_clause))
                        .is_none_or(|clause| {
                            let ck = clause.kind;
                            ck != syntax_kind_ext::INTERFACE_DECLARATION
                                && ck != syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                && ck != syntax_kind_ext::CLASS_DECLARATION
                                && ck != syntax_kind_ext::FUNCTION_DECLARATION
                                && ck != syntax_kind_ext::ENUM_DECLARATION
                                && ck != syntax_kind_ext::VARIABLE_STATEMENT
                                && ck != syntax_kind_ext::MODULE_DECLARATION
                                && ck != syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                        })
                });

            if is_scope_marker {
                self.emitted_scope_marker = true;
                self.emitted_module_indicator = true;
            } else if has_effective_export
                || kind == syntax_kind_ext::EXPORT_DECLARATION
                || kind == syntax_kind_ext::IMPORT_DECLARATION
                || kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                // Any export/import statement is a module indicator
                self.emitted_module_indicator = true;
            }

            if !has_effective_export && kind != syntax_kind_ext::EXPORT_DECLARATION {
                // A declaration without export modifier was emitted.
                // Module augmentations (`declare global`, `declare module "foo"`)
                // are not regular declarations and should not trigger
                // the `export {};` scope-fix marker.
                let is_module_augmentation = kind == syntax_kind_ext::MODULE_DECLARATION
                    && self
                        .arena
                        .get(stmt_idx)
                        .and_then(|n| self.arena.get_module(n))
                        .and_then(|m| self.arena.get(m.name))
                        .is_some_and(|name_node| {
                            name_node.kind == SyntaxKind::StringLiteral as u16
                                || self
                                    .arena
                                    .get_identifier(name_node)
                                    .is_some_and(|id| id.escaped_text == "global")
                        });
                let is_declaration_kind = (kind == syntax_kind_ext::FUNCTION_DECLARATION
                    || kind == syntax_kind_ext::CLASS_DECLARATION
                    || kind == syntax_kind_ext::INTERFACE_DECLARATION
                    || kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || kind == syntax_kind_ext::ENUM_DECLARATION
                    || kind == syntax_kind_ext::VARIABLE_STATEMENT
                    || kind == syntax_kind_ext::MODULE_DECLARATION)
                    && !is_module_augmentation;
                if is_declaration_kind {
                    self.emitted_non_exported_declaration = true;
                }
            }
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
            .has_modifier(&func.modifiers, SyntaxKind::ExportKeyword)
            || self.is_js_named_exported_name(func.name);

        // `export default function() { ... }` — delegate to the export default handler
        // which correctly emits `export default function (): ReturnType;`
        let is_default = self
            .arena
            .has_modifier(&func.modifiers, SyntaxKind::DefaultKeyword);
        if is_exported && is_default {
            self.emit_export_default_function(func_idx);
            return;
        }

        if !is_exported
            && !self.should_emit_public_api_member(&func.modifiers)
            && !self.should_emit_public_api_dependency(func.name)
        {
            return;
        }
        if self.should_skip_ns_internal_member(&func.modifiers, Some(func_idx)) {
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
                self.skip_comments_in_node(func_node.pos, func_node.end);
                return;
            }
        }

        self.emit_pending_js_export_equals_for_name(func.name);
        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write("function ");

        // Function name
        self.emit_node(func.name);

        // Type parameters
        let jsdoc_template_params = if func
            .type_parameters
            .as_ref()
            .is_none_or(|type_params| type_params.nodes.is_empty())
        {
            self.jsdoc_template_params_for_node(func_idx)
        } else {
            Vec::new()
        };
        if let Some(ref type_params) = func.type_parameters {
            if !type_params.nodes.is_empty() {
                self.emit_type_parameters(type_params);
            } else if !jsdoc_template_params.is_empty() {
                self.emit_jsdoc_template_parameters(&jsdoc_template_params);
            }
        } else if !jsdoc_template_params.is_empty() {
            self.emit_jsdoc_template_parameters(&jsdoc_template_params);
        }

        // Parameters
        self.write("(");
        self.emit_parameters_with_body(&func.parameters, func.body);
        self.write(")");

        // Return type
        let func_body = func.body;
        let func_name = func.name;
        if func.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(func.type_annotation);
        } else if let Some(return_type_text) = self.jsdoc_return_type_text_for_node(func_idx) {
            self.write(": ");
            self.write(&return_type_text);
        } else if func_body.is_some()
            && self.emit_js_returned_define_property_function_type(func_body)
        {
        } else if func_body.is_some()
            && self
                .get_identifier_text(func.name)
                .is_some_and(|name| self.function_body_returns_identifier(func_body, &name))
        {
            self.write(": typeof ");
            self.emit_node(func.name);
        } else if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache) {
            // No explicit return type, try to infer it
            let func_type_id = cache
                .node_types
                .get(&func_idx.0)
                .copied()
                .or_else(|| self.get_node_type_or_names(&[func_name]))
                .or_else(|| self.get_type_via_symbol_for_func(func_idx, func_name));

            if let Some(func_type_id) = func_type_id
                && let Some(return_type_id) = type_queries::get_return_type(*interner, func_type_id)
            {
                let effective_return_type_id = if func_body.is_some() {
                    self.refine_invokable_return_type_from_identifier(func_body, return_type_id)
                        .unwrap_or(return_type_id)
                } else {
                    return_type_id
                };
                // If solver returned `any` but the function body clearly returns void,
                // prefer void (the solver's `any` is a fallback, not an actual inference)
                if effective_return_type_id == tsz_solver::types::TypeId::ANY
                    && func_body.is_some()
                    && self.body_returns_void(func_body)
                {
                    self.write(": void");
                } else if func_body.is_some()
                    && let Some(type_text) =
                        self.function_body_preferred_return_type_text(func_body)
                {
                    self.write(": ");
                    self.write(&type_text);
                } else if effective_return_type_id == tsz_solver::types::TypeId::ANY
                    && func_body.is_some()
                    && self
                        .get_identifier_text(func.name)
                        .is_some_and(|name| self.function_body_returns_identifier(func_body, &name))
                {
                    self.write(": typeof ");
                    self.emit_node(func.name);
                } else {
                    self.write(": ");
                    self.write(&self.print_type_id(effective_return_type_id));
                }
            } else if func_body.is_some() {
                if self.body_returns_void(func_body) {
                    self.write(": void");
                } else if let Some(return_text) =
                    self.function_body_preferred_return_type_text(func_body)
                {
                    self.write(": ");
                    self.write(&return_text);
                }
            }
        } else if func_body.is_some() {
            // No type cache available, but we can infer from the body
            if self.body_returns_void(func_body) {
                self.write(": void");
            } else if let Some(return_text) =
                self.function_body_preferred_return_type_text(func_body)
            {
                self.write(": ");
                self.write(&return_text);
            }
        }

        self.write(";");
        self.write_line();
        self.emit_js_synthetic_prototype_class_if_needed(func.name, is_exported);
        self.emit_js_namespace_export_aliases_for_name(func.name);

        // Skip comments within the function body to prevent them from
        // leaking as leading comments on the next statement.
        if let Some(body_node) = self.arena.get(func_body) {
            self.skip_comments_in_node(body_node.pos, body_node.end);
        }
    }

    pub(crate) fn emit_class_declaration(&mut self, class_idx: NodeIndex) {
        let Some(class_node) = self.arena.get(class_idx) else {
            return;
        };
        let Some(class) = self.arena.get_class(class_node) else {
            return;
        };

        let is_exported = self
            .arena
            .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword)
            || self.is_js_named_exported_name(class.name);
        if !is_exported
            && !self.should_emit_public_api_member(&class.modifiers)
            && !self.should_emit_public_api_dependency(class.name)
        {
            return;
        }
        if self.should_skip_ns_internal_member(&class.modifiers, Some(class_idx)) {
            return;
        }
        let is_abstract = self
            .arena
            .has_modifier(&class.modifiers, SyntaxKind::AbstractKeyword);
        let extends_alias = self.emit_synthetic_class_extends_alias_if_needed(
            class.name,
            class.heritage_clauses.as_ref(),
            false,
        );

        self.emit_pending_js_export_equals_for_name(class.name);
        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
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
            self.emit_class_heritage_clauses(heritage, extends_alias.as_deref());
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();

        // Reset constructor and method overload tracking for this class
        self.class_has_constructor_overloads = false;
        self.class_extends_another = class.heritage_clauses.as_ref().is_some_and(|hc| {
            hc.nodes.iter().any(|&clause_idx| {
                self.arena
                    .get_heritage_clause_at(clause_idx)
                    .is_some_and(|h| h.token == SyntaxKind::ExtendsKeyword as u16)
            })
        });
        self.method_names_with_overloads = FxHashSet::default();

        // Suppress method implementations that share a computed name with
        // an accessor (tsc emits only the accessor in .d.ts).
        let shadowed = self.computed_names_shadowed_by_accessors(&class.members);
        self.method_names_with_overloads.extend(shadowed);

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
            let before_jsdoc_len = self.writer.len();
            let saved_comment_idx = self.comment_emit_idx;
            if let Some(mn) = self.arena.get(member_idx) {
                self.emit_leading_jsdoc_comments(mn.pos);
            }
            let before_member_len = self.writer.len();
            self.emit_class_member(member_idx);
            if self.writer.len() == before_member_len {
                // Member didn't emit anything (e.g., skipped implementation overload).
                // Rollback the speculatively emitted JSDoc comments.
                self.writer.truncate(before_jsdoc_len);
                self.comment_emit_idx = saved_comment_idx;
                if let Some(mn) = self.arena.get(member_idx) {
                    self.skip_comments_in_node(mn.pos, mn.end);
                }
            }
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
        self.emit_js_namespace_export_aliases_for_name(class.name);
    }


    /// Pre-scan class members: when a computed property name appears on both
    /// a method implementation and a get/set accessor, tsc suppresses the
    /// method in the .d.ts output (the accessor wins). This returns the set
    /// of computed name texts that should be treated as "already declared"
    /// so the method implementation is skipped.
    pub(super) fn computed_names_shadowed_by_accessors(
        &self,
        members: &tsz_parser::parser::NodeList,
    ) -> rustc_hash::FxHashSet<String> {
        let mut accessor_names: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();
        let mut method_impl_names: Vec<String> = Vec::new();
        for &m in &members.nodes {
            let Some(mn) = self.arena.get(m) else {
                continue;
            };
            let is_accessor = mn.kind == syntax_kind_ext::GET_ACCESSOR
                || mn.kind == syntax_kind_ext::SET_ACCESSOR;
            let is_method = mn.kind == syntax_kind_ext::METHOD_DECLARATION;
            if !is_accessor && !is_method {
                continue;
            }
            let name_idx = if is_accessor {
                self.arena.get_accessor(mn).map(|a| a.name)
            } else {
                self.arena.get_method_decl(mn).map(|md| md.name)
            };
            let Some(name_idx) = name_idx else {
                continue;
            };
            let Some(name_node) = self.arena.get(name_idx) else {
                continue;
            };
            if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                continue;
            }
            let Some(text) = self.get_source_slice(name_node.pos, name_node.end) else {
                continue;
            };
            if is_accessor {
                accessor_names.insert(text);
            } else if self
                .arena
                .get_method_decl(mn)
                .is_some_and(|md| md.body.is_some())
            {
                method_impl_names.push(text);
            }
        }
        let mut result = rustc_hash::FxHashSet::default();
        for name in method_impl_names {
            if accessor_names.contains(&name) {
                result.insert(name);
            }
        }
        result
    }

    pub(super) fn emit_class_member(&mut self, member_idx: NodeIndex) {
        let Some(member_node) = self.arena.get(member_idx) else {
            return;
        };

        // Strip members annotated with @internal when --stripInternal is enabled
        if self.has_internal_annotation(member_node.pos) {
            return;
        }

        // Skip members with private identifier names (#foo) - these are replaced by `#private;`
        if self.member_has_private_identifier_name(member_idx) {
            return;
        }

        // Skip members with computed property names that are not emittable in .d.ts
        // (e.g., ["" + ""], [Symbol()], [variable] — only literals and well-known symbols survive)
        if self.member_has_non_emittable_computed_name(member_idx) {
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
        let prop_node_end = prop_node.end;
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

        // Check if readonly for literal initializer form
        let is_readonly = self
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::ReadonlyKeyword);
        let const_asserted_enum_member = prop
            .initializer
            .is_some()
            .then(|| self.const_asserted_enum_access_member_text(prop.initializer))
            .flatten();
        let widened_enum_type = prop
            .initializer
            .is_some()
            .then(|| self.simple_enum_access_base_name_text(prop.initializer))
            .flatten();

        // Type - use explicit annotation if present, otherwise use inferred type
        // SPECIAL CASE: For private properties, TypeScript omits type annotations in .d.ts
        if prop.type_annotation.is_some() && !is_private {
            self.write(": ");
            self.emit_type(prop.type_annotation);
        } else if !is_private {
            // For readonly properties with an enum member access initializer (e.g., `readonly type = E.A`),
            // emit the initializer expression directly, matching tsc behavior.
            let use_enum_initializer = is_readonly
                && !is_abstract
                && !prop.question_token
                && prop.initializer.is_some()
                && self
                    .simple_enum_access_member_text(prop.initializer)
                    .is_some();

            if use_enum_initializer {
                self.write(" = ");
                self.emit_expression(prop.initializer);
            } else if let Some(enum_member_text) = const_asserted_enum_member {
                self.write(": ");
                self.write(&enum_member_text);
            } else if !is_readonly
                && !is_abstract
                && !prop.question_token
                && let Some(enum_type_text) = widened_enum_type
            {
                self.write(": ");
                self.write(&enum_type_text);
            } else if let Some(type_id) = self.get_node_type_or_names(&[prop_idx, prop.name]) {
                // For readonly properties with literal types, use `= value` form
                // (same as const declarations in tsc)
                if is_readonly
                    && !is_abstract
                    && !prop.question_token
                    && let Some(interner) = self.type_interner
                    && let Some(lit) = tsz_solver::visitor::literal_value(interner, type_id)
                {
                    self.write(" = ");
                    self.write(&Self::format_literal_initializer(&lit, interner));
                } else if is_readonly
                    && !is_abstract
                    && !prop.question_token
                    && prop.initializer.is_some()
                    && let Some(lit_text) =
                        self.const_literal_initializer_text_deep(prop.initializer)
                {
                    // The type system widened the literal (e.g., `false` → `boolean`),
                    // but for readonly properties tsc preserves the `= value` form
                    // when the initializer is a simple literal.
                    self.write(" = ");
                    self.write(&lit_text);
                } else if let Some(typeof_text) = self.typeof_prefix_for_value_entity(
                    prop.initializer,
                    prop.initializer.is_some(),
                    Some(type_id),
                ) {
                    self.write(": ");
                    self.write(&typeof_text);
                    if prop.question_token
                        && self.strict_null_checks
                        && !typeof_text.ends_with("| undefined")
                    {
                        self.write(" | undefined");
                    }
                } else {
                    // For non-readonly properties without an explicit type annotation,
                    // widen literal types to their base types (e.g., `12` → `number`,
                    // `false` → `boolean`) matching tsc's DTS behaviour.
                    let effective_type = if !is_readonly {
                        self.type_interner
                            .map(|interner| {
                                tsz_solver::operations::widening::widen_literal_type(
                                    interner, type_id,
                                )
                            })
                            .unwrap_or(type_id)
                    } else {
                        type_id
                    };
                    let type_text = self
                        .rewrite_recursive_static_class_expression_type(prop_idx, effective_type);
                    self.write(": ");
                    self.write(&type_text);
                    // For optional class properties without an explicit type annotation,
                    // tsc appends `| undefined` when the inferred type doesn't already
                    // include it (e.g., `c? = 2` → `c?: number | undefined`).
                    if prop.question_token
                        && self.strict_null_checks
                        && !type_text.ends_with("| undefined")
                    {
                        self.write(" | undefined");
                    }
                }
            } else if is_readonly
                && !is_abstract
                && !prop.question_token
                && prop.initializer.is_some()
                && let Some(lit_text) = self.const_literal_initializer_text_deep(prop.initializer)
            {
                // For readonly properties with simple literal initializers,
                // emit `= value` form (matching tsc's const-like literal
                // preservation for `static readonly` and `readonly` properties).
                self.write(" = ");
                self.write(&lit_text);
            } else if prop.initializer.is_some()
                && let Some(type_text) = self.infer_fallback_type_text(prop.initializer)
            {
                self.write(": ");
                self.write(&type_text);
                // Same `| undefined` rule for fallback-inferred types on optional
                // class properties.
                if prop.question_token
                    && self.strict_null_checks
                    && !type_text.ends_with("| undefined")
                {
                    self.write(" | undefined");
                }
            }
        }

        self.write(";");
        if !prop.initializer.is_some() {
            self.emit_trailing_comment(prop_node_end);
        }
        self.write_line();
    }

    fn rewrite_recursive_static_class_expression_type(
        &self,
        prop_idx: NodeIndex,
        type_id: tsz_solver::types::TypeId,
    ) -> String {
        let printed = self.print_type_id(type_id);
        let Some(prop_node) = self.arena.get(prop_idx) else {
            return printed;
        };
        let Some(prop) = self.arena.get_property_decl(prop_node) else {
            return printed;
        };
        let Some(property_name) = self
            .arena
            .get_identifier_at(prop.name)
            .map(|ident| ident.escaped_text.clone())
        else {
            return printed;
        };
        if !self.property_initializer_is_recursive_class_expression(prop_idx, prop.initializer) {
            return printed;
        }
        let Some(interner) = self.type_interner else {
            return printed;
        };
        let Some(callable) = type_queries::get_callable_shape(interner, type_id) else {
            return printed;
        };
        if !callable.properties.iter().any(|prop| {
            interner.resolve_atom(prop.name) == property_name
                && prop.type_id == tsz_solver::TypeId::ANY
        }) {
            return printed;
        }

        printed.replacen(
            &format!("{property_name}: any;"),
            &format!("{property_name}: /*elided*/ any;"),
            1,
        )
    }

    fn property_initializer_is_recursive_class_expression(
        &self,
        prop_idx: NodeIndex,
        initializer_idx: NodeIndex,
    ) -> bool {
        let Some(class_expr) = self.arena.get_class_at(initializer_idx) else {
            return false;
        };
        let Some(enclosing_class_idx) = self
            .arena
            .get_extended(prop_idx)
            .map(|extended| extended.parent)
            .filter(|parent| {
                self.arena
                    .get(*parent)
                    .is_some_and(|node| node.kind == syntax_kind_ext::CLASS_DECLARATION)
            })
        else {
            return false;
        };
        let Some(enclosing_class_name) = self
            .arena
            .get_class_at(enclosing_class_idx)
            .and_then(|class| self.arena.get_identifier_at(class.name))
            .map(|ident| ident.escaped_text.clone())
        else {
            return false;
        };
        let Some(heritage_clauses) = class_expr.heritage_clauses.as_ref() else {
            return false;
        };

        heritage_clauses.nodes.iter().copied().any(|clause_idx| {
            self.arena
                .get_heritage_clause_at(clause_idx)
                .filter(|heritage| heritage.token == SyntaxKind::ExtendsKeyword as u16)
                .and_then(|heritage| heritage.types.nodes.first().copied())
                .map(|type_idx| {
                    self.arena
                        .get_expr_type_args_at(type_idx)
                        .map_or(type_idx, |expr_type_args| expr_type_args.expression)
                })
                .and_then(|expr_idx| self.arena.get_identifier_at(expr_idx))
                .is_some_and(|ident| ident.escaped_text == enclosing_class_name)
        })
    }

    fn emit_method_declaration(&mut self, method_idx: NodeIndex) {
        let Some(method_node) = self.arena.get(method_idx) else {
            return;
        };
        let Some(method) = self.arena.get_method_decl(method_node) else {
            return;
        };

        if self.should_skip_js_augmented_static_method(method_idx, method) {
            self.skip_comments_in_node(method_node.pos, method_node.end);
            return;
        }

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
            // For private methods, emit `private foo;` on first encounter only
            if is_private {
                let already_seen = if let Some(ref name) = method_name {
                    !self.method_names_with_overloads.insert(name.clone())
                } else {
                    false
                };
                if !already_seen {
                    // First private overload: emit `private foo;`
                    self.write_indent();
                    self.emit_member_modifiers(&method.modifiers);
                    self.emit_node(method.name);
                    self.write(";");
                    self.write_line();
                }
                self.skip_comments_in_node(method_node.pos, method_node.end);
                return;
            }
            // Mark that this method name has overload signatures
            if let Some(ref name) = method_name {
                self.method_names_with_overloads.insert(name.clone());
            }
        } else if is_implementation {
            // This is an implementation - check if we've seen overloads for this name
            if let Some(ref name) = method_name
                && self.method_names_with_overloads.contains(name)
            {
                // Skip implementation signature when overloads exist
                // (for private methods, `private foo;` was already emitted at first overload)
                self.skip_comments_in_node(method_node.pos, method_node.end);
                return;
            }
        }

        self.write_indent();

        // Modifiers
        self.emit_member_modifiers(&method.modifiers);

        // Name
        self.emit_node(method.name);
        if method.question_token {
            self.write("?");
        }

        // For private methods (no overloads), emit just the name without signature
        if is_private {
            self.write(";");
            self.write_line();
            self.skip_comments_in_node(method_node.pos, method_node.end);
            return;
        }

        // tsc uses property syntax for computed method names in these cases:
        // 1. Computed key with `any` type (from shorthand ambient modules)
        // 2. Optional computed methods (`[key]?()` → `[key]?: (() => T) | undefined`)
        // Non-computed optional methods keep method syntax: `g?(): T`
        let is_computed_name = self
            .arena
            .get(method.name)
            .is_some_and(|node| node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME);

        let use_property_syntax = is_computed_name
            && (method.question_token
                || self
                    .arena
                    .get(method.name)
                    .and_then(|node| self.arena.get_computed_property(node))
                    .and_then(|cp| self.get_node_type_or_names(&[cp.expression, method.name]))
                    .is_some_and(|t| {
                        t == tsz_solver::types::TypeId::ANY
                            || self.type_interner.is_some_and(|interner| {
                                !tsz_solver::type_queries::is_type_usable_as_property_name(
                                    interner, t,
                                )
                            })
                    }))
            // If the computed name resolves to a known literal (e.g. const enum member),
            // keep method syntax — the name is a valid property name in .d.ts
            && self
                .resolved_computed_property_name_text(method.name)
                .is_none();

        if use_property_syntax {
            self.write(": ");
            if method.question_token {
                self.write("(");
            }
            if let Some(ref type_params) = method.type_parameters
                && !type_params.nodes.is_empty()
            {
                self.emit_type_parameters(type_params);
            }
            self.write("(");
            self.emit_parameters_with_body(&method.parameters, method.body);
            self.write(") => ");
            self.emit_method_function_type_return(method_idx, method);
            if method.question_token {
                self.write(") | undefined;");
            } else {
                self.write(";");
            }
            self.write_line();
            if let Some(body_node) = self.arena.get(method.body) {
                self.skip_comments_in_node(body_node.pos, body_node.end);
            }
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
        self.emit_parameters_with_body(&method.parameters, method.body);
        self.write(")");

        // Return type - SPECIAL CASE: For private methods, TypeScript omits return type in .d.ts
        let method_body = method.body;
        self.emit_method_return_type(method_idx, method, is_private);

        self.write(";");
        self.write_line();

        // Skip comments within the method body to prevent them from
        // leaking as leading comments on the next statement.
        if let Some(body_node) = self.arena.get(method_body) {
            self.skip_comments_in_node(body_node.pos, body_node.end);
        }
    }

    fn emit_method_return_type(
        &mut self,
        method_idx: NodeIndex,
        method: &MethodDeclData,
        is_private: bool,
    ) {
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
                .or_else(|| self.get_node_type_or_names(&[method_name]))
                .or_else(|| self.get_type_via_symbol_for_func(method_idx, method_name));

            if let Some(method_type_id) = method_type_id
                && let Some(return_type_id) =
                    type_queries::get_return_type(*interner, method_type_id)
            {
                if return_type_id == tsz_solver::types::TypeId::ANY
                    && method_body.is_some()
                    && self.body_returns_void(method_body)
                {
                    self.write(": void");
                } else if method_body.is_some()
                    && let Some(type_text) =
                        self.function_body_preferred_return_type_text(method_body)
                {
                    self.write(": ");
                    self.write(&type_text);
                } else {
                    self.write(": ");
                    self.write(&self.print_type_id(return_type_id));
                }
            } else if let Some(method_type_id) = method_type_id {
                if method_type_id == tsz_solver::types::TypeId::ANY
                    && method_body.is_some()
                    && self.body_returns_void(method_body)
                {
                    self.write(": void");
                } else if method_body.is_some()
                    && let Some(type_text) =
                        self.function_body_preferred_return_type_text(method_body)
                {
                    self.write(": ");
                    self.write(&type_text);
                } else {
                    self.write(": ");
                    self.write(&self.print_type_id(method_type_id));
                }
            } else if method_body.is_some() {
                if self.body_returns_void(method_body) {
                    self.write(": void");
                } else if let Some(type_text) =
                    self.function_body_preferred_return_type_text(method_body)
                {
                    self.write(": ");
                    self.write(&type_text);
                } else if !self.source_is_declaration_file {
                    self.write(": any");
                }
            } else if !self.source_is_declaration_file {
                self.write(": any");
            }
        } else if !is_private {
            if method_body.is_some() {
                if self.body_returns_void(method_body) {
                    self.write(": void");
                } else if let Some(type_text) =
                    self.function_body_preferred_return_type_text(method_body)
                {
                    self.write(": ");
                    self.write(&type_text);
                } else if !self.source_is_declaration_file {
                    self.write(": any");
                }
            } else if !self.source_is_declaration_file {
                self.write(": any");
            }
        }
    }

    fn emit_method_function_type_return(&mut self, method_idx: NodeIndex, method: &MethodDeclData) {
        let method_body = method.body;
        let method_name = method.name;
        if method.type_annotation.is_some() {
            self.emit_type(method.type_annotation);
        } else if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache) {
            let method_type_id = cache
                .node_types
                .get(&method_idx.0)
                .copied()
                .or_else(|| self.get_node_type_or_names(&[method_name]))
                .or_else(|| self.get_type_via_symbol_for_func(method_idx, method_name));

            if let Some(method_type_id) = method_type_id
                && let Some(return_type_id) =
                    type_queries::get_return_type(*interner, method_type_id)
            {
                if return_type_id == tsz_solver::types::TypeId::ANY
                    && method_body.is_some()
                    && self.body_returns_void(method_body)
                {
                    self.write("void");
                } else if method_body.is_some()
                    && let Some(type_text) =
                        self.function_body_preferred_return_type_text(method_body)
                {
                    self.write(&type_text);
                } else {
                    self.write(&self.print_type_id(return_type_id));
                }
            } else if let Some(method_type_id) = method_type_id {
                if method_type_id == tsz_solver::types::TypeId::ANY
                    && method_body.is_some()
                    && self.body_returns_void(method_body)
                {
                    self.write("void");
                } else if method_body.is_some()
                    && let Some(type_text) =
                        self.function_body_preferred_return_type_text(method_body)
                {
                    self.write(&type_text);
                } else {
                    self.write(&self.print_type_id(method_type_id));
                }
            } else if method_body.is_some() {
                if self.body_returns_void(method_body) {
                    self.write("void");
                } else if let Some(type_text) =
                    self.function_body_preferred_return_type_text(method_body)
                {
                    self.write(&type_text);
                } else if !self.source_is_declaration_file {
                    self.write("any");
                }
            } else if !self.source_is_declaration_file {
                self.write("any");
            }
        } else if method_body.is_some() {
            if self.body_returns_void(method_body) {
                self.write("void");
            } else if let Some(type_text) =
                self.function_body_preferred_return_type_text(method_body)
            {
                self.write(&type_text);
            } else if !self.source_is_declaration_file {
                self.write("any");
            }
        } else if !self.source_is_declaration_file {
            self.write("any");
        }
    }

    fn should_skip_js_augmented_static_method(
        &self,
        method_idx: NodeIndex,
        method: &tsz_parser::parser::node::MethodDeclData,
    ) -> bool {
        if !self.source_is_js_file
            || !self
                .arena
                .has_modifier(&method.modifiers, SyntaxKind::StaticKeyword)
        {
            return false;
        }
        self.js_augmented_static_method_nodes.contains(&method_idx)
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

        let has_visibility_modifier = ctor.modifiers.as_ref().is_some_and(|mods| {
            mods.nodes.iter().any(|&mod_idx| {
                self.arena.get(mod_idx).is_some_and(|mod_node| {
                    mod_node.kind == SyntaxKind::PrivateKeyword as u16
                        || mod_node.kind == SyntaxKind::ProtectedKeyword as u16
                })
            })
        });

        if self.source_is_js_file
            && ctor.parameters.nodes.is_empty()
            && !has_visibility_modifier
            && !self.class_extends_another
        {
            if let Some(body_node) = self.arena.get(ctor.body) {
                self.skip_comments_in_node(body_node.pos, body_node.end);
            }
            return;
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
        let ctor_body = ctor.body;
        if !is_private {
            // Set flag to strip accessibility modifiers from constructor parameters
            self.in_constructor_params = true;
            self.emit_parameters_with_body(&ctor.parameters, ctor.body);
            self.in_constructor_params = false;
        }
        self.write(");");
        self.write_line();

        // Skip comments within the constructor body to prevent them from
        // leaking as leading comments on the next statement.
        if let Some(body_node) = self.arena.get(ctor_body) {
            self.skip_comments_in_node(body_node.pos, body_node.end);
        }
    }

    /// Emit parameter properties from constructor as class properties
    /// Parameter properties (e.g., `constructor(public x: number)`) should be emitted
    /// as property declarations in the class body, then stripped from constructor params
    pub(super) fn emit_parameter_properties(&mut self, members: &tsz_parser::parser::NodeList) {
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
                let has_modifier = self.parameter_has_property_modifier(&param.modifiers);

                if has_modifier {
                    let is_destructuring = self.arena.get(param.name).is_some_and(|name_node| {
                        name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                            || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                    });

                    if is_destructuring {
                        let bindings = self.collect_flattened_binding_entries(
                            param.name,
                            self.preferred_binding_source_type(
                                param.type_annotation,
                                param.initializer,
                                &[param_idx, param.name, param.initializer],
                            ),
                        );
                        for (ident_idx, type_id) in bindings {
                            self.write_indent();
                            let is_private =
                                self.emit_parameter_property_modifiers(&param.modifiers);
                            self.emit_node(ident_idx);
                            if param.question_token {
                                self.write("?");
                            }
                            if !is_private {
                                self.emit_flattened_binding_type_annotation(ident_idx, type_id);
                            }
                            self.write(";");
                            self.write_line();
                        }
                        continue;
                    }

                    // Emit as a property declaration
                    self.write_indent();
                    let is_private = self.emit_parameter_property_modifiers(&param.modifiers);

                    self.emit_node(param.name);

                    // Optional
                    if param.question_token {
                        self.write("?");
                    }

                    // Type annotation (omit for private properties, include for others)
                    if !is_private && param.type_annotation.is_some() {
                        self.write(": ");
                        let before_type = self.writer.len();
                        self.emit_type(param.type_annotation);
                        if param.question_token {
                            // Only append `| undefined` if the type doesn't already
                            // include it (avoid `Type | undefined | undefined`).
                            let full = self.writer.get_output();
                            let type_text = &full[before_type..];
                            if !type_text.ends_with("| undefined") {
                                self.write(" | undefined");
                            }
                        }
                    } else if !is_private
                        && let Some(type_id) = self.get_node_type_or_names(&[param_idx, param.name])
                    {
                        self.write(": ");
                        self.write(&self.print_type_id(type_id));
                    } else if !is_private
                        && param.initializer.is_some()
                        && let Some(type_text) = self.infer_fallback_type_text(param.initializer)
                    {
                        self.write(": ");
                        self.write(&type_text);
                    } else if !is_private && !self.source_is_declaration_file {
                        // Fallback: no explicit type, no inferred type, no initializer
                        self.write(": any");
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
        let accessor_body = accessor.body;

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
            self.skip_comments_in_node(accessor_node.pos, accessor_node.end);
        } else {
            self.emit_parameters_without_types(&accessor.parameters, is_private);
        }
        self.write(")");

        // Return type (for getters) - omit for private accessors
        if is_getter && !is_private && accessor.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(accessor.type_annotation);
        } else if is_getter && !is_private {
            if let Some(type_text) = self.matching_setter_parameter_type_text(accessor_idx) {
                self.write(": ");
                self.write(&type_text);
            } else if let Some(type_id) =
                self.get_node_type_or_names(&[accessor_idx, accessor.name])
            {
                // If solver returned `any` but body clearly returns void, prefer void
                if type_id == tsz_solver::types::TypeId::ANY
                    && accessor_body.is_some()
                    && self.body_returns_void(accessor_body)
                {
                    self.write(": void");
                } else {
                    self.write(": ");
                    self.write(&self.print_type_id(type_id));
                }
            } else if accessor_body.is_some() {
                if self.body_returns_void(accessor_body) {
                    self.write(": void");
                } else if let Some(return_text) =
                    self.function_body_preferred_return_type_text(accessor_body)
                {
                    self.write(": ");
                    self.write(&return_text);
                } else if !self.source_is_declaration_file {
                    self.write(": any");
                }
            } else if !self.source_is_declaration_file {
                self.write(": any");
            }
        }

        self.write(";");
        self.write_line();

        // Skip comments within the accessor body to prevent them from
        // leaking as leading comments on the next statement.
        if let Some(body_node) = self.arena.get(accessor_body) {
            self.skip_comments_in_node(body_node.pos, body_node.end);
        }
    }

    fn matching_setter_parameter_type_text(&mut self, accessor_idx: NodeIndex) -> Option<String> {
        let accessor_name = {
            let accessor_node = self.arena.get(accessor_idx)?;
            let accessor = self.arena.get_accessor(accessor_node)?;
            let name_node = self.arena.get(accessor.name)?;
            self.get_source_slice(name_node.pos, name_node.end)?
        };

        let parent_idx = self.arena.get_extended(accessor_idx)?.parent;
        let parent_node = self.arena.get(parent_idx)?;
        let class_decl = self.arena.get_class(parent_node)?;

        for &member_idx in &class_decl.members.nodes {
            if member_idx == accessor_idx {
                continue;
            }

            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::SET_ACCESSOR {
                continue;
            }

            let Some(setter) = self.arena.get_accessor(member_node) else {
                continue;
            };
            let Some(setter_name_node) = self.arena.get(setter.name) else {
                continue;
            };
            if self
                .get_source_slice(setter_name_node.pos, setter_name_node.end)
                .as_deref()
                != Some(accessor_name.as_str())
            {
                continue;
            }

            let Some(&param_idx) = setter.parameters.nodes.first() else {
                continue;
            };
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };

            if param.type_annotation.is_some() {
                let saved_comment_idx = self.comment_emit_idx;
                let saved_pending_source_pos = self.pending_source_pos;
                let saved_writer = std::mem::take(&mut self.writer);
                self.emit_type(param.type_annotation);
                let type_writer = std::mem::replace(&mut self.writer, saved_writer);
                self.comment_emit_idx = saved_comment_idx;
                self.pending_source_pos = saved_pending_source_pos;
                return Some(type_writer.take_output());
            }

            if let Some(type_id) = self.get_node_type_or_names(&[param_idx, param.name]) {
                return Some(self.print_type_id(type_id));
            }
        }

        None
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
        if self.should_skip_ns_internal_member(&alias.modifiers, Some(alias_idx)) {
            return;
        }

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self
            .arena
            .has_modifier(&alias.modifiers, SyntaxKind::DeclareKeyword)
            && !self.inside_declare_namespace
        {
            self.write("declare ");
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
        if self.should_skip_ns_internal_member(&enum_data.modifiers, Some(enum_idx)) {
            return;
        }
        let is_const = self
            .arena
            .has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword);

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
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

        // Evaluate enum member values to get correct auto-increment behavior.
        // Seed the evaluator with accumulated values from prior enums so that
        // cross-enum references (e.g., `enum B { Y = A.X }`) can be resolved.
        let prior = std::mem::take(&mut self.all_enum_values);
        let mut evaluator = EnumEvaluator::with_prior_values(self.arena, prior);
        let member_values = evaluator.evaluate_enum(enum_idx);
        self.all_enum_values = evaluator.take_all_enum_values();

        for (i, &member_idx) in enum_data.members.nodes.iter().enumerate() {
            if let Some(mn) = self.arena.get(member_idx) {
                self.emit_leading_jsdoc_comments(mn.pos);
            }
            self.write_indent();
            if let Some(member_node) = self.arena.get(member_idx)
                && let Some(member) = self.arena.get_enum_member(member_node)
            {
                self.emit_node(member.name);
                // For ambient enums (inside declare context or with declare keyword), only
                // emit values for members with explicit initializers.
                // For implementation enums, always emit computed values.
                let is_ambient = self.inside_declare_namespace
                    || self
                        .arena
                        .has_modifier(&enum_data.modifiers, SyntaxKind::DeclareKeyword)
                    || self.source_is_declaration_file;
                let has_explicit_init = member.initializer.is_some();
                let should_emit_value = !is_ambient || has_explicit_init || is_const;
                if should_emit_value {
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
                    } else if !is_ambient {
                        // Fallback to index for non-ambient enums if evaluation failed
                        self.write(" = ");
                        self.write(&i.to_string());
                    }
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

    /// Check whether a property/element access is a simple enum member access (E.A or E["key"]).
    /// Returns true only when the left-hand side is a simple identifier (not a chain like a.b.c).
    pub(super) fn is_simple_enum_access(&self, node: &tsz_parser::parser::node::Node) -> bool {
        if let Some(access) = self.arena.get_access_expr(node)
            && let Some(expr_node) = self.arena.get(access.expression)
        {
            return expr_node.kind == SyntaxKind::Identifier as u16;
        }
        false
    }

    /// Check whether a computed property name expression is suitable for `.d.ts` emission.
    ///
    /// In tsc, computed property names survive into declaration output when they are
    /// "entity name expressions" — late-bindable names that can be statically resolved:
    /// 1. String literals: `["hello"]`
    /// 2. Numeric literals: `[42]`, `[-1]`
    /// 3. Well-known symbol accesses: `[Symbol.iterator]`, `[Symbol.hasInstance]`, etc.
    /// 4. Identifiers referencing unique symbols or const enums: `[key]`, `[O]`
    /// 5. Property accesses on entity names: `[E.A]`, `[TestEnum.Test1]`
    pub(super) fn should_emit_computed_property(&self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.arena.get(name_idx) else {
            return true;
        };

        // Not a computed property name — always emit
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return true;
        }

        let Some(computed) = self.arena.get_computed_property(name_node) else {
            return false;
        };

        self.is_entity_name_expression(computed.expression)
    }

    /// Check if an expression is an "entity name expression" — an expression that can
    /// appear as a computed property name in declaration output.
    fn is_entity_name_expression(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };

        match expr_node.kind {
            // String literal: ["hello"]
            k if k == SyntaxKind::StringLiteral as u16 => true,
            // Numeric literal: [42]
            k if k == SyntaxKind::NumericLiteral as u16 => true,
            // Identifier: [key], [O], [symb]
            k if k == SyntaxKind::Identifier as u16 => true,
            // Property access: [Symbol.iterator], [E.A], [TestEnum.Test1]
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(expr_node) {
                    self.is_entity_name_expression(access.expression)
                } else {
                    false
                }
            }
            // Prefix unary: [-1]
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => true,
            _ => false,
        }
    }

    /// Get the name `NodeIndex` of a class or interface member, if it has one.
    fn get_member_name_idx(&self, member_idx: NodeIndex) -> Option<NodeIndex> {
        let member_node = self.arena.get(member_idx)?;

        if let Some(prop) = self.arena.get_property_decl(member_node) {
            return Some(prop.name);
        }
        if let Some(method) = self.arena.get_method_decl(member_node) {
            return Some(method.name);
        }
        if let Some(accessor) = self.arena.get_accessor(member_node) {
            return Some(accessor.name);
        }
        if let Some(sig) = self.arena.get_signature(member_node) {
            return Some(sig.name);
        }
        None
    }

    /// Check if a member has a computed property name that should NOT be emitted in `.d.ts`.
    /// Returns `true` if the member should be skipped.
    pub(super) fn member_has_non_emittable_computed_name(&self, member_idx: NodeIndex) -> bool {
        if let Some(name_idx) = self.get_member_name_idx(member_idx) {
            !self.should_emit_computed_property(name_idx)
        } else {
            false
        }
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
            return false;
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

        let has_export_modifier = self
            .arena
            .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword);
        let has_js_named_export = var_stmt.declarations.nodes.iter().any(|&decl_list_idx| {
            self.arena
                .get(decl_list_idx)
                .and_then(|decl_list_node| self.arena.get_variable(decl_list_node))
                .is_some_and(|decl_list| {
                    decl_list.declarations.nodes.iter().any(|&decl_idx| {
                        self.arena
                            .get(decl_idx)
                            .and_then(|decl_node| self.arena.get_variable_declaration(decl_node))
                            .is_some_and(|decl| self.is_js_named_exported_name(decl.name))
                    })
                })
        });
        if !has_js_named_export && !self.should_emit_public_api_member(&var_stmt.modifiers) {
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
        if self.should_skip_ns_internal_member(&var_stmt.modifiers, None) {
            return;
        }

        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };

            if decl_list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                && let Some(decl_list) = self.arena.get_variable(decl_list_node)
            {
                // Determine let/const/var
                // `using` and `await using` declarations emit as `const` in .d.ts
                let flags = decl_list_node.flags as u32;
                // USING(4) and AWAIT_USING(6) both have the USING bit set
                let keyword = if flags
                    & (tsz_parser::parser::node_flags::USING
                        | tsz_parser::parser::node_flags::CONST)
                    != 0
                {
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
                        let is_destructuring = name_node.is_some_and(|n| {
                            n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                        });

                        if is_destructuring {
                            // Emit destructuring as individual declarations
                            let is_exported =
                                has_export_modifier || self.is_js_named_exported_name(decl.name);
                            self.emit_flattened_variable_declaration(
                                decl_idx,
                                keyword,
                                is_exported,
                            );
                        } else {
                            let is_exported =
                                has_export_modifier || self.is_js_named_exported_name(decl.name);
                            regular_decls.push((is_exported, decl_idx, decl_node, decl));
                        }
                    }
                }

                if regular_decls.len() == 1 {
                    let (is_exported, decl_idx, _decl_node, decl) = regular_decls[0];
                    if self.emit_js_object_literal_namespace_if_possible(
                        decl.name,
                        decl.initializer,
                        is_exported,
                    ) {
                        if let Some(dn) = self.arena.get(decl_idx) {
                            let skip_end =
                                self.arena.get(decl.initializer).map_or(dn.end, |n| n.end);
                            self.skip_comments_in_node(dn.pos, skip_end);
                        }
                        continue;
                    }
                    if self.emit_js_function_variable_declaration_if_possible(
                        decl_idx,
                        decl.name,
                        decl.initializer,
                        is_exported,
                    ) {
                        if let Some(dn) = self.arena.get(decl_idx) {
                            let skip_end =
                                self.arena.get(decl.initializer).map_or(dn.end, |n| n.end);
                            self.skip_comments_in_node(dn.pos, skip_end);
                        }
                        continue;
                    }
                }

                // When emitting a non-exported variable statement purely because of
                // dependency tracking, filter to only the declarations that are actually
                // referenced. E.g. `const key = Symbol(), value = 12` should only emit
                // `key` if only `key` is in used_symbols.
                if !has_export_modifier && !has_js_named_export {
                    regular_decls.retain(|(_is_exported, _decl_idx, _decl_node, decl)| {
                        self.should_emit_public_api_dependency(decl.name)
                    });
                }

                // Emit regular declarations in contiguous export/non-export groups.
                let mut group_start = 0;
                while group_start < regular_decls.len() {
                    let is_exported = regular_decls[group_start].0;
                    let mut group_end = group_start;
                    while group_end < regular_decls.len()
                        && regular_decls[group_end].0 == is_exported
                    {
                        group_end += 1;
                    }
                    for (_, _, _, decl) in &regular_decls[group_start..group_end] {
                        self.emit_pending_js_export_equals_for_name(decl.name);
                    }
                    self.write_indent();
                    if is_exported {
                        self.write("export ");
                    }
                    if self.should_emit_declare_keyword(is_exported) {
                        self.write("declare ");
                    }
                    self.write(keyword);
                    self.write(" ");

                    let mut i = group_start;
                    while i < group_end {
                        if i > group_start {
                            self.write(", ");
                        }
                        let (_is_exported, decl_idx, _decl_node, decl) = &regular_decls[i];

                        // Emit inline comments between keyword and name
                        // (e.g. `var /*4*/ point = ...` → `declare var /*4*/ point: ...`)
                        if let Some(name_node) = self.arena.get(decl.name) {
                            self.emit_inline_block_comments(name_node.pos);
                        }
                        self.emit_node(decl.name);
                        // When a variable's initializer is a simple reference to an
                        // import-equals alias (e.g. `var bVal2 = b` where `import b = a.foo`),
                        // tsc emits `typeof b` instead of expanding the type.
                        if !decl.type_annotation.is_some()
                            && decl.initializer.is_some()
                            && let Some(alias_text) =
                                self.initializer_import_alias_typeof_text(decl.initializer)
                        {
                            self.write(": typeof ");
                            self.write(&alias_text);
                        } else {
                            self.emit_variable_decl_type_or_initializer(
                                keyword,
                                stmt_node.pos,
                                *decl_idx,
                                decl.name,
                                decl.type_annotation,
                                decl.initializer,
                            );
                        }

                        // Skip comments within the declaration's omitted parts (initializer,
                        // inline type comments) to prevent them from leaking as leading
                        // comments on the next statement.
                        // Use the initializer/type end position as the bound, not the full
                        // declaration's end — the parser may set `end` to include trailing
                        // trivia that extends into the next statement's leading JSDoc comments.
                        {
                            let skip_end = if decl.initializer.is_some() {
                                self.arena.get(decl.initializer).map_or(0, |n| n.end)
                            } else if decl.type_annotation.is_some() {
                                self.arena.get(decl.type_annotation).map_or(0, |n| n.end)
                            } else {
                                self.arena.get(decl.name).map_or(0, |n| n.end)
                            };
                            if skip_end > 0
                                && let Some(dn) = self.arena.get(*decl_idx)
                            {
                                self.skip_comments_in_node(dn.pos, skip_end);
                            }
                        }
                        i += 1;
                    }

                    self.write(";");
                    self.write_line();
                    group_start = group_end;
                }
            }
        }
    }

    fn emit_js_object_literal_namespace_if_possible(
        &mut self,
        decl_name: NodeIndex,
        initializer: NodeIndex,
        is_exported: bool,
    ) -> bool {
        if !self.source_is_js_file || !initializer.is_some() {
            return false;
        }

        let Some(name_node) = self.arena.get(decl_name) else {
            return false;
        };
        if name_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }

        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(object) = self.arena.get_literal_expr(init_node) else {
            return false;
        };
        if object.elements.nodes.is_empty() {
            return false;
        }

        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                return false;
            };
            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.arena.get_property_assignment(member_node) else {
                        return false;
                    };
                    let Some(prop_name_node) = self.arena.get(prop.name) else {
                        return false;
                    };
                    if prop_name_node.kind != SyntaxKind::Identifier as u16 {
                        return false;
                    }
                    if !self.js_namespace_object_member_initializer_supported(prop.initializer) {
                        return false;
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.arena.get_method_decl(member_node) else {
                        return false;
                    };
                    let Some(method_name_node) = self.arena.get(method.name) else {
                        return false;
                    };
                    if method_name_node.kind != SyntaxKind::Identifier as u16 {
                        return false;
                    }
                }
                _ => return false,
            }
        }

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write("namespace ");
        self.emit_node(decl_name);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.arena.get_property_assignment(member_node) else {
                        continue;
                    };
                    let Some(init_node) = self.arena.get(prop.initializer) else {
                        continue;
                    };
                    if init_node.kind == syntax_kind_ext::ARROW_FUNCTION
                        || init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    {
                        let Some(func) = self.arena.get_function(init_node) else {
                            continue;
                        };
                        self.emit_js_namespace_function_member(
                            prop.name,
                            func.type_parameters.as_ref(),
                            &func.parameters,
                            func.body,
                            func.type_annotation,
                        );
                    } else if let Some(type_text) =
                        self.js_namespace_value_member_type_text(prop.initializer)
                    {
                        self.emit_js_namespace_value_member(prop.name, &type_text);
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    self.emit_js_namespace_function_member(
                        method.name,
                        method.type_parameters.as_ref(),
                        &method.parameters,
                        method.body,
                        method.type_annotation,
                    );
                }
                _ => {}
            }
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
        true
    }

    fn has_synthetic_js_expression_declaration(&self, stmt_idx: NodeIndex) -> bool {
        self.js_deferred_function_export_statements
            .contains_key(&stmt_idx)
            || self
                .js_deferred_value_export_statements
                .contains_key(&stmt_idx)
            || self
                .js_static_method_augmentation_statements
                .contains_key(&stmt_idx)
    }

    fn emit_js_synthetic_expression_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };

        if let Some((name_idx, initializer, is_exported)) = self
            .js_deferred_function_export_statements
            .get(&stmt_idx)
            .copied()
        {
            self.emit_js_synthetic_function_declaration(name_idx, initializer, is_exported);
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            return;
        }

        if let Some((name_idx, initializer, is_exported)) = self
            .js_deferred_value_export_statements
            .get(&stmt_idx)
            .copied()
        {
            self.emit_js_synthetic_value_declaration(name_idx, initializer, is_exported);
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            return;
        }

        if let Some(group) = self
            .js_static_method_augmentation_statements
            .get(&stmt_idx)
            .cloned()
        {
            self.emit_js_static_method_augmentation_namespace(&group);
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
        }
    }

    fn emit_js_synthetic_function_declaration(
        &mut self,
        name_idx: NodeIndex,
        initializer: NodeIndex,
        is_exported: bool,
    ) {
        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };
        if init_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return;
        }
        let Some(func) = self.arena.get_function(init_node) else {
            return;
        };

        let jsdoc = self.function_like_jsdoc_for_node(initializer);

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write("function ");
        self.emit_node(name_idx);

        let jsdoc_template_params = if func
            .type_parameters
            .as_ref()
            .is_none_or(|type_params| type_params.nodes.is_empty())
        {
            jsdoc
                .as_deref()
                .map(Self::parse_jsdoc_template_params)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        if let Some(ref type_params) = func.type_parameters {
            if !type_params.nodes.is_empty() {
                self.emit_type_parameters(type_params);
            } else if !jsdoc_template_params.is_empty() {
                self.emit_jsdoc_template_parameters(&jsdoc_template_params);
            }
        } else if !jsdoc_template_params.is_empty() {
            self.emit_jsdoc_template_parameters(&jsdoc_template_params);
        }

        self.write("(");
        self.emit_parameters_with_body(&func.parameters, func.body);
        self.write(")");

        if func.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(func.type_annotation);
        } else if let Some(return_type_text) = jsdoc
            .as_deref()
            .and_then(Self::parse_jsdoc_return_type_text)
        {
            self.write(": ");
            self.write(&return_type_text);
        } else if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache) {
            let func_type_id = cache
                .node_types
                .get(&initializer.0)
                .copied()
                .or_else(|| self.get_node_type_or_names(&[name_idx, initializer]));
            if let Some(func_type_id) = func_type_id
                && let Some(return_type_id) = type_queries::get_return_type(*interner, func_type_id)
            {
                if return_type_id == tsz_solver::types::TypeId::ANY
                    && func.body.is_some()
                    && self.body_returns_void(func.body)
                {
                    self.write(": void");
                } else {
                    self.write(": ");
                    self.write(&self.print_type_id(return_type_id));
                }
            } else if func.body.is_some() && self.body_returns_void(func.body) {
                self.write(": void");
            }
        } else if func.body.is_some() && self.body_returns_void(func.body) {
            self.write(": void");
        }

        self.write(";");
        self.write_line();
        if is_exported {
            self.emitted_module_indicator = true;
        }
    }

    fn emit_js_synthetic_value_declaration(
        &mut self,
        name_idx: NodeIndex,
        initializer: NodeIndex,
        is_exported: bool,
    ) {
        if is_exported && self.emit_js_synthetic_class_expression_declaration(name_idx, initializer)
        {
            return;
        }

        let type_text = if is_exported {
            self.js_synthetic_export_value_type_text(initializer)
        } else {
            self.js_namespace_value_member_type_text(initializer)
        };
        let Some(type_text) = type_text else {
            return;
        };

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write(if is_exported {
            self.js_synthetic_export_value_keyword(initializer)
        } else {
            "var "
        });
        self.emit_node(name_idx);
        self.write(": ");
        self.write(&type_text);
        self.write(";");
        self.write_line();
        if is_exported {
            self.emitted_module_indicator = true;
        }
    }

    fn emit_js_synthetic_class_expression_declaration(
        &mut self,
        name_idx: NodeIndex,
        initializer: NodeIndex,
    ) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::CLASS_EXPRESSION {
            return false;
        }
        let Some(class) = self.arena.get_class(init_node) else {
            return false;
        };

        let Some(export_name) = self.get_identifier_text(name_idx) else {
            return false;
        };
        let Some(class_name) = self.get_identifier_text(class.name) else {
            return false;
        };
        if class_name != export_name {
            return false;
        }

        let is_abstract = self
            .arena
            .has_modifier(&class.modifiers, SyntaxKind::AbstractKeyword);

        self.write_indent();
        self.write("export ");
        if self.should_emit_declare_keyword(true) {
            self.write("declare ");
        }
        if is_abstract {
            self.write("abstract ");
        }
        self.write("class ");
        self.emit_node(name_idx);

        if let Some(ref type_params) = class.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }
        if let Some(ref heritage) = class.heritage_clauses {
            self.emit_heritage_clauses(heritage);
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();

        self.class_has_constructor_overloads = false;
        self.class_extends_another = class.heritage_clauses.as_ref().is_some_and(|hc| {
            hc.nodes.iter().any(|&clause_idx| {
                self.arena
                    .get_heritage_clause_at(clause_idx)
                    .is_some_and(|h| h.token == SyntaxKind::ExtendsKeyword as u16)
            })
        });
        self.method_names_with_overloads = FxHashSet::default();

        self.emit_parameter_properties(&class.members);
        if self.class_has_private_identifier_member(&class.members) {
            self.write_indent();
            self.write("#private;");
            self.write_line();
        }

        for &member_idx in &class.members.nodes {
            let before_jsdoc_len = self.writer.len();
            let saved_comment_idx = self.comment_emit_idx;
            if let Some(member_node) = self.arena.get(member_idx) {
                self.emit_leading_jsdoc_comments(member_node.pos);
            }
            let before_member_len = self.writer.len();
            self.emit_class_member(member_idx);
            if self.writer.len() == before_member_len {
                self.writer.truncate(before_jsdoc_len);
                self.comment_emit_idx = saved_comment_idx;
                if let Some(member_node) = self.arena.get(member_idx) {
                    self.skip_comments_in_node(member_node.pos, member_node.end);
                }
            }
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
        self.emitted_module_indicator = true;
        true
    }

    fn emit_js_static_method_augmentation_namespace(
        &mut self,
        group: &crate::declaration_emitter::helpers::JsStaticMethodAugmentationGroup,
    ) {
        let Some(class_node) = self.arena.get(group.class_idx) else {
            return;
        };
        let Some(class) = self.arena.get_class(class_node) else {
            return;
        };
        let Some(method_node) = self.arena.get(group.method_idx) else {
            return;
        };
        let Some(method) = self.arena.get_method_decl(method_node) else {
            return;
        };

        self.write_indent();
        if group.class_is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(group.class_is_exported) {
            self.write("declare ");
        }
        self.write("namespace ");
        self.emit_node(class.name);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        self.emit_js_namespace_function_member(
            method.name,
            method.type_parameters.as_ref(),
            &method.parameters,
            method.body,
            method.type_annotation,
        );

        self.write_indent();
        self.write("namespace ");
        self.emit_node(method.name);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        for &(prop_name, initializer) in &group.properties {
            let Some(init_node) = self.arena.get(initializer) else {
                continue;
            };
            if init_node.kind == syntax_kind_ext::ARROW_FUNCTION
                || init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            {
                let Some(func) = self.arena.get_function(init_node) else {
                    continue;
                };
                self.emit_js_namespace_function_member(
                    prop_name,
                    func.type_parameters.as_ref(),
                    &func.parameters,
                    func.body,
                    func.type_annotation,
                );
            } else if let Some(type_text) = self.js_namespace_value_member_type_text(initializer) {
                self.emit_js_namespace_value_member(prop_name, &type_text);
            }
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    fn emit_js_synthetic_prototype_class_if_needed(
        &mut self,
        name_idx: NodeIndex,
        is_exported: bool,
    ) {
        let Some(name) = self.get_identifier_text(name_idx) else {
            return;
        };
        let Some(methods) = self
            .js_deferred_prototype_method_statements
            .get(&name)
            .cloned()
        else {
            return;
        };
        if methods.is_empty() {
            return;
        }

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write("class ");
        self.emit_node(name_idx);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        for (method_name, initializer) in methods {
            self.emit_js_synthetic_class_method(method_name, initializer);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    fn emit_js_synthetic_class_method(&mut self, name_idx: NodeIndex, initializer: NodeIndex) {
        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };
        if init_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return;
        }
        let Some(func) = self.arena.get_function(init_node) else {
            return;
        };

        let jsdoc = self.function_like_jsdoc_for_node(initializer);

        self.write_indent();
        self.emit_node(name_idx);

        let jsdoc_template_params = if func
            .type_parameters
            .as_ref()
            .is_none_or(|type_params| type_params.nodes.is_empty())
        {
            jsdoc
                .as_deref()
                .map(Self::parse_jsdoc_template_params)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        if let Some(ref type_params) = func.type_parameters {
            if !type_params.nodes.is_empty() {
                self.emit_type_parameters(type_params);
            } else if !jsdoc_template_params.is_empty() {
                self.emit_jsdoc_template_parameters(&jsdoc_template_params);
            }
        } else if !jsdoc_template_params.is_empty() {
            self.emit_jsdoc_template_parameters(&jsdoc_template_params);
        }

        self.write("(");
        self.emit_parameters_with_body(&func.parameters, func.body);
        self.write(")");

        if func.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(func.type_annotation);
        } else if let Some(return_type_text) = jsdoc
            .as_deref()
            .and_then(Self::parse_jsdoc_return_type_text)
        {
            self.write(": ");
            self.write(&return_type_text);
        } else if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache) {
            let func_type_id = cache
                .node_types
                .get(&initializer.0)
                .copied()
                .or_else(|| self.get_node_type_or_names(&[name_idx, initializer]));
            if let Some(func_type_id) = func_type_id
                && let Some(return_type_id) = type_queries::get_return_type(*interner, func_type_id)
            {
                if return_type_id == tsz_solver::types::TypeId::ANY
                    && func.body.is_some()
                    && self.body_returns_void(func.body)
                {
                    self.write(": void");
                } else {
                    self.write(": ");
                    self.write(&self.print_type_id(return_type_id));
                }
            } else if func.body.is_some() && self.body_returns_void(func.body) {
                self.write(": void");
            }
        } else if func.body.is_some() && self.body_returns_void(func.body) {
            self.write(": void");
        }

        self.write(";");
        self.write_line();
    }

    fn emit_js_namespace_function_member(
        &mut self,
        name_idx: NodeIndex,
        type_params: Option<&NodeList>,
        parameters: &NodeList,
        body_idx: NodeIndex,
        type_annotation: NodeIndex,
    ) {
        self.write_indent();
        self.write("function ");
        self.emit_node(name_idx);
        if let Some(type_params) = type_params
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }
        self.write("(");
        self.emit_parameters_with_body(parameters, body_idx);
        self.write(")");
        if type_annotation.is_some() {
            self.write(": ");
            self.emit_type(type_annotation);
        } else if body_idx.is_some() && self.body_returns_void(body_idx) {
            self.write(": void");
        } else if !self.source_is_declaration_file {
            self.write(": any");
        }
        self.write(";");
        self.write_line();
    }

    fn emit_js_namespace_value_member(&mut self, name_idx: NodeIndex, type_text: &str) {
        self.write_indent();
        self.write("let ");
        self.emit_node(name_idx);
        self.write(": ");
        self.write(type_text);
        self.write(";");
        self.write_line();
    }

    fn js_namespace_value_member_type_text(&self, initializer: NodeIndex) -> Option<String> {
        let init_node = self.arena.get(initializer)?;
        match init_node.kind {
            k if k == SyntaxKind::StringLiteral as u16 => Some("string".to_string()),
            k if k == SyntaxKind::NumericLiteral as u16 => Some("number".to_string()),
            k if k == SyntaxKind::BigIntLiteral as u16 => Some("bigint".to_string()),
            k if k == SyntaxKind::TrueKeyword as u16 => Some("boolean".to_string()),
            k if k == SyntaxKind::FalseKeyword as u16 => Some("boolean".to_string()),
            k if k == SyntaxKind::NullKeyword as u16 => Some("null".to_string()),
            k if k == SyntaxKind::UndefinedKeyword as u16 => Some("undefined".to_string()),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if self.is_negative_literal(init_node) {
                    Some("number".to_string())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub(crate) fn js_synthetic_export_value_type_text(
        &self,
        initializer: NodeIndex,
    ) -> Option<String> {
        self.js_synthetic_export_value_type_text_inner(initializer, 0)
    }

    fn js_synthetic_export_value_type_text_inner(
        &self,
        initializer: NodeIndex,
        depth: u8,
    ) -> Option<String> {
        if depth > 4 {
            return None;
        }

        let init_node = self.arena.get(initializer)?;
        if init_node.kind == SyntaxKind::UndefinedKeyword as u16
            || self.is_void_expression(init_node)
        {
            return None;
        }

        match init_node.kind {
            k if k == SyntaxKind::StringLiteral as u16 => {
                return self.get_source_slice_no_semi(init_node.pos, init_node.end);
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                return self.get_source_slice_no_semi(init_node.pos, init_node.end);
            }
            k if k == SyntaxKind::BigIntLiteral as u16 => {
                return self.get_source_slice_no_semi(init_node.pos, init_node.end);
            }
            k if k == SyntaxKind::TrueKeyword as u16 => return Some("true".to_string()),
            k if k == SyntaxKind::FalseKeyword as u16 => return Some("false".to_string()),
            k if k == SyntaxKind::NullKeyword as u16 => return Some("null".to_string()),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if self.is_negative_literal(init_node) {
                    return self.get_source_slice_no_semi(init_node.pos, init_node.end);
                }
            }
            _ => {}
        }

        if let Some(type_id) = self.get_node_type_or_names(&[initializer])
            && type_id != tsz_solver::types::TypeId::ANY
        {
            return Some(self.print_type_id(type_id));
        }

        if init_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(reference_text) = self.nameable_constructor_expression_text(initializer)
            && let Some(assigned_initializer) =
                self.js_assigned_initializer_for_value_reference(initializer)
        {
            let printed =
                self.js_synthetic_export_value_type_text_inner(assigned_initializer, depth + 1)?;
            return Some(self.rewrite_recursive_js_class_expression_type(
                assigned_initializer,
                &reference_text,
                printed,
            ));
        }

        match init_node.kind {
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION =>
            {
                self.infer_fallback_type_text(initializer)
            }
            _ => self.js_namespace_value_member_type_text(initializer),
        }
    }

    pub(crate) fn is_void_expression(&self, node: &Node) -> bool {
        node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            && self
                .arena
                .get_unary_expr(node)
                .is_some_and(|unary| unary.operator == SyntaxKind::VoidKeyword as u16)
    }

    fn js_synthetic_export_value_keyword(&self, initializer: NodeIndex) -> &'static str {
        let resolved_initializer = self
            .js_assigned_initializer_for_value_reference(initializer)
            .unwrap_or(initializer);
        let Some(init_node) = self.arena.get(resolved_initializer) else {
            return "const ";
        };
        if init_node.kind == syntax_kind_ext::CLASS_EXPRESSION {
            "var "
        } else {
            "const "
        }
    }

    fn rewrite_recursive_js_class_expression_type(
        &self,
        initializer: NodeIndex,
        reference_text: &str,
        printed: String,
    ) -> String {
        let Some(class_expr) = self.arena.get_class_at(initializer) else {
            return printed;
        };

        let mut rewritten = printed;
        for &member_idx in &class_expr.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::METHOD_DECLARATION {
                continue;
            }
            let Some(method) = self.arena.get_method_decl(member_node) else {
                continue;
            };
            if method.type_annotation.is_some()
                || !method.body.is_some()
                || !self.function_body_returns_new_reference(method.body, reference_text)
            {
                continue;
            }
            let Some(method_name) = self.get_identifier_text(method.name) else {
                continue;
            };
            rewritten = rewritten.replacen(
                &format!("{method_name}(): any;"),
                &format!("{method_name}(): /*elided*/ any;"),
                1,
            );
        }

        rewritten
    }

    fn function_body_returns_new_reference(
        &self,
        body_idx: NodeIndex,
        reference_text: &str,
    ) -> bool {
        let Some(body_node) = self.arena.get(body_idx) else {
            return false;
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return false;
        };

        block
            .statements
            .nodes
            .iter()
            .copied()
            .any(|stmt_idx| self.statement_returns_new_reference(stmt_idx, reference_text))
    }

    fn statement_returns_new_reference(&self, stmt_idx: NodeIndex, reference_text: &str) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::RETURN_STATEMENT => self
                .arena
                .get_return_statement(stmt_node)
                .is_some_and(|ret| {
                    self.expression_is_new_reference(ret.expression, reference_text)
                }),
            k if k == syntax_kind_ext::BLOCK => {
                self.arena.get_block(stmt_node).is_some_and(|block| {
                    block
                        .statements
                        .nodes
                        .iter()
                        .copied()
                        .any(|nested| self.statement_returns_new_reference(nested, reference_text))
                })
            }
            k if k == syntax_kind_ext::IF_STATEMENT => self
                .arena
                .get_if_statement(stmt_node)
                .is_some_and(|if_data| {
                    self.statement_returns_new_reference(if_data.then_statement, reference_text)
                        || (if_data.else_statement.is_some()
                            && self.statement_returns_new_reference(
                                if_data.else_statement,
                                reference_text,
                            ))
                }),
            _ => false,
        }
    }

    fn expression_is_new_reference(&self, expr_idx: NodeIndex, reference_text: &str) -> bool {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return false;
        }
        let Some(new_expr) = self.arena.get_call_expr(expr_node) else {
            return false;
        };
        self.nameable_constructor_expression_text(new_expr.expression)
            .as_deref()
            == Some(reference_text)
    }

    fn preferred_binding_source_type(
        &self,
        type_annotation: NodeIndex,
        initializer: NodeIndex,
        related_nodes: &[NodeIndex],
    ) -> Option<tsz_solver::types::TypeId> {
        if type_annotation.is_some()
            && let Some(type_id) = self.get_node_type(type_annotation)
        {
            return Some(type_id);
        }
        if initializer.is_some()
            && let Some(type_id) = self.get_node_type(initializer)
        {
            return Some(type_id);
        }
        self.get_node_type_or_names(related_nodes)
    }

    fn destructuring_property_lookup_text(&self, node_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(node_idx)?;

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => self.get_identifier_text(node_idx),
            k if k == SyntaxKind::StringLiteral as u16 => {
                self.arena.get_literal(node).map(|lit| lit.text.clone())
            }
            k if k == SyntaxKind::NumericLiteral as u16 => self
                .arena
                .get_literal(node)
                .map(|lit| Self::normalize_numeric_literal(lit.text.as_ref())),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.arena.get_unary_expr(node)?;
                let operand_idx = self
                    .arena
                    .skip_parenthesized_and_assertions_and_comma(unary.operand);
                let operand_node = self.arena.get(operand_idx)?;
                if operand_node.kind != SyntaxKind::NumericLiteral as u16 {
                    return None;
                }
                let literal = self.arena.get_literal(operand_node)?;
                let normalized = Self::normalize_numeric_literal(literal.text.as_ref());
                match unary.operator {
                    k if k == SyntaxKind::MinusToken as u16 => Some(format!("-{normalized}")),
                    k if k == SyntaxKind::PlusToken as u16 => Some(normalized),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn array_binding_element_type(
        &self,
        tuple_elements: Option<&[tsz_solver::types::TupleElement]>,
        tuple_index: usize,
        array_element_type: Option<tsz_solver::types::TypeId>,
    ) -> Option<tsz_solver::types::TypeId> {
        if let Some(tuple_elements) = tuple_elements
            && let Some(tuple_element) = tuple_elements.get(tuple_index)
        {
            let mut type_id = if tuple_element.rest {
                self.type_interner.and_then(|interner| {
                    type_queries::get_array_element_type(interner, tuple_element.type_id)
                        .or(Some(tuple_element.type_id))
                })?
            } else {
                tuple_element.type_id
            };
            if tuple_element.optional
                && let Some(interner) = self.type_interner
            {
                type_id = interner.union(vec![type_id, tsz_solver::types::TypeId::UNDEFINED]);
            }
            return Some(type_id);
        }

        if array_element_type == Some(tsz_solver::types::TypeId::NEVER) {
            return Some(tsz_solver::types::TypeId::UNDEFINED);
        }

        array_element_type
    }

    fn array_rest_binding_type(
        &self,
        source_type: Option<tsz_solver::types::TypeId>,
        tuple_elements: Option<&[tsz_solver::types::TupleElement]>,
        tuple_index: usize,
        array_element_type: Option<tsz_solver::types::TypeId>,
    ) -> Option<tsz_solver::types::TypeId> {
        let interner = self.type_interner?;

        if tuple_index == 0
            && let Some(source_type) = source_type
            && let Some(union_type) =
                type_queries::get_tuple_element_type_union(interner, source_type)
        {
            return Some(interner.array(union_type));
        }

        if let Some(tuple_elements) = tuple_elements {
            let remaining = tuple_elements
                .get(tuple_index..)
                .map_or_else(Vec::new, ToOwned::to_owned);
            return Some(interner.tuple(remaining));
        }

        array_element_type.map(|element_type| interner.array(element_type))
    }

    fn object_binding_element_type(
        &self,
        source_type: Option<tsz_solver::types::TypeId>,
        element: &tsz_parser::parser::node::BindingElementData,
    ) -> Option<tsz_solver::types::TypeId> {
        let interner = self.type_interner?;
        let source_type = type_queries::unwrap_readonly(interner, source_type?);
        let property_name_idx = if element.property_name.is_some() {
            element.property_name
        } else {
            element.name
        };
        let property_name = self.destructuring_property_lookup_text(property_name_idx)?;
        let property =
            type_queries::find_property_in_type_by_str(interner, source_type, &property_name)?;
        if property.optional {
            Some(interner.union(vec![property.type_id, tsz_solver::types::TypeId::UNDEFINED]))
        } else {
            Some(property.type_id)
        }
    }

    fn collect_typed_bindings_recursive(
        &self,
        node_idx: NodeIndex,
        source_type: Option<tsz_solver::types::TypeId>,
        bindings: &mut Vec<(NodeIndex, Option<tsz_solver::types::TypeId>)>,
    ) {
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                let type_id = source_type
                    .filter(|type_id| *type_id != tsz_solver::types::TypeId::ANY)
                    .or_else(|| self.get_node_type(node_idx))
                    .or_else(|| self.get_type_via_symbol(node_idx));
                bindings.push((node_idx, type_id));
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                if let Some(element) = self.arena.get_binding_element(node) {
                    let effective_type = source_type
                        .filter(|type_id| *type_id != tsz_solver::types::TypeId::ANY)
                        .or_else(|| {
                            if element.initializer.is_some() {
                                self.get_node_type(element.initializer)
                            } else {
                                None
                            }
                        })
                        .or_else(|| {
                            self.get_node_type_or_names(&[
                                node_idx,
                                element.name,
                                element.initializer,
                            ])
                        });
                    self.collect_typed_bindings_recursive(element.name, effective_type, bindings);
                }
            }
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                let tuple_elements = self.type_interner.and_then(|interner| {
                    source_type
                        .and_then(|type_id| type_queries::get_tuple_elements(interner, type_id))
                });
                let array_element_type = self.type_interner.and_then(|interner| {
                    source_type.and_then(|type_id| {
                        type_queries::get_array_element_type(interner, type_id).or_else(|| {
                            type_queries::get_tuple_element_type_union(interner, type_id)
                        })
                    })
                });

                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    let mut tuple_index = 0usize;
                    for &element_idx in &pattern.elements.nodes {
                        let Some(element_node) = self.arena.get(element_idx) else {
                            continue;
                        };
                        if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                            tuple_index += 1;
                            continue;
                        }
                        if element_node.kind != syntax_kind_ext::BINDING_ELEMENT {
                            continue;
                        }
                        let Some(element) = self.arena.get_binding_element(element_node) else {
                            continue;
                        };
                        let element_type = if element.dot_dot_dot_token {
                            self.array_rest_binding_type(
                                source_type,
                                tuple_elements.as_deref(),
                                tuple_index,
                                array_element_type,
                            )
                        } else {
                            self.array_binding_element_type(
                                tuple_elements.as_deref(),
                                tuple_index,
                                array_element_type,
                            )
                        };
                        self.collect_typed_bindings_recursive(element_idx, element_type, bindings);
                        if !element.dot_dot_dot_token {
                            tuple_index += 1;
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    for &element_idx in &pattern.elements.nodes {
                        let Some(element_node) = self.arena.get(element_idx) else {
                            continue;
                        };
                        if element_node.kind != syntax_kind_ext::BINDING_ELEMENT {
                            continue;
                        }
                        let Some(element) = self.arena.get_binding_element(element_node) else {
                            continue;
                        };
                        let element_type = if element.dot_dot_dot_token {
                            source_type
                        } else {
                            self.object_binding_element_type(source_type, element)
                        };
                        self.collect_typed_bindings_recursive(element_idx, element_type, bindings);
                    }
                }
            }
            _ => {}
        }
    }

    fn collect_flattened_binding_entries(
        &self,
        pattern_idx: NodeIndex,
        source_type: Option<tsz_solver::types::TypeId>,
    ) -> Vec<(NodeIndex, Option<tsz_solver::types::TypeId>)> {
        let mut bindings = Vec::new();
        self.collect_typed_bindings_recursive(pattern_idx, source_type, &mut bindings);
        bindings
    }

    fn emit_flattened_binding_type_annotation(
        &mut self,
        ident_idx: NodeIndex,
        type_id: Option<tsz_solver::types::TypeId>,
    ) {
        let type_id = type_id
            .or_else(|| self.get_node_type(ident_idx))
            .or_else(|| self.get_type_via_symbol(ident_idx));
        self.write(": ");
        if let Some(type_id) = type_id {
            self.write(&self.print_type_id(type_id));
        } else {
            self.write("any");
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
        decl_idx: NodeIndex,
        keyword: &str,
        is_exported: bool,
    ) {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return;
        };
        let bindings = self.collect_flattened_binding_entries(
            decl.name,
            self.preferred_binding_source_type(
                decl.type_annotation,
                decl.initializer,
                &[decl_idx, decl.name, decl.initializer],
            ),
        );
        if bindings.is_empty() {
            return;
        }

        self.write_indent();
        if is_exported && (!self.inside_declare_namespace || self.ambient_module_has_scope_marker) {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write(keyword);
        self.write(" ");

        for (index, (ident_idx, type_id)) in bindings.into_iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            self.emit_node(ident_idx);
            self.emit_flattened_binding_type_annotation(ident_idx, type_id);
        }
        self.write(";");
        self.write_line();
    }

    fn emit_parameter_property_modifiers(&mut self, modifiers: &Option<NodeList>) -> bool {
        let mut is_private = false;
        if let Some(modifiers) = modifiers {
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
                        k if k == SyntaxKind::OverrideKeyword as u16 => {
                            // tsc strips `override` in .d.ts output.
                        }
                        _ => {}
                    }
                }
            }
        }
        is_private
    }

    /// Check if an initializer is a simple reference (identifier or qualified name)
    /// to a local import-equals alias (e.g. `import b = a.foo`).
    /// Returns the text to use after `typeof` if so (e.g. `"b"`).
    ///
    /// tsc emits `typeof <alias>` for variables initialized with an import-equals
    /// alias target rather than expanding the resolved type. This preserves the
    /// declarative reference in the .d.ts output.
    pub(super) fn initializer_import_alias_typeof_text(
        &self,
        initializer: NodeIndex,
    ) -> Option<String> {
        let binder = self.binder?;
        let init_node = self.arena.get(initializer)?;

        // Only handle simple identifier references.
        if init_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let ident = self.arena.get_identifier(init_node)?;
        let name = &ident.escaped_text;

        // Resolve the identifier by walking the scope chain from the enclosing scope.
        // The binder's node_symbols map only contains declaration-site mappings, not
        // usage-site references. We need to walk scopes to find the symbol for `b`.
        let scope_id = binder.find_enclosing_scope(self.arena, initializer)?;
        let sym_id = self.resolve_name_in_scope_chain(binder, scope_id, name)?;
        let sym = binder.symbols.get(sym_id)?;

        // Check if this symbol is an alias (import-equals creates ALIAS symbols)
        if sym.flags & tsz_binder::symbol_flags::ALIAS == 0 {
            return None;
        }

        // Verify that at least one declaration is an import-equals declaration
        let has_import_equals_decl = sym.declarations.iter().any(|&decl_idx| {
            self.arena
                .get(decl_idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION)
        });

        if !has_import_equals_decl {
            return None;
        }

        Some(name.clone())
    }

    /// Walk the scope chain from `scope_id` upward, looking for a symbol with the given name.
    fn resolve_name_in_scope_chain(
        &self,
        binder: &BinderState,
        start_scope: tsz_binder::scopes::ScopeId,
        name: &str,
    ) -> Option<SymbolId> {
        let mut scope_id = start_scope;
        let mut iterations = 0;
        while scope_id.is_some() {
            iterations += 1;
            if iterations > 100 {
                break;
            }
            let scope = binder.scopes.get(scope_id.0 as usize)?;
            if let Some(sym_id) = scope.table.get(name) {
                return Some(sym_id);
            }
            scope_id = scope.parent;
        }
        None
    }

    // Export/import emission → exports.rs
    // Type emission and utility helpers → helpers.rs
}
