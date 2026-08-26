//! Source identity, spans, and line maps.

use std::path::{Component, Path, PathBuf};
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

/// Normalize path components without filesystem access or case folding.
fn normalize_path_lexically(path: &Path, preserve_prefix: bool, collapse_parents: bool) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match (component, normalized.components().next_back()) {
            (Component::CurDir, _) => continue,
            (Component::ParentDir, _) if collapse_parents => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            (Component::ParentDir, Some(Component::Normal(_))) => {
                normalized.pop();
            }
            (Component::ParentDir, Some(Component::RootDir)) => {}
            (Component::ParentDir, Some(Component::Prefix(_))) if !preserve_prefix => {}
            (Component::ParentDir, Some(Component::CurDir)) => {
                unreachable!("current directories are removed eagerly")
            }
            (Component::ParentDir, Some(Component::ParentDir) | None) if path.is_absolute() => {}
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

pub(crate) fn normalize_clamped_path_lexically(path: &Path) -> PathBuf {
    normalize_path_lexically(path, false, false)
}

pub(crate) fn normalize_import_path_lexically(path: &Path) -> PathBuf {
    normalize_path_lexically(path, true, false)
}

pub(crate) fn normalize_project_path_lexically(path: &Path) -> PathBuf {
    normalize_path_lexically(path, false, true)
}

pub(crate) fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

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
    /// Logical path used for user-facing products.
    pub path: PathBuf,
    /// Host path retained for module and filesystem resolution.
    pub host_path: PathBuf,
    pub text: Arc<str>,
    coordinates: SourceCoordinateIndex,
}

/// Translation index between scanner-owned UTF-8 byte offsets and
/// TypeScript's public absolute UTF-16 coordinates.
///
/// Syntax spans stay byte-based so slicing is exact. Located diagnostics cross
/// into UTF-16 only once, when they become a public product.
#[derive(Debug, Clone)]
pub(crate) struct SourceCoordinateIndex {
    line_starts: Vec<(u32, u32)>,
}

/// Syntax family selected from the logical source extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    TypeScript,
    TypeScriptJsx,
    JavaScript,
    JavaScriptJsx,
}

impl SourceKind {
    /// JavaScript inputs keep `<` and `>` in the expression grammar even
    /// when `allowJs` admits them to a TypeScript program.
    #[must_use]
    pub const fn supports_expression_type_arguments(self) -> bool {
        matches!(self, Self::TypeScript | Self::TypeScriptJsx)
    }
}

impl SourceText {
    #[must_use]
    pub fn kind(&self) -> SourceKind {
        match self
            .path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("js" | "mjs" | "cjs") => SourceKind::JavaScript,
            Some("jsx") => SourceKind::JavaScriptJsx,
            Some("tsx") => SourceKind::TypeScriptJsx,
            _ => SourceKind::TypeScript,
        }
    }

    #[must_use]
    pub(crate) fn is_declaration_source(&self) -> bool {
        is_declaration_source_path(&self.path)
    }

    #[must_use]
    pub fn new(id: FileId, path: PathBuf, text: Arc<str>) -> Self {
        Self::new_with_host_path(id, path.clone(), path, text)
    }

    #[must_use]
    pub fn new_with_host_path(
        id: FileId,
        path: PathBuf,
        host_path: PathBuf,
        text: Arc<str>,
    ) -> Self {
        let coordinates = SourceCoordinateIndex::new(&text);
        Self {
            id,
            path,
            host_path,
            text,
            coordinates,
        }
    }

    #[must_use]
    pub fn slice(&self, span: Span) -> &str {
        debug_assert_eq!(self.id, span.file);
        self.text
            .get(span.start as usize..span.end as usize)
            .unwrap_or("")
    }

    /// Return one-based line and column for an absolute UTF-16 offset,
    /// matching `tsc` diagnostic rendering.
    #[must_use]
    pub fn line_and_column(&self, offset: u32) -> (u32, u32) {
        self.coordinates.line_and_column(offset)
    }

    /// Convert one byte-based syntax span into public absolute UTF-16 units.
    #[must_use]
    pub(crate) fn utf16_span(&self, span: Span) -> (u32, u32) {
        debug_assert_eq!(self.id, span.file);
        self.coordinates.byte_span(&self.text, span.start, span.end)
    }

    #[must_use]
    pub fn display_path(&self, base: Option<&Path>) -> String {
        let path = base
            .and_then(|base| self.path.strip_prefix(base).ok())
            .unwrap_or(&self.path);
        display_path(path)
    }
}

pub(crate) fn is_declaration_source_path(path: &Path) -> bool {
    let path = display_path(path);
    let path = path.strip_suffix('/').unwrap_or(&path);
    let root = path.starts_with("//") && !path[2..].contains('/')
        || path
            .split_once("://")
            .is_some_and(|(_, path)| !path.contains('/'));
    let name = path.rsplit('/').next().filter(|_| !root).unwrap_or("");
    name.ends_with(".d.mts")
        || name.ends_with(".d.cts")
        || name.ends_with(".ts") && name.contains(".d.")
}

impl SourceCoordinateIndex {
    #[must_use]
    pub(crate) fn new(text: &str) -> Self {
        let mut line_starts = vec![(0, 0)];
        let mut utf16 = 0_u32;
        let mut characters = text.char_indices().peekable();
        while let Some((byte, character)) = characters.next() {
            utf16 = utf16.saturating_add(character.len_utf16() as u32);
            let is_line_end = match character {
                '\r' => !characters.peek().is_some_and(|(_, next)| *next == '\n'),
                '\n' | '\u{2028}' | '\u{2029}' => true,
                _ => false,
            };
            if is_line_end {
                line_starts.push(((byte + character.len_utf8()) as u32, utf16));
            }
        }
        Self { line_starts }
    }

    #[must_use]
    pub(crate) fn byte_span(&self, text: &str, start: u32, end: u32) -> (u32, u32) {
        let start = self.byte_offset(text, start);
        (start, self.byte_offset(text, end).saturating_sub(start))
    }

    #[must_use]
    pub(crate) fn line_and_column(&self, offset: u32) -> (u32, u32) {
        let index = self
            .line_starts
            .partition_point(|line_start| line_start.1 <= offset)
            .saturating_sub(1);
        let column = offset.saturating_sub(self.line_starts[index].1);
        ((index + 1) as u32, column + 1)
    }

    fn byte_offset(&self, text: &str, offset: u32) -> u32 {
        let offset = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(text.len());
        assert!(
            text.is_char_boundary(offset),
            "source span must end at a UTF-8 character boundary"
        );
        let line = self
            .line_starts
            .partition_point(|line_start| line_start.0 as usize <= offset);
        let line_start = self.line_starts[line.saturating_sub(1)];
        let prefix = &text[line_start.0 as usize..offset];
        line_start
            .1
            .saturating_add(prefix.encode_utf16().count() as u32)
    }
}

#[cfg(test)]
#[path = "../rewrite-tests/source_unit.rs"]
mod tests;
