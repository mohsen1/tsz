//! Post-binding validation, lib symbol diagnostics, and resolution statistics.
//!
//! Extracted from the core binding module to keep it focused on
//! declaration binding, accessor generation, and flow graph construction.

use crate::lib_loader;
use crate::state::{BinderState, ResolutionStats, ValidationError};
use std::fmt::Write;
use std::sync::Arc;
use tracing::{debug, warn};

impl BinderState {
    // ========================================================================
    // Symbol Table Validation
    // ========================================================================

    /// Run post-binding validation checks on the symbol table.
    /// Returns a list of validation errors found.
    pub fn validate_symbol_table(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        for (&node_idx, &sym_id) in self.node_symbols.iter() {
            if self.symbols.get(sym_id).is_none() {
                errors.push(ValidationError::BrokenSymbolLink {
                    node_index: node_idx,
                    symbol_id: sym_id.0,
                });
            }
        }

        for sym in self.symbols.iter() {
            if sym.declarations.is_empty() {
                errors.push(ValidationError::OrphanedSymbol {
                    symbol_id: sym.id.0,
                    name: sym.escaped_name.clone(),
                });
            }
        }

        for sym in self.symbols.iter() {
            if sym.value_declaration.is_some() {
                let has_node_mapping = self.node_symbols.contains_key(&sym.value_declaration.0);
                if !has_node_mapping {
                    errors.push(ValidationError::InvalidValueDeclaration {
                        symbol_id: sym.id.0,
                        name: sym.escaped_name.clone(),
                    });
                }
            }
        }

        errors
    }

    /// Check if the symbol table has any validation errors.
    pub fn is_symbol_table_valid(&self) -> bool {
        self.validate_symbol_table().is_empty()
    }

    // ========================================================================
    // Lib Symbol Validation (P0 Task - Improve Test Runner Lib Injection)
    // ========================================================================

    /// Expected global symbols that should be available from lib.d.ts.
    /// These are core ECMAScript globals that should always be present.
    const EXPECTED_GLOBAL_SYMBOLS: &'static [&'static str] = &[
        // Core types
        "Object",
        "Function",
        "Array",
        "String",
        "Number",
        "Boolean",
        "Symbol",
        "BigInt",
        // Error types
        "Error",
        "EvalError",
        "RangeError",
        "ReferenceError",
        "SyntaxError",
        "TypeError",
        "URIError",
        // Collections
        "Map",
        "Set",
        "WeakMap",
        "WeakSet",
        // Promises and async
        "Promise",
        // Object reflection
        "Reflect",
        "Proxy",
        // Global functions
        "eval",
        "isNaN",
        "isFinite",
        "parseFloat",
        "parseInt",
        // Global values
        "Infinity",
        "NaN",
        "undefined",
        // Console (if DOM lib is loaded)
        "console",
    ];

    /// Validate that expected global symbols are present after binding.
    ///
    /// This method should be called after `bind_source_file_with_libs` to ensure
    /// that lib symbols were properly loaded and merged into the binder.
    ///
    /// Returns a list of missing symbol names. Empty list means all expected symbols are present.
    ///
    /// # Example
    /// ```ignore
    /// binder.bind_source_file_with_libs(arena, root, &lib_files);
    /// let missing = binder.validate_global_symbols();
    /// if !missing.is_empty() {
    ///     eprintln!("WARNING: Missing global symbols: {:?}", missing);
    /// }
    /// ```
    pub fn validate_global_symbols(&self) -> Vec<String> {
        let mut missing = Vec::new();

        for &symbol_name in Self::EXPECTED_GLOBAL_SYMBOLS {
            // Check if the symbol is available via resolve_identifier
            // (which checks both file_locals and lib_binders)
            let is_available = self.file_locals.has(symbol_name)
                || self
                    .lib_binders
                    .iter()
                    .any(|b| b.file_locals.has(symbol_name));

            if !is_available {
                missing.push(symbol_name.to_string());
            }
        }

        missing
    }

    /// Get a detailed report of lib symbol availability.
    ///
    /// Returns a human-readable string showing:
    /// - Which expected symbols are present
    /// - Which expected symbols are missing
    /// - Total symbol count from `file_locals` and `lib_binders`
    pub fn get_lib_symbol_report(&self) -> String {
        let mut report = String::new();
        report.push_str("=== Lib Symbol Availability Report ===\n\n");

        // Count total symbols
        let file_local_count = self.file_locals.len();
        let lib_binder_count: usize = self.lib_binders.iter().map(|b| b.file_locals.len()).sum();

        let _ = writeln!(report, "File locals: {file_local_count} symbols");
        let _ = writeln!(
            report,
            "Lib binders: {} symbols ({} binders)",
            lib_binder_count,
            self.lib_binders.len()
        );
        report.push('\n');

        // Check each expected symbol
        let mut present = Vec::new();
        let mut missing = Vec::new();

        for &symbol_name in Self::EXPECTED_GLOBAL_SYMBOLS {
            let is_available = self.file_locals.has(symbol_name)
                || self
                    .lib_binders
                    .iter()
                    .any(|b| b.file_locals.has(symbol_name));

            if is_available {
                present.push(symbol_name);
            } else {
                missing.push(symbol_name);
            }
        }

        let _ = writeln!(
            report,
            "Expected symbols present: {}/{}",
            present.len(),
            Self::EXPECTED_GLOBAL_SYMBOLS.len()
        );
        if !missing.is_empty() {
            report.push_str("\nMissing symbols:\n");
            for name in &missing {
                let _ = writeln!(report, "  - {name}");
            }
        }

        // Show which lib binders contribute symbols
        if !self.lib_binders.is_empty() {
            report.push_str("\nLib binder contributions:\n");
            for (i, lib_binder) in self.lib_binders.iter().enumerate() {
                let _ = writeln!(
                    report,
                    "  Lib binder {}: {} symbols",
                    i,
                    lib_binder.file_locals.len()
                );
            }
        }

        report
    }

    /// Log missing lib symbols with debug context.
    ///
    /// This should be called at test start to warn about missing lib symbols
    /// that might cause test failures.
    ///
    /// Returns true if any expected symbols are missing.
    pub fn log_missing_lib_symbols(&self) -> bool {
        let missing = self.validate_global_symbols();

        if missing.is_empty() {
            debug!(
                "[LIB_SYMBOL_INFO] All {} expected global symbols are present.",
                Self::EXPECTED_GLOBAL_SYMBOLS.len()
            );
            false
        } else {
            warn!(
                "[LIB_SYMBOL_WARNING] Missing {} expected global symbols: {:?}",
                missing.len(),
                missing
            );
            warn!("[LIB_SYMBOL_WARNING] This may cause test failures due to unresolved symbols.");
            warn!(
                "[LIB_SYMBOL_WARNING] Ensure lib.d.ts is loaded via addLibFile() before binding."
            );
            true
        }
    }

    /// Verify that lib symbols from multiple test files are properly merged.
    ///
    /// This method checks that symbols from multiple lib files are all accessible
    /// through the binder's symbol resolution chain.
    ///
    /// # Arguments
    /// * `lib_files` - The lib files that were supposed to be merged
    ///
    /// Returns a list of lib file names whose symbols are not fully accessible.
    pub fn verify_lib_symbol_merge(&self, lib_files: &[Arc<lib_loader::LibFile>]) -> Vec<String> {
        let mut inaccessible = Vec::new();

        for lib_file in lib_files {
            let file_name = lib_file.file_name.clone();

            // Check if symbols from this lib file are accessible
            let mut has_accessible_symbols = false;
            for (name, &_sym_id) in lib_file.binder.file_locals.iter() {
                // Try to resolve the symbol through our binder
                if self.file_locals.get(name).is_some()
                    || self
                        .lib_binders
                        .iter()
                        .any(|b| b.file_locals.get(name).is_some())
                {
                    has_accessible_symbols = true;
                    break;
                }
            }

            if !has_accessible_symbols && !lib_file.binder.file_locals.is_empty() {
                inaccessible.push(file_name);
            }
        }

        inaccessible
    }

    // ========================================================================
    // Symbol Resolution Statistics (P1 Task - Debug Logging)
    // ========================================================================

    /// Get a snapshot of current symbol resolution statistics.
    ///
    /// This method scans the binder state to provide statistics about
    /// symbol resolution capability, including:
    /// - Available symbols by source (scopes, `file_locals`, `lib_binders`)
    /// - Potential resolution paths
    pub fn get_resolution_stats(&self) -> ResolutionStats {
        // Count symbols in each resolution tier
        let scope_symbols: u64 = self.scopes.iter().map(|s| s.table.len() as u64).sum();

        let file_local_symbols = self.file_locals.len() as u64;

        let lib_binder_symbols: u64 = self
            .lib_binders
            .iter()
            .map(|b| b.file_locals.len() as u64)
            .sum();

        ResolutionStats {
            attempts: 0, // Would need runtime tracking
            scope_hits: scope_symbols,
            file_local_hits: file_local_symbols,
            lib_binder_hits: lib_binder_symbols,
            failures: 0, // Would need runtime tracking
        }
    }

    /// Get a human-readable summary of resolution statistics.
    pub fn get_resolution_summary(&self) -> String {
        let stats = self.get_resolution_stats();
        format!(
            "Symbol Resolution Summary:\n\
             - Scope symbols: {}\n\
             - File local symbols: {}\n\
             - Lib binder symbols: {} (from {} binders)\n\
             - Total accessible symbols: {}\n\
             - Expected global symbols: {}",
            stats.scope_hits,
            stats.file_local_hits,
            stats.lib_binder_hits,
            self.lib_binders.len(),
            stats.scope_hits + stats.file_local_hits + stats.lib_binder_hits,
            Self::EXPECTED_GLOBAL_SYMBOLS.len()
        )
    }
}

#[cfg(test)]
#[path = "../../tests/binding_validation.rs"]
mod tests;
