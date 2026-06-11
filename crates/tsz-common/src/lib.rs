//! Common types and utilities for the tsz TypeScript compiler.
//!
//! Provides foundational types used across all tsz crates:
//! - String interning (`Atom`, `Interner`, `ShardedInterner`)
//! - Common enums (`ModuleKind`, `NewLineKind`, `ScriptTarget`)
//! - Source spans (`Span`, `Spanned`, `SpanBuilder`, `ByteSpan`)
//! - Compiler limits and thresholds
//! - Position/Range types for source locations
//! - Source map generation
//! - Comment parsing utilities

// String interning for identifier deduplication
pub mod interner;
pub use interner::{AstAtom, Atom, Interner, ShardedInterner};

// Common types - Shared constants to break circular dependencies
pub mod common;
pub use common::{
    ImportResolutionMode, ModuleKind, NewLineKind, ScriptTarget, Visibility,
    parse_jsdoc_resolution_mode_attribute_clause,
};

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

// ECMAScript directive-prologue (e.g. `"use strict"`) recognition.
pub use common::directives;

// Diagnostic codes and message templates (shared by parser and checker)
pub mod diagnostics;

// Compiler options for type checking (shared by solver and checker)
pub mod options;
// Back-compat alias while we migrate to domain-folder layout.
pub use checker_options::CheckerOptions;
pub use options::checker as checker_options;
pub mod primitives;
pub use primitives::numeric;

// Process-wide performance counters used to drive the perf-architectural
// plan in `docs/plan/PERFORMANCE_PLAN.md`. Gated by the
// `TSZ_PERF_COUNTERS` env var; cheap (one relaxed atomic add) on hot
// paths even when the gate is off.
pub mod perf_counters;

// Centralized file-extension constants and helpers.
pub mod file_extensions;

// Shared module-resolution primitives (e.g. `typesVersions` selection).
pub mod module_resolution;

// Char-boundary-safe text windowing helpers (leading-prefix scan caps).
pub mod text_scan;

// Centralized TypeScript lib symbol capability metadata.
pub mod lib_capabilities;

// Canonical parser for TypeScript test-file directives (`// @option: value`)
// shared by the conformance, emit, fourslash, and checker test harness paths.
pub use common::test_directives;
