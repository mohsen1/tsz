//! LSP Code Lens implementation.
//!
//! Code Lens displays actionable information above code elements, such as:
//! - Reference counts: "3 references" above function declarations
//! - Test runners: "Run Test | Debug Test" above test functions
//! - Implementation counts: "2 implementations" above interfaces
//!
//! Code Lenses are displayed inline in the editor and can be clicked to
//! perform actions like "Show References" or "Run Test".

use crate::binder::BinderState;
use crate::lsp::position::{LineMap, Position, Range};
use crate::lsp::references::FindReferences;
use crate::parser::node::NodeArena;
use crate::parser::{syntax_kind_ext, NodeIndex};

/// A code lens represents a command that can be shown inline with source code.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CodeLens {
    /// The range at which this code lens is displayed.
    pub range: Range,
    /// The command this code lens represents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<CodeLensCommand>,
    /// Additional data that uniquely identifies this code lens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<CodeLensData>,
}

/// A command that can be executed when a code lens is clicked.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CodeLensCommand {
    /// The title of the command (displayed to the user).
    pub title: String,
    /// The identifier of the command to execute.
    pub command: String,
    /// Arguments to pass to the command.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<serde_json::Value>>,
}

/// Additional data for code lens resolution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CodeLensData {
    /// The file path.
    pub file_path: String,
    /// The kind of code lens.
    pub kind: CodeLensKind,
    /// The position of the symbol.
    pub position: Position,
}

/// The kind of code lens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CodeLensKind {
    /// Show references to this symbol.
    References,
    /// Show implementations of this interface/abstract class.
    Implementations,
    /// Run this test.
    RunTest,
    /// Debug this test.
    DebugTest,
}

impl CodeLens {
    /// Create a new code lens without a command (requires resolution).
    pub fn new(range: Range, data: CodeLensData) -> Self {
        Self {
            range,
            command: None,
            data: Some(data),
        }
    }

    /// Create a resolved code lens with a command.
    pub fn resolved(range: Range, command: CodeLensCommand) -> Self {
        Self {
            range,
            command: Some(command),
            data: None,
        }
    }
}

/// Provider for code lenses.
pub struct CodeLensProvider<'a> {
    arena: &'a NodeArena,
    binder: &'a BinderState,
    line_map: &'a LineMap,
    file_name: String,
    source_text: &'a str,
}

impl<'a> CodeLensProvider<'a> {
    /// Create a new code lens provider.
    pub fn new(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        line_map: &'a LineMap,
        file_name: String,
        source_text: &'a str,
    ) -> Self {
        Self {
            arena,
            binder,
            line_map,
            file_name,
            source_text,
        }
    }

    /// Get all code lenses for the document.
    ///
    /// Returns unresolved code lenses that can be resolved later.
    /// Typically, code lenses are returned quickly without computing
    /// expensive data (like reference counts), and then resolved
    /// when they become visible.
    pub fn provide_code_lenses(&self, _root: NodeIndex) -> Vec<CodeLens> {
        let mut lenses = Vec::new();

        // Collect code lenses for various declaration types
        for (i, node) in self.arena.nodes.iter().enumerate() {
            let node_idx = NodeIndex(i as u32);

            match node.kind {
                // Functions
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    if let Some(lens) = self.create_reference_lens(node_idx) {
                        lenses.push(lens);
                    }
                }

                // Methods
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(lens) = self.create_reference_lens(node_idx) {
                        lenses.push(lens);
                    }
                }

                // Classes
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    if let Some(lens) = self.create_reference_lens(node_idx) {
                        lenses.push(lens);
                    }
                }

                // Interfaces - show implementations lens
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    if let Some(lens) = self.create_reference_lens(node_idx) {
                        lenses.push(lens);
                    }
                    if let Some(lens) = self.create_implementations_lens(node_idx) {
                        lenses.push(lens);
                    }
                }

                // Type aliases
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                    if let Some(lens) = self.create_reference_lens(node_idx) {
                        lenses.push(lens);
                    }
                }

                // Enums
                k if k == syntax_kind_ext::ENUM_DECLARATION => {
                    if let Some(lens) = self.create_reference_lens(node_idx) {
                        lenses.push(lens);
                    }
                }

                _ => {}
            }
        }

        lenses
    }

    /// Resolve a code lens by computing its command.
    ///
    /// This is called when a code lens becomes visible to compute
    /// expensive data like reference counts.
    pub fn resolve_code_lens(&self, root: NodeIndex, lens: &CodeLens) -> Option<CodeLens> {
        let data = lens.data.as_ref()?;

        match data.kind {
            CodeLensKind::References => self.resolve_references_lens(root, lens, data),
            CodeLensKind::Implementations => self.resolve_implementations_lens(lens, data),
            CodeLensKind::RunTest | CodeLensKind::DebugTest => {
                // Test lenses are typically resolved immediately
                Some(lens.clone())
            }
        }
    }

    /// Create a "references" code lens for a declaration.
    fn create_reference_lens(&self, decl_idx: NodeIndex) -> Option<CodeLens> {
        let node = self.arena.get(decl_idx)?;
        let start = self.line_map.offset_to_position(node.pos, self.source_text);

        // Place the lens at the start of the declaration
        let range = Range::new(start, Position::new(start.line, start.character + 1));

        let data = CodeLensData {
            file_path: self.file_name.clone(),
            kind: CodeLensKind::References,
            position: start,
        };

        Some(CodeLens::new(range, data))
    }

    /// Create an "implementations" code lens for an interface.
    fn create_implementations_lens(&self, decl_idx: NodeIndex) -> Option<CodeLens> {
        let node = self.arena.get(decl_idx)?;
        let start = self.line_map.offset_to_position(node.pos, self.source_text);

        let range = Range::new(start, Position::new(start.line, start.character + 1));

        let data = CodeLensData {
            file_path: self.file_name.clone(),
            kind: CodeLensKind::Implementations,
            position: start,
        };

        Some(CodeLens::new(range, data))
    }

    /// Resolve a references lens by counting references.
    fn resolve_references_lens(
        &self,
        root: NodeIndex,
        lens: &CodeLens,
        data: &CodeLensData,
    ) -> Option<CodeLens> {
        // Find references using the existing FindReferences implementation
        let finder = FindReferences::new(
            self.arena,
            self.binder,
            self.line_map,
            self.file_name.clone(),
            self.source_text,
        );

        let references = finder.find_references(root, data.position);
        let count = references.as_ref().map_or(0, |r| r.len());

        // Subtract 1 for the declaration itself (if it's included in references)
        let ref_count = if count > 0 { count - 1 } else { 0 };

        let title = if ref_count == 1 {
            "1 reference".to_string()
        } else {
            format!("{} references", ref_count)
        };

        let command = CodeLensCommand {
            title,
            command: "editor.action.showReferences".to_string(),
            arguments: Some(vec![
                serde_json::json!(data.file_path),
                serde_json::json!({
                    "line": data.position.line,
                    "character": data.position.character
                }),
                serde_json::json!([]),
            ]),
        };

        Some(CodeLens::resolved(lens.range, command))
    }

    /// Resolve an implementations lens.
    fn resolve_implementations_lens(
        &self,
        lens: &CodeLens,
        data: &CodeLensData,
    ) -> Option<CodeLens> {
        // Finding implementations requires type checking and class hierarchy analysis
        // For now, return a placeholder that shows the command is available
        let command = CodeLensCommand {
            title: "Find Implementations".to_string(),
            command: "editor.action.goToImplementation".to_string(),
            arguments: Some(vec![
                serde_json::json!(data.file_path),
                serde_json::json!({
                    "line": data.position.line,
                    "character": data.position.character
                }),
            ]),
        };

        Some(CodeLens::resolved(lens.range, command))
    }

    /// Check if a function is a test function (heuristic based).
    #[allow(dead_code)]
    fn is_test_function(&self, node_idx: NodeIndex) -> bool {
        if self.arena.get(node_idx).is_none() {
            return false;
        }

        // Get the function name
        let name = self.get_declaration_name(node_idx);

        // Check for common test patterns
        if let Some(name) = name {
            let name_lower = name.to_lowercase();
            return name_lower.starts_with("test")
                || name_lower.starts_with("it_")
                || name_lower.ends_with("_test")
                || name_lower.ends_with("_spec");
        }

        // Check for decorators like @test
        // (Would need decorator support in parser)

        false
    }

    /// Get the name of a declaration.
    fn get_declaration_name(&self, node_idx: NodeIndex) -> Option<String> {
        // Look for an Identifier child that is the name
        for (i, node) in self.arena.nodes.iter().enumerate() {
            let idx = NodeIndex(i as u32);
            let parent = self
                .arena
                .get_extended(idx)
                .map_or(crate::parser::NodeIndex::NONE, |ext| ext.parent);

            if parent == node_idx
                && node.kind == crate::scanner::SyntaxKind::Identifier as u16
            {
                // Get the identifier text
                let start = node.pos as usize;
                let end = node.end as usize;
                if end <= self.source_text.len() {
                    return Some(self.source_text[start..end].to_string());
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod code_lens_tests {
    use super::*;
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    #[test]
    fn test_code_lens_function() {
        let source = "function foo() {\n  return 1;\n}\nfoo();";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider =
            CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        let lenses = provider.provide_code_lenses(root);

        // Should have at least one lens for the function
        assert!(!lenses.is_empty(), "Should have code lenses");

        // Find the function lens
        let func_lens = lenses
            .iter()
            .find(|l| l.range.start.line == 0)
            .expect("Should have lens at line 0");

        assert!(func_lens.data.is_some(), "Lens should have data");
        assert_eq!(func_lens.data.as_ref().unwrap().kind, CodeLensKind::References);
    }

    #[test]
    fn test_code_lens_class() {
        let source = "class MyClass {\n  method() {}\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider =
            CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        let lenses = provider.provide_code_lenses(root);

        // Should have lenses for both class and method
        assert!(lenses.len() >= 2, "Should have at least 2 code lenses");
    }

    #[test]
    fn test_code_lens_interface() {
        let source = "interface Foo {\n  bar(): void;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider =
            CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        let lenses = provider.provide_code_lenses(root);

        // Should have references and implementations lenses for interface
        let interface_lenses: Vec<_> = lenses
            .iter()
            .filter(|l| l.range.start.line == 0)
            .collect();

        assert!(
            interface_lenses.len() >= 2,
            "Interface should have references and implementations lenses"
        );
    }

    #[test]
    fn test_code_lens_resolve() {
        let source = "function foo() {}\nfoo();\nfoo();";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider =
            CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        let lenses = provider.provide_code_lenses(root);
        let func_lens = lenses
            .iter()
            .find(|l| l.range.start.line == 0)
            .expect("Should have function lens");

        // Resolve the lens
        let resolved = provider.resolve_code_lens(root, func_lens);

        assert!(resolved.is_some(), "Should resolve lens");
        let resolved = resolved.unwrap();
        assert!(resolved.command.is_some(), "Resolved lens should have command");

        let command = resolved.command.unwrap();
        // Should show reference count (2 calls + 1 declaration - 1 = 2 references)
        assert!(
            command.title.contains("reference"),
            "Title should mention references: {}",
            command.title
        );
    }

    #[test]
    fn test_code_lens_enum() {
        let source = "enum Color {\n  Red,\n  Green,\n  Blue\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider =
            CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        let lenses = provider.provide_code_lenses(root);

        // Should have a lens for the enum
        let enum_lens = lenses.iter().find(|l| l.range.start.line == 0);
        assert!(enum_lens.is_some(), "Should have lens for enum");
    }

    #[test]
    fn test_code_lens_type_alias() {
        let source = "type MyType = string | number;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider =
            CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        let lenses = provider.provide_code_lenses(root);

        // Should have a lens for the type alias
        assert!(!lenses.is_empty(), "Should have lens for type alias");
    }

    #[test]
    fn test_code_lens_empty_file() {
        let source = "";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider =
            CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        let lenses = provider.provide_code_lenses(root);

        // Empty file should have no lenses
        assert!(lenses.is_empty(), "Empty file should have no lenses");
    }

    #[test]
    fn test_code_lens_variable_no_lens() {
        let source = "const x = 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider =
            CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        let lenses = provider.provide_code_lenses(root);

        // Variables don't typically get code lenses (too noisy)
        // The lenses should be empty or only contain non-variable lenses
        for lens in &lenses {
            // Verify no lens at the variable position (character 6 is 'x')
            if lens.range.start.character == 6 {
                panic!("Should not have lens for variable");
            }
        }
    }
}
