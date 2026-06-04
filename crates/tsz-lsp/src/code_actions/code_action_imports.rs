use crate::diagnostics::LspDiagnostic;

use crate::rename::TextEdit;

use crate::utils::find_node_at_offset;

use rustc_hash::FxHashMap;

use std::path::Path;

use tsz_common::comments::get_leading_comments_from_cache;

use tsz_common::position::{Position, Range};

use tsz_parser::NodeIndex;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use super::code_action_provider::{
    CodeAction, CodeActionKind, CodeActionProvider, ImportCandidate, ImportCandidateKind,
};

use crate::rename::WorkspaceEdit;

#[derive(Clone, Debug)]
struct NamedImportSpec {
    specifier: NodeIndex,
    import_name: String,
    local_name: String,
    is_type_only: bool,
}

#[derive(Clone, Debug)]
pub(super) enum ImportRemoval {
    Default { name: String },
    Namespace { name: String },
    Named { specifier: NodeIndex, name: String },
    All { module_specifier: String },
}

impl ImportRemoval {
    fn name(&self) -> &str {
        match self {
            Self::Default { name } | Self::Namespace { name } | Self::Named { name, .. } => name,
            Self::All { module_specifier } => module_specifier,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ImportUsage {
    Type,
    Value,
}

#[derive(Clone, Debug)]
pub(super) enum MergeNamedImport {
    Edits(Vec<TextEdit>),
    AlreadyImported,
    NoMatch,
}

#[derive(Clone, Debug)]
pub(super) enum MergeDefaultImport {
    Edits(Vec<TextEdit>),
    AlreadyImported,
    NoMatch,
}

fn compare_import_specifier_local_names(a: &str, b: &str, ignore_case: bool) -> std::cmp::Ordering {
    if !ignore_case {
        return a.cmp(b);
    }

    let a_folded = a.to_ascii_lowercase();
    let b_folded = b.to_ascii_lowercase();
    a_folded.cmp(&b_folded)
}

fn module_specifier_match_for_merge(existing: &str, candidate: &str) -> bool {
    if existing == candidate {
        return true;
    }
    if !existing.starts_with('.') || !candidate.starts_with('.') {
        return false;
    }

    let extension_candidates = [".js", ".jsx", ".mjs", ".cjs"];
    let existing_has_ext = Path::new(existing).extension().is_some();
    let candidate_has_ext = Path::new(candidate).extension().is_some();

    if !existing_has_ext {
        for ext in extension_candidates {
            if format!("{existing}{ext}") == candidate {
                return true;
            }
        }
    }

    if !candidate_has_ext {
        for ext in extension_candidates {
            if format!("{candidate}{ext}") == existing {
                return true;
            }
        }
    }

    false
}

include!("code_action_imports_parts/part1.rs");
include!("code_action_imports_parts/part2.rs");
