//! Common types and constants for the compiler
//!
//! This module contains shared types used across multiple compiler phases
//! (parser, checker, emitter, transforms) to avoid circular dependencies.
//!
//! # Architecture
//!
//! By placing common types here, we establish a clear dependency hierarchy:
//!
//! ```text
//! common (base layer)
//!   ↓
//! lowering_pass → transform_context → transforms → emitter
//! ```
//!
//! No module should depend on a module that appears later in this chain.

/// ECMAScript target version.
///
/// This determines which language features are available during compilation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum ScriptTarget {
    /// ECMAScript 3 (1999)
    ES3 = 0,

    /// ECMAScript 5 (2009)
    ES5 = 1,

    /// ECMAScript 2015 (6th Edition)
    ES2015 = 2,

    /// ECMAScript 2016 (7th Edition)
    ES2016 = 3,

    /// ECMAScript 2017 (8th Edition)
    ES2017 = 4,

    /// ECMAScript 2018 (9th Edition)
    ES2018 = 5,

    /// ECMAScript 2019 (10th Edition)
    ES2019 = 6,

    /// ECMAScript 2020 (11th Edition)
    ES2020 = 7,

    /// ECMAScript 2021 (12th Edition)
    ES2021 = 8,

    /// ECMAScript 2022 (13th Edition)
    ES2022 = 9,

    /// ECMAScript 2023 (14th Edition)
    ES2023 = 10,

    /// ECMAScript 2024 (15th Edition)
    ES2024 = 11,

    /// ECMAScript 2025 (16th Edition) — TS6 default (`LatestStandard`)
    ES2025 = 12,

    /// Latest ECMAScript features
    #[default]
    ESNext = 99,
}

impl ScriptTarget {
    /// Check if this target supports ES2015+ features (classes, arrows, etc.)
    #[must_use]
    pub const fn supports_es2015(self) -> bool {
        (self as u8) >= (Self::ES2015 as u8)
    }

    /// Check if this target supports ES2017+ features (async, etc.)
    #[must_use]
    pub const fn supports_es2017(self) -> bool {
        (self as u8) >= (Self::ES2017 as u8)
    }

    /// Check if this target supports ES2020+ features (optional chaining, etc.)
    #[must_use]
    pub const fn supports_es2020(self) -> bool {
        (self as u8) >= (Self::ES2020 as u8)
    }

    /// Check if this target supports ES2018+ features (async generators, dotAll regex, etc.)
    #[must_use]
    pub const fn supports_es2018(self) -> bool {
        (self as u8) >= (Self::ES2018 as u8)
    }

    /// Check if this target supports ES2022+ features (class fields, regex 'd' flag, etc.)
    #[must_use]
    pub const fn supports_es2022(self) -> bool {
        (self as u8) >= (Self::ES2022 as u8)
    }

    /// Check if this is an ES5 or earlier target (requires downleveling)
    #[must_use]
    pub const fn is_es5(self) -> bool {
        (self as u8) <= (Self::ES5 as u8)
    }
}

/// Module system kind.
///
/// Determines how modules are resolved and emitted in the output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum ModuleKind {
    /// No module system (script mode)
    #[default]
    None = 0,

    /// `CommonJS` (Node.js style)
    CommonJS = 1,

    /// Asynchronous Module Definition (`RequireJS` style)
    AMD = 2,

    /// Universal Module Definition
    UMD = 3,

    /// `SystemJS`
    System = 4,

    /// ES2015 modules (import/export)
    ES2015 = 5,

    /// ES2020 modules with dynamic `import()`
    ES2020 = 6,

    /// ES2022 modules with top-level await
    ES2022 = 7,

    /// Latest module features
    ESNext = 99,

    /// Node.js ESM (package.json "type": "module")
    Node16 = 100,

    /// Node.js with automatic detection
    NodeNext = 199,

    /// Preserve original import/export syntax (let bundler handle it)
    Preserve = 200,
}

impl ModuleKind {
    /// Check if this is a CommonJS-like module system
    #[must_use]
    pub const fn is_commonjs(self) -> bool {
        matches!(
            self,
            Self::CommonJS | Self::UMD | Self::Node16 | Self::NodeNext
        )
    }

    /// Check if this uses ES modules (import/export)
    ///
    /// Returns true only for pure ES module systems where `export =` is forbidden.
    /// Node16/NodeNext are hybrid systems that support both `CommonJS` and `ESM`,
    /// so they return false here (the checker must use file extension to decide).
    #[must_use]
    pub const fn is_es_module(self) -> bool {
        matches!(
            self,
            Self::ES2015 | Self::ES2020 | Self::ES2022 | Self::ESNext | Self::Preserve
        )
    }
}

/// New line kind for source file emission.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NewLineKind {
    /// Line Feed (\n) - Unix, Linux, macOS
    #[default]
    LineFeed = 0,

    /// Carriage Return + Line Feed (\r\n) - Windows
    CarriageReturnLineFeed = 1,
}

impl NewLineKind {
    /// Get the actual newline characters
    #[must_use]
    pub const fn as_bytes(&self) -> &'static [u8] {
        match self {
            Self::LineFeed => b"\n",
            Self::CarriageReturnLineFeed => b"\r\n",
        }
    }

    /// Get the newline as a string
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::LineFeed => "\n",
            Self::CarriageReturnLineFeed => "\r\n",
        }
    }
}

#[cfg(test)]
#[path = "../tests/common.rs"]
mod tests;
