use super::*;

#[test]
fn test_project_cross_file_function_body_edit_preserves_symbol_and_scope_cache() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "export const alpha = 1;\nfunction foo() {\n  const inner = 1;\n  return inner;\n}\n"
            .to_string(),
    );
    project.set_file(
        "b.ts".to_string(),
        "import { alpha } from \"./a\";\nalpha;\n".to_string(),
    );
    let position = Position::new(1, 0);

    let alpha_symbol_before = {
        let file = project.file("a.ts").unwrap();
        file.binder()
            .file_locals
            .get("alpha")
            .expect("Expected symbol for alpha")
    };

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "inner = 1");
        TextEdit::new(range, "inner = 2".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    let alpha_symbol_after = {
        let file = project.file("a.ts").unwrap();
        file.binder()
            .file_locals
            .get("alpha")
            .expect("Expected symbol for alpha after update")
    };

    assert_eq!(alpha_symbol_before, alpha_symbol_after);

    assert!(project.get_hover("b.ts", position).is_some());
    assert!(project.get_definition("b.ts", position).is_some());

    let timing = project
        .performance()
        .timing(ProjectRequestKind::Definition)
        .expect("Expected timing data for definition");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected definition to reuse cached scope"
    );
}

#[test]
fn test_project_scope_cache_reuse_after_other_file_edit() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "export const alpha = 1;\nfunction foo() {\n  return 1;\n}\n".to_string(),
    );
    project.set_file(
        "b.ts".to_string(),
        "import { alpha } from \"./a\";\nalpha;\n".to_string(),
    );
    let position = Position::new(1, 0);

    assert!(project.get_hover("b.ts", position).is_some());

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "return 1");
        TextEdit::new(range, "return 2".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    assert!(project.get_definition("b.ts", position).is_some());

    let timing = project
        .performance()
        .timing(ProjectRequestKind::Definition)
        .expect("Expected timing data for definition");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit after other file edit"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected definition to reuse cached scope after other file edit"
    );
}

#[test]
fn test_project_scope_cache_reuse_after_nested_edit_suffix_export_across_files() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "function outer() {\n  function inner() {\n    return 1;\n  }\n  return inner();\n}\nexport const beta = 1;\n".to_string(),
    );
    project.set_file(
        "b.ts".to_string(),
        "import { beta } from \"./a\";\nbeta;\n".to_string(),
    );
    let position = Position::new(1, 0);

    assert!(project.get_hover("b.ts", position).is_some());

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "return 1");
        TextEdit::new(range, "return 2".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    assert!(project.get_definition("b.ts", position).is_some());

    let timing = project
        .performance()
        .timing(ProjectRequestKind::Definition)
        .expect("Expected timing data for definition");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit after nested edit in other file"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected definition to reuse cached scope after nested edit in other file"
    );
}

#[test]
fn test_project_nested_function_body_edit_preserves_prefix_symbol_and_scope_cache() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "const alpha = 1;\nfunction outer() {\n  function inner() {\n    return alpha;\n  }\n  return inner();\n}\nalpha;\n".to_string(),
    );
    let position = Position::new(7, 0);

    let alpha_symbol_before = {
        let file = project.file("a.ts").unwrap();
        file.binder()
            .file_locals
            .get("alpha")
            .expect("Expected symbol for alpha")
    };

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "return alpha;");
        TextEdit::new(range, "return alpha + 1;".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    let alpha_symbol_after = {
        let file = project.file("a.ts").unwrap();
        file.binder()
            .file_locals
            .get("alpha")
            .expect("Expected symbol for alpha after update")
    };

    assert_eq!(alpha_symbol_before, alpha_symbol_after);

    assert!(project.get_hover("a.ts", position).is_some());
    assert!(project.get_definition("a.ts", position).is_some());
    let first = project
        .performance()
        .timing(ProjectRequestKind::Definition)
        .expect("Expected timing data for definition");

    assert!(project.get_definition("a.ts", position).is_some());
    let second = project
        .performance()
        .timing(ProjectRequestKind::Definition)
        .expect("Expected timing data for definition");

    if first.scope_hits > 0 {
        assert_eq!(
            first.scope_misses, 0,
            "Expected definition to reuse cached scope after nested edit"
        );
    } else {
        assert!(
            first.scope_misses > 0,
            "Expected cache misses after nested edit"
        );
    }

    assert!(
        second.scope_hits > 0,
        "Expected scope cache hit after nested edit"
    );
    assert_eq!(
        second.scope_misses, 0,
        "Expected definition to reuse cached scope after cache warm"
    );
}

#[test]
fn test_project_nested_function_body_edit_preserves_suffix_definition_scope_cache() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "const alpha = 1;\nfunction outer() {\n  function inner() {\n    return alpha;\n  }\n  return inner();\n}\nconst beta = alpha;\nbeta;\n".to_string(),
    );
    let position = {
        let file = project.file("a.ts").unwrap();
        range_for_substring(file.source_text(), file.line_map(), "beta;").start
    };

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "return alpha;");
        TextEdit::new(range, "return alpha + 1;".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    assert!(project.get_hover("a.ts", position).is_some());
    assert!(project.get_definition("a.ts", position).is_some());

    let timing = project
        .performance()
        .timing(ProjectRequestKind::Definition)
        .expect("Expected timing data for definition");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit for suffix symbol after nested edit"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected definition to reuse cached scope for suffix symbol after nested edit"
    );
}

#[test]
fn test_project_nested_function_body_edit_suffix_definition_without_hover() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "const alpha = 1;\nfunction outer() {\n  function inner() {\n    return alpha;\n  }\n  return inner();\n}\nconst beta = alpha;\nbeta;\n".to_string(),
    );
    let position = {
        let file = project.file("a.ts").unwrap();
        range_for_substring(file.source_text(), file.line_map(), "beta;").start
    };

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "return alpha;");
        TextEdit::new(range, "return alpha + 1;".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    let expected_decl_start = {
        let file = project.file("a.ts").unwrap();
        range_for_substring(file.source_text(), file.line_map(), "beta = alpha").start
    };

    let definitions = project
        .get_definition("a.ts", position)
        .expect("Expected definition for suffix symbol");
    assert!(
        definitions
            .iter()
            .any(|loc| { loc.file_path == "a.ts" && loc.range.start == expected_decl_start }),
        "Expected definition to point at beta declaration after nested edit"
    );

    let timing = project
        .performance()
        .timing(ProjectRequestKind::Definition)
        .expect("Expected timing data for definition");

    assert!(
        timing.scope_misses > 0,
        "Expected cache misses on cold definition after nested edit"
    );
    assert_eq!(
        timing.scope_hits, 0,
        "Expected no cache hits on cold definition after nested edit"
    );
}

#[test]
fn test_project_cross_file_references_reexport_named() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "export const foo = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "export { foo as bar } from \"./a\";\n".to_string(),
    );
    project.set_file(
        "c.ts".to_string(),
        "import { bar } from \"./b\";\nbar;\n".to_string(),
    );

    let refs = project.find_references("a.ts", Position::new(0, 13));
    assert!(refs.is_some(), "Should find references across re-exports");

    let refs = refs.unwrap();
    assert!(
        refs.iter().any(|loc| loc.file_path == "b.ts"),
        "Should include re-export reference in b.ts"
    );
    assert!(
        refs.iter().any(|loc| loc.file_path == "c.ts"),
        "Should include references from c.ts"
    );
}

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
fn test_auto_import_reexport_cache_refreshes_after_update_file() {
    let mut project = Project::new();
    project.set_file(
        "a.ts".to_string(),
        "export const MyUtil = 42;\n".to_string(),
    );
    project.set_file(
        "barrel.ts".to_string(),
        "export { Other } from './a';\n".to_string(),
    );
    project.set_file("c.ts".to_string(), "MyUtil;\n".to_string());

    let completions_from_barrel = |project: &mut Project| {
        project
            .get_completions(
                "c.ts",
                Position {
                    line: 0,
                    character: 2,
                },
            )
            .unwrap_or_default()
            .into_iter()
            .filter(|item| {
                item.label == "MyUtil"
                    && item
                        .detail
                        .as_deref()
                        .is_some_and(|detail| detail.contains("./barrel"))
            })
            .collect::<Vec<_>>()
    };

    assert!(
        completions_from_barrel(&mut project).is_empty(),
        "named-only re-export should not suggest MyUtil from barrel"
    );

    let append_star = TextEdit::new(
        Range::new(Position::new(1, 0), Position::new(1, 0)),
        "export * from './a';\n".to_string(),
    );
    project
        .update_file("barrel.ts", &[append_star])
        .expect("expected update_file to append wildcard re-export");
    assert!(
        !completions_from_barrel(&mut project).is_empty(),
        "adding export * through update_file should refresh wildcard re-export cache"
    );

    let remove_star = TextEdit::new(
        Range::new(Position::new(1, 0), Position::new(2, 0)),
        String::new(),
    );
    project
        .update_file("barrel.ts", &[remove_star])
        .expect("expected update_file to remove wildcard re-export");
    assert!(
        completions_from_barrel(&mut project).is_empty(),
        "removing export * through update_file should refresh wildcard re-export cache"
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

#[test]
fn test_export_addition_invalidates_dependents() {
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

    // Clean b.ts diagnostics
    let _ = project.get_diagnostics("b.ts");
    assert!(
        !project.files["b.ts"].diagnostics_dirty,
        "b.ts should be clean after getting diagnostics"
    );

    // Add a new export to a.ts — this changes the export signature
    let a_file = &project.files["a.ts"];
    let a_line_map = a_file.line_map().clone();
    let a_source = a_file.source_text().to_string();
    let end_range = Range::new(
        a_line_map.offset_to_position(a_source.len() as u32, &a_source),
        a_line_map.offset_to_position(a_source.len() as u32, &a_source),
    );
    project.update_file(
        "a.ts",
        &[TextEdit {
            range: end_range,
            new_text: "\nexport function bar() {}".to_string(),
        }],
    );

    // b.ts SHOULD be marked dirty — the export signature changed
    assert!(
        project.files["b.ts"].diagnostics_dirty,
        "b.ts SHOULD be invalidated when a.ts adds a new export"
    );
}

#[test]
fn test_comment_edit_does_not_invalidate_dependents() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "// version 1\nexport const x = 1;\n".to_string(),
    );
    project.set_file(
        "b.ts".to_string(),
        "import { x } from \"./a\";\nconsole.log(x);\n".to_string(),
    );

    // Manually wire the dependency (extract_imports uses raw specifiers like "./a")
    project.dependency_graph.add_dependency("b.ts", "a.ts");

    // Clean b.ts
    let _ = project.get_diagnostics("b.ts");

    // Edit only the comment in a.ts
    let a_file = &project.files["a.ts"];
    let a_line_map = a_file.line_map().clone();
    let a_source = a_file.source_text().to_string();
    let edit_range = range_for_substring(&a_source, &a_line_map, "version 1");
    project.update_file(
        "a.ts",
        &[TextEdit {
            range: edit_range,
            new_text: "version 2".to_string(),
        }],
    );

    // b.ts should NOT be dirty
    assert!(
        !project.files["b.ts"].diagnostics_dirty,
        "b.ts should NOT be invalidated by a comment-only edit in a.ts"
    );
}

#[test]
fn test_private_addition_does_not_invalidate_dependents() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "export function foo() {}\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { foo } from \"./a\";\nfoo();\n".to_string(),
    );

    // Manually wire the dependency (extract_imports uses raw specifiers like "./a")
    project.dependency_graph.add_dependency("b.ts", "a.ts");

    // Clean b.ts
    let _ = project.get_diagnostics("b.ts");

    // Add a private (non-exported) symbol to a.ts
    let a_file = &project.files["a.ts"];
    let a_line_map = a_file.line_map().clone();
    let a_source = a_file.source_text().to_string();
    let end_range = Range::new(
        a_line_map.offset_to_position(a_source.len() as u32, &a_source),
        a_line_map.offset_to_position(a_source.len() as u32, &a_source),
    );
    project.update_file(
        "a.ts",
        &[TextEdit {
            range: end_range,
            new_text: "const helper = 42;\n".to_string(),
        }],
    );

    // b.ts should NOT be dirty — private additions don't change exports
    assert!(
        !project.files["b.ts"].diagnostics_dirty,
        "b.ts should NOT be invalidated when a.ts adds a private symbol"
    );
}

// =============================================================================
// Project-level feature tests for new wrappers
// =============================================================================
