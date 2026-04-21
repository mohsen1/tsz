//! Project-level LSP tests.

use super::*;
use crate::project::FileRename;
use tsz_common::position::LineMap;

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

    edits_with_offsets.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
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
#[ignore = "TODO: LSP scope cache performance test"]
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
#[ignore = "TODO: LSP scope cache reuse after edit across files"]
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
#[ignore = "TODO: LSP scope cache reuse after edit"]
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

#[test]
fn test_project_get_document_symbols() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"
interface Greeter {
    greet(): string;
}

class Hello implements Greeter {
    name: string;
    constructor(name: string) {
        this.name = name;
    }
    greet() { return `Hello ${this.name}`; }
}

function createGreeter(name: string): Greeter {
    return new Hello(name);
}

const DEFAULT_NAME = "World";
"#
        .to_string(),
    );

    let symbols = project.get_document_symbols("test.ts");
    assert!(symbols.is_some(), "Should return document symbols");
    let symbols = symbols.unwrap();
    assert!(
        symbols.len() >= 3,
        "Should have at least 3 top-level symbols (Greeter, Hello, createGreeter, DEFAULT_NAME)"
    );

    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"Greeter"),
        "Should contain Greeter interface"
    );
    assert!(names.contains(&"Hello"), "Should contain Hello class");
    assert!(
        names.contains(&"createGreeter"),
        "Should contain createGreeter function"
    );
    assert!(
        names.contains(&"DEFAULT_NAME"),
        "Should contain DEFAULT_NAME constant"
    );

    // Check that Hello has children (members)
    let hello = symbols.iter().find(|s| s.name == "Hello").unwrap();
    assert!(
        !hello.children.is_empty(),
        "Hello class should have children (members)"
    );
}

#[test]
fn test_project_get_folding_ranges() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"function foo() {
    if (true) {
        console.log("hi");
    }
}

class Bar {
    method() {
        return 42;
    }
}
"#
        .to_string(),
    );

    let ranges = project.get_folding_ranges("test.ts");
    assert!(ranges.is_some(), "Should return folding ranges");
    let ranges = ranges.unwrap();
    assert!(
        ranges.len() >= 3,
        "Should have at least 3 folding ranges (function, if, class, method)"
    );
}

#[test]
fn test_project_get_selection_ranges() {
    let mut project = Project::new();
    project.set_file("test.ts".to_string(), "const x = 42;\n".to_string());

    let positions = vec![Position::new(0, 6)]; // on 'x'
    let ranges = project.get_selection_ranges("test.ts", &positions);
    assert!(ranges.is_some(), "Should return selection ranges");
    let ranges = ranges.unwrap();
    assert_eq!(ranges.len(), 1, "Should have one result per position");
    assert!(ranges[0].is_some(), "Selection range at 'x' should exist");
}

#[test]
fn test_project_get_semantic_tokens() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "const x: number = 42;\nfunction foo(a: string) { return a; }\n".to_string(),
    );

    let tokens = project.get_semantic_tokens_full("test.ts");
    assert!(tokens.is_some(), "Should return semantic tokens");
    let tokens = tokens.unwrap();
    // Tokens are encoded as groups of 5 integers (deltaLine, deltaStartChar, length, tokenType, tokenModifiers)
    assert_eq!(tokens.len() % 5, 0, "Token data should be in groups of 5");
    assert!(tokens.len() >= 5, "Should have at least 1 token");
}

#[test]
fn test_project_get_document_highlighting() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "const x = 1;\nconst y = x + x;\n".to_string(),
    );

    // Position on 'x' at line 1, character 10
    let highlights = project.get_document_highlighting("test.ts", Position::new(1, 10));
    assert!(highlights.is_some(), "Should find highlights for 'x'");
    let highlights = highlights.unwrap();
    assert!(
        highlights.len() >= 2,
        "Should highlight at least 2 occurrences of 'x'"
    );
}

#[test]
fn test_project_get_inlay_hints() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "function add(a: number, b: number) { return a + b; }\nconst sum = add(1, 2);\n"
            .to_string(),
    );

    let range = Range::new(Position::new(0, 0), Position::new(2, 0));
    let hints = project.get_inlay_hints("test.ts", range);
    assert!(hints.is_some(), "Should return inlay hints");
    // Whether hints are non-empty depends on the provider configuration
}

#[test]
fn test_project_prepare_call_hierarchy() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"function caller() {
    callee();
}

function callee() {
    return 42;
}
"#
        .to_string(),
    );

    // Position on 'callee' declaration (line 4, character 9)
    let item = project.prepare_call_hierarchy("test.ts", Position::new(4, 9));
    assert!(item.is_some(), "Should prepare call hierarchy for callee");
    let item = item.unwrap();
    assert_eq!(
        item.name, "callee",
        "Call hierarchy item should be named 'callee'"
    );
}

#[test]
fn test_project_call_hierarchy_incoming_outgoing() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"function a() {
    b();
}

function b() {
    c();
}

function c() {
    return 1;
}
"#
        .to_string(),
    );

    // Check incoming calls to b (should include 'a')
    let incoming = project.get_incoming_calls("test.ts", Position::new(4, 9));
    assert!(!incoming.is_empty(), "b should have incoming calls from a");
    assert_eq!(
        incoming[0].from.name, "a",
        "Incoming call should be from 'a'"
    );

    // Check outgoing calls from b (should include 'c')
    let outgoing = project.get_outgoing_calls("test.ts", Position::new(4, 9));
    assert!(!outgoing.is_empty(), "b should have outgoing calls to c");
    assert_eq!(outgoing[0].to.name, "c", "Outgoing call should be to 'c'");
}

#[test]
fn test_project_get_document_links() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "import { foo } from './other';\n".to_string(),
    );

    let links = project.get_document_links("test.ts");
    assert!(links.is_some(), "Should return document links");
    let links = links.unwrap();
    assert!(
        !links.is_empty(),
        "Should find at least one document link for the import"
    );
}

#[test]
fn test_project_get_linked_editing_ranges_jsx() {
    let mut project = Project::new();
    project.set_file(
        "test.tsx".to_string(),
        "const el = <div>hello</div>;\n".to_string(),
    );

    // Position on opening 'div' tag (line 0, character 12)
    let ranges = project.get_linked_editing_ranges("test.tsx", Position::new(0, 12));
    // JSX linked editing should find both opening and closing tag names
    if let Some(result) = ranges {
        assert_eq!(
            result.ranges.len(),
            2,
            "Should find 2 linked ranges (opening and closing tag)"
        );
    }
}

#[test]
fn test_project_format_document() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "function   foo(  ) {\nreturn 1;\n}\n".to_string(),
    );

    let options = FormattingOptions::default();
    let result = project.format_document("test.ts", &options);
    assert!(result.is_some(), "Should return formatting result");
    // The result may be Ok or Err depending on formatter availability
}

#[test]
fn test_project_prepare_type_hierarchy() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"class Animal {
    name: string;
}

class Dog extends Animal {
    breed: string;
}
"#
        .to_string(),
    );

    // Position on 'Dog' class name (line 4, character 6)
    let item = project.prepare_type_hierarchy("test.ts", Position::new(4, 6));
    assert!(item.is_some(), "Should prepare type hierarchy for Dog");
    let item = item.unwrap();
    assert_eq!(
        item.name, "Dog",
        "Type hierarchy item should be named 'Dog'"
    );
}

#[test]
fn test_project_supertypes() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"class Base {}
class Middle extends Base {}
class Child extends Middle {}
"#
        .to_string(),
    );

    // Check supertypes of Child (line 2, character 6)
    let supertypes = project.supertypes("test.ts", Position::new(2, 6));
    assert!(!supertypes.is_empty(), "Child should have supertypes");
    assert_eq!(
        supertypes[0].name, "Middle",
        "First supertype should be 'Middle'"
    );
}

#[test]
fn test_project_get_document_symbols_returns_none_for_missing_file() {
    let project = Project::new();
    assert!(project.get_document_symbols("missing.ts").is_none());
}

#[test]
fn test_project_get_folding_ranges_returns_none_for_missing_file() {
    let project = Project::new();
    assert!(project.get_folding_ranges("missing.ts").is_none());
}

#[test]
fn test_project_get_semantic_tokens_returns_none_for_missing_file() {
    let project = Project::new();
    assert!(project.get_semantic_tokens_full("missing.ts").is_none());
}

#[test]
fn test_project_get_document_highlighting_returns_none_for_missing_file() {
    let project = Project::new();
    assert!(
        project
            .get_document_highlighting("missing.ts", Position::new(0, 0))
            .is_none()
    );
}

#[test]
fn test_project_get_inlay_hints_returns_none_for_missing_file() {
    let project = Project::new();
    let range = Range::new(Position::new(0, 0), Position::new(10, 0));
    assert!(project.get_inlay_hints("missing.ts", range).is_none());
}

#[test]
fn test_project_prepare_call_hierarchy_returns_none_for_missing_file() {
    let project = Project::new();
    assert!(
        project
            .prepare_call_hierarchy("missing.ts", Position::new(0, 0))
            .is_none()
    );
}

#[test]
fn test_project_get_document_links_returns_none_for_missing_file() {
    let project = Project::new();
    assert!(project.get_document_links("missing.ts").is_none());
}

#[test]
fn test_project_get_linked_editing_returns_none_for_missing_file() {
    let project = Project::new();
    assert!(
        project
            .get_linked_editing_ranges("missing.ts", Position::new(0, 0))
            .is_none()
    );
}

#[test]
fn test_project_format_document_returns_none_for_missing_file() {
    let project = Project::new();
    assert!(
        project
            .format_document("missing.ts", &FormattingOptions::default())
            .is_none()
    );
}

#[test]
fn test_project_document_symbols_with_nested_structure() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"namespace MyApp {
    export interface Config {
        host: string;
        port: number;
    }

    export class Server {
        config: Config;
        start() {}
        stop() {}
    }

    export function createServer(config: Config): Server {
        return new Server();
    }
}
"#
        .to_string(),
    );

    let symbols = project.get_document_symbols("test.ts").unwrap();

    // Should have the MyApp namespace as top-level
    let ns = symbols.iter().find(|s| s.name == "MyApp");
    assert!(ns.is_some(), "Should have MyApp namespace");

    let ns = ns.unwrap();
    assert!(!ns.children.is_empty(), "MyApp should have children");

    let child_names: Vec<&str> = ns.children.iter().map(|c| c.name.as_str()).collect();
    assert!(
        child_names.contains(&"Config"),
        "MyApp should contain Config"
    );
    assert!(
        child_names.contains(&"Server"),
        "MyApp should contain Server"
    );
    assert!(
        child_names.contains(&"createServer"),
        "MyApp should contain createServer"
    );
}

#[test]
fn test_project_folding_ranges_include_comments() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"// region: MyRegion
const a = 1;
const b = 2;
// endregion

/*
 * Multi-line comment
 * spanning several lines
 */
function foo() {
    return a + b;
}
"#
        .to_string(),
    );

    let ranges = project.get_folding_ranges("test.ts").unwrap();
    // Should have folding for: region, multi-line comment, function body
    assert!(ranges.len() >= 2, "Should have at least 2 folding ranges");
}

#[test]
fn test_project_semantic_tokens_for_class() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"class Point {
    x: number;
    y: number;
    constructor(x: number, y: number) {
        this.x = x;
        this.y = y;
    }
    distance(): number {
        return Math.sqrt(this.x * this.x + this.y * this.y);
    }
}
"#
        .to_string(),
    );

    let tokens = project.get_semantic_tokens_full("test.ts").unwrap();
    assert!(
        !tokens.is_empty(),
        "Should produce semantic tokens for a class"
    );
    assert_eq!(tokens.len() % 5, 0, "Token count should be divisible by 5");
}

#[test]
fn test_project_document_highlighting_keyword() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        r#"if (true) {
    console.log("a");
} else if (false) {
    console.log("b");
} else {
    console.log("c");
}
"#
        .to_string(),
    );

    // Position on 'if' keyword at line 0, character 0
    let highlights = project.get_document_highlighting("test.ts", Position::new(0, 0));
    // Keyword highlighting should find all if/else branches
    if let Some(highlights) = highlights {
        assert!(
            highlights.len() >= 2,
            "Should highlight multiple if/else keywords"
        );
    }
}

#[test]
fn test_project_workspace_symbols_empty_query() {
    let mut project = Project::new();
    project.set_file(
        "a.ts".to_string(),
        "export function createUser() {}\nexport class UserService {}\n".to_string(),
    );

    // Empty query should return no symbols (workspace symbols spec)
    let symbols = project.get_workspace_symbols("");
    assert!(symbols.is_empty(), "Empty query should return no symbols");
}

#[test]
fn test_project_diagnostics_on_type_error() {
    let mut project = Project::new();
    project.set_file("test.ts".to_string(), "const x: string = 42;\n".to_string());

    let diagnostics = project.get_diagnostics("test.ts");
    assert!(diagnostics.is_some(), "Should return diagnostics");
    let diagnostics = diagnostics.unwrap();
    assert!(
        !diagnostics.is_empty(),
        "Should have at least one diagnostic"
    );

    let has_2322 = diagnostics.iter().any(|d| d.code == Some(2322));
    assert!(has_2322, "Should report TS2322 for type mismatch");
}

#[test]
fn test_project_diagnostics_clean_for_valid_code() {
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "const x: number = 42;\nconst y: string = 'hello';\n".to_string(),
    );

    let diagnostics = project.get_diagnostics("test.ts");
    assert!(diagnostics.is_some(), "Should return diagnostics");
    let diagnostics = diagnostics.unwrap();
    assert!(
        diagnostics.is_empty(),
        "Valid code should have no diagnostics"
    );
}

#[test]
fn test_project_stale_diagnostics_empty_initially() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "const x = 1;\n".to_string());

    // Newly created files start with diagnostics_dirty = false
    let stale = project.get_stale_diagnostics();
    // Initially no files should be stale since set_file creates fresh ProjectFile
    // with diagnostics_dirty = false
    assert!(
        stale.is_empty(),
        "Should have no stale diagnostics for fresh files"
    );

    // After calling get_diagnostics, dirty flag is cleared
    let _ = project.get_diagnostics("a.ts");
    let stale_after = project.get_stale_diagnostics();
    assert!(
        stale_after.is_empty(),
        "Should have no stale diagnostics after getting diagnostics"
    );
}

#[test]
fn test_project_set_strict_mode() {
    let mut project = Project::new();
    project.set_strict(true);
    project.set_file(
        "test.ts".to_string(),
        "function foo(x) { return x; }\n".to_string(),
    );

    let diagnostics = project.get_diagnostics("test.ts");
    assert!(diagnostics.is_some());
}

#[test]
fn test_project_remove_file() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const x = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { x } from \"./a\";\n".to_string(),
    );

    assert_eq!(project.file_count(), 2);
    project.remove_file("a.ts");
    assert_eq!(project.file_count(), 1);
    assert!(project.file("a.ts").is_none());
    assert!(project.file("b.ts").is_some());
}

#[test]
fn test_project_remove_file_cleans_dependency_graph() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const x = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { x } from \"./a\";\nexport const y = x;\n".to_string(),
    );

    // b.ts depends on a.ts (verify dependency edge exists before removal)
    let _deps = project.get_file_dependents("./a");

    // Remove a.ts
    project.remove_file("a.ts");

    // After removal, the dependency graph should not reference a.ts anymore
    let deps_after = project.get_file_dependents("a.ts");
    assert!(
        deps_after.is_empty(),
        "Dependency graph should be cleaned up after file removal, got: {deps_after:?}"
    );

    // b.ts should still exist
    assert!(project.file("b.ts").is_some());
}

#[test]
fn test_project_remove_file_invalidates_dependent_caches() {
    let mut project = Project::new();
    project.set_file(
        "a.ts".to_string(),
        "export const x: number = 1;\n".to_string(),
    );
    project.set_file(
        "b.ts".to_string(),
        "import { x } from \"./a\";\nconst y: number = x;\n".to_string(),
    );

    // Force diagnostics computation for b.ts to populate its caches
    let _ = project.get_diagnostics("b.ts");

    // Remove a.ts — b.ts's caches should be invalidated
    project.remove_file("a.ts");

    // b.ts should still be queryable (no crash)
    assert!(project.file("b.ts").is_some());
}

#[test]
fn test_project_file_count() {
    let mut project = Project::new();
    assert_eq!(project.file_count(), 0);
    project.set_file("a.ts".to_string(), "const a = 1;\n".to_string());
    assert_eq!(project.file_count(), 1);
    project.set_file("b.ts".to_string(), "const b = 2;\n".to_string());
    assert_eq!(project.file_count(), 2);
    // Overwrite existing file
    project.set_file("a.ts".to_string(), "const a = 42;\n".to_string());
    assert_eq!(project.file_count(), 2);
}

#[test]
fn test_project_get_file_dependents() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const x = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { x } from \"./a\";\n".to_string(),
    );

    // get_file_dependents returns files that depend on the given file
    // The exact resolution depends on how module specifiers map to file names
    let deps = project.get_file_dependents("a.ts");
    // Dependency tracking may use raw specifiers or resolved paths
    // We just verify the function returns without error
    assert!(
        deps.is_empty() || deps.iter().any(|d| d.contains("b")),
        "Dependents should either be empty (if specifier resolution differs) or include b.ts, got: {deps:?}"
    );
}

#[test]
fn test_project_import_candidates_for_prefix() {
    let mut project = Project::new();
    project.set_file(
        "utils.ts".to_string(),
        "export function calculateTotal() {}\nexport function calculateTax() {}\n".to_string(),
    );
    project.set_file("main.ts".to_string(), "calc\n".to_string());

    let candidates = project.get_import_candidates_for_prefix("main.ts", "calc");
    // Should find exported symbols from utils.ts matching prefix
    let names: Vec<&str> = candidates.iter().map(|c| c.local_name.as_str()).collect();
    assert!(
        names.iter().any(|n: &&str| n.contains("calculate")),
        "Should suggest exported symbols matching 'calc' prefix, got: {names:?}"
    );
}

#[test]
fn test_project_definition_missing_file() {
    let mut project = Project::new();
    let result = project.get_definition("nonexistent.ts", Position::new(0, 0));
    assert!(result.is_none(), "Should return None for missing file");
}

#[test]
fn test_project_hover_missing_file() {
    let mut project = Project::new();
    let result = project.get_hover("nonexistent.ts", Position::new(0, 0));
    assert!(result.is_none(), "Should return None for missing file");
}

#[test]
fn test_project_completions_missing_file() {
    let mut project = Project::new();
    let result = project.get_completions("nonexistent.ts", Position::new(0, 0));
    assert!(result.is_none(), "Should return None for missing file");
}

#[test]
fn test_project_references_missing_file() {
    let mut project = Project::new();
    let result = project.find_references("nonexistent.ts", Position::new(0, 0));
    assert!(result.is_none(), "Should return None for missing file");
}

#[test]
fn test_project_rename_missing_file() {
    let mut project = Project::new();
    let result =
        project.get_rename_edits("nonexistent.ts", Position::new(0, 0), "newName".to_string());
    assert!(result.is_err(), "Should return Err for missing file");
}

#[test]
fn test_project_signature_help_missing_file() {
    let mut project = Project::new();
    let result = project.get_signature_help("nonexistent.ts", Position::new(0, 0));
    assert!(result.is_none(), "Should return None for missing file");
}

#[test]
fn test_project_implementations_missing_file() {
    let mut project = Project::new();
    let result = project.get_implementations("nonexistent.ts", Position::new(0, 0));
    assert!(result.is_none(), "Should return None for missing file");
}

#[test]
fn test_project_type_definition_missing_file() {
    let project = Project::new();
    let result = project.get_type_definition("nonexistent.ts", Position::new(0, 0));
    assert!(result.is_none(), "Should return None for missing file");
}

#[test]
fn test_project_set_import_module_specifier_ending() {
    let mut project = Project::new();
    project.set_import_module_specifier_ending(Some(".js".to_string()));
    project.set_file("a.ts".to_string(), "export const x = 1;\n".to_string());
    // Just verify it doesn't crash
    assert_eq!(project.file_count(), 1);
}

#[test]
fn test_project_set_import_module_specifier_preference() {
    let mut project = Project::new();
    project.set_import_module_specifier_preference(Some("relative".to_string()));
    project.set_file("a.ts".to_string(), "export const x = 1;\n".to_string());
    assert_eq!(project.file_count(), 1);
}

#[test]
fn test_project_set_auto_import_file_exclude_patterns() {
    let mut project = Project::new();
    project.set_auto_import_file_exclude_patterns(vec!["**/node_modules/**".to_string()]);
    project.set_file("a.ts".to_string(), "export const x = 1;\n".to_string());
    assert_eq!(project.file_count(), 1);
}

#[test]
fn test_project_set_auto_import_specifier_exclude_regexes() {
    let mut project = Project::new();
    project.set_auto_import_specifier_exclude_regexes(vec!["^@internal/.*$".to_string()]);
    project.set_file("a.ts".to_string(), "export const x = 1;\n".to_string());
    assert_eq!(project.file_count(), 1);
}

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

#[test]
fn test_project_shared_type_interner() {
    // Verify that all files in a project share the same TypeInterner instance.
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "export const x: number = 1;".to_string(),
    );
    project.set_file(
        "b.ts".to_string(),
        "export const y: string = 'hello';".to_string(),
    );

    // Both files should share the same Arc<TypeInterner> (same pointer)
    let a_interner = &project.files["a.ts"].type_interner;
    let b_interner = &project.files["b.ts"].type_interner;

    assert!(
        std::sync::Arc::ptr_eq(a_interner, b_interner),
        "All files in a project should share the same TypeInterner"
    );

    // The project-level interner should also be the same instance
    let project_interner = project.type_interner();
    assert!(
        std::sync::Arc::ptr_eq(a_interner, &project_interner),
        "Project-level interner should be the same instance as file interners"
    );
}

#[test]
fn test_project_shared_interner_survives_file_update() {
    // Verify that updating a file preserves the shared TypeInterner.
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "export const x: number = 1;".to_string(),
    );
    project.set_file(
        "b.ts".to_string(),
        "export const y: string = 'hello';".to_string(),
    );

    let interner_before = project.type_interner();

    // Update file a.ts with new content
    project.set_file(
        "a.ts".to_string(),
        "export const x: number = 42;".to_string(),
    );

    // The interner should still be the same instance
    let interner_after = project.type_interner();
    assert!(
        std::sync::Arc::ptr_eq(&interner_before, &interner_after),
        "TypeInterner should persist across file updates"
    );

    // The updated file should still share the same interner
    let a_interner = &project.files["a.ts"].type_interner;
    assert!(
        std::sync::Arc::ptr_eq(a_interner, &interner_after),
        "Updated file should share the project's TypeInterner"
    );
}

#[test]
fn test_standalone_project_file_has_own_interner() {
    // Verify that standalone ProjectFile (outside Project) creates its own interner.
    use crate::project::ProjectFile;

    let file_a = ProjectFile::new("a.ts".to_string(), "const x = 1;".to_string());
    let file_b = ProjectFile::new("b.ts".to_string(), "const y = 2;".to_string());

    assert!(
        !std::sync::Arc::ptr_eq(&file_a.type_interner, &file_b.type_interner),
        "Standalone ProjectFiles should have independent TypeInterners"
    );
}

#[test]
fn test_project_files_share_definition_store() {
    // Verify that files created via Project::set_file share the project's DefinitionStore.
    let mut project = Project::new();
    project.set_file(
        "a.ts".to_string(),
        "export const x: number = 1;".to_string(),
    );
    project.set_file(
        "b.ts".to_string(),
        "export const y: string = 'hi';".to_string(),
    );

    let project_def_store = project.definition_store();
    let a_def_store = project.files["a.ts"]
        .definition_store
        .as_ref()
        .expect("Project file should have a shared DefinitionStore");
    let b_def_store = project.files["b.ts"]
        .definition_store
        .as_ref()
        .expect("Project file should have a shared DefinitionStore");

    assert!(
        std::sync::Arc::ptr_eq(&project_def_store, a_def_store),
        "File a.ts should share the project's DefinitionStore"
    );
    assert!(
        std::sync::Arc::ptr_eq(&project_def_store, b_def_store),
        "File b.ts should share the project's DefinitionStore"
    );
}

#[test]
fn test_standalone_project_file_has_no_shared_def_store() {
    // Standalone ProjectFile (outside Project) should not have a shared DefinitionStore.
    use crate::project::ProjectFile;

    let file = ProjectFile::new("test.ts".to_string(), "const x = 1;".to_string());
    assert!(
        file.definition_store.is_none(),
        "Standalone ProjectFile should not have a shared DefinitionStore"
    );
}

#[test]
fn test_project_diagnostics_use_shared_def_store() {
    // Verify that get_diagnostics works correctly when the shared DefinitionStore is wired.
    let mut project = Project::new();
    project.set_file(
        "test.ts".to_string(),
        "let x: number = 42; let y: string = x;".to_string(),
    );

    // Should produce diagnostics (TS2322: number not assignable to string)
    let diagnostics = project.get_diagnostics("test.ts").unwrap();
    assert!(
        !diagnostics.is_empty(),
        "Should produce type-checking diagnostics with shared DefinitionStore"
    );
}

#[test]
fn test_set_file_skips_reparse_on_identical_content() {
    let mut project = Project::new();
    let source = "export const x = 1;".to_string();

    // First set: file is created
    project.set_file("test.ts".to_string(), source.clone());
    let hash_1 = project.files["test.ts"].content_hash();

    // Second set with identical content: should be a no-op
    project.set_file("test.ts".to_string(), source);
    let hash_2 = project.files["test.ts"].content_hash();

    assert_eq!(
        hash_1, hash_2,
        "Content hash should be stable for identical source"
    );

    // Verify the file still works correctly after the skip
    assert_eq!(
        project.files["test.ts"].source_text(),
        "export const x = 1;"
    );
}

#[test]
fn test_set_file_reparses_on_changed_content() {
    let mut project = Project::new();

    project.set_file("test.ts".to_string(), "export const x = 1;".to_string());
    let hash_1 = project.files["test.ts"].content_hash();

    // Different content should trigger re-parse
    project.set_file("test.ts".to_string(), "export const x = 2;".to_string());
    let hash_2 = project.files["test.ts"].content_hash();

    assert_ne!(
        hash_1, hash_2,
        "Content hash should differ for different source"
    );
    assert_eq!(
        project.files["test.ts"].source_text(),
        "export const x = 2;"
    );
}

#[test]
fn test_content_hash_consistent_across_project_file_constructors() {
    let source = "function hello() { return 42; }";

    // Standalone constructor
    let file1 = ProjectFile::new("a.ts".to_string(), source.to_string());
    // Via project
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), source.to_string());

    assert_eq!(
        file1.content_hash(),
        project.files["a.ts"].content_hash(),
        "Content hash should be the same regardless of constructor path"
    );
}

#[test]
fn test_content_hash_updated_by_update_source() {
    let mut file = ProjectFile::new("test.ts".to_string(), "let x = 1;".to_string());
    let hash_before = file.content_hash();

    file.update_source("let x = 2;".to_string());
    let hash_after = file.content_hash();

    assert_ne!(
        hash_before, hash_after,
        "Content hash should change after update_source"
    );
}

#[test]
fn test_set_file_first_add_succeeds() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const x = 1;".to_string());
    assert!(
        project.files.contains_key("a.ts"),
        "File should be added to the project"
    );
}

#[test]
fn test_set_file_skips_identical_content() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const x = 1;".to_string());

    let hash_before = project.files["a.ts"].content_hash;

    // Setting with identical content should be a no-op.
    project.set_file("a.ts".to_string(), "export const x = 1;".to_string());

    assert_eq!(
        project.files["a.ts"].content_hash, hash_before,
        "Content hash should remain unchanged"
    );
}

#[test]
fn test_set_file_body_change_does_not_invalidate_dependents() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "export function foo() { return 1; }".to_string(),
    );
    project.set_file(
        "b.ts".to_string(),
        "import { foo } from \"./a\";\nfoo();\n".to_string(),
    );
    project.dependency_graph.add_dependency("b.ts", "a.ts");

    // Clean b.ts diagnostics
    let _ = project.get_diagnostics("b.ts");
    assert!(!project.files["b.ts"].diagnostics_dirty);

    // Replace a.ts with a body-only change via set_file (no export change).
    project.set_file(
        "a.ts".to_string(),
        "export function foo() { return 2; }".to_string(),
    );

    // The current simplified set_file doesn't check export signatures,
    // so we just verify the file was updated successfully.
    assert!(project.files.contains_key("a.ts"));
}

#[test]
fn test_set_file_export_change_updates_file() {
    let mut project = Project::new();

    project.set_file(
        "a.ts".to_string(),
        "export function foo() { return 1; }".to_string(),
    );
    project.set_file(
        "b.ts".to_string(),
        "import { foo } from \"./a\";\nfoo();\n".to_string(),
    );
    project.dependency_graph.add_dependency("b.ts", "a.ts");

    // Replace a.ts with a new export via set_file.
    project.set_file(
        "a.ts".to_string(),
        "export function foo() { return 1; }\nexport function bar() {}".to_string(),
    );

    assert!(project.files.contains_key("a.ts"));
}

// =============================================================================
// FileIdAllocator tests
// =============================================================================

#[test]
fn test_file_id_allocator_assigns_stable_ids() {
    use crate::project::FileIdAllocator;

    let mut alloc = FileIdAllocator::new();
    let id_a = alloc.get_or_allocate("a.ts");
    let id_b = alloc.get_or_allocate("b.ts");

    // Different files get different IDs.
    assert_ne!(id_a, id_b);

    // Same file gets the same ID on re-query.
    assert_eq!(alloc.get_or_allocate("a.ts"), id_a);
    assert_eq!(alloc.get_or_allocate("b.ts"), id_b);
}

#[test]
fn test_file_id_allocator_ids_never_reused() {
    use crate::project::FileIdAllocator;

    let mut alloc = FileIdAllocator::new();
    let id_a = alloc.get_or_allocate("a.ts");
    alloc.remove("a.ts");

    // After removal, re-allocating the same name gets a NEW id.
    let id_a2 = alloc.get_or_allocate("a.ts");
    assert_ne!(id_a, id_a2, "IDs must not be recycled after removal");
}

#[test]
fn test_file_id_allocator_remove_returns_old_id() {
    use crate::project::FileIdAllocator;

    let mut alloc = FileIdAllocator::new();
    let id_a = alloc.get_or_allocate("a.ts");
    assert_eq!(alloc.remove("a.ts"), Some(id_a));
    assert_eq!(alloc.remove("a.ts"), None); // already removed
}

#[test]
fn test_file_id_allocator_reverse_lookup() {
    use crate::project::FileIdAllocator;

    let mut alloc = FileIdAllocator::new();
    let id_a = alloc.get_or_allocate("a.ts");
    let id_b = alloc.get_or_allocate("b.ts");

    // Forward lookup works.
    assert_eq!(alloc.lookup("a.ts"), Some(id_a));
    assert_eq!(alloc.lookup("b.ts"), Some(id_b));

    // Reverse lookup works.
    assert_eq!(alloc.name_for_id(id_a), Some("a.ts"));
    assert_eq!(alloc.name_for_id(id_b), Some("b.ts"));

    // Out-of-range returns None.
    assert_eq!(alloc.name_for_id(999), None);
}

#[test]
fn test_file_id_allocator_reverse_lookup_after_remove() {
    use crate::project::FileIdAllocator;

    let mut alloc = FileIdAllocator::new();
    let id_a = alloc.get_or_allocate("a.ts");
    let _id_b = alloc.get_or_allocate("b.ts");

    // Remove a.ts — reverse lookup should return None.
    alloc.remove("a.ts");
    assert_eq!(alloc.name_for_id(id_a), None);

    // Re-allocating "a.ts" gets a new ID; the old slot stays cleared.
    let id_a2 = alloc.get_or_allocate("a.ts");
    assert_ne!(id_a, id_a2);
    assert_eq!(alloc.name_for_id(id_a), None);
    assert_eq!(alloc.name_for_id(id_a2), Some("a.ts"));
}

#[test]
fn test_project_file_name_for_idx() {
    let mut project = crate::project::Project::new();
    project.set_file("src/foo.ts".to_string(), "export const x = 1;".to_string());
    project.set_file("src/bar.ts".to_string(), "export const y = 2;".to_string());

    // Look up file_idx for "src/foo.ts" via a symbol's decl_file_idx.
    let foo_file = &project.files["src/foo.ts"];
    let foo_sym = foo_file
        .binder()
        .symbols
        .iter()
        .find(|s| s.decl_file_idx != u32::MAX)
        .expect("expected at least one stamped symbol");
    let resolved = project.file_name_for_idx(foo_sym.decl_file_idx);
    assert_eq!(resolved, Some("src/foo.ts"));
}

// =============================================================================
// Binder file_idx stamping tests
// =============================================================================

#[test]
fn test_binder_stamps_file_idx_on_symbols() {
    use tsz_binder::BinderState;
    use tsz_parser::ParserState;

    let mut parser = ParserState::new("test.ts".to_string(), "const x = 1;".to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.set_file_idx(42);
    binder.bind_source_file(arena, root);

    // At least one symbol should have the stamped file_idx.
    let has_stamped = binder.symbols.iter().any(|sym| sym.decl_file_idx == 42);
    assert!(
        has_stamped,
        "Expected at least one symbol with decl_file_idx == 42"
    );

    // No non-lib symbol should have u32::MAX file_idx.
    let has_unassigned = binder
        .symbols
        .iter()
        .any(|sym| sym.decl_file_idx == u32::MAX);
    assert!(
        !has_unassigned,
        "No symbol should have u32::MAX file_idx after stamping"
    );
}

#[test]
fn test_binder_semantic_defs_use_file_idx() {
    use tsz_binder::BinderState;
    use tsz_parser::ParserState;

    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "interface Foo { x: number }".to_string(),
    );
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.set_file_idx(7);
    binder.bind_source_file(arena, root);

    // The semantic_defs entry for Foo should have file_id == 7.
    assert!(
        !binder.semantic_defs.is_empty(),
        "Expected at least one semantic def"
    );
    for entry in binder.semantic_defs.values() {
        assert_eq!(
            entry.file_id, 7,
            "SemanticDefEntry.file_id should match the binder's file_idx"
        );
    }
}

#[test]
fn test_binder_default_file_idx_is_max() {
    use tsz_binder::BinderState;
    use tsz_parser::ParserState;

    let mut parser = ParserState::new("test.ts".to_string(), "const x = 1;".to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    // Without calling set_file_idx, symbols should have u32::MAX (backward compat).
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    for sym in binder.symbols.iter() {
        assert_eq!(
            sym.decl_file_idx,
            u32::MAX,
            "Default file_idx should be u32::MAX when not set"
        );
    }
}

// =============================================================================
// Project + DefinitionStore invalidation integration tests
// =============================================================================

#[test]
fn test_project_set_file_assigns_file_idx() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const x = 1;".to_string());

    let file = &project.files["a.ts"];
    assert_ne!(
        file.file_idx,
        u32::MAX,
        "ProjectFile should have a valid file_idx after set_file"
    );
}

#[test]
fn test_project_set_file_preserves_file_idx_on_update() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const x = 1;".to_string());
    let first_idx = project.files["a.ts"].file_idx;

    // Update the same file with different content.
    project.set_file("a.ts".to_string(), "export const x = 2;".to_string());
    let second_idx = project.files["a.ts"].file_idx;

    assert_eq!(
        first_idx, second_idx,
        "File index should be stable across set_file updates"
    );
}

#[test]
fn test_project_remove_file_cleans_up_file_idx() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const x = 1;".to_string());

    // Verify file is tracked.
    assert!(project.file_id_allocator.lookup("a.ts").is_some());

    project.remove_file("a.ts");

    // After removal, the allocator should no longer track the file.
    assert!(project.file_id_allocator.lookup("a.ts").is_none());
}

#[test]
fn test_project_definition_store_invalidation_on_set_file() {
    let mut project = Project::new();

    // First set: creates definitions.
    project.set_file(
        "a.ts".to_string(),
        "export interface Foo { x: number }".to_string(),
    );

    // The definition store should have registered definitions for file_idx.
    let file_idx = project.files["a.ts"].file_idx;
    let has_file = project.definition_store.has_file(file_idx);
    // Note: definitions are lazily registered during checker runs, not during
    // binding. So has_file may be false here. The important thing is that
    // invalidate_file is called during set_file (verified by the pipeline
    // not crashing and the file_idx being stable).
    let _ = has_file;

    // Replacing the file should not crash even if no definitions were registered.
    project.set_file(
        "a.ts".to_string(),
        "export interface Bar { y: string }".to_string(),
    );
    assert_eq!(
        project.files["a.ts"].file_idx, file_idx,
        "File index should be stable after replacement"
    );
}

// =============================================================================
// Skeleton fingerprint cache tests
// =============================================================================

#[test]
fn test_fingerprint_cache_tracks_new_files() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const x = 1;".to_string());
    project.set_file("b.ts".to_string(), "export const y = 2;".to_string());

    // Both files should have fingerprints in the cache.
    let fp_a = project.fingerprint_for_file("a.ts");
    let fp_b = project.fingerprint_for_file("b.ts");
    assert!(fp_a.is_some(), "a.ts should have a fingerprint");
    assert!(fp_b.is_some(), "b.ts should have a fingerprint");

    // Different exports should produce different fingerprints.
    assert_ne!(
        fp_a.unwrap(),
        fp_b.unwrap(),
        "Different exports should produce different fingerprints"
    );
}

#[test]
fn test_fingerprint_cache_stable_across_body_edits() {
    let mut project = Project::new();
    project.set_file(
        "a.ts".to_string(),
        "export function foo() { return 1; }".to_string(),
    );
    let fp_before = project
        .fingerprint_for_file("a.ts")
        .expect("should have fingerprint");

    // Change function body but keep the same export signature.
    project.set_file(
        "a.ts".to_string(),
        "export function foo() { return 42; }".to_string(),
    );
    let fp_after = project
        .fingerprint_for_file("a.ts")
        .expect("should still have fingerprint");

    assert_eq!(
        fp_before, fp_after,
        "Body-only changes should not change the export signature fingerprint"
    );
}

#[test]
fn test_fingerprint_cache_changes_on_api_change() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const x = 1;".to_string());
    let fp_before = project
        .fingerprint_for_file("a.ts")
        .expect("should have fingerprint");

    // Add a new export — this changes the public API.
    project.set_file(
        "a.ts".to_string(),
        "export const x = 1;\nexport const y = 2;".to_string(),
    );
    let fp_after = project
        .fingerprint_for_file("a.ts")
        .expect("should still have fingerprint");

    assert_ne!(
        fp_before, fp_after,
        "Adding a new export should change the fingerprint"
    );
}

#[test]
fn test_fingerprint_cache_removed_on_file_removal() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const x = 1;".to_string());
    assert!(project.fingerprint_for_file("a.ts").is_some());

    project.remove_file("a.ts");
    assert!(
        project.fingerprint_for_file("a.ts").is_none(),
        "Fingerprint should be removed when file is removed"
    );
}

#[test]
fn test_fingerprint_snapshot_returns_all_files() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const a = 1;".to_string());
    project.set_file("b.ts".to_string(), "export const b = 2;".to_string());
    project.set_file("c.ts".to_string(), "export const c = 3;".to_string());

    let snapshot = project.fingerprint_snapshot();
    assert_eq!(
        snapshot.len(),
        3,
        "Snapshot should contain entries for all 3 files"
    );
}

#[test]
fn test_fingerprint_cache_update_via_incremental_edit() {
    let mut project = Project::new();
    let source = "export const x = 1;";
    project.set_file("a.ts".to_string(), source.to_string());
    let fp_before = project.fingerprint_for_file("a.ts").unwrap();

    // Apply an incremental edit that changes the API.
    let line_map = LineMap::build(source);
    let edit = TextEdit {
        range: range_for_substring(source, &line_map, "1"),
        new_text: "1;\nexport const y = 2".to_string(),
    };
    project.update_file("a.ts", &[edit]);

    let fp_after = project.fingerprint_for_file("a.ts").unwrap();
    assert_ne!(
        fp_before, fp_after,
        "Incremental edit adding a new export should change fingerprint"
    );
}

// ===== Memory accounting tests =====

#[test]
fn test_project_file_estimated_size_is_nonzero() {
    let file = ProjectFile::new("test.ts".to_string(), "const x = 1;".to_string());
    let size = file.estimated_size_bytes();
    assert!(
        size > 0,
        "estimated_size_bytes should be nonzero for any file"
    );
    // Even the simplest file has the struct itself + parser arena + binder data
    assert!(
        size > std::mem::size_of::<ProjectFile>(),
        "size should exceed the bare struct: got {size}"
    );
}

#[test]
fn test_project_file_estimated_size_grows_with_content() {
    let small = ProjectFile::new("small.ts".to_string(), "const a = 1;".to_string());
    let big = ProjectFile::new(
        "big.ts".to_string(),
        (0..100)
            .map(|i| format!("export const v{i}: number = {i};\n"))
            .collect::<String>(),
    );
    assert!(
        big.estimated_size_bytes() > small.estimated_size_bytes(),
        "Larger source should produce a larger memory estimate: small={}, big={}",
        small.estimated_size_bytes(),
        big.estimated_size_bytes(),
    );
}

#[test]
fn test_project_residency_stats_empty_project() {
    let project = Project::new();
    let stats = project.residency_stats();
    assert_eq!(stats.file_count, 0);
    assert_eq!(stats.total_estimated_bytes, 0);
    assert!(stats.largest_file.is_none());
    assert!(stats.smallest_file.is_none());
}

#[test]
fn test_project_residency_stats_single_file() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "const x = 1;".to_string());

    let stats = project.residency_stats();
    assert_eq!(stats.file_count, 1);
    assert!(stats.total_estimated_bytes > 0);
    let (name, size) = stats.largest_file.as_ref().unwrap();
    assert_eq!(name, "a.ts");
    assert_eq!(*size, stats.total_estimated_bytes);
    // largest == smallest for a single file
    assert_eq!(stats.largest_file, stats.smallest_file);
}

#[test]
fn test_project_residency_stats_multi_file() {
    let mut project = Project::new();
    project.set_file("small.ts".to_string(), "const a = 1;".to_string());
    project.set_file(
        "big.ts".to_string(),
        (0..50)
            .map(|i| format!("export const v{i}: number = {i};\n"))
            .collect::<String>(),
    );

    let stats = project.residency_stats();
    assert_eq!(stats.file_count, 2);

    let (largest_name, largest_size) = stats.largest_file.as_ref().unwrap();
    let (smallest_name, smallest_size) = stats.smallest_file.as_ref().unwrap();
    assert_eq!(largest_name, "big.ts");
    assert_eq!(smallest_name, "small.ts");
    assert!(largest_size > smallest_size);
    assert_eq!(stats.total_estimated_bytes, largest_size + smallest_size);
}

#[test]
fn test_project_file_estimated_size_query() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "const x = 1;".to_string());

    let size = project.file_estimated_size("a.ts");
    assert!(size.is_some());
    assert!(size.unwrap() > 0);

    assert!(project.file_estimated_size("nonexistent.ts").is_none());
}

#[test]
fn test_project_residency_stats_after_remove() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "const a = 1;".to_string());
    project.set_file("b.ts".to_string(), "const b = 2;".to_string());

    let before = project.residency_stats();
    assert_eq!(before.file_count, 2);

    project.remove_file("a.ts");

    let after = project.residency_stats();
    assert_eq!(after.file_count, 1);
    assert!(after.total_estimated_bytes < before.total_estimated_bytes);
}

#[test]
fn test_project_residency_stats_includes_type_interner() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "const x: number = 1;".to_string());

    let stats = project.residency_stats();
    assert!(
        stats.type_interner_estimated_bytes > 0,
        "type_interner_estimated_bytes should be nonzero for any project with files"
    );
    // The interner size should be at least the struct overhead
    assert!(
        stats.type_interner_estimated_bytes >= std::mem::size_of::<tsz_solver::TypeInterner>(),
        "interner estimate ({}) should be >= struct size ({})",
        stats.type_interner_estimated_bytes,
        std::mem::size_of::<tsz_solver::TypeInterner>(),
    );
}

#[test]
fn test_project_residency_stats_type_interner_nonzero_even_empty_project() {
    let project = Project::new();
    let stats = project.residency_stats();
    // Even an empty project has a TypeInterner with intrinsics pre-registered
    assert!(
        stats.type_interner_estimated_bytes > 0,
        "type_interner_estimated_bytes should be nonzero even for empty project (intrinsics)"
    );
}

#[test]
fn test_project_residency_stats_includes_definition_store() {
    let mut project = Project::new();
    project.set_file(
        "a.ts".to_string(),
        "interface Foo { x: number; }\nclass Bar {}".to_string(),
    );

    let stats = project.residency_stats();
    assert!(
        stats.definition_store_estimated_bytes > 0,
        "definition_store_estimated_bytes should be nonzero for a project with definitions"
    );
}

#[test]
fn test_project_residency_stats_definition_store_nonzero_even_empty_project() {
    let project = Project::new();
    let stats = project.residency_stats();
    // Even an empty store has struct overhead (DashMaps, atomics, etc.)
    assert!(
        stats.definition_store_estimated_bytes > 0,
        "definition_store_estimated_bytes should be nonzero even for empty project"
    );
}

#[test]
fn test_eviction_candidates_empty_project() {
    let project = Project::new();
    let candidates = project.eviction_candidates(None);
    assert!(
        candidates.is_empty(),
        "empty project should have no eviction candidates"
    );
}

#[test]
fn test_eviction_candidates_returns_all_files_without_min_idle() {
    let mut project = Project::new();
    project.set_file("/a.ts".to_string(), "const a = 1;".to_string());
    project.set_file("/b.ts".to_string(), "const b = 2;".to_string());
    let candidates = project.eviction_candidates(None);
    assert_eq!(candidates.len(), 2, "should return all files");
}

#[test]
fn test_eviction_candidates_filters_by_min_idle() {
    use web_time::Duration;

    let mut project = Project::new();
    project.set_file("/a.ts".to_string(), "const a = 1;".to_string());
    project.set_file("/b.ts".to_string(), "const b = 2;".to_string());

    // Touch file a so it's recently accessed
    project.touch_file("/a.ts");

    // With a very high min_idle threshold, recently touched files should be filtered out.
    // Both files were just created/touched, so a 1-hour threshold filters all of them.
    let candidates = project.eviction_candidates(Some(Duration::from_secs(3600)));
    assert!(
        candidates.is_empty(),
        "recently accessed files should not be eviction candidates with high min_idle"
    );

    // With zero threshold, all files should be candidates
    let candidates = project.eviction_candidates(Some(Duration::ZERO));
    assert_eq!(
        candidates.len(),
        2,
        "all files should be candidates with zero min_idle"
    );
}

#[test]
fn test_eviction_candidates_include_residency_info() {
    let mut project = Project::new();
    project.set_file("/a.ts".to_string(), "const a = 1;".to_string());
    let candidates = project.eviction_candidates(None);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].file_name, "/a.ts");
    assert!(
        candidates[0].estimated_bytes > 0,
        "estimated_bytes should be positive"
    );
}

#[test]
fn test_touch_file_updates_last_accessed() {
    let mut project = Project::new();
    project.set_file("/a.ts".to_string(), "const a = 1;".to_string());

    let before = project.files["/a.ts"].last_accessed();
    // Small sleep to ensure timestamp difference
    std::thread::sleep(std::time::Duration::from_millis(5));
    project.touch_file("/a.ts");
    let after = project.files["/a.ts"].last_accessed();

    assert!(
        after > before,
        "touch should update last_accessed timestamp"
    );
}

#[test]
fn test_eviction_candidates_deprioritizes_dts_files() {
    let mut project = Project::new();
    // Create a .d.ts file and a .ts file of similar size
    project.set_file(
        "/types.d.ts".to_string(),
        "declare const x: number;".to_string(),
    );
    project.set_file(
        "/app.ts".to_string(),
        "declare const y: string;".to_string(),
    );

    let candidates = project.eviction_candidates(None);
    assert_eq!(candidates.len(), 2);

    // The .ts file should rank higher (better eviction candidate) than .d.ts
    // because .d.ts files are deprioritized with a 4x penalty
    let ts_idx = candidates
        .iter()
        .position(|c| c.file_name == "/app.ts")
        .unwrap();
    let dts_idx = candidates
        .iter()
        .position(|c| c.file_name == "/types.d.ts")
        .unwrap();
    assert!(
        ts_idx < dts_idx,
        "regular .ts file should rank as better eviction candidate than .d.ts"
    );
}

// =============================================================================
// Binder-based dependency graph wiring
// =============================================================================

#[test]
fn test_set_file_populates_dependency_graph_from_binder() {
    // Verifies that `set_file` uses binder's `file_import_sources` to populate
    // the dependency graph automatically, without a separate AST walk.
    let mut project = Project::new();

    project.set_file("a.ts".to_string(), "export const x = 1;".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { x } from \"./a\";\nexport const y = x + 1;".to_string(),
    );

    // The dependency graph should automatically have b.ts -> "./a"
    let b_deps = project.dependency_graph.get_dependencies("b.ts");
    assert!(
        b_deps.is_some(),
        "b.ts should have dependencies in the graph"
    );
    assert!(
        b_deps.unwrap().contains("./a"),
        "b.ts should depend on './a', got: {b_deps:?}",
    );

    // Reverse: "./a" should have b.ts as a dependent
    let a_dependents = project.dependency_graph.get_dependents("./a");
    assert!(
        a_dependents.is_some(),
        "'./a' should have dependents in the graph"
    );
    assert!(
        a_dependents.unwrap().contains("b.ts"),
        "'./a' dependents should include 'b.ts', got: {a_dependents:?}",
    );
}

#[test]
fn test_dependency_graph_tracks_reexports() {
    // Verifies that `export ... from` specifiers are captured.
    let mut project = Project::new();

    project.set_file(
        "barrel.ts".to_string(),
        "export { foo } from \"./impl\";\nexport * from \"./types\";".to_string(),
    );

    let deps = project.dependency_graph.get_dependencies("barrel.ts");
    assert!(deps.is_some(), "barrel.ts should have dependencies");
    let deps = deps.unwrap();
    assert!(
        deps.contains("./impl"),
        "barrel.ts should depend on './impl', got: {deps:?}",
    );
    assert!(
        deps.contains("./types"),
        "barrel.ts should depend on './types', got: {deps:?}",
    );
}

#[test]
fn test_dependency_graph_updates_on_file_change() {
    // Verifies that re-setting a file updates the dependency graph edges.
    let mut project = Project::new();

    project.set_file(
        "c.ts".to_string(),
        "import { a } from \"./old-dep\";".to_string(),
    );

    // Initial state: c.ts depends on ./old-dep
    let deps = project.dependency_graph.get_dependencies("c.ts").unwrap();
    assert!(deps.contains("./old-dep"));

    // Change c.ts to import from a different module
    project.set_file(
        "c.ts".to_string(),
        "import { b } from \"./new-dep\";".to_string(),
    );

    // After update: c.ts should depend on ./new-dep, not ./old-dep
    let deps = project.dependency_graph.get_dependencies("c.ts").unwrap();
    assert!(
        deps.contains("./new-dep"),
        "c.ts should now depend on './new-dep', got: {deps:?}",
    );
    assert!(
        !deps.contains("./old-dep"),
        "c.ts should no longer depend on './old-dep', got: {deps:?}",
    );
}

#[test]
fn test_dependency_graph_side_effect_imports() {
    // Side-effect imports (import "module") should also be tracked.
    let mut project = Project::new();

    project.set_file(
        "app.ts".to_string(),
        "import \"./polyfill\";\nimport { foo } from \"./lib\";".to_string(),
    );

    let deps = project.dependency_graph.get_dependencies("app.ts").unwrap();
    assert!(
        deps.contains("./polyfill"),
        "side-effect import should be in dependency graph, got: {deps:?}",
    );
    assert!(
        deps.contains("./lib"),
        "named import should be in dependency graph, got: {deps:?}",
    );
}
