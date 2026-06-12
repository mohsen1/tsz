//! Pull-model diagnostics surface for `Project`.

use rustc_hash::FxHashMap;
use web_time::Instant;

use super::{Project, ProjectRequestKind};
use crate::diagnostics::{
    DocumentDiagnosticReport, DocumentDiagnosticReportKind, FullDocumentDiagnosticReport,
    UnchangedDocumentDiagnosticReport, WorkspaceDiagnosticReport, WorkspaceDiagnosticReportItem,
};
use crate::resolver::ScopeCacheStats;

impl Project {
    /// Pull-model diagnostics for a single document
    /// (LSP `textDocument/diagnostic`).
    ///
    /// When the cached diagnostics are still valid and the client's
    /// `previous_result_id` matches the id of that cache, an `Unchanged`
    /// report is returned and no checking runs. A valid cache with a stale
    /// (or absent) client id is served as a `Full` report without
    /// rechecking; an invalid cache is recomputed and assigned a fresh id.
    pub fn get_document_diagnostics_pull(
        &mut self,
        file_name: &str,
        previous_result_id: Option<&str>,
    ) -> Option<DocumentDiagnosticReport> {
        self.touch_file(file_name);
        let start = Instant::now();
        let result = self.document_diagnostics_pull_inner(file_name, previous_result_id);
        self.performance.record(
            ProjectRequestKind::Diagnostics,
            start.elapsed(),
            ScopeCacheStats::default(),
        );
        result
    }

    fn document_diagnostics_pull_inner(
        &mut self,
        file_name: &str,
        previous_result_id: Option<&str>,
    ) -> Option<DocumentDiagnosticReport> {
        let unchanged = previous_result_id.is_some()
            && self.diagnostics_cache_valid(file_name)
            && self
                .files
                .get(file_name)
                .is_some_and(|file| file.diagnostics_result_id.as_deref() == previous_result_id);
        if unchanged {
            return Some(DocumentDiagnosticReport::Unchanged(
                UnchangedDocumentDiagnosticReport {
                    kind: DocumentDiagnosticReportKind::Unchanged,
                    result_id: previous_result_id?.to_string(),
                },
            ));
        }

        let (items, result_id) = self.diagnostics_with_result_id(file_name)?;
        Some(DocumentDiagnosticReport::Full(
            FullDocumentDiagnosticReport {
                kind: DocumentDiagnosticReportKind::Full,
                result_id: Some(result_id),
                items,
            },
        ))
    }

    /// Get workspace diagnostics for all open files (pull model).
    ///
    /// Returns a `WorkspaceDiagnosticReport` containing diagnostics for every
    /// file in the project. This implements the LSP `workspace/diagnostic`
    /// request which allows clients to pull diagnostics on demand.
    pub fn get_workspace_diagnostics(&mut self) -> WorkspaceDiagnosticReport {
        self.get_workspace_diagnostics_with_previous(&[])
    }

    /// Workspace pull diagnostics honoring the client's `previousResultIds`.
    ///
    /// `previous_result_ids` holds `(file_name, result_id)` pairs from the
    /// client's last pull. Files whose cached diagnostics are still valid and
    /// whose current `resultId` matches the client's previous id are reported
    /// as `Unchanged` (no items, no rechecking); everything else is reported
    /// `Full` — from cache when valid, recomputed otherwise.
    pub fn get_workspace_diagnostics_with_previous(
        &mut self,
        previous_result_ids: &[(String, String)],
    ) -> WorkspaceDiagnosticReport {
        let previous: FxHashMap<&str, &str> = previous_result_ids
            .iter()
            .map(|(file, id)| (file.as_str(), id.as_str()))
            .collect();

        let mut file_names: Vec<String> = self.files.keys().cloned().collect();
        file_names.sort_unstable();
        let mut items = Vec::with_capacity(file_names.len());

        for file_name in file_names {
            let previous_id = previous.get(file_name.as_str()).copied();
            match self.get_document_diagnostics_pull(&file_name, previous_id) {
                Some(DocumentDiagnosticReport::Unchanged(report)) => {
                    items.push(WorkspaceDiagnosticReportItem {
                        uri: file_name,
                        version: None,
                        kind: DocumentDiagnosticReportKind::Unchanged,
                        result_id: Some(report.result_id),
                        items: None,
                    });
                }
                Some(DocumentDiagnosticReport::Full(report)) => {
                    items.push(WorkspaceDiagnosticReportItem {
                        uri: file_name,
                        version: None,
                        kind: DocumentDiagnosticReportKind::Full,
                        result_id: report.result_id,
                        items: Some(report.items),
                    });
                }
                None => {}
            }
        }

        WorkspaceDiagnosticReport { items }
    }
}
