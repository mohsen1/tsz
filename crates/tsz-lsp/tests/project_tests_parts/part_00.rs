#[test]
fn test_project_cross_file_references_named_import() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "export const foo = 1;\nfoo;\n".to_string(),
    );
    project.set_file(
        "b.ts".to_string(),
        "import { foo } from \"./a\";\nfoo;\n".to_string(),
    );

    let refs = project.find_references("b.ts", Position::new(1, 0));
    assert!(refs.is_some(), "Should find references for imported foo");

    let refs = refs.unwrap();
    assert!(
        refs.iter().any(|loc| loc.file_path == "a.ts"),
        "Should include references from a.ts"
    );
    assert!(
        refs.iter().any(|loc| loc.file_path == "b.ts"),
        "Should include references from b.ts"
    );
}

#[test]
fn test_project_cross_file_references_default_import() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "export default function foo() {}\nfoo();".to_string(),
    );
    project.set_file(
        "b.ts".to_string(),
        "import foo from \"./a\";\nfoo();".to_string(),
    );

    let refs = project.find_references("b.ts", Position::new(1, 0));
    assert!(refs.is_some(), "Should find references for default import");

    let refs = refs.unwrap();
    assert!(
        refs.iter().any(|loc| loc.file_path == "a.ts"),
        "Should include references from a.ts"
    );
    assert!(
        refs.iter().any(|loc| loc.file_path == "b.ts"),
        "Should include references from b.ts"
    );
}

#[test]
fn test_project_cross_file_references_namespace_import() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "export const foo = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import * as ns from \"./a\";\nns.foo;\n".to_string(),
    );

    let refs = project.find_references("a.ts", Position::new(0, 13));
    assert!(
        refs.is_some(),
        "Should find references for namespace import"
    );

    let refs = refs.unwrap();
    assert!(
        refs.iter().any(|loc| loc.file_path == "b.ts"),
        "Should include references from b.ts"
    );
}

#[test]
fn test_project_cross_file_references_tsx_import() {
    let mut project = Project::new();

    project.set_file("a.tsx".to_string(), "export const foo = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { foo } from \"./a\";\nfoo;\n".to_string(),
    );

    let refs = project.find_references("b.ts", Position::new(1, 0));
    assert!(refs.is_some(), "Should find references for tsx import");

    let refs = refs.unwrap();
    assert!(
        refs.iter().any(|loc| loc.file_path == "a.tsx"),
        "Should include references from a.tsx"
    );
    assert!(
        refs.iter().any(|loc| loc.file_path == "b.ts"),
        "Should include references from b.ts"
    );
}

#[test]
fn test_project_cross_file_references_quoted_export_name() {
    let mut project = Project::new();

    project.set_file(
        "foo.ts".to_string(),
        [
            "const foo = \"foo\";",
            "export { foo as \"__<alias>\" };",
            "import { \"__<alias>\" as bar } from \"./foo\";",
            "if (bar !== \"foo\") throw bar;",
            "",
        ]
        .join("\n"),
    );
    project.set_file(
        "bar.ts".to_string(),
        [
            "import { \"__<alias>\" as first } from \"./foo\";",
            "export { \"__<alias>\" as \"<other>\" } from \"./foo\";",
            "import { \"<other>\" as second } from \"./bar\";",
            "if (first !== \"foo\") throw first;",
            "if (second !== \"foo\") throw second;",
            "",
        ]
        .join("\n"),
    );

    let refs = project
        .find_references("bar.ts", Position::new(0, 10))
        .expect("Should find references for quoted export name");

    assert!(
        refs.iter().any(|loc| loc.file_path == "foo.ts"),
        "Should include references from foo.ts"
    );
    assert!(
        refs.iter().any(|loc| loc.file_path == "bar.ts"),
        "Should include references from bar.ts"
    );
}

#[test]
fn test_project_rename_cross_file() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "export const value = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { value } from \"./a\";\nvalue;\n".to_string(),
    );

    let edits = project
        .get_rename_edits("b.ts", Position::new(1, 0), "renamed".to_string())
        .expect("Expected rename edits");

    let a_file = &project.files["a.ts"];
    let b_file = project.file("b.ts").unwrap();
    let a_edits = edits.changes.get("a.ts").expect("Expected edits for a.ts");
    let b_edits = edits.changes.get("b.ts").expect("Expected edits for b.ts");

    let updated_a = apply_text_edits(a_file.source_text(), a_file.line_map(), a_edits);
    let updated_b = apply_text_edits(b_file.source_text(), b_file.line_map(), b_edits);

    assert_eq!(updated_a, "export const renamed = 1;\n");
    assert_eq!(updated_b, "import { renamed } from \"./a\";\nrenamed;\n");
}

#[test]
fn test_project_rename_cross_file_alias_import() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "export const value = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { value as alias } from \"./a\";\nalias;\n".to_string(),
    );

    let edits = project
        .get_rename_edits("a.ts", Position::new(0, 13), "renamed".to_string())
        .expect("Expected rename edits");

    let a_file = project.file("a.ts").unwrap();
    let b_file = project.file("b.ts").unwrap();
    let a_edits = edits.changes.get("a.ts").expect("Expected edits for a.ts");
    let b_edits = edits.changes.get("b.ts").expect("Expected edits for b.ts");

    let updated_a = apply_text_edits(a_file.source_text(), a_file.line_map(), a_edits);
    let updated_b = apply_text_edits(b_file.source_text(), b_file.line_map(), b_edits);

    assert_eq!(updated_a, "export const renamed = 1;\n");
    assert_eq!(
        updated_b,
        "import { renamed as alias } from \"./a\";\nalias;\n"
    );
}

#[test]
fn test_project_update_file_applies_edits() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "const value = 1;\n".to_string());

    let file = project.file("a.ts").unwrap();
    let range = range_for_substring(file.source_text(), file.line_map(), "1");
    let edit = TextEdit::new(range, "2".to_string());

    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    let updated = project.file("a.ts").unwrap().source_text();
    assert_eq!(updated, "const value = 2;\n");
}

#[test]
fn test_project_update_file_reuses_prefix_nodes() {
    let mut project = Project::new();
    let source = "const alpha = 1;\nconst beta = 2;\n";
    project.set_file("a.ts".to_string(), source.to_string());

    let (root_before, first_stmt_before, arena_len_before) = {
        let file = project.file("a.ts").unwrap();
        let arena = file.arena();
        let root = file.root();
        let source_node = arena.get(root).unwrap();
        let source_file = arena.get_source_file(source_node).unwrap();
        (root, source_file.statements.nodes[0], arena.len())
    };

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "beta");
        TextEdit::new(range, "gamma".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    let file = project.file("a.ts").unwrap();
    assert_eq!(file.source_text(), "const alpha = 1;\nconst gamma = 2;\n");

    let arena = file.arena();
    let root_after = file.root();
    let source_node = arena.get(root_after).unwrap();
    let source_file = arena.get_source_file(source_node).unwrap();
    assert_eq!(root_after, root_before);
    assert_eq!(source_file.statements.nodes[0], first_stmt_before);
    assert!(
        arena.len() > arena_len_before,
        "Expected incremental parse to append nodes"
    );

    let parent = arena.get_extended(first_stmt_before).unwrap().parent;
    assert_eq!(parent, root_after);
}

#[test]
fn test_project_update_file_reuses_binder_prefix_symbols() {
    let mut project = Project::new();
    let source = "const alpha = 1;\nconst beta = 2;\n";
    project.set_file("a.ts".to_string(), source.to_string());

    let alpha_symbol_before = {
        let file = project.file("a.ts").unwrap();
        file.binder()
            .file_locals
            .get("alpha")
            .expect("Expected symbol for alpha")
    };

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "beta");
        TextEdit::new(range, "gamma".to_string())
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
}

