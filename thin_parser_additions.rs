// This file contains the code additions for ThinParser in src/lib.rs
// These should be manually integrated into the main lib.rs file

// ============================================================================
// SECTION 1: Additional imports (add to existing imports around line 155)
// ============================================================================

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// ============================================================================
// SECTION 2: CompilerOptions struct (add before ImportCandidateInput)
// ============================================================================

/// Compiler options for TypeScript compilation.
/// Controls type checking behavior, target output, and module system.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompilerOptions {
    /// Enable all strict type checking options.
    #[serde(default)]
    pub strict: bool,
    /// Raise error on expressions and declarations with an implied 'any' type.
    #[serde(default)]
    pub no_implicit_any: bool,
    /// Enable strict null checks.
    #[serde(default)]
    pub strict_null_checks: bool,
    /// Enable strict checking of function types.
    #[serde(default)]
    pub strict_function_types: bool,
    /// Specify ECMAScript target version (e.g., "ES5", "ES2015", "ESNext").
    #[serde(default)]
    pub target: String,
    /// Specify module code generation (e.g., "CommonJS", "ES2015", "ESNext").
    #[serde(default)]
    pub module: String,
}

// ============================================================================
// SECTION 3: ThinParser struct fields (add to the end of ThinParser struct)
// ============================================================================

    /// Compiler options for controlling compilation behavior
    compiler_options: Option<CompilerOptions>,
    /// Set of lib file IDs that have been marked as lib files
    lib_file_ids: HashSet<u32>,

// ============================================================================
// SECTION 4: Constructor initialization (add to ThinParser::new)
// ============================================================================

            compiler_options: None,
            lib_file_ids: HashSet::new(),

// ============================================================================
// SECTION 5: New methods (add after add_lib_file method in ThinParser impl)
// ============================================================================

    /// Set compiler options from a JSON string.
    /// The JSON string should match the CompilerOptions struct format.
    #[wasm_bindgen(js_name = setCompilerOptions)]
    pub fn set_compiler_options(&mut self, json: String) -> Result<(), JsValue> {
        let options: CompilerOptions = serde_json::from_str(&json)
            .map_err(|e| JsValue::from_str(&format!("Failed to parse compiler options: {}", e)))?;
        self.compiler_options = Some(options);
        Ok(())
    }

    /// Mark a file ID as a lib file.
    /// This is used to track which files are lib files (e.g., lib.d.ts, lib.es5.d.ts).
    #[wasm_bindgen(js_name = markAsLibFile)]
    pub fn mark_as_lib_file(&mut self, file_id: u32) {
        self.lib_file_ids.insert(file_id);
    }
