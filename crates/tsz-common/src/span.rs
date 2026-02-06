//! Span - Source location tracking for AST nodes and diagnostics
//!
//! A Span represents a range of source code by byte offsets. It is used for:
//! - Tracking the location of AST nodes
//! - Pointing to error locations in diagnostics
//! - Source map generation
//!
//! Spans are small (8 bytes) and cheap to copy.

use serde::{Deserialize, Serialize};

/// A span of source code, represented as a byte range.
///
/// Spans use half-open intervals: `[start, end)`.
/// An empty span has `start == end`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    /// Start byte offset (inclusive)
    pub start: u32,
    /// End byte offset (exclusive)
    pub end: u32,
}

impl Span {
    /// Create a new span from start and end offsets.
    #[inline]
    pub const fn new(start: u32, end: u32) -> Self {
        Span { start, end }
    }

    /// Create an empty span at the given position.
    #[inline]
    pub const fn at(pos: u32) -> Self {
        Span {
            start: pos,
            end: pos,
        }
    }

    /// Create a span from start position and length.
    #[inline]
    pub const fn from_len(start: u32, len: u32) -> Self {
        Span {
            start,
            end: start + len,
        }
    }

    /// Create a dummy/invalid span (used for synthetic nodes).
    #[inline]
    pub const fn dummy() -> Self {
        Span {
            start: u32::MAX,
            end: u32::MAX,
        }
    }

    /// Check if this is a dummy/invalid span.
    #[inline]
    pub const fn is_dummy(&self) -> bool {
        self.start == u32::MAX && self.end == u32::MAX
    }

    /// Get the length of this span in bytes.
    #[inline]
    pub const fn len(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    /// Check if this span is empty.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Check if this span contains a byte offset.
    #[inline]
    pub const fn contains(&self, offset: u32) -> bool {
        offset >= self.start && offset < self.end
    }

    /// Check if this span contains another span.
    #[inline]
    pub const fn contains_span(&self, other: Span) -> bool {
        other.start >= self.start && other.end <= self.end
    }

    /// Check if this span overlaps with another span.
    #[inline]
    pub const fn overlaps(&self, other: Span) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// Merge two spans to create a span covering both.
    #[inline]
    pub const fn merge(&self, other: Span) -> Span {
        let start = if self.start < other.start {
            self.start
        } else {
            other.start
        };
        let end = if self.end > other.end {
            self.end
        } else {
            other.end
        };
        Span { start, end }
    }

    /// Get the intersection of two spans, if they overlap.
    #[inline]
    pub fn intersect(&self, other: Span) -> Option<Span> {
        let start = self.start.max(other.start);
        let end = self.end.min(other.end);
        if start < end {
            Some(Span { start, end })
        } else {
            None
        }
    }

    /// Shrink this span by removing bytes from the start.
    #[inline]
    pub const fn shrink_start(&self, amount: u32) -> Span {
        let new_start = self.start + amount;
        Span {
            start: if new_start > self.end {
                self.end
            } else {
                new_start
            },
            end: self.end,
        }
    }

    /// Shrink this span by removing bytes from the end.
    #[inline]
    pub const fn shrink_end(&self, amount: u32) -> Span {
        let new_end = self.end.saturating_sub(amount);
        Span {
            start: self.start,
            end: if new_end < self.start {
                self.start
            } else {
                new_end
            },
        }
    }

    /// Create a span for just the first byte.
    #[inline]
    pub const fn first_byte(&self) -> Span {
        Span {
            start: self.start,
            end: if self.end > self.start {
                self.start + 1
            } else {
                self.end
            },
        }
    }

    /// Create a span for just the last byte.
    #[inline]
    pub const fn last_byte(&self) -> Span {
        Span {
            start: if self.end > self.start {
                self.end - 1
            } else {
                self.start
            },
            end: self.end,
        }
    }

    /// Extract the slice of text covered by this span.
    #[inline]
    pub fn slice<'a>(&self, text: &'a str) -> &'a str {
        let start = self.start as usize;
        let end = self.end as usize;
        text.get(start..end).unwrap_or("")
    }

    /// Extract the slice of text covered by this span, with safety checks.
    #[inline]
    pub fn slice_safe<'a>(&self, text: &'a str) -> &'a str {
        let start = (self.start as usize).min(text.len());
        let end = (self.end as usize).min(text.len());
        if start <= end {
            text.get(start..end).unwrap_or("")
        } else {
            ""
        }
    }
}

impl From<(u32, u32)> for Span {
    fn from((start, end): (u32, u32)) -> Self {
        Span::new(start, end)
    }
}

impl From<Span> for (u32, u32) {
    fn from(span: Span) -> Self {
        (span.start, span.end)
    }
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

// =============================================================================
// Spanned Trait
// =============================================================================

/// A trait for types that have a source span.
pub trait Spanned {
    /// Get the source span of this element.
    fn span(&self) -> Span;

    /// Get the start byte offset.
    fn start(&self) -> u32 {
        self.span().start
    }

    /// Get the end byte offset.
    fn end(&self) -> u32 {
        self.span().end
    }

    /// Get the length in bytes.
    fn len(&self) -> u32 {
        self.span().len()
    }

    /// Check if the span is empty.
    fn is_empty(&self) -> bool {
        self.span().is_empty()
    }
}

impl Spanned for Span {
    fn span(&self) -> Span {
        *self
    }
}

// =============================================================================
// SpanBuilder - For constructing spans during parsing
// =============================================================================

/// Helper for building spans during parsing.
///
/// Usage:
/// ```ignore
/// let builder = SpanBuilder::start(parser.pos());
/// // ... parse some content ...
/// let span = builder.end(parser.pos());
/// ```
#[derive(Clone, Copy, Debug)]
pub struct SpanBuilder {
    start: u32,
}

impl SpanBuilder {
    /// Start building a span at the given position.
    #[inline]
    pub const fn start(pos: u32) -> Self {
        SpanBuilder { start: pos }
    }

    /// Finish building the span at the given position.
    #[inline]
    pub const fn end(&self, pos: u32) -> Span {
        Span::new(self.start, pos)
    }

    /// Get the start position.
    #[inline]
    pub const fn start_pos(&self) -> u32 {
        self.start
    }
}

// =============================================================================
// ByteSpan - For working with raw byte slices
// =============================================================================

/// A span that also carries a reference to the source text.
///
/// This is useful when you need both the span and the text it covers.
#[derive(Clone, Copy, Debug)]
pub struct ByteSpan<'a> {
    /// The source text
    pub text: &'a str,
    /// The span within the text
    pub span: Span,
}

impl<'a> ByteSpan<'a> {
    /// Create a new ByteSpan.
    pub fn new(text: &'a str, span: Span) -> Self {
        ByteSpan { text, span }
    }

    /// Get the slice of text covered by this span.
    pub fn as_str(&self) -> &'a str {
        self.span.slice(self.text)
    }

    /// Get the length in bytes.
    pub fn len(&self) -> u32 {
        self.span.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.span.is_empty()
    }
}

impl<'a> Spanned for ByteSpan<'a> {
    fn span(&self) -> Span {
        self.span
    }
}

impl<'a> std::fmt::Display for ByteSpan<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_basics() {
        let span = Span::new(10, 20);
        assert_eq!(span.start, 10);
        assert_eq!(span.end, 20);
        assert_eq!(span.len(), 10);
        assert!(!span.is_empty());
    }

    #[test]
    fn test_span_at() {
        let span = Span::at(42);
        assert_eq!(span.start, 42);
        assert_eq!(span.end, 42);
        assert_eq!(span.len(), 0);
        assert!(span.is_empty());
    }

    #[test]
    fn test_span_from_len() {
        let span = Span::from_len(5, 10);
        assert_eq!(span.start, 5);
        assert_eq!(span.end, 15);
        assert_eq!(span.len(), 10);
    }

    #[test]
    fn test_span_contains() {
        let span = Span::new(10, 20);
        assert!(span.contains(10));
        assert!(span.contains(15));
        assert!(span.contains(19));
        assert!(!span.contains(9));
        assert!(!span.contains(20));
    }

    #[test]
    fn test_span_contains_span() {
        let outer = Span::new(10, 30);
        let inner = Span::new(15, 25);
        let partial = Span::new(5, 20);

        assert!(outer.contains_span(inner));
        assert!(!outer.contains_span(partial));
        assert!(outer.contains_span(outer));
    }

    #[test]
    fn test_span_overlaps() {
        let a = Span::new(10, 20);
        let b = Span::new(15, 25);
        let c = Span::new(20, 30);
        let d = Span::new(0, 5);

        assert!(a.overlaps(b));
        assert!(b.overlaps(a));
        assert!(!a.overlaps(c)); // Adjacent, not overlapping
        assert!(!a.overlaps(d));
    }

    #[test]
    fn test_span_merge() {
        let a = Span::new(10, 20);
        let b = Span::new(15, 30);
        let merged = a.merge(b);

        assert_eq!(merged.start, 10);
        assert_eq!(merged.end, 30);
    }

    #[test]
    fn test_span_intersect() {
        let a = Span::new(10, 20);
        let b = Span::new(15, 25);
        let c = Span::new(25, 30);

        let intersect = a.intersect(b);
        assert!(intersect.is_some());
        let i = intersect.unwrap();
        assert_eq!(i.start, 15);
        assert_eq!(i.end, 20);

        assert!(a.intersect(c).is_none());
    }

    #[test]
    fn test_span_shrink() {
        let span = Span::new(10, 30);

        let shrunk_start = span.shrink_start(5);
        assert_eq!(shrunk_start.start, 15);
        assert_eq!(shrunk_start.end, 30);

        let shrunk_end = span.shrink_end(5);
        assert_eq!(shrunk_end.start, 10);
        assert_eq!(shrunk_end.end, 25);

        // Shrink past boundaries
        let over_shrunk = span.shrink_start(25);
        assert_eq!(over_shrunk.start, 30);
        assert_eq!(over_shrunk.end, 30);
    }

    #[test]
    fn test_span_slice() {
        let text = "hello world";
        let span = Span::new(6, 11);
        assert_eq!(span.slice(text), "world");
    }

    #[test]
    fn test_span_slice_safe() {
        let text = "hello";
        let span = Span::new(0, 100);
        assert_eq!(span.slice_safe(text), "hello");

        let inverted = Span::new(100, 0);
        assert_eq!(inverted.slice_safe(text), "");
    }

    #[test]
    fn test_span_dummy() {
        let dummy = Span::dummy();
        assert!(dummy.is_dummy());

        let normal = Span::new(0, 10);
        assert!(!normal.is_dummy());
    }

    #[test]
    fn test_span_display() {
        let span = Span::new(10, 20);
        assert_eq!(format!("{}", span), "10..20");
    }

    #[test]
    fn test_span_builder() {
        let builder = SpanBuilder::start(5);
        assert_eq!(builder.start_pos(), 5);

        let span = builder.end(15);
        assert_eq!(span.start, 5);
        assert_eq!(span.end, 15);
    }

    #[test]
    fn test_byte_span() {
        let text = "hello world";
        let span = Span::new(0, 5);
        let byte_span = ByteSpan::new(text, span);

        assert_eq!(byte_span.as_str(), "hello");
        assert_eq!(byte_span.len(), 5);
        assert!(!byte_span.is_empty());
    }

    #[test]
    fn test_spanned_trait() {
        let span = Span::new(10, 20);
        assert_eq!(span.span(), span);
        assert_eq!(span.start(), 10);
        assert_eq!(span.end(), 20);
        assert_eq!(span.len(), 10);
    }
}
