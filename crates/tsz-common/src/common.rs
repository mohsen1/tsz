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

    /// Latest ECMAScript features
    #[default]
    ESNext = 99,
}

impl ScriptTarget {
    /// Check if this target supports ES2015+ features (classes, arrows, etc.)
    pub fn supports_es2015(self) -> bool {
        (self as u8) >= (ScriptTarget::ES2015 as u8)
    }

    /// Check if this target supports ES2017+ features (async, etc.)
    pub fn supports_es2017(self) -> bool {
        (self as u8) >= (ScriptTarget::ES2017 as u8)
    }

    /// Check if this target supports ES2020+ features (optional chaining, etc.)
    pub fn supports_es2020(self) -> bool {
        (self as u8) >= (ScriptTarget::ES2020 as u8)
    }

    /// Check if this is an ES5 or earlier target (requires downleveling)
    pub fn is_es5(self) -> bool {
        (self as u8) <= (ScriptTarget::ES5 as u8)
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

    /// CommonJS (Node.js style)
    CommonJS = 1,

    /// Asynchronous Module Definition (RequireJS style)
    AMD = 2,

    /// Universal Module Definition
    UMD = 3,

    /// SystemJS
    System = 4,

    /// ES2015 modules (import/export)
    ES2015 = 5,

    /// ES2020 modules with dynamic import()
    ES2020 = 6,

    /// ES2022 modules with top-level await
    ES2022 = 7,

    /// Latest module features
    ESNext = 99,

    /// Node.js ESM (package.json "type": "module")
    Node16 = 100,

    /// Node.js with automatic detection
    NodeNext = 199,
}

impl ModuleKind {
    /// Check if this is a CommonJS-like module system
    pub fn is_commonjs(self) -> bool {
        matches!(
            self,
            ModuleKind::CommonJS | ModuleKind::UMD | ModuleKind::Node16 | ModuleKind::NodeNext
        )
    }

    /// Check if this uses ES modules (import/export)
    pub fn is_es_module(self) -> bool {
        matches!(
            self,
            ModuleKind::ES2015
                | ModuleKind::ES2020
                | ModuleKind::ES2022
                | ModuleKind::ESNext
                | ModuleKind::Node16
                | ModuleKind::NodeNext
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
    pub fn as_bytes(&self) -> &'static [u8] {
        match self {
            NewLineKind::LineFeed => b"\n",
            NewLineKind::CarriageReturnLineFeed => b"\r\n",
        }
    }

    /// Get the newline as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            NewLineKind::LineFeed => "\n",
            NewLineKind::CarriageReturnLineFeed => "\r\n",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_target_comparisons() {
        assert!(ScriptTarget::ES3.is_es5());
        assert!(ScriptTarget::ES5.is_es5());
        assert!(!ScriptTarget::ES2015.is_es5());
        assert!(ScriptTarget::ES2015.supports_es2015());
        assert!(!ScriptTarget::ES5.supports_es2015());
    }

    #[test]
    fn test_module_kind_detection() {
        assert!(ModuleKind::CommonJS.is_commonjs());
        assert!(ModuleKind::UMD.is_commonjs());
        assert!(ModuleKind::ES2015.is_es_module());
        assert!(ModuleKind::ES2020.is_es_module());
        assert!(!ModuleKind::None.is_es_module());
    }

    #[test]
    fn test_newline_kind() {
        assert_eq!(NewLineKind::LineFeed.as_str(), "\n");
        assert_eq!(NewLineKind::CarriageReturnLineFeed.as_str(), "\r\n");
        assert_eq!(NewLineKind::LineFeed.as_bytes(), b"\n");
        assert_eq!(NewLineKind::CarriageReturnLineFeed.as_bytes(), b"\r\n");
    }
}
