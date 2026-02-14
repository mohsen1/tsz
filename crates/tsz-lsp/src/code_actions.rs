//! Code actions and code-fix protocol metadata for tsz-lsp.
//!
//! - `code_action_provider`: quick fixes and refactorings over the AST/symbols.
//! - `code_action_fixes`: tsserver-style code-fix descriptors and registry.

mod code_action_fixes;
mod code_action_provider;

pub use code_action_fixes::{
    CodeFixFileChange, CodeFixInfo, CodeFixPosition, CodeFixRegistry, CodeFixTextChange,
};
pub use code_action_provider::{
    CodeAction, CodeActionContext, CodeActionKind, CodeActionProvider, ImportCandidate,
    ImportCandidateKind,
};
