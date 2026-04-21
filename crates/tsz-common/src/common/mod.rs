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

fn normalize_ts_option(value: &str) -> String {
    let first = value.split(',').next().unwrap_or(value).trim();
    let mut normalized = String::with_capacity(first.len());
    for ch in first.chars() {
        if ch == '-' || ch == '_' || ch.is_whitespace() {
            continue;
        }
        normalized.push(ch.to_ascii_lowercase());
    }
    normalized
}

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
    /// Parse a TypeScript compiler option target value.
    ///
    /// This accepts tsc spelling variants and comma-separated directive values,
    /// taking the first entry to match multi-target conformance directives.
    #[must_use]
    pub fn from_ts_str(value: &str) -> Option<Self> {
        match normalize_ts_option(value).as_str() {
            "es3" => Some(Self::ES3),
            "es5" => Some(Self::ES5),
            "es6" | "es2015" => Some(Self::ES2015),
            "es2016" => Some(Self::ES2016),
            "es2017" => Some(Self::ES2017),
            "es2018" => Some(Self::ES2018),
            "es2019" => Some(Self::ES2019),
            "es2020" => Some(Self::ES2020),
            "es2021" => Some(Self::ES2021),
            "es2022" => Some(Self::ES2022),
            "es2023" => Some(Self::ES2023),
            "es2024" => Some(Self::ES2024),
            "es2025" => Some(Self::ES2025),
            "esnext" => Some(Self::ESNext),
            _ => None,
        }
    }

    /// Parse TypeScript's numeric target enum value.
    #[must_use]
    pub const fn from_ts_numeric(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::ES3),
            1 => Some(Self::ES5),
            2 => Some(Self::ES2015),
            3 => Some(Self::ES2016),
            4 => Some(Self::ES2017),
            5 => Some(Self::ES2018),
            6 => Some(Self::ES2019),
            7 => Some(Self::ES2020),
            8 => Some(Self::ES2021),
            9 => Some(Self::ES2022),
            10 => Some(Self::ES2023),
            11 => Some(Self::ES2024),
            12 => Some(Self::ES2025),
            99 => Some(Self::ESNext),
            _ => None,
        }
    }

    /// Return TypeScript's numeric target ordering value.
    #[must_use]
    pub const fn ts_numeric_value(self) -> u8 {
        self as u8
    }

    /// Return a canonical TypeScript option spelling.
    #[must_use]
    pub const fn as_ts_str(self) -> &'static str {
        match self {
            Self::ES3 => "es3",
            Self::ES5 => "es5",
            Self::ES2015 => "es2015",
            Self::ES2016 => "es2016",
            Self::ES2017 => "es2017",
            Self::ES2018 => "es2018",
            Self::ES2019 => "es2019",
            Self::ES2020 => "es2020",
            Self::ES2021 => "es2021",
            Self::ES2022 => "es2022",
            Self::ES2023 => "es2023",
            Self::ES2024 => "es2024",
            Self::ES2025 => "es2025",
            Self::ESNext => "esnext",
        }
    }

    /// Check if this target supports ES2016+ features (exponentiation operator).
    #[must_use]
    pub const fn supports_es2016(self) -> bool {
        (self as u8) >= (Self::ES2016 as u8)
    }

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

    /// Check if this target supports ES2019+ features (optional catch binding).
    #[must_use]
    pub const fn supports_es2019(self) -> bool {
        (self as u8) >= (Self::ES2019 as u8)
    }

    /// Check if this target supports ES2021+ features (logical assignment).
    #[must_use]
    pub const fn supports_es2021(self) -> bool {
        (self as u8) >= (Self::ES2021 as u8)
    }

    /// Check if this target supports ES2022+ features (class fields, regex 'd' flag, etc.)
    #[must_use]
    pub const fn supports_es2022(self) -> bool {
        (self as u8) >= (Self::ES2022 as u8)
    }

    /// Check if this target supports ES2023+ features.
    #[must_use]
    pub const fn supports_es2023(self) -> bool {
        (self as u8) >= (Self::ES2023 as u8)
    }

    /// Check if this target supports ES2024+ features.
    #[must_use]
    pub const fn supports_es2024(self) -> bool {
        (self as u8) >= (Self::ES2024 as u8)
    }

    /// Check if this target supports ES2025+ features.
    #[must_use]
    pub const fn supports_es2025(self) -> bool {
        (self as u8) >= (Self::ES2025 as u8)
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

    /// Node.js 18 module support
    Node18 = 101,

    /// Node.js 20 module support
    Node20 = 102,

    /// Node.js with automatic detection
    NodeNext = 199,

    /// Preserve original import/export syntax (let bundler handle it)
    Preserve = 200,
}

impl ModuleKind {
    /// Parse a TypeScript compiler option module value.
    ///
    /// This accepts tsc spelling variants and comma-separated directive values,
    /// taking the first entry to match multi-target conformance directives.
    #[must_use]
    pub fn from_ts_str(value: &str) -> Option<Self> {
        match normalize_ts_option(value).as_str() {
            "none" => Some(Self::None),
            "commonjs" => Some(Self::CommonJS),
            "amd" => Some(Self::AMD),
            "umd" => Some(Self::UMD),
            "system" => Some(Self::System),
            "es6" | "es2015" => Some(Self::ES2015),
            "es2020" => Some(Self::ES2020),
            "es2022" => Some(Self::ES2022),
            "esnext" => Some(Self::ESNext),
            "node16" => Some(Self::Node16),
            "node18" => Some(Self::Node18),
            "node20" => Some(Self::Node20),
            "nodenext" => Some(Self::NodeNext),
            "preserve" => Some(Self::Preserve),
            _ => None,
        }
    }

    /// Parse TypeScript's numeric module enum value.
    #[must_use]
    pub const fn from_ts_numeric(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::CommonJS),
            2 => Some(Self::AMD),
            3 => Some(Self::UMD),
            4 => Some(Self::System),
            5 => Some(Self::ES2015),
            6 => Some(Self::ES2020),
            7 => Some(Self::ES2022),
            99 => Some(Self::ESNext),
            100 => Some(Self::Node16),
            101 => Some(Self::Node18),
            102 => Some(Self::Node20),
            199 => Some(Self::NodeNext),
            200 => Some(Self::Preserve),
            _ => None,
        }
    }

    /// Return a canonical TypeScript option spelling.
    #[must_use]
    pub const fn as_ts_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::CommonJS => "commonjs",
            Self::AMD => "amd",
            Self::UMD => "umd",
            Self::System => "system",
            Self::ES2015 => "es2015",
            Self::ES2020 => "es2020",
            Self::ES2022 => "es2022",
            Self::ESNext => "esnext",
            Self::Node16 => "node16",
            Self::Node18 => "node18",
            Self::Node20 => "node20",
            Self::NodeNext => "nodenext",
            Self::Preserve => "preserve",
        }
    }

    /// Return TypeScript's numeric module enum value.
    #[must_use]
    pub const fn ts_numeric_value(self) -> u32 {
        self as u32
    }

    /// Check if this is a CommonJS-like module system
    #[must_use]
    pub const fn is_commonjs(self) -> bool {
        matches!(
            self,
            Self::CommonJS
                | Self::UMD
                | Self::Node16
                | Self::Node18
                | Self::Node20
                | Self::NodeNext
        )
    }

    /// Check if this is a Node.js-style module kind (Node16 through `NodeNext`).
    ///
    /// These require a matching `moduleResolution` of Node16 or `NodeNext`.
    #[must_use]
    pub const fn is_node_module(self) -> bool {
        matches!(
            self,
            Self::Node16 | Self::Node18 | Self::Node20 | Self::NodeNext
        )
    }

    /// Check if this is Node16 or Node18 specifically.
    ///
    /// In TypeScript 6.0+, TS1479 (CJS importing ESM) is only emitted for
    /// these pinned versions. `Node20` and `NodeNext` (which map to Node 22+)
    /// support `require()` of ESM modules, so the diagnostic is suppressed.
    #[must_use]
    pub const fn is_node16_or_node18(self) -> bool {
        matches!(self, Self::Node16 | Self::Node18)
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

    /// Check if this module kind supports dynamic `import()` expressions.
    ///
    /// Dynamic imports require ES2020+, CommonJS, AMD, System, UMD, Node16+,
    /// or `NodeNext`. ES2015 and None do not support them (TS1323).
    #[must_use]
    pub const fn supports_dynamic_import(self) -> bool {
        matches!(
            self,
            Self::ES2020
                | Self::ES2022
                | Self::ESNext
                | Self::CommonJS
                | Self::AMD
                | Self::System
                | Self::UMD
                | Self::Node16
                | Self::Node18
                | Self::Node20
                | Self::NodeNext
                | Self::Preserve
        )
    }

    /// Check if this module kind supports a second argument in dynamic `import()`.
    ///
    /// Only `esnext`, `node16`, `node18`, `node20`, `nodenext`, and `preserve`
    /// support the options argument in `import(specifier, options)` (TS1324).
    #[must_use]
    pub const fn supports_dynamic_import_options(self) -> bool {
        matches!(
            self,
            Self::ESNext
                | Self::Node16
                | Self::Node18
                | Self::Node20
                | Self::NodeNext
                | Self::Preserve
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

/// Visibility modifier for class/interface properties.
///
/// Used by the type system to determine nominal vs structural compatibility:
/// - `Public` properties use structural compatibility
/// - `Private` and `Protected` properties use nominal compatibility
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, Default)]
pub enum Visibility {
    /// Public property - structural compatibility applies
    #[default]
    Public,
    /// Private property - nominal compatibility only
    Private,
    /// Protected property - nominal compatibility only
    Protected,
}

#[cfg(test)]
#[path = "../../tests/common.rs"]
mod tests;
