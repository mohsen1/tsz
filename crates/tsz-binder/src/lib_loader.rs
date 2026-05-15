//! `LibLoader` - Load and merge `lib.d.ts` symbols into the binder.
//!
//! This module provides types and utilities for working with TypeScript library files.
//! Lib files (like lib.d.ts, lib.es2015.d.ts) must be provided externally - either
//! loaded from disk (CLI) or via `addLibFile()` in WASM.

use crate::BinderState;
use crate::SymbolTable;
use rustc_hash::FxHashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tsz_common::diagnostics::Diagnostic;
use tsz_common::lib_capabilities;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::state::ParserState;

// =============================================================================
// LibLoader - Cached lib file loading from disk
// =============================================================================

/// A lib file loader that caches loaded files.
///
/// Used by tsz-server to keep lib files in memory across multiple checks.
pub struct LibLoader {
    /// Directory containing lib.*.d.ts files
    lib_dir: PathBuf,
    /// Cache of loaded lib file contents
    cache: FxHashMap<String, String>,
}

impl LibLoader {
    /// Create a new `LibLoader` with the given lib directory.
    #[must_use]
    pub fn new(lib_dir: PathBuf) -> Self {
        Self {
            lib_dir,
            cache: FxHashMap::default(),
        }
    }

    /// Load a lib file by name (e.g., "es5", "es2015", "dom").
    ///
    /// Returns the file content, or None if not found.
    pub fn load_lib(&mut self, lib_name: &str) -> Option<&str> {
        let normalized = lib_name.trim().to_lowercase();

        // Check cache first.
        if self.cache.contains_key(&normalized) {
            return self.cache.get(&normalized).map(std::string::String::as_str);
        }

        // Try to load from disk
        let candidates = [
            self.lib_dir.join(format!("lib.{normalized}.d.ts")),
            self.lib_dir.join(format!("{normalized}.d.ts")),
        ];

        for candidate in &candidates {
            if candidate.exists()
                && let Ok(content) = std::fs::read_to_string(candidate)
            {
                self.cache.insert(normalized.clone(), content);
                return self.cache.get(&normalized).map(std::string::String::as_str);
            }
        }

        None
    }

    /// Clear the cache.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Get the number of cached lib files.
    #[must_use]
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }
}

// =============================================================================
// Diagnostic Error Codes
// =============================================================================

/// TS2318: Cannot find global type '{0}'.
pub const CANNOT_FIND_GLOBAL_TYPE: u32 = 2318;

/// TS2583: Cannot find name '{0}'. Do you need to change your target library?
pub const MISSING_ES2015_LIB_SUPPORT: u32 = 2583;

// =============================================================================
// LibFile
// =============================================================================

/// Loaded lib file with its arena and binder state.
#[derive(Clone)]
pub struct LibFile {
    /// File name (e.g., "lib.d.ts")
    pub file_name: String,
    /// The arena (shared via Arc for cross-file resolution)
    pub arena: Arc<NodeArena>,
    /// The binder state with bound symbols
    pub binder: Arc<BinderState>,
    /// Root source file node index (needed for re-binding from existing arena)
    pub root_index: NodeIndex,
}

impl LibFile {
    /// Create a new `LibFile` from a parsed and bound lib file.
    #[must_use]
    pub const fn new(
        file_name: String,
        arena: Arc<NodeArena>,
        binder: Arc<BinderState>,
        root_index: NodeIndex,
    ) -> Self {
        Self {
            file_name,
            arena,
            binder,
            root_index,
        }
    }

    /// Get the file locals (global symbols) from this lib file.
    #[must_use]
    pub fn file_locals(&self) -> &SymbolTable {
        &self.binder.file_locals
    }

    /// Parse and bind a lib file from source content.
    ///
    /// This is the standard way to create a `LibFile` from `.d.ts` content.
    #[must_use]
    pub fn from_source(file_name: String, content: String) -> Self {
        let mut parser = ParserState::new(file_name.clone(), content);
        let source_file_idx = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), source_file_idx);

        let arena = Arc::new(parser.into_arena());
        let binder = Arc::new(binder);

        Self::new(file_name, arena, binder, source_file_idx)
    }
}

/// Merge multiple lib files into a single lib file with a unified binder.
///
/// This function takes multiple `LibFile` instances (each with its own `BinderState`)
/// and merges them into a single `LibFile` where all symbols are in one binder
/// with remapped `SymbolIds` to avoid collisions.
#[must_use]
#[allow(dead_code)]
pub(crate) fn merge_lib_files(lib_files: Vec<Arc<LibFile>>) -> Vec<Arc<LibFile>> {
    use crate::state::LibContext as BinderLibContext;

    if lib_files.is_empty() {
        return lib_files;
    }

    // Create a single merged binder from all lib files.
    let mut merged_binder = BinderState::new();
    let lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| BinderLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();

    merged_binder.merge_lib_contexts_into_binder(&lib_contexts);

    let merged_arena = match lib_files.first() {
        Some(lib_file) => Arc::clone(&lib_file.arena),
        None => return lib_files,
    };
    let merged_binder = Arc::new(merged_binder);
    let merged_root = lib_files.first().map_or(NodeIndex(0), |f| f.root_index);
    let merged_file = Arc::new(LibFile::new(
        "merged-libs".to_string(),
        merged_arena,
        merged_binder,
        merged_root,
    ));

    vec![merged_file]
}

// =============================================================================
// Error Emission Helpers
// =============================================================================

/// Create a TS2318 error for a missing global type.
#[must_use]
pub fn emit_error_global_type_missing(
    name: &str,
    file_name: String,
    start: u32,
    length: u32,
) -> Diagnostic {
    Diagnostic::error(
        file_name,
        start,
        length,
        format!("Cannot find global type '{name}'."),
        CANNOT_FIND_GLOBAL_TYPE,
    )
}

/// Create a TS2583 error when a type is missing due to insufficient lib support.
#[must_use]
pub fn emit_error_lib_target_mismatch(
    name: &str,
    file_name: String,
    start: u32,
    length: u32,
) -> Diagnostic {
    Diagnostic::error(
        file_name,
        start,
        length,
        format!(
            "Cannot find name '{name}'. Do you need to change your target library? Try changing the 'lib' compiler option to es2015 or later."
        ),
        MISSING_ES2015_LIB_SUPPORT,
    )
}

/// Check if a type name is an ES2015+ feature that requires specific lib support.
#[must_use]
pub fn is_es2015_plus_type(name: &str) -> bool {
    lib_capabilities::is_known_es_type(name)
}

/// Check if a value-position name should emit TS2583 ("change your target
/// library") instead of plain TS2304 when not found.
///
/// Mirrors tsc's `getCannotFindNameDiagnosticForName` switch (checker.ts):
/// only this narrow set of well-known ES2015+ globals get the lib-upgrade
/// suggestion in value position. Other ES2015+ types (e.g. `Proxy`,
/// `Generator`, `IterableIterator`, `WeakRef`) fall through to TS2304.
#[must_use]
pub fn is_es2015_plus_value_lib_suggestion(name: &str) -> bool {
    lib_capabilities::is_known_value_lib_suggestion(name)
}

/// Get the minimum lib version required for an ES2015+ type name.
///
/// Returns the lib version string (e.g., "es2015", "es2017") that first defines
/// the given type. Returns "es2015" as default for most types.
#[must_use]
pub fn get_suggested_lib_for_type(name: &str) -> &'static str {
    lib_capabilities::suggested_lib_for_type(name).map_or("es2015", |lib| lib.as_str())
}

#[cfg(test)]
#[path = "../tests/lib_loader.rs"]
mod tests;
