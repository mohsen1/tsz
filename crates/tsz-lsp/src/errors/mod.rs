//! Typed error enums for LSP rename and formatting surfaces.
//!
//! Idiomatic Rust represents recoverable failures as typed values whose
//! [`Display`] impl owns the canonical message text, rather than threading
//! `Result<_, String>` and re-authoring the same human-readable literal at
//! every call site. This module follows the precedent set by
//! `tsz_core::module_resolver::ResolutionFailure`: hand-rolled variants and a
//! single [`Display`] impl as the one source of truth for each message; no new
//! workspace dependency (`thiserror` is not yet vendored).
//!
//! The wire-visible message strings are byte-identical to the literals that
//! previously lived inline. The stringly-typed boundary now lives only at the
//! protocol edge: handlers render with `err.to_string()` to produce the same
//! client-facing text.

use std::fmt;

/// Why a rename request failed.
///
/// Each variant maps to exactly one canonical message rendered by the
/// [`Display`] impl below. These are surfaced verbatim to the LSP client (and
/// to `PrepareRenameResult::localized_error_message` via
/// [`RenameError::NotRenamable`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameError {
    /// The element under the cursor is not renamable (not an identifier, a
    /// non-renamable built-in, a meta-property right-hand side, etc.).
    NotRenamable,
    /// The element lives in an external module (e.g. `node_modules`).
    ExternalModule,
    /// No symbol could be resolved for the rename target.
    SymbolNotFound,
    /// The owning file is not loaded in the project.
    FileNotFound,
    /// The rename target string is empty.
    EmptyName,
    /// The requested new name is not a valid identifier. Carries the rejected
    /// name so the message matches the historical inline `format!`.
    InvalidIdentifier(String),
    /// The requested new name is not a valid private identifier. Carries the
    /// rejected name (without normalization) to match the historical message.
    InvalidPrivateIdentifier(String),
}

impl fmt::Display for RenameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRenamable => f.write_str("You cannot rename this element."),
            Self::ExternalModule => {
                f.write_str("You cannot rename elements from external modules.")
            }
            Self::SymbolNotFound => f.write_str("Could not find symbol to rename"),
            Self::FileNotFound => f.write_str("Could not find file"),
            Self::EmptyName => f.write_str("Rename target cannot be empty."),
            Self::InvalidIdentifier(name) => {
                write!(f, "'{name}' is not a valid identifier name")
            }
            Self::InvalidPrivateIdentifier(name) => {
                write!(f, "'{name}' is not a valid private identifier name")
            }
        }
    }
}

impl std::error::Error for RenameError {}

/// Why a formatting request failed.
///
/// External formatters (`Prettier`, `ESLint`) drive real formatting; failures
/// are process-spawn, I/O, formatter-exit, or JSON-parse errors. Each variant
/// pairs a fixed prefix (`stage`/`tool`) with the underlying detail so the
/// rendered string is byte-identical to the historical inline `format!`/`&str`
/// literals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatError {
    /// The configured file path had no usable file name component
    /// (`"Invalid file path"`).
    InvalidFilePath,
    /// Spawning an external formatter process failed.
    /// Renders as `"Failed to spawn {tool}: {source}"`.
    SpawnFailed {
        /// Formatter executable name (e.g. `"prettier"`, `"eslint"`).
        tool: &'static str,
        /// Underlying spawn error text.
        source: String,
    },
    /// Opening the child process stdin handle failed. Renders as the exact
    /// fixed message (e.g. `"Failed to open stdin"`).
    OpenStdin {
        /// Pre-rendered fixed message owning the canonical text.
        message: &'static str,
    },
    /// An I/O step (write/read) against the formatter process failed.
    /// Renders as `"{stage}: {source}"`.
    Io {
        /// Fixed stage prefix (e.g. `"Failed to write to prettier stdin"`).
        stage: &'static str,
        /// Underlying I/O error text.
        source: String,
    },
    /// The formatter exited unsuccessfully. Renders as `"{tool} failed: {stderr}"`.
    FormatterFailed {
        /// Human-readable formatter label (e.g. `"Prettier"`, `"ESLint"`).
        tool: &'static str,
        /// Captured stderr text.
        stderr: String,
    },
    /// Parsing the formatter's JSON output failed.
    /// Renders as `"Failed to parse eslint JSON output: {source}"`.
    JsonParse {
        /// Underlying JSON parse error text.
        source: String,
    },
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFilePath => f.write_str("Invalid file path"),
            Self::SpawnFailed { tool, source } => {
                write!(f, "Failed to spawn {tool}: {source}")
            }
            Self::OpenStdin { message } => f.write_str(message),
            Self::Io { stage, source } => write!(f, "{stage}: {source}"),
            Self::FormatterFailed { tool, stderr } => {
                write!(f, "{tool} failed: {stderr}")
            }
            Self::JsonParse { source } => {
                write!(f, "Failed to parse eslint JSON output: {source}")
            }
        }
    }
}

impl std::error::Error for FormatError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_error_messages_are_canonical() {
        assert_eq!(
            RenameError::NotRenamable.to_string(),
            "You cannot rename this element."
        );
        assert_eq!(
            RenameError::ExternalModule.to_string(),
            "You cannot rename elements from external modules."
        );
        assert_eq!(
            RenameError::SymbolNotFound.to_string(),
            "Could not find symbol to rename"
        );
        assert_eq!(RenameError::FileNotFound.to_string(), "Could not find file");
        assert_eq!(
            RenameError::EmptyName.to_string(),
            "Rename target cannot be empty."
        );
        assert_eq!(
            RenameError::InvalidIdentifier("9bad".to_string()).to_string(),
            "'9bad' is not a valid identifier name"
        );
        assert_eq!(
            RenameError::InvalidPrivateIdentifier("9bad".to_string()).to_string(),
            "'9bad' is not a valid private identifier name"
        );
    }

    #[test]
    fn format_error_messages_are_canonical() {
        assert_eq!(
            FormatError::InvalidFilePath.to_string(),
            "Invalid file path"
        );
        assert_eq!(
            FormatError::SpawnFailed {
                tool: "prettier",
                source: "boom".to_string()
            }
            .to_string(),
            "Failed to spawn prettier: boom"
        );
        assert_eq!(
            FormatError::OpenStdin {
                message: "Failed to open stdin"
            }
            .to_string(),
            "Failed to open stdin"
        );
        assert_eq!(
            FormatError::Io {
                stage: "Failed to write to prettier stdin",
                source: "io".to_string()
            }
            .to_string(),
            "Failed to write to prettier stdin: io"
        );
        assert_eq!(
            FormatError::FormatterFailed {
                tool: "Prettier",
                stderr: "nope".to_string()
            }
            .to_string(),
            "Prettier failed: nope"
        );
        assert_eq!(
            FormatError::JsonParse {
                source: "eof".to_string()
            }
            .to_string(),
            "Failed to parse eslint JSON output: eof"
        );
    }
}
