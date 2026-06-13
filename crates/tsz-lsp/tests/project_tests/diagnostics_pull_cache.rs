//! Pull-model diagnostics cache tests: cache hits, `Unchanged` reports, and
//! invalidation on direct edits, cross-file edits, file add/remove, and
//! compiler-option changes.

use super::*;
use crate::diagnostics::{DocumentDiagnosticReport, DocumentDiagnosticReportKind};

fn full_report(
    report: DocumentDiagnosticReport,
) -> crate::diagnostics::FullDocumentDiagnosticReport {
    match report {
        DocumentDiagnosticReport::Full(full) => full,
        DocumentDiagnosticReport::Unchanged(unchanged) => {
            panic!(
                "expected Full report, got Unchanged with id {}",
                unchanged.result_id
            )
        }
    }
}

fn unchanged_report(
    report: DocumentDiagnosticReport,
) -> crate::diagnostics::UnchangedDocumentDiagnosticReport {
    match report {
        DocumentDiagnosticReport::Unchanged(unchanged) => unchanged,
        DocumentDiagnosticReport::Full(full) => panic!(
            "expected Unchanged report, got Full with id {:?} and {} items",
            full.result_id,
            full.items.len()
        ),
    }
}

#[test]
fn pull_with_matching_previous_result_id_returns_unchanged() {
    let mut project = Project::new();
    project.set_file(
        "main.ts".to_string(),
        "const broken: number = \"oops\";\n".to_string(),
    );

    let first = full_report(
        project
            .get_document_diagnostics_pull("main.ts", None)
            .expect("first pull"),
    );
    let result_id = first.result_id.expect("full report carries a result id");
    assert!(
        !first.items.is_empty(),
        "string-to-number assignment should produce a diagnostic"
    );

    let second = unchanged_report(
        project
            .get_document_diagnostics_pull("main.ts", Some(&result_id))
            .expect("second pull"),
    );
    assert_eq!(
        second.result_id, result_id,
        "Unchanged must echo the client's previousResultId"
    );
    assert_eq!(second.kind, DocumentDiagnosticReportKind::Unchanged);
}

#[test]
fn pull_without_previous_id_serves_cache_with_stable_result_id() {
    let mut project = Project::new();
    project.set_file(
        "main.ts".to_string(),
        "const broken: number = \"oops\";\n".to_string(),
    );

    let first = full_report(
        project
            .get_document_diagnostics_pull("main.ts", None)
            .expect("first pull"),
    );
    let second = full_report(
        project
            .get_document_diagnostics_pull("main.ts", None)
            .expect("second pull"),
    );

    // A recompute would have assigned a fresh monotonic result id; an equal
    // id pins that the second pull was served from the per-file cache.
    assert_eq!(
        first.result_id, second.result_id,
        "no-edit pull must serve the cached result, not recheck"
    );
    assert_eq!(first.items.len(), second.items.len());
}

#[test]
fn direct_edit_invalidates_pull_cache() {
    let mut project = Project::new();
    project.set_file(
        "main.ts".to_string(),
        "const fine: number = 1;\n".to_string(),
    );

    let first = full_report(
        project
            .get_document_diagnostics_pull("main.ts", None)
            .expect("first pull"),
    );
    let first_id = first.result_id.expect("result id");
    assert!(first.items.is_empty(), "initial source has no errors");

    let edit = {
        let file = project.file("main.ts").expect("file");
        let range = range_for_substring(file.source_text(), file.line_map(), "1;");
        TextEdit::new(range, "\"oops\";".to_string())
    };
    project.update_file("main.ts", &[edit]);

    let second = full_report(
        project
            .get_document_diagnostics_pull("main.ts", Some(&first_id))
            .expect("post-edit pull"),
    );
    let second_id = second.result_id.expect("result id");
    assert_ne!(
        first_id, second_id,
        "an edit must produce a fresh result id, never Unchanged"
    );
    assert!(
        !second.items.is_empty(),
        "the introduced type error must be reported after the edit"
    );
}

#[test]
fn cross_file_edit_invalidates_dependent_pull_cache() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const value = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { value } from \"./a\";\nconst use: number = value;\n".to_string(),
    );

    let first = full_report(
        project
            .get_document_diagnostics_pull("b.ts", None)
            .expect("first pull of b.ts"),
    );
    let first_id = first.result_id.expect("result id");

    // Body-level edit in a.ts that changes the inferred type of its export.
    // The export-signature fingerprint does not see this (names and flags are
    // unchanged), so only the coarse generation barrier protects b.ts.
    let edit = {
        let file = project.file("a.ts").expect("file");
        let range = range_for_substring(file.source_text(), file.line_map(), "= 1;");
        TextEdit::new(range, "= \"s\";".to_string())
    };
    project.update_file("a.ts", &[edit]);

    let second = project
        .get_document_diagnostics_pull("b.ts", Some(&first_id))
        .expect("post-edit pull of b.ts");
    let second = full_report(second);
    assert_ne!(
        Some(first_id),
        second.result_id,
        "a change in a.ts must invalidate b.ts's cached diagnostics — \
         serving Unchanged here would be a stale diagnostic"
    );
}

#[test]
fn new_file_invalidates_existing_pull_caches() {
    let mut project = Project::new();
    project.set_file(
        "b.ts".to_string(),
        "import { value } from \"./a\";\nvalue;\n".to_string(),
    );

    let first = full_report(
        project
            .get_document_diagnostics_pull("b.ts", None)
            .expect("first pull"),
    );
    let first_id = first.result_id.expect("result id");

    // Adding a.ts can resolve b.ts's previously-missing import.
    project.set_file("a.ts".to_string(), "export const value = 1;\n".to_string());

    let second = full_report(
        project
            .get_document_diagnostics_pull("b.ts", Some(&first_id))
            .expect("post-add pull"),
    );
    assert_ne!(
        Some(first_id),
        second.result_id,
        "adding a file must invalidate other files' cached diagnostics"
    );
}

#[test]
fn file_removal_invalidates_pull_caches() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const value = 1;\n".to_string());
    project.set_file(
        "b.ts".to_string(),
        "import { value } from \"./a\";\nvalue;\n".to_string(),
    );

    let first = full_report(
        project
            .get_document_diagnostics_pull("b.ts", None)
            .expect("first pull"),
    );
    let first_id = first.result_id.expect("result id");

    project.remove_file("a.ts");

    let second = full_report(
        project
            .get_document_diagnostics_pull("b.ts", Some(&first_id))
            .expect("post-remove pull"),
    );
    assert_ne!(
        Some(first_id),
        second.result_id,
        "removing a file must invalidate other files' cached diagnostics"
    );
}

#[test]
fn set_file_with_identical_content_preserves_pull_cache() {
    let mut project = Project::new();
    let source = "const fine: number = 1;\n".to_string();
    project.set_file("main.ts".to_string(), source.clone());

    let first = full_report(
        project
            .get_document_diagnostics_pull("main.ts", None)
            .expect("first pull"),
    );
    let first_id = first.result_id.expect("result id");

    // didOpen replay with identical content takes the content-hash fast path
    // and must not drop the cache.
    project.set_file("main.ts".to_string(), source);

    let second = unchanged_report(
        project
            .get_document_diagnostics_pull("main.ts", Some(&first_id))
            .expect("second pull"),
    );
    assert_eq!(second.result_id, first_id);
}

#[test]
fn config_change_invalidates_pull_cache() {
    let mut project = Project::new();
    project.set_file(
        "main.ts".to_string(),
        "function id(x) {\n  return x;\n}\n".to_string(),
    );

    let first = full_report(
        project
            .get_document_diagnostics_pull("main.ts", None)
            .expect("first pull"),
    );
    let first_id = first.result_id.expect("result id");
    assert!(
        !first.items.iter().any(|d| d.code == Some(7006)),
        "non-strict mode should not report implicit any"
    );

    project.set_strict(true);

    let second = full_report(
        project
            .get_document_diagnostics_pull("main.ts", Some(&first_id))
            .expect("post-config pull"),
    );
    assert_ne!(
        Some(first_id),
        second.result_id,
        "a compiler-option change must invalidate cached diagnostics"
    );
    assert!(
        second.items.iter().any(|d| d.code == Some(7006)),
        "strict mode must surface the implicit-any diagnostic (TS7006)"
    );
}

#[test]
fn workspace_pull_mixes_unchanged_and_full() {
    let mut project = Project::new();
    project.set_file("a.ts".to_string(), "export const value = 1;\n".to_string());
    project.set_file("b.ts".to_string(), "const fine: number = 2;\n".to_string());

    let first = project.get_workspace_diagnostics_with_previous(&[]);
    assert_eq!(first.items.len(), 2);
    let previous: Vec<(String, String)> = first
        .items
        .iter()
        .map(|item| {
            assert_eq!(item.kind, DocumentDiagnosticReportKind::Full);
            (
                item.uri.clone(),
                item.result_id.clone().expect("full item carries result id"),
            )
        })
        .collect();

    // No edits: every file reports Unchanged against the previous ids.
    let second = project.get_workspace_diagnostics_with_previous(&previous);
    assert_eq!(second.items.len(), 2);
    for item in &second.items {
        assert_eq!(
            item.kind,
            DocumentDiagnosticReportKind::Unchanged,
            "{} should be Unchanged on a no-edit pull",
            item.uri
        );
        assert!(item.items.is_none());
        assert!(item.result_id.is_some());
    }

    // Edit a.ts: the coarse barrier recomputes both files — no Unchanged
    // report may survive a project mutation.
    let edit = {
        let file = project.file("a.ts").expect("file");
        let range = range_for_substring(file.source_text(), file.line_map(), "= 1;");
        TextEdit::new(range, "= \"s\";".to_string())
    };
    project.update_file("a.ts", &[edit]);

    let third = project.get_workspace_diagnostics_with_previous(&previous);
    for item in &third.items {
        assert_eq!(
            item.kind,
            DocumentDiagnosticReportKind::Full,
            "{} must be recomputed after the edit",
            item.uri
        );
    }
}

#[test]
fn push_model_stale_filter_is_preserved() {
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

    let _ = project.get_diagnostics("a.ts");
    let _ = project.get_diagnostics("b.ts");

    // Body-only edit: the push model's signature-gated dirty flag must keep
    // ignoring dependents (existing behavior), while the pull cache for b.ts
    // is still coarsely invalidated.
    let edit = {
        let file = project.file("a.ts").expect("file");
        let range = range_for_substring(file.source_text(), file.line_map(), "return 1");
        TextEdit::new(range, "return 2".to_string())
    };
    project.update_file("a.ts", &[edit]);

    let stale = project.get_stale_diagnostics();
    assert!(
        stale.contains_key("a.ts"),
        "edited file is stale for the push model"
    );
    assert!(
        !stale.contains_key("b.ts"),
        "body-only edit must not mark dependents stale for the push model"
    );
}
