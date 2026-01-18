```rust
// src/parser/ast/literals.rs

//! This module contains the AST node for Literal values.
//!
//! Refactored to use zero-copy semantics. Literals now hold `&str` references
//! pointing to the underlying source arena (or source string) rather than
//! allocating new owned `String`s.

use std::borrow::Cow;
use std::fmt;

/// A specific index type pointing to a string in the Arena.
/// (Assuming a simplified definition for context. In a real scenario, this
/// might come from a crate like `bumpalo` or a custom `StringInterner`).
pub type ArenaIndex = u32;

/// Represents a literal value in the source code.
///
/// This enum holds references (`&str`) to the original source text, ensuring
/// that parsing does not allocate new memory for every number or string literal.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal<'arena> {
    /// A string literal, e.g., `"hello"`.
    /// Holds a slice of the source text.
    String(&'arena str),

    /// A byte string literal, e.g., `b"world"`.
    /// Holds a wrapper around a slice of bytes.
    ByteString(ByteString<'arena>),

    /// A character literal, e.g., `'a'`.
    Char(&'arena str),

    /// A byte character literal, e.g., `b'a'`.
    ByteChar(&'arena str),

    /// An integer literal, e.g., `42`, `0x1A`.
    /// Stored as a string slice to preserve formatting (hex, octal, etc.) and size.
    Int(&'arena str),

    /// A float literal, e.g., `3.14`, `1.0e-10`.
    /// Stored as a string slice.
    Float(&'arena str),

    /// A boolean literal, `true` or `false`.
    Bool(bool),

    /// An Arena Index variant (Alternative to direct `&str`).
    /// This is used if the system uses a central interner rather than
    /// direct references to the source slice.
    ArenaIndex(ArenaIndex),
}

impl<'arena> Literal<'arena> {
    /// Returns the inner string slice if the literal is a string-like type.
    pub fn as_str(&self) -> Option<&'arena str> {
        match self {
            Literal::String(s) => Some(s),
            Literal::Char(s) => Some(s),
            Literal::ByteChar(s) => Some(s),
            Literal::Int(s) => Some(s),
            Literal::Float(s) => Some(s),
            _ => None,
        }
    }

    /// Checks if the literal is a string literal.
    pub fn is_string(&self) -> bool {
        matches!(self, Literal::String(_))
    }

    /// Checks if the literal is a numeric type (Int or Float).
    pub fn is_numeric(&self) -> bool {
        matches!(self, Literal::Int(_) | Literal::Float(_))
    }
}

impl<'arena> fmt::Display for Literal<'arena> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal::String(s) => write!(f, "\"{}\"", s),
            Literal::ByteString(bs) => write!(f, "b\"{}\"", bs.as_str()),
            Literal::Char(c) => write!(f, "'{}'", c),
            Literal::ByteChar(bc) => write!(f, "b'{}'", bc),
            Literal::Int(i) => write!(f, "{}", i),
            Literal::Float(fl) => write!(f, "{}", fl),
            Literal::Bool(b) => write!(f, "{}", b),
            Literal::ArenaIndex(idx) => write!(f, "<arena:{}>", idx),
        }
    }
}

// -----------------------------------------------------------------------------
// Helper Types
// -----------------------------------------------------------------------------

/// Representation of a byte string literal.
/// In a zero-copy context, this usually wraps a `&[u8]` or a `&str` that is known
/// to be valid ASCII.
#[derive(Debug, Clone, PartialEq)]
pub struct ByteString<'arena> {
    // In many parsers, ByteString is just a Vec<u8>.
    // To achieve zero-copy, we store a reference to the source slice.
    // We assume the source slice contains valid ASCII/UTF-8 for the byte string.
    inner: &'arena [u8],
}

impl<'arena> ByteString<'arena> {
    /// Creates a new ByteString reference.
    pub fn new(slice: &'arena [u8]) -> Self {
        Self { inner: slice }
    }

    /// Returns a string view if the byte string is valid UTF-8.
    /// Returns a placeholder string otherwise.
    pub fn as_str(&self) -> &str {
        // We try to convert to str for display/logging purposes
        std::str::from_utf8(self.inner).unwrap_or("<invalid utf8>")
    }
}

impl<'arena> AsRef<[u8]> for ByteString<'arena> {
    fn as_ref(&self) -> &[u8] {
        self.inner
    }
}
```
