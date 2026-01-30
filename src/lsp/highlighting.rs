//! Document Highlighting implementation for LSP.
//!
//! Provides "highlight all occurrences" functionality that shows all
//! references to the symbol at the cursor position, distinguishing
//! between reads (references) and writes (assignments).

use crate::binder::BinderState;
use crate::lsp::position::{LineMap, Position, Range};
use crate::lsp::references::FindReferences;
use crate::parser::NodeIndex;
use crate::parser::node::NodeArena;

/// The kind of highlight - distinguishes between reads and writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum DocumentHighlightKind {
    /// The symbol is being read (referenced).
    Read = 1,
    /// The symbol is being written (assigned to).
    Write = 2,
    /// The symbol is being read and written (text, like +=).
    Text = 3,
}

/// A document highlight (a single occurrence of the symbol).
#[derive(Debug, Clone, serde::Serialize)]
pub struct DocumentHighlight {
    /// The range of the symbol occurrence.
    pub range: Range,
    /// The kind of highlight (read vs write).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<DocumentHighlightKind>,
}

impl DocumentHighlight {
    /// Create a new document highlight.
    pub fn new(range: Range, kind: Option<DocumentHighlightKind>) -> Self {
        Self { range, kind }
    }

    /// Create a read highlight.
    pub fn read(range: Range) -> Self {
        Self {
            range,
            kind: Some(DocumentHighlightKind::Read),
        }
    }

    /// Create a write highlight.
    pub fn write(range: Range) -> Self {
        Self {
            range,
            kind: Some(DocumentHighlightKind::Write),
        }
    }

    /// Create a text highlight (read and write).
    pub fn text(range: Range) -> Self {
        Self {
            range,
            kind: Some(DocumentHighlightKind::Text),
        }
    }
}

/// Provider for document highlighting.
pub struct DocumentHighlightProvider<'a> {
    arena: &'a NodeArena,
    binder: &'a BinderState,
    line_map: &'a LineMap,
    source_text: &'a str,
}

impl<'a> DocumentHighlightProvider<'a> {
    /// Create a new document highlight provider.
    pub fn new(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        line_map: &'a LineMap,
        source_text: &'a str,
    ) -> Self {
        Self {
            arena,
            binder,
            line_map,
            source_text,
        }
    }

    /// Get all highlights for the symbol at the given position.
    ///
    /// Returns a list of all occurrences of the symbol, each with a range
    /// and optionally a kind (read/write) to distinguish the access pattern.
    pub fn get_document_highlights(
        &self,
        root: NodeIndex,
        position: Position,
    ) -> Option<Vec<DocumentHighlight>> {
        // Use FindReferences to get all occurrences
        let finder = FindReferences::new(
            self.arena,
            self.binder,
            self.line_map,
            "<current>".to_string(),
            self.source_text,
        );

        let locations = finder.find_references(root, position)?;

        // Convert locations to highlights with read/write detection
        let highlights: Vec<DocumentHighlight> = locations
            .into_iter()
            .map(|loc| {
                let kind = self.detect_access_kind(loc.range);
                DocumentHighlight::new(loc.range, kind)
            })
            .collect();

        if highlights.is_empty() {
            None
        } else {
            Some(highlights)
        }
    }

    /// Detect whether a reference is a read or write.
    ///
    /// This is a heuristic-based approach that checks the surrounding
    /// context to determine if the identifier is being read or written.
    fn detect_access_kind(&self, range: Range) -> Option<DocumentHighlightKind> {
        let start_offset = self
            .line_map
            .position_to_offset(range.start, self.source_text)?;
        let end_offset = self
            .line_map
            .position_to_offset(range.end, self.source_text)?;

        // Look at a small window before the identifier to detect assignment
        let context_start = start_offset.saturating_sub(20);
        let context_end = if end_offset + 20 < self.source_text.len() as u32 {
            end_offset + 20
        } else {
            self.source_text.len() as u32
        };

        let context = &self.source_text[context_start as usize..context_end as usize];

        // Check for assignment patterns before the identifier
        let before = context
            .get(..(start_offset - context_start) as usize)
            .unwrap_or("");
        let after = context
            .get((end_offset - context_start) as usize..)
            .unwrap_or("");

        // Check if this is a write (assignment)
        let is_write = self.is_write_context(before, after);

        // Check if this is a compound assignment (read and write)
        let is_text = self.is_compound_assignment(before);

        if is_text {
            Some(DocumentHighlightKind::Text)
        } else if is_write {
            Some(DocumentHighlightKind::Write)
        } else {
            Some(DocumentHighlightKind::Read)
        }
    }

    /// Check if the identifier is in a write context (assignment).
    fn is_write_context(&self, before: &str, after: &str) -> bool {
        // Trim both leading and trailing whitespace from the before-context
        // so that `"let y = "` becomes `"let y ="` for correct ends_with checks.
        let before_trimmed = before.trim();

        // Check for assignment operators (=, :=, etc.)
        // But exclude comparison operators (==, ===, !=, !==) and arrow (=>)
        // and generic defaults (<T = Default>).
        if before_trimmed.ends_with('=')
            && !before_trimmed.ends_with("==")
            && !before_trimmed.ends_with("!=")
            && !before_trimmed.ends_with("=>")
            && !before_trimmed.ends_with("<=")
        {
            return true;
        }

        // Check for named compound/colon assignment operators
        if before_trimmed.ends_with(":=")
            || before_trimmed.ends_with("+=")
            || before_trimmed.ends_with("-=")
            || before_trimmed.ends_with("*=")
            || before_trimmed.ends_with("/=")
            || before_trimmed.ends_with("%=")
            || before_trimmed.ends_with("&=")
            || before_trimmed.ends_with("|=")
            || before_trimmed.ends_with("^=")
            || before_trimmed.ends_with("<<=")
            || before_trimmed.ends_with(">>=")
            || before_trimmed.ends_with(">>>=")
        {
            return true;
        }

        // Check for variable declaration keywords (var, let, const)
        // Also handles: function, class, interface, type, enum, import, catch
        let before_trimmed_lower = before_trimmed.to_lowercase();
        let words: Vec<&str> = before_trimmed_lower.split_whitespace().collect();
        if !words.is_empty() {
            let last_word = words.last().unwrap();
            if *last_word == "var"
                || *last_word == "let"
                || *last_word == "const"
                || *last_word == "function"
                || *last_word == "class"
                || *last_word == "interface"
                || *last_word == "type"
                || *last_word == "enum"
                || *last_word == "import"
                || *last_word == "catch"
            {
                return true;
            }
        }

        // Check for for-in / for-of loop variables: `for (const x of ...)` or `for (x of ...)`
        // The before context for the loop variable may end with `(` after stripping keywords
        if before_trimmed.ends_with('(') {
            // Check if the line looks like a for loop: `for (`
            let prefix = before_trimmed.trim_end_matches('(').trim_end();
            if prefix.ends_with("for") {
                return true;
            }
        }

        // Check for catch clause: `catch (`
        if before_trimmed.ends_with('(') {
            let prefix = before_trimmed.trim_end_matches('(').trim_end();
            if prefix.ends_with("catch") {
                return true;
            }
        }

        // Check for object/array literal property (identifier:) or (identifier?)
        // Covers: `{ identifier:`, `[ identifier,`, and after commas
        if before_trimmed.ends_with('{')
            || before_trimmed.ends_with('[')
            || before_trimmed.ends_with(',')
        {
            // Only true if followed by : or ? (object property pattern)
            let after_trimmed = after.trim_start();
            if after_trimmed.starts_with(':') || after_trimmed.starts_with('?') {
                return true;
            }
        }

        // Check for destructuring assignment pattern
        // { identifier } = ... or { identifier, ... } = ...
        // [ identifier ] = ... or [ identifier, ... ] = ...
        if before_trimmed.ends_with('{') || (before_trimmed.ends_with(',') && after.contains('}')) {
            // Check if the after context contains a closing brace followed by =
            let after_trimmed = after.trim_start();
            if after_trimmed.starts_with('}')
                || after_trimmed.starts_with(',')
                || after_trimmed.contains("} =")
            {
                return true;
            }
        }
        if before_trimmed.ends_with('[') || (before_trimmed.ends_with(',') && after.contains(']')) {
            let after_trimmed = after.trim_start();
            if after_trimmed.starts_with(']')
                || after_trimmed.starts_with(',')
                || after_trimmed.contains("] =")
            {
                return true;
            }
        }

        false
    }

    /// Check if this is a compound assignment (+=, -=, etc.).
    fn is_compound_assignment(&self, before: &str) -> bool {
        let before_trimmed = before.trim_end();
        before_trimmed.ends_with("+=")
            || before_trimmed.ends_with("-=")
            || before_trimmed.ends_with("*=")
            || before_trimmed.ends_with("/=")
            || before_trimmed.ends_with("%=")
            || before_trimmed.ends_with("&=")
            || before_trimmed.ends_with("|=")
            || before_trimmed.ends_with("^=")
            || before_trimmed.ends_with("<<=")
            || before_trimmed.ends_with(">>=")
            || before_trimmed.ends_with(">>>=")
    }
}

#[cfg(test)]
mod highlighting_tests {
    use super::*;
    use crate::binder::BinderState;
    use crate::lsp::position::LineMap;
    use crate::parser::ParserState;

    #[test]
    fn test_document_highlight_simple_variable() {
        let source = "let x = 1;\nlet y = x + 1;\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        // Highlight 'x' at position (0, 4) - the declaration
        let pos = Position::new(0, 4);
        let highlights = provider.get_document_highlights(root, pos);

        assert!(highlights.is_some(), "Should find highlights for 'x'");
        let highlights = highlights.unwrap();

        // Should have at least 2 occurrences: declaration and usage
        assert!(highlights.len() >= 2, "Should have at least 2 highlights");

        // All highlights should have a kind assigned
        assert!(highlights.iter().all(|h| h.kind.is_some()));
    }

    #[test]
    fn test_document_highlight_function() {
        let source = "function foo() {\n  return 1;\n}\nfoo();\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        // Highlight 'foo' at the call site (3, 0)
        let pos = Position::new(3, 0);
        let highlights = provider.get_document_highlights(root, pos);

        assert!(highlights.is_some());
        let highlights = highlights.unwrap();

        // Should have at least 2 occurrences: declaration and call
        assert!(highlights.len() >= 2, "Should have at least 2 highlights");

        // All highlights should have a kind assigned
        assert!(highlights.iter().all(|h| h.kind.is_some()));
    }

    #[test]
    fn test_document_highlight_compound_assignment() {
        let source = "let count = 0;\ncount += 1;\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        // Highlight 'count' at the compound assignment
        let pos = Position::new(1, 0);
        let highlights = provider.get_document_highlights(root, pos);

        assert!(highlights.is_some());
        let highlights = highlights.unwrap();

        // Should have at least 2 occurrences
        assert!(highlights.len() >= 2, "Should have at least 2 highlights");

        // All highlights should have a kind assigned
        assert!(highlights.iter().all(|h| h.kind.is_some()));
    }

    #[test]
    fn test_document_highlight_no_symbol() {
        let source = "let x = 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        // Position on the number literal '1', not an identifier
        let pos = Position::new(0, 8);
        let highlights = provider.get_document_highlights(root, pos);

        assert!(highlights.is_none(), "Should not highlight non-identifier");
    }

    #[test]
    fn test_document_highlight_read_kind() {
        let source = "let x = 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        // Test that we get highlights
        let pos = Position::new(0, 4);
        let highlights = provider.get_document_highlights(root, pos);
        assert!(highlights.is_some());
    }

    #[test]
    fn test_document_highlight_structs() {
        let source = "let x = 1;\nconsole.log(x);\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        let pos = Position::new(0, 4);
        let highlights = provider.get_document_highlights(root, pos);

        assert!(highlights.is_some());
        let highlights = highlights.unwrap();
        assert!(highlights.len() >= 2);
    }

    /// Standalone test helper that calls `is_write_context` on a real provider.
    fn test_is_write(source: &str, before: &str, after: &str) -> bool {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);
        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
        provider.is_write_context(before, after)
    }

    fn test_is_compound(source: &str, before: &str) -> bool {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);
        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
        provider.is_compound_assignment(before)
    }

    // ---- Tests for Bug 1 & Bug 2 fixes: duplicate conditions ----

    #[test]
    fn test_write_context_simple_assignment() {
        let src = "let x = 1;";
        // before = "x = ", after context irrelevant
        assert!(test_is_write(src, "x = ", "1;"));
    }

    #[test]
    fn test_write_context_var_declaration() {
        let src = "var x = 1;";
        assert!(test_is_write(src, "var ", "= 1;"));
    }

    #[test]
    fn test_write_context_let_declaration() {
        let src = "let x = 1;";
        assert!(test_is_write(src, "let ", "= 1;"));
    }

    #[test]
    fn test_write_context_const_declaration() {
        let src = "const x = 1;";
        assert!(test_is_write(src, "const ", "= 1;"));
    }

    // ---- Tests for false positive fixes (===, !==, =>) ----

    #[test]
    fn test_triple_equals_is_not_write() {
        let src = "if (x === y) {}";
        // before "y" is "x === ", after is ") {}"
        assert!(
            !test_is_write(src, "x === ", ") {}"),
            "=== should NOT be detected as a write"
        );
    }

    #[test]
    fn test_double_equals_is_not_write() {
        let src = "if (x == y) {}";
        assert!(
            !test_is_write(src, "x == ", ") {}"),
            "== should NOT be detected as a write"
        );
    }

    #[test]
    fn test_not_equals_is_not_write() {
        let src = "if (x !== y) {}";
        assert!(
            !test_is_write(src, "x !== ", ") {}"),
            "!== should NOT be detected as a write"
        );
    }

    #[test]
    fn test_not_double_equals_is_not_write() {
        let src = "if (x != y) {}";
        assert!(
            !test_is_write(src, "x != ", ") {}"),
            "!= should NOT be detected as a write"
        );
    }

    #[test]
    fn test_arrow_is_not_write() {
        let src = "const f = (x) => x + 1;";
        // before the body "x" after arrow is " x + 1;"
        assert!(
            !test_is_write(src, "(x) => ", "+ 1;"),
            "=> should NOT be detected as assignment"
        );
    }

    #[test]
    fn test_less_than_equals_is_not_write() {
        let src = "if (x <= y) {}";
        assert!(
            !test_is_write(src, "x <= ", ") {}"),
            "<= should NOT be detected as a write"
        );
    }

    // ---- Tests for new keyword detection: import, catch ----

    #[test]
    fn test_import_is_write() {
        let src = "import { x } from 'mod';";
        assert!(
            test_is_write(src, "import ", "} from 'mod';"),
            "import specifier should be a write"
        );
    }

    #[test]
    fn test_catch_is_write() {
        let src = "try {} catch (e) {}";
        assert!(
            test_is_write(src, "catch ", ") {}"),
            "catch clause variable should be a write"
        );
    }

    // ---- Tests for for-loop detection ----

    #[test]
    fn test_for_loop_variable_is_write() {
        let src = "let items = []; for (let x of items) {}";
        // before "x" is "for (", after is " of items) {}"
        assert!(
            test_is_write(src, "for (", " of items) {}"),
            "for-of loop variable should be a write"
        );
    }

    #[test]
    fn test_catch_paren_is_write() {
        let src = "try {} catch (e) {}";
        assert!(
            test_is_write(src, "catch (", ") {}"),
            "catch( variable should be a write"
        );
    }

    // ---- Tests for object destructuring (Bug 2 fix) ----

    #[test]
    fn test_object_destructuring_property_with_colon() {
        let src = "const { a: b } = obj;";
        // before "a" is "{ ", after is ": b } = obj;"
        assert!(
            test_is_write(src, "{ ", ": b } = obj;"),
            "Object destructuring property should be a write"
        );
    }

    #[test]
    fn test_array_destructuring_first_element() {
        let src = "const [a, b] = arr;";
        // before "a" is "[", after is ", b] = arr;"
        assert!(
            test_is_write(src, "[", ", b] = arr;"),
            "Array destructuring element should be a write"
        );
    }

    #[test]
    fn test_array_destructuring_bracket() {
        let src = "const [a] = arr;";
        assert!(
            test_is_write(src, "[", "] = arr;"),
            "Array destructuring single element should be a write"
        );
    }

    // ---- Tests for compound assignment detection ----

    #[test]
    fn test_compound_plus_equals() {
        let src = "x += 1;";
        assert!(test_is_compound(src, "x +="));
    }

    #[test]
    fn test_compound_minus_equals() {
        let src = "x -= 1;";
        assert!(test_is_compound(src, "x -="));
    }

    #[test]
    fn test_not_compound_for_simple_equals() {
        let src = "x = 1;";
        assert!(!test_is_compound(src, "x ="));
    }

    // ---- Test that function keyword is still detected ----

    #[test]
    fn test_function_declaration_is_write() {
        let src = "function foo() {}";
        assert!(test_is_write(src, "function ", "() {}"));
    }

    #[test]
    fn test_class_declaration_is_write() {
        let src = "class Foo {}";
        assert!(test_is_write(src, "class ", "{}"));
    }

    #[test]
    fn test_enum_declaration_is_write() {
        let src = "enum Color {}";
        assert!(test_is_write(src, "enum ", "{}"));
    }

    // ---- Test that plain reads are not writes ----

    #[test]
    fn test_plain_read_is_not_write() {
        let src = "console.log(x);";
        assert!(
            !test_is_write(src, "console.log(", ");"),
            "A plain read reference should not be a write"
        );
    }

    #[test]
    fn test_addition_is_not_write() {
        let src = "let z = x + y;";
        assert!(
            !test_is_write(src, "x + ", ";"),
            "Addition operand should not be a write"
        );
    }
}
