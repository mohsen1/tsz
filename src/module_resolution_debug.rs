//! Module Resolution Debugging
//!
//! This module provides debugging infrastructure for symbol table operations,
//! module scope lookups, and cross-file symbol resolution.
//!
//! Enable debug logging by setting `debug_enabled: true` in `ModuleResolutionDebugger`.

use crate::binder::{SymbolId, symbol_flags};
use rustc_hash::FxHashMap;
use std::sync::atomic::{AtomicBool, Ordering};

/// Global flag to enable/disable module resolution debugging.
/// Use `set_debug_enabled(true)` to turn on logging.
static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);

/// Enable or disable module resolution debugging globally.
pub fn set_debug_enabled(enabled: bool) {
    DEBUG_ENABLED.store(enabled, Ordering::SeqCst);
}

/// Check if module resolution debugging is enabled.
pub fn is_debug_enabled() -> bool {
    DEBUG_ENABLED.load(Ordering::Relaxed)
}

/// A record of a symbol declaration event.
#[derive(Debug, Clone)]
pub struct SymbolDeclarationEvent {
    /// The symbol's name
    pub name: String,
    /// The symbol ID
    pub symbol_id: SymbolId,
    /// The symbol's flags (decoded)
    pub flags_description: String,
    /// The file where this symbol was declared
    pub file_name: String,
    /// Whether this was a merge with an existing symbol
    pub is_merge: bool,
    /// The number of declarations after this event
    pub declaration_count: usize,
}

/// A record of a symbol lookup event.
#[derive(Debug, Clone)]
pub struct SymbolLookupEvent {
    /// The name being looked up
    pub name: String,
    /// The scope path searched (from innermost to outermost)
    pub scope_path: Vec<String>,
    /// Whether the lookup was successful
    pub found: bool,
    /// The symbol ID if found
    pub symbol_id: Option<SymbolId>,
    /// The file where the symbol was found (if any)
    pub found_in_file: Option<String>,
}

/// A record of a symbol merge operation.
#[derive(Debug, Clone)]
pub struct SymbolMergeEvent {
    /// The symbol's name
    pub name: String,
    /// The symbol ID
    pub symbol_id: SymbolId,
    /// The existing flags before merge
    pub existing_flags: String,
    /// The new flags being merged
    pub new_flags: String,
    /// The combined flags after merge
    pub combined_flags: String,
    /// The file contributing the new declaration
    pub contributing_file: String,
}

/// Debugger for module resolution operations.
#[derive(Debug, Default)]
pub struct ModuleResolutionDebugger {
    /// All symbol declaration events
    pub declaration_events: Vec<SymbolDeclarationEvent>,
    /// All symbol lookup events
    pub lookup_events: Vec<SymbolLookupEvent>,
    /// All symbol merge events
    pub merge_events: Vec<SymbolMergeEvent>,
    /// Symbol origins: maps SymbolId to the file name where it was first declared
    pub symbol_origins: FxHashMap<SymbolId, String>,
    /// Current file being processed
    pub current_file: String,
}

impl ModuleResolutionDebugger {
    /// Create a new debugger instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the current file being processed.
    pub fn set_current_file(&mut self, file_name: &str) {
        self.current_file = file_name.to_string();
    }

    /// Record a symbol declaration.
    pub fn record_declaration(
        &mut self,
        name: &str,
        symbol_id: SymbolId,
        flags: u32,
        declaration_count: usize,
        is_merge: bool,
    ) {
        if !is_debug_enabled() {
            return;
        }

        let event = SymbolDeclarationEvent {
            name: name.to_string(),
            symbol_id,
            flags_description: flags_to_string(flags),
            file_name: self.current_file.clone(),
            is_merge,
            declaration_count,
        };

        // Track symbol origin (only for first declaration)
        if !is_merge && !self.symbol_origins.contains_key(&symbol_id) {
            self.symbol_origins
                .insert(symbol_id, self.current_file.clone());
        }

        if is_debug_enabled() {
            eprintln!(
                "[MODULE_DEBUG] {} symbol '{}' (id={}) with flags [{}] in {} (decls={})",
                if is_merge { "MERGED" } else { "DECLARED" },
                event.name,
                symbol_id.0,
                event.flags_description,
                event.file_name,
                event.declaration_count
            );
        }

        self.declaration_events.push(event);
    }

    /// Record a symbol merge operation.
    pub fn record_merge(
        &mut self,
        name: &str,
        symbol_id: SymbolId,
        existing_flags: u32,
        new_flags: u32,
        combined_flags: u32,
    ) {
        if !is_debug_enabled() {
            return;
        }

        let event = SymbolMergeEvent {
            name: name.to_string(),
            symbol_id,
            existing_flags: flags_to_string(existing_flags),
            new_flags: flags_to_string(new_flags),
            combined_flags: flags_to_string(combined_flags),
            contributing_file: self.current_file.clone(),
        };

        eprintln!(
            "[MODULE_DEBUG] MERGE '{}' (id={}): [{}] + [{}] = [{}] (from {})",
            event.name,
            symbol_id.0,
            event.existing_flags,
            event.new_flags,
            event.combined_flags,
            event.contributing_file
        );

        self.merge_events.push(event);
    }

    /// Record a symbol lookup.
    pub fn record_lookup(&mut self, name: &str, scope_path: Vec<String>, result: Option<SymbolId>) {
        if !is_debug_enabled() {
            return;
        }

        let found_in_file = result.and_then(|id| self.symbol_origins.get(&id).cloned());

        let event = SymbolLookupEvent {
            name: name.to_string(),
            scope_path: scope_path.clone(),
            found: result.is_some(),
            symbol_id: result,
            found_in_file: found_in_file.clone(),
        };

        eprintln!(
            "[MODULE_DEBUG] LOOKUP '{}': scopes=[{}] -> {} (file: {})",
            event.name,
            scope_path.join(" -> "),
            if event.found {
                format!("FOUND (id={})", result.unwrap().0)
            } else {
                "NOT FOUND".to_string()
            },
            found_in_file.unwrap_or_else(|| "unknown".to_string())
        );

        self.lookup_events.push(event);
    }

    /// Get a summary of all recorded events.
    pub fn get_summary(&self) -> String {
        let mut summary = String::new();
        summary.push_str("=== Module Resolution Debug Summary ===\n\n");

        summary.push_str(&format!(
            "Total declarations: {}\n",
            self.declaration_events.len()
        ));
        summary.push_str(&format!("Total merges: {}\n", self.merge_events.len()));
        summary.push_str(&format!("Total lookups: {}\n\n", self.lookup_events.len()));

        // Symbol origins by file
        summary.push_str("Symbol Origins by File:\n");
        let mut by_file: FxHashMap<String, Vec<SymbolId>> = FxHashMap::default();
        for (sym_id, file) in &self.symbol_origins {
            by_file.entry(file.clone()).or_default().push(*sym_id);
        }
        for (file, symbols) in &by_file {
            summary.push_str(&format!("  {}: {} symbols\n", file, symbols.len()));
        }

        // Merge operations
        if !self.merge_events.is_empty() {
            summary.push_str("\nMerge Operations:\n");
            for event in &self.merge_events {
                summary.push_str(&format!(
                    "  {} (id={}): [{}] + [{}] from {}\n",
                    event.name,
                    event.symbol_id.0,
                    event.existing_flags,
                    event.new_flags,
                    event.contributing_file
                ));
            }
        }

        // Failed lookups
        let failed_lookups: Vec<_> = self.lookup_events.iter().filter(|e| !e.found).collect();
        if !failed_lookups.is_empty() {
            summary.push_str("\nFailed Lookups:\n");
            for event in failed_lookups {
                summary.push_str(&format!(
                    "  '{}': searched [{}]\n",
                    event.name,
                    event.scope_path.join(" -> ")
                ));
            }
        }

        summary
    }

    /// Clear all recorded events.
    pub fn clear(&mut self) {
        self.declaration_events.clear();
        self.lookup_events.clear();
        self.merge_events.clear();
        self.symbol_origins.clear();
    }
}

/// Convert symbol flags to a human-readable string.
pub fn flags_to_string(flags: u32) -> String {
    let mut parts = Vec::new();

    if flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0 {
        parts.push("VAR");
    }
    if flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
        parts.push("LET/CONST");
    }
    if flags & symbol_flags::PROPERTY != 0 {
        parts.push("PROPERTY");
    }
    if flags & symbol_flags::ENUM_MEMBER != 0 {
        parts.push("ENUM_MEMBER");
    }
    if flags & symbol_flags::FUNCTION != 0 {
        parts.push("FUNCTION");
    }
    if flags & symbol_flags::CLASS != 0 {
        parts.push("CLASS");
    }
    if flags & symbol_flags::INTERFACE != 0 {
        parts.push("INTERFACE");
    }
    if flags & symbol_flags::CONST_ENUM != 0 {
        parts.push("CONST_ENUM");
    }
    if flags & symbol_flags::REGULAR_ENUM != 0 {
        parts.push("ENUM");
    }
    if flags & symbol_flags::VALUE_MODULE != 0 {
        parts.push("VALUE_MODULE");
    }
    if flags & symbol_flags::NAMESPACE_MODULE != 0 {
        parts.push("NAMESPACE");
    }
    if flags & symbol_flags::TYPE_LITERAL != 0 {
        parts.push("TYPE_LITERAL");
    }
    if flags & symbol_flags::OBJECT_LITERAL != 0 {
        parts.push("OBJECT_LITERAL");
    }
    if flags & symbol_flags::METHOD != 0 {
        parts.push("METHOD");
    }
    if flags & symbol_flags::CONSTRUCTOR != 0 {
        parts.push("CONSTRUCTOR");
    }
    if flags & symbol_flags::GET_ACCESSOR != 0 {
        parts.push("GETTER");
    }
    if flags & symbol_flags::SET_ACCESSOR != 0 {
        parts.push("SETTER");
    }
    if flags & symbol_flags::TYPE_PARAMETER != 0 {
        parts.push("TYPE_PARAM");
    }
    if flags & symbol_flags::TYPE_ALIAS != 0 {
        parts.push("TYPE_ALIAS");
    }
    if flags & symbol_flags::ALIAS != 0 {
        parts.push("ALIAS");
    }
    if flags & symbol_flags::EXPORT_VALUE != 0 {
        parts.push("EXPORT");
    }

    if parts.is_empty() {
        "NONE".to_string()
    } else {
        parts.join("|")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flags_to_string() {
        assert_eq!(flags_to_string(symbol_flags::FUNCTION), "FUNCTION");
        assert_eq!(flags_to_string(symbol_flags::CLASS), "CLASS");
        assert_eq!(flags_to_string(symbol_flags::INTERFACE), "INTERFACE");
        assert_eq!(
            flags_to_string(symbol_flags::INTERFACE | symbol_flags::CLASS),
            "CLASS|INTERFACE"
        );
        assert_eq!(flags_to_string(symbol_flags::NONE), "NONE");
    }

    #[test]
    fn test_debugger_records_events() {
        set_debug_enabled(true);

        let mut debugger = ModuleResolutionDebugger::new();
        debugger.set_current_file("test.ts");

        // Record a declaration
        debugger.record_declaration("MyClass", SymbolId(1), symbol_flags::CLASS, 1, false);

        assert_eq!(debugger.declaration_events.len(), 1);
        assert_eq!(debugger.declaration_events[0].name, "MyClass");
        assert!(!debugger.declaration_events[0].is_merge);

        // Record a merge
        debugger.record_merge(
            "MyInterface",
            SymbolId(2),
            symbol_flags::INTERFACE,
            symbol_flags::INTERFACE,
            symbol_flags::INTERFACE,
        );

        assert_eq!(debugger.merge_events.len(), 1);
        assert_eq!(debugger.merge_events[0].name, "MyInterface");

        // Record a lookup
        debugger.record_lookup(
            "MyClass",
            vec!["local".into(), "file".into()],
            Some(SymbolId(1)),
        );

        assert_eq!(debugger.lookup_events.len(), 1);
        assert!(debugger.lookup_events[0].found);

        set_debug_enabled(false);
    }

    #[test]
    fn test_summary_generation() {
        set_debug_enabled(true);

        let mut debugger = ModuleResolutionDebugger::new();
        debugger.set_current_file("test.ts");

        debugger.record_declaration("foo", SymbolId(1), symbol_flags::FUNCTION, 1, false);
        debugger.record_lookup("bar", vec!["scope1".into()], None);

        let summary = debugger.get_summary();
        assert!(summary.contains("Module Resolution Debug Summary"));
        assert!(summary.contains("Total declarations: 1"));
        assert!(summary.contains("Failed Lookups:"));

        set_debug_enabled(false);
    }
}
