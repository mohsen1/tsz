use super::text_edits::narrow_indentation_only_edit;

use super::{Server, TsServerRequest, TsServerResponse};

use tsz::emitter::{ModuleKind, Printer, PrinterOptions};

struct CompileOnSaveProject {
    config_path: String,
    config_dir: std::path::PathBuf,
    enabled: bool,
    file_names: Vec<String>,
    uses_out_file: bool,
    out_dir: Option<String>,
    module: ModuleKind,
}

impl CompileOnSaveProject {
    fn output_path_for(&self, file: &str) -> std::path::PathBuf {
        let input = std::path::Path::new(file);
        let mut relative = input
            .strip_prefix(&self.config_dir)
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|_| {
                input
                    .file_name()
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| std::path::PathBuf::from(file))
            });
        relative.set_extension("js");
        if let Some(out_dir) = self.out_dir.as_deref() {
            let out_dir = std::path::Path::new(out_dir);
            let out_dir = if out_dir.is_absolute() {
                out_dir.to_path_buf()
            } else {
                self.config_dir.join(out_dir)
            };
            out_dir.join(relative)
        } else {
            input.with_extension("js")
        }
    }
}

use tsz::lsp::code_actions::CodeActionProvider;

use tsz::lsp::editor_decorations::inlay_hints::{InlayHintKind, InlayHintsProvider};

use tsz::lsp::editor_ranges::folding::FoldingRangeProvider;

use tsz::lsp::editor_ranges::selection_range::SelectionRangeProvider;

use tsz::lsp::hierarchy::call_hierarchy::{
    CallHierarchyIncomingCall, CallHierarchyItem, CallHierarchyOutgoingCall, CallHierarchyProvider,
    ImportResolutionRequest,
};

use tsz::lsp::highlighting::semantic_tokens::SemanticTokensProvider;

use tsz::lsp::position::{LineMap, Position, Range};

use tsz::lsp::rename::file_rename::FileRenameProvider;

use tsz::lsp::rename::linked_editing::LinkedEditingProvider;

use tsz_solver::construction::TypeInterner;

include!("handlers_structure_parts/part1.rs");
include!("handlers_structure_parts/part2.rs");
include!("handlers_structure_parts/part3.rs");
