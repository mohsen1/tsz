//! Workspace Symbols implementation for LSP.
//!
//! Provides project-wide symbol search functionality. Given a query string,
//! searches all indexed symbol definitions across the workspace and returns
//! matching results sorted by relevance (exact match > prefix > substring).

use crate::lsp::document_symbols::SymbolKind;
use crate::lsp::position::Location;
use crate::lsp::symbol_index::SymbolIndex;

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
            let kind = Self::infer_symbol_kind(name);

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
mod workspace_symbols_tests {
    use super::*;
    use crate::lsp::position::{Position, Range};

    fn make_location(file: &str, line: u32, start_col: u32, end_col: u32) -> Location {
        Location::new(
            file.to_string(),
            Range::new(Position::new(line, start_col), Position::new(line, end_col)),
        )
    }

    fn setup_index() -> SymbolIndex {
        let mut index = SymbolIndex::new();

        index.add_definition("MyClass", make_location("types.ts", 0, 0, 7));
        index.add_definition("myFunction", make_location("utils.ts", 5, 0, 10));
        index.add_definition("myVariable", make_location("main.ts", 0, 0, 10));
        index.add_definition("IMyInterface", make_location("types.ts", 10, 0, 12));
        index.add_definition("MAX_COUNT", make_location("constants.ts", 0, 0, 9));
        index.add_definition("helper", make_location("utils.ts", 20, 0, 6));
        index.add_definition("MyOtherClass", make_location("types.ts", 30, 0, 12));

        index
    }

    #[test]
    fn test_empty_query_returns_empty() {
        let index = setup_index();
        let provider = WorkspaceSymbolsProvider::new(&index);

        let results = provider.find_symbols("");
        assert!(results.is_empty(), "Empty query should return no results");
    }

    #[test]
    fn test_exact_match() {
        let index = setup_index();
        let provider = WorkspaceSymbolsProvider::new(&index);

        let results = provider.find_symbols("helper");
        assert!(!results.is_empty(), "Should find 'helper'");
        assert_eq!(results[0].name, "helper");
    }

    #[test]
    fn test_case_insensitive_exact_match() {
        let index = setup_index();
        let provider = WorkspaceSymbolsProvider::new(&index);

        let results = provider.find_symbols("HELPER");
        assert!(
            !results.is_empty(),
            "Should find 'helper' case-insensitively"
        );
        assert_eq!(results[0].name, "helper");
    }

    #[test]
    fn test_prefix_match() {
        let index = setup_index();
        let provider = WorkspaceSymbolsProvider::new(&index);

        let results = provider.find_symbols("my");
        assert!(
            results.len() >= 3,
            "Should find myFunction, myVariable, MyClass, MyOtherClass"
        );

        // All results should contain "my" (case-insensitive)
        for result in &results {
            assert!(
                result.name.to_lowercase().contains("my"),
                "'{}' should contain 'my'",
                result.name
            );
        }
    }

    #[test]
    fn test_substring_match() {
        let index = setup_index();
        let provider = WorkspaceSymbolsProvider::new(&index);

        let results = provider.find_symbols("class");
        assert!(
            results.len() >= 2,
            "Should find MyClass and MyOtherClass via substring match"
        );

        for result in &results {
            assert!(
                result.name.to_lowercase().contains("class"),
                "'{}' should contain 'class'",
                result.name
            );
        }
    }

    #[test]
    fn test_no_match() {
        let index = setup_index();
        let provider = WorkspaceSymbolsProvider::new(&index);

        let results = provider.find_symbols("nonexistent");
        assert!(
            results.is_empty(),
            "Should find no results for nonexistent query"
        );
    }

    #[test]
    fn test_relevance_sorting() {
        let mut index = SymbolIndex::new();

        // Add symbols that will match "foo" with different relevance
        index.add_definition("foo", make_location("a.ts", 0, 0, 3)); // exact
        index.add_definition("fooBar", make_location("b.ts", 0, 0, 6)); // prefix
        index.add_definition("barfoo", make_location("c.ts", 0, 0, 6)); // substring

        let provider = WorkspaceSymbolsProvider::new(&index);
        let results = provider.find_symbols("foo");

        assert_eq!(results.len(), 3);
        // Exact match first
        assert_eq!(results[0].name, "foo");
        // Prefix match second
        assert_eq!(results[1].name, "fooBar");
        // Substring match last
        assert_eq!(results[2].name, "barfoo");
    }

    #[test]
    fn test_max_results_limit() {
        let mut index = SymbolIndex::new();

        // Add more than MAX_RESULTS (100) symbols
        for i in 0..150 {
            let name = format!("symbol_{}", i);
            index.add_definition(&name, make_location("test.ts", i, 0, 10));
        }

        let provider = WorkspaceSymbolsProvider::new(&index);
        let results = provider.find_symbols("symbol");

        assert_eq!(
            results.len(),
            MAX_RESULTS,
            "Should limit to {} results",
            MAX_RESULTS
        );
    }

    #[test]
    fn test_infer_symbol_kind_class() {
        assert_eq!(
            WorkspaceSymbolsProvider::infer_symbol_kind("MyClass"),
            SymbolKind::Class
        );
    }

    #[test]
    fn test_infer_symbol_kind_interface() {
        assert_eq!(
            WorkspaceSymbolsProvider::infer_symbol_kind("IMyInterface"),
            SymbolKind::Interface
        );
    }

    #[test]
    fn test_infer_symbol_kind_constant() {
        assert_eq!(
            WorkspaceSymbolsProvider::infer_symbol_kind("MAX_COUNT"),
            SymbolKind::Constant
        );
    }

    #[test]
    fn test_infer_symbol_kind_variable() {
        assert_eq!(
            WorkspaceSymbolsProvider::infer_symbol_kind("myVariable"),
            SymbolKind::Variable
        );
    }

    #[test]
    fn test_symbol_information_fields() {
        let index = setup_index();
        let provider = WorkspaceSymbolsProvider::new(&index);

        let results = provider.find_symbols("MyClass");
        assert_eq!(results.len(), 1);

        let sym = &results[0];
        assert_eq!(sym.name, "MyClass");
        assert_eq!(sym.kind, SymbolKind::Class);
        assert_eq!(sym.location.file_path, "types.ts");
        assert_eq!(sym.location.range.start.line, 0);
        assert_eq!(sym.location.range.start.character, 0);
        assert_eq!(sym.location.range.end.character, 7);
    }

    #[test]
    fn test_multiple_definitions_same_name() {
        let mut index = SymbolIndex::new();

        // Same symbol defined in two files (e.g., declaration + .d.ts)
        index.add_definition("Config", make_location("config.ts", 0, 0, 6));
        index.add_definition("Config", make_location("config.d.ts", 0, 0, 6));

        let provider = WorkspaceSymbolsProvider::new(&index);
        let results = provider.find_symbols("Config");

        assert_eq!(results.len(), 2, "Should return both definitions");
        assert!(results.iter().all(|r| r.name == "Config"));
    }
}
