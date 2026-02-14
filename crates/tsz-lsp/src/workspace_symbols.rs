//! Workspace Symbols implementation for LSP.
//!
//! Provides project-wide symbol search functionality. Given a query string,
//! searches all indexed symbol definitions across the workspace and returns
//! matching results sorted by relevance (exact match > prefix > substring).

use crate::document_symbols::SymbolKind;
use crate::symbol_index::SymbolIndex;
use tsz_common::position::Location;

/// Maximum number of results returned by a workspace symbol search.
const MAX_RESULTS: usize = 100;

/// Information about a symbol found in the workspace.
///
/// Corresponds to the LSP `SymbolInformation` type.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SymbolInformation {
    /// The name of the symbol.
    pub name: String,
    /// The kind of the symbol (function, class, variable, etc.).
    pub kind: SymbolKind,
    /// The location where the symbol is defined.
    pub location: Location,
}

impl SymbolInformation {
    /// Create a new symbol information entry.
    pub fn new(name: String, kind: SymbolKind, location: Location) -> Self {
        Self {
            name,
            kind,
            location,
        }
    }
}

/// Relevance category for sorting search results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchKind {
    /// The query matches the symbol name exactly (case-insensitive).
    Exact = 0,
    /// The symbol name starts with the query (case-insensitive).
    Prefix = 1,
    /// The query appears somewhere within the symbol name (case-insensitive).
    Substring = 2,
}

/// Provider for workspace-wide symbol search.
///
/// Searches across all indexed symbols in a project's `SymbolIndex` and
/// returns LSP-formatted `SymbolInformation` results.
pub struct WorkspaceSymbolsProvider<'a> {
    index: &'a SymbolIndex,
}

impl<'a> WorkspaceSymbolsProvider<'a> {
    /// Create a new workspace symbols provider.
    ///
    /// # Arguments
    /// * `index` - The project-wide symbol index to search
    pub fn new(index: &'a SymbolIndex) -> Self {
        Self { index }
    }

    /// Find symbols matching the given query string.
    ///
    /// Performs case-insensitive substring matching against all symbol
    /// definitions in the index. Results are sorted by relevance:
    /// 1. Exact matches (case-insensitive)
    /// 2. Prefix matches
    /// 3. Substring matches
    ///
    /// Within each category, results are sorted alphabetically by name.
    /// At most 100 results are returned.
    ///
    /// # Arguments
    /// * `query` - The search query string. An empty query returns no results.
    ///
    /// # Returns
    /// A vector of `SymbolInformation` for matching symbols, sorted by relevance.
    pub fn find_symbols(&self, query: &str) -> Vec<SymbolInformation> {
        if query.is_empty() {
            return Vec::new();
        }

        let query_lower = query.to_lowercase();
        let mut matches: Vec<(MatchKind, SymbolInformation)> = Vec::new();

        for name in self.index.all_definition_names() {
            let name_lower = name.to_lowercase();

            let match_kind = if name_lower == query_lower {
                MatchKind::Exact
            } else if name_lower.starts_with(&query_lower) {
                MatchKind::Prefix
            } else if name_lower.contains(&query_lower) {
                MatchKind::Substring
            } else {
                continue;
            };

            let definitions = self.index.find_definitions(name);
            // Prefer the kind stored in the index (derived from binder data),
            // falling back to naming-convention heuristics when unavailable.
            let kind = self
                .index
                .get_definition_kind(name)
                .unwrap_or_else(|| Self::infer_symbol_kind(name));

            for location in definitions {
                matches.push((
                    match_kind,
                    SymbolInformation::new(name.to_string(), kind, location),
                ));
            }
        }

        // Sort by: match kind (exact < prefix < substring), then alphabetically by name
        matches.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase()))
        });

        matches
            .into_iter()
            .take(MAX_RESULTS)
            .map(|(_, info)| info)
            .collect()
    }

    /// Infer the symbol kind from the symbol name.
    ///
    /// Uses naming conventions to guess the kind:
    /// - PascalCase names starting with "I" followed by uppercase -> Interface
    /// - PascalCase names -> Class
    /// - SCREAMING_SNAKE_CASE -> Constant
    /// - All other names -> Variable
    ///
    /// This is a heuristic; the SymbolIndex does not currently store kind information.
    fn infer_symbol_kind(name: &str) -> SymbolKind {
        if name.is_empty() {
            return SymbolKind::Variable;
        }

        let chars: Vec<char> = name.chars().collect();

        // Check for SCREAMING_SNAKE_CASE (all uppercase with underscores)
        if chars.len() > 1
            && chars
                .iter()
                .all(|c| c.is_ascii_uppercase() || *c == '_' || c.is_ascii_digit())
        {
            return SymbolKind::Constant;
        }

        // Check for PascalCase (starts with uppercase letter)
        if chars[0].is_ascii_uppercase() {
            // Convention: interface names often start with 'I' followed by uppercase
            if chars.len() > 1 && chars[0] == 'I' && chars[1].is_ascii_uppercase() {
                return SymbolKind::Interface;
            }
            return SymbolKind::Class;
        }

        SymbolKind::Variable
    }
}

#[cfg(test)]
#[path = "../tests/workspace_symbols_tests.rs"]
mod workspace_symbols_tests;
