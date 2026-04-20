#[test]
fn test_project_update_file_remove_suffix_preserves_prefix_symbol() {
    let mut project = Project::new();
    let source = "const alpha = 1;\nconst beta = 2;\n";
    project.set_file("a.ts".to_string(), source.to_string());

    let alpha_symbol_before = {
        let file = project.file("a.ts").unwrap();
        let arena = file.arena();
        let root = file.root();
        let source_node = arena.get(root).unwrap();
        let source_file = arena.get_source_file(source_node).unwrap();
        let stmt_idx = source_file.statements.nodes[0];
        let stmt_node = arena.get(stmt_idx).unwrap();
        let var_stmt = arena.get_variable(stmt_node).unwrap();
        let decl_list_idx = var_stmt.declarations.nodes[0];
        let decl_list_node = arena.get(decl_list_idx).unwrap();
        let decl_list = arena.get_variable(decl_list_node).unwrap();
        let decl_idx = decl_list.declarations.nodes[0];
        let decl_node = arena.get(decl_idx).unwrap();
        let decl = arena.get_variable_declaration(decl_node).unwrap();
        let name_idx = decl.name;
        file.binder()
            .get_node_symbol(name_idx)
            .expect("Expected symbol for alpha")
    };

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "const beta = 2;\n");
        TextEdit::new(range, "".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    let alpha_symbol_after = {
        let file = project.file("a.ts").unwrap();
        let arena = file.arena();
        let root = file.root();
        let source_node = arena.get(root).unwrap();
        let source_file = arena.get_source_file(source_node).unwrap();
        let stmt_idx = source_file.statements.nodes[0];
        let stmt_node = arena.get(stmt_idx).unwrap();
        let var_stmt = arena.get_variable(stmt_node).unwrap();
        let decl_list_idx = var_stmt.declarations.nodes[0];
        let decl_list_node = arena.get(decl_list_idx).unwrap();
        let decl_list = arena.get_variable(decl_list_node).unwrap();
        let decl_idx = decl_list.declarations.nodes[0];
        let decl_node = arena.get(decl_idx).unwrap();
        let decl = arena.get_variable_declaration(decl_node).unwrap();
        let name_idx = decl.name;
        file.binder()
            .get_node_symbol(name_idx)
            .expect("Expected symbol for alpha after delete")
    };

    let file = project.file("a.ts").unwrap();
    assert_eq!(file.source_text(), "const alpha = 1;\n");
    assert_eq!(alpha_symbol_before, alpha_symbol_after);
    let locals = &file.binder().file_locals;
    assert!(locals.has("alpha"));
    assert!(!locals.has("beta"));
}

#[test]
fn test_project_update_file_preserves_multiple_prefix_symbols() {
    let mut project = Project::new();
    let source = "const alpha = 1;\nconst beta = 2;\nconst gamma = 3;\n";
    project.set_file("a.ts".to_string(), source.to_string());

    let (alpha_symbol_before, beta_symbol_before) = {
        let file = project.file("a.ts").unwrap();
        let arena = file.arena();
        let root = file.root();
        let source_node = arena.get(root).unwrap();
        let source_file = arena.get_source_file(source_node).unwrap();

        let alpha_stmt_idx = source_file.statements.nodes[0];
        let alpha_stmt_node = arena.get(alpha_stmt_idx).unwrap();
        let alpha_stmt = arena.get_variable(alpha_stmt_node).unwrap();
        let alpha_decl_list_idx = alpha_stmt.declarations.nodes[0];
        let alpha_decl_list_node = arena.get(alpha_decl_list_idx).unwrap();
        let alpha_decl_list = arena.get_variable(alpha_decl_list_node).unwrap();
        let alpha_decl_idx = alpha_decl_list.declarations.nodes[0];
        let alpha_decl_node = arena.get(alpha_decl_idx).unwrap();
        let alpha_decl = arena.get_variable_declaration(alpha_decl_node).unwrap();
        let alpha_name_idx = alpha_decl.name;

        let beta_stmt_idx = source_file.statements.nodes[1];
        let beta_stmt_node = arena.get(beta_stmt_idx).unwrap();
        let beta_stmt = arena.get_variable(beta_stmt_node).unwrap();
        let beta_decl_list_idx = beta_stmt.declarations.nodes[0];
        let beta_decl_list_node = arena.get(beta_decl_list_idx).unwrap();
        let beta_decl_list = arena.get_variable(beta_decl_list_node).unwrap();
        let beta_decl_idx = beta_decl_list.declarations.nodes[0];
        let beta_decl_node = arena.get(beta_decl_idx).unwrap();
        let beta_decl = arena.get_variable_declaration(beta_decl_node).unwrap();
        let beta_name_idx = beta_decl.name;

        let alpha_sym = file
            .binder()
            .get_node_symbol(alpha_name_idx)
            .expect("Expected symbol for alpha");
        let beta_sym = file
            .binder()
            .get_node_symbol(beta_name_idx)
            .expect("Expected symbol for beta");
        (alpha_sym, beta_sym)
    };

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "gamma");
        TextEdit::new(range, "delta".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    let (alpha_symbol_after, beta_symbol_after) = {
        let file = project.file("a.ts").unwrap();
        let arena = file.arena();
        let root = file.root();
        let source_node = arena.get(root).unwrap();
        let source_file = arena.get_source_file(source_node).unwrap();

        let alpha_stmt_idx = source_file.statements.nodes[0];
        let alpha_stmt_node = arena.get(alpha_stmt_idx).unwrap();
        let alpha_stmt = arena.get_variable(alpha_stmt_node).unwrap();
        let alpha_decl_list_idx = alpha_stmt.declarations.nodes[0];
        let alpha_decl_list_node = arena.get(alpha_decl_list_idx).unwrap();
        let alpha_decl_list = arena.get_variable(alpha_decl_list_node).unwrap();
        let alpha_decl_idx = alpha_decl_list.declarations.nodes[0];
        let alpha_decl_node = arena.get(alpha_decl_idx).unwrap();
        let alpha_decl = arena.get_variable_declaration(alpha_decl_node).unwrap();
        let alpha_name_idx = alpha_decl.name;

        let beta_stmt_idx = source_file.statements.nodes[1];
        let beta_stmt_node = arena.get(beta_stmt_idx).unwrap();
        let beta_stmt = arena.get_variable(beta_stmt_node).unwrap();
        let beta_decl_list_idx = beta_stmt.declarations.nodes[0];
        let beta_decl_list_node = arena.get(beta_decl_list_idx).unwrap();
        let beta_decl_list = arena.get_variable(beta_decl_list_node).unwrap();
        let beta_decl_idx = beta_decl_list.declarations.nodes[0];
        let beta_decl_node = arena.get(beta_decl_idx).unwrap();
        let beta_decl = arena.get_variable_declaration(beta_decl_node).unwrap();
        let beta_name_idx = beta_decl.name;

        let alpha_sym = file
            .binder()
            .get_node_symbol(alpha_name_idx)
            .expect("Expected symbol for alpha after update");
        let beta_sym = file
            .binder()
            .get_node_symbol(beta_name_idx)
            .expect("Expected symbol for beta after update");
        (alpha_sym, beta_sym)
    };

    let file = project.file("a.ts").unwrap();
    assert_eq!(
        file.source_text(),
        "const alpha = 1;\nconst beta = 2;\nconst delta = 3;\n"
    );
    assert_eq!(alpha_symbol_before, alpha_symbol_after);
    assert_eq!(beta_symbol_before, beta_symbol_after);
    let locals = &file.binder().file_locals;
    assert!(locals.has("alpha"));
    assert!(locals.has("beta"));
    assert!(locals.has("delta"));
}

#[test]
fn test_project_update_file_refreshes_cross_file_references() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "export const foo = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { foo } from \"./a\";\nfoo;\n".to_string(),
    );

    let before_refs = project
        .find_references("b.ts", Position::new(1, 0))
        .expect("Expected references for foo");
    assert!(before_refs.iter().any(|loc| loc.file_path == "a.ts"));

    let rename_edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "foo");
        TextEdit::new(range, "bar".to_string())
    };
    project
        .update_file("a.ts", &[rename_edit])
        .expect("Expected update to succeed");

    let after_refs = project
        .find_references("b.ts", Position::new(1, 0))
        .expect("Expected references for foo");
    assert!(after_refs.iter().all(|loc| loc.file_path != "a.ts"));
}

#[test]
fn test_project_hover_includes_jsdoc() {
    let mut project = Project::new();
    let source = "/** The answer */\nconst x = 42;\nx;";
    project.set_file("a.ts".to_string(), source.to_string());

    let info = project
        .get_hover("a.ts", Position::new(2, 0))
        .expect("Expected hover info");

    assert!(
        info.contents
            .iter()
            .any(|content| content.contains("The answer"))
    );
}

#[test]
fn test_project_signature_help_includes_jsdoc() {
    let mut project = Project::new();
    let source = "/** Adds two numbers. */\nfunction add(a: number, b: number): number { return a + b; }\nadd(1, 2);";
    project.set_file("a.ts".to_string(), source.to_string());

    let pos = {
        let file = project.file("a.ts").unwrap();
        range_for_substring(file.source_text(), file.line_map(), "1").start
    };

    let help = project
        .get_signature_help("a.ts", pos)
        .expect("Expected signature help");

    let doc = help.signatures[help.active_signature as usize]
        .documentation
        .clone()
        .unwrap_or_default();
    assert_eq!(doc, "Adds two numbers.");
}

#[test]
fn test_project_completions_auto_import_named() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "export const foo = 1;\n".to_string());
    project.set_file("b.ts".to_string(), "foo;\n".to_string());

    let items = project
        .get_completions("b.ts", Position::new(0, 1))
        .expect("Expected completions");

    let has_auto_import = items.iter().any(|item| {
        if item.label != "foo" {
            return false;
        }
        let detail = item.detail.as_deref().unwrap_or("");
        let doc = item.documentation.as_deref().unwrap_or("");
        detail.contains("auto-import")
            && detail.contains("./a")
            && doc.contains("import { foo } from \"./a\";")
            && item.additional_text_edits.is_some()
    });

    assert!(
        has_auto_import,
        "Should include auto-import completion for foo with additionalTextEdits"
    );
}

#[test]
fn test_project_completions_auto_import_function_kind() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "export function foo() {}\n".to_string());
    project.set_file("b.ts".to_string(), "fo\n".to_string());

    let items = project
        .get_completions("b.ts", Position::new(0, 2))
        .expect("Expected completions");

    let foo = items
        .iter()
        .find(|item| item.label == "foo" && item.source.as_deref() == Some("./a"))
        .expect("Expected auto-import completion for foo from ./a");
    assert_eq!(
        foo.kind,
        crate::completions::CompletionItemKind::Function,
        "Auto-import completion should preserve function kind"
    );
    assert_eq!(
        foo.kind_modifiers.as_deref(),
        Some("export"),
        "Auto-import completion should mark entries as exported"
    );
}

#[test]
fn test_project_completions_preserve_keyword_order_when_auto_imports_present() {
    let mut project = Project::new();
    project.set_file(
        "/lib/main.ts".to_string(),
        "export const Button = 1;\n".to_string(),
    );
    project.set_file("/index.ts".to_string(), "Button".to_string());

    let items = project
        .get_completions("/index.ts", Position::new(0, 6))
        .expect("Expected completions");
    let names: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();

    let abstract_idx = names
        .iter()
        .position(|name| *name == "abstract")
        .expect("Expected keyword 'abstract' in completions");
    let array_idx = names
        .iter()
        .position(|name| *name == "Array")
        .expect("Expected global 'Array' in completions");
    assert!(
        abstract_idx < array_idx,
        "Expected keyword completions to keep tsserver-style ordering ahead of globals"
    );
}

#[test]
fn test_project_completions_prefix_matching() {
    let mut project = Project::new();

    // a.ts exports multiple symbols with different prefixes
    project.set_file(
        "a.ts".to_string(),
        "export const useHook = 1;\nexport const useState = 2;\nexport const foo = 3;\n"
            .to_string(),
    );

    // b.ts tries to use "use" - should get both useHook and useState
    project.set_file("b.ts".to_string(), "use".to_string());

    let items = project
        .get_completions("b.ts", Position::new(0, 3))
        .expect("Expected completions");

    // Should have completions for symbols starting with "use"
    let use_completions: Vec<_> = items
        .iter()
        .filter(|item| item.label.starts_with("use"))
        .collect();

    assert!(
        use_completions.len() >= 2,
        "Should have at least 2 completions starting with 'use', got {}",
        use_completions.len()
    );

    // Should have auto-import for useHook
    let has_use_hook = items.iter().any(|item| {
        item.label == "useHook" && item.detail.as_deref().unwrap_or("").contains("auto-import")
    });
    assert!(
        has_use_hook,
        "Should have auto-import completion for useHook"
    );

    // Should have auto-import for useState
    let has_use_state = items.iter().any(|item| {
        item.label == "useState" && item.detail.as_deref().unwrap_or("").contains("auto-import")
    });
    assert!(
        has_use_state,
        "Should have auto-import completion for useState"
    );
}

#[test]
fn test_project_completions_include_export_equals_auto_import_when_name_already_completes() {
    let mut project = Project::new();
    project.set_file(
        "/ts.d.ts".to_string(),
        r#"declare namespace ts {
  interface SourceFile {
    text: string;
  }
}
export = ts;
"#
        .to_string(),
    );
    project.set_file(
        "/types.ts".to_string(),
        "export interface VFS {\n  getSourceFile(path: string): ts\n}\n".to_string(),
    );

    let file = project.file("/types.ts").expect("Expected /types.ts");
    let ts_range = range_for_substring(file.source_text(), file.line_map(), "ts\n");
    let items = project
        .get_completions("/types.ts", ts_range.start)
        .expect("Expected completions");

    let ts_auto_import = items
        .iter()
        .find(|item| item.label == "ts" && item.source.as_deref() == Some("./ts"))
        .expect("Expected auto-import completion for `ts` from `./ts`");

    assert!(ts_auto_import.has_action);
    assert!(
        ts_auto_import.additional_text_edits.is_some(),
        "Expected auto-import completion to include text edits"
    );
}

