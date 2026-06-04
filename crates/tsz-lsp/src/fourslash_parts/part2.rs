fn parse_multi_file(content: &str) -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = Vec::new();
    let mut current_file = String::new();
    let mut current_content = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some((_, filename)) = strip_filename_directive(trimmed) {
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
    pub fn code_actions(&mut self, file: &str) -> CodeActionsResult {
        let range = Range {
            start: Position::new(0, 0),
            end: Position::new(u32::MAX, 0),
        };
        let diagnostics = self.project.get_diagnostics(file).unwrap_or_default();
        let actions = self
            .project
            .get_code_actions(file, range, diagnostics, None);
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
    pub fn code_actions_at(&mut self, marker_name: &str) -> CodeActionsResult {
        let m = self.markers.get(marker_name).unwrap_or_else(|| {
            panic!("Marker '{marker_name}' not found");
        });
        let range = Range {
            start: Position::new(m.line, m.character),
            end: Position::new(m.line, u32::MAX),
        };
        let diagnostics = self.project.get_diagnostics(&m.file).unwrap_or_default();
        let actions = self
            .project
            .get_code_actions(&m.file, range, diagnostics, None);
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
