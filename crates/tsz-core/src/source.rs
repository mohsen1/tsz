//! Source identity, spans, and line maps.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Stable ordinal assigned after input paths are normalized and sorted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FileId(pub u32);

/// Per-file syntax identity. It is never compared across source revisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u32);

/// Declaration identity: deterministic file ordinal plus local bind ordinal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DeclId {
    pub file: FileId,
    pub local: u32,
}

/// Program-local symbol identity, assigned by deterministic declaration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SymbolId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    #[must_use]
    pub const fn new(file: FileId, start: usize, end: usize) -> Self {
        Self {
            file,
            start: start as u32,
            end: end as u32,
        }
    }

    #[must_use]
    pub const fn len(self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start >= self.end
    }

    #[must_use]
    pub const fn merge(self, other: Self) -> Self {
        Self {
            file: self.file,
            start: if self.start < other.start {
                self.start
            } else {
                other.start
            },
            end: if self.end > other.end {
                self.end
            } else {
                other.end
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceText {
    pub id: FileId,
    pub path: PathBuf,
    pub text: Arc<str>,
    line_starts: Vec<u32>,
}

impl SourceText {
    #[must_use]
    pub fn new(id: FileId, path: PathBuf, text: Arc<str>) -> Self {
        let mut line_starts = vec![0];
        for (offset, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push((offset + 1) as u32);
            }
        }
        Self {
            id,
            path,
            text,
            line_starts,
        }
    }

    #[must_use]
    pub fn slice(&self, span: Span) -> &str {
        debug_assert_eq!(self.id, span.file);
        let start = span.start as usize;
        let end = span.end as usize;
        self.text.get(start..end).unwrap_or("")
    }

    /// Return one-based line and column, matching `tsc` diagnostic rendering.
    #[must_use]
    pub fn line_and_column(&self, offset: u32) -> (u32, u32) {
        let line = self.line_starts.partition_point(|start| *start <= offset);
        let index = line.saturating_sub(1);
        let column = offset.saturating_sub(self.line_starts[index]);
        ((index + 1) as u32, column + 1)
    }

    #[must_use]
    pub fn display_path(&self, base: Option<&Path>) -> String {
        let path = base
            .and_then(|base| self.path.strip_prefix(base).ok())
            .unwrap_or(&self.path);
        path.to_string_lossy().replace('\\', "/")
    }
}
