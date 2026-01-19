//! SourceFile - Owns source text and provides references for parsing/scanning
//!
//! The SourceFile struct owns the source text and provides safe access to it
//! for the scanner, parser, and other compilation phases. It maintains:
//!
//! - The source text content
//! - File metadata (name, path)
//! - Line map for offset <-> line/column conversion
//! - Character access utilities
//!
//! # Example
//!
//! ```ignore
//! let source = SourceFile::new("test.ts", "const x = 42;");
//! assert_eq!(source.text(), "const x = 42;");
//! assert_eq!(source.file_name(), "test.ts");
//! ```

use crate::lsp::position::{LineMap, Position, Range};
use crate::span::Span;
use std::sync::Arc;

// =============================================================================
// SourceFile
// =============================================================================

/// A source file that owns its text content and provides safe access.
///
/// SourceFile is designed to be the single owner of source text during
/// compilation. It provides:
/// - Immutable text access via `text()` and `as_str()`
/// - Line/column conversion via the embedded LineMap
/// - Safe character access with bounds checking
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// The file name (not necessarily a path)
    file_name: String,
    /// The source text content
    text: Arc<str>,
    /// Line map for efficient position conversion (lazy initialized)
    line_map: Option<LineMap>,
    /// Length of the text in bytes
    len: u32,
}

impl SourceFile {
    /// Create a new SourceFile from a file name and source text.
    pub fn new(file_name: impl Into<String>, text: impl Into<String>) -> Self {
        let text: String = text.into();
        let len = text.len() as u32;
        let text: Arc<str> = Arc::from(text.into_boxed_str());
        SourceFile {
            file_name: file_name.into(),
            text,
            line_map: None,
            len,
        }
    }

    /// Create a SourceFile with pre-built line map.
    pub fn with_line_map(file_name: impl Into<String>, text: impl Into<String>) -> Self {
        let text: String = text.into();
        let len = text.len() as u32;
        let line_map = Some(LineMap::build(&text));
        let text: Arc<str> = Arc::from(text.into_boxed_str());
        SourceFile {
            file_name: file_name.into(),
            text,
            line_map,
            len,
        }
    }

    /// Get the file name.
    #[inline]
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// Get the source text as a string slice.
    #[inline]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Get the source text as a string slice (alias for `text()`).
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Get the length of the source text in bytes.
    #[inline]
    pub fn len(&self) -> u32 {
        self.len
    }

    /// Check if the source text is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get a character at the given byte offset.
    ///
    /// Returns None if the offset is out of bounds or not a valid char boundary.
    pub fn char_at(&self, offset: u32) -> Option<char> {
        let offset = offset as usize;
        if offset >= self.text.len() {
            return None;
        }
        if !self.text.is_char_boundary(offset) {
            return None;
        }
        self.text[offset..].chars().next()
    }

    /// Get a byte at the given offset.
    ///
    /// Returns None if the offset is out of bounds.
    #[inline]
    pub fn byte_at(&self, offset: u32) -> Option<u8> {
        self.text.as_bytes().get(offset as usize).copied()
    }

    /// Get a slice of the source text.
    ///
    /// Returns an empty string if the span is out of bounds.
    #[inline]
    pub fn slice(&self, span: Span) -> &str {
        span.slice_safe(&self.text)
    }

    /// Get a slice from start to end offsets.
    ///
    /// Returns an empty string if the range is invalid.
    pub fn slice_range(&self, start: u32, end: u32) -> &str {
        Span::new(start, end).slice_safe(&self.text)
    }

    /// Get a slice from an offset to the end.
    pub fn slice_from(&self, start: u32) -> &str {
        let start = (start as usize).min(self.text.len());
        &self.text[start..]
    }

    /// Get a slice from the beginning to an offset.
    pub fn slice_to(&self, end: u32) -> &str {
        let end = (end as usize).min(self.text.len());
        &self.text[..end]
    }

    // =========================================================================
    // Line/Column Conversion
    // =========================================================================

    /// Ensure the line map is built.
    fn ensure_line_map(&mut self) {
        if self.line_map.is_none() {
            self.line_map = Some(LineMap::build(&self.text));
        }
    }

    /// Get a reference to the line map, building it if necessary.
    pub fn line_map(&mut self) -> &LineMap {
        self.ensure_line_map();
        self.line_map.as_ref().unwrap()
    }

    /// Convert a byte offset to a Position (line, character).
    ///
    /// Character is counted in UTF-16 code units for LSP compatibility.
    pub fn offset_to_position(&mut self, offset: u32) -> Position {
        self.ensure_line_map();
        self.line_map
            .as_ref()
            .unwrap()
            .offset_to_position(offset, &self.text)
    }

    /// Convert a Position (line, character) to a byte offset.
    pub fn position_to_offset(&mut self, position: Position) -> Option<u32> {
        self.ensure_line_map();
        self.line_map
            .as_ref()
            .unwrap()
            .position_to_offset(position, &self.text)
    }

    /// Convert a Span to a Range.
    pub fn span_to_range(&mut self, span: Span) -> Range {
        let start = self.offset_to_position(span.start);
        let end = self.offset_to_position(span.end);
        Range::new(start, end)
    }

    /// Convert a Range to a Span.
    pub fn range_to_span(&mut self, range: Range) -> Option<Span> {
        let start = self.position_to_offset(range.start)?;
        let end = self.position_to_offset(range.end)?;
        Some(Span::new(start, end))
    }

    /// Get the line count.
    pub fn line_count(&mut self) -> usize {
        self.ensure_line_map();
        self.line_map.as_ref().unwrap().line_count()
    }

    /// Get the text of a specific line (without newline).
    pub fn line_text(&mut self, line: u32) -> Option<&str> {
        self.ensure_line_map();
        let line_map = self.line_map.as_ref().unwrap();
        let start = line_map.line_start(line as usize)? as usize;
        let end = if (line as usize) + 1 < line_map.line_count() {
            line_map.line_start((line as usize) + 1)? as usize
        } else {
            self.text.len()
        };

        // Strip trailing newline
        let text = &self.text[start..end];
        let text = text.strip_suffix("\r\n").unwrap_or(text);
        let text = text.strip_suffix('\n').unwrap_or(text);
        let text = text.strip_suffix('\r').unwrap_or(text);
        Some(text)
    }

    // =========================================================================
    // Ownership Transfer
    // =========================================================================

    /// Take ownership of the source text, consuming the SourceFile.
    pub fn into_text(self) -> Arc<str> {
        self.text
    }

    /// Take the file name, consuming the SourceFile.
    pub fn into_parts(self) -> (String, Arc<str>) {
        (self.file_name, self.text)
    }
}

impl AsRef<str> for SourceFile {
    fn as_ref(&self) -> &str {
        &self.text
    }
}

impl std::ops::Deref for SourceFile {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.text
    }
}

// =============================================================================
// SourceFileRef - Borrowed view of a SourceFile
// =============================================================================

/// A borrowed reference to source file data.
///
/// This is useful when you need to pass source file information
/// without owning or cloning the data.
#[derive(Clone, Copy, Debug)]
pub struct SourceFileRef<'a> {
    /// The file name
    pub file_name: &'a str,
    /// The source text
    pub text: &'a str,
}

impl<'a> SourceFileRef<'a> {
    /// Create a new SourceFileRef.
    pub fn new(file_name: &'a str, text: &'a str) -> Self {
        SourceFileRef { file_name, text }
    }

    /// Create a SourceFileRef from a SourceFile.
    pub fn from_source_file(source_file: &'a SourceFile) -> Self {
        SourceFileRef {
            file_name: &source_file.file_name,
            text: source_file.text(),
        }
    }

    /// Get the length in bytes.
    pub fn len(&self) -> u32 {
        self.text.len() as u32
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Get a slice of the text.
    pub fn slice(&self, span: Span) -> &'a str {
        span.slice_safe(self.text)
    }
}

impl<'a> From<&'a SourceFile> for SourceFileRef<'a> {
    fn from(source_file: &'a SourceFile) -> Self {
        SourceFileRef::from_source_file(source_file)
    }
}

// =============================================================================
// SourceId - Interned source file identifier
// =============================================================================

/// An interned identifier for a source file.
///
/// This is used in multi-file compilation to efficiently reference
/// source files without cloning strings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SourceId(pub u32);

impl SourceId {
    /// The invalid/unknown source ID.
    pub const UNKNOWN: SourceId = SourceId(u32::MAX);

    /// Create a new SourceId.
    pub const fn new(id: u32) -> Self {
        SourceId(id)
    }

    /// Check if this is the unknown source ID.
    pub const fn is_unknown(&self) -> bool {
        self.0 == u32::MAX
    }
}

impl From<u32> for SourceId {
    fn from(id: u32) -> Self {
        SourceId(id)
    }
}

impl From<SourceId> for u32 {
    fn from(id: SourceId) -> Self {
        id.0
    }
}

// =============================================================================
// SourceLocation - Full location with file, span, and position
// =============================================================================

/// A complete source location with file information.
///
/// This combines a source file reference with a span, providing
/// all the information needed to report a diagnostic location.
#[derive(Clone, Debug)]
pub struct SourceLocation {
    /// File name
    pub file_name: String,
    /// Byte span
    pub span: Span,
    /// Start line (0-indexed)
    pub start_line: u32,
    /// Start column (0-indexed, UTF-16 code units)
    pub start_column: u32,
    /// End line (0-indexed)
    pub end_line: u32,
    /// End column (0-indexed, UTF-16 code units)
    pub end_column: u32,
}

impl SourceLocation {
    /// Create a new SourceLocation.
    pub fn new(
        file_name: String,
        span: Span,
        start_line: u32,
        start_column: u32,
        end_line: u32,
        end_column: u32,
    ) -> Self {
        SourceLocation {
            file_name,
            span,
            start_line,
            start_column,
            end_line,
            end_column,
        }
    }

    /// Create a SourceLocation from a SourceFile and Span.
    pub fn from_span(source_file: &mut SourceFile, span: Span) -> Self {
        let start_pos = source_file.offset_to_position(span.start);
        let end_pos = source_file.offset_to_position(span.end);
        SourceLocation {
            file_name: source_file.file_name().to_string(),
            span,
            start_line: start_pos.line,
            start_column: start_pos.character,
            end_line: end_pos.line,
            end_column: end_pos.character,
        }
    }

    /// Format as "file:line:column".
    pub fn to_string_short(&self) -> String {
        format!(
            "{}:{}:{}",
            self.file_name,
            self.start_line + 1,
            self.start_column + 1
        )
    }

    /// Format as "file(line,column)".
    pub fn to_string_visual_studio(&self) -> String {
        format!(
            "{}({},{})",
            self.file_name,
            self.start_line + 1,
            self.start_column + 1
        )
    }
}

impl std::fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string_short())
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_file_basic() {
        let source = SourceFile::new("test.ts", "const x = 42;");
        assert_eq!(source.file_name(), "test.ts");
        assert_eq!(source.text(), "const x = 42;");
        assert_eq!(source.len(), 13);
        assert!(!source.is_empty());
    }

    #[test]
    fn test_source_file_empty() {
        let source = SourceFile::new("empty.ts", "");
        assert!(source.is_empty());
        assert_eq!(source.len(), 0);
    }

    #[test]
    fn test_source_file_char_at() {
        let source = SourceFile::new("test.ts", "hello");
        assert_eq!(source.char_at(0), Some('h'));
        assert_eq!(source.char_at(4), Some('o'));
        assert_eq!(source.char_at(5), None);
    }

    #[test]
    fn test_source_file_byte_at() {
        let source = SourceFile::new("test.ts", "hello");
        assert_eq!(source.byte_at(0), Some(b'h'));
        assert_eq!(source.byte_at(4), Some(b'o'));
        assert_eq!(source.byte_at(5), None);
    }

    #[test]
    fn test_source_file_slice() {
        let source = SourceFile::new("test.ts", "hello world");
        let span = Span::new(0, 5);
        assert_eq!(source.slice(span), "hello");

        let span2 = Span::new(6, 11);
        assert_eq!(source.slice(span2), "world");
    }

    #[test]
    fn test_source_file_slice_safe() {
        let source = SourceFile::new("test.ts", "hello");
        let span = Span::new(0, 100); // Out of bounds
        assert_eq!(source.slice(span), "hello");
    }

    #[test]
    fn test_source_file_lines() {
        let mut source = SourceFile::new("test.ts", "line1\nline2\nline3");

        assert_eq!(source.line_count(), 3);
        assert_eq!(source.line_text(0), Some("line1"));
        assert_eq!(source.line_text(1), Some("line2"));
        assert_eq!(source.line_text(2), Some("line3"));
        assert_eq!(source.line_text(3), None);
    }

    #[test]
    fn test_source_file_position_conversion() {
        let mut source = SourceFile::new("test.ts", "const x = 1;\nlet y = 2;");

        let pos = source.offset_to_position(0);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 0);

        let pos = source.offset_to_position(13); // Start of second line
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);

        // Roundtrip
        let offset = source.position_to_offset(Position::new(1, 4)).unwrap();
        assert_eq!(offset, 17); // "y" in "let y"
    }

    #[test]
    fn test_source_file_span_to_range() {
        let mut source = SourceFile::new("test.ts", "const x = 1;");
        let span = Span::new(6, 7); // "x"
        let range = source.span_to_range(span);

        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 6);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 7);
    }

    #[test]
    fn test_source_file_with_line_map() {
        let source = SourceFile::with_line_map("test.ts", "a\nb\nc");
        assert!(source.line_map.is_some());
    }

    #[test]
    fn test_source_file_ref() {
        let source = SourceFile::new("test.ts", "hello world");
        let source_ref = SourceFileRef::from_source_file(&source);

        assert_eq!(source_ref.file_name, "test.ts");
        assert_eq!(source_ref.text, "hello world");
        assert_eq!(source_ref.len(), 11);
    }

    #[test]
    fn test_source_id() {
        let id = SourceId::new(42);
        assert_eq!(id.0, 42);
        assert!(!id.is_unknown());

        assert!(SourceId::UNKNOWN.is_unknown());
    }

    #[test]
    fn test_source_location() {
        let mut source = SourceFile::new("test.ts", "const x = 42;");
        let span = Span::new(6, 7); // "x"
        let location = SourceLocation::from_span(&mut source, span);

        assert_eq!(location.file_name, "test.ts");
        assert_eq!(location.start_line, 0);
        assert_eq!(location.start_column, 6);
        assert_eq!(location.to_string_short(), "test.ts:1:7");
        assert_eq!(location.to_string_visual_studio(), "test.ts(1,7)");
    }

    #[test]
    fn test_source_file_into_parts() {
        let source = SourceFile::new("test.ts", "content");
        let (name, text) = source.into_parts();
        assert_eq!(name, "test.ts");
        assert_eq!(text.as_ref(), "content");
    }

    #[test]
    fn test_source_file_deref() {
        let source = SourceFile::new("test.ts", "hello");
        // Can use &str methods directly via Deref
        assert!(source.starts_with("hel"));
        assert_eq!(&*source, "hello");
    }
}
