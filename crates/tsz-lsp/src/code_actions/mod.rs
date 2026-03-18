//! Code Actions for the LSP.
//!
//! Provides quick fixes, refactorings and protocol-facing code-fix metadata.

mod code_action_accessors;
mod code_action_convert;
mod code_action_extract;
mod code_action_extract_function;
mod code_action_extract_type;
mod code_action_fix_all;
mod code_action_fixes;
mod code_action_implement;
mod code_action_imports;
mod code_action_inline;
mod code_action_namespace;
mod code_action_provider;
mod code_action_sort_imports;
mod code_action_surround;
mod code_action_switch;

pub use code_action_fixes::{
    CodeFixFileChange, CodeFixInfo, CodeFixPosition, CodeFixRegistry, CodeFixTextChange,
};
pub use code_action_provider::{
    CodeAction, CodeActionContext, CodeActionKind, CodeActionProvider, ImportCandidate,
    ImportCandidateKind,
};
