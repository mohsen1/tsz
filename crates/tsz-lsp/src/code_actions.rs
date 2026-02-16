//! Code Actions for the LSP.
//!
//! Provides quick fixes, refactorings and protocol-facing code-fix metadata.

mod code_action_fixes;
mod code_action_provider;

pub use code_action_fixes::{
    CodeFixFileChange, CodeFixInfo, CodeFixPosition, CodeFixRegistry, CodeFixTextChange,
};
pub use code_action_provider::{
    CodeAction, CodeActionContext, CodeActionKind, CodeActionProvider, ImportCandidate,
    ImportCandidateKind,
};
