//! Common types and utilities for the tsz TypeScript compiler.
//!
//! This crate provides foundational types used across all tsz crates:
//! - String interning (`Atom`, `Interner`, `ShardedInterner`)
//! - Common enums (`ModuleKind`, `NewLineKind`, `ScriptTarget`)
//! - Source spans (`Span`, `Spanned`, `SpanBuilder`, `ByteSpan`)
//! - Compiler limits and thresholds
//! - Position/Range types for source locations
//! - Source map generation
//! - Comment parsing utilities

// String interning for identifier deduplication
pub mod interner;
pub use interner::{Atom, Interner, ShardedInterner};

// Common types - Shared constants to break circular dependencies
pub mod common;
pub use common::{ModuleKind, NewLineKind, ScriptTarget};

// Span - Source location tracking (byte offsets)
pub mod span;
pub use span::{ByteSpan, Span, SpanBuilder, Spanned};

// Centralized limits and thresholds
pub mod limits;

// Position/Range types for line/column source locations
pub mod position;
pub use position::{LineMap, Location, Position, Range, SourceLocation};

// Source Map generation
pub mod source_map;

// Comment parsing utilities
pub mod comments;
