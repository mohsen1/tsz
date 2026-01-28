//! Lib Loader - Load and merge lib.d.ts symbols into the binder.
//!
//! This module provides types and utilities for working with TypeScript library files.
//! Lib files (like lib.d.ts, lib.es2015.d.ts) must be provided externally - either
//! loaded from disk (CLI) or via `addLibFile()` in WASM.

use crate::binder::BinderState;
use crate::binder::SymbolTable;
use crate::checker::types::diagnostics::Diagnostic;
use crate::parser::ParserState;
use crate::parser::node::NodeArena;
use rustc_hash::FxHashMap;
use std::path::PathBuf;
use std::sync::Arc;

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
    /// Create a new LibLoader with the given lib directory.
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

        // Check cache first
        if self.cache.contains_key(&normalized) {
            return self.cache.get(&normalized).map(|s| s.as_str());
        }

        // Try to load from disk
        let candidates = [
            self.lib_dir.join(format!("lib.{}.d.ts", normalized)),
            self.lib_dir.join(format!("{}.d.ts", normalized)),
        ];

        for candidate in &candidates {
            if candidate.exists() {
                if let Ok(content) = std::fs::read_to_string(candidate) {
                    self.cache.insert(normalized.clone(), content);
                    return self.cache.get(&normalized).map(|s| s.as_str());
                }
            }
        }

        None
    }

    /// Clear the cache.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Get the number of cached lib files.
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
}

impl LibFile {
    /// Create a new LibFile from a parsed and bound lib file.
    pub fn new(file_name: String, arena: Arc<NodeArena>, binder: Arc<BinderState>) -> Self {
        Self {
            file_name,
            arena,
            binder,
        }
    }

    /// Get the file locals (global symbols) from this lib file.
    pub fn file_locals(&self) -> &SymbolTable {
        &self.binder.file_locals
    }

    /// Parse and bind a lib file from source content.
    ///
    /// This is the standard way to create a LibFile from .d.ts content.
    pub fn from_source(file_name: String, content: String) -> Self {
        let mut parser = ParserState::new(file_name.clone(), content);
        let source_file_idx = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), source_file_idx);

        let arena = Arc::new(parser.into_arena());
        let binder = Arc::new(binder);

        Self::new(file_name, arena, binder)
    }
}

/// Merge multiple lib files into a single lib file with a unified binder.
///
/// This function takes multiple LibFile instances (each with its own BinderState)
/// and merges them into a single LibFile where all symbols are in one binder
/// with remapped SymbolIds to avoid collisions.
pub fn merge_lib_files(lib_files: Vec<Arc<LibFile>>) -> Vec<Arc<LibFile>> {
    use crate::binder::state::LibContext as BinderLibContext;

    if lib_files.is_empty() {
        return lib_files;
    }

    // Create a single merged binder from all lib files
    let mut merged_binder = BinderState::new();
    let lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| BinderLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();

    merged_binder.merge_lib_contexts_into_binder(&lib_contexts);

    // Create a single merged LibFile
    let merged_arena = lib_files.first().map(|lib| Arc::clone(&lib.arena)).unwrap();
    let merged_binder = Arc::new(merged_binder);
    let merged_file = Arc::new(LibFile::new(
        "merged-libs".to_string(),
        merged_arena,
        merged_binder,
    ));

    vec![merged_file]
}

/// Merge lib file symbols into a target symbol table.
///
/// **DEPRECATED**: This function copies raw SymbolIds from lib binders, which can
/// collide across different lib files. Use `BinderState::merge_lib_contexts_into_binder`
/// instead, which properly remaps SymbolIds to avoid collisions.
#[deprecated(note = "Use BinderState::merge_lib_contexts_into_binder instead")]
pub fn merge_lib_symbols(target: &mut SymbolTable, lib_files: &[Arc<LibFile>]) {
    for lib in lib_files {
        for (name, sym_id) in lib.binder.file_locals.iter() {
            if !target.has(name) {
                target.set(name.clone(), *sym_id);
            }
        }
    }
}

// =============================================================================
// Error Emission Helpers
// =============================================================================

/// Create a TS2318 error for a missing global type.
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
        format!("Cannot find global type '{}'.", name),
        CANNOT_FIND_GLOBAL_TYPE,
    )
}

/// Create a TS2583 error when a type is missing due to insufficient lib support.
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
            "Cannot find name '{}'. Do you need to change your target library? Try changing the 'lib' compiler option to es2015 or later.",
            name
        ),
        MISSING_ES2015_LIB_SUPPORT,
    )
}

// =============================================================================
// ES2015+ Type Detection
// =============================================================================

/// ES2015+ global types that may not be available in all lib versions.
const ES2015_PLUS_TYPES: &[&str] = &[
    "Promise", "PromiseLike", "PromiseConstructor", "PromiseConstructorLike",
    "PromiseSettledResult", "PromiseFulfilledResult", "PromiseRejectedResult",
    "Map", "MapConstructor", "Set", "SetConstructor",
    "WeakMap", "WeakMapConstructor", "WeakSet", "WeakSetConstructor",
    "Proxy", "ProxyHandler", "ProxyConstructor", "Reflect",
    "Symbol", "SymbolConstructor",
    "Iterator", "Iterable", "IterableIterator", "IteratorResult",
    "IteratorYieldResult", "IteratorReturnResult",
    "AsyncIterator", "AsyncIterable", "AsyncIterableIterator",
    "Generator", "GeneratorFunction", "GeneratorFunctionConstructor",
    "ArrayLike", "ReadonlyMap", "ReadonlySet", "TemplateStringsArray",
    "TypedPropertyDescriptor", "CallableFunction", "NewableFunction", "PropertyKey",
    "AsyncFunction", "AsyncFunctionConstructor",
    "SharedArrayBuffer", "SharedArrayBufferConstructor", "Atomics",
    "AsyncGenerator", "AsyncGeneratorFunction", "AsyncGeneratorFunctionConstructor",
    "ObjectEntries", "ObjectValues",
    "BigInt", "BigIntConstructor", "BigInt64Array", "BigInt64ArrayConstructor",
    "BigUint64Array", "BigUint64ArrayConstructor",
    "FinalizationRegistry", "FinalizationRegistryConstructor",
    "WeakRef", "WeakRefConstructor", "AggregateError", "AggregateErrorConstructor",
    "Awaited", "ErrorOptions", "Disposable", "AsyncDisposable",
    "DataView", "Int8Array", "Uint8Array", "Uint8ClampedArray",
    "Int16Array", "Uint16Array", "Int32Array", "Uint32Array",
    "Float32Array", "Float64Array", "ArrayBuffer", "ArrayBufferConstructor",
    "TypedArray",
];

/// Check if a type name is an ES2015+ feature that requires specific lib support.
pub fn is_es2015_plus_type(name: &str) -> bool {
    ES2015_PLUS_TYPES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binder::{SymbolArena, symbol_flags};

    #[test]
    fn test_lib_file_from_source() {
        let content = r#"
declare var console: { log(msg: string): void };
declare class Array<T> { length: number; }
"#;
        let lib = LibFile::from_source("test-lib.d.ts".to_string(), content.to_string());
        assert!(lib.file_locals().has("console"));
        assert!(lib.file_locals().has("Array"));
    }

    #[test]
    fn test_merge_lib_symbols() {
        let mut target = SymbolTable::new();
        let mut arena = SymbolArena::new();
        let object_id = arena.alloc(symbol_flags::VALUE, "Object".to_string());
        let function_id = arena.alloc(symbol_flags::VALUE, "Function".to_string());
        let console_id = arena.alloc(symbol_flags::VALUE, "console".to_string());

        let mut lib_file_locals = SymbolTable::new();
        lib_file_locals.set("Object".to_string(), object_id);
        lib_file_locals.set("Function".to_string(), function_id);
        lib_file_locals.set("console".to_string(), console_id);

        let lib_binder = BinderState::from_bound_state(arena, lib_file_locals, Default::default());
        let lib = Arc::new(LibFile::new(
            "lib.d.ts".to_string(),
            Arc::new(NodeArena::new()),
            Arc::new(lib_binder),
        ));

        let mut user_arena = SymbolArena::new();
        let user_object_id = user_arena.alloc(symbol_flags::VALUE, "Object".to_string());
        target.set("Object".to_string(), user_object_id);

        #[allow(deprecated)]
        merge_lib_symbols(&mut target, &[lib]);

        assert_eq!(target.get("Object"), Some(user_object_id));
        assert_eq!(target.get("Function"), Some(function_id));
        assert_eq!(target.get("console"), Some(console_id));
    }

    #[test]
    fn test_is_es2015_plus_type() {
        assert!(is_es2015_plus_type("Promise"));
        assert!(is_es2015_plus_type("Map"));
        assert!(is_es2015_plus_type("Set"));
        assert!(is_es2015_plus_type("Symbol"));
        assert!(is_es2015_plus_type("BigInt"));
        assert!(!is_es2015_plus_type("Object"));
        assert!(!is_es2015_plus_type("Array"));
        assert!(!is_es2015_plus_type("Function"));
        assert!(!is_es2015_plus_type("Date"));
        assert!(!is_es2015_plus_type("RegExp"));
    }
}
