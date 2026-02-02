//! Project-level LSP tests.

use super::*;
use crate::lsp::position::LineMap;

fn apply_text_edits(source: &str, line_map: &LineMap, edits: &[TextEdit]) -> String {
    let mut result = source.to_string();
    let mut edits_with_offsets: Vec<(usize, usize, &TextEdit)> = edits
        .iter()
        .map(|edit| {
            let start = line_map
                .position_to_offset(edit.range.start, source)
                .unwrap_or(0) as usize;
            let end = line_map
                .position_to_offset(edit.range.end, source)
                .unwrap_or(0) as usize;
            (start, end, edit)
        })
        .collect();

    edits_with_offsets.sort_by(|a, b| b.0.cmp(&a.0));
    for (start, end, edit) in edits_with_offsets {
        result.replace_range(start..end, &edit.new_text);
    }
    result
}

fn range_for_substring(source: &str, line_map: &LineMap, needle: &str) -> Range {
    let start = source.find(needle).expect("substring not found") as u32;
    let end = start + needle.len() as u32;
    let start_pos = line_map.offset_to_position(start, source);
    let end_pos = line_map.offset_to_position(end, source);
    Range::new(start_pos, end_pos)
}

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

    let a_file = project.file("a.ts").unwrap();
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
    });

    assert!(
        has_auto_import,
        "Should include auto-import completion for foo"
    );
}

#[test]
fn test_project_diagnostics_cached() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value: string = 1;\n".to_string());

    let diagnostics = project
        .get_diagnostics("a.ts")
        .expect("Expected diagnostics");
    assert!(!diagnostics.is_empty(), "Should report diagnostics");
    assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::Error));

    let diagnostics_again = project
        .get_diagnostics("a.ts")
        .expect("Expected diagnostics on cached run");
    assert_eq!(diagnostics_again.len(), diagnostics.len());
}

#[test]
fn test_project_performance_scope_cache_hits_definition() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value = 1;\nvalue;\n".to_string());
    let position = Position::new(1, 0);

    let _ = project.get_definition("a.ts", position);
    let first = project
        .performance()
        .timing(ProjectRequestKind::Definition)
        .expect("Expected timing data for definition");

    let _ = project.get_definition("a.ts", position);
    let second = project
        .performance()
        .timing(ProjectRequestKind::Definition)
        .expect("Expected timing data for definition");

    assert!(
        first.scope_misses > 0,
        "Expected scope cache misses on first request"
    );
    assert!(
        second.scope_hits > 0,
        "Expected scope cache hits on second request"
    );
}

#[test]
fn test_project_performance_scope_cache_hits_hover() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value = 1;\nvalue;\n".to_string());
    let position = Position::new(1, 0);

    assert!(project.get_hover("a.ts", position).is_some());
    let first = project
        .performance()
        .timing(ProjectRequestKind::Hover)
        .expect("Expected timing data for hover");

    assert!(project.get_hover("a.ts", position).is_some());
    let second = project
        .performance()
        .timing(ProjectRequestKind::Hover)
        .expect("Expected timing data for hover");

    assert!(
        first.scope_misses > 0,
        "Expected scope cache misses on first request"
    );
    assert!(
        second.scope_hits > 0,
        "Expected scope cache hits on second request"
    );
}

#[test]
fn test_project_performance_scope_cache_hits_completions() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value = 1;\n".to_string());
    let position = Position::new(1, 0);

    let first_items = project
        .get_completions("a.ts", position)
        .expect("Expected completions on first request");
    assert!(first_items.iter().any(|item| item.label == "value"));

    let first = project
        .performance()
        .timing(ProjectRequestKind::Completions)
        .expect("Expected timing data for completions");

    let second_items = project
        .get_completions("a.ts", position)
        .expect("Expected completions on second request");
    assert!(second_items.iter().any(|item| item.label == "value"));

    let second = project
        .performance()
        .timing(ProjectRequestKind::Completions)
        .expect("Expected timing data for completions");

    assert!(
        first.scope_misses > 0,
        "Expected scope cache misses on first request"
    );
    assert!(
        second.scope_hits > 0,
        "Expected scope cache hits on second request"
    );
}

#[test]
fn test_project_performance_scope_cache_hits_signature_help() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "function foo(a: number, b: string) {}\nfoo(1, \"x\");\n".to_string(),
    );
    let position = Position::new(1, 4);

    assert!(project.get_signature_help("a.ts", position).is_some());
    let first = project
        .performance()
        .timing(ProjectRequestKind::SignatureHelp)
        .expect("Expected timing data for signature help");

    assert!(project.get_signature_help("a.ts", position).is_some());
    let second = project
        .performance()
        .timing(ProjectRequestKind::SignatureHelp)
        .expect("Expected timing data for signature help");

    assert!(
        first.scope_misses > 0,
        "Expected scope cache misses on first request"
    );
    assert!(
        second.scope_hits > 0,
        "Expected scope cache hits on second request"
    );
}

#[test]
fn test_project_performance_scope_cache_hits_references() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value = 1;\nvalue;\n".to_string());
    let position = Position::new(1, 0);

    assert!(project.find_references("a.ts", position).is_some());
    let first = project
        .performance()
        .timing(ProjectRequestKind::References)
        .expect("Expected timing data for references");

    assert!(project.find_references("a.ts", position).is_some());
    let second = project
        .performance()
        .timing(ProjectRequestKind::References)
        .expect("Expected timing data for references");

    assert!(
        first.scope_misses > 0,
        "Expected scope cache misses on first request"
    );
    assert!(
        second.scope_hits > 0,
        "Expected scope cache hits on second request"
    );
}

#[test]
fn test_project_performance_scope_cache_hits_rename() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value = 1;\nvalue;\n".to_string());
    let position = Position::new(1, 0);

    let _ = project
        .get_rename_edits("a.ts", position, "next".to_string())
        .expect("Expected rename edits");
    let first = project
        .performance()
        .timing(ProjectRequestKind::Rename)
        .expect("Expected timing data for rename");

    let _ = project
        .get_rename_edits("a.ts", position, "next2".to_string())
        .expect("Expected rename edits");
    let second = project
        .performance()
        .timing(ProjectRequestKind::Rename)
        .expect("Expected timing data for rename");

    assert!(
        first.scope_misses > 0,
        "Expected scope cache misses on first request"
    );
    assert!(
        second.scope_hits > 0,
        "Expected scope cache hits on second request"
    );
}

#[test]
fn test_project_scope_cache_cleared_after_update() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "const value = 1;\nvalue;\nconst later = 2;\n".to_string(),
    );
    let position = Position::new(1, 0);

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

    assert!(
        first.scope_misses > 0,
        "Expected scope cache misses on first request"
    );
    assert!(
        second.scope_hits > 0,
        "Expected scope cache hits on second request"
    );

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "2");
        TextEdit::new(range, "3".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    assert!(project.get_definition("a.ts", position).is_some());
    let third = project
        .performance()
        .timing(ProjectRequestKind::Definition)
        .expect("Expected timing data for definition");

    assert!(third.scope_misses > 0, "Expected cache misses after edit");
    assert_eq!(
        third.scope_hits, 0,
        "Expected cache hits cleared after edit"
    );
}

#[test]
fn test_project_scope_cache_reuse_hover_to_definition_after_edit() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value = 1;\nvalue;\n".to_string());
    let position = Position::new(1, 0);

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "1");
        TextEdit::new(range, "2".to_string())
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
        "Expected scope cache hit from prior hover after edit"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected definition to reuse cached scope after edit"
    );
}

#[test]
fn test_project_scope_cache_reuse_hover_to_definition_after_edit_across_files() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "export const value = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { value } from \"./a\";\nvalue;\n".to_string(),
    );
    let position = Position::new(1, 0);

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "1");
        TextEdit::new(range, "2".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    assert!(project.get_hover("b.ts", position).is_some());
    assert!(project.get_definition("b.ts", position).is_some());

    let timing = project
        .performance()
        .timing(ProjectRequestKind::Definition)
        .expect("Expected timing data for definition");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover after edit"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected definition to reuse cached scope after edit across files"
    );
}

#[test]
fn test_project_scope_cache_reuse_hover_to_references_after_edit_across_files() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "export const value = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { value } from \"./a\";\nvalue;\n".to_string(),
    );
    let position = Position::new(1, 0);

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "1");
        TextEdit::new(range, "2".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    assert!(project.get_hover("b.ts", position).is_some());
    assert!(project.find_references("b.ts", position).is_some());

    let timing = project
        .performance()
        .timing(ProjectRequestKind::References)
        .expect("Expected timing data for references");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover after edit"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected references to reuse cached scope after edit across files"
    );
}

#[test]
fn test_project_scope_cache_reuse_hover_to_rename_after_edit_across_files() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "export const value = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { value } from \"./a\";\nvalue;\n".to_string(),
    );
    let position = Position::new(1, 0);

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "1");
        TextEdit::new(range, "2".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    assert!(project.get_hover("b.ts", position).is_some());
    let _ = project
        .get_rename_edits("b.ts", position, "next".to_string())
        .expect("Expected rename edits");

    let timing = project
        .performance()
        .timing(ProjectRequestKind::Rename)
        .expect("Expected timing data for rename");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover after edit"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected rename to reuse cached scope after edit across files"
    );
}

#[test]
fn test_project_scope_cache_reuse_hover_to_signature_help_after_edit_across_files() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const other = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "function foo(a: number, b: string) {}\nfoo(1, \"x\");\n".to_string(),
    );
    let hover_position = {
        let file = project.file("b.ts").unwrap();
        range_for_substring(file.source_text(), file.line_map(), "foo(1").start
    };
    let signature_position = {
        let file = project.file("b.ts").unwrap();
        range_for_substring(file.source_text(), file.line_map(), "1").start
    };

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "1");
        TextEdit::new(range, "2".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    assert!(project.get_hover("b.ts", hover_position).is_some());
    assert!(
        project
            .get_signature_help("b.ts", signature_position)
            .is_some()
    );

    let timing = project
        .performance()
        .timing(ProjectRequestKind::SignatureHelp)
        .expect("Expected timing data for signature help");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover after edit"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected signature help to reuse cached scope after edit across files"
    );
}

#[test]
fn test_project_scope_cache_reuse_hover_to_completions_after_edit_across_files() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const other = 1;\n".to_string());
    project.set_file("b.ts".to_string(), "const value = 1;\nvalue;\n".to_string());
    let position = Position::new(1, 0);

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "1");
        TextEdit::new(range, "2".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    assert!(project.get_hover("b.ts", position).is_some());
    let items = project
        .get_completions("b.ts", position)
        .expect("Expected completions");
    assert!(items.iter().any(|item| item.label == "value"));

    let timing = project
        .performance()
        .timing(ProjectRequestKind::Completions)
        .expect("Expected timing data for completions");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover after edit"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected completions to reuse cached scope after edit across files"
    );
}

#[test]
fn test_project_scope_cache_reuse_hover_to_completions_after_edit() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value = 1;\nvalue;\n".to_string());
    let position = Position::new(1, 0);

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "1");
        TextEdit::new(range, "2".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    assert!(project.get_hover("a.ts", position).is_some());
    let items = project
        .get_completions("a.ts", position)
        .expect("Expected completions");
    assert!(items.iter().any(|item| item.label == "value"));

    let timing = project
        .performance()
        .timing(ProjectRequestKind::Completions)
        .expect("Expected timing data for completions");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover after edit"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected completions to reuse cached scope after edit"
    );
}

#[test]
fn test_project_scope_cache_reuse_hover_to_signature_help_after_edit() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "function foo(a: number, b: string) {}\nfoo(1, \"x\");\n".to_string(),
    );
    let hover_position = Position::new(1, 0);
    // Position must be inside the call args (after the opening paren)
    let signature_position = Position::new(1, 4);

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "\"x\"");
        TextEdit::new(range, "\"y\"".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

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
        "Expected scope cache hit from prior hover after edit"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected signature help to reuse cached scope after edit"
    );
}

#[test]
fn test_project_scope_cache_reuse_hover_to_references_after_edit() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value = 1;\nvalue;\n".to_string());
    let position = Position::new(1, 0);

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "1");
        TextEdit::new(range, "2".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

    assert!(project.get_hover("a.ts", position).is_some());
    assert!(project.find_references("a.ts", position).is_some());

    let timing = project
        .performance()
        .timing(ProjectRequestKind::References)
        .expect("Expected timing data for references");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover after edit"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected references to reuse cached scope after edit"
    );
}

#[test]
fn test_project_scope_cache_reuse_hover_to_rename_after_edit() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value = 1;\nvalue;\n".to_string());
    let position = Position::new(1, 0);

    let edit = {
        let file = project.file("a.ts").unwrap();
        let range = range_for_substring(file.source_text(), file.line_map(), "1");
        TextEdit::new(range, "2".to_string())
    };
    project
        .update_file("a.ts", &[edit])
        .expect("Expected update to succeed");

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
        "Expected scope cache hit from prior hover after edit"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected rename to reuse cached scope after edit"
    );
}

#[test]
fn test_project_scope_cache_reuse_across_requests() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value = 1;\nvalue;\n".to_string());
    let position = Position::new(1, 0);

    assert!(project.get_hover("a.ts", position).is_some());
    assert!(project.get_definition("a.ts", position).is_some());

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
fn test_project_scope_cache_reuse_hover_to_completions() {
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "const value = 1;\nvalue;\n".to_string());
    let position = Position::new(1, 0);

    assert!(project.get_hover("a.ts", position).is_some());
    let items = project
        .get_completions("a.ts", position)
        .expect("Expected completions");
    assert!(items.iter().any(|item| item.label == "value"));

    let timing = project
        .performance()
        .timing(ProjectRequestKind::Completions)
        .expect("Expected timing data for completions");

    assert!(
        timing.scope_hits > 0,
        "Expected scope cache hit from prior hover"
    );
    assert_eq!(
        timing.scope_misses, 0,
        "Expected completions to reuse cached scope"
    );
}

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
        code: Some(crate::checker::types::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
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
    assert_eq!(updated, "import { foo } from \"./a\";\nfoo();\n");
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
        code: Some(crate::checker::types::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
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
    assert_eq!(updated, "import foo from \"./a\";\nfoo();\n");
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
        code: Some(crate::checker::types::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
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
    assert_eq!(updated, "import { foo } from \"./a\";\nfoo();\n");
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
        code: Some(crate::checker::types::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
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
    assert_eq!(updated, "import foo from \"./index\";\nfoo();\n");
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
        code: Some(crate::checker::types::diagnostics::diagnostic_codes::CANNOT_FIND_NAME),
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
    assert_eq!(updated, "import { bar } from \"./index\";\nbar();\n");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn test_project_load_tsconfig_strict_true() {
    use std::env;
    use std::fs::{self, File};
    use std::io::Write;

    // Create a temporary directory for the test
    let temp_dir = env::temp_dir().join("typescript_test_strict_true");
    fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    // Create a tsconfig.json with strict: true
    let tsconfig_path = temp_dir.join("tsconfig.json");
    let mut file = File::create(&tsconfig_path).expect("Failed to create tsconfig.json");
    file.write_all(br#"{"compilerOptions": {"strict": true}}"#)
        .expect("Failed to write tsconfig.json");

    // Create a project and load the tsconfig
    let mut project = Project::new();
    project.set_file("test.ts".to_string(), "const x: number = 1;\n".to_string());

    // Verify default is false
    assert!(!project.strict(), "Default strict should be false");
    assert!(!project.file("test.ts").unwrap().strict());

    // Load tsconfig
    let result = project.load_tsconfig(&temp_dir);
    assert!(result.is_ok(), "load_tsconfig should succeed");

    // Verify strict mode is now true
    assert!(
        project.strict(),
        "Strict should be true after loading tsconfig"
    );
    assert!(
        project.file("test.ts").unwrap().strict(),
        "Existing files should be updated to strict mode"
    );

    // Cleanup
    fs::remove_file(&tsconfig_path).ok();
    fs::remove_dir(&temp_dir).ok();
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn test_project_load_tsconfig_strict_false() {
    use std::env;
    use std::fs::{self, File};
    use std::io::Write;

    // Create a temporary directory for the test
    let temp_dir = env::temp_dir().join("typescript_test_strict_false");
    fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    // Create a tsconfig.json with strict: false
    let tsconfig_path = temp_dir.join("tsconfig.json");
    let mut file = File::create(&tsconfig_path).expect("Failed to create tsconfig.json");
    file.write_all(br#"{"compilerOptions": {"strict": false}}"#)
        .expect("Failed to write tsconfig.json");

    // Create a project with strict mode initially true
    let mut project = Project::new();
    project.set_strict(true);
    project.set_file("test.ts".to_string(), "const x: number = 1;\n".to_string());

    assert!(project.strict(), "Should start with strict=true");

    // Load tsconfig with strict: false
    let result = project.load_tsconfig(&temp_dir);
    assert!(result.is_ok(), "load_tsconfig should succeed");

    // Verify strict mode is now false
    assert!(
        !project.strict(),
        "Strict should be false after loading tsconfig"
    );
    assert!(
        !project.file("test.ts").unwrap().strict(),
        "Existing files should be updated to non-strict mode"
    );

    // Cleanup
    fs::remove_file(&tsconfig_path).ok();
    fs::remove_dir(&temp_dir).ok();
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn test_project_load_tsconfig_missing_file() {
    use std::env;

    // Use a non-existent directory
    let temp_dir = env::temp_dir().join("typescript_test_nonexistent");

    // Create a project
    let mut project = Project::new();
    project.set_strict(true);
    project.set_file("test.ts".to_string(), "const x: number = 1;\n".to_string());

    assert!(project.strict(), "Should start with strict=true");

    // Try to load tsconfig from non-existent directory
    let result = project.load_tsconfig(&temp_dir);
    assert!(
        result.is_ok(),
        "Missing tsconfig should not error, just keep default"
    );

    // Verify strict mode is unchanged (true, as we set it)
    assert!(
        project.strict(),
        "Strict mode should remain unchanged when tsconfig is missing"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn test_project_load_tsconfig_updates_all_files() {
    use std::env;
    use std::fs::{self, File};
    use std::io::Write;

    // Create a temporary directory for the test
    let temp_dir = env::temp_dir().join("typescript_test_multi_file");
    fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    // Create a tsconfig.json with strict: true
    let tsconfig_path = temp_dir.join("tsconfig.json");
    let mut file = File::create(&tsconfig_path).expect("Failed to create tsconfig.json");
    file.write_all(br#"{"compilerOptions": {"strict": true}}"#)
        .expect("Failed to write tsconfig.json");

    // Create a project with multiple files
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "const x = 1;\n".to_string());
    project.set_file("b.ts".to_string(), "const y = 2;\n".to_string());
    project.set_file("c.ts".to_string(), "const z = 3;\n".to_string());

    // Verify all files start with strict=false
    assert!(!project.file("a.ts").unwrap().strict());
    assert!(!project.file("b.ts").unwrap().strict());
    assert!(!project.file("c.ts").unwrap().strict());

    // Load tsconfig
    let result = project.load_tsconfig(&temp_dir);
    assert!(result.is_ok(), "load_tsconfig should succeed");

    // Verify ALL files have been updated to strict=true
    assert!(
        project.file("a.ts").unwrap().strict(),
        "File a.ts should be updated to strict mode"
    );
    assert!(
        project.file("b.ts").unwrap().strict(),
        "File b.ts should be updated to strict mode"
    );
    assert!(
        project.file("c.ts").unwrap().strict(),
        "File c.ts should be updated to strict mode"
    );

    // Cleanup
    fs::remove_file(&tsconfig_path).ok();
    fs::remove_dir(&temp_dir).ok();
}
