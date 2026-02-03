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
//! ```rust,ignore
//! use wasm::parallel::parse_files_parallel;
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
use crate::binder::{
    FlowNodeArena, FlowNodeId, Scope, ScopeId, SymbolArena, SymbolId, SymbolTable,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::cli::config::resolve_default_lib_files;
use crate::emitter::ScriptTarget;
use crate::lib_loader;
use crate::parser::NodeIndex;
use crate::parser::node::NodeArena;
use crate::parser::{ParseDiagnostic, ParserState};
#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(target_arch = "wasm32")]
fn resolve_default_lib_files(_target: ScriptTarget) -> anyhow::Result<Vec<PathBuf>> {
    Ok(Vec::new())
}

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
/// * `files` - Vector of (file_name, source_text) pairs
///
/// # Returns
/// Vector of ParseResult for each file
pub fn parse_files_parallel(files: Vec<(String, String)>) -> Vec<ParseResult> {
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
    /// Node-to-symbol mapping
    pub node_symbols: FxHashMap<u32, SymbolId>,
    /// Persistent scopes for stateless checking
    pub scopes: Vec<Scope>,
    /// Map from AST node to scope ID
    pub node_scope_ids: FxHashMap<u32, ScopeId>,
    /// Parse diagnostics
    pub parse_diagnostics: Vec<ParseDiagnostic>,
    /// Shorthand ambient modules (`declare module "foo"` without body)
    pub shorthand_ambient_modules: FxHashSet<String>,
    /// Global augmentations (interface declarations inside `declare global` blocks)
    pub global_augmentations: FxHashMap<String, Vec<NodeIndex>>,
    /// Re-exports: tracks `export { x } from 'module'` declarations
    pub reexports: FxHashMap<String, FxHashMap<String, (String, Option<String>)>>,
    /// Wildcard re-exports: tracks `export * from 'module'` declarations
    pub wildcard_reexports: FxHashMap<String, Vec<String>>,
    /// Lib binders for global type resolution (Array, String, etc.)
    /// These are merged from lib.d.ts files and enable cross-file symbol lookup
    pub lib_binders: Vec<Arc<BinderState>>,
    /// Flow nodes for control flow analysis
    pub flow_nodes: FlowNodeArena,
    /// Node-to-flow mapping: tracks which flow node was active at each AST node
    pub node_flow: FxHashMap<u32, FlowNodeId>,
}

/// Parse and bind multiple files in parallel
///
/// Each file is parsed and bound independently. The binding creates
/// file-local symbols which can later be merged into a global scope.
///
/// # Arguments
/// * `files` - Vector of (file_name, source_text) pairs
///
/// # Returns
/// Vector of BindResult for each file
pub fn parse_and_bind_parallel(files: Vec<(String, String)>) -> Vec<BindResult> {
    maybe_parallel_into!(files)
        .map(|(file_name, source_text)| {
            // Parse
            let mut parser = ParserState::new(file_name.clone(), source_text);
            let source_file = parser.parse_source_file();

            let (arena, parse_diagnostics) = parser.into_parts();

            // Bind
            let mut binder = BinderState::new();
            binder.bind_source_file(&arena, source_file);

            BindResult {
                file_name,
                source_file,
                arena: Arc::new(arena),
                symbols: binder.symbols,
                file_locals: binder.file_locals,
                declared_modules: binder.declared_modules,
                node_symbols: binder.node_symbols,
                scopes: binder.scopes,
                node_scope_ids: binder.node_scope_ids,
                parse_diagnostics,
                shorthand_ambient_modules: binder.shorthand_ambient_modules,
                global_augmentations: binder.global_augmentations,
                reexports: binder.reexports,
                wildcard_reexports: binder.wildcard_reexports,
                lib_binders: Vec::new(), // No libs in this path
                flow_nodes: binder.flow_nodes,
                node_flow: binder.node_flow,
            }
        })
        .collect()
}

/// Bind a single file (for comparison/testing)
pub fn parse_and_bind_single(file_name: String, source_text: String) -> BindResult {
    let mut parser = ParserState::new(file_name.clone(), source_text);
    let source_file = parser.parse_source_file();

    let (arena, parse_diagnostics) = parser.into_parts();

    let mut binder = BinderState::new();
    binder.bind_source_file(&arena, source_file);

    BindResult {
        file_name,
        source_file,
        arena: Arc::new(arena),
        symbols: binder.symbols,
        file_locals: binder.file_locals,
        declared_modules: binder.declared_modules,
        node_symbols: binder.node_symbols,
        scopes: binder.scopes,
        node_scope_ids: binder.node_scope_ids,
        parse_diagnostics,
        shorthand_ambient_modules: binder.shorthand_ambient_modules,
        global_augmentations: binder.global_augmentations,
        reexports: binder.reexports,
        wildcard_reexports: binder.wildcard_reexports,
        lib_binders: Vec::new(), // No libs in this path
        flow_nodes: binder.flow_nodes,
        node_flow: binder.node_flow,
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

/// Load lib.d.ts files and create LibContext objects for the binder.
///
/// This function loads the specified lib.d.ts files (e.g., lib.dom.d.ts, lib.es*.d.ts)
/// and returns LibContext objects that can be used during binding to resolve global
/// symbols like `console`, `Array`, `Promise`, etc.
///
/// This is similar to `load_lib_files_for_contexts` in driver.rs but returns
/// Arc<LibFile> objects for use with `merge_lib_symbols`.
pub fn load_lib_files_for_binding(lib_files: &[&Path]) -> Vec<Arc<lib_loader::LibFile>> {
    use crate::parser::ParserState;
    use rayon::prelude::*;

    if lib_files.is_empty() {
        return Vec::new();
    }

    // Collect paths that exist
    let files_to_load: Vec<_> = lib_files
        .iter()
        .filter_map(|p| {
            let path = p.to_path_buf();
            if path.exists() { Some(path) } else { None }
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

            Some(Arc::new(lib_loader::LibFile::new(file_name, arena, binder)))
        })
        .collect()
}

/// Parse and bind multiple files in parallel with lib symbol injection.
///
/// This is the main entry point for compilation that includes lib.d.ts symbols.
/// Lib files are loaded first, then each file is parsed and bound with lib symbols
/// merged into its binder.
///
/// # Arguments
/// * `files` - Vector of (file_name, source_text) pairs
/// * `lib_files` - Optional list of lib file paths to load
///
/// # Returns
/// Vector of BindResult for each file
pub fn parse_and_bind_parallel_with_lib_files(
    files: Vec<(String, String)>,
    lib_files: &[&Path],
) -> Vec<BindResult> {
    // Load lib files for binding
    let lib_contexts = load_lib_files_for_binding(lib_files);

    // Parse and bind with lib symbols
    parse_and_bind_parallel_with_libs(files, &lib_contexts)
}

/// Parse and bind multiple files in parallel with lib contexts.
///
/// Lib symbols are injected into each file's binder during binding,
/// enabling resolution of global symbols like `console`, `Array`, etc.
///
/// # Arguments
/// * `files` - Vector of (file_name, source_text) pairs
/// * `lib_files` - Lib files to merge into each binder
///
/// # Returns
/// Vector of BindResult for each file
pub fn parse_and_bind_parallel_with_libs(
    files: Vec<(String, String)>,
    lib_files: &[Arc<lib_loader::LibFile>],
) -> Vec<BindResult> {
    maybe_parallel_into!(files)
        .map(|(file_name, source_text)| {
            // Parse
            let mut parser = ParserState::new(file_name.clone(), source_text);
            let source_file = parser.parse_source_file();

            let (arena, parse_diagnostics) = parser.into_parts();

            // Bind with lib symbols
            let mut binder = BinderState::new();

            // IMPORTANT: Merge lib symbols BEFORE binding source file
            // so that symbols like console, Array, Promise are available during binding
            if !lib_files.is_empty() {
                binder.merge_lib_symbols(lib_files);
            }

            binder.bind_source_file(&arena, source_file);

            // Extract lib_binders from binder before it's moved
            let lib_binders = binder.lib_binders.clone();

            BindResult {
                file_name,
                source_file,
                arena: Arc::new(arena),
                symbols: binder.symbols,
                file_locals: binder.file_locals,
                declared_modules: binder.declared_modules,
                node_symbols: binder.node_symbols,
                scopes: binder.scopes,
                node_scope_ids: binder.node_scope_ids,
                parse_diagnostics,
                shorthand_ambient_modules: binder.shorthand_ambient_modules,
                global_augmentations: binder.global_augmentations,
                reexports: binder.reexports,
                wildcard_reexports: binder.wildcard_reexports,
                lib_binders,
                flow_nodes: binder.flow_nodes,
                node_flow: binder.node_flow,
            }
        })
        .collect()
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
    /// Persistent scopes (symbol IDs are global after merge)
    pub scopes: Vec<Scope>,
    /// Map from AST node to scope ID
    pub node_scope_ids: FxHashMap<u32, ScopeId>,
    /// Parse diagnostics
    pub parse_diagnostics: Vec<ParseDiagnostic>,
    /// Global augmentations (interface declarations inside `declare global` blocks)
    pub global_augmentations: FxHashMap<String, Vec<NodeIndex>>,
    /// Flow nodes for control flow analysis
    pub flow_nodes: FlowNodeArena,
    /// Node-to-flow mapping: tracks which flow node was active at each AST node
    pub node_flow: FxHashMap<u32, FlowNodeId>,
}

use crate::solver::TypeInterner;

/// Merged program state after parallel binding
pub struct MergedProgram {
    /// All bound files
    pub files: Vec<BoundFile>,
    /// Global symbol arena (all symbols from all files, with remapped IDs)
    pub symbols: SymbolArena,
    /// Symbol-to-arena mapping for declaration lookup (legacy, stores last arena)
    pub symbol_arenas: FxHashMap<SymbolId, Arc<NodeArena>>,
    /// Declaration-to-arena mapping for precise cross-file declaration lookup
    /// Key: (SymbolId, NodeIndex of declaration) -> Arena containing that declaration
    pub declaration_arenas: FxHashMap<(SymbolId, NodeIndex), Arc<NodeArena>>,
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
    /// Maps (current_file, exported_name) -> (source_module, original_name)
    pub reexports: FxHashMap<String, FxHashMap<String, (String, Option<String>)>>,
    /// Wildcard re-exports: tracks `export * from 'module'` declarations
    /// Maps current_file -> Vec of source_modules
    pub wildcard_reexports: FxHashMap<String, Vec<String>>,
    /// Lib binders for global type resolution (Array, String, Promise, etc.)
    /// These contain symbols from lib.d.ts files and enable resolution of built-in types
    pub lib_binders: Vec<Arc<BinderState>>,
    /// Global type interner - shared across all threads for type deduplication
    pub type_interner: TypeInterner,
}

/// Check if two symbols can be merged across multiple files.
///
/// TypeScript allows merging:
/// - Interface + Interface (declaration merging)
/// - Namespace + Namespace (declaration merging)
/// - Class + Interface (merging for class declarations)
/// - Function + Function (overloads - handled per-file)
fn can_merge_symbols_cross_file(existing_flags: u32, new_flags: u32) -> bool {
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

    // Namespace/module can merge with namespace/module
    if (existing_flags & symbol_flags::MODULE) != 0 && (new_flags & symbol_flags::MODULE) != 0 {
        return true;
    }

    // Namespace can merge with class, function, or enum
    if (existing_flags & symbol_flags::MODULE) != 0
        && (new_flags & (symbol_flags::CLASS | symbol_flags::FUNCTION | symbol_flags::ENUM)) != 0
    {
        return true;
    }
    if (new_flags & symbol_flags::MODULE) != 0
        && (existing_flags & (symbol_flags::CLASS | symbol_flags::FUNCTION | symbol_flags::ENUM))
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

/// Merge bind results into a unified program state
///
/// This is a sequential operation that combines:
/// - All symbol arenas into a single global arena
/// - Merges symbols with the same name across files (for interfaces, namespaces, etc.)
/// - Remaps symbol IDs in node_symbols to use global IDs
///
/// # Arguments
/// * `results` - Vector of BindResult from parallel binding
///
/// # Returns
/// MergedProgram with unified symbol space
pub fn merge_bind_results(results: Vec<BindResult>) -> MergedProgram {
    let refs: Vec<&BindResult> = results.iter().collect();
    merge_bind_results_ref(&refs)
}

pub fn merge_bind_results_ref(results: &[&BindResult]) -> MergedProgram {
    // Collect lib_binders from all results (deduplicated by address)
    let mut lib_binders: Vec<Arc<BinderState>> = Vec::new();
    let mut lib_binder_set: FxHashSet<usize> = FxHashSet::default();
    for result in results {
        for lib_binder in &result.lib_binders {
            let binder_addr = Arc::as_ptr(lib_binder) as usize;
            if lib_binder_set.insert(binder_addr) {
                lib_binders.push(Arc::clone(lib_binder));
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
    let declaration_arenas: FxHashMap<(SymbolId, NodeIndex), Arc<NodeArena>> = FxHashMap::default();
    let mut globals = SymbolTable::new();
    let mut files = Vec::with_capacity(results.len());
    let mut file_locals_list = Vec::with_capacity(results.len());
    let mut declared_modules = FxHashSet::default();
    let mut shorthand_ambient_modules = FxHashSet::default();
    let mut module_exports: FxHashMap<String, SymbolTable> = FxHashMap::default();
    let mut reexports: FxHashMap<String, FxHashMap<String, (String, Option<String>)>> =
        FxHashMap::default();
    let mut wildcard_reexports: FxHashMap<String, Vec<String>> = FxHashMap::default();

    // Track which symbols have been merged to avoid duplicate processing
    let mut merged_symbols: FxHashMap<String, SymbolId> = FxHashMap::default();

    // ==========================================================================
    // PHASE 1: Remap lib symbols to global arena
    // ==========================================================================
    // This creates a mapping from (lib_binder_ptr, local_id) -> global_id
    // so that file_locals can reference lib symbols using global IDs
    let mut lib_symbol_remap: FxHashMap<(usize, SymbolId), SymbolId> = FxHashMap::default();

    for lib_binder in &lib_binders {
        let lib_binder_ptr = Arc::as_ptr(lib_binder) as usize;

        // Process all symbols in this lib binder
        for i in 0..lib_binder.symbols.len() {
            let local_id = SymbolId(i as u32);
            if let Some(lib_sym) = lib_binder.symbols.get(local_id) {
                // Check if a symbol with this name already exists (cross-lib merging)
                let global_id =
                    if let Some(&existing_id) = merged_symbols.get(&lib_sym.escaped_name) {
                        // Symbol already exists - check if we can merge
                        if let Some(existing_sym) = global_symbols.get(existing_id) {
                            if can_merge_symbols_cross_file(existing_sym.flags, lib_sym.flags) {
                                // Merge: reuse existing symbol ID
                                // Merge declarations from this lib
                                if let Some(existing_mut) = global_symbols.get_mut(existing_id) {
                                    existing_mut.flags |= lib_sym.flags;
                                    for decl in &lib_sym.declarations {
                                        if !existing_mut.declarations.contains(decl) {
                                            existing_mut.declarations.push(*decl);
                                        }
                                    }
                                }
                                existing_id
                            } else {
                                // Cannot merge - allocate new (shadowing)
                                let new_id = global_symbols.alloc_from(lib_sym);
                                merged_symbols.insert(lib_sym.escaped_name.clone(), new_id);
                                new_id
                            }
                        } else {
                            // Shouldn't happen - allocate new
                            let new_id = global_symbols.alloc_from(lib_sym);
                            merged_symbols.insert(lib_sym.escaped_name.clone(), new_id);
                            new_id
                        }
                    } else {
                        // New symbol - allocate in global arena
                        let new_id = global_symbols.alloc_from(lib_sym);
                        merged_symbols.insert(lib_sym.escaped_name.clone(), new_id);
                        new_id
                    };

                // Store the remapping
                lib_symbol_remap.insert((lib_binder_ptr, local_id), global_id);
            }
        }
    }

    // Also remap lib file_locals entries that reference symbols by name
    // (for exported lib symbols like Array, Object, console)
    let mut lib_name_to_global: FxHashMap<String, SymbolId> = FxHashMap::default();
    for lib_binder in &lib_binders {
        let lib_binder_ptr = Arc::as_ptr(lib_binder) as usize;
        for (name, &local_id) in lib_binder.file_locals.iter() {
            if let Some(&global_id) = lib_symbol_remap.get(&(lib_binder_ptr, local_id)) {
                // Only keep the first mapping for each name (lib files are processed in order)
                lib_name_to_global.entry(name.clone()).or_insert(global_id);
            }
        }
    }

    // ==========================================================================
    // PHASE 2: Process user files
    // ==========================================================================

    for result in results {
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
            for source_module in source_modules {
                if !entry.contains(source_module) {
                    entry.push(source_module.clone());
                }
            }
        }
        // Copy symbols from this file to global arena, getting new IDs
        let mut id_remap: FxHashMap<SymbolId, SymbolId> = FxHashMap::default();
        for i in 0..result.symbols.len() {
            let old_id = SymbolId(i as u32);
            if let Some(sym) = result.symbols.get(old_id) {
                // Check if symbol already exists in globals (cross-file merging)
                let new_id = if let Some(&existing_id) = merged_symbols.get(&sym.escaped_name) {
                    // Symbol exists - check if we can merge
                    if let Some(existing_sym) = global_symbols.get(existing_id) {
                        // Check if symbols can merge (interface+interface, namespace+namespace, etc.)
                        if can_merge_symbols_cross_file(existing_sym.flags, sym.flags) {
                            // Merge: reuse existing symbol ID, will merge declarations below
                            existing_id
                        } else {
                            // Cannot merge - allocate new symbol (shadowing or duplicate)
                            let new_id = global_symbols.alloc(sym.flags, sym.escaped_name.clone());
                            symbol_arenas.insert(new_id, Arc::clone(&result.arena));
                            merged_symbols.insert(sym.escaped_name.clone(), new_id);
                            new_id
                        }
                    } else {
                        // Shouldn't happen - allocate new
                        let new_id = global_symbols.alloc(sym.flags, sym.escaped_name.clone());
                        symbol_arenas.insert(new_id, Arc::clone(&result.arena));
                        merged_symbols.insert(sym.escaped_name.clone(), new_id);
                        new_id
                    }
                } else {
                    // New symbol - allocate
                    let new_id = global_symbols.alloc(sym.flags, sym.escaped_name.clone());
                    symbol_arenas.insert(new_id, Arc::clone(&result.arena));
                    merged_symbols.insert(sym.escaped_name.clone(), new_id);
                    new_id
                };
                id_remap.insert(old_id, new_id);
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

        // 1) Named exports collected from file_locals.
        for (name, &sym_id) in result.file_locals.iter() {
            if let Some(sym) = result.symbols.get(sym_id)
                && sym.is_exported
                && let Some(&remapped_id) = id_remap.get(&sym_id)
            {
                exports.set(name.clone(), remapped_id);
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

        if !exports.is_empty() {
            module_exports.insert(result.file_name.clone(), exports);
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

        for (old_id, &new_id) in id_remap.iter() {
            let Some(old_sym) = result.symbols.get(*old_id) else {
                continue;
            };
            if let Some(new_sym) = global_symbols.get_mut(new_id) {
                // Check if this is a cross-file merge (same symbol already has data)
                let is_cross_file_merge = !new_sym.declarations.is_empty()
                    && new_sym.declarations != old_sym.declarations;

                if is_cross_file_merge {
                    // Cross-file merge: append declarations and merge flags
                    new_sym.flags |= old_sym.flags;
                    // Append new declarations from this file
                    for decl in &old_sym.declarations {
                        if !new_sym.declarations.contains(decl) {
                            new_sym.declarations.push(*decl);
                        }
                    }
                    // Update value_declaration if the old one was NONE
                    if new_sym.value_declaration.is_none() && !old_sym.value_declaration.is_none() {
                        new_sym.value_declaration = old_sym.value_declaration;
                    }
                    // Merge exports (if both have exports)
                    if let (Some(old_exports), Some(new_exports)) =
                        (old_sym.exports.as_ref(), new_sym.exports.as_mut())
                    {
                        for (name, sym_id) in old_exports.iter() {
                            if !new_exports.has(name) {
                                // Remap the symbol ID and add to exports
                                if let Some(&remapped_id) = id_remap.get(sym_id) {
                                    new_exports.set(name.clone(), remapped_id);
                                }
                            }
                        }
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
        }

        // Remap node_symbols to use global IDs
        // Note: node_symbols primarily maps user file nodes to user symbols,
        // but lib symbols referenced in user code need remapping too
        let mut remapped_node_symbols = FxHashMap::default();
        for (node_idx, old_sym_id) in result.node_symbols.iter() {
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
                globals.set(name.clone(), new_sym_id);
            } else if let Some(&global_id) = lib_name_to_global.get(name) {
                // Lib symbol - use the pre-remapped global ID
                // Only add to file_locals, NOT to globals (lib symbols are accessed
                // through lib_contexts in the checker, not through globals)
                remapped_file_locals.set(name.clone(), global_id);
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

        files.push(BoundFile {
            file_name: result.file_name.clone(),
            source_file: result.source_file,
            arena: Arc::clone(&result.arena),
            node_symbols: remapped_node_symbols,
            scopes: remapped_scopes,
            node_scope_ids: result.node_scope_ids.clone(),
            parse_diagnostics: result.parse_diagnostics.clone(),
            global_augmentations: result.global_augmentations.clone(),
            flow_nodes: result.flow_nodes.clone(),
            node_flow: result.node_flow.clone(),
        });
    }

    // NOTE: We intentionally do NOT populate globals from merged_symbols here.
    // merged_symbols contains ALL symbols (including namespace-local ones like `var Symbol`
    // inside a namespace), but globals should only contain file-level symbols.
    // File-level symbols are already correctly added to globals at lines 873-880.
    //
    // NOTE: lib_binders were collected and processed at the beginning of this function.
    // Their symbols have been remapped to global IDs and are now in:
    // - global_symbols: The actual Symbol data
    // - lib_name_to_global: Name -> global SymbolId mapping
    // - Each file's remapped_file_locals and globals

    MergedProgram {
        files,
        symbols: global_symbols,
        symbol_arenas,
        declaration_arenas,
        globals,
        file_locals: file_locals_list,
        declared_modules,
        shorthand_ambient_modules,
        module_exports,
        reexports,
        wildcard_reexports,
        lib_binders,
        type_interner: TypeInterner::new(),
    }
}

/// Full pipeline: Parse → Bind (parallel) → Merge (sequential)
///
/// This is the main entry point for multi-file compilation.
/// Lib files are automatically loaded and merged during binding.
pub fn compile_files(files: Vec<(String, String)>) -> MergedProgram {
    let lib_files = resolve_default_lib_files(ScriptTarget::ESNext).unwrap_or_default();
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
use crate::checker::state::CheckerState;
use crate::checker::types::diagnostics::Diagnostic;
use crate::lib_loader::LibFile;
use crate::parser::syntax_kind_ext;
use crate::solver::TypeId;

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

    let Some(node) = arena.get(source_file) else {
        return functions;
    };

    let Some(sf) = arena.get_source_file(node) else {
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
/// CheckResult with diagnostics from all functions
pub fn check_functions_parallel(program: &MergedProgram) -> CheckResult {
    // First, collect all functions from all files (sequential)
    let mut all_functions: Vec<(usize, NodeIndex)> = Vec::new();

    for (file_idx, file) in program.files.iter().enumerate() {
        let functions = collect_functions(&file.arena, file.source_file);
        for func_idx in functions {
            all_functions.push((file_idx, func_idx));
        }
    }

    let function_count = all_functions.len();

    // Create a shared QueryCache for memoized evaluate_type/is_subtype_of calls.
    let query_cache = crate::solver::QueryCache::new(&program.type_interner);

    // Check functions in parallel
    // Note: We need to be careful here - CheckerState holds mutable references
    // For now, we group by file and check each file's functions together
    let file_results: Vec<FileCheckResult> = maybe_parallel_iter!(program.files)
        .enumerate()
        .map(|(file_idx, file)| {
            let functions = collect_functions(&file.arena, file.source_file);

            // Create a binder state from the node_symbols
            let binder = create_binder_from_bound_file(file, program, file_idx);

            // Create checker for this file, using the shared type interner
            let compiler_options = crate::checker::context::CheckerOptions::default();
            let mut checker = CheckerState::new(
                &file.arena,
                &binder,
                &query_cache,
                file.file_name.clone(),
                compiler_options, // default options for internal operations
            );

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

    // Create a shared QueryCache for memoized evaluate_type/is_subtype_of calls.
    // This is thread-safe (uses RwLock internally) and shared across all file checks.
    let query_cache = crate::solver::QueryCache::new(&program.type_interner);

    let file_results: Vec<FileCheckResult> = maybe_parallel_iter!(program.files)
        .enumerate()
        .map(|(file_idx, file)| {
            let binder = create_binder_from_bound_file(file, program, file_idx);

            let mut checker = CheckerState::with_options(
                &file.arena,
                &binder,
                &query_cache,
                file.file_name.clone(),
                checker_options,
            );

            if !lib_contexts.is_empty() {
                checker.ctx.set_lib_contexts(lib_contexts.clone());
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

/// Create a BinderState from a BoundFile for type checking
pub(crate) fn create_binder_from_bound_file(
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

    let mut binder = BinderState::from_bound_state_with_scopes_and_augmentations(
        BinderOptions::default(),
        program.symbols.clone(),
        file_locals,
        file.node_symbols.clone(),
        file.scopes.clone(),
        file.node_scope_ids.clone(),
        file.global_augmentations.clone(),
        program.module_exports.clone(),
        program.reexports.clone(),
        program.wildcard_reexports.clone(),
        program.symbol_arenas.clone(),
        program.declaration_arenas.clone(),
        program.shorthand_ambient_modules.clone(),
        file.flow_nodes.clone(),
        file.node_flow.clone(),
    );

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
#[path = "tests/parallel_tests.rs"]
mod tests;
