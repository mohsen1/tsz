#[test]
fn test_project_update_file_function_body_edit_preserves_prefix_symbol() {
    let mut project = Project::new();
    let source = "const alpha = 1;\nfunction foo() {\n  const inner = 1;\n  return inner;\n}\n";
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
}

#[test]
fn test_project_update_file_refreshes_file_locals_for_suffix() {
    let mut project = Project::new();
    let source = "const alpha = 1;\nconst beta = 2;\n";
    project.set_file("a.ts".to_string(), source.to_string());

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "beta");
        TextEdit::new(range, "gamma".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    let file = project.file("a.ts").unwrap();
    let locals = &file.binder().file_locals;
    assert!(locals.has("alpha"), "Expected prefix symbol to remain");
    assert!(locals.has("gamma"), "Expected updated suffix symbol");
    assert!(!locals.has("beta"), "Expected removed suffix symbol");
}

#[test]
fn test_project_update_file_removes_suffix_symbol_mappings() {
    let mut project = Project::new();
    let source = "const alpha = 1;\nconst beta = 2;\n";
    project.set_file("a.ts".to_string(), source.to_string());

    let (beta_decl_idx, beta_name_idx) = {
        let file = project.file("a.ts").unwrap();
        let arena = file.arena();
        let root = file.root();
        let source_node = arena.get(root).unwrap();
        let source_file = arena.get_source_file(source_node).unwrap();
        let stmt_idx = source_file.statements.nodes[1];
        let stmt_node = arena.get(stmt_idx).unwrap();
        let var_stmt = arena.get_variable(stmt_node).unwrap();
        let decl_list_idx = var_stmt.declarations.nodes[0];
        let decl_list_node = arena.get(decl_list_idx).unwrap();
        let decl_list = arena.get_variable(decl_list_node).unwrap();
        let decl_idx = decl_list.declarations.nodes[0];
        let decl_node = arena.get(decl_idx).unwrap();
        let decl = arena.get_variable_declaration(decl_node).unwrap();
        (decl_idx, decl.name)
    };

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "const beta = 2;\n");
        TextEdit::new(range, "".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    let file = project.file("a.ts").unwrap();
    assert_eq!(file.source_text(), "const alpha = 1;\n");
    let binder = file.binder();
    assert!(binder.file_locals.has("alpha"));
    assert!(!binder.file_locals.has("beta"));
    assert!(binder.get_node_symbol(beta_decl_idx).is_none());
    assert!(binder.get_node_symbol(beta_name_idx).is_none());
}

#[test]
fn test_project_update_file_removes_suffix_flow_mappings() {
    let mut project = Project::new();
    let source = "const alpha = 1;\nbeta;\n";
    project.set_file("a.ts".to_string(), source.to_string());

    let beta_ident_idx = {
        let file = project.file("a.ts").unwrap();
        let arena = file.arena();
        let root = file.root();
        let source_node = arena.get(root).unwrap();
        let source_file = arena.get_source_file(source_node).unwrap();
        let stmt_idx = source_file.statements.nodes[1];
        let stmt_node = arena.get(stmt_idx).unwrap();
        let expr_stmt = arena.get_expression_statement(stmt_node).unwrap();
        expr_stmt.expression
    };

    {
        let file = project.file("a.ts").unwrap();
        assert!(file.binder().get_node_flow(beta_ident_idx).is_some());
    }

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "beta;\n");
        TextEdit::new(range, "".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    let file = project.file("a.ts").unwrap();
    assert!(file.binder().get_node_flow(beta_ident_idx).is_none());
}

#[test]
fn test_project_update_file_inserts_suffix_statement() {
    let mut project = Project::new();
    let source = "const alpha = 1;\n";
    project.set_file("a.ts".to_string(), source.to_string());

    let edit = {
        let file = project.file("a.ts").unwrap();
        let source = file.source_text();
        let end = source.len() as u32;
        let pos = file.line_map().offset_to_position(end, source);
        let range = Range::new(pos, pos);
        TextEdit::new(range, "const beta = 2;\n".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    let file = project.file("a.ts").unwrap();
    assert_eq!(file.source_text(), "const alpha = 1;\nconst beta = 2;\n");
    let locals = &file.binder().file_locals;
    assert!(locals.has("alpha"));
    assert!(locals.has("beta"));
}

#[test]
fn test_project_update_file_preserves_prefix_symbol_across_edits() {
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

    let edit_one = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "beta");
        TextEdit::new(range, "gamma".to_string())
    };
    project
        .update_file("a.ts", &[edit_one])
        .expect("Expected update to succeed");

    let edit_two = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "gamma");
        TextEdit::new(range, "delta".to_string())
    };
    project
        .update_file("a.ts", &[edit_two])
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
            .expect("Expected symbol for alpha after updates")
    };

    assert_eq!(alpha_symbol_before, alpha_symbol_after);
}

#[test]
fn test_project_update_file_multiple_edits_preserve_prefix_symbol() {
    let mut project = Project::new();
    let source = "const alpha = 1;\nconst beta = 2;\nconst gamma = 3;\n";
    project.set_file("a.ts".to_string(), source.to_string());

    let alpha_symbol_before = {
        let file = project.file("a.ts").unwrap();
        file.binder()
            .file_locals
            .get("alpha")
            .expect("Expected symbol for alpha")
    };

    let edits = {
        let file = project.file("a.ts").unwrap();
        let rename_beta = range_for_substring(file.source_text(), file.line_map(), "beta");
        let update_gamma = range_for_substring(file.source_text(), file.line_map(), "3");
        vec![
            TextEdit::new(rename_beta, "beta2".to_string()),
            TextEdit::new(update_gamma, "4".to_string()),
        ]
    };
    project
        .update_file("a.ts", &edits)
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
            .expect("Expected symbol for alpha after updates")
    };

    let file = project.file("a.ts").unwrap();
    assert_eq!(
        file.source_text(),
        "const alpha = 1;\nconst beta2 = 2;\nconst gamma = 4;\n"
    );
    assert_eq!(alpha_symbol_before, alpha_symbol_after);
    let locals = &file.binder().file_locals;
    assert!(locals.has("alpha"));
    assert!(locals.has("beta2"));
    assert!(locals.has("gamma"));
}

#[test]
fn test_project_update_file_append_preserves_prefix_symbol() {
    let mut project = Project::new();
    let source = "const alpha = 1;\n";
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
        let source = file.source_text();
        let end = source.len() as u32;
        let pos = file.line_map().offset_to_position(end, source);
        let range = Range::new(pos, pos);
        TextEdit::new(range, "const beta = 2;\n".to_string())
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
            .expect("Expected symbol for alpha after append")
    };

    let file = project.file("a.ts").unwrap();
    assert_eq!(file.source_text(), "const alpha = 1;\nconst beta = 2;\n");
    assert_eq!(alpha_symbol_before, alpha_symbol_after);
    let locals = &file.binder().file_locals;
    assert!(locals.has("alpha"));
    assert!(locals.has("beta"));
}

#[test]
fn test_project_update_file_append_multiple_statements_preserves_prefix_symbol() {
    let mut project = Project::new();
    let source = "const alpha = 1;\n";
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
        let source = file.source_text();
        let end = source.len() as u32;
        let pos = file.line_map().offset_to_position(end, source);
        let range = Range::new(pos, pos);
        TextEdit::new(range, "const beta = 2;\nconst gamma = 3;\n".to_string())
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
            .expect("Expected symbol for alpha after append")
    };

    let file = project.file("a.ts").unwrap();
    assert_eq!(
        file.source_text(),
        "const alpha = 1;\nconst beta = 2;\nconst gamma = 3;\n"
    );
    assert_eq!(alpha_symbol_before, alpha_symbol_after);
    let locals = &file.binder().file_locals;
    assert!(locals.has("alpha"));
    assert!(locals.has("beta"));
    assert!(locals.has("gamma"));
}

#[test]
fn test_project_update_file_append_preserves_multiple_prefix_symbols() {
    let mut project = Project::new();
    let source = "const alpha = 1;\nconst beta = 2;\n";
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
        let source = file.source_text();
        let end = source.len() as u32;
        let pos = file.line_map().offset_to_position(end, source);
        let range = Range::new(pos, pos);
        TextEdit::new(range, "const gamma = 3;\n".to_string())
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
            .expect("Expected symbol for alpha after append");
        let beta_sym = file
            .binder()
            .get_node_symbol(beta_name_idx)
            .expect("Expected symbol for beta after append");
        (alpha_sym, beta_sym)
    };

    let file = project.file("a.ts").unwrap();
    assert_eq!(
        file.source_text(),
        "const alpha = 1;\nconst beta = 2;\nconst gamma = 3;\n"
    );
    assert_eq!(alpha_symbol_before, alpha_symbol_after);
    assert_eq!(beta_symbol_before, beta_symbol_after);
    let locals = &file.binder().file_locals;
    assert!(locals.has("alpha"));
    assert!(locals.has("beta"));
    assert!(locals.has("gamma"));
}

