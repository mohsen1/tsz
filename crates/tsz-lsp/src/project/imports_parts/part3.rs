use std::path::Path;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::code_actions::{ImportCandidate, ImportCandidateKind};

use crate::diagnostics::LspDiagnostic;

use crate::utils::find_node_at_offset;

use tsz_common::position::{Location, Position, Range};

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::{NodeArena, NodeIndex, syntax_kind_ext};

use tsz_scanner::SyntaxKind;

use super::import_collect::{
    AutoImportCandidateContext, ImportCandidateCollectionMode, ImportCandidateKey,
    ImportCandidateSink,
};

use super::{ExportMatch, ImportKind, ImportTarget, Project, ProjectFile};

#[derive(Default)]
pub(super) struct BareSpecifierSourceCache {
    pub(super) quoted_literal_match: FxHashMap<String, bool>,
    pub(super) import_like_match: FxHashMap<String, bool>,
}

include!("part1.rs");
include!("part2.rs");
