#[test]
fn test_project_cross_file_references_namespace_reexport() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "export const foo = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "export * as ns from \"./a\";\n".to_string(),
    );
    project.set_file(
        "c.ts".to_string(),
        "import { ns } from \"./b\";\nns.foo;\n".to_string(),
    );

    let refs = project.find_references("a.ts", Position::new(0, 13));
    assert!(
        refs.is_some(),
        "Should find references through namespace re-export"
    );

    let refs = refs.unwrap();
    assert!(
        refs.iter().any(|loc| loc.file_path == "c.ts"),
        "Should include namespace member reference in c.ts"
    );
}

#[test]
fn test_project_code_actions_missing_import_named() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "export const foo = 1;\n".to_string());
    project.set_file("b.ts".to_string(), "foo();\n".to_string());

    let file = project.file("b.ts").unwrap();
    let source = file.source_text();
    let line_map = file.line_map();
    let start = source.find("foo").unwrap();
    let range = Range::new(
        line_map.offset_to_position(start as u32, source),
        line_map.offset_to_position((start + 3) as u32, source),
    );

    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        source: None,
        message: "Cannot find name 'foo'.".to_string(),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    };

    let actions = project
        .get_code_actions(
            "b.ts",
            Range::new(Position::new(0, 0), Position::new(0, 0)),
            vec![diag],
            Some(vec![CodeActionKind::QuickFix]),
        )
        .expect("Expected missing import quick fix");

    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["b.ts"];
    let updated = apply_text_edits(source, line_map, edits);
    assert_eq!(updated, "import { foo } from \"./a\";\n\nfoo();\n");
}

#[test]
fn test_project_code_actions_missing_import_default_export() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "export default function bar() {}\n".to_string(),
    );
    project.set_file("b.ts".to_string(), "foo();\n".to_string());

    let file = project.file("b.ts").unwrap();
    let source = file.source_text();
    let line_map = file.line_map();
    let start = source.find("foo").unwrap();
    let range = Range::new(
        line_map.offset_to_position(start as u32, source),
        line_map.offset_to_position((start + 3) as u32, source),
    );

    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        source: None,
        message: "Cannot find name 'foo'.".to_string(),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    };

    let actions = project
        .get_code_actions(
            "b.ts",
            Range::new(Position::new(0, 0), Position::new(0, 0)),
            vec![diag],
            Some(vec![CodeActionKind::QuickFix]),
        )
        .expect("Expected missing import quick fix");

    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["b.ts"];
    let updated = apply_text_edits(source, line_map, edits);
    assert_eq!(updated, "import foo from \"./a\";\n\nfoo();\n");
}

#[test]
fn test_project_code_actions_missing_import_tsx() {
    let mut project = Project::new();

    project.set_file("a.tsx".to_string(), "export const foo = 1;\n".to_string());
    project.set_file("b.ts".to_string(), "foo();\n".to_string());

    let file = project.file("b.ts").unwrap();
    let source = file.source_text();
    let line_map = file.line_map();
    let start = source.find("foo").unwrap();
    let range = Range::new(
        line_map.offset_to_position(start as u32, source),
        line_map.offset_to_position((start + 3) as u32, source),
    );

    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        source: None,
        message: "Cannot find name 'foo'.".to_string(),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    };

    let actions = project
        .get_code_actions(
            "b.ts",
            Range::new(Position::new(0, 0), Position::new(0, 0)),
            vec![diag],
            Some(vec![CodeActionKind::QuickFix]),
        )
        .expect("Expected missing import quick fix");

    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["b.ts"];
    let updated = apply_text_edits(source, line_map, edits);
    assert_eq!(updated, "import { foo } from \"./a\";\n\nfoo();\n");
}

#[test]
fn test_project_code_actions_missing_type_only_import_at_point_range() {
    let mut project = Project::new();

    project.set_file(
        "react.ts".to_string(),
        "export interface ComponentProps {}\n".to_string(),
    );
    project.set_file(
        "main.ts".to_string(),
        "type _ = ComponentProps;\n".to_string(),
    );

    let file = project.file("main.ts").unwrap();
    let source = file.source_text();
    let line_map = file.line_map();
    let point = source.find("ComponentProps").unwrap() + "ComponentProps".len();
    let position = line_map.offset_to_position(point as u32, source);
    let range = Range::new(position, position);

    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        source: None,
        message: "Cannot find name 'ComponentProps'.".to_string(),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    };

    let actions = project
        .get_code_actions(
            "main.ts",
            range,
            vec![diag],
            Some(vec![CodeActionKind::QuickFix]),
        )
        .expect("Expected missing type-only import quick fix");

    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["main.ts"];
    let updated = apply_text_edits(source, line_map, edits);
    assert_eq!(
        updated,
        "import type { ComponentProps } from \"./react\";\n\ntype _ = ComponentProps;\n"
    );
}

#[test]
fn test_project_code_actions_missing_import_default_reexport() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "export default function bar() {}\n".to_string(),
    );
    project.set_file(
        "index.ts".to_string(),
        "export { default } from \"./a\";\n".to_string(),
    );
    project.set_file("b.ts".to_string(), "foo();\n".to_string());

    let file = project.file("b.ts").unwrap();
    let source = file.source_text();
    let line_map = file.line_map();
    let start = source.find("foo").unwrap();
    let range = Range::new(
        line_map.offset_to_position(start as u32, source),
        line_map.offset_to_position((start + 3) as u32, source),
    );

    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        source: None,
        message: "Cannot find name 'foo'.".to_string(),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    };

    let actions = project
        .get_code_actions(
            "b.ts",
            Range::new(Position::new(0, 0), Position::new(0, 0)),
            vec![diag],
            Some(vec![CodeActionKind::QuickFix]),
        )
        .expect("Expected missing import quick fix");

    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["b.ts"];
    let updated = apply_text_edits(source, line_map, edits);
    assert_eq!(updated, "import foo from \"./index\";\n\nfoo();\n");
}

#[test]
fn test_project_code_actions_missing_import_reexport() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "export const foo = 1;\n".to_string());
    project.set_file(
        "index.ts".to_string(),
        "export { foo as bar } from \"./a\";\n".to_string(),
    );
    project.set_file("b.ts".to_string(), "bar();\n".to_string());

    let file = project.file("b.ts").unwrap();
    let source = file.source_text();
    let line_map = file.line_map();
    let start = source.find("bar").unwrap();
    let range = Range::new(
        line_map.offset_to_position(start as u32, source),
        line_map.offset_to_position((start + 3) as u32, source),
    );

    let diag = LspDiagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
        source: None,
        message: "Cannot find name 'bar'.".to_string(),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    };

    let actions = project
        .get_code_actions(
            "b.ts",
            Range::new(Position::new(0, 0), Position::new(0, 0)),
            vec![diag],
            Some(vec![CodeActionKind::QuickFix]),
        )
        .expect("Expected missing import quick fix");

    let edit = actions[0].edit.as_ref().unwrap();
    let edits = &edit.changes["b.ts"];
    let updated = apply_text_edits(source, line_map, edits);
    assert_eq!(updated, "import { bar } from \"./index\";\n\nbar();\n");
}

#[test]
fn test_auto_import_via_reexport() {
    let mut project = Project::new();

    // a.ts - declares the symbol
    project.set_file("a.ts".to_string(), "export const MyUtil = 42;".to_string());

    // b.ts - re-exports from a.ts
    project.set_file("b.ts".to_string(), "export * from './a';".to_string());

    // c.ts - tries to use MyUtil (should suggest importing from b.ts)
    project.set_file("c.ts".to_string(), "MyUtil;\n".to_string());

    // Request completions - should find MyUtil and suggest import from b.ts (the re-export)
    let result = project.get_completions(
        "c.ts",
        Position {
            line: 0,
            character: 2,
        },
    );

    // Verify we get a completion for MyUtil
    assert!(
        result.is_some(),
        "Expected completion result to be present for MyUtil test"
    );
    let result = result.unwrap();

    // Should have MyUtil completions from both direct import (./a) and re-export (./b)
    let myutil_completions: Vec<_> = result
        .iter()
        .filter(|item| item.label == "MyUtil")
        .collect();

    assert!(
        !myutil_completions.is_empty(),
        "Should find MyUtil completions"
    );

    // Should have at least one completion from ./b (the re-export)
    let has_b_import = myutil_completions
        .iter()
        .any(|item| item.detail.as_deref().unwrap_or("").contains("./b"));

    assert!(
        has_b_import,
        "Should suggest importing from ./b (the re-export). Found details: {:?}",
        myutil_completions
            .iter()
            .map(|item| &item.detail)
            .collect::<Vec<_>>()
    );

    // Verify one of the completions has all required fields
    let completion = myutil_completions
        .iter()
        .find(|item| item.detail.as_deref().unwrap_or("").contains("./b"))
        .unwrap();

    // Verify it's an auto-import
    let detail = completion.detail.as_deref().unwrap_or("");
    assert!(
        detail.contains("auto-import"),
        "Should be marked as auto-import"
    );

    // Verify it suggests importing from b.ts (the re-export)
    assert!(
        detail.contains("./b"),
        "Should suggest importing from b.ts (the re-export)"
    );

    // Verify additionalTextEdits are present to insert the import
    assert!(
        completion.additional_text_edits.is_some(),
        "Should have additionalTextEdits to insert import"
    );
}

#[test]
fn test_auto_import_reexport_prefers_shorter_source_for_duplicate_symbol_name() {
    let mut project = Project::new();
    project.set_file(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "module": "commonjs",
    "paths": {
      "~/*": ["src/*"]
    }
  }
}"#
        .to_string(),
    );
    project.set_file("/src/dirA/thing1A.ts".to_string(), "Thing".to_string());
    project.set_file(
        "/src/dirA/thing2A.ts".to_string(),
        "export class Thing2A {}".to_string(),
    );
    project.set_file(
        "/src/dirB/index.ts".to_string(),
        "export * from \"./thing1B\";\nexport * from \"./thing2B\";\n".to_string(),
    );
    project.set_file(
        "/src/dirB/thing1B.ts".to_string(),
        "export class Thing1B {}".to_string(),
    );
    project.set_file(
        "/src/dirB/thing2B.ts".to_string(),
        "export class Thing2B {}".to_string(),
    );

    let completions = project
        .get_completions("/src/dirA/thing1A.ts", Position::new(0, 5))
        .expect("expected completions");

    let thing2_completions: Vec<_> = completions
        .iter()
        .filter(|item| item.label == "Thing2B")
        .collect();
    let thing2a_completions: Vec<_> = completions
        .iter()
        .filter(|item| item.label == "Thing2A")
        .collect();

    assert!(
        !thing2_completions.is_empty(),
        "expected Thing2B auto-import completion entries"
    );
    assert!(
        !thing2a_completions.is_empty(),
        "expected Thing2A auto-import completion entries"
    );
    assert_eq!(
        thing2_completions[0].source.as_deref(),
        Some("~/dirB"),
        "expected shorter barrel source to be ordered first"
    );
    assert_eq!(
        thing2a_completions[0].source.as_deref(),
        Some("./thing2A"),
        "expected direct sibling source to outrank ./index for same-directory symbols"
    );
}

// =============================================================================
// Export Signature / Smart Cache Invalidation Tests
// =============================================================================

#[test]
fn test_body_edit_does_not_invalidate_dependents() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "export function foo() { return 1; }".to_string(),
    );
    project.set_file(
        "b.ts".to_string(),
        "import { foo } from \"./a\";\nfoo();\n".to_string(),
    );

    // Manually wire the dependency (extract_imports uses raw specifiers like "./a")
    project.dependency_graph.add_dependency("b.ts", "a.ts");

    // Force b.ts diagnostics to be "clean" by getting them once
    let _ = project.get_diagnostics("b.ts");
    assert!(
        !project.files["b.ts"].diagnostics_dirty,
        "b.ts should be clean after getting diagnostics"
    );

    // Edit a.ts function body only (no export change)
    let a_file = &project.files["a.ts"];
    let a_line_map = a_file.line_map().clone();
    let a_source = a_file.source_text().to_string();
    let edit_range = range_for_substring(&a_source, &a_line_map, "return 1");
    project.update_file(
        "a.ts",
        &[TextEdit {
            range: edit_range,
            new_text: "return 2".to_string(),
        }],
    );

    // b.ts should NOT be marked dirty — the export signature didn't change
    assert!(
        !project.files["b.ts"].diagnostics_dirty,
        "b.ts should NOT be invalidated by a body-only edit in a.ts"
    );
}

