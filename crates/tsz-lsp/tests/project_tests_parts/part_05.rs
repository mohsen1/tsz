#[test]
fn test_project_scope_cache_reuse_hover_to_signature_help() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "function foo(a: number, b: string) {}\nfoo(1, \"x\");\n".to_string(),
    );
    let hover_position = Position::new(1, 0);
    // Position must be inside the call args (after the opening paren)
    let signature_position = Position::new(1, 4);

    assert!(project.get_hover("a.ts", hover_position).is_some());
    assert!(
        project
            .get_signature_help("a.ts", signature_position)
            .is_some()
    );

    let timing = project
        .performance()
        .timing(ProjectRequestKind::SignatureHelp)
        .expect("Expected timing data for signature help");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected signature help to reuse cached scope"
    );
}

#[test]
fn test_project_scope_cache_reuse_hover_to_references() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value = 1;\nvalue;\n".to_string());
    let position = Position::new(1, 0);

    assert!(project.get_hover("a.ts", position).is_some());
    assert!(project.find_references("a.ts", position).is_some());

    let timing = project
        .performance()
        .timing(ProjectRequestKind::References)
        .expect("Expected timing data for references");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected references to reuse cached scope"
    );
}

#[test]
#[ignore = "TODO: LSP scope cache reuse"]
fn test_project_scope_cache_reuse_hover_to_rename() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value = 1;\nvalue;\n".to_string());
    let position = Position::new(1, 0);

    assert!(project.get_hover("a.ts", position).is_some());
    let _ = project
        .get_rename_edits("a.ts", position, "next".to_string())
        .expect("Expected rename edits");

    let timing = project
        .performance()
        .timing(ProjectRequestKind::Rename)
        .expect("Expected timing data for rename");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected rename to reuse cached scope"
    );
}

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

