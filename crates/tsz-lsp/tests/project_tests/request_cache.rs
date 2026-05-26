use super::*;

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
