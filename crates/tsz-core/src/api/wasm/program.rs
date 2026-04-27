use std::sync::Arc;

use rustc_hash::FxHashMap;
use wasm_bindgen::prelude::{JsValue, wasm_bindgen};

use crate::api::wasm::compiler_options::{CompilerOptions, parse_compiler_options_json};
use crate::api::wasm::lib_cache::get_or_create_lib_file;
use crate::api::wasm::program_results::{
    CheckDiagnosticJson, FileCheckResultJson, ParseDiagnosticJson,
};
use crate::lib_loader;
use crate::parallel::{
    BindResult, MergedProgram, check_files_parallel, merge_bind_results, parse_and_bind_parallel,
};

/// Multi-file TypeScript program for cross-file type checking.
///
/// This struct provides an API for compiling multiple TypeScript files together,
/// enabling proper module resolution and cross-file type checking.
///
/// # Example (JavaScript)
/// ```javascript
/// const program = new WasmProgram();
/// program.addFile("a.ts", "export const x = 1;");
/// program.addFile("b.ts", "import { x } from './a'; const y = x + 1;");
/// const result = program.checkAll();
/// console.log(result);
/// ```
#[wasm_bindgen]
pub struct WasmProgram {
    /// Accumulated files before compilation
    files: Vec<(String, String)>,
    /// Merged program state after compilation (lazy)
    merged: Option<MergedProgram>,
    /// Bind results (kept for diagnostics access)
    bind_results: Option<Vec<BindResult>>,
    /// Lib files (lib.d.ts, lib.dom.d.ts, etc.) for global symbol resolution
    lib_files: Vec<(String, String)>,
    /// Compiler options for type checking
    compiler_options: CompilerOptions,
    /// Cached output of `checkAll()`, populated lazily on first call.
    /// Invalidated by `addFile` / `addLibFile` / `setCompilerOptions` / `clear`.
    check_all_cache: Option<String>,
    /// Cached output of `getDiagnosticCodes()`, populated lazily on first call.
    diagnostic_codes_cache: Option<String>,
    /// Cached output of `getAllDiagnosticCodes()`, populated lazily on first call.
    all_diagnostic_codes_cache: Option<Vec<u32>>,
}

impl Default for WasmProgram {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl WasmProgram {
    /// Create a new empty program.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            lib_files: Vec::new(),
            merged: None,
            bind_results: None,
            compiler_options: CompilerOptions::default(),
            check_all_cache: None,
            diagnostic_codes_cache: None,
            all_diagnostic_codes_cache: None,
        }
    }

    /// Drop all cached diagnostic outputs. Call from any mutator that
    /// changes program inputs (`addFile`, `addLibFile`, `setCompilerOptions`,
    /// `clear`) so the next diagnostic query rebuilds from fresh sources.
    fn invalidate_diagnostic_caches(&mut self) {
        self.check_all_cache = None;
        self.diagnostic_codes_cache = None;
        self.all_diagnostic_codes_cache = None;
    }

    /// Add a file to the program.
    ///
    /// Files are accumulated and compiled together when `checkAll` is called.
    /// The `file_name` should be a relative path like "src/a.ts".
    ///
    /// For TypeScript library files (lib.d.ts, lib.dom.d.ts, etc.), use `addLibFile` instead.
    #[wasm_bindgen(js_name = addFile)]
    pub fn add_file(&mut self, file_name: String, source_text: String) {
        // Invalidate any previous compilation
        self.merged = None;
        self.bind_results = None;
        self.invalidate_diagnostic_caches();

        // Skip package.json files - they're used for module resolution but not parsed
        if file_name.ends_with("package.json") {
            return;
        }

        self.files.push((file_name, source_text));
    }

    /// Add a TypeScript library file (lib.d.ts, lib.dom.d.ts, etc.) to the program.
    ///
    /// Lib files are used for global symbol resolution and are merged into
    /// the symbol table before user files are processed.
    ///
    /// Use this method explicitly instead of relying on automatic file name detection.
    /// This makes the API behavior predictable and explicit.
    ///
    /// # Example (JavaScript)
    /// ```javascript
    /// const program = new WasmProgram();
    /// program.addLibFile("lib.d.ts", libContent);
    /// program.addFile("src/a.ts", userCode);
    /// ```
    #[wasm_bindgen(js_name = addLibFile)]
    pub fn add_lib_file(&mut self, file_name: String, source_text: String) {
        // Invalidate any previous compilation
        self.merged = None;
        self.bind_results = None;
        self.invalidate_diagnostic_caches();

        self.lib_files.push((file_name, source_text));
    }

    /// Set compiler options from JSON.
    ///
    /// # Arguments
    /// * `options_json` - JSON string containing compiler options
    #[wasm_bindgen(js_name = setCompilerOptions)]
    pub fn set_compiler_options(&mut self, options_json: &str) -> Result<(), JsValue> {
        let options = parse_compiler_options_json(options_json)?;
        self.compiler_options = options;
        // Invalidate any previous compilation since options affect typing
        self.merged = None;
        self.bind_results = None;
        self.invalidate_diagnostic_caches();
        Ok(())
    }

    /// Get the number of files in the program.
    #[allow(clippy::missing_const_for_fn)] // wasm_bindgen does not support const fn
    #[wasm_bindgen(js_name = getFileCount)]
    pub fn get_file_count(&self) -> usize {
        self.files.len()
    }

    /// Clear all files and reset the program state.
    #[wasm_bindgen]
    pub fn clear(&mut self) {
        self.files.clear();
        self.lib_files.clear();
        self.merged = None;
        self.bind_results = None;
        self.invalidate_diagnostic_caches();
    }

    /// Compile all files and return diagnostics as JSON.
    ///
    /// This performs:
    /// 1. Load lib files for global symbol resolution
    /// 2. Parallel parsing of all files
    /// 3. Parallel binding of all files with lib symbols merged
    /// 4. Symbol merging (sequential)
    /// 5. Parallel type checking
    ///
    /// Returns a JSON object with diagnostics per file.
    #[wasm_bindgen(js_name = checkAll)]
    pub fn check_all(&mut self) -> String {
        if self.files.is_empty() && self.lib_files.is_empty() {
            return r#"{"files":[],"stats":{"totalFiles":0,"totalDiagnostics":0}}"#.to_string();
        }

        // Reuse cached output when the program hasn't been mutated since the
        // last call. The previous behavior was to re-run the full lib-load +
        // parse + bind + merge + check pipeline on every diagnostic call,
        // even when the inputs were unchanged. Conformance harnesses, the
        // playground, and any caller asking for diagnostics in more than one
        // form (e.g. JSON for display PLUS codes for comparison) paid for
        // the entire pipeline twice or three times per program revision.
        if let Some(cached) = self.check_all_cache.as_ref() {
            return cached.clone();
        }

        // Load lib files for binding
        // Use cache to avoid re-parsing lib.d.ts for every test
        let lib_file_objects: Vec<Arc<lib_loader::LibFile>> = self
            .lib_files
            .iter()
            .map(|(file_name, source_text)| {
                get_or_create_lib_file(file_name.clone(), source_text.clone())
            })
            .collect();

        // Parse and bind all files in parallel with lib symbols
        let bind_results = if !lib_file_objects.is_empty() {
            // Use lib-aware binding
            use crate::parallel;
            parallel::parse_and_bind_parallel_with_libs(self.files.clone(), &lib_file_objects)
        } else {
            // No lib files - use regular binding
            parse_and_bind_parallel(self.files.clone())
        };

        // Collect parse diagnostics before merging
        let parse_diags: Vec<Vec<_>> = bind_results
            .iter()
            .map(|r| r.parse_diagnostics.clone())
            .collect();
        let file_names: Vec<String> = bind_results.iter().map(|r| r.file_name.clone()).collect();

        // Merge bind results into unified program
        let merged = merge_bind_results(bind_results);

        // Type check all files in parallel
        let checker_options = self.compiler_options.to_checker_options();
        let check_result = check_files_parallel(&merged, &checker_options, &lib_file_objects);

        // Build JSON result.
        //
        // Build an O(1) file-name -> check_result index up front instead of
        // doing a linear `file_results.iter().find(...)` per file. The
        // previous pattern was O(N²) in `file_names.len()` and lived on the
        // result-assembly path AFTER checking had completed, so it was
        // entirely avoidable scaling cost. On a 6000-file project that's
        // ~36M comparisons; on small projects it's a no-op-cheap helper.
        let check_results_by_file: FxHashMap<&str, &_> = check_result
            .file_results
            .iter()
            .map(|r| (r.file_name.as_str(), r))
            .collect();

        let mut file_results: Vec<FileCheckResultJson> = Vec::with_capacity(file_names.len());
        let mut total_diagnostics = 0;

        for (i, file_name) in file_names.iter().enumerate() {
            let parse_diagnostics: Vec<ParseDiagnosticJson> = parse_diags[i]
                .iter()
                .map(|d| ParseDiagnosticJson {
                    message: d.message.clone(),
                    start: d.start,
                    length: d.length,
                    code: d.code,
                })
                .collect();

            let check_diagnostics: Vec<CheckDiagnosticJson> = check_results_by_file
                .get(file_name.as_str())
                .map(|r| {
                    r.diagnostics
                        .iter()
                        .map(|d| CheckDiagnosticJson {
                            message_text: d.message_text.clone(),
                            code: d.code,
                            start: d.start,
                            length: d.length,
                            category: format!("{:?}", d.category),
                        })
                        .collect()
                })
                .unwrap_or_default();

            total_diagnostics += parse_diagnostics.len() + check_diagnostics.len();

            file_results.push(FileCheckResultJson {
                file_name: file_name.clone(),
                parse_diagnostics,
                check_diagnostics,
            });
        }

        // Store merged program for potential future queries
        self.merged = Some(merged);

        let result = serde_json::json!({
            "files": file_results,
            "stats": {
                "totalFiles": file_names.len(),
                "totalDiagnostics": total_diagnostics,
            }
        });

        let serialized = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
        self.check_all_cache = Some(serialized.clone());
        serialized
    }

    /// Get diagnostic codes for all files (for conformance testing).
    ///
    /// Returns a JSON object mapping file names to arrays of error codes.
    #[wasm_bindgen(js_name = getDiagnosticCodes)]
    pub fn get_diagnostic_codes(&mut self) -> String {
        if self.files.is_empty() && self.lib_files.is_empty() {
            return "{}".to_string();
        }

        if let Some(cached) = self.diagnostic_codes_cache.as_ref() {
            return cached.clone();
        }

        // Load lib files for binding (enables global symbol resolution: console, Array, etc.)
        // Use cache to avoid re-parsing lib.d.ts for every test
        let lib_file_objects: Vec<Arc<lib_loader::LibFile>> = self
            .lib_files
            .iter()
            .map(|(file_name, source_text)| {
                get_or_create_lib_file(file_name.clone(), source_text.clone())
            })
            .collect();

        // Parse and bind all files in parallel with lib symbols
        let bind_results = if !lib_file_objects.is_empty() {
            use crate::parallel;
            parallel::parse_and_bind_parallel_with_libs(self.files.clone(), &lib_file_objects)
        } else {
            parse_and_bind_parallel(self.files.clone())
        };

        // Collect parse diagnostic codes
        let mut file_codes: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        for result in &bind_results {
            let codes: Vec<u32> = result.parse_diagnostics.iter().map(|d| d.code).collect();
            file_codes.insert(result.file_name.clone(), codes);
        }

        // Merge and check
        let merged = merge_bind_results(bind_results);
        let checker_options = self.compiler_options.to_checker_options();
        let check_result = check_files_parallel(&merged, &checker_options, &lib_file_objects);

        // Add check diagnostic codes
        for file_result in &check_result.file_results {
            let entry = file_codes.entry(file_result.file_name.clone()).or_default();
            for diag in &file_result.diagnostics {
                entry.push(diag.code);
            }
        }

        // Store merged program
        self.merged = Some(merged);

        let serialized = serde_json::to_string(&file_codes).unwrap_or_else(|_| "{}".to_string());
        self.diagnostic_codes_cache = Some(serialized.clone());
        serialized
    }

    /// Get all diagnostic codes as a flat array (for simple conformance comparison).
    ///
    /// This combines all parse and check diagnostics from all files into a single
    /// array of error codes, which can be compared against tsc output.
    #[wasm_bindgen(js_name = getAllDiagnosticCodes)]
    pub fn get_all_diagnostic_codes(&mut self) -> Vec<u32> {
        if self.files.is_empty() && self.lib_files.is_empty() {
            return Vec::new();
        }

        if let Some(cached) = self.all_diagnostic_codes_cache.as_ref() {
            return cached.clone();
        }

        // Load lib files for binding (enables global symbol resolution: console, Array, etc.)
        // Use cache to avoid re-parsing lib.d.ts for every test
        let lib_file_objects: Vec<Arc<lib_loader::LibFile>> = self
            .lib_files
            .iter()
            .map(|(file_name, source_text)| {
                get_or_create_lib_file(file_name.clone(), source_text.clone())
            })
            .collect();

        // Parse and bind all files in parallel with lib symbols
        let bind_results = if !lib_file_objects.is_empty() {
            use crate::parallel;
            parallel::parse_and_bind_parallel_with_libs(self.files.clone(), &lib_file_objects)
        } else {
            parse_and_bind_parallel(self.files.clone())
        };

        // Collect all parse diagnostic codes
        let mut all_codes: Vec<u32> = Vec::new();
        for result in &bind_results {
            for diag in &result.parse_diagnostics {
                all_codes.push(diag.code);
            }
        }

        // Merge and check
        let merged = merge_bind_results(bind_results);
        let checker_options = self.compiler_options.to_checker_options();
        let check_result = check_files_parallel(&merged, &checker_options, &lib_file_objects);

        // Add all check diagnostic codes
        for file_result in &check_result.file_results {
            for diag in &file_result.diagnostics {
                all_codes.push(diag.code);
            }
        }

        // Store merged program
        self.merged = Some(merged);

        self.all_diagnostic_codes_cache = Some(all_codes.clone());
        all_codes
    }
}
