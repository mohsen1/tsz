//! Code Actions for the LSP.
//!
//! Provides quick fixes, refactorings and protocol-facing code-fix metadata.

mod code_action_accessors;
mod code_action_async;
mod code_action_convert;
mod code_action_destructure_params;
mod code_action_editor_features;
mod code_action_export;
mod code_action_extract;
mod code_action_extract_function;
mod code_action_extract_interface;
mod code_action_extract_type;
mod code_action_fix_all;
mod code_action_fixes;
mod code_action_implement;
mod code_action_imports;
mod code_action_inline;
mod code_action_move;
mod code_action_namespace;
mod code_action_nullish_coalescing;
mod code_action_optional_chaining;
mod code_action_provider;
mod code_action_quick_fixes;
mod code_action_return_type;
mod code_action_sort_imports;
mod code_action_surround;
mod code_action_switch;
mod code_action_template_string;

pub use code_action_editor_features::{
    FileEvent, FileEventKind, FileReference, LspCommands, SourceActionKind,
};
pub use code_action_fixes::{
    CodeFixFileChange, CodeFixInfo, CodeFixPosition, CodeFixRegistry, CodeFixTextChange,
};
pub use code_action_provider::{
    CodeAction, CodeActionContext, CodeActionKind, CodeActionProvider, ImportCandidate,
    ImportCandidateKind,
};
