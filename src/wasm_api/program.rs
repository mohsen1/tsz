//! TypeScript Program API
//!
//! Provides the `TsProgram` struct which implements TypeScript's Program interface.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use wasm_bindgen::prelude::*;

use crate::binder::BinderState;
use crate::checker::context::CheckerOptions;
use crate::checker::types::DiagnosticCategory;
use crate::lib_loader::LibFile;
use crate::parallel::{
    MergedProgram, check_files_parallel, merge_bind_results, parse_and_bind_parallel,
    parse_and_bind_parallel_with_libs,
};
use crate::parser::ParserState;
use crate::solver::TypeInterner;

use super::source_file::TsSourceFile;
use super::type_checker::TsTypeChecker;

/// Compiler options passed from JavaScript
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TsCompilerOptions {
    #[serde(default)]
    pub strict: Option<bool>,
    #[serde(default)]
    pub no_implicit_any: Option<bool>,
    #[serde(default)]
    pub strict_null_checks: Option<bool>,
    #[serde(default)]
    pub strict_function_types: Option<bool>,
    #[serde(default)]
    pub strict_property_initialization: Option<bool>,
    #[serde(default)]
    pub no_implicit_returns: Option<bool>,
    #[serde(default)]
    pub no_implicit_this: Option<bool>,
    #[serde(default)]
    pub target: Option<u8>,
    #[serde(default)]
    pub module: Option<u8>,
    #[serde(default)]
    pub lib: Option<String>,
    #[serde(default)]
    pub no_lib: Option<bool>,
    #[serde(default)]
    pub declaration: Option<bool>,
    #[serde(default)]
    pub source_map: Option<bool>,
    #[serde(default)]
    pub out_dir: Option<String>,
    #[serde(default)]
    pub root_dir: Option<String>,
    #[serde(default)]
    pub jsx: Option<u8>,
    #[serde(default)]
    pub allow_js: Option<bool>,
    #[serde(default)]
    pub check_js: Option<bool>,
}

impl TsCompilerOptions {
    /// Convert to internal CheckerOptions
    pub fn to_checker_options(&self) -> CheckerOptions {
        let strict = self.strict.unwrap_or(false);
        CheckerOptions {
            strict,
            no_implicit_any: self.no_implicit_any.unwrap_or(strict),
            no_implicit_returns: self.no_implicit_returns.unwrap_or(false),
            strict_null_checks: self.strict_null_checks.unwrap_or(strict),
            strict_function_types: self.strict_function_types.unwrap_or(strict),
            strict_property_initialization: self.strict_property_initialization.unwrap_or(strict),
            no_implicit_this: self.no_implicit_this.unwrap_or(strict),
            use_unknown_in_catch_variables: self.strict_null_checks.unwrap_or(strict),
            isolated_modules: false,
            no_unchecked_indexed_access: false,
            strict_bind_call_apply: false,
            exact_optional_property_types: false,
            no_lib: self.no_lib.unwrap_or(false),
            target: crate::checker::context::ScriptTarget::default(),
            es_module_interop: false,
            allow_synthetic_default_imports: false,
            allow_unreachable_code: false,
            no_property_access_from_index_signature: false,
            sound_mode: false,
            experimental_decorators: false,
            no_unused_locals: false,
            no_unused_parameters: false,
        }
    }
}

/// TypeScript Program - the main compilation unit
///
/// Represents a compiled TypeScript program with access to:
/// - Source files
/// - Diagnostics (syntactic and semantic)
/// - Type checker
/// - Emit functionality
#[wasm_bindgen]
pub struct TsProgram {
    /// Files added to the program (file_name, source_text)
    files: Vec<(String, String)>,
    /// Library files (lib.d.ts, etc.)
    lib_files: Vec<Arc<LibFile>>,
    /// Merged program state (contains bound files with parse diagnostics)
    merged: Option<MergedProgram>,
    /// Type interner for this program
    type_interner: TypeInterner,
    /// Compiler options
    options: TsCompilerOptions,
    /// Cached type checker
    #[allow(dead_code)]
    type_checker: Option<TsTypeChecker>,
    /// Cached source files
    #[allow(dead_code)]
    source_files: Vec<TsSourceFile>,
}

#[wasm_bindgen]
impl TsProgram {
    /// Create a new empty program
    #[wasm_bindgen(constructor)]
    pub fn new() -> TsProgram {
        TsProgram {
            files: Vec::new(),
            lib_files: Vec::new(),
            merged: None,
            type_interner: TypeInterner::new(),
            options: TsCompilerOptions::default(),
            type_checker: None,
            source_files: Vec::new(),
        }
    }

    /// Set compiler options from JSON
    #[wasm_bindgen(js_name = setCompilerOptions)]
    pub fn set_compiler_options(&mut self, options_json: &str) -> Result<(), JsValue> {
        match serde_json::from_str::<TsCompilerOptions>(options_json) {
            Ok(options) => {
                self.options = options;
                // Invalidate caches
                self.merged = None;
                self.type_checker = None;
                Ok(())
            }
            Err(e) => Err(JsValue::from_str(&format!(
                "Failed to parse options: {}",
                e
            ))),
        }
    }

    /// Add a source file to the program
    #[wasm_bindgen(js_name = addSourceFile)]
    pub fn add_source_file(&mut self, file_name: String, source_text: String) {
        // Invalidate caches
        self.merged = None;
        self.type_checker = None;
        self.source_files.clear();

        self.files.push((file_name, source_text));
    }

    /// Add a library file (lib.d.ts, lib.es5.d.ts, etc.)
    #[wasm_bindgen(js_name = addLibFile)]
    pub fn add_lib_file(&mut self, file_name: String, source_text: String) {
        // Invalidate caches
        self.merged = None;
        self.type_checker = None;

        // Parse and bind the lib file
        let mut parser = ParserState::new(file_name.clone(), source_text);
        let root_idx = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root_idx);

        let lib_file = Arc::new(LibFile::new(
            file_name,
            Arc::new(parser.into_arena()),
            Arc::new(binder),
        ));

        self.lib_files.push(lib_file);
    }

    /// Get the number of source files
    #[wasm_bindgen(js_name = getSourceFileCount)]
    pub fn get_source_file_count(&self) -> usize {
        self.files.len()
    }

    /// Get source file names as JSON array
    #[wasm_bindgen(js_name = getRootFileNames)]
    pub fn get_root_file_names(&self) -> JsValue {
        let names: Vec<&str> = self.files.iter().map(|(name, _)| name.as_str()).collect();
        serde_wasm_bindgen::to_value(&names).unwrap_or(JsValue::NULL)
    }

    /// Ensure the program is compiled (parse, bind, merge)
    fn ensure_compiled(&mut self) {
        if self.merged.is_some() {
            return;
        }

        // Determine which lib files to use
        // Libs must be provided externally via addLibFile() - no embedded lib fallback
        let lib_files_to_use = if self.options.no_lib == Some(true) {
            vec![]
        } else {
            self.lib_files.clone()
        };

        // Parse and bind all files
        let bind_results = if !lib_files_to_use.is_empty() {
            parse_and_bind_parallel_with_libs(self.files.clone(), &lib_files_to_use)
        } else {
            parse_and_bind_parallel(self.files.clone())
        };

        // Merge results
        let merged = merge_bind_results(bind_results);

        self.merged = Some(merged);
    }

    /// Get syntactic diagnostics as JSON
    #[wasm_bindgen(js_name = getSyntacticDiagnosticsJson)]
    pub fn get_syntactic_diagnostics_json(&mut self, file_name: Option<String>) -> String {
        self.ensure_compiled();

        let merged = match &self.merged {
            Some(m) => m,
            None => return "[]".to_string(),
        };

        let mut diagnostics: Vec<serde_json::Value> = Vec::new();

        for bound_file in &merged.files {
            // Filter by file if specified
            if let Some(ref name) = file_name {
                if &bound_file.file_name != name {
                    continue;
                }
            }

            for diag in &bound_file.parse_diagnostics {
                diagnostics.push(serde_json::json!({
                    "file": bound_file.file_name,
                    "start": diag.start,
                    "length": diag.length,
                    "messageText": diag.message,
                    "category": 1, // Error
                    "code": diag.code,
                }));
            }
        }

        serde_json::to_string(&diagnostics).unwrap_or_else(|_| "[]".to_string())
    }

    /// Get semantic diagnostics as JSON
    #[wasm_bindgen(js_name = getSemanticDiagnosticsJson)]
    pub fn get_semantic_diagnostics_json(&mut self, file_name: Option<String>) -> String {
        self.ensure_compiled();

        let merged = match &self.merged {
            Some(m) => m,
            None => return "[]".to_string(),
        };

        let checker_options = self.options.to_checker_options();
        let check_result = check_files_parallel(merged, &checker_options, &self.lib_files);

        let mut diagnostics: Vec<serde_json::Value> = Vec::new();

        for file_result in &check_result.file_results {
            // Filter by file if specified
            if let Some(ref name) = file_name {
                if &file_result.file_name != name {
                    continue;
                }
            }

            for diag in &file_result.diagnostics {
                diagnostics.push(serde_json::json!({
                    "file": file_result.file_name,
                    "start": diag.start,
                    "length": diag.length,
                    "messageText": diag.message_text,
                    "category": match diag.category {
                        DiagnosticCategory::Error => 1,
                        DiagnosticCategory::Warning => 0,
                        DiagnosticCategory::Suggestion => 2,
                        DiagnosticCategory::Message => 3,
                    },
                    "code": diag.code,
                }));
            }
        }

        serde_json::to_string(&diagnostics).unwrap_or_else(|_| "[]".to_string())
    }

    /// Get all diagnostics (syntactic + semantic) as JSON
    #[wasm_bindgen(js_name = getPreEmitDiagnosticsJson)]
    pub fn get_pre_emit_diagnostics_json(&mut self) -> String {
        self.ensure_compiled();

        let mut all_diagnostics: Vec<serde_json::Value> = Vec::new();

        // Syntactic diagnostics
        let syntactic: Vec<serde_json::Value> =
            serde_json::from_str(&self.get_syntactic_diagnostics_json(None)).unwrap_or_default();
        all_diagnostics.extend(syntactic);

        // Semantic diagnostics
        let semantic: Vec<serde_json::Value> =
            serde_json::from_str(&self.get_semantic_diagnostics_json(None)).unwrap_or_default();
        all_diagnostics.extend(semantic);

        serde_json::to_string(&all_diagnostics).unwrap_or_else(|_| "[]".to_string())
    }

    /// Get all diagnostic codes as array
    #[wasm_bindgen(js_name = getAllDiagnosticCodes)]
    pub fn get_all_diagnostic_codes(&mut self) -> Vec<u32> {
        self.ensure_compiled();

        let mut codes: Vec<u32> = Vec::new();

        if let Some(ref merged) = self.merged {
            // Parse diagnostics (from bound files)
            for bound_file in &merged.files {
                for diag in &bound_file.parse_diagnostics {
                    codes.push(diag.code);
                }
            }

            // Check diagnostics
            let checker_options = self.options.to_checker_options();
            let check_result = check_files_parallel(merged, &checker_options, &self.lib_files);
            for file_result in &check_result.file_results {
                for diag in &file_result.diagnostics {
                    codes.push(diag.code);
                }
            }
        }

        codes
    }

    /// Get the type checker
    ///
    /// Returns a handle to the type checker for this program.
    /// The type checker provides type information and type-related operations.
    #[wasm_bindgen(js_name = getTypeChecker)]
    pub fn get_type_checker(&mut self) -> TsTypeChecker {
        self.ensure_compiled();

        // Create new type checker
        // In a full implementation, we'd cache this
        TsTypeChecker::new(
            self.merged.as_ref().unwrap(),
            &self.type_interner,
            &self.options,
            &self.lib_files,
        )
    }

    /// Emit JavaScript output for all files
    #[wasm_bindgen(js_name = emitJson)]
    pub fn emit_json(&mut self) -> String {
        self.ensure_compiled();

        let merged = match &self.merged {
            Some(m) => m,
            None => {
                return serde_json::json!({
                    "emitSkipped": true,
                    "diagnostics": [],
                    "emittedFiles": []
                })
                .to_string();
            }
        };

        let mut emitted_files: Vec<serde_json::Value> = Vec::new();

        // Determine target and module from options
        let target = match self.options.target.unwrap_or(1) {
            0 => crate::emitter::ScriptTarget::ES3,
            1 => crate::emitter::ScriptTarget::ES5,
            2 => crate::emitter::ScriptTarget::ES2015,
            99 => crate::emitter::ScriptTarget::ESNext,
            _ => crate::emitter::ScriptTarget::ES5,
        };

        let module = match self.options.module.unwrap_or(0) {
            0 => crate::emitter::ModuleKind::None,
            1 => crate::emitter::ModuleKind::CommonJS,
            6 => crate::emitter::ModuleKind::ES2015,
            99 => crate::emitter::ModuleKind::ESNext,
            _ => crate::emitter::ModuleKind::None,
        };

        // Emit each file
        for (idx, bound_file) in merged.files.iter().enumerate() {
            // Get source text from our files list
            let source_text = if idx < self.files.len() {
                &self.files[idx].1
            } else {
                ""
            };

            let output = super::emit::emit_file(
                &bound_file.arena,
                bound_file.source_file,
                source_text,
                target,
                module,
            );

            // Create output file name (.ts -> .js)
            let output_name = bound_file
                .file_name
                .strip_suffix(".ts")
                .or_else(|| bound_file.file_name.strip_suffix(".tsx"))
                .map(|s| format!("{}.js", s))
                .unwrap_or_else(|| format!("{}.js", bound_file.file_name));

            emitted_files.push(serde_json::json!({
                "name": output_name,
                "text": output,
                "declaration": false,
                "sourceMap": false,
            }));
        }

        serde_json::json!({
            "emitSkipped": false,
            "diagnostics": [],
            "emittedFiles": emitted_files
        })
        .to_string()
    }

    /// Emit a single file by name
    #[wasm_bindgen(js_name = emitFile)]
    pub fn emit_file(&mut self, file_name: &str) -> String {
        self.ensure_compiled();

        let merged = match &self.merged {
            Some(m) => m,
            None => return String::new(),
        };

        // Find the file
        for (idx, bound_file) in merged.files.iter().enumerate() {
            if bound_file.file_name == file_name {
                let source_text = if idx < self.files.len() {
                    &self.files[idx].1
                } else {
                    ""
                };

                let target = match self.options.target.unwrap_or(1) {
                    0 => crate::emitter::ScriptTarget::ES3,
                    1 => crate::emitter::ScriptTarget::ES5,
                    2 => crate::emitter::ScriptTarget::ES2015,
                    99 => crate::emitter::ScriptTarget::ESNext,
                    _ => crate::emitter::ScriptTarget::ES5,
                };

                let module = match self.options.module.unwrap_or(0) {
                    0 => crate::emitter::ModuleKind::None,
                    1 => crate::emitter::ModuleKind::CommonJS,
                    6 => crate::emitter::ModuleKind::ES2015,
                    99 => crate::emitter::ModuleKind::ESNext,
                    _ => crate::emitter::ModuleKind::None,
                };

                return super::emit::emit_file(
                    &bound_file.arena,
                    bound_file.source_file,
                    source_text,
                    target,
                    module,
                );
            }
        }

        String::new()
    }

    /// Get compiler options as JSON
    #[wasm_bindgen(js_name = getCompilerOptionsJson)]
    pub fn get_compiler_options_json(&self) -> String {
        serde_json::to_string(&self.options).unwrap_or_else(|_| "{}".to_string())
    }

    /// Clean up resources
    #[wasm_bindgen]
    pub fn dispose(&mut self) {
        self.files.clear();
        self.lib_files.clear();
        self.merged = None;
        self.type_checker = None;
        self.source_files.clear();
    }
}

impl Default for TsProgram {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a TypeScript program (factory function)
///
/// This is the main entry point for creating a program.
///
/// # Arguments
/// * `root_names_json` - JSON array of file names
/// * `options_json` - JSON object with compiler options
/// * `files_json` - JSON object mapping file names to content
///
/// # Returns
/// A new TsProgram instance
#[wasm_bindgen(js_name = createTsProgram)]
pub fn create_ts_program(
    root_names_json: &str,
    options_json: &str,
    files_json: &str,
) -> Result<TsProgram, JsValue> {
    // Parse inputs
    let root_names: Vec<String> = serde_json::from_str(root_names_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid root names: {}", e)))?;

    let files: std::collections::HashMap<String, String> = serde_json::from_str(files_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid files: {}", e)))?;

    // Create program
    let mut program = TsProgram::new();

    // Set options
    program.set_compiler_options(options_json)?;

    // Add files
    for name in root_names {
        if let Some(content) = files.get(&name) {
            program.add_source_file(name, content.clone());
        }
    }

    Ok(program)
}
