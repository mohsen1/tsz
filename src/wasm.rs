//! WASM Integration Module
//!
//! This module provides the WebAssembly interface for the TypeScript compiler.
//! It exposes parallel type checking capabilities and the TypeInterner for
//! concurrent access from JavaScript.
//!
//! # Architecture
//!
//! The WASM module uses wasm-bindgen to expose Rust types to JavaScript.
//! Key types exposed:
//!
//! - `WasmTypeInterner`: Thread-safe type interning with lock-free concurrent access
//! - `WasmParallelChecker`: Parallel type checking using rayon's work-stealing scheduler
//! - `WasmProgram`: Multi-file compilation with parallel binding and checking
//!
//! # Concurrency Model
//!
//! The TypeInterner uses a sharded DashMap architecture that enables true
//! parallel type checking:
//!
//! - 64 shards distribute type storage to minimize contention
//! - Lock-free reads and writes via DashMap
//! - Atomic counters for ID allocation
//! - Arc<T> for safe sharing across threads
//!
//! # Usage from JavaScript
//!
//! ```javascript
//! import { WasmProgram, WasmTypeInterner } from 'tsz-wasm';
//!
//! // Create a program
//! const program = new WasmProgram();
//! program.addFile("a.ts", "export const x: number = 1;");
//! program.addFile("b.ts", "import { x } from './a'; const y = x + 1;");
//!
//! // Compile and check in parallel
//! const result = program.checkAll();
//! console.log(result);
//! ```

use crate::parallel::{
    BindStats, CheckStats, ParallelStats, compile_files,
};
use crate::solver::TypeInterner;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// WASM-compatible type interner for parallel type checking.
///
/// This wraps the internal TypeInterner with a wasm-bindgen compatible interface.
/// The underlying interner uses lock-free DashMap storage for concurrent access.
#[wasm_bindgen]
pub struct WasmTypeInterner {
    inner: TypeInterner,
}

#[wasm_bindgen]
impl WasmTypeInterner {
    /// Create a new type interner with pre-registered intrinsics.
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmTypeInterner {
        WasmTypeInterner {
            inner: TypeInterner::new(),
        }
    }

    /// Get the number of interned types.
    #[wasm_bindgen(js_name = getTypeCount)]
    pub fn get_type_count(&self) -> usize {
        self.inner.len()
    }

    /// Check if the interner is empty (only has intrinsics).
    #[wasm_bindgen(js_name = isEmpty)]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Intern a string and return its atom ID.
    #[wasm_bindgen(js_name = internString)]
    pub fn intern_string(&self, s: &str) -> u32 {
        self.inner.intern_string(s).0
    }

    /// Resolve an atom ID back to its string value.
    #[wasm_bindgen(js_name = resolveAtom)]
    pub fn resolve_atom(&self, atom_id: u32) -> String {
        self.inner.resolve_atom(crate::interner::Atom(atom_id))
    }
}

impl Default for WasmTypeInterner {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics from parallel parsing.
#[derive(Serialize, Deserialize)]
pub struct WasmParseStats {
    pub file_count: usize,
    pub total_bytes: usize,
    pub total_nodes: usize,
    pub error_count: usize,
}

impl From<ParallelStats> for WasmParseStats {
    fn from(stats: ParallelStats) -> Self {
        WasmParseStats {
            file_count: stats.file_count,
            total_bytes: stats.total_bytes,
            total_nodes: stats.total_nodes,
            error_count: stats.error_count,
        }
    }
}

/// Statistics from parallel binding.
#[derive(Serialize, Deserialize)]
pub struct WasmBindStats {
    pub file_count: usize,
    pub total_nodes: usize,
    pub total_symbols: usize,
    pub parse_error_count: usize,
}

impl From<BindStats> for WasmBindStats {
    fn from(stats: BindStats) -> Self {
        WasmBindStats {
            file_count: stats.file_count,
            total_nodes: stats.total_nodes,
            total_symbols: stats.total_symbols,
            parse_error_count: stats.parse_error_count,
        }
    }
}

/// Statistics from parallel type checking.
#[derive(Serialize, Deserialize)]
pub struct WasmCheckStats {
    pub file_count: usize,
    pub function_count: usize,
    pub diagnostic_count: usize,
}

impl From<CheckStats> for WasmCheckStats {
    fn from(stats: CheckStats) -> Self {
        WasmCheckStats {
            file_count: stats.file_count,
            function_count: stats.function_count,
            diagnostic_count: stats.diagnostic_count,
        }
    }
}

/// Parallel file parser using rayon's work-stealing scheduler.
///
/// This provides high-performance parallel parsing of multiple TypeScript files.
/// Each file is parsed independently, producing its own AST arena.
#[wasm_bindgen]
pub struct WasmParallelParser {
    files: Vec<(String, String)>,
}

#[wasm_bindgen]
impl WasmParallelParser {
    /// Create a new parallel parser.
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmParallelParser {
        WasmParallelParser { files: Vec::new() }
    }

    /// Add a file to be parsed.
    #[wasm_bindgen(js_name = addFile)]
    pub fn add_file(&mut self, file_name: String, source_text: String) {
        self.files.push((file_name, source_text));
    }

    /// Get the number of files added.
    #[wasm_bindgen(js_name = getFileCount)]
    pub fn get_file_count(&self) -> usize {
        self.files.len()
    }

    /// Parse all files in parallel and return statistics.
    #[wasm_bindgen(js_name = parseAll)]
    pub fn parse_all(&mut self) -> JsValue {
        let files = std::mem::take(&mut self.files);
        let (_results, stats) = crate::parallel::parse_files_with_stats(files);

        let wasm_stats = WasmParseStats::from(stats);
        serde_wasm_bindgen::to_value(&wasm_stats).unwrap_or(JsValue::NULL)
    }

    /// Clear all files.
    #[wasm_bindgen]
    pub fn clear(&mut self) {
        self.files.clear();
    }
}

impl Default for WasmParallelParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of parallel type checking exposed to JavaScript.
#[derive(Serialize, Deserialize)]
pub struct WasmCheckResult {
    pub stats: WasmCheckStats,
    pub diagnostics: Vec<WasmDiagnostic>,
}

/// A diagnostic message exposed to JavaScript.
#[derive(Serialize, Deserialize)]
pub struct WasmDiagnostic {
    pub file: String,
    pub start: u32,
    pub length: u32,
    pub code: u32,
    pub message: String,
    pub category: String,
}

/// Parallel type checker using the shared TypeInterner.
///
/// This enables parallel type checking of function bodies across multiple files
/// while sharing a single type interner for deduplication.
#[wasm_bindgen]
pub struct WasmParallelChecker {
    files: Vec<(String, String)>,
}

#[wasm_bindgen]
impl WasmParallelChecker {
    /// Create a new parallel checker.
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmParallelChecker {
        WasmParallelChecker { files: Vec::new() }
    }

    /// Add a file to be checked.
    #[wasm_bindgen(js_name = addFile)]
    pub fn add_file(&mut self, file_name: String, source_text: String) {
        self.files.push((file_name, source_text));
    }

    /// Get the number of files added.
    #[wasm_bindgen(js_name = getFileCount)]
    pub fn get_file_count(&self) -> usize {
        self.files.len()
    }

    /// Compile and check all files in parallel.
    ///
    /// This performs:
    /// 1. Parallel parsing of all files
    /// 2. Parallel binding to create symbols
    /// 3. Sequential merging of symbol tables
    /// 4. Parallel type checking of function bodies
    #[wasm_bindgen(js_name = checkAll)]
    pub fn check_all(&mut self) -> JsValue {
        let files = std::mem::take(&mut self.files);

        // Compile files (parse, bind, merge)
        let program = compile_files(files);

        // Check in parallel
        let (result, stats) = crate::parallel::check_functions_with_stats(&program);

        // Convert diagnostics
        let mut diagnostics = Vec::new();
        for file_result in result.file_results {
            for diag in file_result.diagnostics {
                diagnostics.push(WasmDiagnostic {
                    file: file_result.file_name.clone(),
                    start: diag.start,
                    length: diag.length,
                    code: diag.code,
                    message: diag.message_text.clone(),
                    category: format!("{:?}", diag.category),
                });
            }
        }

        let wasm_result = WasmCheckResult {
            stats: WasmCheckStats::from(stats),
            diagnostics,
        };

        serde_wasm_bindgen::to_value(&wasm_result).unwrap_or(JsValue::NULL)
    }

    /// Clear all files.
    #[wasm_bindgen]
    pub fn clear(&mut self) {
        self.files.clear();
    }
}

impl Default for WasmParallelChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// Tests for the native (non-wasm) interface.
/// The wasm-specific tests are skipped on non-wasm targets because wasm-bindgen
/// functions cannot be called on non-wasm targets.
#[cfg(test)]
mod tests {
    use crate::solver::TypeInterner;

    #[test]
    fn test_type_interner_basic() {
        use crate::solver::TypeId;

        // Test the underlying TypeInterner directly (works on all targets)
        let interner = TypeInterner::new();

        // Should start empty (no user-defined types, only intrinsics)
        assert!(interner.is_empty());
        let initial_count = interner.len();
        assert_eq!(
            initial_count,
            TypeId::FIRST_USER as usize,
            "TypeInterner should have intrinsics"
        );

        // Intern a string
        let atom1 = interner.intern_string("hello");
        let atom2 = interner.intern_string("hello");
        assert_eq!(atom1, atom2); // Deduplication

        // Resolve the string
        let resolved = interner.resolve_atom(atom1);
        assert_eq!(resolved, "hello");

        // Intern a literal type - this should make it non-empty
        let str_type = interner.literal_string("test");
        assert!(!interner.is_empty());
        assert!(interner.len() > initial_count);
    }

    #[test]
    fn test_parallel_parsing() {
        // Test the parallel parsing directly (works on all targets)
        let files = vec![
            ("a.ts".to_string(), "let x = 1;".to_string()),
            ("b.ts".to_string(), "let y = 2;".to_string()),
        ];

        let results = crate::parallel::parse_files_parallel(files);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_parallel_compile_and_check() {
        // Test the full pipeline directly (works on all targets)
        let files = vec![
            (
                "a.ts".to_string(),
                "function add(x: number, y: number): number { return x + y; }".to_string(),
            ),
            (
                "b.ts".to_string(),
                "function mul(x: number, y: number): number { return x * y; }".to_string(),
            ),
        ];

        let program = crate::parallel::compile_files(files);
        assert_eq!(program.files.len(), 2);

        let (result, stats) = crate::parallel::check_functions_with_stats(&program);
        assert_eq!(stats.file_count, 2);
        assert!(stats.function_count >= 2);
    }
}
