//! Parallel Processing Module
//!
//! Provides parallel file parsing and processing using Rayon.
//! This enables significant speedups on multi-core machines.
//!
//! # Architecture
//!
//! The compilation pipeline has these parallelization opportunities:
//!
//! 1. **Parsing** - Each file can be parsed independently (embarrassingly parallel)
//! 2. **Binding** - After parsing, binding can be parallelized per-file
//! 3. **Type Checking** - Function bodies can be checked in parallel
//!    (once global symbols are merged)
//!
//! # Usage
//!
//! ```text
//! use tsz::parallel::parse_files_parallel;
//!
//! let files = vec![
//!     ("src/a.ts".to_string(), "let a = 1;".to_string()),
//!     ("src/b.ts".to_string(), "let b = 2;".to_string()),
//! ];
//!
//! let results = parse_files_parallel(files);
//! // results is Vec<ParseResult> with parsed ASTs
//! ```

use crate::binder::BinderOptions;
use crate::binder::BinderState;
use crate::binder::state::{BinderStateScopeInputs, CrossFileNodeSymbols, DeclarationArenaMap};
use crate::binder::{
    FlowNodeArena, FlowNodeId, Scope, ScopeId, SymbolArena, SymbolId, SymbolTable,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::config::resolve_default_lib_files;
use crate::emitter::ScriptTarget;
use crate::lib_loader;
use crate::parser::NodeIndex;
use crate::parser::NodeList;
use crate::parser::node::{NodeArena, SourceFileData};
use crate::parser::{ParseDiagnostic, ParserState};
use anyhow::{Context, Result, bail};
#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::{
    IndexedParallelIterator, IntoParallelIterator, IntoParallelRefIterator, ParallelIterator,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Once;
use tsz_common::interner::{Atom, Interner};
use tsz_scanner::SyntaxKind;

type ModuleExportEntry = FxHashMap<String, (String, Option<String>)>;
type Reexports = FxHashMap<String, ModuleExportEntry>;

/// Validate JSON syntax and return parse diagnostics for violations.
///
/// TypeScript's JSON parser enforces strict JSON rules when parsing `.json` files.
/// This validates property names must be double-quoted string literals (TS1327).
/// Violations include single-quoted strings, computed property names (`[expr]`),
/// and unquoted identifiers used as property names.
fn validate_json_syntax(source: &str) -> Vec<ParseDiagnostic> {
    let mut diagnostics = Vec::new();
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    // Track whether we're inside an object and expecting a property name.
    // JSON property names must be double-quoted strings per the JSON spec.
    // We use a simple state machine: after `{` or `,` inside an object,
    // the next non-whitespace token must be `"` (property name) or `}` (end).
    let mut object_depth: i32 = 0;
    let mut array_depth: i32 = 0;
    let mut expecting_property_name = false;

    while i < len {
        let b = bytes[i];

        // Skip whitespace
        if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
            i += 1;
            continue;
        }

        if b == b'{' {
            object_depth += 1;
            expecting_property_name = true;
            i += 1;
            continue;
        }

        if b == b'}' {
            object_depth -= 1;
            expecting_property_name = false;
            i += 1;
            continue;
        }

        if b == b'[' && !expecting_property_name {
            array_depth += 1;
            i += 1;
            continue;
        }

        if b == b']' && array_depth > 0 {
            array_depth -= 1;
            i += 1;
            continue;
        }

        if b == b',' {
            // After a comma inside an object (not array), expect a property name
            if object_depth > array_depth {
                expecting_property_name = true;
            }
            i += 1;
            continue;
        }

        if b == b':' {
            expecting_property_name = false;
            i += 1;
            continue;
        }

        // When expecting a property name, check what we got
        if expecting_property_name && object_depth > 0 {
            if b == b'"' {
                // Valid double-quoted property name - skip past the string
                expecting_property_name = false;
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' {
                        i += 2; // skip escape sequence
                    } else if bytes[i] == b'"' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
                continue;
            }

            // Not a double-quoted string in property name position → TS1327
            diagnostics.push(ParseDiagnostic {
                start: i as u32,
                length: 1,
                message: tsz_common::diagnostics::diagnostic_messages::STRING_LITERAL_WITH_DOUBLE_QUOTES_EXPECTED.to_string(),
                code: tsz_common::diagnostics::diagnostic_codes::STRING_LITERAL_WITH_DOUBLE_QUOTES_EXPECTED,
            });
            expecting_property_name = false;
        }

        // Skip over strings (both single and double quoted) to avoid false matches
        if b == b'"' || b == b'\'' {
            i += 1;
            while i < len {
                if bytes[i] == b'\\' {
                    i += 2;
                } else if bytes[i] == b {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
            continue;
        }

        i += 1;
    }

    diagnostics
}

fn synthesize_json_bind_result(file_name: String, source_text: String) -> BindResult {
    let parse_diagnostics = validate_json_syntax(&source_text);

    let mut arena = NodeArena::new();
    let end_pos = source_text.len() as u32;
    let eof_token = arena.add_token(SyntaxKind::EndOfFileToken as u16, end_pos, end_pos);
    let source_file = arena.add_source_file(
        0,
        end_pos,
        SourceFileData {
            statements: NodeList::default(),
            end_of_file_token: eof_token,
            file_name: file_name.clone(),
            text: Arc::<str>::from(source_text),
            language_version: 99,
            language_variant: 0,
            script_kind: 3,
            is_declaration_file: false,
            has_no_default_lib: false,
            comments: Vec::new(),
            parent: NodeIndex::NONE,
            id: 0,
            modifier_flags: 0,
            transform_flags: 0,
        },
    );

    let mut binder = BinderState::new();
    binder.set_debug_file(&file_name);
    binder.bind_source_file(&arena, source_file);

    BindResult {
        file_name,
        source_file,
        arena: Arc::new(arena),
        symbols: binder.symbols,
        file_locals: binder.file_locals,
        declared_modules: binder.declared_modules,
        module_exports: binder.module_exports,
        node_symbols: binder.node_symbols,
        module_declaration_exports_publicly: binder.module_declaration_exports_publicly,
        symbol_arenas: binder.symbol_arenas,
        declaration_arenas: binder.declaration_arenas,
        scopes: binder.scopes,
        node_scope_ids: binder.node_scope_ids,
        parse_diagnostics,
        shorthand_ambient_modules: binder.shorthand_ambient_modules,
        global_augmentations: binder.global_augmentations,
        module_augmentations: binder.module_augmentations,
        augmentation_target_modules: binder.augmentation_target_modules,
        reexports: binder.reexports,
        wildcard_reexports: binder.wildcard_reexports,
        wildcard_reexports_type_only: binder.wildcard_reexports_type_only,
        lib_binders: Vec::new(),
        lib_arenas: Vec::new(),
        lib_symbol_ids: binder.lib_symbol_ids,
        lib_symbol_reverse_remap: binder.lib_symbol_reverse_remap,
        flow_nodes: binder.flow_nodes,
        node_flow: binder.node_flow,
        switch_clause_to_switch: std::mem::take(&mut binder.switch_clause_to_switch),
        is_external_module: binder.is_external_module,
        expando_properties: std::mem::take(&mut binder.expando_properties),
        alias_partners: binder.alias_partners,
        file_features: binder.file_features,
        semantic_defs: binder.semantic_defs,
    }
}

#[cfg(target_arch = "wasm32")]
fn resolve_default_lib_files(_target: ScriptTarget) -> anyhow::Result<Vec<PathBuf>> {
    Ok(Vec::new())
}

#[cfg(not(target_arch = "wasm32"))]
static RAYON_POOL_INIT: Once = Once::new();

/// Ensure Rayon global pool is configured once with stack size suitable for checker recursion.
///
/// We initialize lazily to avoid paying global pool startup cost for single-file sequential paths.
#[cfg(not(target_arch = "wasm32"))]
pub fn ensure_rayon_global_pool() {
    RAYON_POOL_INIT.call_once(|| {
        // If the pool was already initialized through another rayon call, keep going.
        let _ = rayon::ThreadPoolBuilder::new()
            .stack_size(8 * 1024 * 1024)
            .build_global();
    });
}

#[cfg(target_arch = "wasm32")]
pub fn ensure_rayon_global_pool() {}

/// Conditionally use parallel or sequential iteration based on target.
/// For WASM, Rayon parallelism creates oversubscription when combined with
/// external worker-level parallelism (e.g., Node worker threads in conformance tests).
/// This causes worker crashes and OOM issues.
///
/// Usage:
/// - `maybe_parallel_iter!(collection)` for `.par_iter()` / `.iter()`
/// - `maybe_parallel_into!(collection)` for `.into_par_iter()` / `.into_iter()`
#[cfg(target_arch = "wasm32")]
macro_rules! maybe_parallel_iter {
    ($iter:expr) => {
        $iter.iter()
    };
}

#[cfg(not(target_arch = "wasm32"))]
macro_rules! maybe_parallel_iter {
    ($iter:expr) => {
        $iter.par_iter()
    };
}

#[cfg(target_arch = "wasm32")]
macro_rules! maybe_parallel_into {
    ($iter:expr) => {
        $iter.into_iter()
    };
}

#[cfg(not(target_arch = "wasm32"))]
macro_rules! maybe_parallel_into {
    ($iter:expr) => {
        $iter.into_par_iter()
    };
}

/// Result of parsing a single file
pub struct ParseResult {
    /// File name
    pub file_name: String,
    /// The parsed source file node index
    pub source_file: NodeIndex,
    /// The arena containing all nodes
    pub arena: NodeArena,
    /// Parse diagnostics
    pub parse_diagnostics: Vec<ParseDiagnostic>,
}

/// Parse multiple files in parallel using Parser
///
/// Each file is parsed independently, producing its own arena.
/// This is optimal for initial parsing before symbol resolution.
///
/// # Arguments
/// * `files` - Vector of (`file_name`, `source_text`) pairs
///
/// # Returns
/// Vector of `ParseResult` for each file
pub fn parse_files_parallel(files: Vec<(String, String)>) -> Vec<ParseResult> {
    #[cfg(not(target_arch = "wasm32"))]
    ensure_rayon_global_pool();

    maybe_parallel_into!(files)
        .map(|(file_name, source_text)| {
            let mut parser = ParserState::new(file_name.clone(), source_text);
            let source_file = parser.parse_source_file();

            // Consume the parser and take its arena/diagnostics
            let (arena, parse_diagnostics) = parser.into_parts();

            ParseResult {
                file_name,
                source_file,
                arena,
                parse_diagnostics,
            }
        })
        .collect()
}

/// Parse a single file (for comparison/testing)
pub fn parse_file_single(file_name: String, source_text: String) -> ParseResult {
    let mut parser = ParserState::new(file_name.clone(), source_text);
    let source_file = parser.parse_source_file();

    // Consume the parser and take its arena/diagnostics
    let (arena, parse_diagnostics) = parser.into_parts();

    ParseResult {
        file_name,
        source_file,
        arena,
        parse_diagnostics,
    }
}

/// Statistics about parallel parsing performance
#[derive(Debug, Clone)]
pub struct ParallelStats {
    /// Number of files parsed
    pub file_count: usize,
    /// Total source bytes
    pub total_bytes: usize,
    /// Total nodes created
    pub total_nodes: usize,
    /// Number of parse errors
    pub error_count: usize,
}

// =============================================================================
// Parallel Binding
// =============================================================================

/// Result of binding a single file
pub struct BindResult {
    /// File name
    pub file_name: String,
    /// The parsed source file node index
    pub source_file: NodeIndex,
    /// The arena containing all nodes
    pub arena: Arc<NodeArena>,
    /// Symbols created in this file
    pub symbols: SymbolArena,
    /// File-level symbol table (exports, declarations)
    pub file_locals: SymbolTable,
    /// Ambient module declarations by specifier
    pub declared_modules: FxHashSet<String>,
    /// Module exports keyed by specifier or file name
    pub module_exports: FxHashMap<String, SymbolTable>,
    /// Node-to-symbol mapping
    pub node_symbols: FxHashMap<u32, SymbolId>,
    /// Export visibility of namespace/module declaration nodes after binder rules.
    pub module_declaration_exports_publicly: FxHashMap<u32, bool>,
    /// Symbol-to-arena mapping for cross-file declaration lookup (including lib symbols)
    pub symbol_arenas: FxHashMap<SymbolId, Arc<NodeArena>>,
    /// Declaration-to-arena mapping for precise cross-file declaration lookup
    pub declaration_arenas: DeclarationArenaMap,
    /// Persistent scopes for stateless checking
    pub scopes: Vec<Scope>,
    /// Map from AST node to scope ID
    pub node_scope_ids: FxHashMap<u32, ScopeId>,
    /// Parse diagnostics
    pub parse_diagnostics: Vec<ParseDiagnostic>,
    /// Shorthand ambient modules (`declare module "foo"` without body)
    pub shorthand_ambient_modules: FxHashSet<String>,
    /// Global augmentations (interface declarations inside `declare global` blocks)
    pub global_augmentations: FxHashMap<String, Vec<crate::binder::GlobalAugmentation>>,
    /// Module augmentations (interface/type declarations inside `declare module 'x'` blocks)
    /// Maps module specifier -> [`ModuleAugmentation`]
    pub module_augmentations: FxHashMap<String, Vec<crate::binder::ModuleAugmentation>>,
    /// Maps symbols declared inside module augmentation blocks to their target module specifier
    pub augmentation_target_modules: FxHashMap<SymbolId, String>,
    /// Re-exports: tracks `export { x } from 'module'` declarations
    pub reexports: Reexports,
    /// Wildcard re-exports: tracks `export * from 'module'` declarations
    pub wildcard_reexports: FxHashMap<String, Vec<String>>,
    /// Wildcard re-export type-only provenance aligned with `wildcard_reexports`.
    pub wildcard_reexports_type_only: FxHashMap<String, Vec<(String, bool)>>,
    /// Lib binders for global type resolution (Array, String, etc.)
    /// These are merged from lib.d.ts files and enable cross-file symbol lookup
    pub lib_binders: Vec<Arc<BinderState>>,
    /// Arenas corresponding to each `lib_binder` (same order/length as `lib_binders`).
    /// Used by `merge_bind_results_ref` to populate `declaration_arenas` for lib symbols.
    pub lib_arenas: Vec<Arc<NodeArena>>,
    /// Symbol IDs that originated from lib files (pre-merge local IDs)
    pub lib_symbol_ids: FxHashSet<SymbolId>,
    /// Reverse mapping from user-local lib symbol IDs to (`lib_binder_ptr`, `original_local_id`)
    pub lib_symbol_reverse_remap: FxHashMap<SymbolId, (usize, SymbolId)>,
    /// Flow nodes for control flow analysis
    pub flow_nodes: FlowNodeArena,
    /// Node-to-flow mapping: tracks which flow node was active at each AST node
    pub node_flow: FxHashMap<u32, FlowNodeId>,
    /// Map from switch clause `NodeIndex` to parent switch statement `NodeIndex`
    /// Used by control flow analysis for switch exhaustiveness checking
    pub switch_clause_to_switch: FxHashMap<u32, NodeIndex>,
    /// Whether this file is an external module (has imports/exports)
    pub is_external_module: bool,
    /// Expando property assignments detected during binding
    pub expando_properties: FxHashMap<String, FxHashSet<String>>,
    /// Per-file alias partners from binder (`TYPE_ALIAS` → `ALIAS` mapping, pre-remap)
    pub alias_partners: FxHashMap<SymbolId, SymbolId>,
    pub file_features: crate::binder::FileFeatures,
    /// Binder-captured semantic definitions for top-level declarations (Phase 1 DefId-first).
    /// Maps pre-remap `SymbolId` → `SemanticDefEntry`.
    pub semantic_defs: FxHashMap<SymbolId, crate::binder::SemanticDefEntry>,
}

/// Parse and bind multiple files in parallel
///
/// Each file is parsed and bound independently. The binding creates
/// file-local symbols which can later be merged into a global scope.
///
/// # Arguments
/// * `files` - Vector of (`file_name`, `source_text`) pairs
///
/// # Returns
/// Vector of `BindResult` for each file
pub fn parse_and_bind_parallel(files: Vec<(String, String)>) -> Vec<BindResult> {
    #[cfg(not(target_arch = "wasm32"))]
    ensure_rayon_global_pool();

    maybe_parallel_into!(files)
        .map(|(file_name, source_text)| {
            // Skip parsing .json files - they should not be parsed as TypeScript.
            // JSON module imports should be resolved during module resolution and
            // emit TS2732 if resolveJsonModule is false.
            if file_name.ends_with(".json") {
                return synthesize_json_bind_result(file_name, source_text);
            }

            // Parse
            let mut parser = ParserState::new(file_name.clone(), source_text);
            let source_file = parser.parse_source_file();

            let (arena, parse_diagnostics) = parser.into_parts();

            // Bind
            let mut binder = BinderState::new();
            binder.set_debug_file(&file_name);
            binder.bind_source_file(&arena, source_file);

            BindResult {
                file_name,
                source_file,
                arena: Arc::new(arena),
                symbols: binder.symbols,
                file_locals: binder.file_locals,
                declared_modules: binder.declared_modules,
                module_exports: binder.module_exports,
                node_symbols: binder.node_symbols,
                module_declaration_exports_publicly: binder.module_declaration_exports_publicly,
                symbol_arenas: binder.symbol_arenas,
                declaration_arenas: binder.declaration_arenas,
                scopes: binder.scopes,
                node_scope_ids: binder.node_scope_ids,
                parse_diagnostics,
                shorthand_ambient_modules: binder.shorthand_ambient_modules,
                global_augmentations: binder.global_augmentations,
                module_augmentations: binder.module_augmentations,
                augmentation_target_modules: binder.augmentation_target_modules,
                reexports: binder.reexports,
                wildcard_reexports: binder.wildcard_reexports,
                wildcard_reexports_type_only: binder.wildcard_reexports_type_only,
                lib_binders: Vec::new(), // No libs in this path
                lib_arenas: Vec::new(),
                lib_symbol_ids: binder.lib_symbol_ids,
                lib_symbol_reverse_remap: binder.lib_symbol_reverse_remap,
                flow_nodes: binder.flow_nodes,
                node_flow: binder.node_flow,
                switch_clause_to_switch: std::mem::take(&mut binder.switch_clause_to_switch),
                is_external_module: binder.is_external_module,
                expando_properties: std::mem::take(&mut binder.expando_properties),
                alias_partners: binder.alias_partners,
                file_features: binder.file_features,
                semantic_defs: binder.semantic_defs,
            }
        })
        .collect()
}

/// Bind a single file (for comparison/testing)
pub fn parse_and_bind_single(file_name: String, source_text: String) -> BindResult {
    if file_name.ends_with(".json") {
        return synthesize_json_bind_result(file_name, source_text);
    }

    let mut parser = ParserState::new(file_name.clone(), source_text);
    let source_file = parser.parse_source_file();

    let (arena, parse_diagnostics) = parser.into_parts();

    let mut binder = BinderState::new();
    binder.set_debug_file(&file_name);
    binder.bind_source_file(&arena, source_file);

    BindResult {
        file_name,
        source_file,
        arena: Arc::new(arena),
        symbols: binder.symbols,
        file_locals: binder.file_locals,
        declared_modules: binder.declared_modules,
        module_exports: binder.module_exports,
        node_symbols: binder.node_symbols,
        module_declaration_exports_publicly: binder.module_declaration_exports_publicly,
        symbol_arenas: binder.symbol_arenas,
        declaration_arenas: binder.declaration_arenas,
        scopes: binder.scopes,
        node_scope_ids: binder.node_scope_ids,
        parse_diagnostics,
        shorthand_ambient_modules: binder.shorthand_ambient_modules,
        global_augmentations: binder.global_augmentations,
        module_augmentations: binder.module_augmentations,
        augmentation_target_modules: binder.augmentation_target_modules,
        reexports: binder.reexports,
        wildcard_reexports: binder.wildcard_reexports,
        wildcard_reexports_type_only: binder.wildcard_reexports_type_only,
        lib_binders: Vec::new(), // No libs in this path
        lib_arenas: Vec::new(),
        lib_symbol_ids: binder.lib_symbol_ids,
        lib_symbol_reverse_remap: binder.lib_symbol_reverse_remap,
        flow_nodes: binder.flow_nodes,
        node_flow: binder.node_flow,
        switch_clause_to_switch: std::mem::take(&mut binder.switch_clause_to_switch),
        is_external_module: binder.is_external_module,
        expando_properties: std::mem::take(&mut binder.expando_properties),
        alias_partners: binder.alias_partners,
        file_features: binder.file_features,
        semantic_defs: binder.semantic_defs,
    }
}

/// Statistics about parallel binding performance
#[derive(Debug, Clone)]
pub struct BindStats {
    /// Number of files bound
    pub file_count: usize,
    /// Total nodes across all files
    pub total_nodes: usize,
    /// Total symbols created
    pub total_symbols: usize,
    /// Number of parse errors
    pub parse_error_count: usize,
}

/// Parse and bind files with statistics
pub fn parse_and_bind_with_stats(files: Vec<(String, String)>) -> (Vec<BindResult>, BindStats) {
    let file_count = files.len();
    let results = parse_and_bind_parallel(files);

    let total_nodes: usize = results.iter().map(|r| r.arena.len()).sum();
    let total_symbols: usize = results.iter().map(|r| r.symbols.len()).sum();
    let parse_error_count: usize = results.iter().map(|r| r.parse_diagnostics.len()).sum();

    let stats = BindStats {
        file_count,
        total_nodes,
        total_symbols,
        parse_error_count,
    };

    (results, stats)
}

/// Load lib.d.ts files and create `LibContext` objects for the binder.
///
/// This function loads the specified lib.d.ts files (e.g., lib.dom.d.ts, lib.es*.d.ts)
/// and returns `LibContext` objects that can be used during binding to resolve global
/// symbols like `console`, `Array`, `Promise`, etc.
///
/// This is similar to `load_lib_files_for_contexts` in driver.rs but returns
/// Arc<LibFile> objects for use with `merge_lib_symbols`.
pub fn load_lib_files_for_binding(lib_files: &[&Path]) -> Vec<Arc<lib_loader::LibFile>> {
    use crate::parser::ParserState;
    use rayon::prelude::{IntoParallelIterator, ParallelIterator};

    if lib_files.is_empty() {
        return Vec::new();
    }

    // Collect paths that exist
    let files_to_load: Vec<_> = lib_files
        .iter()
        .filter_map(|p| {
            let path = p.to_path_buf();
            path.exists().then_some(path)
        })
        .collect();

    // Parse and bind lib files in parallel for faster startup
    files_to_load
        .into_par_iter()
        .filter_map(|lib_path| {
            // Read the lib file content
            let source_text = std::fs::read_to_string(&lib_path).ok()?;

            // Parse the lib file
            let file_name = lib_path.to_string_lossy().to_string();
            let mut lib_parser = ParserState::new(file_name.clone(), source_text);
            let source_file_idx = lib_parser.parse_source_file();

            // Skip if there are parse errors
            if !lib_parser.get_diagnostics().is_empty() {
                return None;
            }

            // Bind the lib file
            let mut lib_binder = BinderState::new();
            lib_binder.bind_source_file(lib_parser.get_arena(), source_file_idx);

            // Create the LibFile
            let arena = Arc::new(lib_parser.into_arena());
            let binder = Arc::new(lib_binder);

            Some(Arc::new(lib_loader::LibFile::new(
                file_name,
                arena,
                binder,
                source_file_idx,
            )))
        })
        .collect()
}

/// Load lib.d.ts files from disk for binding, failing on any load/parse error.
///
/// Unlike `load_lib_files_for_binding`, this enforces strict disk-loading semantics:
/// missing files, unreadable files, and parse errors are surfaced as hard errors.
pub fn load_lib_files_for_binding_strict(
    lib_files: &[&Path],
) -> Result<Vec<Arc<lib_loader::LibFile>>> {
    if lib_files.is_empty() {
        return Ok(Vec::new());
    }

    // Phase 1: Read all files and resolve references.
    //
    // OPTIMIZATION: Pre-read ALL .d.ts files in the lib directory into memory
    // before processing references. This batches all file I/O upfront instead
    // of interleaving reads with reference resolution, reducing the impact of
    // I/O contention under system load. On a loaded system (load avg 20+),
    // individual file reads can take 10-20ms each due to scheduling delays.
    // Batch reading brings this down to ~2-5ms total for the entire directory.
    // Check if all requested lib files are available as embedded content.
    // If so, skip the batch disk read entirely (zero I/O startup).
    let all_embedded = lib_files.iter().all(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(crate::embedded_libs::is_embedded_lib)
    });

    let lib_dir = lib_files
        .first()
        .and_then(|p| p.parent())
        .unwrap_or(Path::new("."));
    let mut file_cache: FxHashMap<PathBuf, String> = FxHashMap::default();
    if !all_embedded {
        // Custom lib dir or non-embedded files — read from disk
        if let Ok(entries) = std::fs::read_dir(lib_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "ts")
                    && let Ok(content) = std::fs::read_to_string(&path)
                {
                    file_cache.insert(path, content);
                }
            }
        }
    }

    let mut loaded = FxHashSet::default();
    let mut file_contents: Vec<(String, String)> = Vec::new();
    for path in lib_files {
        collect_lib_files_recursive_cached(path, &mut loaded, &mut file_contents, &file_cache)?;
    }

    if file_contents.is_empty() {
        return Ok(Vec::new());
    }

    // Phase 2: Parse and bind all files in parallel (CPU bound — the expensive part).
    // Sort largest files first so rayon's work-stealing starts them early.
    // dom.d.ts (40K lines, 2MB) dominates parse time — without this sort it's
    // file #81 of 87 and becomes the critical-path bottleneck.
    file_contents.sort_by_key(|b| std::cmp::Reverse(b.1.len()));

    // Parse and bind all lib files in parallel using the global rayon pool.
    // The global pool threads are already warm (no thread creation overhead).
    #[cfg(not(target_arch = "wasm32"))]
    ensure_rayon_global_pool();

    let results: Vec<Result<Arc<lib_loader::LibFile>>> = maybe_parallel_into!(file_contents)
        .map(|(file_name, source_text)| parse_and_bind_lib_file(file_name, source_text))
        .collect();

    // Collect results, propagating any parse errors
    results.into_iter().collect()
}

/// Parse and bind a single lib file, returning a `LibFile` or error.
fn parse_and_bind_lib_file(
    file_name: String,
    source_text: String,
) -> Result<Arc<lib_loader::LibFile>> {
    let mut lib_parser = ParserState::new(file_name.clone(), source_text);
    let source_file_idx = lib_parser.parse_source_file();
    let diagnostics = lib_parser.get_diagnostics();
    if !diagnostics.is_empty() {
        let first = &diagnostics[0];
        bail!(
            "failed to parse lib file {} ({}:{}): {}",
            file_name,
            first.start,
            first.length,
            first.message
        );
    }

    let mut lib_binder = BinderState::new();
    lib_binder.bind_source_file(lib_parser.get_arena(), source_file_idx);

    let arena = Arc::new(lib_parser.into_arena());
    let binder = Arc::new(lib_binder);
    Ok(Arc::new(lib_loader::LibFile::new(
        file_name,
        arena,
        binder,
        source_file_idx,
    )))
}

/// Phase 1 helper with pre-loaded file cache. Uses embedded lib contents
/// first (zero I/O), then pre-read file cache, then disk as last resort.
fn collect_lib_files_recursive_cached(
    path: &Path,
    loaded: &mut FxHashSet<PathBuf>,
    file_contents: &mut Vec<(String, String)>,
    file_cache: &FxHashMap<PathBuf, String>,
) -> Result<()> {
    // Skip canonicalize (stat syscall) when using embedded content.
    let basename_check = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let lib_path = if crate::embedded_libs::is_embedded_lib(basename_check) && file_cache.is_empty()
    {
        path.to_path_buf()
    } else {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    };
    if !loaded.insert(lib_path.clone()) {
        return Ok(());
    }

    // Priority: embedded (comment-stripped, 58% smaller) > disk cache > disk read.
    // Embedded libs contain the same declarations as disk files but with comments
    // removed at build time, reducing parse work by ~58%. This is safe because
    // declaration files don't use comments for semantics.
    let basename = lib_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let source_text = if let Some(embedded) = crate::embedded_libs::get_lib_content(basename) {
        // Built-in embedded content — zero I/O, comment-stripped for faster parsing
        embedded.to_string()
    } else if let Some(cached) = file_cache.get(&lib_path) {
        // File was read from disk (custom lib dir with non-standard files) — use it
        cached.clone()
    } else {
        // Fallback to disk read
        std::fs::read_to_string(&lib_path)
            .with_context(|| format!("failed to read lib file {}", lib_path.display()))?
    };

    // Resolve references before adding this file (dependencies come first)
    for ref_lib in parse_lib_references(&source_text) {
        if let Some(ref_path) = resolve_lib_reference_path(&lib_path, &ref_lib) {
            collect_lib_files_recursive_cached(&ref_path, loaded, file_contents, file_cache)?;
        }
    }

    let file_name = lib_path.to_string_lossy().to_string();
    file_contents.push((file_name, source_text));
    Ok(())
}

fn parse_lib_references(content: &str) -> Vec<String> {
    let mut refs = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        // References are always at the top of lib files. Once we see a line
        // that isn't a comment, empty, or copyright header, stop scanning.
        // This avoids iterating through 40K+ lines for dom.d.ts.
        if !trimmed.is_empty()
            && !trimmed.starts_with("///")
            && !trimmed.starts_with("//")
            && !trimmed.starts_with("/*")
            && !trimmed.starts_with('*')
        {
            break;
        }
        if !trimmed.starts_with("///") {
            continue;
        }
        if let Some(start) = trimmed.find("<reference") {
            let rest = &trimmed[start..];
            if let Some(lib_start) = rest.find("lib=") {
                let after_lib = &rest[lib_start + 4..];
                let quote = after_lib.chars().next();
                if quote == Some('"') || quote == Some('\'') {
                    let quote_char = quote
                        .expect("guarded by quote == Some('\"') || quote == Some('\\'') check");
                    let value_start = 1;
                    if let Some(end) = after_lib[value_start..].find(quote_char) {
                        refs.push(
                            after_lib[value_start..value_start + end]
                                .trim()
                                .to_lowercase(),
                        );
                    }
                }
            }
        }
    }
    refs
}

fn resolve_lib_reference_path(base_path: &Path, lib_name: &str) -> Option<PathBuf> {
    let lib_dir = base_path.parent()?;
    let normalized = normalize_lib_reference_name(lib_name);
    let mut candidate_names = vec![normalized.clone()];
    match normalized.as_str() {
        // Source-tree libs use *.generated.d.ts while built/local and npm libs use plain names.
        "dom" => candidate_names.push("dom.generated".to_string()),
        "dom.iterable" => candidate_names.push("dom.iterable.generated".to_string()),
        "dom.asynciterable" => candidate_names.push("dom.asynciterable.generated".to_string()),
        "dom.generated" => candidate_names.push("dom".to_string()),
        "dom.iterable.generated" => candidate_names.push("dom.iterable".to_string()),
        "dom.asynciterable.generated" => candidate_names.push("dom.asynciterable".to_string()),
        _ => {}
    }
    let candidates: Vec<PathBuf> = candidate_names
        .into_iter()
        .flat_map(|name| {
            [
                lib_dir.join(format!("lib.{name}.d.ts")),
                lib_dir.join(format!("{name}.d.ts")),
            ]
        })
        .collect();
    // Check embedded libs first (no syscall), then fall back to disk stat.
    candidates.into_iter().find(|candidate| {
        candidate
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(crate::embedded_libs::is_embedded_lib)
            || candidate.exists()
    })
}

fn normalize_lib_reference_name(name: &str) -> String {
    match name.to_lowercase().trim() {
        "es6" => "es6".to_string(),
        "es7" => "es2016".to_string(),
        "lib" | "lib.d.ts" => "es5".to_string(),
        // Modern TypeScript (6.x) uses lib.dom.d.ts directly, not .generated suffix.
        // Pass through as-is — the file candidates already include lib.{name}.d.ts.
        "dom" | "dom.iterable" | "dom.asynciterable" => name.to_lowercase(),
        s if s.starts_with("lib.") && s.ends_with(".d.ts") => {
            let inner = &s[4..s.len() - 5];
            normalize_lib_reference_name(inner)
        }
        other => other.to_string(),
    }
}

/// Parse and bind multiple files in parallel with lib symbol injection.
///
/// This is the main entry point for compilation that includes lib.d.ts symbols.
/// Lib files are loaded first, then each file is parsed and bound with lib symbols
/// merged into its binder.
///
/// # Arguments
/// * `files` - Vector of (`file_name`, `source_text`) pairs
/// * `lib_files` - Optional list of lib file paths to load
///
/// # Returns
/// Vector of `BindResult` for each file
pub fn parse_and_bind_parallel_with_lib_files(
    files: Vec<(String, String)>,
    lib_files: &[&Path],
) -> Vec<BindResult> {
    // Load lib files for binding.
    // This path is intentionally strict so missing/unreadable lib files are not ignored.
    let lib_contexts = load_lib_files_for_binding_strict(lib_files)
        .unwrap_or_else(|err| panic!("failed to load lib files from disk: {err}"));

    // Parse and bind with lib symbols
    parse_and_bind_parallel_with_libs(files, &lib_contexts)
}

/// Parse and bind multiple files in parallel with lib contexts.
///
/// Lib symbols are injected into each file's binder during binding,
/// enabling resolution of global symbols like `console`, `Array`, etc.
///
/// # Arguments
/// * `files` - Vector of (`file_name`, `source_text`) pairs
/// * `lib_files` - Lib files to merge into each binder
///
/// # Returns
/// Vector of `BindResult` for each file
pub fn parse_and_bind_parallel_with_libs(
    files: Vec<(String, String)>,
    lib_files: &[Arc<lib_loader::LibFile>],
) -> Vec<BindResult> {
    if files.len() <= 1 {
        return files
            .into_iter()
            .map(|(file_name, source_text)| bind_file_with_libs(file_name, source_text, lib_files))
            .collect();
    }

    #[cfg(not(target_arch = "wasm32"))]
    ensure_rayon_global_pool();

    maybe_parallel_into!(files)
        .map(|(file_name, source_text)| bind_file_with_libs(file_name, source_text, lib_files))
        .collect()
}

fn bind_file_with_libs(
    file_name: String,
    source_text: String,
    lib_files: &[Arc<lib_loader::LibFile>],
) -> BindResult {
    // Skip parsing .json files - they should not be parsed as TypeScript.
    // JSON module imports should be resolved during module resolution and
    // emit TS2732 if resolveJsonModule is false.
    if file_name.ends_with(".json") {
        return synthesize_json_bind_result(file_name, source_text);
    }

    // Parse
    let mut parser = ParserState::new(file_name.clone(), source_text);
    let source_file = parser.parse_source_file();

    let (arena, parse_diagnostics) = parser.into_parts();

    // Bind with lib symbols
    let mut binder = BinderState::new();
    binder.set_debug_file(&file_name);

    // IMPORTANT: Merge lib symbols BEFORE binding source file
    // so that symbols like console, Array, Promise are available during binding
    if !lib_files.is_empty() {
        binder.merge_lib_symbols(lib_files);
    }

    binder.bind_source_file(&arena, source_file);

    // Extract lib_binders and lib_arenas from binder before it's moved
    let lib_binders = binder.lib_binders.clone();
    let lib_arenas: Vec<Arc<NodeArena>> =
        lib_files.iter().map(|lf| Arc::clone(&lf.arena)).collect();

    BindResult {
        file_name,
        source_file,
        arena: Arc::new(arena),
        symbols: binder.symbols,
        file_locals: binder.file_locals,
        declared_modules: binder.declared_modules,
        module_exports: binder.module_exports,
        node_symbols: binder.node_symbols,
        module_declaration_exports_publicly: binder.module_declaration_exports_publicly,
        symbol_arenas: binder.symbol_arenas,
        declaration_arenas: binder.declaration_arenas,
        scopes: binder.scopes,
        node_scope_ids: binder.node_scope_ids,
        parse_diagnostics,
        shorthand_ambient_modules: binder.shorthand_ambient_modules,
        global_augmentations: binder.global_augmentations,
        module_augmentations: binder.module_augmentations,
        augmentation_target_modules: binder.augmentation_target_modules,
        reexports: binder.reexports,
        wildcard_reexports: binder.wildcard_reexports,
        wildcard_reexports_type_only: binder.wildcard_reexports_type_only,
        lib_binders,
        lib_arenas,
        lib_symbol_ids: binder.lib_symbol_ids,
        lib_symbol_reverse_remap: binder.lib_symbol_reverse_remap,
        flow_nodes: binder.flow_nodes,
        node_flow: binder.node_flow,
        switch_clause_to_switch: std::mem::take(&mut binder.switch_clause_to_switch),
        is_external_module: binder.is_external_module,
        expando_properties: std::mem::take(&mut binder.expando_properties),
        alias_partners: binder.alias_partners,
        file_features: binder.file_features,
        semantic_defs: binder.semantic_defs,
    }
}

// =============================================================================
// File Skeleton IR
// =============================================================================
//
// The skeleton captures the minimal per-file information needed for global merge
// decisions (symbol merging, augmentation stitching, export/re-export graphs)
// without retaining the full AST arena, flow graph, or scope tree.
//
// Pipeline: BindResult → extract_skeleton() → FileSkeleton
//           Vec<FileSkeleton> → reduce_skeletons() → SkeletonIndex
//
// The legacy full-arena path (`merge_bind_results`) remains unchanged.
// Skeletons run alongside it to prove the data can be captured without full
// arena retention, as a stepping stone toward Phase 2 of the large-repo plan.

/// A top-level symbol as seen from the skeleton layer.
///
/// This contains only the merge-relevant fields from `Symbol`, not the full
/// declaration list or member/export sub-tables (which require arena access).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkeletonSymbol {
    /// The escaped name (same semantics as `Symbol::escaped_name`).
    pub name: String,
    /// Symbol flags (same encoding as `symbol_flags`).
    pub flags: u32,
    /// Whether this symbol is exported from its file.
    pub is_exported: bool,
    /// Number of declarations in the source file.
    pub declaration_count: u32,
    /// Whether the symbol has an `exports` sub-table (namespace/module).
    pub has_exports: bool,
    /// Whether the symbol has a `members` sub-table (class/interface).
    pub has_members: bool,
    /// Whether this symbol originated from a lib file.
    pub is_lib_origin: bool,
    /// Whether this is an external-module import alias.
    pub is_import_alias: bool,
    /// Import module specifier, if this is an import alias.
    pub import_module: Option<String>,
}

/// Augmentation candidate as seen from the skeleton layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkeletonAugmentation {
    /// Target name (interface name for global augmentations, module specifier for module augmentations).
    pub target: String,
    /// Number of augmentation declarations for this target in this file.
    pub declaration_count: u32,
}

/// Re-export edge as seen from the skeleton layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkeletonReexport {
    /// The exported name (as visible to importers).
    pub exported_name: String,
    /// Source module specifier.
    pub source_module: String,
    /// Original name in the source module (None = same as `exported_name`).
    pub original_name: Option<String>,
}

/// Wildcard re-export edge (`export * from 'module'`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkeletonWildcardReexport {
    /// Source module specifier.
    pub source_module: String,
    /// Whether this is a type-only re-export.
    pub type_only: bool,
}

/// Minimal per-file skeleton extracted from a `BindResult`.
///
/// Contains only the data needed for:
/// - Determining which top-level symbols exist and whether they can merge.
/// - Tracking augmentation candidates (global and module).
/// - Capturing re-export/wildcard-re-export graph edges.
/// - Identifying declared ambient modules and shorthand modules.
///
/// Does NOT contain: full AST arena, flow graph, scope tree, node-to-symbol
/// mappings, parse diagnostics, or per-node data. Those remain in the legacy
/// `BindResult`/`BoundFile` path.
#[derive(Debug, Clone)]
pub struct FileSkeleton {
    /// Source file name.
    pub file_name: String,
    /// Whether this file is an external module (has imports/exports).
    pub is_external_module: bool,
    /// Top-level symbols (root scope + exported `file_locals`).
    pub symbols: Vec<SkeletonSymbol>,
    /// Global augmentation targets from `declare global {}` blocks.
    pub global_augmentations: Vec<SkeletonAugmentation>,
    /// Module augmentation targets from `declare module 'x' {}` blocks.
    pub module_augmentations: Vec<SkeletonAugmentation>,
    /// Named re-exports (`export { x } from 'module'`).
    pub reexports: Vec<SkeletonReexport>,
    /// Wildcard re-exports (`export * from 'module'`).
    pub wildcard_reexports: Vec<SkeletonWildcardReexport>,
    /// Ambient module declarations (`declare module "foo"`).
    pub declared_modules: Vec<String>,
    /// Shorthand ambient modules (`declare module "foo"` without body).
    pub shorthand_ambient_modules: Vec<String>,
    /// Module export specifiers — keys from `module_exports` map.
    /// These represent module specifiers that have explicit export declarations
    /// (e.g., from `declare module "xxx" { export ... }`).
    pub module_export_specifiers: Vec<String>,
    /// Expando property assignments: maps identifier name -> set of property names
    /// assigned via `X.prop = value` patterns. Used to suppress false TS2339 errors.
    pub expando_properties: Vec<(String, Vec<String>)>,
    /// Binder-detected file features (generators, decorators, etc.).
    pub file_features: crate::binder::FileFeatures,
}

/// Extract a `FileSkeleton` from a `BindResult` without consuming it.
///
/// This is a pure map operation: one `BindResult` → one `FileSkeleton`.
/// The skeleton captures merge-relevant data without retaining the arena.
pub fn extract_skeleton(result: &BindResult) -> FileSkeleton {
    // Collect top-level symbols from root scope + file_locals.
    // We use file_locals as the primary source since it represents what the
    // binder considers top-level (including symbols from nested scopes that
    // are hoisted to file-level, like `declare namespace`).
    let mut symbols = Vec::new();
    let mut seen_names = FxHashSet::default();

    for (name, &sym_id) in result.file_locals.iter() {
        if !seen_names.insert(name.clone()) {
            continue;
        }
        if let Some(sym) = result.symbols.get(sym_id) {
            symbols.push(SkeletonSymbol {
                name: name.clone(),
                flags: sym.flags,
                is_exported: sym.is_exported,
                declaration_count: sym.declarations.len() as u32,
                has_exports: sym.exports.is_some(),
                has_members: sym.members.is_some(),
                is_lib_origin: result.lib_symbol_ids.contains(&sym_id),
                is_import_alias: (sym.flags & crate::binder::symbol_flags::ALIAS) != 0,
                import_module: sym.import_module.clone(),
            });
        }
    }

    // Also include root-scope symbols NOT in file_locals (rare, but possible
    // for non-exported declarations in script files).
    if let Some(root_scope) = result.scopes.first() {
        for (name, &sym_id) in root_scope.table.iter() {
            if seen_names.contains(name) {
                continue;
            }
            if let Some(sym) = result.symbols.get(sym_id) {
                seen_names.insert(name.clone());
                symbols.push(SkeletonSymbol {
                    name: name.clone(),
                    flags: sym.flags,
                    is_exported: sym.is_exported,
                    declaration_count: sym.declarations.len() as u32,
                    has_exports: sym.exports.is_some(),
                    has_members: sym.members.is_some(),
                    is_lib_origin: result.lib_symbol_ids.contains(&sym_id),
                    is_import_alias: (sym.flags & crate::binder::symbol_flags::ALIAS) != 0,
                    import_module: sym.import_module.clone(),
                });
            }
        }
    }

    // Sort symbols by name for deterministic output.
    symbols.sort_by(|a, b| a.name.cmp(&b.name));

    // Global augmentations
    let mut global_augmentations: Vec<SkeletonAugmentation> = result
        .global_augmentations
        .iter()
        .map(|(target, augs)| SkeletonAugmentation {
            target: target.clone(),
            declaration_count: augs.len() as u32,
        })
        .collect();
    global_augmentations.sort_by(|a, b| a.target.cmp(&b.target));

    // Module augmentations
    let mut module_augmentations: Vec<SkeletonAugmentation> = result
        .module_augmentations
        .iter()
        .map(|(target, augs)| SkeletonAugmentation {
            target: target.clone(),
            declaration_count: augs.len() as u32,
        })
        .collect();
    module_augmentations.sort_by(|a, b| a.target.cmp(&b.target));

    // Named re-exports
    let mut reexports = Vec::new();
    for (file_name, file_reexports) in &result.reexports {
        // Only include re-exports from this file (the reexport map key is the file name)
        if file_name == &result.file_name {
            for (exported_name, (source_module, original_name)) in file_reexports {
                reexports.push(SkeletonReexport {
                    exported_name: exported_name.clone(),
                    source_module: source_module.clone(),
                    original_name: original_name.clone(),
                });
            }
        }
    }
    reexports.sort_by(|a, b| a.exported_name.cmp(&b.exported_name));

    // Wildcard re-exports
    let mut wildcard_reexports = Vec::new();
    if let Some(sources) = result.wildcard_reexports.get(&result.file_name) {
        let type_only_entries = result.wildcard_reexports_type_only.get(&result.file_name);
        for (i, source_module) in sources.iter().enumerate() {
            let type_only = type_only_entries
                .and_then(|entries| entries.get(i).map(|(_, is_to)| *is_to))
                .unwrap_or(false);
            wildcard_reexports.push(SkeletonWildcardReexport {
                source_module: source_module.clone(),
                type_only,
            });
        }
    }
    wildcard_reexports.sort_by(|a, b| a.source_module.cmp(&b.source_module));

    // Declared modules
    let mut declared_modules: Vec<String> = result.declared_modules.iter().cloned().collect();
    declared_modules.sort();

    // Shorthand ambient modules
    let mut shorthand_ambient_modules: Vec<String> =
        result.shorthand_ambient_modules.iter().cloned().collect();
    shorthand_ambient_modules.sort();

    // Module export specifiers (keys from module_exports map)
    let mut module_export_specifiers: Vec<String> = result.module_exports.keys().cloned().collect();
    module_export_specifiers.sort();

    // Expando properties: convert FxHashMap<String, FxHashSet<String>> to sorted Vec
    let mut expando_properties: Vec<(String, Vec<String>)> = result
        .expando_properties
        .iter()
        .map(|(obj_key, props)| {
            let mut sorted_props: Vec<String> = props.iter().cloned().collect();
            sorted_props.sort();
            (obj_key.clone(), sorted_props)
        })
        .collect();
    expando_properties.sort_by(|a, b| a.0.cmp(&b.0));

    FileSkeleton {
        file_name: result.file_name.clone(),
        is_external_module: result.is_external_module,
        symbols,
        global_augmentations,
        module_augmentations,
        reexports,
        wildcard_reexports,
        declared_modules,
        shorthand_ambient_modules,
        module_export_specifiers,
        expando_properties,
        file_features: result.file_features,
    }
}

/// A merge candidate discovered during skeleton reduction.
///
/// Records that a symbol name appears in multiple files and can potentially
/// be merged (interfaces, namespaces, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkeletonMergeCandidate {
    /// The symbol name.
    pub name: String,
    /// Combined flags from all contributing files.
    pub merged_flags: u32,
    /// Files contributing to this symbol (indices into the original skeleton slice).
    pub source_files: Vec<usize>,
    /// Whether the merge is valid according to `can_merge_symbols_cross_file`.
    pub is_valid_merge: bool,
}

/// Global index produced by reducing a set of file skeletons.
///
/// This is a lightweight alternative to `MergedProgram` that captures the
/// merge topology without retaining any arena or symbol data.
#[derive(Debug, Clone)]
pub struct SkeletonIndex {
    /// Number of files in the index.
    pub file_count: usize,
    /// Symbols that appear in multiple files and can merge.
    pub merge_candidates: Vec<SkeletonMergeCandidate>,
    /// All global augmentation targets across all files, with contributing file indices.
    pub global_augmentation_targets: FxHashMap<String, Vec<usize>>,
    /// All module augmentation targets across all files, with contributing file indices.
    pub module_augmentation_targets: FxHashMap<String, Vec<usize>>,
    /// All declared ambient modules across all files.
    pub declared_modules: FxHashSet<String>,
    /// All shorthand ambient modules across all files.
    pub shorthand_ambient_modules: FxHashSet<String>,
    /// All module export specifiers across all files (keys from `module_exports`).
    pub module_export_specifiers: FxHashSet<String>,
    /// Merged expando property assignments across all files.
    /// Maps identifier name -> set of property names assigned via `X.prop = value`.
    pub expando_properties: FxHashMap<String, FxHashSet<String>>,
    /// Total number of top-level symbols across all files (before merge).
    pub total_symbol_count: usize,
    /// Total number of re-export edges across all files.
    pub total_reexport_count: usize,
    /// Total number of wildcard re-export edges across all files.
    pub total_wildcard_reexport_count: usize,
}

/// Deterministically reduce a set of file skeletons into a `SkeletonIndex`.
///
/// This is a pure function: the same input skeletons (in the same order) always
/// produce the same output. The reduction is sequential and ordered.
///
/// # Arguments
/// * `skeletons` - Slice of file skeletons, in file order.
pub fn reduce_skeletons(skeletons: &[FileSkeleton]) -> SkeletonIndex {
    let mut symbol_map: FxHashMap<String, (u32, Vec<usize>)> = FxHashMap::default();
    let mut global_augmentation_targets: FxHashMap<String, Vec<usize>> = FxHashMap::default();
    let mut module_augmentation_targets: FxHashMap<String, Vec<usize>> = FxHashMap::default();
    let mut declared_modules = FxHashSet::default();
    let mut shorthand_ambient_modules = FxHashSet::default();
    let mut module_export_specifiers = FxHashSet::default();
    let mut expando_properties: FxHashMap<String, FxHashSet<String>> = FxHashMap::default();
    let mut total_symbol_count = 0usize;
    let mut total_reexport_count = 0usize;
    let mut total_wildcard_reexport_count = 0usize;

    for (file_idx, skeleton) in skeletons.iter().enumerate() {
        // Only merge symbols from non-external-module files (script files).
        // External modules' symbols are file-scoped and don't contribute to globals.
        if !skeleton.is_external_module {
            for sym in &skeleton.symbols {
                if sym.is_lib_origin || sym.is_import_alias {
                    continue;
                }
                total_symbol_count += 1;
                let entry = symbol_map
                    .entry(sym.name.clone())
                    .or_insert_with(|| (0, Vec::new()));
                entry.0 |= sym.flags;
                entry.1.push(file_idx);
            }
        } else {
            total_symbol_count += skeleton.symbols.len();
        }

        for aug in &skeleton.global_augmentations {
            global_augmentation_targets
                .entry(aug.target.clone())
                .or_default()
                .push(file_idx);
        }

        for aug in &skeleton.module_augmentations {
            module_augmentation_targets
                .entry(aug.target.clone())
                .or_default()
                .push(file_idx);
        }

        declared_modules.extend(skeleton.declared_modules.iter().cloned());
        shorthand_ambient_modules.extend(skeleton.shorthand_ambient_modules.iter().cloned());
        module_export_specifiers.extend(skeleton.module_export_specifiers.iter().cloned());

        for (obj_key, props) in &skeleton.expando_properties {
            expando_properties
                .entry(obj_key.clone())
                .or_default()
                .extend(props.iter().cloned());
        }

        total_reexport_count += skeleton.reexports.len();
        total_wildcard_reexport_count += skeleton.wildcard_reexports.len();
    }

    // Build merge candidates: symbols appearing in >1 file.
    let mut merge_candidates: Vec<SkeletonMergeCandidate> = symbol_map
        .into_iter()
        .filter(|(_, (_, files))| files.len() > 1)
        .map(|(name, (merged_flags, source_files))| {
            // Determine if the merge is valid by checking all pairs.
            // A simple approximation: check if the first file's flags can merge
            // with the combined flags of all others.
            let is_valid_merge = {
                let first_flags = skeletons[source_files[0]]
                    .symbols
                    .iter()
                    .find(|s| s.name == name)
                    .map(|s| s.flags)
                    .unwrap_or(0);
                let rest_flags = merged_flags & !first_flags | first_flags;
                // Check pairwise: for simplicity, check first vs rest_combined.
                // This is an approximation; the full merge uses pairwise checks.
                can_merge_symbols_cross_file(first_flags, merged_flags & !first_flags)
                    || can_merge_symbols_cross_file(first_flags, rest_flags)
            };
            SkeletonMergeCandidate {
                name,
                merged_flags,
                source_files,
                is_valid_merge,
            }
        })
        .collect();

    // Sort for deterministic output.
    merge_candidates.sort_by(|a, b| a.name.cmp(&b.name));

    SkeletonIndex {
        file_count: skeletons.len(),
        merge_candidates,
        global_augmentation_targets,
        module_augmentation_targets,
        declared_modules,
        shorthand_ambient_modules,
        module_export_specifiers,
        expando_properties,
        total_symbol_count,
        total_reexport_count,
        total_wildcard_reexport_count,
    }
}

impl SkeletonIndex {
    /// Build the set of all known declared/ambient module names from the skeleton data.
    ///
    /// This produces the same result as the `set_all_binders` loop in the checker
    /// that scans `module_exports` keys, `declared_modules`, and
    /// `shorthand_ambient_modules` — but reads from pre-reduced skeleton data
    /// instead of scanning full binders.
    ///
    /// Returns `(exact_names, wildcard_patterns)` where exact names are normalized
    /// (quotes stripped) module names and wildcard patterns contain `*`.
    #[must_use]
    pub fn build_declared_module_sets(&self) -> (FxHashSet<String>, Vec<String>) {
        let mut exact = FxHashSet::default();
        let mut patterns = Vec::new();

        // Collect from all three sources, normalizing the same way set_all_binders does.
        let all_sources = self
            .declared_modules
            .iter()
            .chain(self.shorthand_ambient_modules.iter())
            .chain(self.module_export_specifiers.iter());

        for name in all_sources {
            let normalized = name.trim_matches('"').trim_matches('\'');
            if normalized.contains('*') {
                patterns.push(normalized.to_string());
            } else {
                exact.insert(normalized.to_string());
            }
        }

        // Deduplicate and sort patterns for determinism.
        patterns.sort();
        patterns.dedup();

        (exact, patterns)
    }

    /// Validate that skeleton-derived data matches the legacy `MergedProgram` state.
    ///
    /// In debug builds, asserts that:
    /// - `declared_modules` match exactly
    /// - `shorthand_ambient_modules` match exactly
    /// - `module_export_specifiers` match the keys of `module_exports`
    ///   (excluding user file names that the legacy path inserts as module_exports keys)
    ///
    /// This proves the skeleton captures all merge-relevant ambient module topology
    /// without retaining arenas. In release builds, this is a no-op.
    pub fn validate_against_merged(
        &self,
        merged_declared_modules: &FxHashSet<String>,
        merged_shorthand_ambient_modules: &FxHashSet<String>,
        merged_module_export_keys: &FxHashSet<String>,
        user_file_names: &FxHashSet<String>,
    ) {
        if cfg!(debug_assertions) {
            // 1) declared_modules must match exactly.
            assert_eq!(
                &self.declared_modules, merged_declared_modules,
                "skeleton declared_modules differs from legacy merge"
            );

            // 2) shorthand_ambient_modules must match exactly.
            assert_eq!(
                &self.shorthand_ambient_modules, merged_shorthand_ambient_modules,
                "skeleton shorthand_ambient_modules differs from legacy merge"
            );

            // 3) module_export_specifiers: both skeleton and legacy include
            //    binder-produced module_exports keys. The legacy merge also
            //    inserts user file names (from the per-file export collection).
            //    The skeleton captures the binder-level keys which may also
            //    include the file's own name for external modules. Filter user
            //    file names from both sides before comparing.
            let legacy_non_file_keys: FxHashSet<String> = merged_module_export_keys
                .iter()
                .filter(|k| !user_file_names.contains(k.as_str()))
                .cloned()
                .collect();
            let skeleton_non_file_keys: FxHashSet<String> = self
                .module_export_specifiers
                .iter()
                .filter(|k| !user_file_names.contains(k.as_str()))
                .cloned()
                .collect();
            assert_eq!(
                &skeleton_non_file_keys, &legacy_non_file_keys,
                "skeleton module_export_specifiers differs from legacy merge (after filtering user file names)"
            );
        }
    }
}

/// Estimate the in-memory size of a `FileSkeleton` in bytes.
///
/// This is a rough estimate for comparison with full `BindResult` size.
/// It counts string allocations and vec capacities.
impl FileSkeleton {
    #[must_use]
    pub fn estimated_size_bytes(&self) -> usize {
        let mut size = std::mem::size_of::<Self>();
        size += self.file_name.capacity();
        for sym in &self.symbols {
            size += std::mem::size_of::<SkeletonSymbol>();
            size += sym.name.capacity();
            if let Some(ref m) = sym.import_module {
                size += m.capacity();
            }
        }
        for aug in &self.global_augmentations {
            size += std::mem::size_of::<SkeletonAugmentation>();
            size += aug.target.capacity();
        }
        for aug in &self.module_augmentations {
            size += std::mem::size_of::<SkeletonAugmentation>();
            size += aug.target.capacity();
        }
        for re in &self.reexports {
            size += std::mem::size_of::<SkeletonReexport>();
            size += re.exported_name.capacity();
            size += re.source_module.capacity();
            if let Some(ref o) = re.original_name {
                size += o.capacity();
            }
        }
        for wre in &self.wildcard_reexports {
            size += std::mem::size_of::<SkeletonWildcardReexport>();
            size += wre.source_module.capacity();
        }
        for dm in &self.declared_modules {
            size += dm.capacity();
        }
        for sm in &self.shorthand_ambient_modules {
            size += sm.capacity();
        }
        for ms in &self.module_export_specifiers {
            size += ms.capacity();
        }
        for (obj_key, props) in &self.expando_properties {
            size += obj_key.capacity();
            for prop in props {
                size += prop.capacity();
            }
        }
        size
    }
}

// =============================================================================
// Symbol Merging
// =============================================================================

/// A bound file ready for type checking
pub struct BoundFile {
    /// File name
    pub file_name: String,
    /// The parsed source file node index
    pub source_file: NodeIndex,
    /// The arena containing all nodes (owned by this file)
    pub arena: Arc<NodeArena>,
    /// Node-to-symbol mapping (symbol IDs are global after merge)
    pub node_symbols: FxHashMap<u32, SymbolId>,
    /// Export visibility of namespace/module declaration nodes after binder rules.
    pub module_declaration_exports_publicly: FxHashMap<u32, bool>,
    /// Persistent scopes (symbol IDs are global after merge)
    pub scopes: Vec<Scope>,
    /// Map from AST node to scope ID
    pub node_scope_ids: FxHashMap<u32, ScopeId>,
    /// Parse diagnostics
    pub parse_diagnostics: Vec<ParseDiagnostic>,
    /// Global augmentations (interface declarations inside `declare global` blocks)
    pub global_augmentations: FxHashMap<String, Vec<crate::binder::GlobalAugmentation>>,
    /// Module augmentations (interface/type declarations inside `declare module 'x'` blocks)
    pub module_augmentations: FxHashMap<String, Vec<crate::binder::ModuleAugmentation>>,
    /// Maps symbols declared inside module augmentation blocks to their target module specifier
    pub augmentation_target_modules: FxHashMap<SymbolId, String>,
    /// Flow nodes for control flow analysis
    pub flow_nodes: FlowNodeArena,
    /// Node-to-flow mapping: tracks which flow node was active at each AST node
    pub node_flow: FxHashMap<u32, FlowNodeId>,
    /// Map from switch clause `NodeIndex` to parent switch statement `NodeIndex`
    /// Used by control flow analysis for switch exhaustiveness checking
    pub switch_clause_to_switch: FxHashMap<u32, NodeIndex>,
    /// Whether this file is an external module (has imports/exports)
    pub is_external_module: bool,
    /// Expando property assignments detected during binding
    pub expando_properties: FxHashMap<String, FxHashSet<String>>,
    pub file_features: crate::binder::FileFeatures,
}

use tsz_solver::TypeInterner;

/// Merged program state after parallel binding
pub struct MergedProgram {
    /// All bound files
    pub files: Vec<BoundFile>,
    /// Global symbol arena (all symbols from all files, with remapped IDs)
    pub symbols: SymbolArena,
    /// Symbol-to-arena mapping for declaration lookup (legacy, stores last arena)
    pub symbol_arenas: FxHashMap<SymbolId, Arc<NodeArena>>,
    /// Declaration-to-arena mapping for precise cross-file declaration lookup
    /// Key: (`SymbolId`, `NodeIndex` of declaration) -> Arena(s) containing that declaration
    pub declaration_arenas: DeclarationArenaMap,
    /// Cross-file `node_symbols`: maps arena pointer → `node_symbols` for that arena.
    /// Enables resolving type references in cross-file interface declarations.
    pub cross_file_node_symbols: CrossFileNodeSymbols,
    /// Global symbol table (exports from all files)
    pub globals: SymbolTable,
    /// Per-file symbol tables (file-local symbols, symbol IDs remapped)
    pub file_locals: Vec<SymbolTable>,
    /// Ambient module declarations across all files
    pub declared_modules: FxHashSet<String>,
    /// Shorthand ambient modules (`declare module "foo"` without body) - imports from these are `any`
    pub shorthand_ambient_modules: FxHashSet<String>,
    /// Module exports: maps file name (or module specifier) to its exported symbols
    /// This enables cross-file module resolution: import { X } from './file' can find X's symbol
    pub module_exports: FxHashMap<String, SymbolTable>,
    /// Re-exports: tracks `export { x } from 'module'` declarations
    /// Maps (`current_file`, `exported_name`) -> (`source_module`, `original_name`)
    pub reexports: Reexports,
    /// Wildcard re-exports: tracks `export * from 'module'` declarations
    /// Maps `current_file` -> Vec of `source_modules`
    pub wildcard_reexports: FxHashMap<String, Vec<String>>,
    /// Wildcard re-export type-only provenance per entry.
    pub wildcard_reexports_type_only: FxHashMap<String, Vec<(String, bool)>>,
    /// Lib binders for global type resolution (Array, String, Promise, etc.)
    /// These contain symbols from lib.d.ts files and enable resolution of built-in types
    pub lib_binders: Vec<Arc<BinderState>>,
    /// Global symbol IDs that originated from lib files (remapped to global arena IDs)
    pub lib_symbol_ids: FxHashSet<SymbolId>,
    /// Global type interner - shared across all threads for type deduplication
    pub type_interner: TypeInterner,
    /// Alias partners: maps `TYPE_ALIAS` `SymbolId` → `ALIAS` `SymbolId` for merged type+namespace exports.
    /// When `export type X = ...` and `export * as X from "..."` coexist, the exports table
    /// holds the `TYPE_ALIAS` symbol and this map links it to the ALIAS symbol for value resolution.
    pub alias_partners: FxHashMap<SymbolId, SymbolId>,
    /// Binder-captured semantic definitions for top-level declarations (Phase 1 DefId-first).
    /// Maps post-remap `SymbolId` → `SemanticDefEntry` across all files.
    /// The checker reads this during construction to pre-create solver `DefIds`.
    pub semantic_defs: FxHashMap<SymbolId, crate::binder::SemanticDefEntry>,
    /// Skeleton index computed alongside the legacy merge path.
    ///
    /// This captures the same merge-relevant topology (symbol merging, augmentation
    /// targets, re-export graph) without retaining any arena or binder state.
    /// It is computed from pre-merge `BindResult`s during `merge_bind_results_ref`
    /// and stored here so downstream consumers can begin migrating off arena-backed
    /// lookups toward skeleton-based queries.
    pub skeleton_index: Option<SkeletonIndex>,
}

/// High-level residency counters for `MergedProgram` state.
///
/// These numbers give a stable baseline for large-repo performance work without
/// pretending to be exact heap accounting. The important question is how many
/// arenas and declaration mappings the current pipeline retains after merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MergedProgramResidencyStats {
    /// Number of user files in the merged program.
    pub file_count: usize,
    /// Number of bound-file arena handles retained directly by `program.files`.
    pub bound_file_arena_count: usize,
    /// Number of unique `NodeArena` allocations retained across all arena maps.
    pub unique_arena_count: usize,
    /// Number of entries in the symbol -> arena lookup table.
    pub symbol_arena_count: usize,
    /// Number of declaration -> arena buckets retained for cross-file lookup.
    pub declaration_arena_bucket_count: usize,
    /// Total number of declaration -> arena edges across all buckets.
    pub declaration_arena_mapping_count: usize,
    /// Whether the skeleton index was computed alongside the legacy merge.
    pub has_skeleton_index: bool,
    /// Number of merge candidates identified by the skeleton (symbols in >1 file).
    pub skeleton_merge_candidate_count: usize,
    /// Total top-level symbols tracked by the skeleton (before merge).
    pub skeleton_total_symbol_count: usize,
}

impl MergedProgram {
    /// Return residency-oriented counters for the current merged program.
    #[must_use]
    pub fn residency_stats(&self) -> MergedProgramResidencyStats {
        let mut unique_arenas: FxHashSet<usize> = FxHashSet::default();

        for file in &self.files {
            unique_arenas.insert(Arc::as_ptr(&file.arena) as usize);
        }
        for arena in self.symbol_arenas.values() {
            unique_arenas.insert(Arc::as_ptr(arena) as usize);
        }
        for arenas in self.declaration_arenas.values() {
            for arena in arenas {
                unique_arenas.insert(Arc::as_ptr(arena) as usize);
            }
        }

        let (has_skeleton, skel_merge_count, skel_sym_count) =
            if let Some(ref idx) = self.skeleton_index {
                (true, idx.merge_candidates.len(), idx.total_symbol_count)
            } else {
                (false, 0, 0)
            };

        MergedProgramResidencyStats {
            file_count: self.files.len(),
            bound_file_arena_count: self.files.len(),
            unique_arena_count: unique_arenas.len(),
            symbol_arena_count: self.symbol_arenas.len(),
            declaration_arena_bucket_count: self.declaration_arenas.len(),
            declaration_arena_mapping_count: self
                .declaration_arenas
                .values()
                .map(|arenas| arenas.len())
                .sum(),
            has_skeleton_index: has_skeleton,
            skeleton_merge_candidate_count: skel_merge_count,
            skeleton_total_symbol_count: skel_sym_count,
        }
    }
}

/// Check if two symbols can be merged across multiple files.
///
/// TypeScript allows merging:
/// - Interface + Interface (declaration merging)
/// - Namespace + Namespace (declaration merging)
/// - Class + Interface (merging for class declarations)
/// - Function + Function (overloads - handled per-file)
const fn can_merge_symbols_cross_file(existing_flags: u32, new_flags: u32) -> bool {
    use crate::binder::symbol_flags;

    // Interface can merge with interface
    if (existing_flags & symbol_flags::INTERFACE) != 0 && (new_flags & symbol_flags::INTERFACE) != 0
    {
        return true;
    }

    // Class can merge with interface
    if ((existing_flags & symbol_flags::CLASS) != 0 && (new_flags & symbol_flags::INTERFACE) != 0)
        || ((existing_flags & symbol_flags::INTERFACE) != 0
            && (new_flags & symbol_flags::CLASS) != 0)
    {
        return true;
    }

    // Interface can merge with variable (e.g., `interface Promise<T>` + `declare var Promise: PromiseConstructor`)
    // This is fundamental to how TypeScript lib declarations work: types have both an interface
    // (type side) and a variable declaration (value side).
    if ((existing_flags & symbol_flags::INTERFACE) != 0
        && (new_flags & symbol_flags::VARIABLE) != 0)
        || ((existing_flags & symbol_flags::VARIABLE) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0)
    {
        return true;
    }

    // Interface can merge with function (e.g., `interface Array<T>` + `declare function Array(...)`)
    if ((existing_flags & symbol_flags::INTERFACE) != 0
        && (new_flags & symbol_flags::FUNCTION) != 0)
        || ((existing_flags & symbol_flags::FUNCTION) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0)
    {
        return true;
    }

    // Namespace/module can merge with namespace/module
    if (existing_flags & symbol_flags::MODULE) != 0 && (new_flags & symbol_flags::MODULE) != 0 {
        return true;
    }

    // Variable can merge with variable cross-file (so we can detect and report cross-file redeclarations of let/const)
    if (existing_flags & symbol_flags::VARIABLE) != 0 && (new_flags & symbol_flags::VARIABLE) != 0 {
        return true;
    }

    // Class can merge with Class cross-file (invalid, but merged to report duplicate)
    if (existing_flags & symbol_flags::CLASS) != 0 && (new_flags & symbol_flags::CLASS) != 0 {
        return true;
    }

    // Class can merge with Type Alias (invalid, but merged to report duplicate)
    if ((existing_flags & symbol_flags::CLASS) != 0 && (new_flags & symbol_flags::TYPE_ALIAS) != 0)
        || ((existing_flags & symbol_flags::TYPE_ALIAS) != 0
            && (new_flags & symbol_flags::CLASS) != 0)
    {
        return true;
    }

    // Type Alias can merge with Type Alias (invalid, but merged to report duplicate)
    if (existing_flags & symbol_flags::TYPE_ALIAS) != 0
        && (new_flags & symbol_flags::TYPE_ALIAS) != 0
    {
        return true;
    }

    // Type Alias can merge with Interface (invalid, but merged to report duplicate)
    if ((existing_flags & symbol_flags::TYPE_ALIAS) != 0
        && (new_flags & symbol_flags::INTERFACE) != 0)
        || ((existing_flags & symbol_flags::INTERFACE) != 0
            && (new_flags & symbol_flags::TYPE_ALIAS) != 0)
    {
        return true;
    }

    // Class can merge with Variable (invalid, but merged to report duplicate)
    if ((existing_flags & symbol_flags::CLASS) != 0 && (new_flags & symbol_flags::VARIABLE) != 0)
        || ((existing_flags & symbol_flags::VARIABLE) != 0
            && (new_flags & symbol_flags::CLASS) != 0)
    {
        return true;
    }

    // Type Alias can merge with Variable (invalid, but merged to report duplicate)
    if ((existing_flags & symbol_flags::TYPE_ALIAS) != 0
        && (new_flags & symbol_flags::VARIABLE) != 0)
        || ((existing_flags & symbol_flags::VARIABLE) != 0
            && (new_flags & symbol_flags::TYPE_ALIAS) != 0)
    {
        return true;
    }

    // Namespace can merge with class, function, enum, or variable
    if (existing_flags & symbol_flags::MODULE) != 0
        && (new_flags
            & (symbol_flags::CLASS
                | symbol_flags::FUNCTION
                | symbol_flags::ENUM
                | symbol_flags::VARIABLE))
            != 0
    {
        return true;
    }
    if (new_flags & symbol_flags::MODULE) != 0
        && (existing_flags
            & (symbol_flags::CLASS
                | symbol_flags::FUNCTION
                | symbol_flags::ENUM
                | symbol_flags::VARIABLE))
            != 0
    {
        return true;
    }

    // Enum can merge with enum
    if (existing_flags & symbol_flags::ENUM) != 0 && (new_flags & symbol_flags::ENUM) != 0 {
        return true;
    }

    false
}

/// Append declarations from `incoming` into `existing` without duplicates.
///
/// Small declaration lists are common, so use linear scans there to avoid
/// hash set allocation overhead. Switch to a set only for larger collections.
fn append_unique_declarations(existing: &mut Vec<NodeIndex>, incoming: &[NodeIndex]) {
    existing.extend_from_slice(incoming);
}

/// Remap `__unique_{SymbolId}` keys in `expando_properties` to use global `SymbolIds`.
///
/// During binding, expando property tracking stores unique symbol keys as
/// `__unique_{local_SymbolId}`. After `merge_bind_results` remaps all `SymbolIds`
/// to a global arena, these encoded IDs become stale. This function updates
/// them so the checker's `UniqueSymbol` types (which use global IDs) match.
fn remap_expando_properties(
    expando: &FxHashMap<String, FxHashSet<String>>,
    id_remap: &FxHashMap<SymbolId, SymbolId>,
) -> FxHashMap<String, FxHashSet<String>> {
    expando
        .iter()
        .map(|(obj_name, props)| {
            let remapped_props = props
                .iter()
                .map(|prop| {
                    if let Some(old_id_str) = prop.strip_prefix("__unique_")
                        && let Ok(old_id) = old_id_str.parse::<u32>()
                        && let Some(&new_id) = id_remap.get(&SymbolId(old_id))
                    {
                        return format!("__unique_{}", new_id.0);
                    }
                    prop.clone()
                })
                .collect();
            (obj_name.clone(), remapped_props)
        })
        .collect()
}

/// Merge bind results into a unified program state
///
/// This is a sequential operation that combines:
/// - All symbol arenas into a single global arena
/// - Merges symbols with the same name across files (for interfaces, namespaces, etc.)
/// - Remaps symbol IDs in `node_symbols` to use global IDs
///
/// # Arguments
/// * `results` - Vector of `BindResult` from parallel binding
///
/// # Returns
/// `MergedProgram` with unified symbol space
pub fn merge_bind_results(results: Vec<BindResult>) -> MergedProgram {
    let refs: Vec<&BindResult> = results.iter().collect();
    merge_bind_results_ref(&refs)
}

pub fn merge_bind_results_ref(results: &[&BindResult]) -> MergedProgram {
    // Extract file skeletons from pre-merge bind results and reduce them into a
    // global index. This runs before the legacy merge so we capture the original
    // per-file symbol/augmentation/re-export data without any remapping.
    let skeletons: Vec<FileSkeleton> = results.iter().map(|r| extract_skeleton(r)).collect();
    let skeleton_index = reduce_skeletons(&skeletons);

    // Collect lib_binders from all results (deduplicated by address), paired with their arenas
    let mut lib_binders: Vec<Arc<BinderState>> = Vec::new();
    let mut lib_binder_set: FxHashSet<usize> = FxHashSet::default();
    let mut lib_binder_arena_map: FxHashMap<usize, Arc<NodeArena>> = FxHashMap::default();
    for result in results {
        for (lib_binder, lib_arena) in result.lib_binders.iter().zip(result.lib_arenas.iter()) {
            let binder_addr = Arc::as_ptr(lib_binder) as usize;
            if lib_binder_set.insert(binder_addr) {
                lib_binders.push(Arc::clone(lib_binder));
                lib_binder_arena_map.insert(binder_addr, Arc::clone(lib_arena));
            }
        }
    }

    // Calculate total symbols needed (including lib symbols)
    let lib_symbol_count: usize = lib_binders.iter().map(|b| b.symbols.len()).sum();
    let user_symbol_count: usize = results.iter().map(|r| r.symbols.len()).sum();
    let total_symbols = lib_symbol_count + user_symbol_count;

    // Create global symbol arena with pre-allocated capacity
    let mut global_symbols = SymbolArena::with_capacity(total_symbols);
    let mut symbol_arenas = FxHashMap::default();
    let mut declaration_arenas: DeclarationArenaMap = FxHashMap::default();
    let mut cross_file_node_symbols: CrossFileNodeSymbols = FxHashMap::default();
    let mut globals = SymbolTable::new();
    let mut files = Vec::with_capacity(results.len());
    let mut file_locals_list = Vec::with_capacity(results.len());
    let mut declared_modules = FxHashSet::default();
    let mut shorthand_ambient_modules = FxHashSet::default();
    let mut module_exports: FxHashMap<String, SymbolTable> = FxHashMap::default();
    let mut alias_partners: FxHashMap<SymbolId, SymbolId> = FxHashMap::default();
    let mut semantic_defs: FxHashMap<SymbolId, crate::binder::SemanticDefEntry> =
        FxHashMap::default();
    let mut reexports: Reexports = FxHashMap::default();
    let mut wildcard_reexports: FxHashMap<String, Vec<String>> = FxHashMap::default();
    let mut wildcard_reexports_type_only: FxHashMap<String, Vec<(String, bool)>> =
        FxHashMap::default();
    let mut global_lib_symbol_ids: FxHashSet<SymbolId> = FxHashSet::default();

    // Track which symbols have been merged to avoid duplicate processing.
    // Use interned atoms to avoid repeated String hashing/cloning on hot merge paths.
    let mut name_interner = Interner::new();
    // Track which symbols have been merged to avoid duplicate processing
    // IMPORTANT: This map is ONLY for symbols in the ROOT scope (ScopeId(0))
    // Symbols from nested scopes should NEVER be merged across files/scopes
    let mut merged_symbols: FxHashMap<Atom, SymbolId> = FxHashMap::default();

    // ==========================================================================
    // PHASE 1: Remap lib symbols to global arena
    // ==========================================================================
    // This creates a mapping from (lib_binder_ptr, local_id) -> global_id
    // so that file_locals can reference lib symbols using global IDs
    let mut lib_symbol_remap: FxHashMap<(usize, SymbolId), SymbolId> = FxHashMap::default();

    for lib_binder in &lib_binders {
        let lib_binder_ptr = Arc::as_ptr(lib_binder) as usize;

        // Pre-build a set of top-level symbol IDs from file_locals for O(1) lookup.
        // This avoids an O(N*F) quadratic scan where each symbol would linearly
        // search file_locals to check if it's top-level.
        let top_level_ids: FxHashSet<SymbolId> =
            lib_binder.file_locals.iter().map(|(_, id)| *id).collect();

        // Process all symbols in this lib binder
        for i in 0..lib_binder.symbols.len() {
            let local_id = SymbolId(i as u32);
            if let Some(lib_sym) = lib_binder.symbols.get(local_id) {
                // Determine if this is a top-level symbol by checking file_locals.
                // In lib files, declarations like `declare namespace Reflect` may appear
                // in a child scope (e.g., ScopeId(1)) even though they're conceptually
                // top-level. Using file_locals is more reliable than the scope check
                // for determining which lib symbols should be globally merged.
                let is_top_level = top_level_ids.contains(&local_id);

                // Check if a symbol with this name already exists (cross-lib merging)
                // IMPORTANT: Only merge top-level symbols (those in file_locals)
                // Nested symbols (namespace members, etc.) should NEVER be merged across scopes
                let global_id = if is_top_level {
                    let name_atom = name_interner.intern(&lib_sym.escaped_name);
                    if let Some(&existing_id) = merged_symbols.get(&name_atom) {
                        // Symbol already exists - check if we can merge
                        if let Some(existing_sym) = global_symbols.get(existing_id) {
                            if can_merge_symbols_cross_file(existing_sym.flags, lib_sym.flags) {
                                // Merge: reuse existing symbol ID
                                // Merge declarations from this lib
                                if let Some(existing_mut) = global_symbols.get_mut(existing_id) {
                                    existing_mut.flags |= lib_sym.flags;
                                    append_unique_declarations(
                                        &mut existing_mut.declarations,
                                        &lib_sym.declarations,
                                    );
                                }
                                existing_id
                            } else {
                                // Cannot merge - allocate new (shadowing)
                                let new_id = global_symbols.alloc_from(lib_sym);
                                merged_symbols.insert(name_atom, new_id);
                                new_id
                            }
                        } else {
                            // Shouldn't happen - allocate new
                            let new_id = global_symbols.alloc_from(lib_sym);
                            merged_symbols.insert(name_atom, new_id);
                            new_id
                        }
                    } else {
                        // New symbol - allocate in global arena
                        let new_id = global_symbols.alloc_from(lib_sym);
                        merged_symbols.insert(name_atom, new_id);
                        new_id
                    }
                } else {
                    // Nested symbol - always allocate new, never merge

                    // NOTE: Don't add to merged_symbols - nested symbols should never be cross-file merged
                    global_symbols.alloc_from(lib_sym)
                };

                // Store the remapping
                lib_symbol_remap.insert((lib_binder_ptr, local_id), global_id);

                // Set arena mappings for this lib symbol using the lib file's arena.
                // The original lib binder's symbol_arenas/declaration_arenas are empty
                // (only populated during per-file merge which uses a different binder).
                // We use lib_binder_arena_map to get the correct arena for this lib file.
                if let Some(lib_arena) = lib_binder_arena_map.get(&lib_binder_ptr) {
                    symbol_arenas
                        .entry(global_id)
                        .or_insert_with(|| Arc::clone(lib_arena));
                    for &decl in &lib_sym.declarations {
                        declaration_arenas
                            .entry((global_id, decl))
                            .or_default()
                            .push(Arc::clone(lib_arena));
                    }
                }
            }
        }
    }

    // ==========================================================================
    // PHASE 1.25: Clear un-remapped exports/members from global symbols
    // ==========================================================================
    // Phase 1's `alloc_from()` copies symbols including their exports/members
    // tables, but those tables contain lib-LOCAL SymbolIds. In the global arena,
    // those same numeric IDs map to DIFFERENT symbols (e.g., lib-local SymbolId(2)
    // might be DateTimeFormat in es5.d.ts, but SymbolId(2) in the global arena is
    // cancelIdleCallback from dom.d.ts). Phase 1.5 will rebuild exports/members
    // with correctly remapped global IDs, so we must clear the corrupt data first.
    {
        let lib_global_ids: FxHashSet<SymbolId> = lib_symbol_remap.values().copied().collect();
        for &global_id in &lib_global_ids {
            if let Some(sym) = global_symbols.get_mut(global_id) {
                sym.exports = None;
                sym.members = None;
            }
        }
    }

    // ==========================================================================
    // PHASE 1.5: Remap internal references (parent, exports, members)
    // ==========================================================================
    // After all lib symbols have been allocated in the global arena, we need a
    // second pass to fix up internal SymbolId references. The `alloc_from()` call
    // copies the symbol data including members/exports/parent, but those fields
    // still contain LOCAL SymbolIds from the original lib binder. We must remap
    // them to the corresponding global IDs using lib_symbol_remap.
    // (This mirrors Phase 2 in state.rs merge_lib_contexts_into_binder.)
    for lib_binder in &lib_binders {
        let lib_binder_ptr = Arc::as_ptr(lib_binder) as usize;

        for i in 0..lib_binder.symbols.len() {
            let local_id = SymbolId(i as u32);
            let Some(&global_id) = lib_symbol_remap.get(&(lib_binder_ptr, local_id)) else {
                continue;
            };
            let Some(lib_sym) = lib_binder.symbols.get(local_id) else {
                continue;
            };

            // Remap parent
            if !lib_sym.parent.is_none()
                && let Some(&new_parent) = lib_symbol_remap.get(&(lib_binder_ptr, lib_sym.parent))
                && let Some(sym) = global_symbols.get_mut(global_id)
            {
                sym.parent = new_parent;
            }

            // Remap exports: replace local IDs with global IDs.
            // When an export name was already remapped by a previous lib binder,
            // merge the new symbol's flags/declarations into the existing one
            // (e.g., INTERFACE from one lib + VALUE from another, like
            // DateTimeFormat in Intl across es5.d.ts and es2017.intl.d.ts).
            if let Some(exports) = &lib_sym.exports {
                let mut new_exports: Vec<(String, SymbolId)> = Vec::new();
                let mut merge_targets: Vec<(SymbolId, SymbolId)> = Vec::new();

                if let Some(sym) = global_symbols.get(global_id) {
                    let existing_exports = sym.exports.as_ref();
                    for (name, &export_id) in exports.iter() {
                        if let Some(&new_export_id) =
                            lib_symbol_remap.get(&(lib_binder_ptr, export_id))
                        {
                            let prev = existing_exports.and_then(|e| e.get(name));
                            if let Some(prev_export_id) = prev {
                                if prev_export_id != new_export_id {
                                    merge_targets.push((prev_export_id, new_export_id));
                                }
                            } else {
                                new_exports.push((name.clone(), new_export_id));
                            }
                        }
                    }
                }

                for (dst_id, src_id) in merge_targets {
                    let src_data = global_symbols
                        .get(src_id)
                        .map(|s| (s.flags, s.declarations.clone(), s.value_declaration));
                    if let Some((src_flags, src_decls, src_value_decl)) = src_data
                        && let Some(dst) = global_symbols.get_mut(dst_id)
                    {
                        dst.flags |= src_flags;
                        for d in src_decls {
                            if !dst.declarations.contains(&d) {
                                dst.declarations.push(d);
                            }
                        }
                        if dst.value_declaration.is_none() && src_value_decl.is_some() {
                            dst.value_declaration = src_value_decl;
                        }
                    }

                    // Copy declaration_arenas and symbol_arenas entries from src to dst.
                    // When interface symbols inside namespaces are merged (e.g.,
                    // Intl.DateTimeFormat from es5.d.ts + es2017.intl.d.ts), the dst
                    // symbol gets src's declarations appended, but the checker needs
                    // declaration_arenas[(dst_id, decl)] to find the correct arena
                    // for each declaration. Without this, merged declarations are
                    // invisible and the interface type is incomplete.
                    if let Some(src_sym) = global_symbols.get(src_id) {
                        let src_decls = src_sym.declarations.clone();
                        for decl_idx in src_decls {
                            if let Some(arenas) =
                                declaration_arenas.get(&(src_id, decl_idx)).cloned()
                            {
                                declaration_arenas
                                    .entry((dst_id, decl_idx))
                                    .or_default()
                                    .extend(arenas);
                            }
                        }
                    }
                    if let Some(src_arena) = symbol_arenas.get(&src_id).cloned() {
                        symbol_arenas.entry(dst_id).or_insert(src_arena);
                    }
                }

                if !new_exports.is_empty()
                    && let Some(sym) = global_symbols.get_mut(global_id)
                {
                    if sym.exports.is_none() {
                        sym.exports = Some(Box::new(SymbolTable::new()));
                    }
                    if let Some(existing) = sym.exports.as_mut() {
                        for (name, id) in new_exports {
                            existing.set(name, id);
                        }
                    }
                }
            }

            // Remap members: replace local IDs with global IDs.
            // Same merge-instead-of-overwrite logic as exports.
            if let Some(members) = &lib_sym.members {
                let mut new_members: Vec<(String, SymbolId)> = Vec::new();
                let mut merge_targets: Vec<(SymbolId, SymbolId)> = Vec::new();

                if let Some(sym) = global_symbols.get(global_id) {
                    let existing_members = sym.members.as_ref();
                    for (name, &member_id) in members.iter() {
                        if let Some(&new_member_id) =
                            lib_symbol_remap.get(&(lib_binder_ptr, member_id))
                        {
                            let prev = existing_members.and_then(|m| m.get(name));
                            if let Some(prev_member_id) = prev {
                                if prev_member_id != new_member_id {
                                    merge_targets.push((prev_member_id, new_member_id));
                                }
                            } else {
                                new_members.push((name.clone(), new_member_id));
                            }
                        }
                    }
                }

                for (dst_id, src_id) in merge_targets {
                    let src_data = global_symbols
                        .get(src_id)
                        .map(|s| (s.flags, s.declarations.clone(), s.value_declaration));
                    if let Some((src_flags, src_decls, src_value_decl)) = src_data
                        && let Some(dst) = global_symbols.get_mut(dst_id)
                    {
                        dst.flags |= src_flags;
                        for d in src_decls {
                            if !dst.declarations.contains(&d) {
                                dst.declarations.push(d);
                            }
                        }
                        if dst.value_declaration.is_none() && src_value_decl.is_some() {
                            dst.value_declaration = src_value_decl;
                        }
                    }

                    // Copy declaration_arenas and symbol_arenas (same as exports above)
                    if let Some(src_sym) = global_symbols.get(src_id) {
                        let src_decls = src_sym.declarations.clone();
                        for decl_idx in src_decls {
                            if let Some(arenas) =
                                declaration_arenas.get(&(src_id, decl_idx)).cloned()
                            {
                                declaration_arenas
                                    .entry((dst_id, decl_idx))
                                    .or_default()
                                    .extend(arenas);
                            }
                        }
                    }
                    if let Some(src_arena) = symbol_arenas.get(&src_id).cloned() {
                        symbol_arenas.entry(dst_id).or_insert(src_arena);
                    }
                }

                if !new_members.is_empty()
                    && let Some(sym) = global_symbols.get_mut(global_id)
                {
                    if sym.members.is_none() {
                        sym.members = Some(Box::new(SymbolTable::new()));
                    }
                    if let Some(existing) = sym.members.as_mut() {
                        for (name, id) in new_members {
                            existing.set(name, id);
                        }
                    }
                }
            }
        }
    }

    // Also remap lib file_locals entries that reference symbols by name
    // (for exported lib symbols like Array, Object, console)
    let mut lib_name_to_global: FxHashMap<Atom, SymbolId> = FxHashMap::default();
    for lib_binder in &lib_binders {
        let lib_binder_ptr = Arc::as_ptr(lib_binder) as usize;
        for (name, &local_id) in lib_binder.file_locals.iter() {
            if let Some(&global_id) = lib_symbol_remap.get(&(lib_binder_ptr, local_id)) {
                // Only keep the first mapping for each name (lib files are processed in order)
                let name_atom = name_interner.intern(name);
                lib_name_to_global.entry(name_atom).or_insert(global_id);
            }
        }
    }

    // ==========================================================================
    // PHASE 2: Process user files
    // ==========================================================================

    for (file_idx, result) in results.iter().enumerate() {
        declared_modules.extend(result.declared_modules.iter().cloned());
        shorthand_ambient_modules.extend(result.shorthand_ambient_modules.iter().cloned());

        // Merge reexports from this file
        for (file_name, file_reexports) in &result.reexports {
            let entry = reexports.entry(file_name.clone()).or_default();
            for (export_name, mapping) in file_reexports {
                entry.insert(export_name.clone(), mapping.clone());
            }
        }

        // Merge wildcard reexports from this file
        for (file_name, source_modules) in &result.wildcard_reexports {
            let entry = wildcard_reexports.entry(file_name.clone()).or_default();
            let type_only_entry = wildcard_reexports_type_only
                .entry(file_name.clone())
                .or_default();
            let source_type_only = result.wildcard_reexports_type_only.get(file_name);

            if entry.len() + source_modules.len() <= 16 {
                for (i, source_module) in source_modules.iter().enumerate() {
                    // Use index-based access to get the correct type-only flag
                    let source_is_type_only = source_type_only
                        .and_then(|entries| entries.get(i).map(|(_, is_to)| *is_to))
                        .unwrap_or(false);

                    if let Some(pos) = entry.iter().position(|m| m == source_module) {
                        // Already have this source — if this path is non-type-only,
                        // override the existing flag (value re-export takes priority).
                        if !source_is_type_only {
                            type_only_entry[pos].1 = false;
                        }
                    } else {
                        entry.push(source_module.clone());
                        type_only_entry.push((source_module.clone(), source_is_type_only));
                    }
                }
            } else {
                let mut seen: FxHashMap<String, usize> = entry.iter().cloned().zip(0..).collect();
                for (i, source_module) in source_modules.iter().enumerate() {
                    let source_is_type_only = source_type_only
                        .and_then(|entries| entries.get(i).map(|(_, is_to)| *is_to))
                        .unwrap_or(false);

                    if let Some(&pos) = seen.get(source_module) {
                        if !source_is_type_only {
                            type_only_entry[pos].1 = false;
                        }
                    } else {
                        let pos = entry.len();
                        seen.insert(source_module.clone(), pos);
                        entry.push(source_module.clone());
                        type_only_entry.push((source_module.clone(), source_is_type_only));
                    }
                }
            }
        }
        // Copy symbols from this file to global arena, getting new IDs
        let mut id_remap: FxHashMap<SymbolId, SymbolId> = FxHashMap::default();
        for i in 0..result.symbols.len() {
            let old_id = SymbolId(i as u32);
            if let Some(sym) = result.symbols.get(old_id) {
                // For lib-originated symbols, reuse the Phase 1 global IDs rather than
                // allocating new ones. This prevents duplicate lib symbols and ensures
                // the Phase 1.5 remapped exports/members are preserved.
                if result.lib_symbol_ids.contains(&old_id) {
                    // For lib-originated symbols, use the reverse remap to find the
                    // original (lib_binder_ptr, local_id), then look up the Phase 1
                    // global ID via lib_symbol_remap. This ensures all lib symbols
                    // (both top-level and nested) map to their Phase 1 global IDs,
                    // preserving the Phase 1.5 export/member remapping.
                    let mut resolved_global_id = None;
                    if let Some(&(binder_ptr, original_local_id)) =
                        result.lib_symbol_reverse_remap.get(&old_id)
                        && let Some(&global_id) =
                            lib_symbol_remap.get(&(binder_ptr, original_local_id))
                    {
                        resolved_global_id = Some(global_id);
                    }
                    // Fallback: look up by name in merged_symbols or lib_name_to_global
                    if resolved_global_id.is_none() {
                        let name_atom = name_interner.intern(&sym.escaped_name);
                        if let Some(&global_id) = merged_symbols.get(&name_atom) {
                            resolved_global_id = Some(global_id);
                        }
                        if resolved_global_id.is_none()
                            && let Some(&global_id) = lib_name_to_global.get(&name_atom)
                        {
                            resolved_global_id = Some(global_id);
                        }
                    }
                    if let Some(global_id) = resolved_global_id {
                        // The user binder may have merged additional flags and declarations
                        // into this lib symbol (e.g., user `interface Event<T>` augments
                        // lib's non-generic `Event`, or user `type Proxy<T>` adds TYPE_ALIAS
                        // to lib's `declare var Proxy`). Always propagate extra flags and
                        // user-local declarations to the global symbol so that type parameter
                        // resolution can find them.
                        if let Some(global_sym) = global_symbols.get_mut(global_id) {
                            let extra_flags = sym.flags & !global_sym.flags;
                            if extra_flags != 0 {
                                global_sym.flags |= extra_flags;
                            }
                            // Always copy user declarations that were merged into this symbol,
                            // even when flags are identical. Without this, user declarations
                            // (e.g., a generic `interface Event<T>`) are lost and
                            // get_type_params_for_symbol won't find their type parameters.
                            append_unique_declarations(
                                &mut global_sym.declarations,
                                &sym.declarations,
                            );
                        }
                        id_remap.insert(old_id, global_id);
                        continue;
                    }
                    // Last resort: allocate a new ID (shouldn't happen normally)
                    let new_id = global_symbols.alloc(sym.flags, sym.escaped_name.clone());
                    symbol_arenas.insert(new_id, Arc::clone(&result.arena));
                    id_remap.insert(old_id, new_id);
                    continue;
                }

                // Check if this symbol is from a nested scope.
                // We check whether this symbol ID appears in the ROOT scope table
                // (ScopeId(0) = SourceFile scope). This is more reliable than checking
                // node_scope_ids because not all declaration types create scopes
                // (e.g., InterfaceDeclaration does not create a scope, so its node
                // won't appear in node_scope_ids, causing false negatives).
                let is_nested_symbol = !result.scopes.first().is_some_and(|root_scope| {
                    root_scope
                        .table
                        .get(&sym.escaped_name)
                        .is_some_and(|root_sym_id| root_sym_id == old_id)
                });

                // Check if symbol already exists in globals (cross-file merging)
                // IMPORTANT: Only merge symbols from ROOT scope (ScopeId(0))
                // Nested scope symbols should NEVER be merged across scopes
                let new_id = if !is_nested_symbol && !result.is_external_module {
                    let name_atom = name_interner.intern(&sym.escaped_name);
                    if let Some(&existing_id) = merged_symbols.get(&name_atom) {
                        // Symbol exists - check if we can merge
                        if let Some(existing_sym) = global_symbols.get(existing_id) {
                            // Check if symbols can merge (interface+interface, namespace+namespace, etc.)
                            if can_merge_symbols_cross_file(existing_sym.flags, sym.flags) {
                                // Merge: reuse existing symbol ID, will merge declarations below
                                existing_id
                            } else {
                                // Cannot merge - allocate new symbol (shadowing or duplicate)
                                let new_id =
                                    global_symbols.alloc(sym.flags, sym.escaped_name.clone());
                                symbol_arenas.insert(new_id, Arc::clone(&result.arena));
                                merged_symbols.insert(name_atom, new_id);
                                new_id
                            }
                        } else {
                            // Shouldn't happen - allocate new
                            let new_id = global_symbols.alloc(sym.flags, sym.escaped_name.clone());
                            symbol_arenas.insert(new_id, Arc::clone(&result.arena));
                            merged_symbols.insert(name_atom, new_id);
                            new_id
                        }
                    } else {
                        // New symbol - allocate
                        let new_id = global_symbols.alloc(sym.flags, sym.escaped_name.clone());
                        symbol_arenas.insert(new_id, Arc::clone(&result.arena));
                        merged_symbols.insert(name_atom, new_id);
                        new_id
                    }
                } else {
                    // Nested symbol - always allocate new, never merge or add to merged_symbols
                    let new_id = global_symbols.alloc(sym.flags, sym.escaped_name.clone());
                    symbol_arenas.insert(new_id, Arc::clone(&result.arena));
                    // NOTE: Don't add to merged_symbols - nested symbols should never be cross-file merged
                    new_id
                };
                id_remap.insert(old_id, new_id);
            }
        }

        // Track remapped lib symbol IDs for unused-checking exclusion
        for &old_lib_id in &result.lib_symbol_ids {
            if let Some(&new_id) = id_remap.get(&old_lib_id) {
                global_lib_symbol_ids.insert(new_id);
            }
        }

        // Copy symbol_arenas entries from user file, remapping IDs
        // This propagates lib symbol arena mappings that were created during merge_lib_symbols
        for (&old_sym_id, arena) in &result.symbol_arenas {
            if let Some(&new_sym_id) = id_remap.get(&old_sym_id) {
                symbol_arenas
                    .entry(new_sym_id)
                    .or_insert_with(|| Arc::clone(arena));
            }
        }

        // Copy declaration_arenas entries from user file, remapping symbol IDs
        for (&(old_sym_id, decl_idx), arenas) in &result.declaration_arenas {
            if let Some(&new_sym_id) = id_remap.get(&old_sym_id) {
                let target = declaration_arenas
                    .entry((new_sym_id, decl_idx))
                    .or_default();
                for arena in arenas {
                    target.push(Arc::clone(arena));
                }
            }
        }

        // Collect exported symbols for this file (for module_exports map).
        //
        // Note: `export default ...` must be represented under the `"default"` export name
        // so that `import X from "./mod"` can resolve correctly.
        //
        // We intentionally do *not* depend solely on `sym.is_exported` for determining whether
        // a file is an external module, because default exports may not correspond to a named
        // export in `file_locals`.
        let mut exports = SymbolTable::new();
        let mut export_equals_old: Option<SymbolId> = None;

        // 1) Named exports collected from file_locals.
        for (name, &sym_id) in result.file_locals.iter() {
            // Skip lib/global symbols (e.g. `escape`, `unescape`) that were merged
            // into file_locals from lib.d.ts. These are global builtins that should
            // not appear in a user module's module_exports.
            if result.lib_symbol_ids.contains(&sym_id) {
                continue;
            }
            if name == "export=" {
                export_equals_old = Some(sym_id);
            }
            if let Some(sym) = result.symbols.get(sym_id)
                && (sym.is_exported || name == "export=")
                && let Some(&remapped_id) = id_remap.get(&sym_id)
            {
                exports.set(name.clone(), remapped_id);
            }
        }

        // 1b) `export = target` should also expose namespace members from `target`.
        if let Some(old_export_equals_sym) = export_equals_old
            && let Some(target_symbol) = result.symbols.get(old_export_equals_sym)
        {
            if let Some(target_exports) = target_symbol.exports.as_ref() {
                for (export_name, old_sym_id) in target_exports.iter() {
                    if let Some(&remapped_id) = id_remap.get(old_sym_id) {
                        exports.set(export_name.clone(), remapped_id);
                    }
                }
            }
            if let Some(target_members) = target_symbol.members.as_ref() {
                for (member_name, old_sym_id) in target_members.iter() {
                    if let Some(&remapped_id) = id_remap.get(old_sym_id) {
                        exports.set(member_name.clone(), remapped_id);
                    }
                }
            }
        }

        // 2) Default export: add `"default"` entry when present.
        let mut default_export_old: Option<SymbolId> = None;
        if let Some(root_node) = result.arena.get(result.source_file)
            && let Some(source) = result.arena.get_source_file(root_node)
        {
            for &stmt_idx in &source.statements.nodes {
                let Some(stmt_node) = result.arena.get(stmt_idx) else {
                    continue;
                };
                if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                    continue;
                }
                let Some(export_decl) = result.arena.get_export_decl(stmt_node) else {
                    continue;
                };
                if !export_decl.is_default_export {
                    continue;
                }

                // `export default <expr>;`
                let Some(clause_node) = result.arena.get(export_decl.export_clause) else {
                    continue;
                };

                // Best-effort: if the default export is a reference to a named declaration
                // (identifier/class/function), map `"default"` to that symbol.
                //
                // This matches the needs of `import X from "./mod"` and keeps the symbol ID
                // stable across files without synthesizing a new symbol.
                if clause_node.kind == crate::scanner::SyntaxKind::Identifier as u16 {
                    if let Some(ident) = result.arena.get_identifier(clause_node) {
                        default_export_old = result.file_locals.get(&ident.escaped_text);
                    }
                } else if let Some(func) = result.arena.get_function(clause_node) {
                    if let Some(name_node) = result.arena.get(func.name)
                        && let Some(ident) = result.arena.get_identifier(name_node)
                    {
                        default_export_old = result.file_locals.get(&ident.escaped_text);
                    }
                } else if let Some(class) = result.arena.get_class(clause_node)
                    && let Some(name_node) = result.arena.get(class.name)
                    && let Some(ident) = result.arena.get_identifier(name_node)
                {
                    default_export_old = result.file_locals.get(&ident.escaped_text);
                }

                // Only one default export per module.
                break;
            }
        }

        if let Some(old_sym_id) = default_export_old
            && let Some(&remapped_id) = id_remap.get(&old_sym_id)
        {
            exports.set("default".to_string(), remapped_id);
        }

        let remap_symbol_table =
            |table: &SymbolTable, id_remap: &FxHashMap<SymbolId, SymbolId>| -> SymbolTable {
                let mut remapped = SymbolTable::new();
                for (name, old_sym_id) in table.iter() {
                    if let Some(&new_sym_id) = id_remap.get(old_sym_id) {
                        remapped.set(name.clone(), new_sym_id);
                    }
                }
                remapped
            };

        let merge_symbol_table = |dst: &mut SymbolTable, src: &SymbolTable| {
            for (name, sym_id) in src.iter() {
                if !dst.has(name) {
                    dst.set(name.clone(), *sym_id);
                }
            }
        };

        if !exports.is_empty() {
            module_exports.insert(result.file_name.clone(), exports);
        }

        for (module_key, exports_table) in &result.module_exports {
            let remapped = remap_symbol_table(exports_table, &id_remap);
            if !remapped.is_empty() {
                merge_symbol_table(
                    module_exports.entry(module_key.clone()).or_default(),
                    &remapped,
                );
            }
        }

        // Remap binder's per-file alias_partners to global SymbolIds
        for (&type_alias_id, &alias_id) in &result.alias_partners {
            if let (Some(&new_ta), Some(&new_alias)) =
                (id_remap.get(&type_alias_id), id_remap.get(&alias_id))
            {
                alias_partners.insert(new_ta, new_alias);
            }
        }

        // Remap binder's per-file semantic_defs to global SymbolIds (Phase 1 DefId-first)
        for (old_sym_id, entry) in &result.semantic_defs {
            if let Some(&new_sym_id) = id_remap.get(old_sym_id) {
                // Update file_id to use the global file index
                let mut remapped_entry = entry.clone();
                remapped_entry.file_id = file_idx as u32;
                // Only insert the first occurrence (declaration merging keeps first identity)
                semantic_defs.entry(new_sym_id).or_insert(remapped_entry);
            }
        }

        // Collect all nested merge pairs across all symbols in this file,
        // then process them AFTER all symbols have their data populated.
        // This is critical because HashMap iteration order is random — if a
        // parent symbol is processed before its children, the children won't
        // have their exports populated yet, making recursive merge ineffective.
        let mut all_nested_merges: Vec<(SymbolId, SymbolId)> = Vec::new();

        for (old_id, &new_id) in &id_remap {
            // Skip lib-originated symbols - they were already set up by Phase 1 + 1.5
            if result.lib_symbol_ids.contains(old_id) {
                continue;
            }
            let Some(old_sym) = result.symbols.get(*old_id) else {
                continue;
            };

            // CRITICAL: Populate declaration_arenas for user symbols
            for &decl_idx in &old_sym.declarations {
                declaration_arenas
                    .entry((new_id, decl_idx))
                    .or_default()
                    .push(Arc::clone(&result.arena));
            }

            let mut nested_merges: Vec<(SymbolId, SymbolId)> = Vec::new();
            if let Some(new_sym) = global_symbols.get_mut(new_id) {
                // Check if this is a cross-file merge (same symbol already has data)
                let is_cross_file_merge = !new_sym.declarations.is_empty();

                if is_cross_file_merge {
                    // Cross-file merge: append declarations and merge flags
                    new_sym.flags |= old_sym.flags;
                    // Append new declarations from this file, but skip NodeIndex values
                    // that already exist from a DIFFERENT arena (cross-file NodeIndex
                    // collision). When two files produce the same NodeIndex for different
                    // declarations, adding duplicates causes the checker to look up the
                    // wrong arena and misidentify declaration kinds (e.g., treating a
                    // remote interface as a local class, triggering false TS2300).
                    // The declaration_arenas entry already contains both arenas for the
                    // colliding NodeIndex, so the checker can iterate all arenas there.
                    {
                        let mut filtered_decls: Vec<NodeIndex> = Vec::new();
                        for &decl_idx in &old_sym.declarations {
                            if new_sym.declarations.contains(&decl_idx) {
                                // NodeIndex collision: this index already exists in the
                                // merged symbol from a previous file. Check if the
                                // declaration_arenas entry has a different arena (meaning
                                // it's from a different file, not a true duplicate).
                                if let Some(arenas) = declaration_arenas.get(&(new_id, decl_idx)) {
                                    let has_different_arena = arenas.iter().any(|a| {
                                        !std::ptr::eq(Arc::as_ptr(a), Arc::as_ptr(&result.arena))
                                    });
                                    if has_different_arena {
                                        // Skip: this is a cross-file collision, not a
                                        // true duplicate declaration within the same file.
                                        continue;
                                    }
                                }
                            }
                            filtered_decls.push(decl_idx);
                        }
                        append_unique_declarations(&mut new_sym.declarations, &filtered_decls);
                    }
                    // Update value_declaration if the old one was NONE
                    if new_sym.value_declaration.is_none() && !old_sym.value_declaration.is_none() {
                        new_sym.value_declaration = old_sym.value_declaration;
                    }
                    // Merge exports (if both have exports)
                    // First pass: add missing exports, collect nested merge targets
                    if let (Some(old_exports), Some(new_exports)) =
                        (old_sym.exports.as_ref(), new_sym.exports.as_mut())
                    {
                        for (name, sym_id) in old_exports.iter() {
                            if !new_exports.has(name) {
                                // Remap the symbol ID and add to exports
                                if let Some(&remapped_id) = id_remap.get(sym_id) {
                                    new_exports.set(name.clone(), remapped_id);
                                }
                            } else if let Some(&remapped_new_id) = id_remap.get(sym_id) {
                                // Both files export the same name (e.g., nested namespace Utils).
                                // Record for deferred merge outside the get_mut borrow scope.
                                let existing_export_id = new_exports
                                    .get(name)
                                    .expect("else branch guarantees name exists in new_exports");
                                if existing_export_id != remapped_new_id {
                                    nested_merges.push((existing_export_id, remapped_new_id));
                                }
                            }
                        }
                    }
                    // Handle case where old symbol has exports but new doesn't yet
                    if old_sym.exports.is_some() && new_sym.exports.is_none() {
                        new_sym.exports = old_sym
                            .exports
                            .as_ref()
                            .map(|table| Box::new(remap_symbol_table(table.as_ref(), &id_remap)));
                    }
                    // Merge members (if both have members)
                    if let (Some(old_members), Some(new_members)) =
                        (old_sym.members.as_ref(), new_sym.members.as_mut())
                    {
                        for (name, sym_id) in old_members.iter() {
                            if !new_members.has(name) {
                                // Remap the symbol ID and add to members
                                if let Some(&remapped_id) = id_remap.get(sym_id) {
                                    new_members.set(name.clone(), remapped_id);
                                }
                            }
                        }
                    }
                } else {
                    // First time seeing this symbol - full update
                    let mut updated = old_sym.clone();
                    updated.id = new_id;
                    updated.parent = id_remap
                        .get(&old_sym.parent)
                        .copied()
                        .unwrap_or(SymbolId::NONE);
                    updated.value_declaration = old_sym.value_declaration;
                    updated.declarations = old_sym.declarations.clone();
                    updated.is_exported = old_sym.is_exported;
                    updated.is_umd_export = old_sym.is_umd_export;
                    // Track which file this symbol was declared in for TDZ cross-file detection
                    updated.decl_file_idx = file_idx as u32;
                    updated.exports = old_sym
                        .exports
                        .as_ref()
                        .map(|table| Box::new(remap_symbol_table(table.as_ref(), &id_remap)));
                    updated.members = old_sym
                        .members
                        .as_ref()
                        .map(|table| Box::new(remap_symbol_table(table.as_ref(), &id_remap)));
                    *new_sym = updated;
                }
            }

            // Collect nested merges for processing AFTER all symbols are populated
            all_nested_merges.extend(nested_merges);
        }

        // Process all nested merges now that every symbol has its data populated.
        // Uses a work queue to handle arbitrarily deep nesting (e.g.,
        // namespace A.B.C.D declared across files needs recursive merge).
        while let Some((existing_id, source_id)) = all_nested_merges.pop() {
            // Collect data from source symbol first
            let merge_data = global_symbols.get(source_id).map(|src| {
                (
                    src.flags,
                    src.declarations.clone(),
                    src.value_declaration,
                    src.exports.as_ref().cloned(),
                    src.members.as_ref().cloned(),
                )
            });
            if let Some((src_flags, src_decls, src_val_decl, src_exports, src_members)) = merge_data
                && let Some(dst) = global_symbols.get_mut(existing_id)
            {
                let can_merge = can_merge_symbols_cross_file(dst.flags, src_flags);
                if !can_merge {
                    continue;
                }
                dst.flags |= src_flags;
                // Propagate declaration_arenas from source to destination
                // so the checker can find declarations from the merged file
                for &decl_idx in &src_decls {
                    let cloned_arenas: Option<Vec<Arc<NodeArena>>> = declaration_arenas
                        .get(&(source_id, decl_idx))
                        .map(|a| a.iter().cloned().collect());
                    if let Some(arenas) = cloned_arenas {
                        let target = declaration_arenas
                            .entry((existing_id, decl_idx))
                            .or_default();
                        for arena in arenas {
                            target.push(arena);
                        }
                    }
                }
                // Also propagate symbol_arenas if source has one
                let cloned_arena = symbol_arenas.get(&source_id).cloned();
                if let Some(arena) = cloned_arena {
                    symbol_arenas.entry(existing_id).or_insert(arena);
                }
                append_unique_declarations(&mut dst.declarations, &src_decls);
                if dst.value_declaration.is_none() && !src_val_decl.is_none() {
                    dst.value_declaration = src_val_decl;
                }
                if let Some(src_exp) = src_exports {
                    let dst_exp = dst
                        .exports
                        .get_or_insert_with(|| Box::new(SymbolTable::new()));
                    for (ename, &esym) in src_exp.iter() {
                        if !dst_exp.has(ename) {
                            dst_exp.set(ename.clone(), esym);
                        } else {
                            // Both sides export the same name — queue recursive merge
                            let existing_export_id = dst_exp
                                .get(ename)
                                .expect("else branch guarantees ename exists in dst_exp");
                            if existing_export_id != esym {
                                all_nested_merges.push((existing_export_id, esym));
                            }
                        }
                    }
                }
                if let Some(src_mem) = src_members {
                    let dst_mem = dst
                        .members
                        .get_or_insert_with(|| Box::new(SymbolTable::new()));
                    for (mname, &msym) in src_mem.iter() {
                        if !dst_mem.has(mname) {
                            dst_mem.set(mname.clone(), msym);
                        } else {
                            // Both sides have the same member — queue recursive merge
                            let existing_member_id = dst_mem
                                .get(mname)
                                .expect("else branch guarantees mname exists in dst_mem");
                            if existing_member_id != msym {
                                all_nested_merges.push((existing_member_id, msym));
                            }
                        }
                    }
                }
            }
        }

        // Remap node_symbols to use global IDs
        // Note: node_symbols primarily maps user file nodes to user symbols,
        // but lib symbols referenced in user code need remapping too
        let mut remapped_node_symbols = FxHashMap::default();
        for (node_idx, old_sym_id) in &result.node_symbols {
            if let Some(&new_sym_id) = id_remap.get(old_sym_id) {
                remapped_node_symbols.insert(*node_idx, new_sym_id);
            }
            // Note: We don't need to check lib_symbol_remap here because
            // node_symbols are created during binding of user files, and at that point
            // lib symbols are accessed by name lookup (file_locals), not by node mapping
        }

        // Remap file_locals to use global IDs
        // This handles both user symbols (from id_remap) and lib symbols (from lib_name_to_global)
        let mut remapped_file_locals = SymbolTable::new();
        for (name, old_sym_id) in result.file_locals.iter() {
            if let Some(&new_sym_id) = id_remap.get(old_sym_id) {
                // User symbol - use remapped ID
                remapped_file_locals.set(name.clone(), new_sym_id);
                // Also add to globals (all top-level declarations visible globally)
                // EXCEPT ALIAS symbols (import declarations) which are file-local by design.
                // Leaking import aliases to globals causes cross-file contamination where
                // other files try to resolve the import and get incorrect types.
                // Exception: UMD namespace exports (`export as namespace Foo`) are ALIAS
                // symbols that SHOULD be globally visible — they register a name on the
                // global object.
                let sym_info = global_symbols.get(new_sym_id);
                let is_alias =
                    sym_info.is_some_and(|s| s.flags & crate::binder::symbol_flags::ALIAS != 0);
                let is_umd = sym_info.is_some_and(|s| s.is_umd_export);
                if !is_alias || is_umd {
                    globals.set(name.clone(), new_sym_id);
                }
            } else {
                let name_atom = name_interner.intern(name);
                if let Some(&global_id) = lib_name_to_global.get(&name_atom) {
                    // Lib symbol - use the pre-remapped global ID
                    // Only add to file_locals, NOT to globals (lib symbols are accessed
                    // through lib_contexts in the checker, not through globals)
                    remapped_file_locals.set(name.clone(), global_id);
                }
            }
        }

        let mut remapped_scopes = Vec::with_capacity(result.scopes.len());
        for scope in &result.scopes {
            let mut table = SymbolTable::new();
            for (name, old_sym_id) in scope.table.iter() {
                if let Some(&new_sym_id) = id_remap.get(old_sym_id) {
                    // User symbol - include in scope
                    table.set(name.clone(), new_sym_id);
                }
                // NOTE: We intentionally do NOT add lib symbols to scopes.
                // Lib symbols have declaration NodeIndex values from lib arenas which
                // can accidentally match valid indices in user file arenas, causing
                // false duplicate identifier detection. Lib symbols are accessible
                // through file_locals for type lookup, but should not be in scopes.
            }
            remapped_scopes.push(Scope {
                parent: scope.parent,
                table,
                kind: scope.kind,
                container_node: scope.container_node,
            });
        }

        file_locals_list.push(remapped_file_locals);

        // Populate arena context for module augmentations
        let module_augmentations: FxHashMap<String, Vec<crate::binder::ModuleAugmentation>> =
            result
                .module_augmentations
                .iter()
                .map(|(spec, augs)| {
                    let arena = Arc::clone(&result.arena);
                    (
                        spec.clone(),
                        augs.iter()
                            .map(|aug| {
                                crate::binder::ModuleAugmentation::with_arena(
                                    aug.name.clone(),
                                    aug.node,
                                    Arc::clone(&arena),
                                )
                            })
                            .collect(),
                    )
                })
                .collect();

        files.push(BoundFile {
            file_name: result.file_name.clone(),
            source_file: result.source_file,
            arena: Arc::clone(&result.arena),
            node_symbols: remapped_node_symbols,
            module_declaration_exports_publicly: result.module_declaration_exports_publicly.clone(),
            scopes: remapped_scopes,
            node_scope_ids: result.node_scope_ids.clone(),
            parse_diagnostics: result.parse_diagnostics.clone(),
            global_augmentations: result.global_augmentations.clone(),
            module_augmentations,
            augmentation_target_modules: result
                .augmentation_target_modules
                .iter()
                .map(|(&old_sym, name)| {
                    let new_sym = id_remap.get(&old_sym).copied().unwrap_or(old_sym);
                    (new_sym, name.clone())
                })
                .collect(),
            flow_nodes: result.flow_nodes.clone(),
            node_flow: result.node_flow.clone(),
            switch_clause_to_switch: result.switch_clause_to_switch.clone(),
            is_external_module: result.is_external_module,
            expando_properties: remap_expando_properties(&result.expando_properties, &id_remap),
            file_features: result.file_features,
        });
    }

    // Build cross_file_node_symbols: map each arena pointer to its remapped node_symbols.
    // This enables the checker to resolve type references in cross-file interface declarations.
    for file in &files {
        let arena_ptr = Arc::as_ptr(&file.arena) as usize;
        cross_file_node_symbols.insert(arena_ptr, Arc::new(file.node_symbols.clone()));
    }

    // Validate skeleton data against legacy merge state before construction.
    // This runs only in debug builds and proves skeleton captures all
    // merge-relevant ambient module topology.
    {
        let user_file_names: FxHashSet<String> =
            files.iter().map(|f| f.file_name.clone()).collect();
        let module_export_keys: FxHashSet<String> = module_exports.keys().cloned().collect();
        skeleton_index.validate_against_merged(
            &declared_modules,
            &shorthand_ambient_modules,
            &module_export_keys,
            &user_file_names,
        );
    }

    MergedProgram {
        files,
        symbols: global_symbols,
        symbol_arenas,
        declaration_arenas,
        cross_file_node_symbols,
        globals,
        file_locals: file_locals_list,
        declared_modules,
        shorthand_ambient_modules,
        module_exports,
        reexports,
        wildcard_reexports,
        wildcard_reexports_type_only,
        lib_binders,
        lib_symbol_ids: global_lib_symbol_ids,
        type_interner: TypeInterner::new(),
        alias_partners,
        semantic_defs,
        skeleton_index: Some(skeleton_index),
    }
}

/// Full pipeline: Parse → Bind (parallel) → Merge (sequential)
///
/// This is the main entry point for multi-file compilation.
/// Lib files are automatically loaded and merged during binding.
pub fn compile_files(files: Vec<(String, String)>) -> MergedProgram {
    let lib_files = resolve_default_lib_files(ScriptTarget::ESNext)
        .unwrap_or_else(|err| panic!("failed to resolve default lib files: {err}"));
    compile_files_with_libs(files, &lib_files)
}

/// Full pipeline with explicit lib files.
///
/// Callers are responsible for providing the resolved lib file paths.
pub fn compile_files_with_libs(
    files: Vec<(String, String)>,
    lib_files: &[PathBuf],
) -> MergedProgram {
    let lib_paths: Vec<&Path> = lib_files.iter().map(PathBuf::as_path).collect();
    let bind_results = parse_and_bind_parallel_with_lib_files(files, &lib_paths);
    merge_bind_results(bind_results)
}

// =============================================================================
// Parallel Type Checking
// =============================================================================

use crate::checker::context::{CheckerOptions, LibContext};
use crate::checker::diagnostics::Diagnostic;
use crate::checker::state::CheckerState;
use crate::lib_loader::LibFile;
use crate::parser::syntax_kind_ext;
use tsz_solver::TypeId;

/// Result of type checking a single function body
#[derive(Debug)]
pub struct FunctionCheckResult {
    /// Function node index within its file
    pub function_idx: NodeIndex,
    /// File index in the program
    pub file_idx: usize,
    /// Inferred return type
    pub return_type: TypeId,
    /// Diagnostics produced
    pub diagnostics: Vec<Diagnostic>,
}

/// Result of type checking all function bodies in a file
pub struct FileCheckResult {
    /// File index
    pub file_idx: usize,
    /// File name
    pub file_name: String,
    /// Function check results
    pub function_results: Vec<FunctionCheckResult>,
    /// File-level diagnostics
    pub diagnostics: Vec<Diagnostic>,
}

/// Result of parallel type checking
pub struct CheckResult {
    /// Per-file check results
    pub file_results: Vec<FileCheckResult>,
    /// Total functions checked
    pub function_count: usize,
    /// Total diagnostics
    pub diagnostic_count: usize,
}

/// Collect all function declarations from a source file
fn collect_functions(arena: &NodeArena, source_file: NodeIndex) -> Vec<NodeIndex> {
    let mut functions = Vec::new();

    let Some(sf) = arena.get_source_file_at(source_file) else {
        return functions;
    };

    for &stmt_idx in &sf.statements.nodes {
        collect_functions_from_node(arena, stmt_idx, &mut functions);
    }

    functions
}

/// Recursively collect functions from a node
fn collect_functions_from_node(
    arena: &NodeArena,
    node_idx: NodeIndex,
    functions: &mut Vec<NodeIndex>,
) {
    let Some(node) = arena.get(node_idx) else {
        return;
    };

    match node.kind {
        k if k == syntax_kind_ext::FUNCTION_DECLARATION
            || k == syntax_kind_ext::FUNCTION_EXPRESSION
            || k == syntax_kind_ext::ARROW_FUNCTION =>
        {
            functions.push(node_idx);
            // Also collect nested functions in the body
            if let Some(func) = arena.get_function(node)
                && !func.body.is_none()
            {
                collect_functions_from_node(arena, func.body, functions);
            }
        }
        k if k == syntax_kind_ext::METHOD_DECLARATION => {
            functions.push(node_idx);
            // Also collect nested functions in the body
            if let Some(method) = arena.get_method_decl(node)
                && !method.body.is_none()
            {
                collect_functions_from_node(arena, method.body, functions);
            }
        }
        k if k == syntax_kind_ext::CLASS_DECLARATION => {
            if let Some(class) = arena.get_class(node) {
                for &member_idx in &class.members.nodes {
                    collect_functions_from_node(arena, member_idx, functions);
                }
            }
        }
        k if k == syntax_kind_ext::BLOCK => {
            if let Some(block) = arena.get_block(node) {
                for &stmt_idx in &block.statements.nodes {
                    collect_functions_from_node(arena, stmt_idx, &mut *functions);
                }
            }
        }
        k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
            // Variable statement contains a declaration list which contains declarations
            if let Some(var_stmt) = arena.get_variable(node) {
                // var_stmt.declarations contains the VARIABLE_DECLARATION_LIST node(s)
                for &decl_list_idx in &var_stmt.declarations.nodes {
                    if let Some(decl_list_node) = arena.get(decl_list_idx) {
                        // The declaration list also uses VariableData
                        if let Some(decl_list) = arena.get_variable(decl_list_node) {
                            // Now decl_list.declarations contains the actual VARIABLE_DECLARATION nodes
                            for &decl_idx in &decl_list.declarations.nodes {
                                if let Some(decl_node) = arena.get(decl_idx)
                                    && let Some(decl) = arena.get_variable_declaration(decl_node)
                                    && !decl.initializer.is_none()
                                {
                                    collect_functions_from_node(arena, decl.initializer, functions);
                                }
                            }
                        }
                    }
                }
            }
        }
        k if k == syntax_kind_ext::EXPORT_DECLARATION => {
            // Export declarations may contain function/class declarations
            if let Some(export) = arena.get_export_decl(node)
                && !export.export_clause.is_none()
            {
                collect_functions_from_node(arena, export.export_clause, functions);
            }
        }
        _ => {}
    }
}

/// Type check function bodies in parallel
///
/// After binding is complete and symbols are merged, function bodies
/// can be type-checked in parallel because:
/// 1. Each function body only uses local variables and global symbols
/// 2. Local type inference doesn't modify global state
/// 3. Each function is independent
///
/// # Arguments
/// * `program` - The merged program with global symbols
///
/// # Returns
/// `CheckResult` with diagnostics from all functions
pub fn check_functions_parallel(program: &MergedProgram) -> CheckResult {
    let shared_binders: Vec<Arc<BinderState>> = program
        .files
        .iter()
        .enumerate()
        .map(|(file_idx, file)| Arc::new(create_binder_from_bound_file(file, program, file_idx)))
        .collect();
    let all_binders = Arc::new(shared_binders.clone());
    let all_arenas = Arc::new(
        program
            .files
            .iter()
            .map(|file| Arc::clone(&file.arena))
            .collect::<Vec<_>>(),
    );
    let symbol_file_targets: Vec<(tsz_binder::SymbolId, usize)> = program
        .symbol_arenas
        .iter()
        .filter_map(|(sym_id, arena)| {
            all_arenas
                .iter()
                .position(|file_arena| Arc::ptr_eq(file_arena, arena))
                .map(|file_idx| (*sym_id, file_idx))
        })
        .collect();

    // First, collect all functions from all files (sequential)
    let mut all_functions: Vec<(usize, NodeIndex)> = Vec::new();

    for (file_idx, file) in program.files.iter().enumerate() {
        let functions = collect_functions(&file.arena, file.source_file);
        for func_idx in functions {
            all_functions.push((file_idx, func_idx));
        }
    }

    let function_count = all_functions.len();

    // Check functions in parallel
    // Note: We need to be careful here - CheckerState holds mutable references
    // For now, we group by file and check each file's functions together
    let file_results: Vec<FileCheckResult> = maybe_parallel_iter!(program.files)
        .enumerate()
        .map(|(file_idx, file)| {
            let functions = collect_functions(&file.arena, file.source_file);

            let binder = Arc::clone(&shared_binders[file_idx]);

            // Create a per-thread QueryCache for memoized evaluate_type/is_subtype_of calls.
            // Each thread gets its own cache using RefCell/Cell (no atomic overhead).
            let query_cache = tsz_solver::QueryCache::new(&program.type_interner);

            // Create checker for this file, using the shared type interner
            let compiler_options = crate::checker::context::CheckerOptions::default();
            let mut checker = CheckerState::new(
                &file.arena,
                binder.as_ref(),
                &query_cache,
                file.file_name.clone(),
                compiler_options, // default options for internal operations
            );
            checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
            checker.ctx.set_all_binders(Arc::clone(&all_binders));
            checker.ctx.set_current_file_idx(file_idx);
            {
                let mut targets = checker.ctx.cross_file_symbol_targets.borrow_mut();
                for (sym_id, owner_idx) in &symbol_file_targets {
                    targets.insert(*sym_id, *owner_idx);
                }
            }

            let mut function_results = Vec::new();

            for func_idx in functions {
                // Check the function
                let return_type = checker.get_type_of_node(func_idx);

                function_results.push(FunctionCheckResult {
                    function_idx: func_idx,
                    file_idx,
                    return_type,
                    diagnostics: Vec::new(), // Diagnostics are collected at file level
                });
            }

            // Collect diagnostics from checker
            let diagnostics = std::mem::take(&mut checker.ctx.diagnostics);

            FileCheckResult {
                file_idx,
                file_name: file.file_name.clone(),
                function_results,
                diagnostics,
            }
        })
        .collect();

    let diagnostic_count: usize = file_results.iter().map(|r| r.diagnostics.len()).sum();

    CheckResult {
        file_results,
        function_count,
        diagnostic_count,
    }
}

/// Type check full source files in parallel.
///
/// This runs `check_source_file` for each file, which validates all top-level
/// statements and function bodies. Compiler options and lib contexts are applied
/// so diagnostics match normal compilation behavior.
pub fn check_files_parallel(
    program: &MergedProgram,
    checker_options: &CheckerOptions,
    lib_files: &[Arc<LibFile>],
) -> CheckResult {
    // Create lib_contexts from lib_files (contains both arena and binder)
    // The binders in lib_files should match the binders in program.lib_binders
    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();

    let shared_binders: Vec<Arc<BinderState>> = program
        .files
        .iter()
        .enumerate()
        .map(|(file_idx, file)| Arc::new(create_binder_from_bound_file(file, program, file_idx)))
        .collect();
    let all_binders = Arc::new(shared_binders.clone());
    let all_arenas = Arc::new(
        program
            .files
            .iter()
            .map(|file| Arc::clone(&file.arena))
            .collect::<Vec<_>>(),
    );
    let symbol_file_targets: Vec<(tsz_binder::SymbolId, usize)> = program
        .symbol_arenas
        .iter()
        .filter_map(|(sym_id, arena)| {
            all_arenas
                .iter()
                .position(|file_arena| Arc::ptr_eq(file_arena, arena))
                .map(|file_idx| (*sym_id, file_idx))
        })
        .collect();

    let file_results: Vec<FileCheckResult> = maybe_parallel_iter!(program.files)
        .enumerate()
        .map(|(file_idx, file)| {
            let binder = Arc::clone(&shared_binders[file_idx]);

            // Create a per-thread QueryCache for memoized evaluate_type/is_subtype_of calls.
            // Each thread gets its own cache using RefCell/Cell (no atomic overhead).
            let query_cache = tsz_solver::QueryCache::new(&program.type_interner);

            let mut checker = CheckerState::with_options(
                &file.arena,
                binder.as_ref(),
                &query_cache,
                file.file_name.clone(),
                checker_options,
            );
            checker.ctx.set_all_arenas(Arc::clone(&all_arenas));

            // Use skeleton-derived declared modules when available (skips binder scan).
            if let Some(ref skel) = program.skeleton_index {
                let (exact, patterns) = skel.build_declared_module_sets();
                checker.ctx.set_declared_modules_from_skeleton(Arc::new(
                    crate::checker::context::GlobalDeclaredModules::from_skeleton(exact, patterns),
                ));
            }

            checker.ctx.set_all_binders(Arc::clone(&all_binders));
            checker.ctx.set_current_file_idx(file_idx);

            {
                let mut targets = checker.ctx.cross_file_symbol_targets.borrow_mut();
                for (sym_id, owner_idx) in &symbol_file_targets {
                    targets.insert(*sym_id, *owner_idx);
                }
            }

            if !lib_contexts.is_empty() {
                checker.ctx.set_lib_contexts(lib_contexts.clone());
                checker.ctx.set_actual_lib_file_count(lib_contexts.len());
            }

            checker.check_source_file(file.source_file);

            let diagnostics = std::mem::take(&mut checker.ctx.diagnostics);

            FileCheckResult {
                file_idx,
                file_name: file.file_name.clone(),
                function_results: Vec::new(),
                diagnostics,
            }
        })
        .collect();

    let diagnostic_count: usize = file_results.iter().map(|r| r.diagnostics.len()).sum();

    CheckResult {
        file_results,
        function_count: 0,
        diagnostic_count,
    }
}

/// Create a `BinderState` from a `BoundFile` for type checking
pub fn create_binder_from_bound_file(
    file: &BoundFile,
    program: &MergedProgram,
    file_idx: usize,
) -> BinderState {
    // Get file locals for this specific file
    let mut file_locals = SymbolTable::new();

    // Copy from program.file_locals if available
    if file_idx < program.file_locals.len() {
        for (name, &sym_id) in program.file_locals[file_idx].iter() {
            file_locals.set(name.clone(), sym_id);
        }
    }

    // Also add globals (for cross-file references)
    for (name, &sym_id) in program.globals.iter() {
        if !file_locals.has(name) {
            file_locals.set(name.clone(), sym_id);
        }
    }

    // Merge module augmentations from all files
    // When checking a file, we need access to augmentations from all other files
    let mut merged_module_augmentations: rustc_hash::FxHashMap<
        String,
        Vec<crate::binder::ModuleAugmentation>,
    > = rustc_hash::FxHashMap::default();
    let mut merged_augmentation_target_modules: rustc_hash::FxHashMap<
        crate::binder::SymbolId,
        String,
    > = rustc_hash::FxHashMap::default();

    for other_file in &program.files {
        for (spec, augs) in &other_file.module_augmentations {
            merged_module_augmentations
                .entry(spec.clone())
                .or_default()
                .extend(augs.iter().map(|aug| {
                    crate::binder::ModuleAugmentation::with_arena(
                        aug.name.clone(),
                        aug.node,
                        Arc::clone(&other_file.arena),
                    )
                }));
        }
        for (&sym_id, module_spec) in &other_file.augmentation_target_modules {
            merged_augmentation_target_modules.insert(sym_id, module_spec.clone());
        }
    }

    // Merge global augmentations from all files
    // When checking a file, we need access to `declare global` augmentations from all other files.
    // Each augmentation gets tagged with its source arena for cross-file resolution.
    let mut merged_global_augmentations: rustc_hash::FxHashMap<
        String,
        Vec<crate::binder::GlobalAugmentation>,
    > = rustc_hash::FxHashMap::default();

    for other_file in &program.files {
        for (name, decls) in &other_file.global_augmentations {
            merged_global_augmentations
                .entry(name.clone())
                .or_default()
                .extend(decls.iter().map(|aug| {
                    // Tag each augmentation with its source file's arena
                    // so the checker can read declaration nodes from the correct arena
                    crate::binder::GlobalAugmentation::with_arena(
                        aug.node,
                        Arc::clone(&other_file.arena),
                    )
                }));
        }
    }

    let mut binder = BinderState::from_bound_state_with_scopes_and_augmentations(
        BinderOptions::default(),
        program.symbols.clone(),
        file_locals,
        file.node_symbols.clone(),
        BinderStateScopeInputs {
            scopes: file.scopes.clone(),
            node_scope_ids: file.node_scope_ids.clone(),
            global_augmentations: merged_global_augmentations,
            module_augmentations: merged_module_augmentations,
            augmentation_target_modules: merged_augmentation_target_modules,
            module_exports: program.module_exports.clone(),
            module_declaration_exports_publicly: file.module_declaration_exports_publicly.clone(),
            reexports: program.reexports.clone(),
            wildcard_reexports: program.wildcard_reexports.clone(),
            wildcard_reexports_type_only: program.wildcard_reexports_type_only.clone(),
            symbol_arenas: program.symbol_arenas.clone(),
            declaration_arenas: program.declaration_arenas.clone(),
            cross_file_node_symbols: program.cross_file_node_symbols.clone(),
            shorthand_ambient_modules: program.shorthand_ambient_modules.clone(),
            modules_with_export_equals: FxHashSet::default(),
            flow_nodes: file.flow_nodes.clone(),
            node_flow: file.node_flow.clone(),
            switch_clause_to_switch: file.switch_clause_to_switch.clone(),
            expando_properties: file.expando_properties.clone(),
            alias_partners: program.alias_partners.clone(),
        },
    );

    binder.is_external_module = file.is_external_module;
    binder.file_features = file.file_features;
    binder.semantic_defs = program.semantic_defs.clone();
    if let Some(root_scope) = binder.scopes.first() {
        binder.current_scope = root_scope.table.clone();
        binder.current_scope_id = crate::binder::ScopeId(0);
    }

    binder.declared_modules = program.declared_modules.clone();

    // Mark lib symbols as merged since the MergedProgram's symbol arena
    // contains all remapped lib symbols with unique global IDs.
    // This enables the fast path in get_symbol() that avoids cross-binder lookups.
    binder.set_lib_symbols_merged(true);

    binder
}

/// Check function bodies with statistics
pub fn check_functions_with_stats(program: &MergedProgram) -> (CheckResult, CheckStats) {
    let result = check_functions_parallel(program);

    let stats = CheckStats {
        file_count: result.file_results.len(),
        function_count: result.function_count,
        diagnostic_count: result.diagnostic_count,
    };

    (result, stats)
}

/// Statistics about parallel type checking
#[derive(Debug, Clone)]
pub struct CheckStats {
    /// Number of files checked
    pub file_count: usize,
    /// Number of functions checked
    pub function_count: usize,
    /// Number of diagnostics produced
    pub diagnostic_count: usize,
}

/// Parse files and collect statistics
pub fn parse_files_with_stats(files: Vec<(String, String)>) -> (Vec<ParseResult>, ParallelStats) {
    let total_bytes: usize = files.iter().map(|(_, src)| src.len()).sum();
    let file_count = files.len();

    let results = parse_files_parallel(files);

    let total_nodes: usize = results.iter().map(|r| r.arena.len()).sum();
    let error_count: usize = results.iter().map(|r| r.parse_diagnostics.len()).sum();

    let stats = ParallelStats {
        file_count,
        total_bytes,
        total_nodes,
        error_count,
    };

    (results, stats)
}

#[cfg(test)]
#[path = "../tests/parallel_tests.rs"]
mod tests;
