//! Fourslash-style test framework for LSP features.
//!
//! Provides a declarative test DSL inspired by TypeScript's fourslash test format.
//! Tests use markers (`/*name*/`) in source text to identify cursor positions,
//! then verify LSP features like hover, definition, completions, references, etc.
//!
//! # Example
//!
//! ```ignore
//! use tsz_lsp::fourslash::FourslashTest;
//!
//! let mut t = FourslashTest::new("
//!     const /*def*/x = 42;
//!     /*ref*/x + 1;
//! ");
//!
//! // Verify go-to-definition
//! t.go_to_definition("ref")
//!     .expect_at_marker("def");
//!
//! // Verify hover
//! t.hover("def")
//!     .expect_display_string_contains("const x: number");
//! ```

use rustc_hash::FxHashMap;

use crate::project::Project;
use tsz_common::position::{Location, Position, Range};

/// A marker position extracted from test source text.
#[derive(Debug, Clone)]
pub struct Marker {
    /// The marker name (e.g., "def" from `/*def*/`).
    pub name: String,
    /// The file this marker is in.
    pub file: String,
    /// Zero-based line number.
    pub line: u32,
    /// Zero-based column (after marker text is removed).
    pub character: u32,
    /// Byte offset in the cleaned source.
    pub offset: u32,
}

/// Result of a go-to-definition query, for fluent assertions.
pub struct DefinitionResult {
    pub locations: Option<Vec<Location>>,
    markers: FxHashMap<String, Marker>,
}

impl DefinitionResult {
    /// Assert that definition(s) were found.
    pub fn expect_found(&self) -> &Self {
        assert!(
            self.locations.as_ref().is_some_and(|v| !v.is_empty()),
            "Expected definition to be found, but got none"
        );
        self
    }

    /// Assert that no definitions were found.
    pub fn expect_none(&self) {
        assert!(
            !self.locations.as_ref().is_some_and(|v| !v.is_empty()),
            "Expected no definition, but found {:?}",
            self.locations
        );
    }

    /// Assert that at least one definition contains the given marker position.
    ///
    /// Checks that the marker falls within the range of at least one definition.
    /// This handles cases where the definition range spans the whole declaration
    /// (e.g., `function foo` has a range starting at `function`, not `foo`).
    pub fn expect_at_marker(&self, marker_name: &str) -> &Self {
        self.expect_found();
        let marker = self
            .markers
            .get(marker_name)
            .unwrap_or_else(|| panic!("Unknown marker: {marker_name}"));
        let locs = self.locations.as_ref().unwrap();

        // Check if any definition's range contains the marker position,
        // or if the definition starts at the marker, or is on the same line.
        let found = locs.iter().any(|loc| {
            if loc.file_path != marker.file {
                return false;
            }
            // Exact match at start
            if loc.range.start.line == marker.line && loc.range.start.character == marker.character
            {
                return true;
            }
            // Marker is within the definition range
            let marker_pos = (marker.line, marker.character);
            let start = (loc.range.start.line, loc.range.start.character);
            let end = (loc.range.end.line, loc.range.end.character);
            marker_pos >= start && marker_pos <= end
        });
        assert!(
            found,
            "Expected definition containing marker '{marker_name}' ({}:{},{}), but got: {locs:?}",
            marker.file, marker.line, marker.character
        );
        self
    }

    /// Assert that at least one definition is on the given line.
    pub fn expect_on_line(&self, line: u32) -> &Self {
        self.expect_found();
        let locs = self.locations.as_ref().unwrap();
        let found = locs.iter().any(|loc| loc.range.start.line == line);
        assert!(
            found,
            "Expected definition on line {line}, but got: {locs:?}"
        );
        self
    }

    /// Assert that at least one definition is in the given file.
    pub fn expect_in_file(&self, file: &str) -> &Self {
        self.expect_found();
        let locs = self.locations.as_ref().unwrap();
        let found = locs.iter().any(|loc| loc.file_path == file);
        assert!(
            found,
            "Expected definition in file '{file}', but got: {locs:?}"
        );
        self
    }

    /// Assert there are exactly N definitions.
    pub fn expect_count(&self, n: usize) -> &Self {
        let count = self.locations.as_ref().map(|l| l.len()).unwrap_or(0);
        assert_eq!(count, n, "Expected {n} definitions, got {count}");
        self
    }
}

/// Result of a hover query, for fluent assertions.
pub struct HoverResult {
    pub info: Option<crate::hover::HoverInfo>,
}

impl HoverResult {
    /// Assert hover info was found.
    pub fn expect_found(&self) -> &Self {
        assert!(self.info.is_some(), "Expected hover info, but got none");
        self
    }

    /// Assert no hover info was found.
    pub fn expect_none(&self) {
        assert!(
            self.info.is_none(),
            "Expected no hover info, but got: {:?}",
            self.info.as_ref().unwrap().display_string
        );
    }

    /// Assert the display string contains the given substring.
    pub fn expect_display_string_contains(&self, needle: &str) -> &Self {
        self.expect_found();
        let info = self.info.as_ref().unwrap();
        assert!(
            info.display_string.contains(needle),
            "Expected display string to contain '{needle}', got: '{}'",
            info.display_string
        );
        self
    }

    /// Assert the display string equals the given string exactly.
    pub fn expect_display_string(&self, expected: &str) -> &Self {
        self.expect_found();
        let info = self.info.as_ref().unwrap();
        assert_eq!(info.display_string, expected, "Display string mismatch");
        self
    }

    /// Assert the documentation contains the given substring.
    pub fn expect_documentation_contains(&self, needle: &str) -> &Self {
        self.expect_found();
        let info = self.info.as_ref().unwrap();
        assert!(
            info.documentation.contains(needle),
            "Expected documentation to contain '{needle}', got: '{}'",
            info.documentation
        );
        self
    }
}

/// Result of a find-references query, for fluent assertions.
pub struct ReferencesResult {
    pub locations: Option<Vec<Location>>,
}

impl ReferencesResult {
    /// Assert references were found.
    pub fn expect_found(&self) -> &Self {
        assert!(
            self.locations.as_ref().is_some_and(|v| !v.is_empty()),
            "Expected references, but found none"
        );
        self
    }

    /// Assert exactly N references.
    pub fn expect_count(&self, n: usize) -> &Self {
        let count = self.locations.as_ref().map(|l| l.len()).unwrap_or(0);
        assert_eq!(count, n, "Expected {n} references, got {count}");
        self
    }

    /// Assert no references were found.
    pub fn expect_none(&self) {
        let count = self.locations.as_ref().map(|l| l.len()).unwrap_or(0);
        assert_eq!(count, 0, "Expected no references, but found {count}");
    }
}

/// Result of a completions query, for fluent assertions.
pub struct CompletionsResult {
    pub items: Vec<crate::completions::CompletionItem>,
}

impl CompletionsResult {
    /// Assert completions were found.
    pub fn expect_found(&self) -> &Self {
        assert!(
            !self.items.is_empty(),
            "Expected completions, but found none"
        );
        self
    }

    /// Assert no completions were found.
    pub fn expect_none(&self) {
        assert!(
            self.items.is_empty(),
            "Expected no completions, but found {} items",
            self.items.len()
        );
    }

    /// Assert a completion with the given label exists.
    pub fn expect_contains(&self, label: &str) -> &Self {
        let found = self.items.iter().any(|item| item.label == label);
        assert!(
            found,
            "Expected completion '{label}', available: {:?}",
            self.items.iter().map(|i| &i.label).collect::<Vec<_>>()
        );
        self
    }

    /// Alias for `expect_contains` for readability in tests.
    pub fn expect_includes(&self, label: &str) -> &Self {
        self.expect_contains(label)
    }

    /// Assert a completion with the given label does NOT exist.
    pub fn expect_not_contains(&self, label: &str) -> &Self {
        let found = self.items.iter().any(|item| item.label == label);
        assert!(!found, "Expected completion '{label}' to not be present");
        self
    }

    /// Assert the number of completions equals N.
    pub fn expect_count(&self, n: usize) -> &Self {
        assert_eq!(
            self.items.len(),
            n,
            "Expected {n} completions, got {}",
            self.items.len()
        );
        self
    }

    /// Assert completions include all given labels.
    pub fn expect_contains_all(&self, labels: &[&str]) -> &Self {
        for label in labels {
            self.expect_contains(label);
        }
        self
    }
}

/// Result of a rename query, for fluent assertions.
pub struct RenameResult {
    pub result: Result<crate::rename::WorkspaceEdit, String>,
}

impl RenameResult {
    /// Assert rename succeeded.
    pub fn expect_success(&self) -> &Self {
        assert!(
            self.result.is_ok(),
            "Expected rename to succeed, got error: {:?}",
            self.result.as_ref().err()
        );
        self
    }

    /// Assert rename failed.
    pub fn expect_failure(&self) {
        assert!(
            self.result.is_err(),
            "Expected rename to fail, but it succeeded"
        );
    }

    /// Assert the rename produced edits in the given file.
    pub fn expect_edits_in_file(&self, file: &str) -> &Self {
        self.expect_success();
        let edit = self.result.as_ref().unwrap();
        assert!(
            edit.changes.contains_key(file),
            "Expected edits in file '{file}', but only have edits in: {:?}",
            edit.changes.keys().collect::<Vec<_>>()
        );
        self
    }

    /// Assert the total number of edits across all files.
    pub fn expect_total_edits(&self, n: usize) -> &Self {
        self.expect_success();
        let edit = self.result.as_ref().unwrap();
        let total: usize = edit.changes.values().map(|v| v.len()).sum();
        assert_eq!(total, n, "Expected {n} total edits, got {total}");
        self
    }
}

/// Result of a diagnostics query, for fluent assertions.
pub struct DiagnosticsResult {
    pub diagnostics: Vec<crate::diagnostics::LspDiagnostic>,
}

impl DiagnosticsResult {
    /// Assert at least one diagnostic was found.
    pub fn expect_found(&self) -> &Self {
        assert!(
            !self.diagnostics.is_empty(),
            "Expected diagnostics, but found none"
        );
        self
    }

    /// Assert no diagnostics.
    pub fn expect_none(&self) {
        assert!(
            self.diagnostics.is_empty(),
            "Expected no diagnostics, got {}:\n{}",
            self.diagnostics.len(),
            self.diagnostics
                .iter()
                .map(|d| format!("  [{:?}] {}", d.code, d.message))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// Assert exactly N diagnostics.
    pub fn expect_count(&self, n: usize) -> &Self {
        assert_eq!(
            self.diagnostics.len(),
            n,
            "Expected {n} diagnostics, got {}",
            self.diagnostics.len()
        );
        self
    }

    /// Assert a diagnostic with the given code exists.
    pub fn expect_code(&self, code: u32) -> &Self {
        let found = self.diagnostics.iter().any(|d| d.code == Some(code));
        assert!(
            found,
            "Expected diagnostic with code {code}, available: {:?}",
            self.diagnostics
                .iter()
                .filter_map(|d| d.code)
                .collect::<Vec<_>>()
        );
        self
    }

    /// Assert no diagnostic with the given code exists.
    pub fn expect_no_code(&self, code: u32) -> &Self {
        let found = self.diagnostics.iter().any(|d| d.code == Some(code));
        assert!(
            !found,
            "Expected no diagnostic with code {code}, but it was found"
        );
        self
    }

    /// Assert a diagnostic message contains the given substring.
    pub fn expect_message_contains(&self, needle: &str) -> &Self {
        let found = self.diagnostics.iter().any(|d| d.message.contains(needle));
        assert!(
            found,
            "Expected a diagnostic containing '{needle}', messages: {:?}",
            self.diagnostics
                .iter()
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );
        self
    }
}

/// Result of a document symbols query, for fluent assertions.
pub struct DocumentSymbolsResult {
    pub symbols: Vec<crate::symbols::DocumentSymbol>,
}

impl DocumentSymbolsResult {
    /// Assert symbols were found.
    pub fn expect_found(&self) -> &Self {
        assert!(
            !self.symbols.is_empty(),
            "Expected document symbols, but found none"
        );
        self
    }

    /// Assert a symbol with the given name exists.
    pub fn expect_symbol(&self, name: &str) -> &Self {
        let found = self.symbols.iter().any(|s| s.name == name)
            || self
                .symbols
                .iter()
                .any(|s| s.children.iter().any(|c| c.name == name));
        assert!(
            found,
            "Expected symbol '{name}', available: {:?}",
            self.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        self
    }

    /// Assert the top-level symbol count.
    pub fn expect_count(&self, n: usize) -> &Self {
        assert_eq!(
            self.symbols.len(),
            n,
            "Expected {n} symbols, got {}",
            self.symbols.len()
        );
        self
    }
}

/// Result of a signature help query, for fluent assertions.
pub struct SignatureHelpResult {
    pub help: Option<crate::signature_help::SignatureHelp>,
}

impl SignatureHelpResult {
    /// Assert signature help was found.
    pub fn expect_found(&self) -> &Self {
        assert!(self.help.is_some(), "Expected signature help, but got none");
        self
    }

    /// Assert no signature help.
    pub fn expect_none(&self) {
        assert!(self.help.is_none(), "Expected no signature help");
    }

    /// Assert the active parameter index.
    pub fn expect_active_parameter(&self, index: u32) -> &Self {
        self.expect_found();
        let help = self.help.as_ref().unwrap();
        assert_eq!(
            help.active_parameter, index,
            "Expected active parameter {index}, got {}",
            help.active_parameter
        );
        self
    }

    /// Assert the signature label contains the given substring.
    pub fn expect_label_contains(&self, needle: &str) -> &Self {
        self.expect_found();
        let help = self.help.as_ref().unwrap();
        let found = help.signatures.iter().any(|s| s.label.contains(needle));
        assert!(
            found,
            "Expected signature label containing '{needle}', labels: {:?}",
            help.signatures.iter().map(|s| &s.label).collect::<Vec<_>>()
        );
        self
    }

    /// Assert the number of signatures.
    pub fn expect_signature_count(&self, n: usize) -> &Self {
        self.expect_found();
        let help = self.help.as_ref().unwrap();
        assert_eq!(
            help.signatures.len(),
            n,
            "Expected {n} signatures, got {}",
            help.signatures.len()
        );
        self
    }

    /// Assert the parameter count of the first (or active) signature.
    pub fn expect_parameter_count(&self, n: usize) -> &Self {
        self.expect_found();
        let help = self.help.as_ref().unwrap();
        let sig = &help.signatures[0];
        assert_eq!(
            sig.parameters.len(),
            n,
            "Expected {n} parameters, got {}",
            sig.parameters.len()
        );
        self
    }
}

/// Result of a folding ranges query, for fluent assertions.
pub struct FoldingRangesResult {
    pub ranges: Vec<crate::editor_ranges::folding::FoldingRange>,
}

impl FoldingRangesResult {
    /// Assert folding ranges were found.
    pub fn expect_found(&self) -> &Self {
        assert!(
            !self.ranges.is_empty(),
            "Expected folding ranges, but found none"
        );
        self
    }

    /// Assert exactly N folding ranges.
    pub fn expect_count(&self, n: usize) -> &Self {
        assert_eq!(
            self.ranges.len(),
            n,
            "Expected {n} folding ranges, got {}",
            self.ranges.len()
        );
        self
    }

    /// Assert at least N folding ranges.
    pub fn expect_min_count(&self, n: usize) -> &Self {
        assert!(
            self.ranges.len() >= n,
            "Expected at least {n} folding ranges, got {}",
            self.ranges.len()
        );
        self
    }

    /// Assert a folding range spans from `start_line` to `end_line`.
    pub fn expect_range(&self, start_line: u32, end_line: u32) -> &Self {
        let found = self
            .ranges
            .iter()
            .any(|r| r.start_line == start_line && r.end_line == end_line);
        assert!(
            found,
            "Expected folding range {start_line}..{end_line}, available: {:?}",
            self.ranges
                .iter()
                .map(|r| format!("{}..{}", r.start_line, r.end_line))
                .collect::<Vec<_>>()
        );
        self
    }

    /// Assert a folding range with the given kind exists.
    pub fn expect_kind(&self, kind: &str) -> &Self {
        let found = self.ranges.iter().any(|r| r.kind.as_deref() == Some(kind));
        assert!(
            found,
            "Expected folding range with kind '{kind}', kinds: {:?}",
            self.ranges
                .iter()
                .filter_map(|r| r.kind.as_deref())
                .collect::<Vec<_>>()
        );
        self
    }
}

/// Result of a selection range query, for fluent assertions.
pub struct SelectionRangeResult {
    pub ranges: Vec<Option<crate::editor_ranges::selection_range::SelectionRange>>,
}

impl SelectionRangeResult {
    /// Assert selection ranges were found.
    pub fn expect_found(&self) -> &Self {
        assert!(
            self.ranges.iter().any(|r| r.is_some()),
            "Expected selection ranges, but all were None"
        );
        self
    }

    /// Assert the first selection range is at the expected range.
    pub fn expect_first_range(&self, start_line: u32, start_char: u32) -> &Self {
        let first = self
            .ranges
            .first()
            .and_then(|r| r.as_ref())
            .expect("No selection ranges found");
        assert_eq!(
            first.range.start.line, start_line,
            "Expected start line {start_line}, got {}",
            first.range.start.line
        );
        assert_eq!(
            first.range.start.character, start_char,
            "Expected start character {start_char}, got {}",
            first.range.start.character
        );
        self
    }

    /// Assert the selection range has a parent (can expand further).
    pub fn expect_has_parent(&self) -> &Self {
        let first = self
            .ranges
            .first()
            .and_then(|r| r.as_ref())
            .expect("No selection ranges found");
        assert!(
            first.parent.is_some(),
            "Expected selection range to have a parent"
        );
        self
    }

    /// Count the depth of selection range nesting.
    pub fn depth(&self) -> usize {
        let first = match self.ranges.first().and_then(|r| r.as_ref()) {
            Some(r) => r,
            None => return 0,
        };
        let mut count = 1;
        let mut current = first;
        while let Some(ref parent) = current.parent {
            count += 1;
            current = parent;
        }
        count
    }
}

/// Result of a document highlighting query, for fluent assertions.
pub struct DocumentHighlightResult {
    pub highlights: Option<Vec<crate::highlighting::DocumentHighlight>>,
}

impl DocumentHighlightResult {
    /// Assert highlights were found.
    pub fn expect_found(&self) -> &Self {
        assert!(
            self.highlights.as_ref().is_some_and(|v| !v.is_empty()),
            "Expected document highlights, but found none"
        );
        self
    }

    /// Assert no highlights were found.
    pub fn expect_none(&self) {
        let count = self.highlights.as_ref().map(|h| h.len()).unwrap_or(0);
        assert_eq!(count, 0, "Expected no highlights, but found {count}");
    }

    /// Assert exactly N highlights.
    pub fn expect_count(&self, n: usize) -> &Self {
        let count = self.highlights.as_ref().map(|h| h.len()).unwrap_or(0);
        assert_eq!(count, n, "Expected {n} highlights, got {count}");
        self
    }

    /// Assert at least one highlight is a write.
    pub fn expect_has_write(&self) -> &Self {
        self.expect_found();
        let found = self
            .highlights
            .as_ref()
            .unwrap()
            .iter()
            .any(|h| h.kind == Some(crate::highlighting::DocumentHighlightKind::Write));
        assert!(found, "Expected at least one write highlight");
        self
    }

    /// Assert at least one highlight is a read.
    pub fn expect_has_read(&self) -> &Self {
        self.expect_found();
        let found = self
            .highlights
            .as_ref()
            .unwrap()
            .iter()
            .any(|h| h.kind == Some(crate::highlighting::DocumentHighlightKind::Read));
        assert!(found, "Expected at least one read highlight");
        self
    }
}

/// Result of a semantic tokens query, for fluent assertions.
pub struct SemanticTokensResult {
    pub data: Vec<u32>,
}

impl SemanticTokensResult {
    /// Assert tokens were found.
    pub fn expect_found(&self) -> &Self {
        assert!(
            !self.data.is_empty(),
            "Expected semantic tokens, but found none"
        );
        self
    }

    /// Assert the total number of tokens (each token is 5 u32s).
    pub fn expect_token_count(&self, n: usize) -> &Self {
        let count = self.data.len() / 5;
        assert_eq!(count, n, "Expected {n} tokens, got {count}");
        self
    }

    /// Assert there are at least N tokens.
    pub fn expect_min_tokens(&self, n: usize) -> &Self {
        let count = self.data.len() / 5;
        assert!(count >= n, "Expected at least {n} tokens, got {count}");
        self
    }
}

/// Result of a formatting query, for fluent assertions.
pub struct FormattingResult {
    pub edits: Result<Vec<crate::formatting::TextEdit>, String>,
}

impl FormattingResult {
    /// Assert formatting succeeded.
    pub fn expect_success(&self) -> &Self {
        assert!(
            self.edits.is_ok(),
            "Expected formatting to succeed, got error: {:?}",
            self.edits.as_ref().err()
        );
        self
    }

    /// Assert formatting produced no changes (file is already formatted).
    pub fn expect_no_changes(&self) -> &Self {
        self.expect_success();
        let edits = self.edits.as_ref().unwrap();
        assert!(
            edits.is_empty(),
            "Expected no formatting changes, got {} edits",
            edits.len()
        );
        self
    }

    /// Assert formatting produced changes.
    pub fn expect_has_changes(&self) -> &Self {
        self.expect_success();
        let edits = self.edits.as_ref().unwrap();
        assert!(
            !edits.is_empty(),
            "Expected formatting changes, but got none"
        );
        self
    }
}

/// Result of a code actions query, for fluent assertions.
pub struct CodeActionsResult {
    pub actions: Option<Vec<crate::code_actions::CodeAction>>,
}

impl CodeActionsResult {
    /// Assert code actions were found.
    pub fn expect_found(&self) -> &Self {
        assert!(
            self.actions.as_ref().is_some_and(|v| !v.is_empty()),
            "Expected code actions, but found none"
        );
        self
    }

    /// Assert no code actions.
    pub fn expect_none(&self) {
        let count = self.actions.as_ref().map(|a| a.len()).unwrap_or(0);
        assert_eq!(count, 0, "Expected no code actions, but found {count}");
    }

    /// Assert a code action with the given title exists.
    pub fn expect_action(&self, title: &str) -> &Self {
        self.expect_found();
        let found = self
            .actions
            .as_ref()
            .unwrap()
            .iter()
            .any(|a| a.title.contains(title));
        assert!(
            found,
            "Expected code action containing '{title}', available: {:?}",
            self.actions
                .as_ref()
                .unwrap()
                .iter()
                .map(|a| &a.title)
                .collect::<Vec<_>>()
        );
        self
    }
}

/// Result of a workspace symbols query, for fluent assertions.
pub struct WorkspaceSymbolsResult {
    pub symbols: Vec<crate::symbols::SymbolInformation>,
}

impl WorkspaceSymbolsResult {
    /// Assert no symbols were found.
    pub fn expect_none(&self) {
        assert!(
            self.symbols.is_empty(),
            "Expected no workspace symbols, but found {}",
            self.symbols.len()
        );
    }

    /// Assert symbols were found.
    pub fn expect_found(&self) -> &Self {
        assert!(
            !self.symbols.is_empty(),
            "Expected workspace symbols, but found none"
        );
        self
    }

    /// Assert a symbol with the given name exists.
    pub fn expect_symbol(&self, name: &str) -> &Self {
        let found = self.symbols.iter().any(|s| s.name == name);
        assert!(
            found,
            "Expected symbol '{name}', available: {:?}",
            self.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        self
    }

    /// Assert exactly N symbols.
    pub fn expect_count(&self, n: usize) -> &Self {
        assert_eq!(
            self.symbols.len(),
            n,
            "Expected {n} symbols, got {}",
            self.symbols.len()
        );
        self
    }
}

/// Result of an inlay hints query, for fluent assertions.
pub struct InlayHintsResult {
    pub hints: Vec<crate::editor_decorations::inlay_hints::InlayHint>,
}

impl InlayHintsResult {
    /// Assert hints were found.
    pub fn expect_found(&self) -> &Self {
        assert!(
            !self.hints.is_empty(),
            "Expected inlay hints, but found none"
        );
        self
    }

    /// Assert exactly N hints.
    pub fn expect_count(&self, n: usize) -> &Self {
        assert_eq!(
            self.hints.len(),
            n,
            "Expected {n} inlay hints, got {}",
            self.hints.len()
        );
        self
    }

    /// Assert at least N inlay hints.
    pub fn expect_min_count(&self, n: usize) -> &Self {
        assert!(
            self.hints.len() >= n,
            "Expected at least {n} inlay hints, got {}",
            self.hints.len()
        );
        self
    }

    /// Assert a hint with the given label exists.
    pub fn expect_label(&self, label: &str) -> &Self {
        let found = self.hints.iter().any(|h| h.label.contains(label));
        assert!(
            found,
            "Expected hint containing '{label}', labels: {:?}",
            self.hints.iter().map(|h| &h.label).collect::<Vec<_>>()
        );
        self
    }
}

/// Result of a type definition query, for fluent assertions.
pub struct TypeDefinitionResult {
    pub locations: Option<Vec<Location>>,
    markers: FxHashMap<String, Marker>,
}

impl TypeDefinitionResult {
    /// Assert that type definition(s) were found.
    pub fn expect_found(&self) -> &Self {
        assert!(
            self.locations.as_ref().is_some_and(|v| !v.is_empty()),
            "Expected type definition to be found, but got none"
        );
        self
    }

    /// Assert that no type definitions were found.
    pub fn expect_none(&self) {
        assert!(
            !self.locations.as_ref().is_some_and(|v| !v.is_empty()),
            "Expected no type definition, but found {:?}",
            self.locations
        );
    }

    /// Assert that at least one type definition contains the given marker position.
    pub fn expect_at_marker(&self, marker_name: &str) -> &Self {
        self.expect_found();
        let marker = self
            .markers
            .get(marker_name)
            .unwrap_or_else(|| panic!("Unknown marker: {marker_name}"));
        let locs = self.locations.as_ref().unwrap();

        let found = locs.iter().any(|loc| {
            if loc.file_path != marker.file {
                return false;
            }
            if loc.range.start.line == marker.line && loc.range.start.character == marker.character
            {
                return true;
            }
            let marker_pos = (marker.line, marker.character);
            let start = (loc.range.start.line, loc.range.start.character);
            let end = (loc.range.end.line, loc.range.end.character);
            marker_pos >= start && marker_pos <= end
        });
        assert!(
            found,
            "Expected type definition containing marker '{marker_name}' ({}:{},{}), but got: {locs:?}",
            marker.file, marker.line, marker.character
        );
        self
    }

    /// Assert that at least one type definition is in the given file.
    pub fn expect_in_file(&self, file: &str) -> &Self {
        self.expect_found();
        let locs = self.locations.as_ref().unwrap();
        let found = locs.iter().any(|loc| loc.file_path == file);
        assert!(
            found,
            "Expected type definition in file '{file}', but got: {locs:?}"
        );
        self
    }
}

/// Result of a go-to-implementation query, for fluent assertions.
pub struct ImplementationResult {
    pub locations: Option<Vec<Location>>,
    markers: FxHashMap<String, Marker>,
}

impl ImplementationResult {
    /// Assert that implementation(s) were found.
    pub fn expect_found(&self) -> &Self {
        assert!(
            self.locations.as_ref().is_some_and(|v| !v.is_empty()),
            "Expected implementations to be found, but got none"
        );
        self
    }

    /// Assert that no implementations were found.
    pub fn expect_none(&self) {
        assert!(
            !self.locations.as_ref().is_some_and(|v| !v.is_empty()),
            "Expected no implementations, but found {:?}",
            self.locations
        );
    }

    /// Assert that at least one implementation contains the given marker position.
    pub fn expect_at_marker(&self, marker_name: &str) -> &Self {
        self.expect_found();
        let marker = self
            .markers
            .get(marker_name)
            .unwrap_or_else(|| panic!("Unknown marker: {marker_name}"));
        let locs = self.locations.as_ref().unwrap();

        let found = locs.iter().any(|loc| {
            if loc.file_path != marker.file {
                return false;
            }
            if loc.range.start.line == marker.line && loc.range.start.character == marker.character
            {
                return true;
            }
            let marker_pos = (marker.line, marker.character);
            let start = (loc.range.start.line, loc.range.start.character);
            let end = (loc.range.end.line, loc.range.end.character);
            marker_pos >= start && marker_pos <= end
        });
        assert!(
            found,
            "Expected implementation containing marker '{marker_name}' ({}:{},{}), but got: {locs:?}",
            marker.file, marker.line, marker.character
        );
        self
    }

    /// Assert there are exactly N implementations.
    pub fn expect_count(&self, n: usize) -> &Self {
        let count = self.locations.as_ref().map(|l| l.len()).unwrap_or(0);
        assert_eq!(count, n, "Expected {n} implementations, got {count}");
        self
    }
}

/// Result of a call hierarchy prepare query, for fluent assertions.
pub struct CallHierarchyPrepareResult {
    pub item: Option<crate::hierarchy::call_hierarchy::CallHierarchyItem>,
}

impl CallHierarchyPrepareResult {
    /// Assert a call hierarchy item was found.
    pub fn expect_found(&self) -> &Self {
        assert!(
            self.item.is_some(),
            "Expected call hierarchy item, but got none"
        );
        self
    }

    /// Assert no call hierarchy item was found.
    pub fn expect_none(&self) {
        assert!(
            self.item.is_none(),
            "Expected no call hierarchy item, but got: {:?}",
            self.item.as_ref().unwrap().name
        );
    }

    /// Assert the item name matches.
    pub fn expect_name(&self, expected: &str) -> &Self {
        self.expect_found();
        let item = self.item.as_ref().unwrap();
        assert_eq!(
            item.name, expected,
            "Expected call hierarchy name '{expected}', got '{}'",
            item.name
        );
        self
    }

    /// Assert the item name contains a substring.
    pub fn expect_name_contains(&self, needle: &str) -> &Self {
        self.expect_found();
        let item = self.item.as_ref().unwrap();
        assert!(
            item.name.contains(needle),
            "Expected call hierarchy name containing '{needle}', got '{}'",
            item.name
        );
        self
    }
}

/// Result of an incoming calls query, for fluent assertions.
pub struct IncomingCallsResult {
    pub calls: Vec<crate::hierarchy::call_hierarchy::CallHierarchyIncomingCall>,
}

impl IncomingCallsResult {
    /// Assert incoming calls were found.
    pub fn expect_found(&self) -> &Self {
        assert!(
            !self.calls.is_empty(),
            "Expected incoming calls, but found none"
        );
        self
    }

    /// Assert no incoming calls.
    pub fn expect_none(&self) {
        assert!(
            self.calls.is_empty(),
            "Expected no incoming calls, but found {}",
            self.calls.len()
        );
    }

    /// Assert exactly N incoming calls.
    pub fn expect_count(&self, n: usize) -> &Self {
        assert_eq!(
            self.calls.len(),
            n,
            "Expected {n} incoming calls, got {}",
            self.calls.len()
        );
        self
    }

    /// Assert an incoming call from a function with the given name.
    pub fn expect_caller(&self, name: &str) -> &Self {
        let found = self.calls.iter().any(|c| c.from.name == name);
        assert!(
            found,
            "Expected incoming call from '{name}', callers: {:?}",
            self.calls.iter().map(|c| &c.from.name).collect::<Vec<_>>()
        );
        self
    }
}

/// Result of an outgoing calls query, for fluent assertions.
pub struct OutgoingCallsResult {
    pub calls: Vec<crate::hierarchy::call_hierarchy::CallHierarchyOutgoingCall>,
}

impl OutgoingCallsResult {
    /// Assert outgoing calls were found.
    pub fn expect_found(&self) -> &Self {
        assert!(
            !self.calls.is_empty(),
            "Expected outgoing calls, but found none"
        );
        self
    }

    /// Assert no outgoing calls.
    pub fn expect_none(&self) {
        assert!(
            self.calls.is_empty(),
            "Expected no outgoing calls, but found {}",
            self.calls.len()
        );
    }

    /// Assert exactly N outgoing calls.
    pub fn expect_count(&self, n: usize) -> &Self {
        assert_eq!(
            self.calls.len(),
            n,
            "Expected {n} outgoing calls, got {}",
            self.calls.len()
        );
        self
    }

    /// Assert an outgoing call to a function with the given name.
    pub fn expect_callee(&self, name: &str) -> &Self {
        let found = self.calls.iter().any(|c| c.to.name == name);
        assert!(
            found,
            "Expected outgoing call to '{name}', callees: {:?}",
            self.calls.iter().map(|c| &c.to.name).collect::<Vec<_>>()
        );
        self
    }
}

/// Result of a type hierarchy prepare query, for fluent assertions.
pub struct TypeHierarchyPrepareResult {
    pub item: Option<crate::hierarchy::type_hierarchy::TypeHierarchyItem>,
}

impl TypeHierarchyPrepareResult {
    /// Assert a type hierarchy item was found.
    pub fn expect_found(&self) -> &Self {
        assert!(
            self.item.is_some(),
            "Expected type hierarchy item, but got none"
        );
        self
    }

    /// Assert no type hierarchy item was found.
    pub fn expect_none(&self) {
        assert!(self.item.is_none(), "Expected no type hierarchy item");
    }

    /// Assert the item name matches.
    pub fn expect_name(&self, expected: &str) -> &Self {
        self.expect_found();
        let item = self.item.as_ref().unwrap();
        assert_eq!(
            item.name, expected,
            "Expected type hierarchy name '{expected}', got '{}'",
            item.name
        );
        self
    }
}

/// Result of a supertypes/subtypes query, for fluent assertions.
pub struct TypeHierarchyItemsResult {
    pub items: Vec<crate::hierarchy::type_hierarchy::TypeHierarchyItem>,
}

impl TypeHierarchyItemsResult {
    /// Assert items were found.
    pub fn expect_found(&self) -> &Self {
        assert!(
            !self.items.is_empty(),
            "Expected type hierarchy items, but found none"
        );
        self
    }

    /// Assert no items found.
    pub fn expect_none(&self) {
        assert!(
            self.items.is_empty(),
            "Expected no type hierarchy items, but found {}",
            self.items.len()
        );
    }

    /// Assert exactly N items.
    pub fn expect_count(&self, n: usize) -> &Self {
        assert_eq!(
            self.items.len(),
            n,
            "Expected {n} type hierarchy items, got {}",
            self.items.len()
        );
        self
    }

    /// Assert an item with the given name exists.
    pub fn expect_name(&self, name: &str) -> &Self {
        let found = self.items.iter().any(|i| i.name == name);
        assert!(
            found,
            "Expected type hierarchy item '{name}', available: {:?}",
            self.items.iter().map(|i| &i.name).collect::<Vec<_>>()
        );
        self
    }
}

/// Result of a code lenses query, for fluent assertions.
pub struct CodeLensesResult {
    pub lenses: Vec<crate::editor_decorations::code_lens::CodeLens>,
}

impl CodeLensesResult {
    /// Assert code lenses were found.
    pub fn expect_found(&self) -> &Self {
        assert!(
            !self.lenses.is_empty(),
            "Expected code lenses, but found none"
        );
        self
    }

    /// Assert no code lenses.
    pub fn expect_none(&self) {
        assert!(
            self.lenses.is_empty(),
            "Expected no code lenses, but found {}",
            self.lenses.len()
        );
    }

    /// Assert exactly N code lenses.
    pub fn expect_count(&self, n: usize) -> &Self {
        assert_eq!(
            self.lenses.len(),
            n,
            "Expected {n} code lenses, got {}",
            self.lenses.len()
        );
        self
    }

    /// Assert at least N code lenses.
    pub fn expect_min_count(&self, n: usize) -> &Self {
        assert!(
            self.lenses.len() >= n,
            "Expected at least {n} code lenses, got {}",
            self.lenses.len()
        );
        self
    }
}

/// Result of a document links query, for fluent assertions.
pub struct DocumentLinksResult {
    pub links: Vec<crate::document_links::DocumentLink>,
}

impl DocumentLinksResult {
    /// Assert document links were found.
    pub fn expect_found(&self) -> &Self {
        assert!(
            !self.links.is_empty(),
            "Expected document links, but found none"
        );
        self
    }

    /// Assert no document links.
    pub fn expect_none(&self) {
        assert!(
            self.links.is_empty(),
            "Expected no document links, but found {}",
            self.links.len()
        );
    }

    /// Assert exactly N document links.
    pub fn expect_count(&self, n: usize) -> &Self {
        assert_eq!(
            self.links.len(),
            n,
            "Expected {n} document links, got {}",
            self.links.len()
        );
        self
    }

    /// Assert at least N document links.
    pub fn expect_min_count(&self, n: usize) -> &Self {
        assert!(
            self.links.len() >= n,
            "Expected at least {n} document links, got {}",
            self.links.len()
        );
        self
    }

    /// Assert a link with the given target exists.
    pub fn expect_target(&self, target: &str) -> &Self {
        let found = self
            .links
            .iter()
            .any(|l| l.target.as_deref() == Some(target));
        assert!(
            found,
            "Expected document link with target '{target}', targets: {:?}",
            self.links
                .iter()
                .filter_map(|l| l.target.as_deref())
                .collect::<Vec<_>>()
        );
        self
    }
}

/// Result of a linked editing ranges query, for fluent assertions.
pub struct LinkedEditingResult {
    pub ranges: Option<crate::rename::linked_editing::LinkedEditingRanges>,
}

impl LinkedEditingResult {
    /// Assert linked editing ranges were found.
    pub fn expect_found(&self) -> &Self {
        assert!(
            self.ranges.is_some(),
            "Expected linked editing ranges, but got none"
        );
        self
    }

    /// Assert no linked editing ranges.
    pub fn expect_none(&self) {
        assert!(self.ranges.is_none(), "Expected no linked editing ranges");
    }

    /// Assert the number of linked ranges.
    pub fn expect_range_count(&self, n: usize) -> &Self {
        self.expect_found();
        let ranges = &self.ranges.as_ref().unwrap().ranges;
        assert_eq!(
            ranges.len(),
            n,
            "Expected {n} linked editing ranges, got {}",
            ranges.len()
        );
        self
    }
}

/// Parse markers from source text and return (`cleaned_source`, markers).
///
/// Markers have the format `/*name*/` where `name` is the marker identifier.
/// The anonymous marker `/**/` gets the name `""`.
fn parse_markers(file: &str, source: &str) -> (String, Vec<Marker>) {
    let mut cleaned = String::with_capacity(source.len());
    let mut markers = Vec::new();
    let mut i = 0;
    let bytes = source.as_bytes();
    let mut offset: u32 = 0;

    while i < bytes.len() {
        if i + 3 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            // Check for marker pattern: /*name*/
            if let Some(end) = find_marker_end(&bytes[i + 2..]) {
                let name_bytes = &bytes[i + 2..i + 2 + end];
                let name = String::from_utf8_lossy(name_bytes).to_string();
                // Calculate position from cleaned text
                let (line, character) = offset_to_line_col(&cleaned, offset);
                markers.push(Marker {
                    name,
                    file: file.to_string(),
                    line,
                    character,
                    offset,
                });
                i += 2 + end + 2; // skip /*name*/
                continue;
            }
        }
        cleaned.push(bytes[i] as char);
        offset += 1;
        i += 1;
    }

    (cleaned, markers)
}

/// Find the end of a marker name (position of `*/` relative to start).
fn find_marker_end(bytes: &[u8]) -> Option<usize> {
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
            // Only match if marker name is "valid" (no spaces, not a multi-line comment)
            let name = &bytes[..i];
            if name.iter().any(|&b| b == b'\n' || b == b'\r') {
                return None;
            }
            return Some(i);
        }
    }
    None
}

/// Convert byte offset to (line, character) in cleaned text.
fn offset_to_line_col(text: &str, offset: u32) -> (u32, u32) {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in text.chars().enumerate() {
        if i as u32 == offset {
            return (line, col);
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Parse multi-file test content.
///
/// Multi-file tests use `// @filename: path.ts` directives to separate files:
/// ```text
/// // @filename: a.ts
/// export const x = 1;
/// // @filename: b.ts
/// import { x } from "./a";
/// /*ref*/x;
/// ```
fn parse_multi_file(content: &str) -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = Vec::new();
    let mut current_file = String::new();
    let mut current_content = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(filename) = trimmed
            .strip_prefix("// @filename:")
            .or_else(|| trimmed.strip_prefix("//@filename:"))
        {
            if !current_file.is_empty() {
                files.push((current_file, current_content));
                current_content = String::new();
            }
            current_file = filename.trim().to_string();
        } else {
            if !current_content.is_empty() {
                current_content.push('\n');
            }
            current_content.push_str(line);
        }
    }

    if !current_file.is_empty() {
        files.push((current_file, current_content));
    }

    // If no @filename directives were found, treat the whole thing as a single file
    if files.is_empty() {
        files.push(("test.ts".to_string(), content.to_string()));
    }

    files
}

/// The main fourslash test harness.
///
/// Provides a fluent API for declaring tests with marker positions and verifying
/// LSP feature behavior.
pub struct FourslashTest {
    project: Project,
    markers: FxHashMap<String, Marker>,
    /// All markers in order of appearance.
    marker_list: Vec<Marker>,
    /// Map from file name to cleaned source text.
    file_sources: FxHashMap<String, String>,
}

impl FourslashTest {
    /// Create a new test from source text with markers.
    ///
    /// Source can contain `/*marker_name*/` markers that identify cursor positions.
    /// Leading/trailing whitespace in source is trimmed.
    ///
    /// # Example
    /// ```ignore
    /// let t = FourslashTest::new("
    ///     const /*def*/x = 42;
    ///     /*ref*/x;
    /// ");
    /// ```
    pub fn new(source: &str) -> Self {
        let dedented = dedent(source);
        Self::from_content(&dedented)
    }

    /// Create a new test from raw content (no dedenting).
    pub fn from_content(content: &str) -> Self {
        let raw_files = parse_multi_file(content);
        let mut project = Project::new();
        let mut all_markers = FxHashMap::default();
        let mut marker_list = Vec::new();
        let mut file_sources = FxHashMap::default();

        for (file_name, raw_source) in &raw_files {
            let (cleaned, markers) = parse_markers(file_name, raw_source);
            project.set_file(file_name.clone(), cleaned.clone());
            file_sources.insert(file_name.clone(), cleaned);
            for marker in markers {
                marker_list.push(marker.clone());
                all_markers.insert(marker.name.clone(), marker);
            }
        }

        Self {
            project,
            markers: all_markers,
            marker_list,
            file_sources,
        }
    }

    /// Create a multi-file test.
    ///
    /// Each tuple is (`file_name`, `source_with_markers`).
    ///
    /// # Example
    /// ```ignore
    /// let t = FourslashTest::multi_file(&[
    ///     ("a.ts", "export const /*def*/x = 1;"),
    ///     ("b.ts", "import { /*ref*/x } from './a';"),
    /// ]);
    /// ```
    pub fn multi_file(files: &[(&str, &str)]) -> Self {
        let mut project = Project::new();
        let mut all_markers = FxHashMap::default();
        let mut marker_list = Vec::new();
        let mut file_sources = FxHashMap::default();

        for (file_name, raw_source) in files {
            let (cleaned, markers) = parse_markers(file_name, raw_source);
            project.set_file(file_name.to_string(), cleaned.clone());
            file_sources.insert(file_name.to_string(), cleaned);
            for marker in markers {
                marker_list.push(marker.clone());
                all_markers.insert(marker.name.clone(), marker);
            }
        }

        Self {
            project,
            markers: all_markers,
            marker_list,
            file_sources,
        }
    }

    /// Get a marker by name. Panics if not found.
    pub fn marker(&self, name: &str) -> &Marker {
        self.markers.get(name).unwrap_or_else(|| {
            let available: Vec<_> = self.markers.keys().collect();
            panic!("Marker '{name}' not found. Available markers: {available:?}");
        })
    }

    /// Get the position for a marker.
    pub fn marker_position(&self, name: &str) -> Position {
        let m = self.marker(name);
        Position::new(m.line, m.character)
    }

    /// Get the file name for a marker.
    pub fn marker_file(&self, name: &str) -> &str {
        &self.marker(name).file
    }

    /// Get all marker names.
    pub fn marker_names(&self) -> Vec<&str> {
        self.markers.keys().map(|s| s.as_str()).collect()
    }

    /// Access the underlying project (for advanced operations).
    pub const fn project(&mut self) -> &mut Project {
        &mut self.project
    }

    // ─── LSP Feature Queries ─────────────────────────────────────────

    /// Go to definition at a marker.
    pub fn go_to_definition(&mut self, marker_name: &str) -> DefinitionResult {
        let m = self.marker(marker_name).clone();
        let pos = Position::new(m.line, m.character);
        let locations = self.project.get_definition(&m.file, pos);
        DefinitionResult {
            locations,
            markers: self.markers.clone(),
        }
    }

    /// Get hover info at a marker.
    pub fn hover(&mut self, marker_name: &str) -> HoverResult {
        let m = self.marker(marker_name).clone();
        let pos = Position::new(m.line, m.character);
        let info = self.project.get_hover(&m.file, pos);
        HoverResult { info }
    }

    /// Get completions at a marker.
    pub fn completions(&mut self, marker_name: &str) -> CompletionsResult {
        let m = self.marker(marker_name).clone();
        let pos = Position::new(m.line, m.character);
        let items = self
            .project
            .get_completions(&m.file, pos)
            .unwrap_or_default();
        CompletionsResult { items }
    }

    /// Find references at a marker.
    pub fn references(&mut self, marker_name: &str) -> ReferencesResult {
        let m = self.marker(marker_name).clone();
        let pos = Position::new(m.line, m.character);
        let locations = self.project.find_references(&m.file, pos);
        ReferencesResult { locations }
    }

    /// Rename at a marker with a new name.
    pub fn rename(&mut self, marker_name: &str, new_name: &str) -> RenameResult {
        let m = self.marker(marker_name).clone();
        let pos = Position::new(m.line, m.character);
        let result = self
            .project
            .get_rename_edits(&m.file, pos, new_name.to_string());
        RenameResult { result }
    }

    /// Get diagnostics for a file.
    pub fn diagnostics(&mut self, file: &str) -> DiagnosticsResult {
        let diagnostics = self.project.get_diagnostics(file).unwrap_or_default();
        DiagnosticsResult { diagnostics }
    }

    /// Get diagnostics for the primary file (first file or "test.ts").
    pub fn diagnostics_for_primary(&mut self) -> DiagnosticsResult {
        let file = self
            .file_sources
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| "test.ts".to_string());
        self.diagnostics(&file)
    }

    /// Get document symbols for a file.
    pub fn document_symbols(&mut self, file: &str) -> DocumentSymbolsResult {
        let symbols = self.project.get_document_symbols(file).unwrap_or_default();
        DocumentSymbolsResult { symbols }
    }

    /// Get signature help at a marker.
    pub fn signature_help(&mut self, marker_name: &str) -> SignatureHelpResult {
        let m = self.marker(marker_name).clone();
        let pos = Position::new(m.line, m.character);
        let help = self.project.get_signature_help(&m.file, pos);
        SignatureHelpResult { help }
    }

    /// Get folding ranges for a file.
    pub fn folding_ranges(&self, file: &str) -> FoldingRangesResult {
        let ranges = self.project.get_folding_ranges(file).unwrap_or_default();
        FoldingRangesResult { ranges }
    }

    /// Get selection ranges at a marker.
    pub fn selection_range(&self, marker_name: &str) -> SelectionRangeResult {
        let m = self.markers.get(marker_name).unwrap_or_else(|| {
            panic!("Marker '{marker_name}' not found");
        });
        let pos = Position::new(m.line, m.character);
        let ranges = self
            .project
            .get_selection_ranges(&m.file, &[pos])
            .unwrap_or_default();
        SelectionRangeResult { ranges }
    }

    /// Get document highlights at a marker.
    pub fn document_highlights(&self, marker_name: &str) -> DocumentHighlightResult {
        let m = self.markers.get(marker_name).unwrap_or_else(|| {
            panic!("Marker '{marker_name}' not found");
        });
        let pos = Position::new(m.line, m.character);
        let highlights = self.project.get_document_highlighting(&m.file, pos);
        DocumentHighlightResult { highlights }
    }

    /// Get semantic tokens for a file.
    pub fn semantic_tokens(&self, file: &str) -> SemanticTokensResult {
        let data = self
            .project
            .get_semantic_tokens_full(file)
            .unwrap_or_default();
        SemanticTokensResult { data }
    }

    /// Format a document.
    pub fn format(&self, file: &str) -> FormattingResult {
        let options = crate::formatting::FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            trim_trailing_whitespace: Some(true),
            insert_final_newline: Some(true),
            trim_final_newlines: Some(true),
            semicolons: None,
        };
        let edits = self
            .project
            .format_document(file, &options)
            .unwrap_or_else(|| Ok(vec![]));
        FormattingResult { edits }
    }

    /// Get code actions for a range at a marker.
    pub fn code_actions(&self, file: &str) -> CodeActionsResult {
        let range = Range {
            start: Position::new(0, 0),
            end: Position::new(u32::MAX, 0),
        };
        let actions = self.project.get_code_actions(file, range, vec![], None);
        CodeActionsResult { actions }
    }

    /// Get workspace symbols matching a query.
    pub fn workspace_symbols(&self, query: &str) -> WorkspaceSymbolsResult {
        let symbols = self.project.get_workspace_symbols(query);
        WorkspaceSymbolsResult { symbols }
    }

    /// Get inlay hints for the full file.
    pub fn inlay_hints(&self, file: &str) -> InlayHintsResult {
        let range = Range {
            start: Position::new(0, 0),
            end: Position::new(u32::MAX, 0),
        };
        let hints = self
            .project
            .get_inlay_hints(file, range)
            .unwrap_or_default();
        InlayHintsResult { hints }
    }

    /// Go to type definition at a marker.
    pub fn go_to_type_definition(&self, marker_name: &str) -> TypeDefinitionResult {
        let m = self.markers.get(marker_name).unwrap_or_else(|| {
            panic!("Marker '{marker_name}' not found");
        });
        let pos = Position::new(m.line, m.character);
        let locations = self.project.get_type_definition(&m.file, pos);
        TypeDefinitionResult {
            locations,
            markers: self.markers.clone(),
        }
    }

    /// Go to implementation at a marker.
    pub fn go_to_implementation(&mut self, marker_name: &str) -> ImplementationResult {
        let m = self.marker(marker_name).clone();
        let pos = Position::new(m.line, m.character);
        let locations = self.project.get_implementations(&m.file, pos);
        ImplementationResult {
            locations,
            markers: self.markers.clone(),
        }
    }

    /// Prepare call hierarchy at a marker.
    pub fn prepare_call_hierarchy(&self, marker_name: &str) -> CallHierarchyPrepareResult {
        let m = self.markers.get(marker_name).unwrap_or_else(|| {
            panic!("Marker '{marker_name}' not found");
        });
        let pos = Position::new(m.line, m.character);
        let item = self.project.prepare_call_hierarchy(&m.file, pos);
        CallHierarchyPrepareResult { item }
    }

    /// Get incoming calls at a marker.
    pub fn incoming_calls(&self, marker_name: &str) -> IncomingCallsResult {
        let m = self.markers.get(marker_name).unwrap_or_else(|| {
            panic!("Marker '{marker_name}' not found");
        });
        let pos = Position::new(m.line, m.character);
        let calls = self.project.get_incoming_calls(&m.file, pos);
        IncomingCallsResult { calls }
    }

    /// Get outgoing calls at a marker.
    pub fn outgoing_calls(&self, marker_name: &str) -> OutgoingCallsResult {
        let m = self.markers.get(marker_name).unwrap_or_else(|| {
            panic!("Marker '{marker_name}' not found");
        });
        let pos = Position::new(m.line, m.character);
        let calls = self.project.get_outgoing_calls(&m.file, pos);
        OutgoingCallsResult { calls }
    }

    /// Prepare type hierarchy at a marker.
    pub fn prepare_type_hierarchy(&self, marker_name: &str) -> TypeHierarchyPrepareResult {
        let m = self.markers.get(marker_name).unwrap_or_else(|| {
            panic!("Marker '{marker_name}' not found");
        });
        let pos = Position::new(m.line, m.character);
        let item = self.project.prepare_type_hierarchy(&m.file, pos);
        TypeHierarchyPrepareResult { item }
    }

    /// Get supertypes at a marker.
    pub fn supertypes(&self, marker_name: &str) -> TypeHierarchyItemsResult {
        let m = self.markers.get(marker_name).unwrap_or_else(|| {
            panic!("Marker '{marker_name}' not found");
        });
        let pos = Position::new(m.line, m.character);
        let items = self.project.supertypes(&m.file, pos);
        TypeHierarchyItemsResult { items }
    }

    /// Get subtypes at a marker.
    pub fn subtypes(&self, marker_name: &str) -> TypeHierarchyItemsResult {
        let m = self.markers.get(marker_name).unwrap_or_else(|| {
            panic!("Marker '{marker_name}' not found");
        });
        let pos = Position::new(m.line, m.character);
        let items = self.project.subtypes(&m.file, pos);
        TypeHierarchyItemsResult { items }
    }

    /// Get code lenses for a file.
    pub fn code_lenses(&self, file: &str) -> CodeLensesResult {
        let lenses = self.project.get_code_lenses(file).unwrap_or_default();
        CodeLensesResult { lenses }
    }

    /// Get document links for a file.
    pub fn document_links(&self, file: &str) -> DocumentLinksResult {
        let links = self.project.get_document_links(file).unwrap_or_default();
        DocumentLinksResult { links }
    }

    /// Get linked editing ranges at a marker.
    pub fn linked_editing_ranges(&self, marker_name: &str) -> LinkedEditingResult {
        let m = self.markers.get(marker_name).unwrap_or_else(|| {
            panic!("Marker '{marker_name}' not found");
        });
        let pos = Position::new(m.line, m.character);
        let ranges = self.project.get_linked_editing_ranges(&m.file, pos);
        LinkedEditingResult { ranges }
    }

    /// Get code actions at a marker position (with range from marker to end of line).
    pub fn code_actions_at(&self, marker_name: &str) -> CodeActionsResult {
        let m = self.markers.get(marker_name).unwrap_or_else(|| {
            panic!("Marker '{marker_name}' not found");
        });
        let range = Range {
            start: Position::new(m.line, m.character),
            end: Position::new(m.line, u32::MAX),
        };
        let actions = self.project.get_code_actions(&m.file, range, vec![], None);
        CodeActionsResult { actions }
    }

    /// Edit a file's content (for testing edit-related features).
    pub fn edit_file(&mut self, file: &str, new_source: &str) {
        let (cleaned, new_markers) = parse_markers(file, new_source);
        self.project.set_file(file.to_string(), cleaned.clone());
        self.file_sources.insert(file.to_string(), cleaned);
        for marker in new_markers {
            self.marker_list.push(marker.clone());
            self.markers.insert(marker.name.clone(), marker);
        }
    }

    /// Get the cleaned source text for a file.
    pub fn source(&self, file: &str) -> &str {
        self.file_sources
            .get(file)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    /// Verify that go-to-definition at one marker leads to another marker.
    /// Convenience for the common pattern: `t.go_to_definition("ref").expect_at_marker("def")`.
    pub fn verify_definition(&mut self, from: &str, to: &str) {
        self.go_to_definition(from).expect_at_marker(to);
    }

    /// Verify hover at a marker contains the expected text.
    pub fn verify_hover_contains(&mut self, marker: &str, expected: &str) {
        self.hover(marker).expect_display_string_contains(expected);
    }

    /// Verify completions at a marker include the expected labels.
    pub fn verify_completions_include(&mut self, marker: &str, expected: &[&str]) {
        self.completions(marker).expect_contains_all(expected);
    }

    /// Verify type definition at one marker leads to another marker.
    pub fn verify_type_definition(&self, from: &str, to: &str) {
        self.go_to_type_definition(from).expect_at_marker(to);
    }

    /// Verify that a diagnostic with a specific code exists.
    pub fn verify_diagnostic_code(&mut self, file: &str, code: u32) {
        self.diagnostics(file).expect_code(code);
    }

    /// Verify that diagnostics at a file are empty (no errors).
    pub fn verify_no_errors(&mut self, file: &str) {
        self.diagnostics(file).expect_none();
    }

    /// Verify references count at a marker.
    pub fn verify_references_count(&mut self, marker: &str, count: usize) {
        self.references(marker).expect_count(count);
    }

    /// Get the file count in the project.
    pub fn file_count(&self) -> usize {
        self.file_sources.len()
    }

    /// Remove a file from the project.
    pub fn remove_file(&mut self, file: &str) {
        self.project.remove_file(file);
        self.file_sources.remove(file);
        // Remove markers from the removed file
        self.markers.retain(|_, m| m.file != file);
        self.marker_list.retain(|m| m.file != file);
    }

    /// Set strict mode for the project.
    pub fn set_strict(&mut self, strict: bool) {
        self.project.set_strict(strict);
    }
}

/// Remove common leading whitespace from a multi-line string.
///
/// This allows tests to be written with natural indentation:
/// ```ignore
/// let t = FourslashTest::new("
///     const x = 1;
///     x + 1;
/// ");
/// ```
fn dedent(s: &str) -> String {
    let lines: Vec<&str> = s.lines().collect();

    // Find minimum indentation (ignoring empty lines and the first/last if empty)
    let min_indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    let result: Vec<&str> = lines
        .iter()
        .map(|line| {
            if line.len() >= min_indent {
                &line[min_indent..]
            } else {
                line.trim()
            }
        })
        .collect();

    // Trim leading and trailing empty lines
    let start = result.iter().position(|l| !l.is_empty()).unwrap_or(0);
    let end = result
        .iter()
        .rposition(|l| !l.is_empty())
        .map(|i| i + 1)
        .unwrap_or(0);

    if start >= end {
        return String::new();
    }

    result[start..end].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_markers_simple() {
        let (cleaned, markers) = parse_markers("test.ts", "const /*def*/x = 42;");
        assert_eq!(cleaned, "const x = 42;");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].name, "def");
        assert_eq!(markers[0].character, 6); // position of 'x'
    }

    #[test]
    fn test_parse_markers_anonymous() {
        let (cleaned, markers) = parse_markers("test.ts", "foo(/**/);");
        assert_eq!(cleaned, "foo();");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].name, "");
    }

    #[test]
    fn test_parse_markers_multiple() {
        let (cleaned, markers) = parse_markers("test.ts", "/*a*/x + /*b*/y");
        assert_eq!(cleaned, "x + y");
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].name, "a");
        assert_eq!(markers[1].name, "b");
    }

    #[test]
    fn test_parse_markers_multiline() {
        let (cleaned, markers) = parse_markers("test.ts", "const /*def*/x = 1;\n/*ref*/x;");
        assert_eq!(cleaned, "const x = 1;\nx;");
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].name, "def");
        assert_eq!(markers[0].line, 0);
        assert_eq!(markers[0].character, 6);
        assert_eq!(markers[1].name, "ref");
        assert_eq!(markers[1].line, 1);
        assert_eq!(markers[1].character, 0);
    }

    #[test]
    fn test_parse_multi_file() {
        let content =
            "// @filename: a.ts\nexport const x = 1;\n// @filename: b.ts\nimport { x } from './a';";
        let files = parse_multi_file(content);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].0, "a.ts");
        assert_eq!(files[0].1, "export const x = 1;");
        assert_eq!(files[1].0, "b.ts");
        assert_eq!(files[1].1, "import { x } from './a';");
    }

    #[test]
    fn test_dedent() {
        let input = "
            const x = 1;
            x + 1;
        ";
        let result = dedent(input);
        assert_eq!(result, "const x = 1;\nx + 1;");
    }

    #[test]
    fn test_fourslash_go_to_definition() {
        let mut t = FourslashTest::new(
            "
            const /*def*/x = 1;
            /*ref*/x + 1;
        ",
        );
        t.go_to_definition("ref").expect_at_marker("def");
    }

    #[test]
    fn test_fourslash_hover() {
        let mut t = FourslashTest::new(
            "
            const /*x*/x = 42;
        ",
        );
        t.hover("x").expect_found();
    }

    #[test]
    fn test_fourslash_references() {
        let mut t = FourslashTest::new(
            "
            const /*def*/x = 1;
            /*ref1*/x + /*ref2*/x;
        ",
        );
        // Should find references (the definition + usages)
        t.references("def").expect_found();
    }

    #[test]
    fn test_fourslash_multi_file() {
        let mut t = FourslashTest::multi_file(&[
            ("a.ts", "export const x = 1;"),
            ("b.ts", "const /*def*/y = 2;\n/*ref*/y;"),
        ]);
        // Definition within same file should work
        t.go_to_definition("ref").expect_at_marker("def");
    }

    #[test]
    fn test_fourslash_document_symbols() {
        let mut t = FourslashTest::new(
            "
            function foo() {}
            class Bar {}
            const baz = 1;
        ",
        );
        t.document_symbols("test.ts")
            .expect_found()
            .expect_symbol("foo")
            .expect_symbol("Bar")
            .expect_symbol("baz");
    }

    #[test]
    fn test_fourslash_completions() {
        let mut t = FourslashTest::new(
            "
            const myVariable = 42;
            /**/my
        ",
        );
        // At the marker position, we should get completions including our variable
        let result = t.completions("");
        // Completions may or may not include myVariable depending on implementation
        // This just verifies the framework works
        // Framework test - completions query should work without panic
        let _ = result.items.len();
    }

    #[test]
    fn test_fourslash_rename() {
        let mut t = FourslashTest::new(
            "
            const /*x*/x = 1;
            x + x;
        ",
        );
        t.rename("x", "y")
            .expect_success()
            .expect_edits_in_file("test.ts");
    }

    #[test]
    fn test_fourslash_at_filename_parsing() {
        let t = FourslashTest::from_content(
            "// @filename: utils.ts\nexport function /*def*/helper() {}\n// @filename: main.ts\nimport { /*ref*/helper } from './utils';\nhelper();",
        );
        // Verify markers were parsed in correct files
        assert_eq!(t.marker_file("def"), "utils.ts");
        assert_eq!(t.marker_file("ref"), "main.ts");
    }
}
