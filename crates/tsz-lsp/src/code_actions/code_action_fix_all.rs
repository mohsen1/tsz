//! Fix All of Same Kind code action.
//!
//! When a quick fix is available for a diagnostic, this generates an
//! action that applies the same fix to all diagnostics of the same
//! code in the file.

use crate::diagnostics::LspDiagnostic;

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};
use tsz_parser::NodeIndex;

/// A mapping from diagnostic code to the fix function.
pub struct FixAllEntry {
    /// The diagnostic code (e.g., 2304 for "Cannot find name").
    pub code: u32,
    /// Human-readable fix description.
    pub description: &'static str,
    /// The fix ID (matches tsserver fixId).
    pub fix_id: &'static str,
}

/// Known fix-all entries matching TypeScript's code fix registry.
pub const FIX_ALL_ENTRIES: &[FixAllEntry] = &[
    FixAllEntry {
        code: 6133,
        description: "Remove all unused declarations",
        fix_id: "unusedIdentifier_delete",
    },
    FixAllEntry {
        code: 6196,
        description: "Remove all unused declarations",
        fix_id: "unusedIdentifier_delete",
    },
    FixAllEntry {
        code: 6138,
        description: "Prefix all unused parameters with '_'",
        fix_id: "unusedIdentifier_prefix",
    },
    FixAllEntry {
        code: 2304,
        description: "Add all missing imports",
        fix_id: "fixMissingImport",
    },
    FixAllEntry {
        code: 2552,
        description: "Fix all spelling errors",
        fix_id: "fixSpelling",
    },
    FixAllEntry {
        code: 7005,
        description: "Infer all types from usage",
        fix_id: "inferFromUsage",
    },
    FixAllEntry {
        code: 80005,
        description: "Convert all require() to import",
        fix_id: "requireInTs",
    },
];

impl<'a> CodeActionProvider<'a> {
    /// Generate "Fix All" code actions for diagnostics that have a known
    /// fix-all entry.
    ///
    /// Returns one "Fix All of Same Kind" action per distinct diagnostic
    /// code that has a known fix-all mapping.
    pub fn fix_all_actions(
        &self,
        _root: NodeIndex,
        diagnostics: &[LspDiagnostic],
    ) -> Vec<CodeAction> {
        let mut seen_codes = std::collections::HashSet::new();
        let mut actions = Vec::new();

        for diag in diagnostics {
            let code = match diag.code {
                Some(c) => c,
                None => continue,
            };

            if !seen_codes.insert(code) {
                continue;
            }

            // Find matching fix-all entry
            if let Some(entry) = FIX_ALL_ENTRIES.iter().find(|e| e.code == code) {
                actions.push(CodeAction {
                    title: entry.description.to_string(),
                    kind: CodeActionKind::QuickFix,
                    edit: None, // Resolved lazily via codeAction/resolve
                    is_preferred: false,
                    data: Some(serde_json::json!({
                        "fixId": entry.fix_id,
                        "fixAllDescription": entry.description,
                        "diagnosticCode": code,
                        "fileName": self.file_name,
                        "actionType": "fixAll"
                    })),
                });
            }
        }

        actions
    }
}
