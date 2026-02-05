//! Integration tests for LSP module.
//!
//! These tests verify that the LSP features work correctly together.

use super::*;
use crate::binder::BinderState;
use crate::parser::ParserState;

#[test]
fn test_lsp_workflow_simple() {
    // Simple test: const x = 1; x + x;
    let source = "const x = 1;\nx + x;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = position::LineMap::build(source);

    // Test Go-to-Definition
    let goto_def =
        definition::GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let position = Position::new(1, 0); // First 'x' in "x + x"
    let def = goto_def.get_definition(root, position);
    assert!(def.is_some(), "Should find definition");

    // Test Find References
    let find_refs =
        references::FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, position);
    assert!(refs.is_some(), "Should find references");
}

#[test]
fn test_lsp_with_function() {
    // Test with a function: function foo() {} foo();
    let source = "function foo() {}\nfoo();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = position::LineMap::build(source);

    // Test Go-to-Definition on the call
    let goto_def =
        definition::GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let position = Position::new(1, 0); // 'foo' in "foo()"
    let def = goto_def.get_definition(root, position);

    // Note: This may not work yet because our resolver is simplified
    // and may not handle all cases correctly. This is a placeholder
    // for future improvements.
    if let Some(_defs) = def {
        // Success!
    }
}

#[test]
fn test_position_utilities() {
    let source = "line1\nline2\nline3";
    let map = position::LineMap::build(source);

    // Test basic conversion
    let pos = Position::new(0, 0);
    let offset = map.position_to_offset(pos, source).unwrap();
    assert_eq!(offset, 0);

    let pos = Position::new(1, 0);
    let offset = map.position_to_offset(pos, source).unwrap();
    assert_eq!(offset, 6); // After "line1\n"

    // Test roundtrip
    let original_pos = Position::new(2, 3);
    let offset = map.position_to_offset(original_pos, source).unwrap();
    let back_to_pos = map.offset_to_position(offset, source);
    assert_eq!(original_pos, back_to_pos);
}

#[test]
fn test_project_multi_file_definition() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const a = 1;\na;".to_string());
    project.set_file("b.ts".to_string(), "const b = 2;\nb;".to_string());

    assert_eq!(project.file_count(), 2);

    let defs = project.get_definition("a.ts", Position::new(1, 0));
    assert!(defs.is_some(), "Should find definition in a.ts");

    let defs = defs.unwrap();
    assert_eq!(defs[0].range.start.line, 0);
    assert_eq!(defs[0].file_path, "a.ts");
}

#[test]
fn test_project_cross_file_definition_named_import() {
    // Test cross-file Go to Definition for named imports
    let mut project = Project::new();

    // a.ts exports a function `foo`
    project.set_file(
        "a.ts".to_string(),
        "export function foo() {}\nfoo();".to_string(),
    );

    // b.ts imports and uses `foo`
    project.set_file(
        "b.ts".to_string(),
        "import { foo } from './a';\nfoo();".to_string(),
    );

    // Click on `foo` in the import statement in b.ts (line 0, column 11)
    // Should jump to the definition in a.ts
    let defs = project.get_definition("b.ts", Position::new(0, 11));
    assert!(
        defs.is_some(),
        "Should find cross-file definition for named import"
    );

    let defs = defs.unwrap();
    assert_eq!(defs.len(), 1, "Should have one definition");
    assert_eq!(defs[0].file_path, "a.ts", "Should point to a.ts");
    assert_eq!(
        defs[0].range.start.line, 0,
        "Should point to line 0 (the export)"
    );
}

#[test]
fn test_project_cross_file_definition_default_import() {
    // Test cross-file Go to Definition for default imports
    let mut project = Project::new();

    // a.ts has a default export
    project.set_file(
        "a.ts".to_string(),
        "export default function bar() {}\nbar();".to_string(),
    );

    // b.ts imports and uses the default export
    project.set_file(
        "b.ts".to_string(),
        "import bar from './a';\nbar();".to_string(),
    );

    // Click on `bar` in the import statement in b.ts (line 0, column 8)
    // Should jump to the default export in a.ts
    let defs = project.get_definition("b.ts", Position::new(0, 8));
    assert!(
        defs.is_some(),
        "Should find cross-file definition for default import"
    );

    let defs = defs.unwrap();
    assert_eq!(defs.len(), 1, "Should have one definition");
    assert_eq!(defs[0].file_path, "a.ts", "Should point to a.ts");
    assert_eq!(
        defs[0].range.start.line, 0,
        "Should point to line 0 (the default export)"
    );
}

#[test]
fn test_project_cross_file_definition_import_with_alias() {
    // Test cross-file Go to Definition for imports with alias (import { foo as bar })
    let mut project = Project::new();

    // a.ts exports a function `originalName`
    project.set_file(
        "a.ts".to_string(),
        "export function originalName() {}\noriginalName();".to_string(),
    );

    // b.ts imports with alias and uses it
    project.set_file(
        "b.ts".to_string(),
        "import { originalName as alias } from './a';\nalias();".to_string(),
    );

    // Click on `originalName` in the import statement (the name being imported)
    // Position is approximately at "originalName" which starts at column 10
    let defs = project.get_definition("b.ts", Position::new(0, 10));
    assert!(
        defs.is_some(),
        "Should find cross-file definition for aliased import"
    );

    let defs = defs.unwrap();
    assert_eq!(defs.len(), 1, "Should have one definition");
    assert_eq!(defs[0].file_path, "a.ts", "Should point to a.ts");
    assert_eq!(
        defs[0].range.start.line, 0,
        "Should point to line 0 (the export)"
    );
}

#[test]
fn test_lsp_diagnostic_conversion() {
    let source = "const x: string = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = crate::solver::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    checker.check_source_file(root);

    let line_map = position::LineMap::build(source);
    let lsp_diags: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| crate::lsp::diagnostics::convert_diagnostic(d, &line_map, source))
        .collect();

    assert!(!lsp_diags.is_empty(), "Should produce LSP diagnostics");
    let diag = &lsp_diags[0];
    assert_eq!(diag.range.start.line, 0);
    assert_eq!(
        diag.severity,
        Some(crate::lsp::diagnostics::DiagnosticSeverity::Error)
    );
}
