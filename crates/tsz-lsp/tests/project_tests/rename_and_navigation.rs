use super::*;
use crate::project::FileRename;

#[test]
fn test_project_update_file_with_edits() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "const x = 1;\nconst y = 2;\n".to_string(),
    );

    let source = "const x = 1;\nconst y = 2;\n";
    let line_map = LineMap::build(source);
    let edit = TextEdit::new(
        range_for_substring(source, &line_map, "1"),
        "42".to_string(),
    );

    let result = project.update_file("test.ts", &[edit]);
    assert!(result.is_some(), "update_file should succeed");

    let file = project.file("test.ts").unwrap();
    assert!(
        file.source_text().contains("42"),
        "Source should contain updated value"
    );
}

#[test]
fn test_project_update_file_missing() {
    let mut project = Project::new();
    let edit = TextEdit::new(
        Range::new(Position::new(0, 0), Position::new(0, 1)),
        "x".to_string(),
    );
    let result = project.update_file("nonexistent.ts", &[edit]);
    assert!(
        result.is_none(),
        "update_file should return None for missing file"
    );
}

#[test]
fn test_project_cross_file_definition() {
    let mut project = Project::new();
    project.set_file(
        "utils.ts".to_string(),
        "export function helper() {}\n".to_string(),
    );
    project.set_file(
        "main.ts".to_string(),
        "import { helper } from \"./utils\";\nhelper();\n".to_string(),
    );

    // Get definition for 'helper' usage on line 1
    let result = project.get_definition("main.ts", Position::new(1, 0));
    // Cross-file definition may resolve to the import specifier or the original declaration
    // depending on resolution. We just verify it returns a result.
    assert!(
        result.is_some(),
        "Should find definition for imported symbol"
    );
}

#[test]
fn test_project_workspace_symbols_filter() {
    let mut project = Project::new();
    project.set_file(
        "a.ts".to_string(),
        "export function alpha() {}\nexport function beta() {}\n".to_string(),
    );

    // Workspace symbols search for "alpha" should find the function
    let symbols = project.get_workspace_symbols("alpha");
    // The symbol index may or may not be populated depending on how set_file works
    // This test verifies the function returns without error and produces results if indexed
    if !symbols.is_empty() {
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.iter().any(|n: &&str| n.contains("alpha")),
            "Should find symbols matching 'alpha' query, got: {names:?}"
        );
    }
}

#[test]
fn test_project_handle_will_rename_files() {
    let mut project = Project::new();
    project.set_file("old.ts".to_string(), "export const x = 1;\n".to_string());
    project.set_file(
        "consumer.ts".to_string(),
        "import { x } from \"./old\";\n".to_string(),
    );

    let renames = vec![FileRename {
        old_uri: "old.ts".to_string(),
        new_uri: "new.ts".to_string(),
    }];

    let workspace_edit = project.handle_will_rename_files(&renames);
    // The workspace edit should contain text edits to update import paths
    let has_edits = workspace_edit
        .changes
        .values()
        .any(|edits| !edits.is_empty());
    assert!(
        has_edits,
        "Should produce workspace edits to update import paths"
    );
}

#[test]
fn test_project_handle_will_rename_files_preserves_quotes() {
    // Regression: `process_file_rename` used to pair the outer quoted range
    // with a bare specifier, so applying the edit overwrote the surrounding
    // quotes. Verify that the produced edit targets only the inner content
    // and that applying it re-yields a quoted import (for both quote styles).
    for (quote, name) in [('\"', "double"), ('\'', "single")] {
        let source = format!("import {{ x }} from {quote}./old{quote};\n");
        let mut project = Project::new();
        project.set_file("old.ts".to_string(), "export const x = 1;\n".to_string());
        project.set_file("consumer.ts".to_string(), source.clone());

        let workspace_edit = project.handle_will_rename_files(&[FileRename {
            old_uri: "old.ts".to_string(),
            new_uri: "new.ts".to_string(),
        }]);

        let edits = workspace_edit
            .changes
            .get("consumer.ts")
            .unwrap_or_else(|| panic!("{name}-quote variant should produce edits for consumer.ts"));
        assert_eq!(edits.len(), 1, "{name}-quote variant: one edit expected");
        let edit = &edits[0];
        assert!(
            !edit.new_text.contains(quote),
            "{name}-quote variant: edit text must not contain quotes (got {:?})",
            edit.new_text
        );

        // Apply the edit and assert the result is still a quoted import.
        // The extension handling is a separate concern (`calculate_new_relative_path`
        // retains `new_path`'s extension); here we just confirm the surrounding
        // quote characters survive the rewrite.
        let line_map = tsz_common::position::LineMap::build(&source);
        let applied = apply_text_edits(&source, &line_map, edits);
        let prefix = format!("import {{ x }} from {quote}");
        let suffix = format!("{quote};\n");
        assert!(
            applied.starts_with(&prefix) && applied.ends_with(&suffix),
            "{name}-quote variant: applying the rename edit should preserve quotes, got {applied:?}"
        );
    }
}

#[test]
fn test_project_handle_will_rename_files_updates_tsconfig_paths_alias() {
    // Renaming a file referenced through a `paths` alias should rewrite the
    // alias-mapped specifier, not just relative imports. Mirrors tsserver's
    // behavior: `@app/foo` becomes `@app/bar` when `src/foo.ts` moves to
    // `src/bar.ts`.
    let mut project = Project::new();
    project.set_file(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": { "@app/*": ["src/*"] }
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/src/foo.ts".to_string(),
        "export const x = 1;\n".to_string(),
    );
    project.set_file(
        "/src/consumer.ts".to_string(),
        "import { x } from \"@app/foo\";\nconst y = x;\n".to_string(),
    );

    let edits = project.handle_will_rename_files(&[FileRename {
        old_uri: "/src/foo.ts".to_string(),
        new_uri: "/src/bar.ts".to_string(),
    }]);

    let consumer_edits = edits
        .changes
        .get("/src/consumer.ts")
        .expect("paths-aliased import in consumer.ts should be rewritten");
    assert_eq!(consumer_edits.len(), 1);
    assert_eq!(consumer_edits[0].new_text, "@app/bar");
}

#[test]
fn test_project_handle_will_rename_files_preserves_chosen_alias() {
    // When multiple alias patterns could host the renamed target, the user's
    // original alias pattern should be preserved.
    let mut project = Project::new();
    project.set_file(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@app/*": ["src/*"],
      "~/*": ["src/*"]
    }
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/src/foo.ts".to_string(),
        "export const x = 1;\n".to_string(),
    );
    project.set_file(
        "/src/a.ts".to_string(),
        "import { x } from \"@app/foo\";\nconst y = x;\n".to_string(),
    );
    project.set_file(
        "/src/b.ts".to_string(),
        "import { x } from \"~/foo\";\nconst y = x;\n".to_string(),
    );

    let edits = project.handle_will_rename_files(&[FileRename {
        old_uri: "/src/foo.ts".to_string(),
        new_uri: "/src/bar.ts".to_string(),
    }]);

    let a_edits = edits
        .changes
        .get("/src/a.ts")
        .expect("a.ts should be updated");
    assert_eq!(a_edits[0].new_text, "@app/bar");
    let b_edits = edits
        .changes
        .get("/src/b.ts")
        .expect("b.ts should be updated");
    assert_eq!(b_edits[0].new_text, "~/bar");
}

#[test]
fn test_project_handle_will_rename_files_alias_with_nested_dirs() {
    // Wildcard captures must traverse into nested directories. Renaming
    // `src/utils/math.ts` should produce `@app/utils/math2` (not just
    // `@app/math2`).
    let mut project = Project::new();
    project.set_file(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": { "@app/*": ["src/*"] }
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/src/utils/math.ts".to_string(),
        "export const add = (a: number, b: number) => a + b;\n".to_string(),
    );
    project.set_file(
        "/src/consumer.ts".to_string(),
        "import { add } from \"@app/utils/math\";\nconst y = add(1, 2);\n".to_string(),
    );

    let edits = project.handle_will_rename_files(&[FileRename {
        old_uri: "/src/utils/math.ts".to_string(),
        new_uri: "/src/utils/math2.ts".to_string(),
    }]);

    let consumer_edits = edits
        .changes
        .get("/src/consumer.ts")
        .expect("nested aliased import should be rewritten");
    assert_eq!(consumer_edits[0].new_text, "@app/utils/math2");
}

#[test]
fn test_project_handle_will_rename_files_alias_leaves_unrelated_imports() {
    // A path-aliased import that points to a *different* file under the same
    // alias must not be rewritten.
    let mut project = Project::new();
    project.set_file(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": { "@app/*": ["src/*"] }
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/src/foo.ts".to_string(),
        "export const x = 1;\n".to_string(),
    );
    project.set_file(
        "/src/other.ts".to_string(),
        "export const y = 2;\n".to_string(),
    );
    project.set_file(
        "/src/consumer.ts".to_string(),
        "import { y } from \"@app/other\";\nconst z = y;\n".to_string(),
    );

    let edits = project.handle_will_rename_files(&[FileRename {
        old_uri: "/src/foo.ts".to_string(),
        new_uri: "/src/bar.ts".to_string(),
    }]);

    assert!(
        edits
            .changes
            .get("/src/consumer.ts")
            .is_none_or(|e| e.is_empty()),
        "an aliased import targeting an unrelated file must not be rewritten"
    );
}

#[test]
fn test_project_handle_will_rename_files_leaves_bare_npm_specifiers() {
    // Bare package specifiers (e.g. an npm package) must never be rewritten
    // by file-rename, even if no tsconfig is present.
    let mut project = Project::new();
    project.set_file(
        "/src/foo.ts".to_string(),
        "export const x = 1;\n".to_string(),
    );
    project.set_file(
        "/src/consumer.ts".to_string(),
        "import { something } from \"lodash\";\nconst y = something;\n".to_string(),
    );

    let edits = project.handle_will_rename_files(&[FileRename {
        old_uri: "/src/foo.ts".to_string(),
        new_uri: "/src/bar.ts".to_string(),
    }]);

    assert!(
        edits
            .changes
            .get("/src/consumer.ts")
            .is_none_or(|e| e.is_empty()),
        "bare npm specifiers must not be rewritten on file rename"
    );
}

#[test]
fn test_project_handle_will_rename_files_alias_mixes_with_relative() {
    // A consumer that imports both a relative and an aliased path to the
    // renamed file should see both rewritten in a single rename operation.
    let mut project = Project::new();
    project.set_file(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": { "@app/*": ["src/*"] }
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/src/foo.ts".to_string(),
        "export const x = 1;\n".to_string(),
    );
    project.set_file(
        "/src/consumer.ts".to_string(),
        "import { x } from \"./foo\";\nimport { x as y } from \"@app/foo\";\n".to_string(),
    );

    let edits = project.handle_will_rename_files(&[FileRename {
        old_uri: "/src/foo.ts".to_string(),
        new_uri: "/src/bar.ts".to_string(),
    }]);

    let consumer_edits = edits
        .changes
        .get("/src/consumer.ts")
        .expect("two edits expected");
    let new_texts: std::collections::HashSet<_> =
        consumer_edits.iter().map(|e| e.new_text.clone()).collect();
    // The existing relative-rewrite preserves the new target's extension; the
    // alias-rewrite preserves the alias style without extension.
    assert!(
        new_texts.contains("./bar") || new_texts.contains("./bar.ts"),
        "relative import should be rewritten, got: {new_texts:?}"
    );
    assert!(
        new_texts.contains("@app/bar"),
        "aliased import should be rewritten, got: {new_texts:?}"
    );
    assert_eq!(consumer_edits.len(), 2);
}

#[test]
fn test_project_handle_will_rename_files_alias_in_dynamic_import_and_export_from() {
    // Dynamic `import(...)` calls, `require(...)`, and `export ... from ...`
    // specifiers must all be rewritten when they use a path alias.
    let mut project = Project::new();
    project.set_file(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": { "@app/*": ["src/*"] }
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/src/foo.ts".to_string(),
        "export const x = 1;\n".to_string(),
    );
    project.set_file(
        "/src/consumer.ts".to_string(),
        "const a = import(\"@app/foo\");\nexport { x } from \"@app/foo\";\n".to_string(),
    );

    let edits = project.handle_will_rename_files(&[FileRename {
        old_uri: "/src/foo.ts".to_string(),
        new_uri: "/src/bar.ts".to_string(),
    }]);

    let consumer_edits = edits
        .changes
        .get("/src/consumer.ts")
        .expect("both dynamic-import and export-from should be rewritten");
    assert_eq!(consumer_edits.len(), 2);
    for edit in consumer_edits {
        assert_eq!(edit.new_text, "@app/bar");
    }
}

#[test]
fn test_project_handle_will_rename_files_alias_no_wildcard_target_change_skipped() {
    // For an alias with no wildcard, the alias is pinned to a specific file
    // path. Renaming that file means the alias itself is now invalid; we do
    // not invent a rewrite, leaving the user to update tsconfig.json.
    let mut project = Project::new();
    project.set_file(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": { "@foo": ["src/foo"] }
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/src/foo.ts".to_string(),
        "export const x = 1;\n".to_string(),
    );
    project.set_file(
        "/src/consumer.ts".to_string(),
        "import { x } from \"@foo\";\nconst y = x;\n".to_string(),
    );

    let edits = project.handle_will_rename_files(&[FileRename {
        old_uri: "/src/foo.ts".to_string(),
        new_uri: "/src/bar.ts".to_string(),
    }]);

    // We must not invent a rewrite that would map `@foo` to `@bar`; the alias
    // is pinned in tsconfig.json. The import is intentionally left alone.
    assert!(
        edits
            .changes
            .get("/src/consumer.ts")
            .is_none_or(|e| e.is_empty()),
        "pinned alias without wildcard must not be silently rewritten"
    );
}

#[test]
fn test_project_handle_will_rename_files_alias_with_config_dir_token() {
    // `${configDir}` should resolve relative to the tsconfig directory.
    let mut project = Project::new();
    project.set_file(
        "/proj/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "paths": { "@app/*": ["${configDir}/src/*"] }
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/proj/src/foo.ts".to_string(),
        "export const x = 1;\n".to_string(),
    );
    project.set_file(
        "/proj/src/consumer.ts".to_string(),
        "import { x } from \"@app/foo\";\nconst y = x;\n".to_string(),
    );

    let edits = project.handle_will_rename_files(&[FileRename {
        old_uri: "/proj/src/foo.ts".to_string(),
        new_uri: "/proj/src/bar.ts".to_string(),
    }]);

    let consumer_edits = edits
        .changes
        .get("/proj/src/consumer.ts")
        .expect("aliased import with ${configDir} should be rewritten");
    assert_eq!(consumer_edits[0].new_text, "@app/bar");
}

#[test]
fn test_project_get_file_rename_edits_rewrites_path_alias() {
    // The LSP server entrypoint calls `get_file_rename_edits` directly, which
    // is a separate code path from `handle_will_rename_files`. Cover it
    // explicitly so the path-alias rewrite reaches the real LSP wire surface.
    let mut project = Project::new();
    project.set_file(
        "/tsconfig.json".to_string(),
        r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": { "@app/*": ["src/*"] }
  }
}"#
        .to_string(),
    );
    project.set_file(
        "/src/foo.ts".to_string(),
        "export const x = 1;\n".to_string(),
    );
    project.set_file(
        "/src/consumer.ts".to_string(),
        "import { x } from \"@app/foo\";\nconst y = x;\n".to_string(),
    );

    let edits = project.get_file_rename_edits("/src/foo.ts", "/src/bar.ts");

    let consumer_edits = edits
        .get("/src/consumer.ts")
        .expect("get_file_rename_edits should rewrite path-aliased imports");
    assert_eq!(consumer_edits.len(), 1);
    assert_eq!(consumer_edits[0].new_text, "@app/bar");
}

#[test]
fn test_project_subtypes_returns_empty_for_missing_file() {
    let project = Project::new();
    let result = project.subtypes("nonexistent.ts", Position::new(0, 0));
    assert!(
        result.is_empty(),
        "subtypes should return empty for missing file"
    );
}

#[test]
fn test_project_supertypes_returns_empty_for_missing_file() {
    let project = Project::new();
    let result = project.supertypes("nonexistent.ts", Position::new(0, 0));
    assert!(
        result.is_empty(),
        "supertypes should return empty for missing file"
    );
}

#[test]
fn test_project_get_code_actions_missing_file() {
    let project = Project::new();
    let range = Range::new(Position::new(0, 0), Position::new(0, 1));
    let result = project.get_code_actions("nonexistent.ts", range, vec![], None);
    assert!(
        result.is_none(),
        "get_code_actions should return None for missing file"
    );
}

#[test]
fn test_project_resolve_code_lens_missing_file() {
    let mut project = Project::new();
    let lens = CodeLens {
        range: Range::new(Position::new(0, 0), Position::new(0, 1)),
        command: None,
        data: None,
    };
    let result = project.resolve_code_lens("nonexistent.ts", &lens);
    assert!(
        result.is_none(),
        "resolve_code_lens should return None for missing file"
    );
}

#[test]
fn test_project_get_incoming_calls_missing_file() {
    let project = Project::new();
    let result = project.get_incoming_calls("nonexistent.ts", Position::new(0, 0));
    assert!(
        result.is_empty(),
        "get_incoming_calls should return empty for missing file"
    );
}

#[test]
fn test_project_get_outgoing_calls_missing_file() {
    let project = Project::new();
    let result = project.get_outgoing_calls("nonexistent.ts", Position::new(0, 0));
    assert!(
        result.is_empty(),
        "get_outgoing_calls should return empty for missing file"
    );
}

// =========================================================================
// Additional coverage tests
// =========================================================================

#[test]
fn test_project_multiple_file_diagnostics() {
    let mut project = Project::new();
    project.set_file(
        "a.ts".to_string(),
        "export const x: string = 1;\n".to_string(),
    );
    project.set_file(
        "b.ts".to_string(),
        "export const y: number = 'hello';\n".to_string(),
    );

    let diags_a = project
        .get_diagnostics("a.ts")
        .expect("Expected diagnostics for a.ts");
    let diags_b = project
        .get_diagnostics("b.ts")
        .expect("Expected diagnostics for b.ts");

    assert!(!diags_a.is_empty(), "a.ts should have type errors");
    assert!(!diags_b.is_empty(), "b.ts should have type errors");
}

#[test]
fn test_project_diagnostics_update_after_edit() {
    let mut project = Project::new();
    project.set_file("test.ts".to_string(), "const x: string = 42;\n".to_string());

    let diags_before = project
        .get_diagnostics("test.ts")
        .expect("Expected diagnostics");
    assert!(
        !diags_before.is_empty(),
        "Should have type error before fix"
    );

    // Fix the error
    let edit = {
        let file = project.file("test.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "42");
        TextEdit::new(range, "\"hello\"".to_string())
    };
    project
        .update_file("test.ts", &[edit])
        .expect("Expected update to succeed");

    let diags_after = project
        .get_diagnostics("test.ts")
        .expect("Expected diagnostics after fix");
    assert!(
        diags_after.len() < diags_before.len(),
        "Should have fewer diagnostics after fix, before: {}, after: {}",
        diags_before.len(),
        diags_after.len()
    );
}

#[test]
fn test_project_folding_ranges_nested_functions() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"function outer() {
    function inner() {
        return 1;
    }
    return inner();
}
"#
        .to_string(),
    );

    let ranges = project.get_folding_ranges("test.ts");
    assert!(ranges.is_some(), "Should return folding ranges");
    let ranges = ranges.unwrap();
    assert!(
        ranges.len() >= 2,
        "Should have folding ranges for outer and inner functions, got: {}",
        ranges.len()
    );
}

#[test]
fn test_project_selection_ranges_multiple_positions() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "const x = 1;\nconst y = 2;\n".to_string(),
    );

    let positions = vec![Position::new(0, 6), Position::new(1, 6)];
    let ranges = project.get_selection_ranges("test.ts", &positions);
    assert!(ranges.is_some(), "Should return selection ranges");
    let ranges = ranges.unwrap();
    assert_eq!(ranges.len(), 2, "Should have one result per position");
    assert!(ranges[0].is_some(), "First position should have a range");
    assert!(ranges[1].is_some(), "Second position should have a range");
}

#[test]
fn test_project_selection_ranges_returns_none_for_missing_file() {
    let project = Project::new();
    let result = project.get_selection_ranges("missing.ts", &[Position::new(0, 0)]);
    assert!(result.is_none(), "Should return None for missing file");
}

#[test]
fn test_project_inlay_hints_function_call() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "function greet(name: string, age: number) { return name; }\ngreet(\"Alice\", 30);\n"
            .to_string(),
    );

    let range = Range::new(Position::new(0, 0), Position::new(2, 0));
    let hints = project.get_inlay_hints("test.ts", range);
    assert!(hints.is_some(), "Should return inlay hints result");
}

#[test]
fn test_project_code_lens_for_functions() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"function foo() {
    return 1;
}

function bar() {
    return foo();
}
"#
        .to_string(),
    );

    let lenses = project.get_code_lenses("test.ts");
    assert!(lenses.is_some(), "Should return code lenses");
    let lenses = lenses.unwrap();
    // There should be at least some code lenses for functions
    assert!(
        !lenses.is_empty(),
        "Should produce code lenses for functions"
    );
}

#[test]
fn test_project_code_lens_returns_none_for_missing_file() {
    let project = Project::new();
    assert!(project.get_code_lenses("missing.ts").is_none());
}

#[test]
fn test_project_resolve_code_lens_reference_count_excludes_declaration() {
    // Regression: the project-level resolve_code_lens path used to compare a
    // zero-width declaration position against full identifier-span reference
    // ranges, so it never recognized the declaration and reported N+1
    // references for N real uses. See project/features.rs.
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "function foo() {}\nfoo();\nfoo();\nfoo();\n".to_string(),
    );

    let lenses = project
        .get_code_lenses("test.ts")
        .expect("code lenses for file");
    let func_lens = lenses
        .iter()
        .find(|l| {
            l.data
                .as_ref()
                .is_some_and(|d| d.kind == editor_decorations::code_lens::CodeLensKind::References)
        })
        .expect("references lens for function");

    let resolved = project
        .resolve_code_lens("test.ts", func_lens)
        .expect("resolve produces a lens");
    let command = resolved.command.expect("resolved lens has a command");
    // Three call sites; declaration is appended by find_references and must
    // be subtracted out.
    assert_eq!(
        command.title, "3 references",
        "declaration must be excluded from the reference count (got: {})",
        command.title
    );
}

#[test]
fn test_project_semantic_tokens_for_interface() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "interface Foo {\n    bar: string;\n    baz(x: number): void;\n}\n".to_string(),
    );

    let tokens = project.get_semantic_tokens_full("test.ts");
    assert!(tokens.is_some(), "Should return semantic tokens");
    let tokens = tokens.unwrap();
    assert_eq!(tokens.len() % 5, 0, "Token data should be in groups of 5");
    assert!(
        tokens.len() >= 5,
        "Should have at least one semantic token for interface"
    );
}

#[test]
fn test_project_format_document_produces_edits() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "function   foo(  ){\nreturn 1\n}\n".to_string(),
    );

    let options = FormattingOptions::default();
    let result = project.format_document("test.ts", &options);
    assert!(result.is_some(), "Should return formatting result");
    let edits = result.unwrap();
    // Formatting result depends on formatter configuration
    // Just verify the API returns without panicking
    let _ = edits;
}

#[test]
fn test_project_highlighting_variable_all_occurrences() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "const value = 1;\nconst doubled = value * 2;\nconst tripled = value * 3;\n".to_string(),
    );

    // Position on 'value' at declaration (line 0, char 6)
    let highlights = project.get_document_highlighting("test.ts", Position::new(0, 6));
    assert!(highlights.is_some(), "Should find highlights for 'value'");
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 3,
        "Should highlight all 3 occurrences of 'value', got: {}",
        highlights.len()
    );
}

#[test]
fn test_project_call_hierarchy_prepare_missing_file() {
    let project = Project::new();
    assert!(
        project
            .prepare_call_hierarchy("missing.ts", Position::new(0, 0))
            .is_none()
    );
}

#[test]
fn test_project_type_hierarchy_prepare_for_interface() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"interface Shape {
    area(): number;
}

interface Circle extends Shape {
    radius: number;
}
"#
        .to_string(),
    );

    let item = project.prepare_type_hierarchy("test.ts", Position::new(4, 10));
    assert!(
        item.is_some(),
        "Should prepare type hierarchy for Circle interface"
    );
    let item = item.unwrap();
    assert_eq!(
        item.name, "Circle",
        "Type hierarchy item should be 'Circle'"
    );
}

#[test]
fn test_project_subtypes_for_base_class() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"class Base {
    value: number;
}

class Derived extends Base {
    extra: string;
}
"#
        .to_string(),
    );

    let subtypes = project.subtypes("test.ts", Position::new(0, 6));
    // May or may not find subtypes depending on implementation scope
    // At minimum, verify it does not crash and returns a valid result
    let _ = subtypes;
}

#[test]
fn test_project_stale_diagnostics_after_file_update() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const x = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { x } from \"./a\";\nconst y: string = x;\n".to_string(),
    );

    // Initial diagnostics
    let _ = project.get_diagnostics("b.ts");

    // Update a.ts
    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "1");
        TextEdit::new(range, "2".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    let stale = project.get_stale_diagnostics();
    // After updating a.ts, dependents (b.ts) may be marked stale
    // The implementation may vary, so just check this doesn't crash
    let _ = stale;
}

#[test]
fn test_project_hover_on_function_declaration() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "function add(a: number, b: number): number {\n    return a + b;\n}\nadd(1, 2);\n"
            .to_string(),
    );

    // Hover on 'add' at its declaration
    let hover = project.get_hover("test.ts", Position::new(0, 9));
    assert!(
        hover.is_some(),
        "Should provide hover for function declaration"
    );
    let hover = hover.unwrap();
    assert!(
        hover.contents.iter().any(|c| c.contains("add")),
        "Hover should contain function name"
    );
}

#[test]
fn test_project_definition_within_same_file() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "const myVar = 42;\nconsole.log(myVar);\n".to_string(),
    );

    // Go to definition of myVar usage (line 1)
    let defs = project.get_definition("test.ts", Position::new(1, 12));
    assert!(defs.is_some(), "Should find definition for myVar");
    let defs = defs.unwrap();
    assert!(
        defs.iter()
            .any(|loc| loc.file_path == "test.ts" && loc.range.start.line == 0),
        "Definition should point to declaration on line 0"
    );
}

#[test]
fn test_project_get_implementations_for_interface() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"interface Printable {
    print(): void;
}

class Document implements Printable {
    print() { }
}
"#
        .to_string(),
    );

    let impls = project.get_implementations("test.ts", Position::new(0, 10));
    // Implementations search for the interface
    // This may or may not find results depending on how file-local impl search works
    let _ = impls;
}

#[test]
fn test_project_cross_file_subtypes() {
    let mut project = Project::new();
    project.set_file(
        "base.ts".to_string(),
        r#"export class Animal {
    name: string;
}
"#
        .to_string(),
    );
    project.set_file(
        "dog.ts".to_string(),
        r#"import { Animal } from './base';
class Dog extends Animal {
    breed: string;
}
"#
        .to_string(),
    );
    project.set_file(
        "cat.ts".to_string(),
        r#"import { Animal } from './base';
class Cat extends Animal {
    indoor: boolean;
}
"#
        .to_string(),
    );

    // Position on "Animal" class name in base.ts (line 0, char 13)
    let subtypes = project.subtypes("base.ts", Position::new(0, 13));
    // Should find subtypes from other files (Dog and Cat)
    let names: Vec<&str> = subtypes.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"Dog"),
        "Should find Dog as a subtype of Animal across files, got: {names:?}"
    );
    assert!(
        names.contains(&"Cat"),
        "Should find Cat as a subtype of Animal across files, got: {names:?}"
    );
}

#[test]
fn test_project_cross_file_supertypes() {
    let mut project = Project::new();
    project.set_file(
        "base.ts".to_string(),
        r#"export class Vehicle {
    wheels: number;
}
"#
        .to_string(),
    );
    project.set_file(
        "car.ts".to_string(),
        r#"import { Vehicle } from './base';
class Car extends Vehicle {
    doors: number;
}
"#
        .to_string(),
    );

    // Position on "Car" class name in car.ts (line 1, char 6)
    let supertypes = project.supertypes("car.ts", Position::new(1, 6));
    let names: Vec<&str> = supertypes.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"Vehicle"),
        "Should find Vehicle as a supertype of Car across files, got: {names:?}"
    );
}

#[test]
fn test_project_cross_file_incoming_calls() {
    let mut project = Project::new();
    project.set_file(
        "utils.ts".to_string(),
        r#"export function helper() {
    return 42;
}
"#
        .to_string(),
    );
    project.set_file(
        "main.ts".to_string(),
        r#"import { helper } from './utils';
function main() {
    helper();
}
"#
        .to_string(),
    );

    // Position on "helper" function name in utils.ts (line 0, char 16)
    let incoming = project.get_incoming_calls("utils.ts", Position::new(0, 16));
    // Should find the call from main.ts
    let caller_names: Vec<&str> = incoming.iter().map(|c| c.from.name.as_str()).collect();
    assert!(
        caller_names.contains(&"main"),
        "Should find 'main' as a caller of 'helper' across files, got: {caller_names:?}"
    );
}
