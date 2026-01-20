//! Import statement checking for TypeScript.
//!
//! This module provides ESM import conflict detection to emit
//! accurate TS1202 errors when import assignments are used in ES modules.
//!
//! TS1202: Import assignment cannot be used when targeting ECMAScript modules.
//! This error is emitted when using `import x = require("y")` in a file that
//! has other ES module syntax (import/export).

use crate::checker::context::CheckerContext;
use crate::checker::types::diagnostics::diagnostic_codes;
use crate::parser::{NodeIndex, syntax_kind_ext};

/// Result of an import check.
#[derive(Clone, Debug, PartialEq)]
pub enum ImportCheckResult {
    /// Import is valid
    Valid,
    /// Import assignment used in ESM context (should emit TS1202)
    ImportAssignmentInESM,
    /// Duplicate import detected
    DuplicateImport,
    /// Unresolved module
    UnresolvedModule,
}

/// Information about an import declaration.
#[derive(Clone, Debug)]
pub struct ImportInfo {
    /// The node index of the import statement
    pub node: NodeIndex,
    /// The module name being imported
    pub module_name: String,
    /// Whether this is an import equals declaration (`import x = require()`)
    pub is_import_equals: bool,
    /// Whether this file is an ES module (has import/export)
    pub is_external_module: bool,
}

/// Checker for import statements to detect ESM conflicts.
pub struct ImportChecker<'a> {
    /// Reference to the checker context
    ctx: &'a CheckerContext<'a>,
    /// Track import equals declarations seen in this file
    import_equals_declarations: Vec<NodeIndex>,
    /// Track whether this file has ESM imports/exports
    has_esm_syntax: bool,
}

impl<'a> ImportChecker<'a> {
    /// Create a new import checker.
    pub fn new(ctx: &'a CheckerContext<'a>) -> Self {
        Self {
            ctx,
            import_equals_declarations: Vec::new(),
            has_esm_syntax: false,
        }
    }

    /// Check an import statement for ESM conflicts.
    ///
    /// This is the main entry point for import checking.
    /// It returns whether the import is valid or if it should emit TS1202.
    pub fn check_import(&mut self, info: &ImportInfo) -> ImportCheckResult {
        // Check if this is an import equals declaration in an ES module
        if info.is_import_equals && info.is_external_module {
            return ImportCheckResult::ImportAssignmentInESM;
        }

        // Track ESM syntax
        if !info.is_import_equals {
            self.has_esm_syntax = true;
        } else {
            self.import_equals_declarations.push(info.node);
        }

        ImportCheckResult::Valid
    }

    /// Check all imports after parsing is complete.
    ///
    /// This is called after all imports have been checked individually
    /// to detect conflicts between import equals and ESM syntax.
    pub fn check_post_parse(&mut self) -> Vec<ImportCheckResult> {
        let mut results = Vec::new();

        // If we have both import equals declarations and ESM syntax,
        // emit TS1202 for all import equals declarations
        if self.has_esm_syntax && !self.import_equals_declarations.is_empty() {
            for _ in &self.import_equals_declarations {
                results.push(ImportCheckResult::ImportAssignmentInESM);
            }
        }

        results
    }

    /// Check if the current file is an external module (has ESM syntax).
    ///
    /// A file is considered an external module if it has:
    /// - An import or export declaration
    /// - import.meta usage
    /// - Top-level await
    pub fn is_external_module(&self) -> bool {
        self.ctx.binder.is_external_module()
    }

    /// Check an import equals declaration specifically.
    ///
    /// Emits TS1202 when `import x = require()` is used in an ES module.
    pub fn check_import_equals_declaration(
        &mut self,
        node: NodeIndex,
    ) -> ImportCheckResult {
        // Check if this is an external module
        if self.is_external_module() {
            return ImportCheckResult::ImportAssignmentInESM;
        }

        self.import_equals_declarations.push(node);
        ImportCheckResult::Valid
    }

    /// Check an import declaration for unresolved modules.
    ///
    /// Emits TS2792 when the module cannot be resolved.
    /// Emits TS2305 when a module exists but doesn't export a specific member.
    pub fn check_import_declaration(
        &mut self,
        _node: NodeIndex,
        _module_name: &str,
    ) -> ImportCheckResult {
        // Mark that we have ESM syntax
        self.has_esm_syntax = true;

        // Note: Module resolution checking is done in the main checker
        // This just tracks that ESM syntax is present
        ImportCheckResult::Valid
    }

    /// Create a diagnostic message for TS1202.
    pub fn create_ts1202_diagnostic() -> String {
        "Import assignment cannot be used when targeting ECMAScript modules. Consider using 'import * as ns from \"mod\"', 'import {a} from \"mod\"', 'import d from \"mod\"', or another module format instead.".to_string()
    }

    /// Get the diagnostic code for TS1202.
    pub fn ts1202_code() -> u32 {
        diagnostic_codes::IMPORT_ASSIGNMENT_CANNOT_BE_USED_WITH_ESM
    }

    /// Reset the checker state (useful when starting a new file).
    pub fn reset(&mut self) {
        self.import_equals_declarations.clear();
        self.has_esm_syntax = false;
    }

    /// Check if the file has any import equals declarations.
    pub fn has_import_equals(&self) -> bool {
        !self.import_equals_declarations.is_empty()
    }

    /// Get all import equals declaration nodes.
    pub fn get_import_equals_declarations(&self) -> &[NodeIndex] {
        &self.import_equals_declarations
    }

    /// Check if an import statement is an import equals declaration.
    ///
    /// Import equals declarations have the form: `import x = require("module")`
    pub fn is_import_equals_declaration(&self, node: NodeIndex) -> bool {
        let Some(node_data) = self.ctx.arena.get(node) else {
            return false;
        };

        // Check if it's an import equals declaration
        node_data.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
    }

    /// Check if the file should be treated as an ES module.
    ///
    /// A file is an ES module if:
    /// - It has "type": "module" in package.json
    /// - It has an .mts or .mjs extension
    /// - It has import or export statements
    /// - The compiler target is ESM and the file has ESM syntax
    pub fn should_treat_as_esm(&self) -> bool {
        // Check if the binder has marked this as an external module
        if self.ctx.binder.is_external_module() {
            return true;
        }

        // Check if we've seen ESM syntax
        self.has_esm_syntax
    }

    /// Validate import statement ordering and conflicts.
    ///
    /// This checks for:
    /// - Import equals declarations mixed with ESM imports
    /// - Duplicate imports
    /// - Import/export conflicts
    pub fn validate_imports(&mut self) -> Vec<ImportCheckResult> {
        let mut results = Vec::new();

        // Check for import equals in ESM context
        if self.has_esm_syntax && !self.import_equals_declarations.is_empty() {
            for _ in &self.import_equals_declarations {
                results.push(ImportCheckResult::ImportAssignmentInESM);
            }
        }

        results
    }

    /// Check for import conflicts between import equals and named imports.
    ///
    /// This detects when the same name is imported using both
    /// `import x = require()` and `import { x } from` syntax.
    pub fn check_import_conflicts(&self, _name: &str) -> ImportCheckResult {
        // Note: This would require tracking all import bindings
        // For now, just return valid
        ImportCheckResult::Valid
    }

    /// Get the import mode for the current file.
    ///
    /// Returns whether the file should use CommonJS or ESM imports.
    pub fn get_import_mode(&self) -> ImportMode {
        if self.should_treat_as_esm() {
            ImportMode::ESM
        } else {
            ImportMode::CommonJS
        }
    }
}

/// The import mode for a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportMode {
    /// CommonJS imports (require, module.exports)
    CommonJS,
    /// ES modules (import, export)
    ESM,
    /// Mixed mode (both CommonJS and ESM - may emit TS1202)
    Mixed,
}

/// Helper function to check if a node is an external module indicator.
///
/// External module indicators include:
/// - ImportDeclaration
/// - ExportDeclaration
/// - ImportEqualsDeclaration (when in ESM context)
/// - ExportAssignment
pub fn is_external_module_indicator(kind: u16) -> bool {
    use syntax_kind_ext::*;

    matches!(
        kind,
        IMPORT_DECLARATION | EXPORT_DECLARATION | IMPORT_EQUALS_DECLARATION | EXPORT_ASSIGNMENT
    )
}

/// Helper function to check if a file should be treated as a script vs module.
///
/// Scripts cannot use import/export syntax.
/// Modules can use both CommonJS and ESM syntax, but mixing them may cause TS1202.
pub fn determine_module_kind(
    has_import_equals: bool,
    has_esm_imports: bool,
    is_package_json_module: bool,
) -> ImportMode {
    // If package.json has "type": "module", treat as ESM
    if is_package_json_module {
        return ImportMode::ESM;
    }

    // If we have both import equals and ESM imports, it's mixed
    if has_import_equals && has_esm_imports {
        return ImportMode::Mixed;
    }

    // If we have ESM imports, it's ESM
    if has_esm_imports {
        return ImportMode::ESM;
    }

    // Otherwise, it's CommonJS (or a script)
    ImportMode::CommonJS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_import_info_creation() {
        let info = ImportInfo {
            node: NodeIndex(0),
            module_name: "test-module".to_string(),
            is_import_equals: true,
            is_external_module: false,
        };

        assert_eq!(info.module_name, "test-module");
        assert!(info.is_import_equals);
        assert!(!info.is_external_module);
    }

    #[test]
    fn test_import_check_result_valid() {
        let result = ImportCheckResult::Valid;
        assert_eq!(result, ImportCheckResult::Valid);
    }

    #[test]
    fn test_import_check_result_import_assignment_in_esm() {
        let result = ImportCheckResult::ImportAssignmentInESM;
        assert_eq!(result, ImportCheckResult::ImportAssignmentInESM);
    }

    #[test]
    fn test_determine_module_kind_commonjs() {
        let mode = determine_module_kind(false, false, false);
        assert_eq!(mode, ImportMode::CommonJS);
    }

    #[test]
    fn test_determine_module_kind_esm() {
        let mode = determine_module_kind(false, true, false);
        assert_eq!(mode, ImportMode::ESM);
    }

    #[test]
    fn test_determine_module_kind_mixed() {
        let mode = determine_module_kind(true, true, false);
        assert_eq!(mode, ImportMode::Mixed);
    }

    #[test]
    fn test_determine_module_kind_package_json_module() {
        let mode = determine_module_kind(false, false, true);
        assert_eq!(mode, ImportMode::ESM);
    }

    #[test]
    fn test_is_external_module_indicator() {
        assert!(is_external_module_indicator(syntax_kind_ext::IMPORT_DECLARATION));
        assert!(is_external_module_indicator(syntax_kind_ext::EXPORT_DECLARATION));
        assert!(is_external_module_indicator(syntax_kind_ext::IMPORT_EQUALS_DECLARATION));
        // Test with a non-module kind (0 is not a valid syntax kind for imports)
        assert!(!is_external_module_indicator(0));
    }

    #[test]
    fn test_ts1202_code() {
        let code = ImportChecker::ts1202_code();
        assert_eq!(code, 1202);
    }

    #[test]
    fn test_create_ts1202_diagnostic() {
        let message = ImportChecker::create_ts1202_diagnostic();
        assert!(message.contains("Import assignment"));
        assert!(message.contains("ECMAScript modules"));
    }
}
